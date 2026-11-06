//! The "shrink to fit" alignment flag must survive an XLSX round trip.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    reader,
    writer,
};

fn styles_xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("xl/styles.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

#[test]
fn shrink_to_fit_survives_round_trip() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("A1").set_value("A long label in a narrow cell");
    sheet
        .style_mut("A1")
        .alignment_mut()
        .set_shrink_to_fit(true);
    sheet.cell_mut("B1").set_value("plain");
    sheet.style_mut("B1").alignment_mut().set_wrap_text(true);

    let mut bytes = Vec::new();
    writer::xlsx::write_writer(&book, &mut bytes).unwrap();
    assert!(styles_xml(&bytes).contains(r#"shrinkToFit="1""#));

    let reopened = reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap();
    let sheet = reopened.sheet(0).unwrap();
    let a1 = sheet.cell("A1").unwrap().style().alignment().unwrap();
    assert!(a1.shrink_to_fit());
    let b1 = sheet.cell("B1").unwrap().style().alignment().unwrap();
    assert!(!b1.shrink_to_fit());
    assert!(b1.wrap_text());

    // Writing again must keep the flag.
    let mut again = Vec::new();
    writer::xlsx::write_writer(&reopened, &mut again).unwrap();
    assert!(styles_xml(&again).contains(r#"shrinkToFit="1""#));
}
