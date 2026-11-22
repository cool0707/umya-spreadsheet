//! User forms as they are stored in a project: handing out the designer
//! storages of an existing project, writing a form as the `.frm` / `.frx`
//! pair that `VBComponents.Import` of the VBA editor reads, and reading the
//! pair that `VBComponent.Export` writes.

use std::{
    collections::BTreeMap,
    fmt::Write,
};

use super::{
    FormFiles,
    ModuleKind,
    UserForm,
    cfbio::{
        self,
        Node,
        Tree,
    },
    codepage,
    error::VbaError,
    project_bin::VbaProject,
};

fn same_name(a: &str, b: &str) -> bool {
    a.to_uppercase() == b.to_uppercase()
}

/// The user forms of a `vbaProject.bin`: for every form module its code
/// (without the `Attribute` header) and its designer storage, in the layout
/// of [`UserForm`]. A form module without a designer storage is returned
/// with empty `streams` and `storages`.
pub fn read_user_forms(bin: &[u8]) -> Result<Vec<UserForm>, VbaError> {
    Ok(VbaProject::read(bin)?.user_forms())
}

impl VbaProject {
    /// The user forms of the project (see [`read_user_forms`]).
    #[must_use]
    pub fn user_forms(&self) -> Vec<UserForm> {
        let project = self;
        let mut forms = Vec::new();
        for module in project
            .modules
            .iter()
            .filter(|m| m.kind == ModuleKind::Form)
        {
            let name = &module.record.name;
            let root = |n: &&Node| n.is_storage() && n.path.len() == 1;
            // The designer storage is named like the module stream.
            let storage = project
                .tree
                .nodes
                .iter()
                .filter(root)
                .find(|n| same_name(&n.path[0], &module.stream))
                .or_else(|| {
                    project
                        .tree
                        .nodes
                        .iter()
                        .filter(root)
                        .find(|n| same_name(&n.path[0], name))
                });
            let mut form = UserForm {
                name: name.clone(),
                code: module
                    .source
                    .replace("\r\n", "\n")
                    .lines()
                    .skip_while(|l| l.starts_with("Attribute ") || l.starts_with("VERSION "))
                    .flat_map(|l| [l, "\n"])
                    .collect(),
                ..UserForm::default()
            };
            if let Some(storage) = storage {
                let top = &storage.path[0];
                for node in project.tree.nodes.iter().filter(|n| &n.path[0] == top) {
                    let relative = node.path[1..].join("/");
                    if let Some(data) = &node.data {
                        form.streams.insert(relative, data.clone());
                    } else {
                        let clsid = match &node.meta {
                            Some(meta) => meta.clsid().to_bytes_le(),
                            None => node.clsid.unwrap_or_default(),
                        };
                        form.storages.insert(relative, clsid);
                    }
                }
            }
            forms.push(form);
        }
        forms
    }
}

/// CLSID of the form designer (`{C62A69F0-16DC-11CE-9E98-00AA00574A4F}`), wire
/// byte order.
const FORM_CLSID: [u8; 16] = [
    0xf0, 0x69, 0x2a, 0xc6, 0xdc, 0x16, 0xce, 0x11, 0x9e, 0x98, 0, 0xaa, 0, 0x57, 0x4a, 0x4f,
];

/// `CompObj` stream of the root storage of a `.frx` file. Unlike the one in
/// a project it names the class and its `ProgID`.
fn frx_compobj() -> Vec<u8> {
    let mut b = Vec::with_capacity(110);
    b.extend_from_slice(&0xfffe_0001u32.to_le_bytes());
    b.extend_from_slice(&0x0a03u32.to_le_bytes());
    b.extend_from_slice(&u32::MAX.to_le_bytes());
    b.extend_from_slice(&FORM_CLSID);
    for text in [
        "Microsoft Forms 2.0 Form\0",
        "Embedded Object\0",
        "Forms.Form.1\0",
    ] {
        b.extend_from_slice(&u32::try_from(text.len()).unwrap_or_default().to_le_bytes());
        b.extend_from_slice(text.as_bytes());
    }
    b.extend_from_slice(&0x71b2_39f4u32.to_le_bytes());
    b.extend_from_slice(&[0; 12]);
    b
}

/// Value of the property `name` in the lines of a designer block.
fn frame_property(lines: &[&str], name: &str) -> Option<i64> {
    lines.iter().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        if !key.trim().eq_ignore_ascii_case(name) {
            return None;
        }
        value.split_whitespace().next()?.parse().ok()
    })
}

