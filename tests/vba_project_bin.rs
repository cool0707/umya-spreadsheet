//! `vba::read_project_bin` / `vba::write_project_bin` on raw
//! `vbaProject.bin` data.

mod vba_support;

use umya_spreadsheet::vba::{
    DocumentKind,
    DocumentSpec,
    ModuleKind,
    ProjectEdit,
    VbaError,
    compression::{
        compress,
        decompress,
    },
    read_project_bin,
    write_project_bin,
};
use vba_support::*;

const XLSXWRITER: &str = "xlsxwriter_vbaProject.bin";
const MAC: &str = "rust_xlsxwriter_mac_vbaProject.bin";
const WORD_FORMS: &str = "oletools_oleform-PR314_vbaProject.bin";

const THIS_WORKBOOK_HEADER: &str = "Attribute VB_Name = \"ThisWorkbook\"\r\nAttribute VB_Base = \"0{00020819-0000-0000-C000-000000000046}\"\r\nAttribute VB_GlobalNameSpace = False\r\nAttribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = True\r\nAttribute VB_Exposed = True\r\nAttribute VB_TemplateDerived = False\r\nAttribute VB_Customizable = True\r\n";

fn sheet_header(name: &str) -> String {
    format!(
        "Attribute VB_Name = \"{name}\"\r\nAttribute VB_Base = \"0{{00020820-0000-0000-C000-000000000046}}\"\r\nAttribute VB_GlobalNameSpace = False\r\nAttribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = True\r\nAttribute VB_Exposed = True\r\nAttribute VB_TemplateDerived = False\r\nAttribute VB_Customizable = True\r\n"
    )
}

fn docs(sheets: &[&str]) -> Vec<DocumentSpec> {
    let mut list = vec![DocumentSpec {
        code_name: "ThisWorkbook".into(),
        kind:      DocumentKind::Workbook,
    }];
    list.extend(sheets.iter().map(|s| DocumentSpec {
        code_name: (*s).to_string(),
        kind:      DocumentKind::Worksheet,
    }));
    list
}

fn kinds(info: &umya_spreadsheet::vba::VbaProjectInfo) -> Vec<(String, ModuleKind)> {
    info.modules
        .iter()
        .map(|m| (m.name.clone(), m.kind))
        .collect()
}

fn source_of<'a>(info: &'a umya_spreadsheet::vba::VbaProjectInfo, name: &str) -> &'a str {
    &info
        .modules
        .iter()
        .find(|m| m.name == name)
        .unwrap_or_else(|| panic!("module {name}"))
        .source
}

// ---------------------------------------------------------------------------
// Reading projects saved by Office (p-code in front of the source)
// ---------------------------------------------------------------------------

#[test]
fn reads_a_project_saved_by_excel() {
    let info = read_project_bin(&fixture_bytes(XLSXWRITER)).unwrap();
    assert_eq!(info.name, "VBAProject");
    assert_eq!(info.codepage, 1252);
    assert!(!info.is_protected);
    assert!(info.stomped_modules.is_empty());
    assert_eq!(
        kinds(&info),
        vec![
            ("ThisWorkbook".to_string(), ModuleKind::Document),
            ("Sheet1".to_string(), ModuleKind::Document),
            ("Module1".to_string(), ModuleKind::Standard),
            ("ThisWorkbook1".to_string(), ModuleKind::Document),
            ("Sheet2".to_string(), ModuleKind::Document),
        ]
    );
    assert_eq!(
        source_of(&info, "Module1"),
        "Attribute VB_Name = \"Module1\"\r\nSub say_hello()\r\n    MsgBox (\"Hello from Python!\")\r\nEnd Sub\r\n"
    );
    assert_eq!(source_of(&info, "ThisWorkbook"), THIS_WORKBOOK_HEADER);
    assert_eq!(source_of(&info, "Sheet2"), sheet_header("Sheet2"));
}

#[test]
fn reads_a_project_saved_by_excel_for_mac() {
    let info = read_project_bin(&fixture_bytes(MAC)).unwrap();
    assert_eq!(info.codepage, 10000);
    assert!(!info.is_protected);
    assert!(info.stomped_modules.is_empty());
    assert_eq!(info.modules.len(), 5);
    // Excel for Mac stores LF line endings; the text is returned as stored.
    assert_eq!(
        source_of(&info, "Module1"),
        "Attribute VB_Name = \"Module1\"\nSub say_hello()\n    MsgBox (\"Hello from Rust!\")\nEnd Sub\n"
    );
    assert_eq!(info.modules[2].kind, ModuleKind::Standard);
    assert_eq!(info.modules[0].kind, ModuleKind::Document);
}

