//! The `PROJECT` stream (\[MS-OVBA\] 2.3.1) and the `PROJECTwm` stream
//! (\[MS-OVBA\] 2.3.3).

use super::{
    ModuleKind,
    codepage,
    crypto,
    error::VbaError,
    source::split_lines,
};

const HOST_EXTENDER_SECTION: &str = "[Host Extender Info]";
const HOST_EXTENDER_VBE: &str = "&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000";
const WORKSPACE_SECTION: &str = "[Workspace]";

/// The interesting properties of a `PROJECT` stream.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectProperties {
    /// `ID="{...}"` without the quotes.
    pub id:      Option<String>,
    /// `(kind, module name)` of every `Module=` / `Class=` / `Document=` /
    /// `BaseClass=` line, in stream order.
    pub modules: Vec<(ModuleKind, String)>,
    pub name:    Option<String>,
    pub cmg:     Option<String>,
    pub dpb:     Option<String>,
    pub gc:      Option<String>,
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    match v.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(inner) => inner.to_string(),
        None => v.to_string(),
    }
}

/// Classifies a property line of the first section as a module line.
fn module_line(line: &str) -> Option<(ModuleKind, &str)> {
    let (key, value) = line.split_once('=')?;
    let value = value.trim();
    match key.trim() {
        "Module" => Some((ModuleKind::Standard, value)),
        "Class" => Some((ModuleKind::Class, value)),
        "BaseClass" => Some((ModuleKind::Form, value)),
        "Document" => {
            // Document=<name>/&H<DocTlibVer>
            let name = match value.rfind("/&H") {
                Some(idx) => &value[..idx],
                None => value,
            };
            Some((ModuleKind::Document, name))
        }
        _ => None,
    }
}

fn is_section_header(line: &str) -> bool {
    line.starts_with('[')
}

impl ProjectProperties {
    pub(crate) fn parse(text: &str) -> ProjectProperties {
        let mut props = ProjectProperties::default();
        for line in split_lines(text) {
            if is_section_header(line) {
                break;
            }
            if let Some((kind, name)) = module_line(line) {
                props.modules.push((kind, name.to_string()));
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "ID" => props.id = Some(unquote(value)),
                "Name" => props.name = Some(unquote(value)),
                "CMG" => props.cmg = Some(unquote(value)),
                "DPB" => props.dpb = Some(unquote(value)),
                "GC" => props.gc = Some(unquote(value)),
                _ => {}
            }
        }
        props
    }

    /// Kind recorded for `name` (module names are case-insensitive).
    pub(crate) fn kind_of(&self, name: &str) -> Option<ModuleKind> {
        self.modules
            .iter()
            .find(|(_, n)| same_name(n, name))
            .map(|(k, _)| *k)
    }
}

/// VBA identifiers compare case-insensitively.
pub(crate) fn same_name(a: &str, b: &str) -> bool {
    a == b || a.to_lowercase() == b.to_lowercase()
}

fn module_line_for(kind: ModuleKind, name: &str) -> String {
    match kind {
        ModuleKind::Standard => format!("Module={name}"),
        ModuleKind::Class => format!("Class={name}"),
        ModuleKind::Document => format!("Document={name}/&H00000000"),
        ModuleKind::Form => format!("BaseClass={name}"),
    }
}

fn workspace_line_for(name: &str) -> String {
    format!("{name}=0, 0, 0, 0, C")
}

fn join_sections(sections: &[Vec<String>]) -> String {
    let mut out = String::new();
    for (i, section) in sections.iter().enumerate() {
        if i > 0 {
            out.push_str("\r\n");
        }
        for line in section {
            out.push_str(line);
            out.push_str("\r\n");
        }
    }
    out
}

