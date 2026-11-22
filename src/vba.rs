//! Reading and writing the VBA project of a macro-enabled workbook — the
//! `xl/vbaProject.bin` part of an `.xlsm`, `.xltm`, `.xlam` or `.xlsb` file
//! ([`Workbook::macros_code`](crate::Workbook::macros_code)), or the
//! `_VBA_PROJECT_CUR` storage of an `.xls` file — without Excel.
//!
//! The project is a compound file (\[MS-CFB\]) laid out as \[MS-OVBA\]
//! describes. This module
//!
//! * reads it ([`VbaProject::read`], [`VbaProject::read_storage`],
//!   [`read_project_bin`]), including projects saved by Office, where every
//!   module stream starts with compiled p-code and the source follows at
//!   `MODULEOFFSET`;
//! * writes it ([`build_project`], [`write_project_bin`]) **without p-code**:
//!   module streams contain only the compressed source, `_VBA_PROJECT` is
//!   reduced to its 7-byte version-independent header with version `0xFFFF`,
//!   and the `__SRP_*` caches are dropped, so Office recompiles from source on
//!   the next open regardless of its build or bitness;
//! * keeps everything it does not have to touch: project references, the
//!   project id and protection values, user forms (designer storages) and
//!   unrelated streams, byte for byte.
//!
//! The package around the project (the zip entries, the relationships, the
//! content types and the code names of the sheets) is not handled here.
//!
//! ```rust
//! use umya_spreadsheet::{
//!     reader,
//!     vba::VbaProject,
//! };
//!
//! let path = std::path::Path::new("./tests/test_files/aaa.xlsm");
//! let book = reader::xlsx::read(path).unwrap();
//! let project = VbaProject::read(book.macros_code().unwrap()).unwrap();
//! for module in project.modules() {
//!     println!("{} ({:?})", module.name(), module.kind());
//!     print!("{}", module.source());
//! }
//! ```
//!
//! # Module source text
//!
//! [`VbaModule::source`] is the complete text of a module stream, including
//! the leading `Attribute VB_...` lines that the VBA editor hides. When a
//! module is written
//!
//! * line terminators are normalised to CR LF,
//! * `Attribute VB_Name` is made to match the module name,
//! * a standard module without `Attribute VB_Name` gets one; a class module
//!   that lacks part of the standard header (`VB_Name`, `VB_GlobalNameSpace`,
//!   `VB_Creatable`, `VB_PredeclaredId`, `VB_Exposed`) gets the missing lines
//!   with their default value `False`,
//! * a new document module gets the document header (`VB_Base` of the workbook
//!   or worksheet class, `VB_PredeclaredId = True`, ...),
//! * the text is encoded in the project code page; a character the code page
//!   cannot represent is an error ([`VbaError::Encoding`]), never a `?`.
//!
//! # User forms
//!
//! User forms can be created, or their designer storage replaced, from a
//! complete designer storage generated elsewhere ([`UserForm`]). Existing
//! designer storages are handed out as they are ([`read_user_forms`],
//! [`VbaProject::user_forms`]) and can be written as the `.frm` / `.frx`
//! pair the VBA editor imports ([`export_user_form`]); they are never
//! decoded or patched here.
//!
//! # Protection
//!
//! A project that is locked for viewing or has a password can be read (its
//! source is stored all the same; [`VbaProject::is_protected`] tells), but
//! it is never written: [`build_project`] refuses it with
//! [`VbaError::Protected`]. Re-signing a signed project is not implemented.

use std::collections::BTreeMap;

mod cfbio;
mod codepage;
pub mod compression;
mod crypto;
mod dir;
mod error;
mod forms;
mod project;
mod project_bin;
pub mod source;
mod stomp;

pub use self::{
    error::VbaError,
    forms::{
        export_user_form,
        import_user_form,
        read_user_forms,
    },
    project_bin::{
        BuiltProject,
        ProjectModule,
        VbaProject,
        build_project,
        read_project_bin,
        verify_modules,
        write_project_bin,
    },
};

/// Encodes `text` in a Windows code page (as used for module source and
/// designer properties). A character the code page cannot represent is a
/// [`VbaError::Encoding`] error.
pub fn encode_text(codepage: u16, text: &str) -> Result<Vec<u8>, VbaError> {
    codepage::encode(codepage, text, "text")
}

/// Decodes text stored in a Windows code page (module source, the designer
/// properties of a user form). Bytes that are not valid in the code page
/// become U+FFFD.
pub fn decode_text(codepage: u16, bytes: &[u8]) -> Result<String, VbaError> {
    codepage::decode(codepage, bytes)
}