#[test]
fn reads_a_project_with_user_forms() {
    let info = read_project_bin(&fixture_bytes(WORD_FORMS)).unwrap();
    assert_eq!(info.name, "Project");
    assert_eq!(
        kinds(&info),
        vec![
            ("ThisDocument".to_string(), ModuleKind::Document),
            ("UserFormTEST1".to_string(), ModuleKind::Form),
            ("UserFormTest2".to_string(), ModuleKind::Form),
            ("NewMacros".to_string(), ModuleKind::Standard),
        ]
    );
    assert!(
        source_of(&info, "UserFormTEST1")
            .starts_with("Attribute VB_Name = \"UserFormTEST1\"\r\nAttribute VB_Base = \"0{")
    );
    assert!(info.stomped_modules.is_empty());
}

#[test]
fn agrees_with_the_ovba_crate_on_genuine_projects() {
    for name in [XLSXWRITER, MAC, WORD_FORMS] {
        let bin = fixture_bytes(name);
        let ours: Vec<(String, String)> = read_project_bin(&bin)
            .unwrap()
            .modules
            .into_iter()
            .map(|m| (m.name, m.source))
            .collect();
        assert_eq!(ours, ovba_modules(&bin), "{name}");
    }
    // ... and for the project inside the signed workbook.
    let bin = zip_entry(
        &fixture("rust_xlsxwriter_macro04_signed.xlsm"),
        "xl/vbaProject.bin",
    )
    .unwrap();
    let ours: Vec<(String, String)> = read_project_bin(&bin)
        .unwrap()
        .modules
        .into_iter()
        .map(|m| (m.name, m.source))
        .collect();
    assert_eq!(ours, ovba_modules(&bin));
}

#[test]
fn malformed_input_is_an_error_not_a_panic() {
    assert!(read_project_bin(b"").is_err());
    assert!(read_project_bin(b"PK\x03\x04junk").is_err());
    let good = fixture_bytes(XLSXWRITER);
    assert!(read_project_bin(&good[..good.len() / 2]).is_err());
    assert!(read_project_bin(&good[..600]).is_err());
    // Flip bytes all over the file: any outcome is fine except a panic.
    for step in [7usize, 31, 101, 257] {
        let mut bad = good.clone();
        let mut i = step;
        while i < bad.len() {
            bad[i] ^= 0xA5;
            i += step * 13;
        }
        let _ = read_project_bin(&bad);
        let _ = write_project_bin(Some(&bad), &[], &ProjectEdit::default());
    }
    for offset in (0..good.len()).step_by(97) {
        let mut bad = good.clone();
        bad[offset] = bad[offset].wrapping_add(0x77);
        let _ = read_project_bin(&bad);
    }
}

// ---------------------------------------------------------------------------
// Creating a new project
// ---------------------------------------------------------------------------

const JP_MODULE: &str = "Option Explicit\r\n\r\n' 日本語のコメント（全角カッコ）\r\nPublic Sub あいさつ()\r\n    MsgBox \"こんにちは、世界！ ｶﾀｶﾅ 表示 ―～\"\r\nEnd Sub\r\n";

fn new_japanese_project() -> Vec<u8> {
    let edit = ProjectEdit {
        upsert: vec![
            standard("標準モジュール", JP_MODULE),
            class("Greeter", "Option Explicit\r\nPublic Name As String\r\n"),
        ],
        ..ProjectEdit::default()
    };
    write_project_bin(None, &docs(&["Sheet1", "Sheet2"]), &edit).unwrap()
}

#[test]
fn new_project_reads_back() {
    let bin = new_japanese_project();
    let info = read_project_bin(&bin).unwrap();
    assert_eq!(info.name, "VBAProject");
    assert_eq!(info.codepage, 932);
    assert!(!info.is_protected);
    assert!(info.stomped_modules.is_empty());
    assert_eq!(
        kinds(&info),
        vec![
            ("ThisWorkbook".to_string(), ModuleKind::Document),
            ("Sheet1".to_string(), ModuleKind::Document),
            ("Sheet2".to_string(), ModuleKind::Document),
            ("標準モジュール".to_string(), ModuleKind::Standard),
            ("Greeter".to_string(), ModuleKind::Class),
        ]
    );
    assert_eq!(source_of(&info, "ThisWorkbook"), THIS_WORKBOOK_HEADER);
    assert_eq!(source_of(&info, "Sheet2"), sheet_header("Sheet2"));
    assert_eq!(
        source_of(&info, "標準モジュール"),
        format!("Attribute VB_Name = \"標準モジュール\"\r\n{JP_MODULE}")
    );
    assert_eq!(
        source_of(&info, "Greeter"),
        "Attribute VB_Name = \"Greeter\"\r\nAttribute VB_Base = \"0{FCFB3D2A-A0FA-1068-A738-08002B3371B5}\"\r\nAttribute VB_GlobalNameSpace = False\r\nAttribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = False\r\nAttribute VB_Exposed = False\r\nAttribute VB_TemplateDerived = False\r\nAttribute VB_Customizable = False\r\nOption Explicit\r\nPublic Name As String\r\n"
    );
}