/// Text of the `PROJECT` stream for a brand-new, unprotected project.
pub(crate) fn new_project_text(project_name: &str, modules: &[(ModuleKind, String)]) -> String {
    let id = crypto::new_guid();
    let mut header = vec![format!("ID=\"{id}\"")];
    header.extend(
        modules
            .iter()
            .map(|(kind, name)| module_line_for(*kind, name)),
    );
    header.push(format!("Name=\"{project_name}\""));
    header.push("HelpContextID=\"0\"".to_string());
    header.push("VersionCompatible32=\"393222000\"".to_string());
    // Unprotected, no password, visible ([MS-OVBA] 2.3.1.15 - 2.3.1.17).
    header.push(format!(
        "CMG=\"{}\"",
        crypto::encrypt_random(&[0, 0, 0, 0], &id)
    ));
    header.push(format!("DPB=\"{}\"", crypto::encrypt_random(&[0x00], &id)));
    header.push(format!("GC=\"{}\"", crypto::encrypt_random(&[0xFF], &id)));

    let host = vec![
        HOST_EXTENDER_SECTION.to_string(),
        HOST_EXTENDER_VBE.to_string(),
    ];
    let mut workspace = vec![WORKSPACE_SECTION.to_string()];
    workspace.extend(modules.iter().map(|(_, name)| workspace_line_for(name)));
    join_sections(&[header, host, workspace])
}

