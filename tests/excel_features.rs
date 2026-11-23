//! What a workbook made by Excel has besides its cells survives a round trip
//! (found by opening the saved workbooks in Excel 2021).
//!
//! `excel_features.xlsx` was made by Excel 2021 (xljs
//! `tools/excel-verify/excel-make-features.ps1`): a table with a style of its
//! own, sparklines, a pivot table with a slicer and a timeline, a slicer of
//! the table, form controls, data validations with messages, a note, sheet
//! and workbook protection, hyperlinks, an external link, conditional
//! formats, the page setup (black and white, print area), grouped rows,
//! a picture, furigana, names, custom XML.
use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
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
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        map.insert(f.name().to_string(), String::from_utf8_lossy(&data).into_owned());
    }
    map
}

/// The name of a part without its numbers (`xl/slicers/slicer#.xml`).
fn kind(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn all_text(p: &BTreeMap<String, String>, prefix: &str) -> String {
    p.iter().filter(|(n, _)| n.starts_with(prefix)).map(|(_, d)| d.as_str()).collect()
}

/// The values of `attr="..."` in the text.
fn values<'a>(text: &'a str, attr: &str) -> Vec<&'a str> {
    let pat = format!(" {attr}=\"");
    text.match_indices(&pat)
        .map(|(i, _)| {
            let rest = &text[i + pat.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

fn rels_of(name: &str) -> String {
    let (dir, file) = name.rsplit_once('/').unwrap_or(("", name));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

#[test]
fn what_excel_made_survives_a_round_trip() {
    let src = file("excel_features.xlsx");
    let before = parts(&src);
    let saved = write(&read(&src));
    let after = parts(&saved);

    // No kind of part is lost, but the calculation chain (Excel makes it
    // again) and the VML of the form controls (Excel 2010 and later read
    // them from the sheet and the drawing).
    let kinds = |p: &BTreeMap<String, String>| p.keys().map(|n| kind(n)).collect::<BTreeSet<_>>();
    let lost: Vec<String> = kinds(&before)
        .difference(&kinds(&after))
        .filter(|k| !matches!(k.as_str(), "xl/calcChain.xml" | "xl/drawings/vmlDrawing#.vml"))
        .cloned()
        .collect();
    assert!(lost.is_empty(), "lost: {lost:?}");
    for kind_needed in [
        "xl/externalLinks/externalLink#.xml",
        "xl/externalLinks/_rels/externalLink#.xml.rels",
        "xl/slicerCaches/slicerCache#.xml",
        "xl/slicers/slicer#.xml",
        "xl/timelineCaches/timelineCache#.xml",
        "xl/timelines/timeline#.xml",
        "xl/ctrlProps/ctrlProp#.xml",
        "customXml/item#.xml",
        "customXml/itemProps#.xml",
        "xl/pivotTables/_rels/pivotTable#.xml.rels",
        "xl/pivotCache/_rels/pivotCacheDefinition#.xml.rels",
        "xl/pivotCache/pivotCacheRecords#.xml",
    ] {
        assert!(kinds(&after).contains(kind_needed), "{kind_needed}");
    }
    // One pivot cache, though the workbook and the pivot table both refer to it.
    assert_eq!(after.keys().filter(|n| kind(n) == "xl/pivotCache/pivotCacheDefinition#.xml").count(), 1);

    let sheets = all_text(&after, "xl/worksheets/sheet");
    let workbook = &after["xl/workbook.xml"];
    for (what, text, needle) in [
        ("sparklines", &sheets, "<x14:sparklineGroups"),
        ("slicers of the sheet", &sheets, "<x14:slicerList"),
        ("timelines of the sheet", &sheets, "<x15:timelineRefs"),
        ("form controls", &sheets, "<controls"),
        ("grouped rows", &sheets, r#"outlineLevel="1""#),
        ("furigana shown", &sheets, r#" ph="1""#),
        ("black and white", &sheets, r#"blackAndWhite="1""#),
        ("external link", workbook, "<externalReferences"),
        ("pivot caches", workbook, "<pivotCaches"),
        ("slicer caches", workbook, "slicerCaches"),
        ("timeline caches", workbook, "<x15:timelineCacheRefs"),
        ("furigana", &after["xl/sharedStrings.xml"], "<rPh"),
        ("a table style of its own", &after["xl/styles.xml"], r#"<tableStyle name="独自の表""#),
        ("the timeline style", &after["xl/styles.xml"], "<x15:timelineStyles"),
    ] {
        assert!(text.contains(needle), "{what}: {needle}");
    }
    assert!(!all_text(&after, "xl/drawings/").contains(r#"noChangeAspect="2""#));

    // Every r:id refers to a relationship of its part.
    for (name, data) in &after {
        if !name.ends_with(".xml") || name.contains("_rels/") {
            continue;
        }
        let ids = values(data, "r:id");
        if ids.is_empty() {
            continue;
        }
        let rels = after.get(&rels_of(name)).unwrap_or_else(|| panic!("no relationships for {name}"));
        let have = values(rels, "Id");
        for id in ids {
            assert!(have.contains(&id), "{name}: r:id=\"{id}\" has no relationship");
        }
    }

    // Every prefix used is declared (a kept element using xr2:uid needs it).
    for (name, data) in &after {
        if !name.ends_with(".xml") {
            continue;
        }
        let declared: BTreeSet<&str> = data.match_indices("xmlns:").map(|(i, _)| {
            let rest = &data[i + 6..];
            &rest[..rest.find('=').unwrap()]
        }).collect();
        let mut used = BTreeSet::new();
        for (i, _) in data.match_indices('<') {
            let rest = &data[i + 1..];
            let tag = rest.trim_start_matches('/');
            let end = tag.find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(tag.len());
            let tag = &tag[..end];
            if let Some((prefix, _)) = tag.split_once(':') {
                if !prefix.starts_with('?') && !prefix.starts_with('!') {
                    used.insert(prefix.to_string());
                }
            }
            // The prefixed attributes of the tag.
            let close = rest.find('>').unwrap_or(rest.len());
            for part in rest[..close].split_whitespace().skip(1) {
                if let Some((prefix, _)) = part.split_once(':') {
                    if prefix != "xmlns" && part.contains('=') && !prefix.contains('"') {
                        used.insert(prefix.to_string());
                    }
                }
            }
        }
        for prefix in used {
            assert!(prefix == "xml" || declared.contains(prefix.as_str()), "{name}: {prefix}: is not declared");
        }
    }

    // Writing again gives the same parts.
    let again = parts(&write(&read(&saved)));
    assert_eq!(kinds(&again), kinds(&after));
}

#[test]
fn rows_and_columns_keep_their_groups() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("A3").set_value("x");
    sheet.row_dimension_mut(2).set_outline_level(1).set_hidden(true);
    sheet.row_dimension_mut(3).set_outline_level(2).set_collapsed(true);
    sheet.column_dimension_mut("B").set_outline_level(1);
    let bytes = write(&book);
    let xml = &parts(&bytes)["xl/worksheets/sheet1.xml"];
    assert!(xml.contains(r#"<row r="2" hidden="1" outlineLevel="1""#), "{xml}");
    assert!(xml.contains(r#"outlineLevel="2" collapsed="1""#));
    assert!(xml.contains(r#"outlineLevel="1"/>"#) || xml.contains(r#"outlineLevel="1" "#));
    let book = read(&bytes);
    let sheet = book.sheet(0).unwrap();
    assert_eq!(sheet.row_dimension(3).unwrap().outline_level(), 2);
    assert!(sheet.row_dimension(3).unwrap().collapsed());
    assert_eq!(sheet.column_dimension("B").unwrap().outline_level(), 1);
}

#[test]
fn furigana_can_be_set_and_read() {
    let mut book = umya_spreadsheet::new_file();
    let sheet = book.sheet_mut(0).unwrap();
    sheet.cell_mut("A1").set_value("東京都");
    sheet.cell_mut("A1").cell_value_mut().set_phonetic_runs(
        &[(0, 2, "とうきょう".to_string()), (2, 3, "と".to_string())],
        Some("Hiragana"),
    );
    sheet.cell_mut("A1").set_show_phonetic(true);
    sheet.cell_mut("A2").set_value("大阪");
    sheet.cell_mut("A2").cell_value_mut().set_phonetic_runs(&[(0, 2, "オオサカ".to_string())], None);
    // The text changed afterwards: the furigana are not of it.
    sheet.cell_mut("A3").set_value("名古屋");
    sheet.cell_mut("A3").cell_value_mut().set_phonetic_runs(&[(0, 3, "ナゴヤ".to_string())], None);
    sheet.cell_mut("A3").set_value("京都");
    // A number takes none.
    sheet.cell_mut("A4").set_value_number(5);
    sheet.cell_mut("A4").cell_value_mut().set_phonetic_runs(&[(0, 1, "ゴ".to_string())], None);

    let bytes = write(&book);
    let shared = &parts(&bytes)["xl/sharedStrings.xml"];
    assert!(shared.contains(r#"<rPh sb="0" eb="2"><t>とうきょう</t></rPh><rPh sb="2" eb="3"><t>と</t></rPh><phoneticPr fontId="1" type="Hiragana"/>"#), "{shared}");
    assert!(!shared.contains("ナゴヤ"));

    let book = read(&bytes);
    let sheet = book.sheet(0).unwrap();
    let a1 = sheet.cell("A1").unwrap();
    assert_eq!(a1.cell_value().phonetic_runs(), [(0, 2, "とうきょう".to_string()), (2, 3, "と".to_string())]);
    assert_eq!(a1.cell_value().phonetic_type().as_deref(), Some("Hiragana"));
    assert!(a1.show_phonetic());
    let a2 = sheet.cell("A2").unwrap();
    assert_eq!(a2.cell_value().phonetic_runs(), [(0, 2, "オオサカ".to_string())]);
    assert_eq!(a2.cell_value().phonetic_type().as_deref(), Some("fullwidthKatakana"));
    assert!(sheet.cell("A3").unwrap().cell_value().phonetic_runs().is_empty());
    assert!(sheet.cell("A4").unwrap().cell_value().phonetic_runs().is_empty());

    // As Excel wrote them.
    let book = read(&file("excel_features.xlsx"));
    let cell = book.sheet_by_name("その他").unwrap().cell("E1").unwrap();
    assert_eq!(cell.cell_value().phonetic_runs(), [(0, 2, "トウキョウ".to_string())]);
}

/// The ActiveX controls of a sheet (`xl/activeX`, the `control` of the
/// sheet's `controls`) were dropped while the sheet kept referring to them:
/// the `r:id` of the control pointed at no relationship. `activeX_controls.xlsm`
/// is RAS's test workbook (`crates/ras-workbook/src/view/tests.rs`): form
/// controls, an ActiveX CommandButton, a chart and a picture.
#[test]
fn activex_controls_survive_a_round_trip() {
    let before = parts(&file("activex_controls.xlsm"));
    let after = parts(&write(&read(&file("activex_controls.xlsm"))));
    for part in ["xl/activeX/activeX1.xml", "xl/activeX/activeX1.bin"] {
        assert!(after.contains_key(part), "{part}: {:?}", after.keys().collect::<Vec<_>>());
    }
    assert_eq!(after["xl/activeX/activeX1.bin"], before["xl/activeX/activeX1.bin"]);
    let sheet = &after["xl/worksheets/sheet1.xml"];
    let rels = &after[&rels_of("xl/worksheets/sheet1.xml")];
    for id in values(sheet, "r:id") {
        assert!(rels.contains(&format!("Id=\"{id}\"")), "{id} is not a relationship of the sheet");
    }
    assert!(sheet.contains(r#"name="CommandButton1""#));
}

/// The picture Excel keeps of an ActiveX control (the `r:id` of its
/// `controlPr`, an image of the sheet) goes with it.
#[test]
fn the_picture_of_an_activex_control_goes_with_it() {
    use std::io::Write as _;
    let source = file("activex_controls.xlsm");
    let mut archive = zip::ZipArchive::new(Cursor::new(&source)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        if name == "xl/worksheets/sheet1.xml" {
            let text = String::from_utf8(data).unwrap().replacen(
                r#"<control shapeId="1026" r:id="rId4" name="CommandButton1"><controlPr defaultSize="0" autoLine="0">"#,
                r#"<control shapeId="1026" r:id="rId4" name="CommandButton1"><controlPr defaultSize="0" autoLine="0" r:id="rId90">"#,
                1,
            );
            assert!(text.contains("rId90"));
            data = text.into_bytes();
        } else if name == "xl/worksheets/_rels/sheet1.xml.rels" {
            data = String::from_utf8(data)
                .unwrap()
                .replace(
                    "</Relationships>",
                    r#"<Relationship Id="rId90" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image90.emf"/></Relationships>"#,
                )
                .into_bytes();
        }
        out.start_file(name, options).unwrap();
        out.write_all(&data).unwrap();
    }
    out.start_file("xl/media/image90.emf", options).unwrap();
    out.write_all(b"EMF of the control").unwrap();
    let edited = out.finish().unwrap().into_inner();

    let after = parts(&write(&read(&edited)));
    let sheet = &after["xl/worksheets/sheet1.xml"];
    let rels = &after[&rels_of("xl/worksheets/sheet1.xml")];
    for id in values(sheet, "r:id") {
        assert!(rels.contains(&format!("Id=\"{id}\"")), "{id} is not a relationship of the sheet");
    }
    assert!(after.values().any(|d| d == "EMF of the control"));
}

/// A SmartArt diagram (the parts of `xl/diagrams` its frame refers to) was
/// dropped. RAS's test workbook has its data and drawing; the layout, the
/// quick style and the colors are added here as Excel would have them.
#[test]
fn smartart_survives_a_round_trip() {
    use std::io::Write as _;
    let source = file("activex_controls.xlsm");
    let mut archive = zip::ZipArchive::new(Cursor::new(&source)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        out.start_file(name, options).unwrap();
        out.write_all(&data).unwrap();
    }
    let dgm = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
    for (part, root) in [("layout1.xml", "layoutDef"), ("quickStyle1.xml", "styleDef"), ("colors1.xml", "colorsDef")] {
        out.start_file(format!("xl/diagrams/{part}"), options).unwrap();
        write!(out, r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><dgm:{root} xmlns:dgm="{dgm}" uniqueId="{part}"/>"#).unwrap();
    }
    let edited = out.finish().unwrap().into_inner();

    let after = parts(&write(&read(&edited)));
    for part in ["xl/diagrams/data1.xml", "xl/diagrams/layout1.xml", "xl/diagrams/quickStyle1.xml", "xl/diagrams/colors1.xml"] {
        assert!(after.contains_key(part), "{part}: {:?}", after.keys().collect::<Vec<_>>());
    }
    let drawing = &after["xl/drawings/drawing1.xml"];
    let rels = &after[&rels_of("xl/drawings/drawing1.xml")];
    assert!(drawing.contains("drawingml/2006/diagram"));
    for attr in ["r:dm", "r:lo", "r:qs", "r:cs"] {
        for id in values(drawing, attr) {
            assert!(rels.contains(&format!("Id=\"{id}\"")), "{attr}={id} is not a relationship of the drawing");
        }
    }
    // The drawing of the diagram (what Excel draws until it lays the diagram
    // out again) goes with it, under the id its data names.
    assert!(after.contains_key("xl/diagrams/drawing1.xml"), "{:?}", after.keys().collect::<Vec<_>>());
    let rel_id = values(&after["xl/diagrams/data1.xml"], "relId")[0].to_string();
    assert!(
        rels.contains(&format!(r#"Id="{rel_id}" Type="http://schemas.microsoft.com/office/2007/relationships/diagramDrawing" Target="/xl/diagrams/drawing1.xml""#)),
        "{rel_id}: {rels}"
    );
    // The chart of the same drawing is still there.
    assert!(after.keys().any(|k| k.starts_with("xl/charts/chart")));
    // And again.
    let again = parts(&write(&read(&write(&read(&edited)))));
    assert_eq!(values(&again["xl/diagrams/data1.xml"], "relId"), [rel_id.as_str()]);
    assert!(again[&rels_of("xl/drawings/drawing1.xml")].contains(&format!("Id=\"{rel_id}\"")));
}