#[test]
fn new_project_is_readable_by_the_ovba_crate() {
    let bin = new_japanese_project();
    let ours: Vec<(String, String)> = read_project_bin(&bin)
        .unwrap()
        .modules
        .into_iter()
        .map(|m| (m.name, m.source))
        .collect();
    assert_eq!(ours, ovba_modules(&bin));

    let project = ovba::open_project(bin).unwrap();
    assert_eq!(project.information.code_page, 932);
    assert!(matches!(project.information.sys_kind, ovba::SysKind::Win32));
    assert_eq!(project.references.len(), 2);
    assert!(
        project
            .references
            .iter()
            .all(|r| matches!(r, ovba::Reference::Registered(_)))
    );
    for module in &project.modules {
        assert_eq!(module.text_offset, 0, "{}: no p-code", module.name);
        assert_eq!(module.stream_name, module.name);
        assert!(!module.read_only && !module.private);
    }
    let types: Vec<bool> = project
        .modules
        .iter()
        .map(|m| matches!(m.module_type, ovba::ModuleType::Procedural))
        .collect();
    assert_eq!(types, vec![false, false, false, true, false]);
}

#[test]
fn new_project_stream_layout() {
    let bin = new_japanese_project();
    let streams = cfb_streams(&bin);
    let names: Vec<&str> = streams.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "/PROJECT",
            "/PROJECTwm",
            "/VBA/Greeter",
            "/VBA/Sheet1",
            "/VBA/Sheet2",
            "/VBA/ThisWorkbook",
            "/VBA/_VBA_PROJECT",
            "/VBA/dir",
            "/VBA/標準モジュール",
        ]
    );
    // _VBA_PROJECT: Reserved1 0x61CC, Version 0xFFFF, Reserved2 0, Reserved3;
    // no cache.
    assert_eq!(
        cfb_stream(&bin, "/VBA/_VBA_PROJECT").unwrap(),
        [0xCC, 0x61, 0xFF, 0xFF, 0x00, 0x01, 0x00]
    );

    // Module stream = compressed source only, in code page 932.
    let module = decompress(&cfb_stream(&bin, "/VBA/標準モジュール").unwrap()).unwrap();
    let (expected, ..) = encoding_rs::SHIFT_JIS
        .encode("Attribute VB_Name = \"標準モジュール\"\r\nOption Explicit\r\n");
    assert!(module.starts_with(&expected));

    // PROJECT stream (MBCS text).
    let project_bytes = cfb_stream(&bin, "/PROJECT").unwrap();
    let (project, ..) = encoding_rs::SHIFT_JIS.decode(&project_bytes);
    let lines: Vec<&str> = project.split("\r\n").collect();
    assert!(lines[0].starts_with("ID=\"{") && lines[0].ends_with("}\"") && lines[0].len() == 43);
    assert_eq!(
        &lines[1..9],
        &[
            "Document=ThisWorkbook/&H00000000",
            "Document=Sheet1/&H00000000",
            "Document=Sheet2/&H00000000",
            "Module=標準モジュール",
            "Class=Greeter",
            "Name=\"VBAProject\"",
            "HelpContextID=\"0\"",
            "VersionCompatible32=\"393222000\"",
        ]
    );
    assert!(
        lines[9].starts_with("CMG=\"")
            && lines[10].starts_with("DPB=\"")
            && lines[11].starts_with("GC=\"")
    );
    assert_eq!(
        &lines[12..],
        &[
            "",
            "[Host Extender Info]",
            "&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000",
            "",
            "[Workspace]",
            "ThisWorkbook=0, 0, 0, 0, C",
            "Sheet1=0, 0, 0, 0, C",
            "Sheet2=0, 0, 0, 0, C",
            "標準モジュール=0, 0, 0, 0, C",
            "Greeter=0, 0, 0, 0, C",
            "",
        ]
    );

    // PROJECTwm: MBCS name, NUL, UTF-16LE name, NUL NUL ... final NUL NUL.
    let wm = cfb_stream(&bin, "/PROJECTwm").unwrap();
    let mut expected = Vec::new();
    for name in [
        "ThisWorkbook",
        "Sheet1",
        "Sheet2",
        "標準モジュール",
        "Greeter",
    ] {
        expected.extend_from_slice(&encoding_rs::SHIFT_JIS.encode(name).0);
        expected.push(0);
        expected.extend(name.encode_utf16().flat_map(|u| u.to_le_bytes()));
        expected.extend([0, 0]);
    }
    expected.extend([0, 0]);
    assert_eq!(wm, expected);

    // dir stream: record ids in the order of MS-OVBA 2.3.4.2.
    let dir = decompress(&cfb_stream(&bin, "/VBA/dir").unwrap()).unwrap();
    let mut ids = Vec::new();
    let mut pos = 0usize;
    while pos < dir.len() {
        let id = u16::from_le_bytes([dir[pos], dir[pos + 1]]);
        let size =
            u32::from_le_bytes([dir[pos + 2], dir[pos + 3], dir[pos + 4], dir[pos + 5]]) as usize;
        ids.push(id);
        pos += 6 + if id == 0x0009 { 6 } else { size };
    }
    assert_eq!(pos, dir.len());
    let module_ids = |type_id: u16| {
        vec![
            0x19, 0x47, 0x1A, 0x32, 0x1C, 0x48, 0x31, 0x1E, 0x2C, type_id, 0x2B,
        ]
    };
    let mut expected_ids = vec![
        0x01, 0x02, 0x14, 0x03, 0x04, 0x05, 0x40, 0x06, 0x3D, 0x07, 0x08, 0x09, 0x0C,
        0x3C, // information
        0x16, 0x3E, 0x0D, 0x16, 0x3E, 0x0D, // stdole, Office
        0x0F, 0x13,
    ];
    for type_id in [0x22, 0x22, 0x22, 0x21, 0x22] {
        expected_ids.extend(module_ids(type_id));
    }
    expected_ids.push(0x10);
    assert_eq!(ids, expected_ids);
}

