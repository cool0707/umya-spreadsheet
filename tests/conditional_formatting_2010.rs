//! Conditional formatting: icon sets with their kind, data bars with their
//! settings, several formulas, and the Excel 2010 rules (x14) must be read and
//! survive an XLSX round trip.
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    ConditionalFormatValueObjectValues,
    ConditionalFormatting,
    ConditionalFormattingRule,
    Formula,
    NumberingFormat,
    Style,
    Worksheet,
    reader,
    writer,
};

fn sheet_xml(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive
        .by_name("xl/worksheets/sheet1.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    xml
}

fn rule<'a>(ws: &'a Worksheet, sqref: &str) -> &'a ConditionalFormattingRule {
    let cf: &ConditionalFormatting = ws
        .conditional_formatting_collection()
        .iter()
        .find(|c| c.sequence_of_references().get_sqref() == sqref)
        .unwrap();
    &cf.conditional_collection()[0]
}

fn rule_2010<'a>(ws: &'a Worksheet, sqref: &str) -> &'a ConditionalFormattingRule {
    let cf = ws
        .conditional_formatting_2010_collection()
        .iter()
        .find(|c| c.sequence_of_references().get_sqref() == sqref)
        .unwrap();
    &cf.conditional_collection()[0]
}

fn check(ws: &Worksheet) {
    // Icon set: its kind.
    let icons = rule(ws, "D1:D10").icon_set().unwrap();
    assert_eq!(icons.icon_set(), "3Arrows");
    assert_eq!(icons.cfvo_collection().len(), 3);
    assert_eq!(icons.cfvo_collection()[1].val(), "33");

    // A data bar written in both places.
    let main = rule(ws, "E1:E10");
    assert_eq!(main.ext_id(), "{00000000-0000-0000-0000-00000000E0E0}");
    let x = rule_2010(ws, "E1:E10");
    assert_eq!(x.id(), main.ext_id());
    let bar = x.data_bar().unwrap();
    assert_eq!((bar.min_length(), bar.max_length()), (Some(0), Some(100)));
    assert!(!bar.gradient());
    assert!(bar.border());
    assert_eq!(bar.border_color().unwrap().argb_str(), "FF63C384");
    assert_eq!(bar.negative_fill_color().unwrap().argb_str(), "FFFF0000");
    assert_eq!(bar.axis_color().unwrap().argb_str(), "FF000000");
    assert_eq!(
        bar.cfvo_collection()[0].get_type(),
        &ConditionalFormatValueObjectValues::AutoMin
    );

    // An Excel 2010 icon set: values from xm:f, gte.
    let x = rule_2010(ws, "F1:F10");
    assert_eq!(x.priority(), 91);
    let icons = x.icon_set().unwrap();
    assert_eq!(icons.icon_set(), "3Stars");
    assert!(!icons.show_value());
    let vals: Vec<_> = icons.cfvo_collection().iter().map(|v| v.val()).collect();
    assert_eq!(vals, ["0", "4", "8"]);
    assert!(icons.cfvo_collection()[1].gte());
    assert!(!icons.cfvo_collection()[2].gte());

    // Text rules keep their text.
    let cf = ws
        .conditional_formatting_collection()
        .iter()
        .find(|c| c.sequence_of_references().get_sqref() == "G1:G4")
        .unwrap();
    assert_eq!(cf.conditional_collection()[1].text(), "ぶど");
}

#[test]
fn read_and_round_trip() {
    let book = reader::xlsx::read("./tests/test_files/conditional_formatting_2010.xlsx").unwrap();
    check(book.sheet(0).unwrap());

    let mut bytes = Vec::new();
    writer::xlsx::write_writer(&book, &mut bytes).unwrap();
    let xml = sheet_xml(&bytes);
    assert!(xml.contains(r#"<iconSet iconSet="3Arrows">"#), "{xml}");
    assert!(xml.contains("<x14:conditionalFormattings>"));
    assert!(xml.contains("<x14:id>{00000000-0000-0000-0000-00000000E0E0}</x14:id>"));

    let reopened = reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap();
    check(reopened.sheet(0).unwrap());
}

#[test]
fn between_and_number_format() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    let mut r = ConditionalFormattingRule::default();
    r.set_type(umya_spreadsheet::ConditionalFormatValues::CellIs);
    r.set_operator(umya_spreadsheet::ConditionalFormattingOperatorValues::Between);
    r.set_priority(1);
    let mut f1 = Formula::default();
    f1.set_string_value("1");
    let mut f2 = Formula::default();
    f2.set_string_value("5");
    r.add_formula_collection(f1).add_formula_collection(f2);
    let mut style = Style::default();
    let mut nf = NumberingFormat::default();
    nf.set_format_code("0.0\"pt\"");
    style.set_numbering_format(nf);
    style.font_mut().set_bold(true);
    r.set_style(style);
    let mut cf = ConditionalFormatting::default();
    cf.sequence_of_references_mut().set_sqref("A1:A3");
    cf.add_conditional_collection(r);
    ws.add_conditional_formatting_collection(cf);

    let mut bytes = Vec::new();
    writer::xlsx::write_writer(&book, &mut bytes).unwrap();
    let reopened = reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap();
    let r = rule(reopened.sheet(0).unwrap(), "A1:A3");
    let fs: Vec<_> = r.formula_collection().iter().map(|f| f.address_str()).collect();
    assert_eq!(fs, ["1", "5"]);
    let style = r.style().unwrap();
    assert_eq!(style.numbering_format().unwrap().format_code(), "0.0\"pt\"");
    assert!(style.font().unwrap().bold());
}
