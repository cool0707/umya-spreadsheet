//! Reading and writing `vbaProject.bin` (\[MS-OVBA\] 2.2).

// `web_time`: the browser's clock on wasm32, where `std::time::SystemTime::now`
// panics.
use web_time::{
    SystemTime,
    UNIX_EPOCH,
};

use super::{
    DocumentSpec,
    ModuleKind,
    ProjectEdit,
    UserForm,
    VbaModule,
    VbaProjectInfo,
    cfbio::{
        self,
        Node,
        Tree,
    },
    codepage,
    compression,
    crypto::{
        self,
        Protection,
    },
    dir::{
        self,
        DirModule,
        DirStream,
    },
    error::{
        VbaError,
        format_err,
    },
    project::{
        self,
        ProjectProperties,
        same_name,
    },
    source,
    stomp,
};

/// `_VBA_PROJECT` without a performance cache (\[MS-OVBA\] 2.3.4.1):
/// Reserved1 = 0x61CC, Version = 0xFFFF, Reserved2 = 0x00 and
/// Reserved3 = 0x0001 — the value of the specification's own example
/// (3.1.1) and the one Windows versions of Office write; the field itself
/// is "undefined, MUST be ignored".
pub(crate) const VBA_PROJECT_STREAM: [u8; 7] = [0xCC, 0x61, 0xFF, 0xFF, 0x00, 0x01, 0x00];

const DEFAULT_PROJECT_NAME: &str = "VBAProject";

/// A module of a [`VbaProject`].
#[derive(Clone, Debug)]
pub struct ProjectModule {
    pub(crate) kind:       ModuleKind,
    /// Name of the stream inside the VBA storage, as stored.
    pub(crate) stream:     String,
    /// Decompressed source in the project code page.
    pub(crate) raw_source: Vec<u8>,
    /// `raw_source` decoded to UTF-8.
    pub(crate) source:     String,
    pub(crate) record:     DirModule,
}

impl ProjectModule {
    /// Module name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.record.name
    }

    /// What kind of module this is: the `PROJECT` stream says so; without
    /// an entry there the module type record of the `dir` stream and the
    /// `VB_Base` attribute decide.
    #[must_use]
    pub fn kind(&self) -> ModuleKind {
        self.kind
    }

    /// The full module text, decoded from the project code page (bytes that
    /// are not valid in it become U+FFFD), including the leading `Attribute
    /// VB_...` lines.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// A VBA project as read from a compound file: what the reading functions
/// hand out, and what [`build_project`] rewrites.
#[derive(Clone, Debug)]
pub struct VbaProject {
    pub(crate) tree:         Tree,
    /// Name of the VBA storage as stored (normally `VBA`).
    pub(crate) vba_storage:  String,
    pub(crate) dir:          DirStream,
    pub(crate) project_text: Option<String>,
    pub(crate) protection:   Protection,
    pub(crate) modules:      Vec<ProjectModule>,
    pub(crate) stomped:      Vec<String>,
}

fn find_child<'a>(
    tree: &'a Tree,
    parent: &[String],
    name: &str,
    storage: bool,
) -> Option<&'a Node> {
    tree.nodes.iter().find(|n| {
        n.is_storage() == storage
            && n.path.len() == parent.len() + 1
            && n.path[..parent.len()] == *parent
            && n.name().eq_ignore_ascii_case(name)
    })
}

fn find_child_exact_or_caseless<'a>(
    tree: &'a Tree,
    parent: &[String],
    name: &str,
) -> Option<&'a Node> {
    let candidates = || {
        tree.nodes.iter().filter(|n| {
            !n.is_storage() && n.path.len() == parent.len() + 1 && n.path[..parent.len()] == *parent
        })
    };
    candidates()
        .find(|n| n.name() == name)
        .or_else(|| candidates().find(|n| same_name(n.name(), name)))
}

impl VbaProject {
    /// Reads a `vbaProject.bin`.
    ///
    /// The `dir` stream is parsed according to \[MS-OVBA\] and every module
    /// is decompressed from its `MODULEOFFSET`, so projects saved by Office
    /// (with p-code in front of the source) and projects written by this
    /// module are handled alike. Module order is the order of the `dir`
    /// stream.
    ///
    /// A protected project is read all the same ([`Self::is_protected`]).
    pub fn read(bin: &[u8]) -> Result<VbaProject, VbaError> {
        Self::from_tree(cfbio::read_tree(bin)?)
    }

    /// Reads the project that a compound file holds in the storage
    /// `storage` instead of its root: `_VBA_PROJECT_CUR` of an `.xls` workbook
    /// (`Macros` of a `.doc` document). `storage` is the path of the storage
    /// from the root, its names separated by `/`; names compare
    /// case-insensitively. Nothing outside that storage is read.
    pub fn read_storage(bin: &[u8], storage: &str) -> Result<VbaProject, VbaError> {
        let path: Vec<String> = storage
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        Self::from_tree(cfbio::read_tree_at(bin, &path)?)
    }