#[test]
fn source_is_normalised_on_write() {
    let edit = ProjectEdit {
        upsert: vec![
            standard("Lf", "Sub A()\nEnd Sub"),
            standard(
                "Cr",
                "Attribute VB_Name = \"WrongName\"\rSub B()\rEnd Sub\r",
            ),
            class(
                "Exported",
                "VERSION 1.0 CLASS\r\nBEGIN\r\n  MultiUse = -1  'True\r\nEND\r\nAttribute VB_Name = \"Exported\"\r\nAttribute VB_GlobalNameSpace = False\r\nAttribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = True\r\nAttribute VB_Exposed = False\r\nOption Explicit\r\n",
            ),
        ],
        codepage: Some(1252),
        ..ProjectEdit::default()
    };
    let bin = write_project_bin(None, &docs(&["Sheet1"]), &edit).unwrap();
    let info = read_project_bin(&bin).unwrap();
    assert_eq!(info.codepage, 1252);
    assert_eq!(
        source_of(&info, "Lf"),
        "Attribute VB_Name = \"Lf\"\r\nSub A()\r\nEnd Sub\r\n"
    );
    assert_eq!(
        source_of(&info, "Cr"),
        "Attribute VB_Name = \"Cr\"\r\nSub B()\r\nEnd Sub\r\n"
    );
    // An exported .cls has no VB_Base; the stored header is completed the way
    // Excel writes it.
    assert_eq!(
        source_of(&info, "Exported"),
        "Attribute VB_Name = \"Exported\"\r\nAttribute VB_Base = \"0{FCFB3D2A-A0FA-1068-A738-08002B3371B5}\"\r\nAttribute VB_GlobalNameSpace = False\r\nAttribute VB_Creatable = False\r\nAttribute VB_PredeclaredId = True\r\nAttribute VB_Exposed = False\r\nAttribute VB_TemplateDerived = False\r\nAttribute VB_Customizable = False\r\nOption Explicit\r\n"
    );
}

#[test]
fn large_module_round_trips() {
    let mut body = String::from("Option Explicit\r\n");
    for i in 0..3000 {
        body.push_str(&format!("Public Function 関数{i}(ByVal x As Long) As Long\r\n    関数{i} = x * {i} ' 計算\r\nEnd Function\r\n"));
    }
    assert!(body.len() > 200_000);
    let edit = ProjectEdit {
        upsert: vec![standard("Big", &body)],
        ..ProjectEdit::default()
    };
    let bin = write_project_bin(None, &docs(&["Sheet1"]), &edit).unwrap();
    let info = read_project_bin(&bin).unwrap();
    assert_eq!(
        source_of(&info, "Big"),
        format!("Attribute VB_Name = \"Big\"\r\n{body}")
    );
    assert_eq!(
        ovba_modules(&bin)[2].1,
        format!("Attribute VB_Name = \"Big\"\r\n{body}")
    );
}

