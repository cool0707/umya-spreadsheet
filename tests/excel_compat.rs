//! Workbooks read and written again must still open in Excel and show the
//! same (found by opening them in Excel 2021).
//!
//! - `drawing_shapes.xlsx` (xljs `make_shapes.py`): 22 shapes as Excel writes
//!   them, WordArt with `a:prstTxWarp`, an empty `<a:ln w="0"/>` in a run,
//!   vertical text (`vert="eaVert"`) and a picture.
//! - `drawing_picture.xlsx` (openpyxl): a picture with `<a:prstGeom prst="rect"/>`.
//! - `builtin_number_formats.xlsx`: cells styled with the built-in number
//!   formats 0..81 (row n has id n-1) without their definitions, as Excel
//!   saves them.
use std::{
    collections::BTreeMap,
    io::{
        Cursor,
        Read,
    },
};

use umya_spreadsheet::{
    Workbook,
    reader,
    writer,
};

fn file(name: &str) -> Vec<u8> {
    std::fs::read(format!("./tests/test_files/{name}")).unwrap()
}

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn read(bytes: &[u8]) -> Workbook {
    reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap()
}

fn parts(file: &[u8]) -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut map = BTreeMap::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = String::new();
        if f.read_to_string(&mut data).is_ok() {
            map.insert(f.name().to_string(), data);
        }
    }
    map
}

fn count(s: &str, pat: &str) -> usize {
    s.matches(pat).count()
}

/// The names of the shapes (`cNvPr`), in order.
fn shape_names(xml: &str) -> Vec<String> {
    xml.split("<xdr:cNvPr ")
        .skip(1)
        .map(|s| {
            let a = s.find("name=\"").unwrap() + 6;
            s[a..a + s[a..].find('"').unwrap()].to_string()
        })
        .collect()
}