    fn from_tree(tree: Tree) -> Result<VbaProject, VbaError> {
        let Some(vba) = find_child(&tree, &[], "VBA", true) else {
            return format_err("the compound file has no VBA storage");
        };
        let vba_storage = vba.name().to_string();
        let vba_path = vec![vba_storage.clone()];

        let Some(dir_node) = find_child(&tree, &vba_path, "dir", false) else {
            return format_err("the VBA storage has no dir stream");
        };
        let dir_bytes = compression::decompress(dir_node.data.as_deref().unwrap_or(&[]))
            .map_err(|e| VbaError::Format(format!("dir stream: {e}")))?;
        let dir = DirStream::parse(&dir_bytes)?;
        let cp = dir.codepage;

        let project_text = match find_child(&tree, &[], "PROJECT", false) {
            Some(node) => Some(codepage::decode(cp, node.data.as_deref().unwrap_or(&[]))?),
            None => None,
        };
        let properties = project_text
            .as_deref()
            .map(ProjectProperties::parse)
            .unwrap_or_default();
        let protection =
            Protection::from_properties(properties.cmg.as_deref(), properties.dpb.as_deref());

        let vba_version = find_child(&tree, &vba_path, "_VBA_PROJECT", false)
            .and_then(|n| n.data.as_deref())
            .filter(|d| d.len() >= 4 && d[0] == 0xCC && d[1] == 0x61)
            .map(|d| u16::from_le_bytes([d[2], d[3]]));

        let mut modules = Vec::with_capacity(dir.modules.len());
        let mut stomped = Vec::new();
        for record in &dir.modules {
            let node = [&record.stream_name, &record.stream_name_mbcs, &record.name]
                .into_iter()
                .find_map(|candidate| find_child_exact_or_caseless(&tree, &vba_path, candidate));
            let Some(node) = node else {
                return Err(VbaError::MissingModuleStream {
                    module: record.name.clone(),
                    stream: record.stream_name.clone(),
                });
            };
            let data = node.data.as_deref().unwrap_or(&[]);
            let offset = record.offset as usize;
            if offset > data.len() {
                return format_err(format!(
                    "module `{}`: source offset {offset} is beyond the end of its stream ({} bytes)",
                    record.name,
                    data.len()
                ));
            }
            let raw_source = if offset == data.len() {
                Vec::new()
            } else {
                compression::decompress(&data[offset..])
                    .map_err(|e| VbaError::Format(format!("module `{}`: {e}", record.name)))?
            };
            let source = codepage::decode(cp, &raw_source)?;
            let kind = module_kind(&properties, record, &source, &tree);

            if offset > 0 && !protection.is_protected() {
                if let Some(version) = vba_version {
                    if stomp::is_stomped(&data[..offset], version, &source) == Some(true) {
                        stomped.push(record.name.clone());
                    }
                }
            }
            modules.push(ProjectModule {
                kind,
                stream: node.name().to_string(),
                raw_source,
                source,
                record: record.clone(),
            });
        }

        Ok(VbaProject {
            tree,
            vba_storage,
            dir,
            project_text,
            protection,
            modules,
            stomped,
        })
    }

    /// Project name (`PROJECTNAME`).
    #[must_use]
    pub fn name(&self) -> &str {
        &self.dir.name
    }

    /// Windows code page of all MBCS text in the project.
    #[must_use]
    pub fn codepage(&self) -> u16 {
        self.dir.codepage
    }

    /// The project is protected (see [`VbaProjectInfo::is_protected`]);
    /// [`build_project`] refuses to rewrite it.
    #[must_use]
    pub fn is_protected(&self) -> bool {
        self.protection.is_protected()
    }

    /// The modules, in the order of the `dir` stream.
    #[must_use]
    pub fn modules(&self) -> &[ProjectModule] {
        &self.modules
    }

    /// The module named `name` (module names compare case-insensitively).
    #[must_use]
    pub fn module(&self, name: &str) -> Option<&ProjectModule> {
        self.modules
            .iter()
            .find(|m| same_name(&m.record.name, name))
    }

    /// Modules whose stored source does not match their compiled p-code
    /// (see [`VbaProjectInfo::stomped_modules`]).
    #[must_use]
    pub fn stomped_modules(&self) -> &[String] {
        &self.stomped
    }

    /// What [`read_project_bin`] reports about the project.
    #[must_use]
    pub fn info(&self) -> VbaProjectInfo {
        VbaProjectInfo {
            name:            self.dir.name.clone(),
            codepage:        self.dir.codepage,
            modules:         self
                .modules
                .iter()
                .map(|m| VbaModule {
                    name:   m.record.name.clone(),
                    kind:   m.kind,
                    source: m.source.clone(),
                })
                .collect(),
            is_protected:    self.protection.is_protected(),
            stomped_modules: self.stomped.clone(),
        }
    }
}

/// Determines the kind of a module: the `PROJECT` stream is authoritative;
/// without an entry there the module type record and the `VB_Base`
/// attribute decide.
fn module_kind(
    properties: &ProjectProperties,
    record: &DirModule,
    source_text: &str,
    tree: &Tree,
) -> ModuleKind {
    if let Some(kind) = properties.kind_of(&record.name) {
        return kind;
    }
    if record.procedural {
        return ModuleKind::Standard;
    }
    match source::attribute_value(source_text, "VB_Base") {
        // A designer's base is `0{clsid}{clsid}`; a document's is `0{clsid}`.
        Some(base) if base.matches('{').count() >= 2 => ModuleKind::Form,
        Some(base) if base.eq_ignore_ascii_case(source::VB_BASE_CLASS) => ModuleKind::Class,
        Some(_) => ModuleKind::Document,
        None => {
            if find_child(tree, &[], &record.stream_name, true).is_some() {
                ModuleKind::Form
            } else {
                ModuleKind::Class
            }
        }
    }
}