/// Checks that a project can be stored in `codepage`: a Windows code page
/// that is supported, and not UTF-8 (65001), which a VBA project cannot use.
pub fn check_codepage(codepage: u16) -> Result<(), VbaError> {
    codepage::writable_encoding(codepage).map(|_| ())
}

/// The four kinds of VBA modules.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ModuleKind {
    /// Standard (procedural) module, `.bas`.
    Standard,
    /// Class module, `.cls`.
    Class,
    /// Document module: `ThisWorkbook` or the module behind a sheet.
    Document,
    /// Code module of a user form (designer module), `.frm`.
    Form,
}

/// A VBA module and its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaModule {
    /// Module name (a VBA identifier, at most 31 characters).
    pub name:   String,
    /// What kind of module this is.
    pub kind:   ModuleKind,
    /// Full module text as stored in the module stream, decoded to UTF-8,
    /// including leading `Attribute VB_...` lines.
    pub source: String,
}

/// What [`read_project_bin`] found in a `vbaProject.bin`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaProjectInfo {
    /// Project name (`PROJECTNAME`).
    pub name:            String,
    /// Windows code page of all MBCS text in the project.
    pub codepage:        u16,
    /// The modules, in the order of the `dir` stream.
    pub modules:         Vec<VbaModule>,
    /// The project is protected: its `DPB` value carries a password, its
    /// `CMG` value says it is locked (by the user, the host or the editor),
    /// or one of the two cannot be decrypted (Office calls such a project
    /// "unviewable"). [`build_project`] refuses protected projects.
    pub is_protected:    bool,
    /// Modules whose stored source does not match their compiled p-code
    /// ("VBA stomping"). Best effort: only a mismatch between the p-code
    /// line table and the number of source lines is detected; empty when
    /// nothing is known or nothing is wrong. The source text is what is
    /// returned in any case.
    pub stomped_modules: Vec<String>,
}

/// A user form to create, or whose code and designer storage to replace.
///
/// The designer storage is written as given (it is not validated beyond its
/// paths); the code module gets the standard form header. A new form gets a
/// fresh `VB_Base` component identity, an existing form keeps its header.
/// Writing a form adds the reference to the Microsoft Forms 2.0 Object
/// Library (`MSForms`) when the project does not have it yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserForm {
    /// Name of the form (module name and name of the designer storage).
    pub name:     String,
    /// Code pane text of the form, without `Attribute` lines.
    pub code:     String,
    /// Designer streams, keyed by their path relative to the form storage
    /// (`"f"`, `"o"`, `"\u{1}CompObj"`, `"\u{3}VBFrame"`, `"i01/f"`, ...).
    pub streams:  BTreeMap<String, Vec<u8>>,
    /// Storages below (and including, as `""`) the form storage with their
    /// CLSID in wire byte order.
    pub storages: BTreeMap<String, [u8; 16]>,
}

/// A user form as the two files the VBA editor exports and imports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormFiles {
    /// Text of `<Name>.frm`: the designer block, the attributes and the
    /// code, with CR LF line terminators. To be stored in the ANSI code
    /// page of the machine that imports it.
    pub frm: String,
    /// Contents of `<Name>.frx`, which must lie next to the `.frm` file.
    pub frx: Vec<u8>,
}

/// Changes to apply with [`build_project`] or [`write_project_bin`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectEdit {
    /// Modules to create or replace (standard, class, document; the code
    /// of an *existing* form may be replaced too). `source` is the full
    /// module text including `Attribute` lines.
    pub upsert:         Vec<VbaModule>,
    /// Names of standard or class modules to delete (never documents or
    /// forms). Names that do not exist are ignored.
    pub remove:         Vec<String>,
    /// Modules to rename, `(old, new)`: a document module whose sheet got
    /// another code name. Names that do not exist are ignored.
    pub rename:         Vec<(String, String)>,
    /// When set, document modules whose name is not listed are deleted (the
    /// modules of sheets that were removed from the workbook).
    pub keep_documents: Option<Vec<String>>,
    /// Code page to store the project in. `None` keeps the existing one (a
    /// new project uses 932). Changing it re-encodes every module.
    pub codepage:       Option<u16>,
    /// New project name; `None` keeps the existing one (`VBAProject` for a
    /// new project).
    pub project_name:   Option<String>,
    /// User forms to create or replace (code and designer storage).
    pub forms:          Vec<UserForm>,
}

/// What a document module belongs to; it decides the `VB_Base` attribute of
/// a module that has to be created.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DocumentKind {
    /// The workbook (`ThisWorkbook`).
    Workbook,
    /// A worksheet.
    Worksheet,
    /// A chart sheet.
    Chartsheet,
}

/// A document module that must exist in the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentSpec {
    /// Code name of the workbook or sheet.
    pub code_name: String,
    /// What the code name belongs to.
    pub kind:      DocumentKind,
}