/// Writes `form` as a `.frm` / `.frx` pair for `VBComponents.Import`.
///
/// The `.frx` file is a 24-byte header (`LB`, the length of what follows
/// and the designer window rectangle in twips) followed by a compound file
/// that holds the designer storage: the streams `f`, `o` and `CompObj`, and
/// the storages of nested containers. The designer properties of the
/// `VBFrame` stream go to the head of the `.frm` text instead.
/// `codepage` is the code page the `VBFrame` stream is stored in.
pub fn export_user_form(form: &UserForm, codepage: u16) -> Result<FormFiles, VbaError> {
    let bad = |what: &str| VbaError::InvalidRequest(format!("user form `{}`: {what}", form.name));
    let Some(frame) = form.streams.get("\u{3}VBFrame") else {
        return Err(bad("the VBFrame stream is missing"));
    };
    for name in ["f", "o"] {
        if !form.streams.contains_key(name) {
            return Err(bad("the designer streams are missing"));
        }
    }
    let frame = codepage::decode_strict(codepage, frame, "VBFrame")?;
    let frame = frame.replace("\r\n", "\n");
    let mut lines: Vec<&str> = frame.lines().collect();
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }
    if lines.len() < 3
        || !lines[0]
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("VERSION")
        || !lines[1]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("begin ")
        || !lines[lines.len() - 1].trim().eq_ignore_ascii_case("End")
    {
        return Err(bad("the VBFrame stream is not a designer block"));
    }
    let client_width = frame_property(&lines, "ClientWidth").unwrap_or(4800);
    let client_height = frame_property(&lines, "ClientHeight").unwrap_or(3600);
    // Offset of the client area inside the designer window, as the VBA
    // editor writes it at 96 DPI.
    let client_left = frame_property(&lines, "ClientLeft").unwrap_or(108);
    let client_top = frame_property(&lines, "ClientTop").unwrap_or(456);

    // Properties in the order the VBA editor writes them (by name).
    let blob = format!("   OleObjectBlob   =   \"{}.frx\":0000", form.name);
    let left = format!("   ClientLeft = {client_left}");
    let top = format!("   ClientTop = {client_top}");
    let mut properties: Vec<String> = lines[2..lines.len() - 1]
        .iter()
        .filter(|l| {
            !l.trim_start()
                .to_ascii_lowercase()
                .starts_with("oleobjectblob")
        })
        .map(ToString::to_string)
        .collect();
    if frame_property(&lines, "ClientLeft").is_none() {
        properties.push(left);
    }
    if frame_property(&lines, "ClientTop").is_none() {
        properties.push(top);
    }
    properties.push(blob);
    properties.sort_by_key(|l| {
        l.split('=')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
    });

    let mut frm = String::new();
    for line in lines[..2]
        .iter()
        .copied()
        .chain(properties.iter().map(String::as_str))
    {
        frm.push_str(line);
        frm.push_str("\r\n");
    }
    frm.push_str("End\r\n");
    for (key, value) in [
        ("VB_Name", format!("\"{}\"", form.name)),
        ("VB_GlobalNameSpace", "False".into()),
        ("VB_Creatable", "False".into()),
        ("VB_PredeclaredId", "True".into()),
        ("VB_Exposed", "False".into()),
    ] {
        let _ = write!(frm, "Attribute {key} = {value}\r\n");
    }
    frm.push_str(&super::source::normalize_newlines(&form.code));

    let mut tree = Tree {
        root_clsid: Some(FORM_CLSID),
        ..Tree::default()
    };
    let path = |relative: &str| relative.split('/').map(str::to_string).collect::<Vec<_>>();
    let storages: BTreeMap<&String, &[u8; 16]> = form
        .storages
        .iter()
        .filter(|(p, _)| !p.is_empty())
        .collect();
    for (relative, clsid) in storages {
        let mut node = Node::storage(path(relative));
        node.clsid = Some(*clsid);
        tree.nodes.push(node);
    }
    for (relative, data) in &form.streams {
        let data = match relative.as_str() {
            "\u{3}VBFrame" => continue,
            "\u{1}CompObj" => frx_compobj(),
            _ => data.clone(),
        };
        tree.nodes.push(Node::stream(path(relative), data));
    }
    if !form.streams.contains_key("\u{1}CompObj") {
        tree.nodes
            .push(Node::stream(path("\u{1}CompObj"), frx_compobj()));
    }
    let compound = cfbio::write_tree(&tree)?;

    let field = |value: i64| {
        i32::try_from(value).map_err(|_| bad("the size of the designer window is out of range"))
    };
    let length = u32::try_from(compound.len()).map_err(|_| bad("the designer is too large"))?;
    let mut frx = Vec::with_capacity(24 + compound.len());
    frx.extend_from_slice(b"LB\x08\x00");
    frx.extend_from_slice(&length.to_le_bytes());
    frx.extend_from_slice(&0i32.to_le_bytes());
    frx.extend_from_slice(&0i32.to_le_bytes());
    frx.extend_from_slice(&field(client_width + 2 * client_left)?.to_le_bytes());
    frx.extend_from_slice(&field(client_height + client_top + client_left)?.to_le_bytes());
    frx.extend_from_slice(&compound);
    Ok(FormFiles { frm, frx })
}