/// Reads a `vbaProject.bin` ([`VbaProject::read`]) and reports what it
/// holds.
///
/// For a protected project the function still succeeds and reports
/// `is_protected = true`.
pub fn read_project_bin(bin: &[u8]) -> Result<VbaProjectInfo, VbaError> {
    Ok(VbaProject::read(bin)?.info())
}

/// A module of the project that is about to be written.
struct PlannedModule {
    name:         String,
    kind:         ModuleKind,
    stream:       String,
    /// Source bytes in the target code page.
    bytes:        Vec<u8>,
    docstring:    String,
    help_context: u32,
    procedural:   bool,
    readonly:     bool,
    private:      bool,
}

fn validate_module_name(name: &str) -> Result<(), VbaError> {
    let mut chars = name.chars();
    let ok = match chars.next() {
        Some(first) => {
            first.is_alphabetic()
                && chars.all(|c| c.is_alphanumeric() || c == '_')
                && name.encode_utf16().count() <= 31
        }
        None => false,
    };
    if !ok {
        return Err(VbaError::InvalidRequest(format!(
            "`{name}` is not a valid module name (a letter followed by letters, digits or `_`, at most 31 characters)"
        )));
    }
    let upper = name.to_ascii_uppercase();
    if upper == "DIR" || upper == "_VBA_PROJECT" || upper.starts_with("__SRP_") {
        return Err(VbaError::InvalidRequest(format!(
            "module name `{name}` collides with a reserved stream name"
        )));
    }
    Ok(())
}

fn validate_project_name(name: &str) -> Result<(), VbaError> {
    if name.is_empty() || name.chars().any(|c| c.is_control() || c == '"') {
        return Err(VbaError::InvalidRequest(format!(
            "`{name}` is not a valid project name"
        )));
    }
    Ok(())
}

/// Compresses module source so that it reads back exactly.
///
/// The format cannot store a final chunk that is both shorter than 4096
/// bytes and incompressible without padding it with NUL bytes
/// (\[MS-OVBA\] 2.4.1.3.10). In that (very rare) case blank lines are
/// appended until the chunk is full, which VBA ignores. Returns the bytes
/// that were actually stored and their compressed form.
pub(crate) fn compress_source(mut bytes: Vec<u8>) -> (Vec<u8>, Vec<u8>) {
    loop {
        let compressed = compression::compress(&bytes);
        if compression::is_lossless(&bytes, &compressed) {
            return (bytes, compressed);
        }
        let missing = 4096 - bytes.len() % 4096;
        for _ in 0..missing.div_ceil(2) {
            bytes.extend_from_slice(b"\r\n");
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "any 32-bit value will do; the seconds since 1970 are one that changes"
)]
fn now_as_version() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x6000_0000, |d| d.as_secs() as u32)
}

/// Writes a new `vbaProject.bin`, or rewrites the existing one `existing`
/// (see [`build_project`]).
pub fn write_project_bin(
    existing: Option<&[u8]>,
    documents: &[DocumentSpec],
    edit: &ProjectEdit,
) -> Result<Vec<u8>, VbaError> {
    let loaded = match existing {
        Some(bin) => Some(VbaProject::read(bin)?),
        None => None,
    };
    Ok(build_project(loaded.as_ref(), documents, edit)?.bytes)
}

/// Result of [`build_project`].
#[derive(Clone, Debug)]
pub struct BuiltProject {
    /// The new `vbaProject.bin`.
    pub bytes:             Vec<u8>,
    /// Every module of the new project with the text that was stored
    /// (after `Attribute` completion, CR LF normalisation and the round trip
    /// through the code page).
    pub modules:           Vec<VbaModule>,
    /// Modules that received trailing blank lines because their last chunk
    /// could not be stored otherwise: the format cannot store a final chunk
    /// that is both shorter than 4096 bytes and incompressible, so blank
    /// lines, which VBA ignores, fill it up.
    pub padded:            Vec<String>,
    /// User forms whose designer storage differs from the previous one
    /// (or is new).
    pub changed_designers: Vec<String>,
}

