//! Defined names edited through the model: their attributes (the comment,
//! a macro registered as a name), and adding, finding and removing them in a
//! scope (the workbook, or a sheet), also when sheets are removed.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    Workbook,
    XlsxError,
    is_valid_defined_name,
    reader,
    writer,
};

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn read(bytes: &[u8]) -> Workbook {
    reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap()
}

fn workbook_xml(file: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut data = String::new();
    archive.by_name("xl/workbook.xml").unwrap().read_to_string(&mut data).unwrap();
    data
}

/// A workbook with the sheets 一覧, 集計, 予備.
fn three_sheets() -> Workbook {
    let mut book = umya_spreadsheet::new_file();
    book.set_sheet_name(0, "一覧").unwrap();
    book.new_sheet("集計").unwrap();
    book.new_sheet("予備").unwrap();
    book
}

#[test]
fn names_are_added_found_and_removed_in_their_scope() {
    let mut book = three_sheets();
    book.add_defined_name("売上", "一覧!$B$2:$B$100", None).unwrap();
    book.add_defined_name("税率", "0.1", None).unwrap();
    // The same name local to a sheet.
    book.add_defined_name("売上", "集計!$C$2", Some("集計")).unwrap();
    assert!(matches!(book.add_defined_name("売上", "一覧!$A$1", None), Err(XlsxError::NameDuplicate())));
    assert!(book.add_defined_name("SALES", "一覧!$A$1", None).is_ok());
    assert!(matches!(book.add_defined_name("sales", "一覧!$A$1", None), Err(XlsxError::NameDuplicate())));
    assert!(matches!(book.add_defined_name("A1", "一覧!$A$1", None), Err(XlsxError::InvalidName(_))));
    assert!(matches!(book.add_defined_name("x", "1", Some("ない")), Err(XlsxError::NotFound())));

    // (The model writes the names of sheets in quotes.)
    assert_eq!(book.defined_name("売上", None).unwrap().address(), "'一覧'!$B$2:$B$100");
    assert_eq!(book.defined_name("売上", Some("集計")).unwrap().address(), "'集計'!$C$2");
    assert!(book.defined_name("売上", Some("予備")).is_none());
    let all = book.all_defined_names();
    assert_eq!(all.len(), 4);
    assert!(all.iter().any(|(scope, d)| *scope == Some("集計") && d.name() == "売上"));

    // Changed and saved.
    book.defined_name_mut("税率", None).unwrap().set_address("0.08").set_comment(Some("消費税の軽減税率"));
    let saved = write(&book);
    let xml = workbook_xml(&saved);
    assert!(xml.contains(r#"<definedName name="売上" localSheetId="1">'集計'!$C$2</definedName>"#), "{xml}");
    assert!(xml.contains(r#"<definedName name="税率" comment="消費税の軽減税率">0.08</definedName>"#), "{xml}");

    let mut book = read(&saved);
    assert_eq!(book.defined_name("税率", None).unwrap().comment(), Some("消費税の軽減税率"));
    assert_eq!(book.defined_name("売上", Some("集計")).unwrap().address(), "'集計'!$C$2");
    assert!(book.remove_defined_name("売上", Some("集計")));
    assert!(!book.remove_defined_name("売上", Some("集計")));
    assert!(book.defined_name("売上", None).is_some());
    assert!(book.remove_defined_name("売上", None));
    assert!(!workbook_xml(&write(&book)).contains("売上"));
}

#[test]
fn attributes_of_names() {
    let mut book = umya_spreadsheet::new_file();
    let d = book.add_defined_name("集計する", "Sheet1!$A$1", None).unwrap();
    d.set_function(true)
        .set_vb_procedure(true)
        .set_function_group_id(Some(14))
        .set_shortcut_key(Some("S"))
        .set_description(Some("合計を出す"))
        .set_help(Some("help.chm!1"))
        .set_status_bar(Some("集計中"))
        .set_custom_menu(Some("集計(&S)"))
        .set_publish_to_server(true)
        .set_workbook_parameter(true)
        .set_xlm(true);
    let saved = write(&book);
    let book = read(&saved);
    let d = book.defined_name("集計する", None).unwrap();
    assert!(d.function() && d.vb_procedure() && d.xlm() && d.publish_to_server() && d.workbook_parameter());
    assert_eq!(d.function_group_id(), Some(14));
    assert_eq!(
        (d.shortcut_key(), d.description(), d.help(), d.status_bar(), d.custom_menu()),
        (Some("S"), Some("合計を出す"), Some("help.chm!1"), Some("集計中"), Some("集計(&S)"))
    );

    // Turned off: not written (as Excel leaves them out).
    let mut book = book;
    book.defined_name_mut("集計する", None).unwrap().set_function(false).set_comment(Some("")).set_description(None);
    let xml = workbook_xml(&write(&book));
    assert!(!xml.contains("function=") && !xml.contains("description=") && !xml.contains("comment="), "{xml}");

    // Written by LibreOffice ("false"): read as off, written back as read.
    let book = read(&std::fs::read("./tests/test_files/libre.xlsm").unwrap());
    let d = book.all_defined_names().into_iter().find(|(_, d)| d.name() == "COL_TEST").unwrap().1;
    assert!(!d.function() && !d.vb_procedure());
    assert_eq!(d.attribute("function"), Some("false"));
}

#[test]
fn names_of_removed_sheets() {
    let mut book = three_sheets();
    book.add_defined_name("一覧の名前", "一覧!$A$1", Some("一覧")).unwrap();
    book.add_defined_name("集計の名前", "集計!$A$1", Some("集計")).unwrap();
    book.add_defined_name("予備の名前", "予備!$A$1", Some("予備")).unwrap();
    book.add_defined_name("全体", "集計!$B$2", None).unwrap();
    book.add_defined_name("二つ", "SUM(集計!$A$1,一覧!$A$1)", None).unwrap();
    // (Read again: the workbook's names that refer to a sheet are kept with it.)
    let mut book = read(&write(&book));
    book.remove_sheet_by_name("集計").unwrap();

    // Local to it: gone. Local to the sheet after it: still that sheet's.
    assert!(book.defined_name("集計の名前", Some("集計")).is_none());
    assert_eq!(book.defined_name("予備の名前", Some("予備")).unwrap().local_sheet_id(), 1);
    assert_eq!(book.defined_name("一覧の名前", Some("一覧")).unwrap().local_sheet_id(), 0);
    // The workbook's: what referred to it is #REF!.
    assert_eq!(book.defined_name("全体", None).unwrap().address(), "#REF!$B$2");
    assert_eq!(book.defined_name("二つ", None).unwrap().address(), "SUM(#REF!$A$1,一覧!$A$1)");
    let xml = workbook_xml(&write(&book));
    assert!(xml.contains(r#"<definedName name="予備の名前" localSheetId="1">'予備'!$A$1</definedName>"#), "{xml}");
    assert!(xml.contains(r#"<definedName name="全体">#REF!$B$2</definedName>"#), "{xml}");
}

#[test]
fn names_excel_takes() {
    for ok in ["売上", "_x", "\\path", "Rate", "A1B", "XFE1", "名前.1", "Q1_2026"] {
        assert!(is_valid_defined_name(ok), "{ok}");
    }
    for bad in ["", "1st", "A1", "XFD1048576", "R", "c", "R1C1", "R1C", "RC", "R2", "C3", "a b", "a-b", "a!b"] {
        assert!(!is_valid_defined_name(bad), "{bad}");
    }
}
