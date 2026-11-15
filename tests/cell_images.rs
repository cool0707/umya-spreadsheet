//! Pictures placed in cells are rich values: the cell refers to a block of
//! the value metadata of `xl/metadata.xml` with `vm`, which refers to a rich
//! value of `xl/richData`, which refers to a picture in `xl/media`. They
//! must stay in the cells when a workbook is read and saved again.
//!
//! `cell_images.xlsx`: sheet 1 has pictures in B2 (vm 1), B3 (vm 2) and B4
//! (vm 3) and the formulas `=B2`, `=B3` in D2, D3 with the `vm` of what they
//! show; sheet 2 has `=商品!B3` in A2 (vm 2).
use std::io::{
    Cursor,
    Read,
};

use umya_spreadsheet::{
    CellFormula,
    Workbook,
    reader,
    structs::{
        Image,
        drawing::spreadsheet::MarkerType,
    },
    writer,
};

const FILE: &str = "./tests/test_files/cell_images.xlsx";
const SHEET: &str = "xl/worksheets/sheet1.xml";
const METADATA: &str = "xl/metadata.xml";
const RELS: &str = "xl/_rels/workbook.xml.rels";
const TYPES: &str = "[Content_Types].xml";
const RICH_PARTS: [&str; 7] = [
    "xl/richData/rdrichvalue.xml",
    "xl/richData/rdrichvaluestructure.xml",
    "xl/richData/rdRichValueTypes.xml",
    "xl/richData/richValueRel.xml",
    "xl/media/image1.png",
    "xl/media/image2.jpeg",
    "xl/media/image3.png",
];

fn file() -> Vec<u8> {
    std::fs::read(FILE).unwrap()
}

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn read(bytes: &[u8]) -> Workbook {
    reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap()
}

fn bytes(file: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut data = Vec::new();
    archive.by_name(name).ok()?.read_to_end(&mut data).unwrap();
    Some(data)
}

fn part(file: &[u8], name: &str) -> Option<String> {
    bytes(file, name).map(|data| String::from_utf8(data).unwrap())
}

fn names(file: &[u8]) -> Vec<String> {
    let archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    archive.file_names().map(ToString::to_string).collect()
}

fn vm(book: &Workbook, sheet: usize, cell: &str) -> u32 {
    book.sheet(sheet)
        .unwrap()
        .cell(cell)
        .map_or(0, umya_spreadsheet::Cell::value_meta_index)
}