/// Creates a new `vbaProject.bin` or rewrites an existing one.
///
/// * `existing = None` builds a brand-new project (code page 932 unless
///   `edit.codepage` says otherwise, name `VBAProject`, references to `stdole`
///   and `Office`, a fresh project GUID, unprotected).
/// * `documents` lists the document modules that must exist — `ThisWorkbook`
///   and one per sheet code name. Missing ones are created with the proper
///   attribute header; in a new project they come first, in the given order.
/// * `edit.upsert` creates or replaces modules. The source is the complete
///   module text; missing `Attribute` header lines are added (see the [module
///   documentation](super)). A new `ModuleKind::Document` module must be listed
///   in `documents` ([`VbaError::UnknownCodeName`] otherwise) and a new
///   `ModuleKind::Form` cannot be created this way
///   ([`VbaError::UnsupportedFormat`]); the code of an existing form can be
///   replaced.
/// * `edit.forms` creates user forms or replaces the code and the designer
///   storage of existing ones (see [`UserForm`]); the `MSForms` reference is
///   added when it is missing.
/// * `edit.remove` deletes standard and class modules; unknown names are
///   ignored, document modules and forms are refused.
/// * `edit.rename` renames modules (and their `Attribute VB_Name`), and
///   `edit.keep_documents` deletes the document modules it does not list.
///
/// What is written (\[MS-OVBA\]): module streams hold the compressed source
/// only (`MODULEOFFSET = 0`, no p-code), `_VBA_PROJECT` is the 7-byte
/// version-independent header with version `0xFFFF`, `__SRP_*` caches are
/// dropped, `dir`, `PROJECT` and `PROJECTwm` are regenerated. Project
/// information, references, `ID`, `CMG` / `DPB` / `GC`, host extender info,
/// window records, designer storages and all unrelated streams of an
/// existing project are kept; modules that are not touched keep their source
/// bytes exactly. The result is read back before it is returned: every
/// module must come out as [`BuiltProject::modules`] says
/// ([`VbaError::Verify`] otherwise).
///
/// A protected project is refused with [`VbaError::Protected`].
pub fn build_project(
    loaded: Option<&VbaProject>,
    documents: &[DocumentSpec],
    edit: &ProjectEdit,
) -> Result<BuiltProject, VbaError> {
    if let Some(project) = loaded {
        if project.protection.is_protected() {
            return Err(VbaError::Protected);
        }
    }
    let old_cp = loaded.map(|p| p.dir.codepage);
    let cp = edit
        .codepage
        .or(old_cp)
        .unwrap_or(codepage::DEFAULT_CODEPAGE);
    if Some(cp) != old_cp {
        codepage::writable_encoding(cp)?;
    }
    if let Some(name) = &edit.project_name {
        validate_project_name(name)?;
    }
    let doc_base = |name: &str| -> Option<&'static str> {
        documents
            .iter()
            .find(|d| same_name(&d.code_name, name))
            .map(|d| d.kind.base())
    };

    // ---- module list ------------------------------------------------------
    let mut modules: Vec<PlannedModule> = Vec::new();
    // Decoded source of the module each planned module replaces.
    let mut previous_source: Vec<Option<String>> = Vec::new();
    if let Some(project) = loaded {
        for module in &project.modules {
            let bytes = if Some(cp) == old_cp {
                module.raw_source.clone()
            } else {
                let what = format!("source of module `{}`", module.record.name);
                let text =
                    codepage::decode_strict(project.dir.codepage, &module.raw_source, &what)?;
                codepage::encode(cp, &text, &what)?
            };
            modules.push(PlannedModule {
                name: module.record.name.clone(),
                kind: module.kind,
                stream: module.stream.clone(),
                bytes,
                docstring: module.record.docstring.clone(),
                help_context: module.record.help_context,
                procedural: module.record.procedural,
                readonly: module.record.readonly,
                private: module.record.private,
            });
            previous_source.push(Some(module.source.clone()));
        }
    }

    for name in &edit.remove {
        if let Some(index) = modules.iter().position(|m| same_name(&m.name, name)) {
            match modules[index].kind {
                ModuleKind::Standard | ModuleKind::Class => {
                    modules.remove(index);
                    previous_source.remove(index);
                }
                ModuleKind::Document | ModuleKind::Form => {
                    return Err(VbaError::InvalidRequest(format!(
                        "`{name}` is a document module or form and cannot be removed"
                    )));
                }
            }
        }
    }

    // A document module follows its sheet's code name.
    for (old, new) in &edit.rename {
        let Some(index) = modules.iter().position(|m| same_name(&m.name, old)) else {
            continue;
        };
        validate_module_name(new)?;
        if modules
            .iter()
            .enumerate()
            .any(|(i, m)| i != index && same_name(&m.name, new))
        {
            return Err(VbaError::InvalidRequest(format!(
                "module `{old}` cannot be renamed to `{new}`: a module of that name exists"
            )));
        }
        let what = format!("source of module `{old}`");
        let text = codepage::decode_strict(cp, &modules[index].bytes, &what)?;
        let renamed: String = text
            .split_inclusive('\n')
            .map(|line| {
                if line
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("attribute vb_name")
                {
                    format!(
                        "Attribute VB_Name = \"{new}\"{}",
                        if line.ends_with("\r\n") {
                            "\r\n"
                        } else if line.ends_with('\n') {
                            "\n"
                        } else {
                            ""
                        }
                    )
                } else {
                    line.to_string()
                }
            })
            .collect();
        modules[index].bytes = codepage::encode(cp, &renamed, &what)?;
        modules[index].name.clone_from(new);
        modules[index].stream.clone_from(new);
    }
    // The modules of sheets that are gone.
    if let Some(keep) = &edit.keep_documents {
        let mut index = 0;
        while index < modules.len() {
            if modules[index].kind == ModuleKind::Document
                && !keep.iter().any(|k| same_name(k, &modules[index].name))
            {
                modules.remove(index);
                previous_source.remove(index);
            } else {
                index += 1;
            }
        }
    }

    // Document modules that must exist.
    let mut created_documents = 0usize;
    for doc in documents {
        match modules.iter().find(|m| same_name(&m.name, &doc.code_name)) {
            Some(existing) if existing.kind == ModuleKind::Document => {}
            Some(existing) => {
                return Err(VbaError::InvalidRequest(format!(
                    "code name `{}` is already used by a module that is not a document module ({:?})",
                    doc.code_name, existing.kind
                )));
            }
            None => {
                validate_module_name(&doc.code_name)?;
                let text = source::document_source(&doc.code_name, doc.kind);
                let what = format!("source of module `{}`", doc.code_name);
                let module = PlannedModule {
                    name:         doc.code_name.clone(),
                    kind:         ModuleKind::Document,
                    stream:       doc.code_name.clone(),
                    bytes:        codepage::encode(cp, &text, &what)?,
                    docstring:    String::new(),
                    help_context: 0,
                    procedural:   false,
                    readonly:     false,
                    private:      false,
                };
                if loaded.is_none() {
                    // New project: documents first, in the given order.
                    modules.insert(created_documents, module);
                    previous_source.insert(created_documents, None);
                    created_documents += 1;
                } else {
                    modules.push(module);
                    previous_source.push(None);
                }
            }
        }
    }

    for (position, upsert) in edit.upsert.iter().enumerate() {
        if edit.upsert[..position]
            .iter()
            .any(|m| same_name(&m.name, &upsert.name))
        {
            return Err(VbaError::InvalidRequest(format!(
                "module `{}` is listed more than once",
                upsert.name
            )));
        }
        if edit.remove.iter().any(|n| same_name(n, &upsert.name)) {
            return Err(VbaError::InvalidRequest(format!(
                "module `{}` is both written and removed",
                upsert.name
            )));
        }
        validate_module_name(&upsert.name)?;
        let what = format!("source of module `{}`", upsert.name);
        if let Some(index) = modules
            .iter()
            .position(|m| same_name(&m.name, &upsert.name))
        {
            let old_kind = modules[index].kind;
            let fixed = |k: ModuleKind| matches!(k, ModuleKind::Document | ModuleKind::Form);
            if old_kind != upsert.kind && (fixed(old_kind) || fixed(upsert.kind)) {
                return Err(VbaError::InvalidRequest(format!(
                    "module `{}` is a {:?} module and cannot become a {:?} module",
                    upsert.name, old_kind, upsert.kind
                )));
            }
            let base = doc_base(&upsert.name).unwrap_or(source::VB_BASE_WORKSHEET);
            let text = source::prepare_module_source(
                upsert.kind,
                &upsert.name,
                &upsert.source,
                previous_source[index].as_deref(),
                base,
            );
            let module = &mut modules[index];
            module.name.clone_from(&upsert.name);
            module.kind = upsert.kind;
            module.procedural = upsert.kind == ModuleKind::Standard;
            module.bytes = codepage::encode(cp, &text, &what)?;
        } else {
            let base = match upsert.kind {
                ModuleKind::Form => {
                    return Err(VbaError::UnsupportedFormat(format!(
                        "creating the new user form `{}` is not supported (only the code of an existing form can be replaced)",
                        upsert.name
                    )));
                }
                ModuleKind::Document => match doc_base(&upsert.name) {
                    Some(base) => base,
                    None => return Err(VbaError::UnknownCodeName(upsert.name.clone())),
                },
                _ => "",
            };
            let text = source::prepare_module_source(
                upsert.kind,
                &upsert.name,
                &upsert.source,
                None,
                base,
            );
            modules.push(PlannedModule {
                name:         upsert.name.clone(),
                kind:         upsert.kind,
                stream:       upsert.name.clone(),
                bytes:        codepage::encode(cp, &text, &what)?,
                docstring:    String::new(),
                help_context: 0,
                procedural:   upsert.kind == ModuleKind::Standard,
                readonly:     false,
                private:      false,
            });
            previous_source.push(None);
        }
    }

    // ---- user forms
    // ----------------------------------------------------------
    let mut designer_names: Vec<String> = Vec::new();
    for form in &edit.forms {
        validate_module_name(&form.name)?;
        if ["VBA", "PROJECT", "PROJECTWM"].contains(&form.name.to_ascii_uppercase().as_str()) {
            return Err(VbaError::InvalidRequest(format!(
                "`{}` is reserved and cannot name a user form",
                form.name
            )));
        }
        if designer_names.iter().any(|n| same_name(n, &form.name))
            || edit.upsert.iter().any(|m| same_name(&m.name, &form.name))
            || edit.remove.iter().any(|n| same_name(n, &form.name))
        {
            return Err(VbaError::InvalidRequest(format!(
                "user form `{}` is listed more than once",
                form.name
            )));
        }
        let body = source::normalize_newlines(&form.code);
        if body
            .split("\r\n")
            .any(|line| line.trim_start().starts_with("Attribute VB_"))
        {
            return Err(VbaError::InvalidRequest(format!(
                "the code of user form `{}` must not contain `Attribute VB_` lines",
                form.name
            )));
        }
        let body = if body.is_empty() || body.ends_with("\r\n") {
            body
        } else {
            body + "\r\n"
        };
        validate_designer(form)?;
        let what = format!("source of module `{}`", form.name);
        if let Some(index) = modules.iter().position(|m| same_name(&m.name, &form.name)) {
            if modules[index].kind != ModuleKind::Form {
                return Err(VbaError::InvalidRequest(format!(
                    "module `{}` is a {:?} module, not a user form",
                    form.name, modules[index].kind
                )));
            }
            let text = source::prepare_module_source(
                ModuleKind::Form,
                &form.name,
                &body,
                previous_source[index].as_deref(),
                "",
            );
            let module = &mut modules[index];
            module.name.clone_from(&form.name);
            module.bytes = codepage::encode(cp, &text, &what)?;
        } else {
            let storage_taken = loaded.is_some_and(|p| {
                p.tree
                    .nodes
                    .iter()
                    .any(|n| !n.path.is_empty() && same_name(&n.path[0], &form.name))
            });
            if storage_taken {
                return Err(VbaError::InvalidRequest(format!(
                    "the project already has a storage named `{}`",
                    form.name
                )));
            }
            let text = user_form_header(&form.name) + &body;
            modules.push(PlannedModule {
                name:         form.name.clone(),
                kind:         ModuleKind::Form,
                stream:       form.name.clone(),
                bytes:        codepage::encode(cp, &text, &what)?,
                docstring:    String::new(),
                help_context: 0,
                procedural:   false,
                readonly:     false,
                private:      false,
            });
            previous_source.push(None);
        }
        designer_names.push(form.name.clone());
    }

    // Stream names must be unique within the VBA storage.
    for (i, module) in modules.iter().enumerate() {
        if modules[..i]
            .iter()
            .any(|m| m.stream.to_uppercase() == module.stream.to_uppercase())
        {
            return Err(VbaError::InvalidRequest(format!(
                "two modules would share the stream `{}`",
                module.stream
            )));
        }
    }

    // ---- dir ---------------------------------------------------------------
    let project_name = edit
        .project_name
        .clone()
        .or_else(|| loaded.map(|p| p.dir.name.clone()))
        .unwrap_or_else(|| DEFAULT_PROJECT_NAME.to_string());
    let records: Vec<DirModule> = modules
        .iter()
        .map(|m| DirModule {
            name:             m.name.clone(),
            stream_name:      m.stream.clone(),
            stream_name_mbcs: m.stream.clone(),
            docstring:        m.docstring.clone(),
            offset:           0,
            help_context:     m.help_context,
            procedural:       m.procedural,
            readonly:         m.readonly,
            private:          m.private,
        })
        .collect();
    let (mut dir_stream, mut references) = if let Some(project) = loaded {
        let mut d = project.dir.clone();
        let references = if cp == d.codepage {
            d.references_raw.clone()
        } else {
            dir::serialize_references(&d.references, d.codepage, cp)?
        };
        d.codepage = cp;
        d.name.clone_from(&project_name);
        d.version_major = now_as_version();
        d.modules = records;
        (d, references)
    } else {
        let references = dir::default_references();
        let raw = dir::serialize_references(&references, cp, cp)?;
        let d = DirStream {
            syskind: dir::SYSKIND_WIN32,
            compat_version: None,
            lcid: dir::LCID_EN_US,
            lcid_invoke: dir::LCID_EN_US,
            codepage: cp,
            name: project_name.clone(),
            docstring: String::new(),
            help_file: String::new(),
            help_context: 0,
            lib_flags: 0,
            version_major: now_as_version(),
            version_minor: 1,
            constants: Some(String::new()),
            references_raw: raw.clone(),
            references,
            modules: records,
        };
        (d, raw)
    };
    if !edit.forms.is_empty() && !has_msforms_reference(&dir_stream.references) {
        let addition = msforms_reference();
        references.extend(dir::serialize_references(&addition, cp, cp)?);
        dir_stream.references.extend(addition);
        dir_stream.references_raw.clone_from(&references);
    }
    let dir_bytes = dir_stream.serialize(&references)?;
    let dir_compressed = compression::compress(&dir_bytes);
    if !compression::is_lossless(&dir_bytes, &dir_compressed) {
        return format_err("the dir stream cannot be stored without padding");
    }

    // ---- PROJECT / PROJECTwm ----------------------------------------------
    let module_list: Vec<(ModuleKind, String)> =
        modules.iter().map(|m| (m.kind, m.name.clone())).collect();
    let project_text = match loaded.and_then(|p| p.project_text.as_deref()) {
        Some(text) => {
            project::regenerate_project_text(text, &module_list, edit.project_name.as_deref())
        }
        None => project::new_project_text(&project_name, &module_list),
    };
    let project_bytes = codepage::encode(cp, &project_text, "PROJECT stream")?;
    let names: Vec<String> = modules.iter().map(|m| m.name.clone()).collect();
    let project_wm = project::project_wm(&names, cp)?;

    // ---- compound file ------------------------------------------------------
    let vba = loaded.map_or_else(|| "VBA".to_string(), |p| p.vba_storage.clone());
    let mut tree = Tree::default();
    if let Some(project) = loaded {
        tree.root_meta.clone_from(&project.tree.root_meta);
        tree.root_times = project.tree.root_times;
        let old_streams: Vec<String> = project
            .modules
            .iter()
            .map(|m| m.stream.to_uppercase())
            .collect();
        for node in &project.tree.nodes {
            let top = node.path.len() == 1;
            let in_vba = node.path.len() == 2 && node.path[0] == vba && !node.is_storage();
            let upper = node.name().to_uppercase();
            if top && !node.is_storage() && (upper == "PROJECT" || upper == "PROJECTWM") {
                continue;
            }
            if designer_names.iter().any(|n| same_name(n, &node.path[0])) {
                continue;
            }
            if in_vba
                && (upper == "DIR"
                    || upper == "_VBA_PROJECT"
                    || upper.starts_with("__SRP_")
                    || old_streams.contains(&upper))
            {
                continue;
            }
            let mut node = node.clone();
            if cp != project.dir.codepage
                && !node.is_storage()
                && node.name().eq_ignore_ascii_case("\u{3}VBFrame")
            {
                // Designer properties are MBCS text in the project code page.
                if let Some(data) = &node.data {
                    if !data.is_ascii() {
                        let text =
                            codepage::decode_strict(project.dir.codepage, data, "VBFrame stream")?;
                        node.data = Some(codepage::encode(cp, &text, "VBFrame stream")?);
                    }
                }
            }
            tree.nodes.push(node);
        }
    }
    if !tree
        .nodes
        .iter()
        .any(|n| n.is_storage() && n.path.len() == 1 && n.path[0] == vba)
    {
        tree.nodes.push(Node::storage(vec![vba.clone()]));
    }
    let mut changed_designers = Vec::new();
    for form in &edit.forms {
        let mut new_nodes = Vec::new();
        for (path, clsid) in &form.storages {
            let mut node = Node::storage(designer_path(&form.name, path));
            node.clsid = Some(*clsid);
            new_nodes.push(node);
        }
        for (path, data) in &form.streams {
            new_nodes.push(Node::stream(designer_path(&form.name, path), data.clone()));
        }
        let old = loaded
            .map(|p| designer_snapshot(p.tree.nodes.iter(), &form.name))
            .unwrap_or_default();
        if old != designer_snapshot(new_nodes.iter(), &form.name) {
            changed_designers.push(form.name.clone());
        }
        tree.nodes.extend(new_nodes);
    }
    tree.nodes
        .push(Node::stream(vec!["PROJECT".to_string()], project_bytes));
    tree.nodes
        .push(Node::stream(vec!["PROJECTwm".to_string()], project_wm));
    tree.nodes.push(Node::stream(
        vec![vba.clone(), "_VBA_PROJECT".to_string()],
        VBA_PROJECT_STREAM.to_vec(),
    ));
    tree.nodes.push(Node::stream(
        vec![vba.clone(), "dir".to_string()],
        dir_compressed,
    ));
    let mut built_modules = Vec::with_capacity(modules.len());
    let mut padded = Vec::new();
    for module in modules {
        let original_len = module.bytes.len();
        let (stored, compressed) = compress_source(module.bytes);
        if stored.len() != original_len {
            padded.push(module.name.clone());
        }
        if tree.nodes.iter().any(|n| {
            n.path.len() == 2
                && n.path[0] == vba
                && n.name().to_uppercase() == module.stream.to_uppercase()
        }) {
            return Err(VbaError::InvalidRequest(format!(
                "module `{}` collides with an existing stream in the VBA storage",
                module.name
            )));
        }
        built_modules.push(VbaModule {
            name:   module.name,
            kind:   module.kind,
            source: codepage::decode(cp, &stored)?,
        });
        tree.nodes
            .push(Node::stream(vec![vba.clone(), module.stream], compressed));
    }
    let bytes = cfbio::write_tree(&tree)?;

    // Read the result back with the regular reader: every module must come
    // out exactly as planned.
    let check = VbaProject::read(&bytes)
        .map_err(|e| VbaError::Verify(format!("the new VBA project cannot be read back: {e}")))?;
    verify_modules(&built_modules, &check.info().modules)?;
    if check.dir.codepage != cp || check.protection.is_protected() {
        return Err(VbaError::Verify(
            "project properties did not read back as written".into(),
        ));
    }
    Ok(BuiltProject {
        bytes,
        modules: built_modules,
        padded,
        changed_designers,
    })
}

