//! Inserting and removing rows and columns: what is on the sheet moves, and
//! the formulas of the whole workbook go through the function given (here a
//! simple one that moves the rows of references to `元`).
//!
//! `rename_sheet.xlsx` (made by Excel 2021) has the sheet `元` (A1:G5) and, on
//! the sheet `参照`, formulas (A1:A3), a hyperlink (A6), a data validation
//! (B1), a conditional format (C1:C4), sparklines (D1:D4), a chart, a pivot
//! table (F10:G15) and a form control that refer to it.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
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

fn all_parts(file: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut all = String::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        all.push_str(&String::from_utf8_lossy(&data));
    }
    all
}

fn open() -> Workbook {
    let data = std::fs::read("./tests/test_files/rename_sheet.xlsx").unwrap();
    reader::xlsx::read_reader(Cursor::new(&data), true).unwrap()
}

/// The rows (at or below `at`) of the references to `sheet` moved by `by`:
/// enough for the formulas of the test file.
fn move_rows(formula: &str, sheet: &str, at: u32, by: i64) -> Option<String> {
    let prefix = format!("{sheet}!");
    if !formula.contains(&prefix) {
        return None;
    }
    let mut out = String::new();
    let mut rest = formula;
    while let Some(i) = rest.find(&prefix) {
        out.push_str(&rest[..i + prefix.len()]);
        rest = &rest[i + prefix.len()..];
        let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '$' || c == ':')).unwrap_or(rest.len());
        let mut part = String::new();
        let mut digits = String::new();
        for c in rest[..end].chars().chain(std::iter::once(' ')) {
            if c.is_ascii_digit() {
                digits.push(c);
                continue;
            }
            if !digits.is_empty() {
                let n: i64 = digits.parse().unwrap();
                part.push_str(&(if n >= i64::from(at) { n + by } else { n }).to_string());
                digits.clear();
            }
            if c != ' ' {
                part.push(c);
            }
        }
        out.push_str(&part);
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out != formula).then_some(out)
}

#[test]
fn every_formula_goes_through_the_function() {
    let mut book = open();
    let mut hosts = Vec::new();
    book.edit_lines("元", &LineEdit::insert_rows(3, 2), &mut |host, f| {
        hosts.push((host.map(str::to_string), f.to_string()));
        move_rows(&f.replace("'元'!", "元!"), "元", 3, 2)
    })
    .unwrap();
    assert!(hosts.contains(&(Some("参照".to_string()), "SUM(元!$B$2:$B$5)".to_string())));
    assert!(hosts.iter().any(|(h, f)| h.is_none() && f == "'元'!$B$2:$B$5"), "{hosts:?}");
    let all = all_parts(&write(&book));
    for (want, what) in [
        ("<f>元!B2+1</f>", "cell formula above"),
        ("<f>SUM(元!$B$2:$B$7)</f>", "cell formula"),
        (">'元'!$B$2:$B$7</definedName>", "name"),
        (">'元'!$A$1:$B$7</definedName>", "print area"),
        ("<xm:f>元!$A$2:$A$7</xm:f>", "data validation"),
        ("<xm:f>C1&gt;元!$B$2</xm:f>", "conditional format"),
        ("<xm:f>元!D5:G5</xm:f>", "sparkline"),
        ("<c:f>元!$B$2:$B$7</c:f>", "chart"),
        ("fmlaRange=\"元!$A$2:$A$7\"", "form control"),
        ("<worksheetSource ref=\"A1:B7\" sheet=\"元\"", "pivot cache"),
        // Not on the sheet edited: left where they are.
        ("<xm:sqref>D2</xm:sqref>", "sparkline place"),
        ("<location ref=\"F10:G15\"", "pivot table place"),
    ] {
        assert!(all.contains(want), "{what}: {want}");
    }
    let sheet = book.sheet_by_name("元").unwrap();
    assert_eq!(sheet.cell("B7").map(|c| c.value().to_string()), open().sheet_by_name("元").unwrap().cell("B5").map(|c| c.value().to_string()));
}