#[test]
fn shapes_survive_a_round_trip() {
    let src = file("drawing_shapes.xlsx");
    let before = parts(&src)["xl/drawings/drawing1.xml"].clone();
    let saved = write(&read(&src));
    let after = parts(&saved)["xl/drawings/drawing1.xml"].clone();

    // An empty `<a:ln w="0"/>` used to take in the next shape, up to its `</a:ln>`.
    let mut want = shape_names(&before);
    let mut got = shape_names(&after);
    want.sort();
    got.sort();
    assert_eq!(got, want);
    for text in ["影付きの文字", "アーチの文字", "WAVE", "円に沿った文字の並び"] {
        assert_eq!(count(&after, text), 1, "{text}");
    }
    assert!(!after.contains(r#"prst="""#));

    // WordArt keeps its warp and the vertical text box its direction.
    assert_eq!(count(&after, "<a:prstTxWarp "), count(&before, "<a:prstTxWarp "));
    for warp in ["textNoShape", "textArchUp", "textWave1", "textInflate", "textCircle"] {
        assert!(after.contains(&format!(r#"<a:prstTxWarp prst="{warp}">"#)), "{warp}");
    }
    assert_eq!(count(&after, "<a:noAutofit/>"), count(&before, "<a:noAutofit/>"));
    assert_eq!(count(&after, r#"vert="eaVert""#), 1);
    // A freeform keeps its path.
    assert_eq!(count(&after, "<a:custGeom>"), 1);
    assert!(after.contains(r#"<a:cubicBezTo><a:pt x="250" y="0"/>"#));
    // Text boxes stay text boxes.
    assert_eq!(count(&after, r#"<xdr:cNvSpPr txBox="1"/>"#), count(&before, r#"<xdr:cNvSpPr txBox="1"/>"#));

    // Writing again gives the same.
    let again = write(&read(&saved));
    assert_eq!(parts(&again)["xl/drawings/drawing1.xml"], after);
}

#[test]
fn a_picture_keeps_its_geometry() {
    let saved = write(&read(&file("drawing_picture.xlsx")));
    let xml = &parts(&saved)["xl/drawings/drawing1.xml"];
    assert!(!xml.contains(r#"prst="""#));
    let pic = &xml[xml.find("<xdr:pic>").unwrap()..];
    assert!(pic.contains(r#"<a:prstGeom prst="rect">"#));
}

/// The numFmtId of the style of each cell of column A, by row.
fn number_format_ids(file: &[u8]) -> Vec<u32> {
    let p = parts(file);
    let styles = &p["xl/styles.xml"];
    let xfs = &styles[styles.find("<cellXfs").unwrap()..styles.find("</cellXfs>").unwrap()];
    let ids: Vec<u32> = xfs
        .split("<xf ")
        .skip(1)
        .map(|x| {
            let a = x.find("numFmtId=\"").unwrap() + 10;
            x[a..a + x[a..].find('"').unwrap()].parse().unwrap()
        })
        .collect();
    let sheet = &p["xl/worksheets/sheet1.xml"];
    sheet
        .split("<c r=\"A")
        .skip(1)
        .map(|c| {
            let tag = &c[..c.find('>').unwrap()];
            let s = tag.find("s=\"").map_or(0, |a| tag[a + 3..a + 3 + tag[a + 3..].find('"').unwrap()].parse::<usize>().unwrap());
            ids[s]
        })
        .collect()
}

#[test]
fn built_in_number_formats_without_definitions_stay_apart() {
    let src = file("builtin_number_formats.xlsx");
    let want: Vec<u32> = (0..82).collect();
    assert_eq!(number_format_ids(&src), want);
    // Those the crate has no code for (5-8, 23-26, 41-43, 63-66...) used to be
    // merged into one style.
    let saved = write(&read(&src));
    assert_eq!(number_format_ids(&saved), want);
}

#[test]
fn strings_with_characters_xml_cannot_hold() {
    let values = [
        "\t制御\u{1}文字\u{1f}",
        "_x000D_ そのまま",
        "a_x005F_b",
        "_x12_ 短い",
        "改行\nあり",
    ];
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    for (i, v) in values.iter().enumerate() {
        sheet.cell_mut((1, i as u32 + 1)).set_value(*v);
    }
    let bytes = write(&book);
    let shared = &parts(&bytes)["xl/sharedStrings.xml"];
    // As Excel writes them.
    assert!(shared.contains("\t制御_x0001_文字_x001F_"));
    assert!(shared.contains("_x005F_x000D_ そのまま"));
    assert!(shared.contains("a_x005F_x005F_b"));
    assert!(shared.contains("_x12_ 短い"));
    assert!(!shared.contains('\u{1}'));

    let reopened = read(&bytes);
    let sheet = reopened.sheet(0).unwrap();
    for (i, v) in values.iter().enumerate() {
        assert_eq!(sheet.value((1, i as u32 + 1)), *v);
    }
}

#[test]
fn strings_escaped_by_excel_are_read_back() {
    // What Excel writes for a cell with U+0001 and one with the text `_x000D_`.
    let mut book = umya_spreadsheet::new_file();
    book.sheet_mut(0).unwrap().cell_mut("A1").set_value("x");
    let bytes = write(&book);
    let mut p = parts(&bytes);
    // And a line end as Excel 2021 wrote one: a CR LF in the file is a LF.
    let shared = p["xl/sharedStrings.xml"].replace("<t>x</t>", "<t>a_x0001_b _x005F_x000D_\r\nc&#13;</t>");
    p.insert("xl/sharedStrings.xml".into(), shared);
    let mut out = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut out);
        for (name, data) in &p {
            zip.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
            std::io::Write::write_all(&mut zip, data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    let book = read(out.get_ref());
    assert_eq!(book.sheet(0).unwrap().value("A1"), "a\u{1}b _x000D_\nc\r");
}

/// What the model of a drawing did not read was lost when the workbook was
/// saved: the start of a line (an oval), a reflection, "do not print" on an
/// object, the alternative text of a picture, the underline and the
/// superscript of a run.
#[test]
fn drawings_keep_what_their_objects_look_like() {
    let shapes = parts(&write(&read(&file("drawing_shapes.xlsx"))));
    let drawing = &shapes["xl/drawings/drawing1.xml"];
    for kept in [
        r#"<a:headEnd type="oval"/>"#,
        r#"<a:reflection blurRad="6350" stA="53000" endA="300" endPos="35500" dist="12700" dir="5400000" sy="-90000" algn="bl" rotWithShape="0"/>"#,
        r#"<xdr:clientData fPrintsWithSheet="0"/>"#,
    ] {
        assert!(drawing.contains(kept), "{kept}");
    }
    let picture = parts(&write(&read(&file("drawing_picture.xlsx"))));
    assert!(picture["xl/drawings/drawing1.xml"].contains(r#"descr="Picture""#));

    let mut book = read(&file("drawing_shapes.xlsx"));
    let anchors = book.sheet_mut(0).unwrap().worksheet_drawing_mut().two_cell_anchor_collection_mut();
    assert!(anchors.iter().any(|a| !a.client_data().prints_with_sheet()));
}

/// The macros of pictures and connectors (`macro` of `xdr:pic`, `xdr:cxnSp`),
/// the apostrophe of text typed with one (`quotePrefix`) and the cell styles
/// of the cells (`xfId`) were lost when saving.
#[test]
fn macros_of_pictures_and_what_cells_were_typed_with() {
    let mut book = read(&file("activex_controls.xlsm"));
    // (The model keeps pictures as images of the sheet.)
    let image = &mut book.sheet_mut(0).unwrap().image_collection_mut()[0];
    let pic = match image.two_cell_anchor_mut() {
        Some(anchor) => anchor.picture_mut(),
        None => image.one_cell_anchor_mut().and_then(|a| a.picture_mut()),
    }
    .unwrap();
    pic.set_macro("'Book1.xlsm'!Module1.Run");
    let ws = book.sheet_mut(0).unwrap();
    ws.cell_mut("A1").set_value("001").style_mut().set_quote_prefix(true);
    let saved = write(&book);
    let p = parts(&saved);
    assert!(p["xl/drawings/drawing1.xml"].contains(r#"<xdr:pic macro="&apos;Book1.xlsm&apos;!Module1.Run">"#) || p["xl/drawings/drawing1.xml"].contains(r#"<xdr:pic macro="'Book1.xlsm'!Module1.Run">"#), "{}", p["xl/drawings/drawing1.xml"]);
    assert!(p["xl/styles.xml"].contains(r#"quotePrefix="1""#));
    let again = read(&saved);
    assert!(again.sheet(0).unwrap().cell("A1").unwrap().style().quote_prefix());
    let again = parts(&write(&again));
    assert!(again["xl/drawings/drawing1.xml"].contains("Module1.Run"));
    assert!(!again["xl/drawings/drawing1.xml"].contains("&amp;apos;"));
}
