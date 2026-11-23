//! Settings of the workbook and its sheets that are read, set and written
//! through the model: furigana (`phoneticPr`, the `ph` of cells, rows and
//! columns), the version of the application (`fileVersion`), the password to
//! modify and read-only recommended (`fileSharing`), the errors not flagged
//! (`ignoredErrors`), how a sheet is calculated (`sheetCalcPr`).
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    FileSharing,
    FileVersion,
    IgnoredError,
    IgnoredErrorKind,
    PhoneticAlignment,
    PhoneticProperties,
    PhoneticType,
    Workbook,
    helper::rewrite::LineEdit,
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

fn part(file: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut data = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut data).unwrap();
    data
}

fn file(name: &str) -> Vec<u8> {
    std::fs::read(format!("./tests/test_files/{name}")).unwrap()
}

#[test]
fn furigana_settings() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    // The sheet's.
    let mut sheet = PhoneticProperties::default();
    sheet.set_phonetic_type(PhoneticType::Hiragana).set_alignment(PhoneticAlignment::Center);
    ws.set_phonetic_properties(Some(sheet));
    // A text's, with its furigana.
    ws.cell_mut("A1").set_value("東京");
    ws.cell_mut("A1").set_show_phonetic(true);
    ws.cell_mut("A1").cell_value_mut().set_phonetic_runs(&[(0, 2, "トウキョウ".into())], None);
    let mut text = PhoneticProperties::default();
    text.set_phonetic_type(PhoneticType::HalfwidthKatakana).set_alignment(PhoneticAlignment::Distributed).set_font_id(2);
    ws.cell_mut("A1").cell_value_mut().set_phonetic_properties(Some(&text));
    // Settings of a text without furigana.
    ws.cell_mut("A2").set_value("大阪");
    let mut other = PhoneticProperties::default();
    other.set_alignment(PhoneticAlignment::NoControl);
    ws.cell_mut("A2").cell_value_mut().set_phonetic_properties(Some(&other));
    // Rows and columns that show them.
    ws.row_dimension_mut(3).set_show_phonetic(true);
    ws.column_dimension_mut("B").set_show_phonetic(true);

    let saved = write(&book);
    let sheet_xml = part(&saved, "xl/worksheets/sheet1.xml");
    assert!(sheet_xml.contains(r#"<phoneticPr fontId="1" type="Hiragana" alignment="center"/>"#), "{sheet_xml}");
    assert!(sheet_xml.contains(r#"ph="1""#), "{sheet_xml}");
    assert!(sheet_xml.contains(r#"phonetic="1""#), "{sheet_xml}");
    let strings = part(&saved, "xl/sharedStrings.xml");
    assert!(strings.contains(r#"<phoneticPr fontId="2" type="halfwidthKatakana" alignment="distributed"/>"#), "{strings}");
    assert!(strings.contains(r#"<phoneticPr fontId="1" alignment="noControl"/>"#), "{strings}");

    let book = read(&saved);
    let ws = book.sheet(0).unwrap();
    assert_eq!(ws.phonetic_properties(), Some(&sheet));
    let a1 = ws.cell("A1").unwrap();
    assert!(a1.show_phonetic());
    assert_eq!(a1.cell_value().phonetic_properties(), Some(text));
    assert_eq!(a1.cell_value().phonetic_runs(), [(0, 2, "トウキョウ".to_string())]);
    assert_eq!(ws.cell("A2").unwrap().cell_value().phonetic_properties(), Some(other));
    assert!(ws.row_dimension(3).unwrap().show_phonetic());
    assert!(ws.column_dimension("B").unwrap().show_phonetic());

    // New furigana keep the alignment and font of the text.
    let mut book = book;
    let cell = book.sheet_mut(0).unwrap().cell_mut("A1");
    cell.cell_value_mut().set_phonetic_runs(&[(0, 2, "とうきょう".into())], Some("Hiragana"));
    let p = cell.cell_value().phonetic_properties().unwrap();
    assert_eq!((p.phonetic_type(), p.alignment(), p.font_id()), (PhoneticType::Hiragana, PhoneticAlignment::Distributed, 2));
    // None: the default, the furigana stay.
    cell.cell_value_mut().set_phonetic_properties(None);
    assert_eq!(cell.cell_value().phonetic_properties(), Some(PhoneticProperties::default()));
    assert_eq!(cell.cell_value().phonetic_runs().len(), 1);
}

#[test]
fn the_version_of_the_application() {
    // A new workbook: Excel's.
    let saved = write(&umya_spreadsheet::new_file());
    assert!(part(&saved, "xl/workbook.xml").contains(r#"<fileVersion appName="xl" lastEdited="5" lowestEdited="4" rupBuild="9302"/>"#));

    // As read, and set.
    let source = file("aaa.xlsm");
    let before = part(&source, "xl/workbook.xml");
    let mut book = read(&source);
    let version = book.file_version().unwrap().clone();
    assert_eq!(version.app_name(), Some("xl"));
    assert!(before.contains(&format!(r#"rupBuild="{}""#, version.rup_build().unwrap())));
    let mut changed = version.clone();
    changed.set_last_edited(Some("7")).set_code_name(None);
    book.set_file_version(Some(changed));
    let workbook = part(&write(&book), "xl/workbook.xml");
    assert!(workbook.contains(r#"lastEdited="7""#), "{workbook}");
    assert!(!workbook[..workbook.find("<workbookPr").unwrap()].contains("codeName"), "{workbook}");

    let mut v = FileVersion::default();
    v.set_app_name(Some("xl"));
    book.set_file_version(Some(v));
    assert!(part(&write(&book), "xl/workbook.xml").contains(r#"<fileVersion appName="xl"/>"#));
}

#[test]
fn the_password_to_modify_and_read_only_recommended() {
    let mut book = umya_spreadsheet::new_file();
    assert!(!part(&write(&book), "xl/workbook.xml").contains("fileSharing"));
    book.file_sharing_mut().set_read_only_recommended(true).set_user_name(Some("経理")).set_password("書き込み用");
    let saved = write(&book);
    let workbook = part(&saved, "xl/workbook.xml");
    assert!(workbook.contains(r#"<fileSharing readOnlyRecommended="1" userName="経理" algorithmName="SHA-512" hashValue=""#), "{workbook}");
    // Before workbookPr, as the schema has it.
    assert!(workbook.find("<fileSharing").unwrap() < workbook.find("<workbookPr").unwrap());

    let mut book = read(&saved);
    let sharing = book.file_sharing().unwrap();
    assert!(sharing.read_only_recommended() && sharing.has_password());
    assert_eq!(sharing.user_name(), Some("経理"));
    assert!(sharing.check_password("書き込み用"));
    assert!(!sharing.check_password("違う"));

    // Removed: only read-only recommended is left.
    book.file_sharing_mut().remove_password();
    let workbook = part(&write(&book), "xl/workbook.xml");
    assert!(workbook.contains(r#"<fileSharing readOnlyRecommended="1" userName="経理"/>"#), "{workbook}");
    // Nothing left: nothing written.
    book.set_file_sharing(Some(FileSharing::default()));
    assert!(!part(&write(&book), "xl/workbook.xml").contains("fileSharing"));

    // The password of Excel 2007 (reservationPassword) is checked too.
    let mut book = umya_spreadsheet::new_file();
    book.file_sharing_mut().set_read_only_recommended(true);
    let saved = write(&book);
    let workbook = part(&saved, "xl/workbook.xml").replace(r#"readOnlyRecommended="1""#, r#"reservationPassword="DAA7""#);
    let mut archive = zip::ZipArchive::new(Cursor::new(&saved)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "xl/workbook.xml" {
            data = workbook.clone().into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        std::io::Write::write_all(&mut out, &data).unwrap();
    }
    let book = read(&out.finish().unwrap().into_inner());
    let sharing = book.file_sharing().unwrap();
    assert!(sharing.has_password() && sharing.check_password("secret") && !sharing.check_password("other"));
}

#[test]
fn errors_not_flagged() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    ws.cell_mut("A1").set_value("1");
    ws.add_ignored_error(IgnoredError::new("B2:C5 E5", &[IgnoredErrorKind::NumberStoredAsText]));
    ws.add_ignored_error(IgnoredError::new("D4", &[IgnoredErrorKind::Formula, IgnoredErrorKind::EvalError]));
    let saved = write(&book);
    let sheet = part(&saved, "xl/worksheets/sheet1.xml");
    assert!(
        sheet.contains(r#"<ignoredErrors><ignoredError sqref="B2:C5 E5" numberStoredAsText="1"/><ignoredError sqref="D4" evalError="1" formula="1"/></ignoredErrors>"#),
        "{sheet}"
    );
    assert!(sheet.find("<pageMargins").unwrap() < sheet.find("<ignoredErrors").unwrap());

    let mut book = read(&saved);
    let list = book.sheet(0).unwrap().ignored_errors().to_vec();
    assert_eq!(list.len(), 2);
    assert!(list[1].is_ignored(IgnoredErrorKind::Formula) && !list[1].is_ignored(IgnoredErrorKind::NumberStoredAsText));

    // They follow their cells; those all of whose cells go are taken away.
    book.edit_lines("Sheet1", &LineEdit::insert_rows(3, 2), &mut |_, _| None).unwrap();
    book.edit_lines("Sheet1", &LineEdit::remove_columns(4, 1), &mut |_, _| None).unwrap();
    let list = book.sheet(0).unwrap().ignored_errors();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].sqref(), "B2:C7 D7");
    book.insert_new_row("Sheet1", 1, 1);
    assert_eq!(book.sheet(0).unwrap().ignored_errors()[0].sqref(), "B3:C8 D8");

    // Changed.
    let ws = book.sheet_mut(0).unwrap();
    ws.ignored_errors_mut()[0].set_ignored(IgnoredErrorKind::NumberStoredAsText, false).set_ignored(IgnoredErrorKind::TwoDigitTextYear, true);
    let sheet = part(&write(&book), "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"<ignoredError sqref="B3:C8 D8" twoDigitTextYear="1"/>"#), "{sheet}");
    // None left: nothing written.
    book.sheet_mut(0).unwrap().ignored_errors_mut().clear();
    assert!(!part(&write(&book), "xl/worksheets/sheet1.xml").contains("ignoredErrors"));
}

#[test]
fn how_a_sheet_is_calculated() {
    let mut book = umya_spreadsheet::new_file();
    assert_eq!(book.sheet(0).unwrap().full_calc_on_load(), None);
    book.sheet_mut(0).unwrap().set_full_calc_on_load(Some(true));
    let saved = write(&book);
    let sheet = part(&saved, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"<sheetData/><sheetCalcPr fullCalcOnLoad="1"/>"#), "{sheet}");
    let mut book = read(&saved);
    assert_eq!(book.sheet(0).unwrap().full_calc_on_load(), Some(true));
    book.sheet_mut(0).unwrap().set_full_calc_on_load(None);
    assert!(!part(&write(&book), "xl/worksheets/sheet1.xml").contains("sheetCalcPr"));
}