#[test]
fn what_is_on_the_sheet_moves() {
    let mut book = open();
    book.edit_lines("参照", &LineEdit::remove_rows(1, 1), &mut |_, _| None).unwrap();
    let sheet = book.sheet_by_name("参照").unwrap();
    assert_eq!(sheet.cell("A1").unwrap().formula(), "SUM(元!$B$2:$B$5)");
    assert!(sheet.cell("A5").unwrap().hyperlink().is_some());
    assert_eq!(sheet.pivot_table_areas(), vec!["F9:G14".to_string()]);
    let all = all_parts(&write(&book));
    assert!(all.contains("<xm:sqref>C1:C3</xm:sqref>"), "conditional format");
    assert!(!all.contains("<xm:sqref>B1</xm:sqref>"), "data validation removed with its cell");
    // The sparkline of the row removed goes; the others move up.
    assert!(!all.contains("<xm:f>元!D2:G2</xm:f>"));
    for row in 1..=3 {
        assert!(all.contains(&format!("<xm:sqref>D{row}</xm:sqref>")), "sparkline D{row}");
    }
    assert!(all.contains("<location ref=\"F9:G14\""));
}

#[test]
fn columns_inserted_into_a_table_get_names() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    for (i, h) in ["A", "B", "C"].iter().enumerate() {
        sheet.cell_mut((i as u32 + 1, 1)).set_value(*h);
    }
    let mut table = umya_spreadsheet::Table::new("T", ((1, 1), (3, 4)));
    for h in ["A", "B", "C"] {
        table.add_column(umya_spreadsheet::TableColumn::new(h));
    }
    sheet.add_table(table);
    book.edit_lines("Sheet1", &LineEdit::insert_columns(2, 2).with_table_column("列"), &mut |_, _| None).unwrap();
    let sheet = book.sheet(0).unwrap();
    let t = &sheet.tables()[0];
    assert_eq!(t.columns().iter().map(|c| c.name()).collect::<Vec<_>>(), ["A", "列1", "列2", "B", "C"]);
    assert_eq!((t.area().1.col_num(), t.area().1.row_num()), (5, 4));
    assert_eq!(sheet.value("C1"), "列2");
    assert_eq!(sheet.value("D1"), "B");
    book.edit_lines("Sheet1", &LineEdit::remove_columns(1, 2), &mut |_, _| None).unwrap();
    let t = &book.sheet(0).unwrap().tables()[0];
    assert_eq!(t.columns().iter().map(|c| c.name()).collect::<Vec<_>>(), ["列2", "B", "C"]);
    book.edit_lines("Sheet1", &LineEdit::remove_columns(1, 3), &mut |_, _| None).unwrap();
    assert!(book.sheet(0).unwrap().tables().is_empty());
}

#[test]
fn a_copy_refers_to_itself() {
    let mut book = open();
    book.copy_sheet(1, "参照 (2)", 2).unwrap();
    let names: Vec<_> = book.sheet_collection().iter().map(|s| s.name().to_string()).collect();
    assert_eq!(names, ["元", "参照", "参照 (2)"]);
    let copy = book.sheet_by_name("参照 (2)").unwrap();
    // Formulas keep the sheets they name.
    assert_eq!(copy.cell("A2").unwrap().formula(), "SUM(元!$B$2:$B$5)");
    let out = write(&book);
    let reread = reader::xlsx::read_reader(Cursor::new(&out), true).unwrap();
    assert_eq!(reread.sheet_collection().len(), 3);
    let all = all_parts(&out);
    assert_eq!(all.matches("tabSelected=\"1\"").count(), 1);
}

#[test]
fn copies_of_sheets_with_names_and_tables() {
    let mut book = umya_spreadsheet::new_file();
    book.new_sheet("B").unwrap();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("A1").set_value("x");
    let mut table = umya_spreadsheet::Table::new("Sales", ((1, 1), (1, 3)));
    table.add_column(umya_spreadsheet::TableColumn::new("x"));
    sheet.add_table(table);
    let mut name = umya_spreadsheet::DefinedName::default();
    name.set_name("Local");
    name.set_address("Sheet1!$A$1");
    name.set_local_sheet_id(0);
    sheet.defined_names_mut().push(name);
    let mut name = umya_spreadsheet::DefinedName::default();
    name.set_name("OfB");
    name.set_address("B!$A$1");
    name.set_local_sheet_id(1);
    book.sheet_mut(1).unwrap().defined_names_mut().push(name);
    book.copy_sheet(0, "Sheet1 (2)", 1).unwrap();
    let copy = book.sheet(1).unwrap();
    assert_eq!(copy.tables()[0].name(), "Sales_2");
    let local = &copy.defined_names()[0];
    assert_eq!((local.address(), local.local_sheet_id()), ("'Sheet1 (2)'!$A$1".to_string(), 1));
    assert_eq!(book.sheet(2).unwrap().defined_names()[0].local_sheet_id(), 2);
    assert_eq!(book.sheet(0).unwrap().defined_names()[0].local_sheet_id(), 0);
}