#[test]
fn unencodable_text_is_rejected() {
    let edit = ProjectEdit {
        upsert: vec![standard("M", "' emoji 😀\r\n")],
        ..ProjectEdit::default()
    };
    match write_project_bin(None, &docs(&[]), &edit) {
        Err(VbaError::Encoding(message)) => {
            assert!(
                message.contains("U+1F600") && message.contains("932") && message.contains("`M`"),
                "{message}"
            );
        }
        other => panic!("expected an encoding error, got {other:?}"),
    }
    let utf8 = ProjectEdit {
        codepage: Some(65001),
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(None, &docs(&[]), &utf8),
        Err(VbaError::Encoding(_))
    ));
    let unknown = ProjectEdit {
        codepage: Some(4242),
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(None, &docs(&[]), &unknown),
        Err(VbaError::Encoding(_))
    ));
}

#[test]
fn invalid_requests_are_rejected() {
    let upsert = |m| ProjectEdit {
        upsert: vec![m],
        ..ProjectEdit::default()
    };
    let d = docs(&["Sheet1"]);
    // New user forms cannot be created.
    assert!(matches!(
        write_project_bin(None, &d, &upsert(module(ModuleKind::Form, "UserForm1", ""))),
        Err(VbaError::UnsupportedFormat(_))
    ));
    // Document modules only for known code names.
    match write_project_bin(
        None,
        &d,
        &upsert(module(ModuleKind::Document, "Sheet9", "")),
    ) {
        Err(VbaError::UnknownCodeName(name)) => assert_eq!(name, "Sheet9"),
        other => panic!("{other:?}"),
    }
    // Bad names.
    for name in ["", "1st", "has space", "dir", "a-b", &"x".repeat(32)] {
        assert!(
            matches!(
                write_project_bin(None, &d, &upsert(standard(name, ""))),
                Err(VbaError::InvalidRequest(_))
            ),
            "{name}"
        );
    }
    // Duplicates (module names are case-insensitive).
    let twice = ProjectEdit {
        upsert: vec![standard("Dup", ""), standard("DUP", "")],
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(None, &d, &twice),
        Err(VbaError::InvalidRequest(_))
    ));
    // A standard module cannot take the name of a document module.
    assert!(matches!(
        write_project_bin(None, &d, &upsert(standard("Sheet1", ""))),
        Err(VbaError::InvalidRequest(_))
    ));
    // Documents cannot be removed.
    let bin = write_project_bin(None, &d, &ProjectEdit::default()).unwrap();
    let remove = ProjectEdit {
        remove: vec!["sheet1".into()],
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(Some(&bin), &d, &remove),
        Err(VbaError::InvalidRequest(_))
    ));
    // Removing something that does not exist is not an error.
    let remove = ProjectEdit {
        remove: vec!["Nope".into()],
        ..ProjectEdit::default()
    };
    assert!(write_project_bin(Some(&bin), &d, &remove).is_ok());
}

// ---------------------------------------------------------------------------
// Rewriting an existing project
// ---------------------------------------------------------------------------

#[test]
fn rewriting_an_excel_project_drops_pcode_and_keeps_sources() {
    let original = fixture_bytes(XLSXWRITER);
    let before = read_project_bin(&original).unwrap();
    let rewritten = write_project_bin(Some(&original), &[], &ProjectEdit::default()).unwrap();
    let after = read_project_bin(&rewritten).unwrap();
    assert_eq!(before.modules, after.modules);
    assert_eq!(
        (&before.name, before.codepage),
        (&after.name, after.codepage)
    );

    let streams = cfb_streams(&rewritten);
    assert!(streams.iter().all(|(path, _)| !path.contains("__SRP_")));
    assert_eq!(
        cfb_stream(&rewritten, "/VBA/_VBA_PROJECT").unwrap().len(),
        7
    );
    // Module streams are now exactly the compressed source.
    let module1 = cfb_stream(&rewritten, "/VBA/Module1").unwrap();
    assert_eq!(module1, compress(source_of(&before, "Module1").as_bytes()));
    assert!(rewritten.len() < original.len());

    // The independent reader agrees and sees MODULEOFFSET = 0 everywhere.
    let project = ovba::open_project(rewritten.clone()).unwrap();
    assert!(project.modules.iter().all(|m| m.text_offset == 0));
    assert_eq!(project.references.len(), 2);
    let ours: Vec<(String, String)> = after
        .modules
        .into_iter()
        .map(|m| (m.name, m.source))
        .collect();
    assert_eq!(ours, ovba_modules(&rewritten));
}

