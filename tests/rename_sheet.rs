//! Renaming a sheet renames what refers to it, as in Excel (checked by
//! opening the saved workbook in Excel 2021 and comparing it with the one
//! Excel saves after the same rename).
//!
//! `rename_sheet.xlsx` was made by Excel 2021 (xljs
//! `tools/excel-verify/excel-make-rename.ps1`): the sheet `元` and, on the
//! sheet `参照`, formulas, a name, a data validation, a conditional format,
//! sparklines, a chart, a pivot table, a form control and a hyperlink that
//! refer to it.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    Workbook,
    reader,
    writer,
};

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

/// The XML parts of the file (name, content).
fn parts(file: &[u8]) -> Vec<(String, String)> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut list = Vec::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let xml = String::from_utf8_lossy(&data).replace("&apos;", "'");
        list.push((f.name().to_string(), xml));
    }
    list
}

#[test]
fn references_follow_a_renamed_sheet() {
    let data = std::fs::read("./tests/test_files/rename_sheet.xlsx").unwrap();
    let mut book = reader::xlsx::read_reader(Cursor::new(&data), true).unwrap();
    book.set_sheet_name(0, "元 データ").unwrap();
    let out = write(&book);
    let parts = parts(&out);
    let all: String = parts.iter().map(|(_, xml)| xml.as_str()).collect();

    for (want, what) in [
        ("<f>'元 データ'!B2+1</f>", "cell formula"),
        ("<f>SUM('元 データ'!$B$2:$B$5)</f>", "cell formula"),
        (">'元 データ'!$B$2:$B$5</definedName>", "name"),
        (">'元 データ'!$1:$1</definedName>", "print titles"),
        ("<xm:f>'元 データ'!$A$2:$A$5</xm:f>", "data validation"),
        ("<xm:f>C1&gt;'元 データ'!$B$2</xm:f>", "conditional format"),
        ("<xm:f>'元 データ'!D2:G2</xm:f>", "sparkline"),
        ("<c:f>'元 データ'!$B$2:$B$5</c:f>", "chart"),
        ("fmlaRange=\"'元 データ'!$A$2:$A$5\"", "form control"),
        ("sheet=\"元 データ\"", "pivot cache"),
    ] {
        assert!(all.contains(want), "{what}: {want}");
    }
    // Left as they are, as in Excel: a hyperlink and the text of a string.
    assert!(all.contains("location=\"元!A1\""));
    assert!(all.contains("INDIRECT(&quot;元!B2&quot;)") || all.contains("INDIRECT(\"元!B2\")"));
    for (name, xml) in &parts {
        let stripped = xml.replace("location=\"元!A1\"", "").replace("元!B2&quot;", "").replace("元!B2\"", "");
        assert!(!stripped.contains("元!"), "{name} still refers to 元");
    }
}

#[test]
fn renaming_a_sheet_of_a_new_workbook_renames_formulas() {
    let mut book = umya_spreadsheet::new_file();
    book.new_sheet("Data").unwrap();
    let sheet = book.sheet_by_name_mut("Sheet1").unwrap();
    sheet.cell_mut("A1").set_formula("SUM(Data!A1:A3)+'Data'!B1+DataX!A1");
    sheet.add_defined_name("Area", "Data!$A$1:$A$3").unwrap();
    book.set_sheet_name(1, "1月").unwrap();
    let sheet = book.sheet_by_name("Sheet1").unwrap();
    assert_eq!(sheet.cell("A1").unwrap().formula(), "SUM('1月'!A1:A3)+'1月'!B1+DataX!A1");
    assert_eq!(sheet.defined_names()[0].address(), "'1月'!$A$1:$A$3");
}