/// LIBID prefix of the Microsoft Forms 2.0 Object Library.
const MSFORMS_LIBID_PREFIX: &str = "*\\G{0D452EE1-E08F-101A-852E-02608C4D0BB4}#";
/// Its GUID in wire byte order (`OriginalTypeLib` of a control reference).
const MSFORMS_GUID_WIRE: [u8; 16] = [
    0xE1, 0x2E, 0x45, 0x0D, 0x8F, 0xE0, 0x1A, 0x10, 0x85, 0x2E, 0x02, 0x60, 0x8C, 0x4D, 0x0B, 0xB4,
];

fn has_msforms_reference(parts: &[dir::ReferencePart]) -> bool {
    parts.iter().any(|part| match part {
        dir::ReferencePart::Registered { libid, .. } => String::from_utf8_lossy(libid)
            .to_ascii_uppercase()
            .starts_with(MSFORMS_LIBID_PREFIX),
        dir::ReferencePart::Control {
            original_type_lib, ..
        } => *original_type_lib == MSFORMS_GUID_WIRE,
        _ => false,
    })
}

/// The `MSForms` reference Excel adds with the first user form.
fn msforms_reference() -> Vec<dir::ReferencePart> {
    let name = "MSForms";
    let libid = "*\\G{0D452EE1-E08F-101A-852E-02608C4D0BB4}#2.0#0#C:\\Windows\\System32\\FM20.DLL#Microsoft Forms 2.0 Object Library";
    vec![
        dir::ReferencePart::Name {
            name:    name.as_bytes().to_vec(),
            unicode: Some(codepage::utf16le(name)),
        },
        dir::ReferencePart::Registered {
            libid:     libid.as_bytes().to_vec(),
            reserved1: 0,
            reserved2: 0,
        },
    ]
}