#[test]
fn project_stream_properties_are_preserved() {
    let original = fixture_bytes(XLSXWRITER);
    let edit = ProjectEdit {
        upsert: vec![
            standard("Added", "Sub X()\r\nEnd Sub\r\n"),
            class("Thing", ""),
        ],
        remove: vec!["Module1".into()],
        ..ProjectEdit::default()
    };
    let rewritten = write_project_bin(Some(&original), &[], &edit).unwrap();
    let text = String::from_utf8(cfb_stream(&rewritten, "/PROJECT").unwrap()).unwrap();
    assert_eq!(
        text,
        "ID=\"{8D807122-0657-42C8-BC6F-4B5FD08031C9}\"\r\nDocument=ThisWorkbook/&H00000000\r\nDocument=Sheet1/&H00000000\r\nDocument=ThisWorkbook1/&H00000000\r\nDocument=Sheet2/&H00000000\r\nModule=Added\r\nClass=Thing\r\nName=\"VBAProject\"\r\nHelpContextID=\"0\"\r\nVersionCompatible32=\"393222000\"\r\nCMG=\"DBD966ECE4F0E4F0E4F0E4F0\"\r\nDPB=\"5557E86E18E919E919E9\"\r\nGC=\"CFCD72F0961011111111EE\"\r\n\r\n[Host Extender Info]\r\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\r\n\r\n[Workspace]\r\nThisWorkbook=0, 0, 0, 0, C\r\nSheet1=0, 0, 0, 0, C\r\nThisWorkbook1=0, 0, 0, 0, C\r\nSheet2=0, 0, 0, 0, C\r\nAdded=0, 0, 0, 0, C\r\nThing=0, 0, 0, 0, C\r\n"
    );
    let info = read_project_bin(&rewritten).unwrap();
    assert_eq!(
        kinds(&info),
        vec![
            ("ThisWorkbook".to_string(), ModuleKind::Document),
            ("Sheet1".to_string(), ModuleKind::Document),
            ("ThisWorkbook1".to_string(), ModuleKind::Document),
            ("Sheet2".to_string(), ModuleKind::Document),
            ("Added".to_string(), ModuleKind::Standard),
            ("Thing".to_string(), ModuleKind::Class),
        ]
    );
    assert!(cfb_stream(&rewritten, "/VBA/Module1").is_none());

    // The reference records are byte-identical to the original ones.
    let references = |bin: &[u8]| {
        let dir = decompress(&cfb_stream(bin, "/VBA/dir").unwrap()).unwrap();
        let start = dir
            .windows(8)
            .position(|w| w == [0x16, 0, 6, 0, 0, 0, b's', b't'])
            .unwrap();
        let end = dir
            .windows(8)
            .position(|w| w[..6] == [0x0F, 0, 2, 0, 0, 0] && w[6..] != [0, 0])
            .unwrap();
        dir[start..end].to_vec()
    };
    assert_eq!(references(&original), references(&rewritten));
}