/// Reads a user form from the `.frm` / `.frx` pair the VBA editor writes
/// (`VBComponent.Export`), into the layout of [`UserForm`] that
/// [`read_user_forms`] hands out: the designer block of the `.frm` becomes
/// the `VBFrame` stream (stored in `codepage`), the compound file of the
/// `.frx` the other streams and storages. The code is what follows the
/// `Attribute` lines. `frm` is the text of the `.frm` file, already decoded
/// from the ANSI code page it was written in.
pub fn import_user_form(frm: &str, frx: &[u8], codepage: u16) -> Result<UserForm, VbaError> {
    let bad = |what: &str| VbaError::Format(format!("exported user form: {what}"));
    let text = frm.replace("\r\n", "\n");
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 3
        || !lines[0]
            .trim_start()
            .to_ascii_uppercase()
            .starts_with("VERSION")
        || !lines[1]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("begin ")
    {
        return Err(bad("the .frm file does not start with a designer block"));
    }
    // The end of the designer block (nested `Begin` / `BeginProperty` blocks
    // are skipped).
    let mut depth = 0usize;
    let mut end = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        let word = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        match word.as_str() {
            "begin" | "beginproperty" => depth += 1,
            "end" | "endproperty" => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let Some(end) = end else {
        return Err(bad("the designer block of the .frm file has no End"));
    };
    let name = lines[1].split_whitespace().nth(2).unwrap_or("").to_string();
    let mut frame = String::new();
    for line in &lines[..=end] {
        if line
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("oleobjectblob")
        {
            continue;
        }
        frame.push_str(line);
        frame.push_str("\r\n");
    }
    let mut rest = lines[end + 1..].iter().peekable();
    let mut form_name = name;
    while let Some(line) = rest.next_if(|l| l.starts_with("Attribute ")) {
        if let Some(value) = line.strip_prefix("Attribute VB_Name = ") {
            form_name = value.trim().trim_matches('"').to_string();
        }
    }
    let code: String = rest.flat_map(|l| [*l, "\n"]).collect();

    if frx.len() < 24 || &frx[..2] != b"LB" {
        return Err(bad("the .frx file does not start with its header"));
    }
    let length = u32::from_le_bytes([frx[4], frx[5], frx[6], frx[7]]) as usize;
    let compound = frx.get(24..24 + length).unwrap_or(&frx[24..]);
    let tree = cfbio::read_tree(compound)?;
    let mut form = UserForm {
        name: form_name,
        code,
        ..UserForm::default()
    };
    let clsid_of = |meta: &Option<cfb::Entry>| {
        meta.as_ref()
            .map(|m| m.clsid().to_bytes_le())
            .unwrap_or_default()
    };
    form.storages
        .insert(String::new(), clsid_of(&tree.root_meta));
    for node in &tree.nodes {
        let relative = node.path.join("/");
        match &node.data {
            Some(data) => {
                form.streams.insert(relative, data.clone());
            }
            None => {
                form.storages.insert(relative, clsid_of(&node.meta));
            }
        }
    }
    for name in ["f", "o"] {
        if !form.streams.contains_key(name) {
            return Err(bad("the .frx file has no designer streams"));
        }
    }
    form.streams.insert(
        "\u{3}VBFrame".into(),
        codepage::encode(codepage, &frame, "VBFrame")?,
    );
    Ok(form)
}