/// Attribute header of a new user form module with a fresh component
/// identity (`VB_Base = "0{guid}{guid}"`).
fn user_form_header(name: &str) -> String {
    let guid = || {
        crypto::new_guid()
            .trim_matches(|c| c == '{' || c == '}')
            .to_string()
    };
    let (first, mut second) = (guid(), guid());
    while second == first {
        second = guid();
    }
    [
        format!("Attribute VB_Name = \"{name}\""),
        format!("Attribute VB_Base = \"0{{{first}}}{{{second}}}\""),
        "Attribute VB_GlobalNameSpace = False".to_string(),
        "Attribute VB_Creatable = False".to_string(),
        "Attribute VB_PredeclaredId = True".to_string(),
        "Attribute VB_Exposed = False".to_string(),
        "Attribute VB_TemplateDerived = False".to_string(),
        "Attribute VB_Customizable = False".to_string(),
    ]
    .join("\r\n")
        + "\r\n"
}

fn designer_path(form: &str, relative: &str) -> Vec<String> {
    let mut path = vec![form.to_string()];
    path.extend(
        relative
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    );
    path
}

/// Checks that a designer storage is complete and well formed.
fn validate_designer(form: &UserForm) -> Result<(), VbaError> {
    let bad = |what: String| {
        Err(VbaError::InvalidRequest(format!(
            "designer of user form `{}`: {what}",
            form.name
        )))
    };
    if !form.storages.contains_key("") {
        return bad("the form storage itself is missing".into());
    }
    for name in ["f", "o", "\u{1}CompObj", "\u{3}VBFrame"] {
        if !form.streams.contains_key(name) {
            return bad(format!("the stream `{}` is missing", name.escape_debug()));
        }
    }
    for path in form.storages.keys().chain(form.streams.keys()) {
        if path
            .split('/')
            .any(|s| s.is_empty() || s.encode_utf16().count() > 31)
            && !path.is_empty()
        {
            return bad(format!("invalid path `{}`", path.escape_debug()));
        }
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        if !path.is_empty() && !form.storages.contains_key(parent) {
            return bad(format!("`{}` has no parent storage", path.escape_debug()));
        }
    }
    if form.streams.keys().any(|k| form.storages.contains_key(k)) {
        return bad("a path is both a stream and a storage".into());
    }
    Ok(())
}

