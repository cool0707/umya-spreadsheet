//! Array formulas (`t="array"`) and dynamic array formulas (those whose cell
//! refers to the dynamic array properties of `xl/metadata.xml` with `cm`)
//! must stay what they are when a workbook is read and saved again.
use std::io::{
    Cursor,
    Read,
    Write,
};

use umya_spreadsheet::{
    CellFormula,
    CellFormulaValues,
    Workbook,
    reader,
    writer,
};

const SHEET: &str = "xl/worksheets/sheet1.xml";
const METADATA: &str = "xl/metadata.xml";
const RELS: &str = "xl/_rels/workbook.xml.rels";
const TYPES: &str = "[Content_Types].xml";

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn read(bytes: &[u8]) -> Workbook {
    reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap()
}

fn part(bytes: &[u8], name: &str) -> Option<String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    archive.by_name(name).ok()?.read_to_string(&mut xml).unwrap();
    Some(xml)
}

/// The file with its parts edited, and `added` put in.
fn rewrite(bytes: &[u8], edit: impl Fn(&str, &mut String), added: &[(&str, &str)]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let name = file.name().to_string();
        let mut xml = String::new();
        file.read_to_string(&mut xml).unwrap();
        edit(&name, &mut xml);
        output.start_file(name, options).unwrap();
        output.write_all(xml.as_bytes()).unwrap();
    }
    for (name, xml) in added {
        output.start_file(*name, options).unwrap();
        output.write_all(xml.as_bytes()).unwrap();
    }
    output.finish().unwrap().into_inner()
}

