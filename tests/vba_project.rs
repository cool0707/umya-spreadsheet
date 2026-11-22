//! `vba::VbaProject`: the project of a workbook read with `reader::xlsx`,
//! projects kept in a storage (`.xls`), and the module lookups.

mod vba_support;

use std::io::{
    Cursor,
    Write,
};

use umya_spreadsheet::{
    reader,
    vba::{
        DocumentKind,
        DocumentSpec,
        ModuleKind,
        ProjectEdit,
        VbaError,
        VbaProject,
        build_project,
        write_project_bin,
    },
    writer,
};
use vba_support::*;

fn aaa_xlsm() -> umya_spreadsheet::Workbook {
    reader::xlsx::read(std::path::Path::new("./tests/test_files/aaa.xlsm")).unwrap()
}

#[test]
fn reads_the_project_of_a_workbook() {
    let book = aaa_xlsm();
    let project = VbaProject::read(book.macros_code().unwrap()).unwrap();
    assert_eq!(project.name(), "VBAProject");
    assert_eq!(project.codepage(), 932);
    assert!(!project.is_protected());
    assert!(project.stomped_modules().is_empty());
    let modules: Vec<(&str, ModuleKind)> = project
        .modules()
        .iter()
        .map(|m| (m.name(), m.kind()))
        .collect();
    assert_eq!(
        modules,
        [
            ("ThisWorkbook", ModuleKind::Document),
            ("Sheet1", ModuleKind::Document),
            ("Sheet2", ModuleKind::Document),
            ("Sheet3", ModuleKind::Document),
            ("Module1", ModuleKind::Standard),
        ]
    );
    // Module names compare case-insensitively.
    let module = project.module("module1").unwrap();
    assert_eq!(module.name(), "Module1");
    assert!(
        module
            .source()
            .starts_with("Attribute VB_Name = \"Module1\"\r\nSub Macro1()\r\n")
    );
    assert!(project.module("Module2").is_none());
    assert_eq!(project.info().modules.len(), 5);
}

#[test]
fn a_rewritten_project_is_saved_with_the_workbook() {
    let mut book = aaa_xlsm();
    let project = VbaProject::read(book.macros_code().unwrap()).unwrap();
    let edit = ProjectEdit {
        upsert: vec![standard("Added", "Sub Added()\r\nEnd Sub\r\n")],
        ..ProjectEdit::default()
    };
    let built = build_project(Some(&project), &[], &edit).unwrap();
    assert_eq!(built.modules.len(), 6);
    book.set_macros_code(built.bytes.clone());

    let mut saved = Cursor::new(Vec::new());
    writer::xlsx::write_writer(&book, &mut saved).unwrap();
    let back = reader::xlsx::read_reader(Cursor::new(saved.into_inner()), true).unwrap();
    assert_eq!(back.macros_code().unwrap(), built.bytes.as_slice());
    let project = VbaProject::read(back.macros_code().unwrap()).unwrap();
    assert_eq!(
        project.module("Added").unwrap().source(),
        "Attribute VB_Name = \"Added\"\r\nSub Added()\r\nEnd Sub\r\n"
    );
    assert_eq!(project.modules().len(), 6);
}

/// A compound file with the streams and storages of `bin` below the storage
/// `storage`, next to a `Workbook` stream (the layout of an `.xls` file).
fn nest(bin: &[u8], storage: &str) -> Vec<u8> {
    let mut file = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    file.create_stream("/Workbook")
        .unwrap()
        .write_all(b"not a project")
        .unwrap();
    file.create_storage(format!("/{storage}")).unwrap();
    for (path, _) in cfb_storages(bin) {
        if path != "/" {
            file.create_storage(format!("/{storage}{path}")).unwrap();
        }
    }
    for (path, data) in cfb_streams(bin) {
        file.create_stream(format!("/{storage}{path}"))
            .unwrap()
            .write_all(&data)
            .unwrap();
    }
    file.flush().unwrap();
    file.into_inner().into_inner()
}

fn new_project() -> Vec<u8> {
    let documents = [
        DocumentSpec {
            code_name: "ThisWorkbook".into(),
            kind:      DocumentKind::Workbook,
        },
        DocumentSpec {
            code_name: "Sheet1".into(),
            kind:      DocumentKind::Worksheet,
        },
    ];
    let edit = ProjectEdit {
        upsert: vec![
            standard("Module1", "Sub A()\r\n    MsgBox \"あ\"\r\nEnd Sub\r\n"),
            class("Class1", "Public X As Long\r\n"),
        ],
        ..ProjectEdit::default()
    };
    write_project_bin(None, &documents, &edit).unwrap()
}