#[test]
fn pictures_in_cells_come_back_from_a_save() {
    let original = file();
    let book = read(&original);
    assert_eq!(vm(&book, 0, "B2"), 1);
    assert_eq!(vm(&book, 0, "B4"), 3);
    assert_eq!(vm(&book, 0, "A2"), 0);
    assert_eq!(vm(&book, 1, "A2"), 2);

    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(xml.contains(r#"<c r="B2" t="e" vm="1"><v>#VALUE!</v></c>"#), "{xml}");
    assert!(xml.contains(r#"<c r="D3" t="e" vm="2"><f>B3</f><v>#VALUE!</v></c>"#), "{xml}");
    assert!(xml.contains(r#"<c r="B4" t="e" s="1" vm="3"><v>#VALUE!</v></c>"#), "{xml}");
    let xml = part(&saved, "xl/worksheets/sheet2.xml").unwrap();
    assert!(xml.contains(r#"<c r="A2" t="e" vm="2">"#), "{xml}");

    // The value metadata refers to the rich values it did.
    let metadata = part(&saved, METADATA).unwrap();
    assert!(metadata.contains(r#"<metadataTypes count="1"><metadataType name="XLRICHVALUE""#), "{metadata}");
    for i in 0..3 {
        assert!(metadata.contains(&format!(r#"<xlrd:rvb i="{i}"/>"#)), "{metadata}");
    }
    assert!(
        metadata.contains(r#"<valueMetadata count="3"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk><bk><rc t="1" v="2"/></bk></valueMetadata>"#),
        "{metadata}"
    );
    assert!(!metadata.contains("cellMetadata"), "{metadata}");

    // The rich values and their pictures are what they were.
    for name in RICH_PARTS {
        assert_eq!(bytes(&saved, name), bytes(&original, name), "{name}");
    }
    let rels = part(&saved, "xl/richData/_rels/richValueRel.xml.rels").unwrap();
    for (id, target) in [
        ("rId1", "../media/image1.png"),
        ("rId2", "../media/image2.jpeg"),
        ("rId3", "../media/image3.png"),
    ] {
        assert!(rels.contains(&format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{target}"/>"#)), "{rels}");
    }

    // The workbook refers to them, and their content types are declared.
    let rels = part(&saved, RELS).unwrap();
    for (rel_type, target) in [
        ("http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata", "metadata.xml"),
        ("http://schemas.microsoft.com/office/2022/10/relationships/richValueRel", "richData/richValueRel.xml"),
        ("http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue", "richData/rdrichvalue.xml"),
        ("http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure", "richData/rdrichvaluestructure.xml"),
        ("http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes", "richData/rdRichValueTypes.xml"),
    ] {
        assert!(rels.contains(&format!(r#"Type="{rel_type}" Target="{target}"/>"#)), "{rels}");
    }
    let types = part(&saved, TYPES).unwrap();
    for (name, content_type) in [
        ("/xl/metadata.xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"),
        ("/xl/richData/rdrichvalue.xml", "application/vnd.ms-excel.rdrichvalue+xml"),
        ("/xl/richData/rdrichvaluestructure.xml", "application/vnd.ms-excel.rdrichvaluestructure+xml"),
        ("/xl/richData/richValueRel.xml", "application/vnd.ms-excel.richvaluerel+xml"),
        ("/xl/richData/rdRichValueTypes.xml", "application/vnd.ms-excel.rdrichvaluetypes+xml"),
    ] {
        assert!(types.contains(&format!(r#"<Override PartName="{name}" ContentType="{content_type}"/>"#)), "{types}");
    }
    assert!(types.contains(r#"<Default Extension="png" ContentType="image/png"/>"#), "{types}");
    assert!(types.contains(r#"<Default Extension="jpeg" ContentType="image/jpeg"/>"#), "{types}");

    // And once more.
    let again = write(&read(&saved));
    assert_eq!(part(&again, SHEET), part(&saved, SHEET));
    assert_eq!(part(&again, METADATA), part(&saved, METADATA));
    for name in RICH_PARTS {
        assert_eq!(bytes(&again, name), bytes(&original, name), "{name}");
    }
}

#[test]
fn pictures_move_with_their_cells() {
    let mut book = read(&file());
    book.insert_new_row("商品", 3, 2);
    book.insert_new_column("商品", "A", 1);
    assert_eq!(vm(&book, 0, "C2"), 1);
    assert_eq!(vm(&book, 0, "C5"), 2);
    assert_eq!(vm(&book, 0, "B3"), 0);
    let xml = part(&write(&book), SHEET).unwrap();
    assert!(xml.contains(r#"<c r="C2" t="e" vm="1">"#), "{xml}");
    assert!(xml.contains(r#"<c r="C5" t="e" vm="2">"#), "{xml}");
    assert!(xml.contains(r#"<c r="C6" t="e" s="1" vm="3">"#), "{xml}");
    assert!(xml.contains(r#"<c r="E5" t="e" vm="2"><f>C5</f>"#), "{xml}");
}

#[test]
fn a_value_written_over_a_picture_takes_its_place() {
    let mut book = read(&file());
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("B2").set_value("りんご");
    sheet.cell_mut("B3").set_value_number(3);
    // Taken off by hand: the error stays.
    sheet.cell_mut("B4").set_value_meta_index(0);
    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(xml.contains(r#"<c r="B2" t="s"><v>"#), "{xml}");
    assert!(xml.contains(r#"<c r="B3"><v>3</v></c>"#), "{xml}");
    assert!(xml.contains(r#"<c r="B4" t="e" s="1"><v>#VALUE!</v></c>"#), "{xml}");
    // The formulas still show what they did.
    assert!(xml.contains(r#"<c r="D2" t="e" vm="1"><f>B2</f>"#), "{xml}");
    assert!(part(&saved, METADATA).is_some());
}

#[test]
fn rich_values_no_cell_refers_to_are_not_written() {
    let mut book = read(&file());
    for (sheet, cell) in [(0, "B2"), (0, "B3"), (0, "B4"), (0, "D2"), (0, "D3"), (1, "A2")] {
        book.sheet_mut(sheet).unwrap().cell_mut(cell).set_value("");
    }
    let saved = write(&book);
    assert!(!part(&saved, SHEET).unwrap().contains("vm="));
    let names = names(&saved);
    assert!(!names.iter().any(|name| name.starts_with("xl/richData/")), "{names:?}");
    assert!(!names.iter().any(|name| name.starts_with("xl/media/")), "{names:?}");
    assert!(!names.iter().any(|name| name == METADATA), "{names:?}");
    assert!(!part(&saved, RELS).unwrap().contains("richData"));
    assert!(!part(&saved, TYPES).unwrap().contains("richData"));
}

#[test]
fn a_vm_that_refers_to_nothing_is_not_written() {
    let mut book = read(&file());
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("A2").set_error("#VALUE!").set_value_meta_index(9);
    // Not an error: the cell holds a value of its own.
    sheet.cell_mut("A3").set_value_meta_index(1);
    let xml = part(&write(&book), SHEET).unwrap();
    assert!(xml.contains(r#"<c r="A2" t="e"><v>#VALUE!</v></c>"#), "{xml}");
    assert!(xml.contains(r#"<c r="A3" t="s"><v>"#), "{xml}");

    // A workbook without rich values.
    let mut book = umya_spreadsheet::new_file();
    book.sheet_mut(0)
        .unwrap()
        .cell_mut("A1")
        .set_error("#VALUE!")
        .set_value_meta_index(1);
    let saved = write(&book);
    assert!(!part(&saved, SHEET).unwrap().contains("vm="));
    assert!(part(&saved, METADATA).is_none());
}

#[test]
fn value_metadata_and_dynamic_arrays_share_the_metadata() {
    let mut book = read(&file());
    let mut f = CellFormula::default();
    f.set_text("SORT(A2:A4)").set_dynamic_array(true);
    book.sheet_mut(0)
        .unwrap()
        .cell_mut("F2")
        .cell_value_mut()
        .set_formula_obj(f);
    let saved = write(&book);
    let xml = part(&saved, SHEET).unwrap();
    assert!(xml.contains(r#"<c r="F2" cm="1">"#), "{xml}");
    assert!(xml.contains(r#"<c r="B3" t="e" vm="2">"#), "{xml}");
    let metadata = part(&saved, METADATA).unwrap();
    assert!(metadata.contains(r#"<metadataTypes count="2"><metadataType name="XLDAPR""#), "{metadata}");
    assert!(metadata.contains(r#"<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata>"#), "{metadata}");
    assert!(metadata.contains(r#"<valueMetadata count="3"><bk><rc t="2" v="0"/></bk><bk><rc t="2" v="1"/></bk><bk><rc t="2" v="2"/></bk></valueMetadata>"#), "{metadata}");

    let book = read(&saved);
    assert_eq!(vm(&book, 0, "B3"), 2);
    assert!(book.sheet(0).unwrap().cell("F2").unwrap().formula_obj().unwrap().dynamic_array());
    assert_eq!(part(&write(&book), METADATA), Some(metadata));
}

#[test]
fn sheets_written_as_read_keep_their_pictures() {
    // Not deserialized: the sheets are written back as their XML was.
    let original = file();
    let book = reader::xlsx::read_reader(Cursor::new(&original), false).unwrap();
    let saved = write(&book);
    assert!(part(&saved, SHEET).unwrap().contains(r#"<c r="B2" t="e" vm="1">"#));
    assert!(part(&saved, METADATA).unwrap().contains(r#"<valueMetadata count="3">"#));
    for name in RICH_PARTS {
        assert_eq!(bytes(&saved, name), bytes(&original, name), "{name}");
    }
    assert_eq!(vm(&read(&saved), 0, "B4"), 3);
}

#[test]
fn a_picture_of_a_sheet_with_the_same_name_does_not_replace_one_in_a_cell() {
    let original = file();
    let mut book = read(&original);
    // A picture over the cells, named like the first picture in a cell.
    let other = bytes(&original, "xl/media/image3.png").unwrap();
    let mut marker = MarkerType::default();
    marker.set_coordinate("F2");
    let mut image = Image::default();
    image.new_image_with_dimensions(60, 30, "image1.png", other.clone(), marker);
    book.sheet_mut(0).unwrap().add_image(image);

    let saved = write(&book);
    assert_eq!(bytes(&saved, "xl/media/image1.png"), Some(other));
    // image2 is the other picture in a cell and image3 the third: the next
    // free name.
    let rels = part(&saved, "xl/richData/_rels/richValueRel.xml.rels").unwrap();
    assert!(rels.contains(r#"<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="/xl/media/image2.png"/>"#), "{rels}");
    assert!(rels.contains(r#"Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image2.jpeg"/>"#), "{rels}");
    assert_eq!(
        bytes(&saved, "xl/media/image2.png"),
        bytes(&original, "xl/media/image1.png")
    );
    assert_eq!(
        bytes(&saved, "xl/media/image3.png"),
        bytes(&original, "xl/media/image3.png")
    );

    // Read again, every picture is still its own.
    let again = write(&read(&saved));
    assert_eq!(
        bytes(&again, "xl/media/image2.png"),
        bytes(&original, "xl/media/image1.png")
    );
    assert_eq!(part(&again, SHEET).unwrap().matches("vm=").count(), 5);
}