#[test]
fn user_forms_and_other_storages_are_preserved_byte_for_byte() {
    let original = fixture_bytes(WORD_FORMS);
    let edit = ProjectEdit {
        upsert: vec![
            standard("NewMacros", "Sub Changed()\r\nEnd Sub\r\n"),
            // Replacing the code of an existing form is allowed.
            module(
                ModuleKind::Form,
                "UserFormTest2",
                "Private Sub UserForm_Click()\r\n    Beep\r\nEnd Sub\r\n",
            ),
            class("Fresh", "Public V As Variant\r\n"),
        ],
        project_name: Some("Renamed".into()),
        ..ProjectEdit::default()
    };
    let rewritten = write_project_bin(Some(&original), &[], &edit).unwrap();

    let before = cfb_streams(&original);
    let after = cfb_streams(&rewritten);
    let mut designer_streams = 0;
    for (path, data) in &before {
        if path.starts_with("/UserFormTEST1/") || path.starts_with("/UserFormTest2/") {
            designer_streams += 1;
            let same = after.iter().find(|(p, _)| p == path).map(|(_, d)| d);
            assert_eq!(same, Some(data), "{path}");
        }
    }
    assert_eq!(designer_streams, 21);
    // Storages keep their CLSIDs.
    assert_eq!(cfb_storages(&original), cfb_storages(&rewritten));

    let old = read_project_bin(&original).unwrap();
    let new = read_project_bin(&rewritten).unwrap();
    assert_eq!(new.name, "Renamed");
    assert_eq!(
        kinds(&new),
        vec![
            ("ThisDocument".to_string(), ModuleKind::Document),
            ("UserFormTEST1".to_string(), ModuleKind::Form),
            ("UserFormTest2".to_string(), ModuleKind::Form),
            ("NewMacros".to_string(), ModuleKind::Standard),
            ("Fresh".to_string(), ModuleKind::Class),
        ]
    );
    // Untouched modules keep their text exactly.
    assert_eq!(
        source_of(&old, "ThisDocument"),
        source_of(&new, "ThisDocument")
    );
    assert_eq!(
        source_of(&old, "UserFormTEST1"),
        source_of(&new, "UserFormTEST1")
    );
    assert_eq!(
        source_of(&new, "NewMacros"),
        "Attribute VB_Name = \"NewMacros\"\r\nSub Changed()\r\nEnd Sub\r\n"
    );
    // The form keeps its attribute header (VB_Base etc.), only the code
    // changes.
    let old_form = source_of(&old, "UserFormTest2");
    let header_end = old_form
        .find("Attribute VB_Customizable = False\r\n")
        .unwrap()
        + "Attribute VB_Customizable = False\r\n".len();
    assert_eq!(
        source_of(&new, "UserFormTest2"),
        format!(
            "{}Private Sub UserForm_Click()\r\n    Beep\r\nEnd Sub\r\n",
            &old_form[..header_end]
        )
    );

    // PROJECT: BaseClass lines and designer window records survive.
    let text = String::from_utf8(cfb_stream(&rewritten, "/PROJECT").unwrap()).unwrap();
    assert!(text.starts_with("ID=\"{9C293C51-EECF-41EF-AB5E-B5F515179A31}\"\r\nDocument=ThisDocument/&H00000000\r\nBaseClass=UserFormTEST1\r\nBaseClass=UserFormTest2\r\nModule=NewMacros\r\nClass=Fresh\r\nName=\"Renamed\"\r\n"));
    assert!(text.contains("UserFormTEST1=260, 260, 1391, 890, C, 104, 104, 1209, 759, \r\n"));
    assert!(text.contains("CMG=\"3E3C332FFFF361F761F761F761F7\"\r\nDPB=\"A4A6A9D5693A6A3A6A3A\"\r\nGC=\"0A0807736C746C7493\"\r\n"));

    // All four reference record kinds survive and the ovba crate can still
    // parse the dir stream.
    let project = ovba::open_project(rewritten.clone()).unwrap();
    let same_references = ovba::open_project(original.clone())
        .unwrap()
        .references
        .len();
    assert_eq!(project.references.len(), same_references);
    assert_eq!(project.references.len(), 5);
    assert!(matches!(
        project.references[0],
        ovba::Reference::Registered(_)
    ));
    assert!(matches!(project.references[1], ovba::Reference::Project(_)));
    assert!(matches!(
        project.references[3],
        ovba::Reference::Original(_)
    ));
    assert!(matches!(project.references[4], ovba::Reference::Control(_)));
    let ours: Vec<(String, String)> = new
        .modules
        .into_iter()
        .map(|m| (m.name, m.source))
        .collect();
    assert_eq!(ours, ovba_modules(&rewritten));

    // Removing a form is refused.
    let remove = ProjectEdit {
        remove: vec!["UserFormTEST1".into()],
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(Some(&original), &[], &remove),
        Err(VbaError::InvalidRequest(_))
    ));
}

#[test]
fn update_remove_and_kind_change() {
    let d = docs(&["Sheet1"]);
    let first = ProjectEdit {
        upsert: vec![
            standard("A", "Sub A1()\r\nEnd Sub\r\n"),
            standard("B", "' b\r\n"),
            class("C", "' c\r\n"),
        ],
        ..ProjectEdit::default()
    };
    let bin = write_project_bin(None, &d, &first).unwrap();
    let second = ProjectEdit {
        upsert: vec![
            standard("a", "Sub A2()\r\nEnd Sub\r\n"), // same module, new case
            class("B", "' now a class\r\n"),          // standard -> class
        ],
        remove: vec!["c".into()],
        ..ProjectEdit::default()
    };
    let bin2 = write_project_bin(Some(&bin), &d, &second).unwrap();
    let info = read_project_bin(&bin2).unwrap();
    assert_eq!(
        kinds(&info),
        vec![
            ("ThisWorkbook".to_string(), ModuleKind::Document),
            ("Sheet1".to_string(), ModuleKind::Document),
            ("a".to_string(), ModuleKind::Standard),
            ("B".to_string(), ModuleKind::Class),
        ]
    );
    assert_eq!(
        source_of(&info, "a"),
        "Attribute VB_Name = \"a\"\r\nSub A2()\r\nEnd Sub\r\n"
    );
    assert!(source_of(&info, "B").contains("Attribute VB_PredeclaredId = False\r\n"));
    // The project id survives the rewrite.
    let id = |bin: &[u8]| {
        String::from_utf8(cfb_stream(bin, "/PROJECT").unwrap())
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_string()
    };
    assert_eq!(id(&bin), id(&bin2));
    // Document <-> standard changes are refused.
    let bad = ProjectEdit {
        upsert: vec![module(ModuleKind::Document, "a", "")],
        ..ProjectEdit::default()
    };
    assert!(write_project_bin(Some(&bin2), &d, &bad).is_err());
}