/// Rewrites an existing `PROJECT` stream for a new module list.
///
/// Module lines of surviving modules stay where they are (their text is
/// kept unless the kind changed), lines of removed modules disappear and
/// new modules are added after the last module line. The `[Workspace]`
/// section is regenerated, keeping the window records of surviving
/// modules. Every other line — `ID`, `Package`, `HelpFile`, `Name`,
/// `VersionCompatible32`, `CMG`, `DPB`, `GC`, `[Host Extender Info]`, unknown
/// sections — is preserved verbatim.
pub(crate) fn regenerate_project_text(
    existing: &str,
    modules: &[(ModuleKind, String)],
    project_name: Option<&str>,
) -> String {
    let lines = split_lines(existing);
    let first_section = lines
        .iter()
        .position(|l| is_section_header(l))
        .unwrap_or(lines.len());

    // --- property section -------------------------------------------------
    let mut header: Vec<String> = Vec::new();
    let mut emitted = vec![false; modules.len()];
    let mut insert_at: Option<usize> = None;
    let mut name_seen = false;
    for line in &lines[..first_section] {
        if let Some((_, name)) = module_line(line) {
            let found = modules
                .iter()
                .enumerate()
                .find(|(i, (_, n))| !emitted[*i] && same_name(n, name));
            if let Some((i, (kind, new_name))) = found {
                emitted[i] = true;
                let unchanged = module_line(line).is_some_and(|(k, n)| k == *kind && n == new_name);
                header.push(if unchanged {
                    line.to_string()
                } else {
                    module_line_for(*kind, new_name)
                });
                insert_at = Some(header.len());
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let key = line.split_once('=').map_or("", |(k, _)| k.trim());
        if key == "Name" {
            name_seen = true;
            if let Some(new_name) = project_name {
                header.push(format!("Name=\"{new_name}\""));
                continue;
            }
        }
        header.push(line.to_string());
    }
    // New modules go after the last module line; without any, after `ID=`.
    let mut at = insert_at.unwrap_or_else(|| {
        header
            .iter()
            .position(|l| l.starts_with("ID="))
            .map_or(0, |i| i + 1)
    });
    for (i, (kind, name)) in modules.iter().enumerate() {
        if !emitted[i] {
            header.insert(at, module_line_for(*kind, name));
            at += 1;
        }
    }
    if !name_seen {
        if let Some(new_name) = project_name {
            header.insert(at, format!("Name=\"{new_name}\""));
        }
    }

    // --- other sections ---------------------------------------------------
    let mut sections: Vec<Vec<String>> = vec![header];
    let mut workspace_done = false;
    let mut i = first_section;
    while i < lines.len() {
        let end = lines[i + 1..]
            .iter()
            .position(|l| is_section_header(l))
            .map_or(lines.len(), |p| i + 1 + p);
        let body = &lines[i + 1..end];
        if lines[i].trim() == WORKSPACE_SECTION && !workspace_done {
            workspace_done = true;
            let mut section = vec![WORKSPACE_SECTION.to_string()];
            for (_, name) in modules {
                let old = body.iter().find(|l| {
                    l.split_once('=')
                        .is_some_and(|(k, _)| same_name(k.trim(), name))
                });
                section.push(match old {
                    Some(l) => match l.split_once('=') {
                        Some((_, value)) => format!("{name}={value}"),
                        None => workspace_line_for(name),
                    },
                    None => workspace_line_for(name),
                });
            }
            sections.push(section);
        } else {
            let mut section = vec![lines[i].to_string()];
            section.extend(
                body.iter()
                    .filter(|l| !l.trim().is_empty())
                    .map(ToString::to_string),
            );
            sections.push(section);
        }
        i = end;
    }
    if !sections
        .iter()
        .any(|s| s.first().map(|l| l.trim()) == Some(HOST_EXTENDER_SECTION))
    {
        // HostExtenders is mandatory in the grammar.
        sections.insert(
            1,
            vec![
                HOST_EXTENDER_SECTION.to_string(),
                HOST_EXTENDER_VBE.to_string(),
            ],
        );
    }
    if !workspace_done {
        let mut section = vec![WORKSPACE_SECTION.to_string()];
        section.extend(modules.iter().map(|(_, name)| workspace_line_for(name)));
        sections.push(section);
    }
    join_sections(&sections)
}

/// Builds the `PROJECTwm` stream: for every module the MBCS name, a NUL,
/// the UTF-16LE name and a 16-bit NUL; then a final 16-bit NUL.
pub(crate) fn project_wm(module_names: &[String], cp: u16) -> Result<Vec<u8>, VbaError> {
    let mut out = Vec::new();
    for name in module_names {
        out.extend_from_slice(&codepage::encode(
            cp,
            name,
            &format!("name of module `{name}`"),
        )?);
        out.push(0);
        out.extend_from_slice(&codepage::utf16le(name));
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXCEL: &str = "ID=\"{8D807122-0657-42C8-BC6F-4B5FD08031C9}\"\r\nDocument=ThisWorkbook/&H00000000\r\nDocument=Sheet1/&H00000000\r\nPackage={AC9F2F90-E877-11CE-9F68-00AA00574A4F}\r\nModule=Module1\r\nBaseClass=UserForm1\r\nHelpFile=\"c:\\help.chm\"\r\nName=\"VBAProject\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\nCMG=\"DBD966ECE4F0E4F0E4F0E4F0\"\r\nDPB=\"5557E86E18E919E919E9\"\r\nGC=\"CFCD72F0961011111111EE\"\r\n\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n&H00000002={000209F2-0000-0000-C000-000000000046};Word8.0;&H00000000\r\n\r\n[Workspace]\r\nThisWorkbook=0, 0, 0, 0, C\r\nSheet1=0, 0, 0, 0, C\r\nModule1=110, 145, 994, 721, \r\nUserForm1=0, 0, 0, 0, C, 46, 46, 701, 294, Z\r\n";

    fn mods(list: &[(ModuleKind, &str)]) -> Vec<(ModuleKind, String)> {
        list.iter().map(|(k, n)| (*k, n.to_string())).collect()
    }

    #[test]
    fn parses_properties() {
        let p = ProjectProperties::parse(EXCEL);
        assert_eq!(
            p.id.as_deref(),
            Some("{8D807122-0657-42C8-BC6F-4B5FD08031C9}")
        );
        assert_eq!(p.name.as_deref(), Some("VBAProject"));
        assert_eq!(p.cmg.as_deref(), Some("DBD966ECE4F0E4F0E4F0E4F0"));
        assert_eq!(
            p.modules,
            mods(&[
                (ModuleKind::Document, "ThisWorkbook"),
                (ModuleKind::Document, "Sheet1"),
                (ModuleKind::Standard, "Module1"),
                (ModuleKind::Form, "UserForm1"),
            ])
        );
        assert_eq!(p.kind_of("module1"), Some(ModuleKind::Standard));
        assert_eq!(p.kind_of("nope"), None);
    }

    #[test]
    fn regenerating_with_the_same_modules_is_identity() {
        let p = ProjectProperties::parse(EXCEL);
        assert_eq!(regenerate_project_text(EXCEL, &p.modules, None), EXCEL);
    }

    #[test]
    fn regenerate_adds_removes_and_retypes() {
        let modules = mods(&[
            (ModuleKind::Document, "ThisWorkbook"),
            (ModuleKind::Document, "Sheet1"),
            (ModuleKind::Class, "Module1"),  // kind changed
            (ModuleKind::Standard, "Extra"), // new
        ]);
        let text = regenerate_project_text(EXCEL, &modules, Some("Renamed"));
        let lines = split_lines(&text);
        assert_eq!(lines[0], "ID=\"{8D807122-0657-42C8-BC6F-4B5FD08031C9}\"");
        assert_eq!(lines[1], "Document=ThisWorkbook/&H00000000");
        assert_eq!(lines[2], "Document=Sheet1/&H00000000");
        assert_eq!(lines[3], "Package={AC9F2F90-E877-11CE-9F68-00AA00574A4F}");
        assert_eq!(lines[4], "Class=Module1");
        assert_eq!(lines[5], "Module=Extra");
        assert_eq!(lines[6], "HelpFile=\"c:\\help.chm\"");
        assert_eq!(lines[7], "Name=\"Renamed\"");
        assert!(!text.contains("UserForm1"));
        assert!(text.contains("CMG=\"DBD966ECE4F0E4F0E4F0E4F0\"\r\nDPB=\"5557E86E18E919E919E9\"\r\nGC=\"CFCD72F0961011111111EE\"\r\n\r\n[Host Extender Info]\r\n"));
        assert!(text.contains("&H00000002={000209F2-0000-0000-C000-000000000046};Word8.0;&H00000000\r\n\r\n[Workspace]\r\n"));
        assert!(text.ends_with("[Workspace]\r\nThisWorkbook=0, 0, 0, 0, C\r\nSheet1=0, 0, 0, 0, C\r\nModule1=110, 145, 994, 721, \r\nExtra=0, 0, 0, 0, C\r\n"));
        assert_eq!(ProjectProperties::parse(&text).modules, modules);
    }

    #[test]
    fn new_project_text_is_well_formed() {
        let modules = mods(&[
            (ModuleKind::Document, "ThisWorkbook"),
            (ModuleKind::Document, "Sheet1"),
            (ModuleKind::Standard, "Module1"),
            (ModuleKind::Class, "Class1"),
        ]);
        let text = new_project_text("VBAProject", &modules);
        let p = ProjectProperties::parse(&text);
        assert_eq!(p.modules, modules);
        assert_eq!(p.name.as_deref(), Some("VBAProject"));
        let id = p.id.unwrap();
        assert_eq!(id.len(), 38);
        assert_eq!(
            crypto::decrypt(p.cmg.as_deref().unwrap()).unwrap(),
            vec![0, 0, 0, 0]
        );
        assert_eq!(crypto::decrypt(p.dpb.as_deref().unwrap()).unwrap(), vec![0]);
        assert_eq!(
            crypto::decrypt(p.gc.as_deref().unwrap()).unwrap(),
            vec![0xFF]
        );
        // The project key embedded in the values is the checksum of the id.
        let cmg = p.cmg.unwrap();
        let seed = u8::from_str_radix(&cmg[0..2], 16).unwrap();
        let key_enc = u8::from_str_radix(&cmg[4..6], 16).unwrap();
        assert_eq!(seed ^ key_enc, crypto::project_key(&id));
        assert!(text.contains("\r\n\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n\r\n[Workspace]\r\nThisWorkbook=0, 0, 0, 0, C\r\n"));
        assert!(text.ends_with("Class1=0, 0, 0, 0, C\r\n"));
        assert!(text.contains("\r\nDocument=Sheet1/&H00000000\r\nModule=Module1\r\nClass=Class1\r\nName=\"VBAProject\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\nCMG=\""));
    }

    #[test]
    fn project_wm_layout() {
        let wm = project_wm(&["A".to_string(), "あ".to_string()], 932).unwrap();
        assert_eq!(
            wm,
            vec![
                b'A', 0, b'A', 0, 0, 0, 0x82, 0xA0, 0, 0x42, 0x30, 0, 0, 0, 0
            ]
        );
    }
}