/// The `<c>` element of a cell.
fn cell_xml(xml: &str, cell: &str) -> String {
    let start = xml
        .find(&format!(r#"<c r="{cell}""#))
        .unwrap_or_else(|| panic!("{cell} not written: {xml}"));
    let end = start + xml[start..].find("</c>").unwrap() + 4;
    xml[start..end].to_string()
}

/// A workbook as Excel saves it, with numbers in A1:A3 and the given cells
/// in C1 and E1 (their `<c>` elements).
fn book_with(c1: &str, e1: &str, metadata: Option<&str>) -> Vec<u8> {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    for row in 1..=3 {
        sheet.cell_mut((1, row)).set_value_number(row);
    }
    sheet.cell_mut("C1").set_value_number(2);
    sheet.cell_mut("C2").set_value_number(4);
    sheet.cell_mut("C3").set_value_number(6);
    sheet.cell_mut("E1").set_value_number(6);
    let bytes = write(&book);
    let edit = |name: &str, xml: &mut String| match name {
        SHEET => {
            for (plain, with) in [(r#"<c r="C1"><v>2</v></c>"#, c1), (r#"<c r="E1"><v>6</v></c>"#, e1)] {
                assert!(xml.contains(plain), "{xml}");
                *xml = xml.replace(plain, with);
            }
        }
        RELS if metadata.is_some() => {
            *xml = xml.replace(
                "</Relationships>",
                r#"<Relationship Id="rId99" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/></Relationships>"#,
            );
        }
        TYPES if metadata.is_some() => {
            *xml = xml.replace(
                "</Types>",
                r#"<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/></Types>"#,
            );
        }
        _ => {}
    };
    match metadata {
        Some(metadata) => rewrite(&bytes, edit, &[(METADATA, metadata)]),
        None => rewrite(&bytes, edit, &[]),
    }
}

/// The metadata of a workbook with a picture in a cell (a rich value, which
/// is value metadata), a cube function (MDX metadata) and dynamic arrays:
/// the cell metadata blocks are 1 = MDX, 2 = dynamic array, 3 = collapsed
/// dynamic array.
const MIXED_METADATA: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="3"><metadataType name="XLMDX" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/><metadataType name="XLRICHVALUE" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/></metadataTypes><mdxMetadata count="1"><mdx n="0" f="v"><t c="1"><n x="1"/></t></mdx></mdxMetadata><futureMetadata name="XLDAPR" count="2"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="1"/></ext></extLst></bk></futureMetadata><futureMetadata name="XLRICHVALUE" count="1"><bk><extLst><ext uri="{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"><xlrd:rvb i="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="3"><bk><rc t="1" v="0"/></bk><bk><rc t="2" v="0"/></bk><bk><rc t="2" v="1"/></bk></cellMetadata><valueMetadata count="1"><bk><rc t="3" v="0"/></bk></valueMetadata></metadata>"#;

fn formula<'a>(book: &'a Workbook, cell: &str) -> &'a CellFormula {
    book.sheet(0)
        .unwrap()
        .cell(cell)
        .unwrap()
        .formula_obj()
        .unwrap_or_else(|| panic!("{cell} has no formula"))
}

#[test]
fn array_formula_keeps_its_type_and_range() {
    let bytes = book_with(
        r#"<c r="C1"><f t="array" ref="C1:C3">A1:A3*2</f><v>2</v></c>"#,
        r#"<c r="E1"><f t="array" ref="E1">SUM(A1:A3*1)</f><v>6</v></c>"#,
        None,
    );

    let book = read(&bytes);
    let f = formula(&book, "C1");
    assert_eq!(f.formula_type(), &CellFormulaValues::Array);
    assert_eq!(f.reference(), "C1:C3");
    assert_eq!(f.text(), "A1:A3*2");
    assert!(!f.dynamic_array());

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    let c1 = cell_xml(&xml, "C1");
    assert!(c1.contains(r#"<f t="array" ref="C1:C3">A1:A3*2</f>"#), "{c1}");
    assert!(!c1.contains("cm="), "{c1}");
    let e1 = cell_xml(&xml, "E1");
    assert!(e1.contains(r#"<f t="array" ref="E1">SUM(A1:A3*1)</f>"#), "{e1}");
    // The cells the array fills stay plain values.
    assert_eq!(cell_xml(&xml, "C2"), r#"<c r="C2"><v>4</v></c>"#);
    // No cell refers to cell metadata: there is none.
    assert!(part(&saved, METADATA).is_none());
    assert!(!part(&saved, RELS).unwrap().contains("sheetMetadata"));
    assert!(!part(&saved, TYPES).unwrap().contains("metadata.xml"));

    let book = read(&saved);
    assert_eq!(formula(&book, "C1").formula_type(), &CellFormulaValues::Array);
    assert_eq!(formula(&book, "C1").reference(), "C1:C3");
}

#[test]
fn dynamic_array_formula_keeps_its_metadata() {
    let bytes = book_with(
        r#"<c r="C1" cm="2"><f t="array" ref="C1:C3">_xlfn._xlws.FILTER(A1:A3*2,A1:A3&gt;0)</f><v>2</v></c>"#,
        r#"<c r="E1" cm="3"><f t="array" ref="E1">SUM(A1:A3*1)</f><v>6</v></c>"#,
        Some(MIXED_METADATA),
    );

    let book = read(&bytes);
    let f = formula(&book, "C1");
    assert_eq!(f.formula_type(), &CellFormulaValues::Array);
    assert_eq!(f.reference(), "C1:C3");
    assert!(f.dynamic_array());
    assert!(!f.dynamic_array_collapsed());
    let f = formula(&book, "E1");
    assert!(f.dynamic_array());
    assert!(f.dynamic_array_collapsed());

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    // The blocks are numbered again: only what the cells refer to is written.
    let c1 = cell_xml(&xml, "C1");
    assert!(c1.starts_with(r#"<c r="C1" cm="1">"#), "{c1}");
    assert!(
        c1.contains(r#"<f t="array" ref="C1:C3">_xlfn._xlws.FILTER(A1:A3*2,A1:A3&gt;0)</f>"#),
        "{c1}"
    );
    let e1 = cell_xml(&xml, "E1");
    assert!(e1.starts_with(r#"<c r="E1" cm="2">"#), "{e1}");
    assert!(!cell_xml(&xml, "C2").contains("cm="));

    let metadata = part(&saved, METADATA).unwrap();
    assert!(metadata.contains(r#"<metadataTypes count="1"><metadataType name="XLDAPR""#), "{metadata}");
    assert!(metadata.contains(r#"<futureMetadata name="XLDAPR" count="2">"#), "{metadata}");
    assert!(metadata.contains(r#"<xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/>"#), "{metadata}");
    assert!(metadata.contains(r#"<xda:dynamicArrayProperties fDynamic="1" fCollapsed="1"/>"#), "{metadata}");
    assert!(
        metadata.contains(r#"<cellMetadata count="2"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk></cellMetadata>"#),
        "{metadata}"
    );
    assert!(!metadata.contains("valueMetadata"), "{metadata}");
    let rels = part(&saved, RELS).unwrap();
    assert!(
        rels.contains(r#"Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml""#),
        "{rels}"
    );
    let types = part(&saved, TYPES).unwrap();
    assert!(
        types.contains(r#"<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/>"#),
        "{types}"
    );

    let book = read(&saved);
    assert!(formula(&book, "C1").dynamic_array());
    assert!(!formula(&book, "C1").dynamic_array_collapsed());
    assert!(formula(&book, "E1").dynamic_array_collapsed());
}

#[test]
fn other_cell_metadata_is_not_a_dynamic_array() {
    // Block 1 of the metadata is the MDX metadata of a cube function.
    let bytes = book_with(
        r#"<c r="C1" cm="1"><f>CUBEVALUE("c","[m]")</f><v>2</v></c>"#,
        r#"<c r="E1" cm="9"><f>SUM(A1:A3)</f><v>6</v></c>"#,
        Some(MIXED_METADATA),
    );
    let book = read(&bytes);
    assert!(!formula(&book, "C1").dynamic_array());
    assert!(!formula(&book, "E1").dynamic_array());
    assert_eq!(book.sheet(0).unwrap().cell("C1").unwrap().cell_meta_index(), 1);

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(!xml.contains("cm="), "{xml}");
    assert!(cell_xml(&xml, "C1").contains(r#"<f>CUBEVALUE("c","[m]")</f>"#));
    assert!(part(&saved, METADATA).is_none());
}

#[test]
fn new_dynamic_array_formula() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    for row in 1..=3 {
        sheet.cell_mut((1, row)).set_value_number(row);
    }
    // No range: the formula's own cell, until something calculates it.
    let mut f = CellFormula::default();
    f.set_text("_xlfn._xlws.SORT(A1:A3,1,-1)").set_dynamic_array(true);
    sheet.cell_mut("C1").cell_value_mut().set_formula_obj(f);
    // With the range its result fills.
    let mut f = CellFormula::default();
    f.set_text("A1:A3*2").set_reference("E1:E3").set_dynamic_array(true);
    sheet.cell_mut("E1").cell_value_mut().set_formula_obj(f);
    // An ordinary formula.
    sheet.cell_mut("G1").set_formula("SUM(A1:A3)");

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    let c1 = cell_xml(&xml, "C1");
    assert!(c1.starts_with(r#"<c r="C1" cm="1">"#), "{c1}");
    assert!(c1.contains(r#"<f t="array" ref="C1">_xlfn._xlws.SORT(A1:A3,1,-1)</f>"#), "{c1}");
    let e1 = cell_xml(&xml, "E1");
    assert!(e1.contains(r#"cm="1""#), "{e1}");
    assert!(e1.contains(r#"<f t="array" ref="E1:E3">A1:A3*2</f>"#), "{e1}");
    let g1 = cell_xml(&xml, "G1");
    assert!(g1.contains("<f>SUM(A1:A3)</f>"), "{g1}");
    assert!(!g1.contains("cm="), "{g1}");
    let metadata = part(&saved, METADATA).unwrap();
    assert!(metadata.contains(r#"<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata>"#), "{metadata}");

    let mut book = read(&saved);
    let f = formula(&book, "C1");
    assert!(f.dynamic_array());
    assert_eq!(f.formula_type(), &CellFormulaValues::Array);
    assert_eq!(f.reference(), "C1");

    // An ordinary formula again: nothing refers to the metadata any more.
    let sheet = book.sheet_mut(0).unwrap();
    for cell in ["C1", "E1"] {
        let mut f = formula_of(sheet.cell(cell).unwrap());
        f.set_dynamic_array(false);
        f.set_formula_type(CellFormulaValues::Normal);
        sheet.cell_mut(cell).cell_value_mut().set_formula_obj(f);
    }
    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(
        cell_xml(&xml, "C1").contains(r#"<f t="normal">_xlfn._xlws.SORT(A1:A3,1,-1)</f>"#),
        "{xml}"
    );
    assert!(!xml.contains("cm="), "{xml}");
    assert!(part(&saved, METADATA).is_none());
}

fn formula_of(cell: &umya_spreadsheet::Cell) -> CellFormula {
    cell.formula_obj().unwrap().clone()
}

#[test]
fn array_range_moves_with_rows_and_columns() {
    let bytes = book_with(
        r#"<c r="C1" cm="2"><f t="array" ref="C1:C3">A1:A3*2</f><v>2</v></c>"#,
        r#"<c r="E1"><f t="array" ref="E1">SUM(A1:A3*1)</f><v>6</v></c>"#,
        Some(MIXED_METADATA),
    );
    let mut book = read(&bytes);
    let sheet = book.sheet_mut(0).unwrap();
    sheet.insert_new_row(1, 2);
    sheet.insert_new_column("B", 1);
    let f = formula(&book, "D3");
    assert_eq!(f.reference(), "D3:D5");
    assert_eq!(f.text(), "A3:A5*2");
    assert!(f.dynamic_array());
    assert_eq!(formula(&book, "F3").reference(), "F3");

    // A row inside the range.
    let sheet = book.sheet_mut(0).unwrap();
    sheet.insert_new_row(4, 1);
    assert_eq!(formula(&book, "D3").reference(), "D3:D6");
    let sheet = book.sheet_mut(0).unwrap();
    sheet.remove_row(4, 1);
    sheet.remove_row(1, 2);
    sheet.remove_column("B", 1);
    assert_eq!(formula(&book, "C1").reference(), "C1:C3");

    let xml = part(&write(&book), SHEET).unwrap();
    assert!(cell_xml(&xml, "C1").contains(r#"<f t="array" ref="C1:C3">A1:A3*2</f>"#), "{xml}");
}

#[test]
fn sheets_written_as_read_keep_their_metadata_indices() {
    let bytes = book_with(
        r#"<c r="C1" cm="2"><f t="array" ref="C1:C3">A1:A3*2</f><v>2</v></c>"#,
        r#"<c r="E1" cm="3"><f t="array" ref="E1">SUM(A1:A3*1)</f><v>6</v></c>"#,
        Some(MIXED_METADATA),
    );
    // Not deserialized: the sheet is written back as its XML was.
    let mut book = reader::xlsx::read_reader(Cursor::new(&bytes), false).unwrap();
    // A sheet that is, with a dynamic array formula of its own.
    let sheet = book.new_sheet("Sheet2").unwrap();
    let mut f = CellFormula::default();
    f.set_text("Sheet1!A1:A3*3").set_dynamic_array(true);
    sheet.cell_mut("A1").cell_value_mut().set_formula_obj(f);

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(xml.contains(r#"<c r="C1" cm="2">"#), "{xml}");
    assert!(xml.contains(r#"<c r="E1" cm="3">"#), "{xml}");
    let sheet2 = part(&saved, "xl/worksheets/sheet2.xml").unwrap();
    assert!(sheet2.contains(r#"<c r="A1" cm="2">"#), "{sheet2}");
    let metadata = part(&saved, METADATA).unwrap();
    // Block 1 was the MDX metadata: it stays a block, one that says nothing.
    assert!(
        metadata.contains(r#"<cellMetadata count="3"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk><bk><rc t="1" v="2"/></bk></cellMetadata>"#),
        "{metadata}"
    );
    assert!(metadata.contains(r#"<bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="0" fCollapsed="0"/></ext></extLst></bk>"#), "{metadata}");

    let book = read(&saved);
    assert!(formula(&book, "C1").dynamic_array());
    assert!(formula(&book, "E1").dynamic_array_collapsed());
    let f = book.sheet(1).unwrap().cell("A1").unwrap().formula_obj().unwrap();
    assert!(f.dynamic_array() && !f.dynamic_array_collapsed());
}