type DesignerEntry = (Vec<String>, Option<Vec<u8>>, [u8; 16]);

/// What a designer storage consists of, for change detection.
fn designer_snapshot<'a>(nodes: impl Iterator<Item = &'a Node>, form: &str) -> Vec<DesignerEntry> {
    let mut entries: Vec<DesignerEntry> = nodes
        .filter(|n| !n.path.is_empty() && same_name(&n.path[0], form))
        .map(|n| {
            let clsid = match &n.meta {
                Some(meta) => meta.clsid().to_bytes_le(),
                None => n.clsid.unwrap_or_default(),
            };
            let path = n.path.iter().map(|s| s.to_uppercase()).collect();
            (
                path,
                n.data.clone(),
                if n.is_storage() { clsid } else { [0; 16] },
            )
        })
        .collect();
    entries.sort();
    entries
}

/// Checks that `actual` (the modules of a project read back after writing
/// it) is exactly `expected`: same modules in the same order, same kinds,
/// same source text. [`VbaError::Verify`] says what differs.
pub fn verify_modules(expected: &[VbaModule], actual: &[VbaModule]) -> Result<(), VbaError> {
    if expected.len() != actual.len() {
        return Err(VbaError::Verify(format!(
            "expected {} modules, found {}",
            expected.len(),
            actual.len()
        )));
    }
    for (want, got) in expected.iter().zip(actual) {
        if want.name != got.name {
            return Err(VbaError::Verify(format!(
                "expected module `{}`, found `{}`",
                want.name, got.name
            )));
        }
        if want.kind != got.kind {
            return Err(VbaError::Verify(format!(
                "module `{}` is a {:?} module, expected {:?}",
                want.name, got.kind, want.kind
            )));
        }
        if want.source != got.source {
            return Err(VbaError::Verify(format!(
                "the source of module `{}` does not read back as written",
                want.name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_names() {
        for good in ["Module1", "a", "Mod_1", "標準モジュール", "Ｍｏｄ１"] {
            assert!(validate_module_name(good).is_ok(), "{good}");
        }
        for bad in [
            "",
            "1abc",
            "_x",
            "a b",
            "a.b",
            "a/b",
            "dir",
            "_VBA_PROJECT",
            "__SRP_0",
            &"x".repeat(32),
        ] {
            assert!(validate_module_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn incompressible_tail_is_padded_with_blank_lines() {
        // 4000 bytes without usable repetitions: the final chunk cannot be
        // stored as tokens and would be zero-padded by the plain algorithm.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let bytes: Vec<u8> = (0..4000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 32).to_le_bytes()[0]
            })
            .collect();
        let direct = compression::compress(&bytes);
        assert!(!compression::is_lossless(&bytes, &direct));
        assert_ne!(compression::decompress(&direct).unwrap(), bytes);

        let (stored, compressed) = compress_source(bytes.clone());
        assert_eq!(stored.len(), 4096);
        assert!(stored.starts_with(&bytes));
        assert!(stored[bytes.len()..].chunks(2).all(|c| c == b"\r\n"));
        assert_eq!(compression::decompress(&compressed).unwrap(), stored);

        // An odd gap spills one byte into a further chunk.
        let (stored, compressed) = compress_source(bytes[..3999].to_vec());
        assert_eq!(stored.len(), 4097);
        assert_eq!(compression::decompress(&compressed).unwrap(), stored);

        // Ordinary text is stored as is.
        let text = b"Sub A()\r\nEnd Sub\r\n".to_vec();
        assert_eq!(compress_source(text.clone()).0, text);
    }
}