#[test]
fn missing_document_modules_are_added_to_an_existing_project() {
    let bin = write_project_bin(None, &docs(&["Sheet1"]), &ProjectEdit::default()).unwrap();
    let bin2 = write_project_bin(
        Some(&bin),
        &docs(&["Sheet1", "Sheet2"]),
        &ProjectEdit::default(),
    )
    .unwrap();
    let info = read_project_bin(&bin2).unwrap();
    assert_eq!(info.modules.len(), 3);
    assert_eq!(info.modules[2].name, "Sheet2");
    assert_eq!(info.modules[2].kind, ModuleKind::Document);
    assert_eq!(info.modules[2].source, sheet_header("Sheet2"));
}

#[test]
fn code_page_change_re_encodes_the_whole_project() {
    // (Half-width katakana, used by the other tests, do not exist in GBK.)
    let edit = ProjectEdit {
        upsert: vec![
            standard(
                "標準モジュール",
                "Sub あいさつ()\r\n    MsgBox \"こんにちは、世界！\"\r\nEnd Sub\r\n",
            ),
            class("Greeter", "Option Explicit\r\n"),
        ],
        ..ProjectEdit::default()
    };
    let bin = write_project_bin(None, &docs(&["Sheet1", "Sheet2"]), &edit).unwrap();
    assert!(matches!(
        write_project_bin(
            Some(&new_japanese_project()),
            &[],
            &ProjectEdit {
                codepage: Some(936),
                ..ProjectEdit::default()
            }
        ),
        Err(VbaError::Encoding(_))
    ));
    // 932 -> 936 (GBK can represent kana and these kanji).
    let to_gbk = ProjectEdit {
        codepage: Some(936),
        ..ProjectEdit::default()
    };
    let gbk = write_project_bin(Some(&bin), &[], &to_gbk).unwrap();
    let before = read_project_bin(&bin).unwrap();
    let after = read_project_bin(&gbk).unwrap();
    assert_eq!(after.codepage, 936);
    // Same text, different bytes.
    assert_eq!(before.modules, after.modules);
    assert_eq!(after.modules[3].name, "標準モジュール");
    let raw_932 = decompress(&cfb_stream(&bin, "/VBA/標準モジュール").unwrap()).unwrap();
    let raw_936 = decompress(&cfb_stream(&gbk, "/VBA/標準モジュール").unwrap()).unwrap();
    assert_ne!(raw_932, raw_936);
    assert_eq!(ovba::open_project(gbk).unwrap().information.code_page, 936);

    // 932 -> 1252 cannot represent the Japanese text.
    let to_latin = ProjectEdit {
        codepage: Some(1252),
        ..ProjectEdit::default()
    };
    assert!(matches!(
        write_project_bin(Some(&bin), &[], &to_latin),
        Err(VbaError::Encoding(_))
    ));

    // The Mac project (code page 10000, ASCII only) converts to 1252.
    let mac = fixture_bytes(MAC);
    let converted = write_project_bin(Some(&mac), &[], &to_latin).unwrap();
    let info = read_project_bin(&converted).unwrap();
    assert_eq!(info.codepage, 1252);
    assert_eq!(info.modules, read_project_bin(&mac).unwrap().modules);
}

// ---------------------------------------------------------------------------
// VBA stomping
// ---------------------------------------------------------------------------

#[test]
fn stomped_source_is_reported() {
    let original = fixture_bytes(XLSXWRITER);
    // Replace the compressed source of Module1 but keep its p-code: the
    // classic "VBA stomping" manipulation. MODULEOFFSET of Module1 is 955.
    let stream = cfb_stream(&original, "/VBA/Module1").unwrap();
    let offset = 955;
    assert_eq!(stream[offset], 0x01);
    let mut stomped_stream = stream[..offset].to_vec();
    stomped_stream.extend(compress(
        b"Attribute VB_Name = \"Module1\"\r\n' nothing to see here\r\n",
    ));

    let mut file = cfb::CompoundFile::open(std::io::Cursor::new(original.clone())).unwrap();
    {
        use std::io::Write;
        let mut s = file.create_stream("/VBA/Module1").unwrap();
        s.write_all(&stomped_stream).unwrap();
    }
    file.flush().unwrap();
    let stomped = file.into_inner().into_inner();

    let info = read_project_bin(&stomped).unwrap();
    assert_eq!(info.stomped_modules, vec!["Module1".to_string()]);
    // The source text is what is returned.
    assert_eq!(
        source_of(&info, "Module1"),
        "Attribute VB_Name = \"Module1\"\r\n' nothing to see here\r\n"
    );
    // Once rewritten without p-code there is nothing left to disagree.
    let clean = write_project_bin(Some(&stomped), &[], &ProjectEdit::default()).unwrap();
    assert!(read_project_bin(&clean).unwrap().stomped_modules.is_empty());
}
