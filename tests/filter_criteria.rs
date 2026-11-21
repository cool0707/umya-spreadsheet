//! What the columns of a filter show is read and written.

use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    FilterColumn,
    FilterCriteria,
    reader,
    writer,
};
use zip::ZipArchive;

fn sheet_xml(bytes: &[u8]) -> String {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut xml).unwrap();
    xml
}

#[test]
fn the_criteria_of_a_filter_survive_a_round_trip() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.set_auto_filter("A1:D9");
    let filter = sheet.auto_filter_mut().unwrap();
    let mut by_values = FilterColumn::default();
    by_values.set_column_id(0).set_values(vec!["東京".to_string(), "A&B".to_string()], true);
    let mut by_tests = FilterColumn::default();
    by_tests.set_column_id(1).set_custom(vec![(">=".to_string(), "10".to_string()), ("<".to_string(), "20".to_string())], true);
    let mut by_rank = FilterColumn::default();
    by_rank.set_column_id(2).set_top10(false, true, 25.0);
    let mut plain = FilterColumn::default();
    plain.set_column_id(3).set_hidden_button(true);
    for column in [by_values, by_tests, by_rank, plain] {
        filter.add_filter_column(column);
    }

    let mut bytes = Vec::new();
    writer::xlsx::write_writer(&book, &mut bytes).unwrap();
    let xml = sheet_xml(&bytes);
    assert!(xml.contains(r#"<filterColumn colId="0"><filters blank="1"><filter val="東京"/><filter val="A&amp;B"/></filters></filterColumn>"#), "{xml}");
    assert!(xml.contains(r#"<customFilters and="1"><customFilter operator="greaterThanOrEqual" val="10"/><customFilter operator="lessThan" val="20"/></customFilters>"#), "{xml}");
    assert!(xml.contains(r#"<top10 top="0" percent="1" val="25"/>"#), "{xml}");
    assert!(xml.contains(r#"<filterColumn colId="3" hiddenButton="1"/>"#), "{xml}");

    let mut read = reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap();
    read.read_sheet_collection();
    let columns = read.sheet(0).unwrap().auto_filter().unwrap().filter_columns();
    assert_eq!(columns.len(), 4);
    assert_eq!(columns[0].criteria(), Some(&FilterCriteria::Values { values: vec!["東京".to_string(), "A&B".to_string()], blank: true }));
    assert_eq!(
        columns[1].criteria(),
        Some(&FilterCriteria::Custom { tests: vec![(">=".to_string(), "10".to_string()), ("<".to_string(), "20".to_string())], and: true })
    );
    assert_eq!(columns[2].criteria(), Some(&FilterCriteria::Top10 { top: false, percent: true, val: 25.0 }));
    assert_eq!((columns[3].column_id(), columns[3].hidden_button(), columns[3].criteria()), (3, true, None));
}