#[test]
fn reads_a_project_kept_in_a_storage() {
    let bin = new_project();
    let xls = nest(&bin, "_VBA_PROJECT_CUR");
    let project = VbaProject::read_storage(&xls, "_VBA_PROJECT_CUR").unwrap();
    assert_eq!(project.info(), VbaProject::read(&bin).unwrap().info());
    // Names compare case-insensitively; `/` separates them.
    let lower = VbaProject::read_storage(&xls, "/_vba_project_cur").unwrap();
    assert_eq!(lower.info(), project.info());
    // An empty path is the root.
    assert_eq!(
        VbaProject::read_storage(&bin, "").unwrap().info(),
        project.info()
    );

    // A deeper storage.
    let doc = nest(&nest(&bin, "Macros"), "Outer");
    let project = VbaProject::read_storage(&doc, "Outer/Macros").unwrap();
    assert_eq!(project.modules().len(), 4);

    // The root of the file is not a project, and neither is a missing storage
    // or a stream.
    assert!(matches!(VbaProject::read(&xls), Err(VbaError::Format(_))));
    assert!(matches!(
        VbaProject::read_storage(&xls, "Macros"),
        Err(VbaError::Format(_))
    ));
    assert!(VbaProject::read_storage(&xls, "Workbook").is_err());

    // The project read from a storage can be written as a vbaProject.bin.
    let project = VbaProject::read_storage(&xls, "_VBA_PROJECT_CUR").unwrap();
    let built = build_project(Some(&project), &[], &ProjectEdit::default()).unwrap();
    assert_eq!(
        VbaProject::read(&built.bytes).unwrap().info().modules,
        project.info().modules
    );
}

#[test]
fn a_missing_module_stream_is_reported() {
    let bin = new_project();
    let mut file = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
    for (path, _) in cfb_storages(&bin) {
        if path != "/" {
            file.create_storage(&path).unwrap();
        }
    }
    for (path, data) in cfb_streams(&bin) {
        if path != "/VBA/Class1" {
            file.create_stream(&path).unwrap().write_all(&data).unwrap();
        }
    }
    file.flush().unwrap();
    let broken = file.into_inner().into_inner();
    match VbaProject::read(&broken) {
        Err(VbaError::MissingModuleStream { module, stream }) => {
            assert_eq!((module.as_str(), stream.as_str()), ("Class1", "Class1"));
        }
        other => panic!("{other:?}"),
    }
}

/// The signature of a VBA project (`xl/vbaProjectSignature.bin`, the
/// relationship of `xl/vbaProject.bin`) is kept while the project is not
/// replaced: it was dropped, and Excel then took the macros for unsigned.
#[test]
fn the_signature_of_the_project_is_kept() {
    use std::io::{Cursor, Read};
    let path = "./tests/test_files/vba/rust_xlsxwriter_macro04_signed.xlsm";
    let original = std::fs::read(path).unwrap();
    let part = |file: &[u8], name: &str| -> Option<Vec<u8>> {
        let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
        let mut data = Vec::new();
        archive.by_name(name).ok()?.read_to_end(&mut data).ok()?;
        Some(data)
    };
    let write = |book: &umya_spreadsheet::Workbook| {
        let mut out = Vec::new();
        umya_spreadsheet::writer::xlsx::write_writer(book, &mut out).unwrap();
        out
    };
    let mut book = umya_spreadsheet::reader::xlsx::read_reader(Cursor::new(&original), true).unwrap();
    assert!(book.has_vba_signature());
    let saved = write(&book);
    assert_eq!(part(&saved, "xl/vbaProjectSignature.bin"), part(&original, "xl/vbaProjectSignature.bin"));
    let rels = String::from_utf8(part(&saved, "xl/_rels/vbaProject.bin.rels").unwrap()).unwrap();
    assert!(rels.contains("relationships/vbaProjectSignature") && rels.contains(r#"Target="vbaProjectSignature.bin""#), "{rels}");
    let types = String::from_utf8(part(&saved, "[Content_Types].xml").unwrap()).unwrap();
    assert!(types.contains(r#"<Override PartName="/xl/vbaProjectSignature.bin" ContentType="application/vnd.ms-office.vbaProjectSignature"/>"#), "{types}");

    // A project of its own: the signature does not sign it.
    let project = book.macros_code().unwrap().to_vec();
    book.set_macros_code(project);
    assert!(!book.has_vba_signature());
    let saved = write(&book);
    assert!(part(&saved, "xl/vbaProjectSignature.bin").is_none());
    assert!(part(&saved, "xl/_rels/vbaProject.bin.rels").is_none());
}
