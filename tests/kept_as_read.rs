//! What the model has nothing for (or leaves out some of), kept as it was
//! read and written back: the connections and query tables of external data,
//! the parts of the package (the sensitivity label, the ribbon), custom views
//! and the other elements of the workbook and its sheets, the attributes of
//! names and links, shapes and pictures, the theme, the frames of comments.
use std::{
    collections::BTreeMap,
    io::{
        Cursor,
        Read,
        Write,
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

fn round_trip(bytes: &[u8]) -> Vec<u8> {
    write(&read(bytes))
}

/// The text parts of the package.
fn parts(file: &[u8]) -> BTreeMap<String, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        out.insert(f.name().to_string(), String::from_utf8_lossy(&data).into_owned());
    }
    out
}

/// The package with `edit` applied to the text of its parts, and the parts
/// of `add` added.
fn edit(file: &[u8], edit: impl Fn(&str, String) -> String, add: &[(&str, &str)]) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let data = match String::from_utf8(data) {
            Ok(text) => edit(&name, text).into_bytes(),
            Err(e) => e.into_bytes(),
        };
        out.start_file(name, options).unwrap();
        out.write_all(&data).unwrap();
    }
    for (name, text) in add {
        out.start_file(*name, options).unwrap();
        out.write_all(text.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// The values of the attribute `attr` in the text.
fn values<'a>(text: &'a str, attr: &str) -> Vec<&'a str> {
    let key = format!(" {attr}=\"");
    text.match_indices(&key)
        .map(|(i, _)| {
            let rest = &text[i + key.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect()
}

fn rels_of(name: &str) -> String {
    let (dir, file) = name.rsplit_once('/').unwrap();
    format!("{dir}/_rels/{file}.rels")
}

/// Whether every `r:id` (and `attr`) of the part is one of its relationships.
fn ids_resolve(p: &BTreeMap<String, String>, name: &str, attrs: &[&str]) {
    let rels = p.get(&rels_of(name)).cloned().unwrap_or_default();
    for attr in attrs {
        for id in values(&p[name], attr) {
            assert!(rels.contains(&format!("Id=\"{id}\"")), "{name}: {attr}={id} is not a relationship: {rels}");
        }
    }
}

fn index(text: &str, what: &str) -> usize {
    text.find(what).unwrap_or_else(|| panic!("no {what} in {text}"))
}

#[test]
fn connections_and_query_tables_of_external_data() {
    let before = parts(&file("issue_363.xlsx"));
    let after = parts(&round_trip(&file("issue_363.xlsx")));
    assert_eq!(after["xl/connections.xml"], before["xl/connections.xml"]);
    assert!(after["xl/_rels/workbook.xml.rels"].contains("relationships/connections"));
    assert!(after["[Content_Types].xml"].contains(r#"PartName="/xl/connections.xml""#));
    // The table goes on to its query table.
    let table = after.keys().find(|k| k.starts_with("xl/tables/table")).unwrap().clone();
    assert!(after[&table].contains(r#"tableType="queryTable""#));
    let rels = &after[&rels_of(&table)];
    assert!(rels.contains("relationships/queryTable"), "{rels}");
    let target = values(rels, "Target")[0].trim_start_matches('/').to_string();
    assert!(after[&target].contains("<queryTable "), "{target}");
    assert!(after["[Content_Types].xml"].contains(&format!(r#"PartName="/{target}""#)));
}

#[test]
fn the_sensitivity_label_of_the_package() {
    let before = parts(&file("issue_281.xlsx"));
    let after = parts(&round_trip(&file("issue_281.xlsx")));
    assert_eq!(after["docMetadata/LabelInfo.xml"], before["docMetadata/LabelInfo.xml"]);
    assert!(after["_rels/.rels"].contains(r#"Type="http://schemas.microsoft.com/office/2020/02/relationships/classificationlabels" Target="/docMetadata/LabelInfo.xml""#));
    assert!(after["[Content_Types].xml"].contains(r#"PartName="/docMetadata/LabelInfo.xml""#));
}

#[test]
fn the_ribbon_and_its_pictures() {
    let ui = r#"<customUI xmlns="http://schemas.microsoft.com/office/2009/07/customui"><ribbon><tabs><tab id="t" label="業務"><group id="g" label="g"><button id="b" image="icon" onAction="Run"/></group></tab></tabs></ribbon></customUI>"#;
    let source = edit(
        &file("aaa.xlsm"),
        |name, text| match name {
            "_rels/.rels" => text.replace(
                "</Relationships>",
                r#"<Relationship Id="rIdUI" Type="http://schemas.microsoft.com/office/2007/relationships/ui/extensibility" Target="customUI/customUI14.xml"/></Relationships>"#,
            ),
            "[Content_Types].xml" => text.replace("</Types>", r#"<Default Extension="png" ContentType="image/png"/></Types>"#),
            _ => text,
        },
        &[
            ("customUI/customUI14.xml", ui),
            (
                "customUI/_rels/customUI14.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="icon" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="images/icon.png"/></Relationships>"#,
            ),
            ("customUI/images/icon.png", "PNG"),
        ],
    );
    let after = parts(&round_trip(&source));
    assert_eq!(after["customUI/customUI14.xml"], ui);
    assert!(after["_rels/.rels"].contains("ui/extensibility"));
    assert!(after["customUI/_rels/customUI14.xml.rels"].contains(r#"Id="icon""#));
    assert_eq!(after["customUI/images/icon.png"], "PNG");
    // The workbook and its properties are not written twice.
    assert_eq!(after["_rels/.rels"].matches("relationships/officeDocument\"").count(), 1);
}

#[test]
fn custom_views_with_their_printer_settings() {
    let before = parts(&file("issue_296.xlsx"));
    let after = parts(&round_trip(&file("issue_296.xlsx")));
    let workbook = &after["xl/workbook.xml"];
    assert!(workbook.contains("<customWorkbookView "), "{workbook}");
    assert!(index(workbook, "<calcPr") < index(workbook, "<customWorkbookViews"));
    let sheet = &after["xl/worksheets/sheet1.xml"];
    let views = before["xl/worksheets/sheet1.xml"].matches("<customSheetView ").count();
    assert!(views > 1);
    assert_eq!(sheet.matches("<customSheetView ").count(), views, "{sheet}");
    assert!(index(sheet, "</sheetData>") < index(sheet, "<customSheetViews"));
    // (Each view has page margins of its own.)
    assert!(index(sheet, "</customSheetViews>") < sheet.rfind("<pageMargins").unwrap());
    ids_resolve(&after, "xl/worksheets/sheet1.xml", &["r:id"]);
    assert_eq!(after.keys().filter(|k| k.starts_with("xl/printerSettings/")).count(), 3);
    // Read again, the ids written (`rIdKept2`...) stay those of their parts.
    let again = parts(&round_trip(&round_trip(&file("issue_296.xlsx"))));
    ids_resolve(&again, "xl/worksheets/sheet1.xml", &["r:id"]);
    assert_eq!(again["xl/worksheets/sheet1.xml"], after["xl/worksheets/sheet1.xml"]);
    assert_eq!(again["xl/worksheets/_rels/sheet1.xml.rels"], after["xl/worksheets/_rels/sheet1.xml.rels"]);
}

#[test]
fn elements_of_sheets_the_model_has_nothing_for() {
    let mut book = umya_spreadsheet::new_file();
    book.sheet_mut(0).unwrap().cell_mut("A1").set_value("1");
    let source = edit(
        &write(&book),
        |name, text| {
            if name != "xl/worksheets/sheet1.xml" {
                return text;
            }
            let text = text.replacen(
                "</sheetData>",
                r#"</sheetData><sheetCalcPr fullCalcOnLoad="1"/><protectedRanges><protectedRange sqref="A1:B2" name="入力欄"/></protectedRanges>"#,
                1,
            );
            let i = text.find("<pageMargins").unwrap();
            let end = i + text[i..].find("/>").unwrap() + 2;
            format!(
                r#"{}<ignoredErrors><ignoredError sqref="A1" numberStoredAsText="1"/></ignoredErrors>{}"#,
                &text[..end],
                &text[end..]
            )
            .replacen("<sheetPr", r#"<sheetPr filterMode="1""#, 1)
            .replacen(
                "<sheetFormatPr",
                r#"<sheetPr filterMode="1"><outlinePr summaryBelow="0" summaryRight="0"/></sheetPr><sheetFormatPr"#,
                usize::from(!text.contains("<sheetPr")),
            )
        },
        &[],
    );
    let after = parts(&round_trip(&source));
    let sheet = &after["xl/worksheets/sheet1.xml"];
    for what in [
        r#"<sheetCalcPr fullCalcOnLoad="1"/>"#,
        r#"<protectedRange sqref="A1:B2" name="入力欄"/>"#,
        r#"<ignoredError sqref="A1" numberStoredAsText="1"/>"#,
        r#"filterMode="1""#,
    ] {
        assert!(sheet.contains(what), "{what}: {sheet}");
    }
    // In the order of the schema.
    assert!(index(sheet, "</sheetData>") < index(sheet, "<sheetCalcPr"));
    assert!(index(sheet, "<sheetCalcPr") < index(sheet, "<protectedRanges>"));
    assert!(index(sheet, "<protectedRanges>") < index(sheet, "<pageMargins"));
    assert!(index(sheet, "<pageMargins") < index(sheet, "<ignoredErrors>"));
}

#[test]
fn where_the_summary_rows_of_groups_are() {
    let mut book = umya_spreadsheet::new_file();
    book.sheet_mut(0).unwrap().cell_mut("A1").set_value("1");
    let source = edit(
        &write(&book),
        |name, text| {
            if name != "xl/worksheets/sheet1.xml" {
                return text;
            }
            assert!(!text.contains("<sheetPr"));
            text.replacen("<dimension", r#"<sheetPr><outlinePr summaryBelow="0"/></sheetPr><dimension"#, 1)
        },
        &[],
    );
    let after = parts(&round_trip(&source));
    assert!(after["xl/worksheets/sheet1.xml"].contains(r#"<sheetPr><outlinePr summaryBelow="0"/></sheetPr>"#));
}

#[test]
fn attributes_of_the_workbook_and_its_names() {
    let after = parts(&round_trip(&file("issue_216.xlsx")));
    assert!(after["xl/workbook.xml"].contains(r#"<fileRecoveryPr repairLoad="1"/>"#));

    let book = read(&file("issue_200.xlsx"));
    assert_eq!(book.workbook_pr_attribute("showObjects"), Some("all"));
    let after = parts(&write(&book));
    assert!(after["xl/workbook.xml"].contains(r#"showObjects="all""#), "{}", after["xl/workbook.xml"]);

    let after = parts(&round_trip(&file("libre.xlsm")));
    let workbook = &after["xl/workbook.xml"];
    assert_eq!(values(workbook, "vbProcedure").len(), values(&parts(&file("libre.xlsm"))["xl/workbook.xml"], "vbProcedure").len());
    assert!(workbook.contains(r#"function="false""#));

    // A macro registered as a name.
    let mut book = umya_spreadsheet::new_file();
    book.sheet_mut(0).unwrap().cell_mut("A1").set_value("1");
    let mut defined = umya_spreadsheet::DefinedName::default();
    defined.set_name("Run").set_address("Sheet1!$A$1");
    book.add_defined_names(defined);
    let source = edit(
        &write(&book),
        |name, text| {
            if name == "xl/workbook.xml" {
                text.replace(r#"<definedName name="Run""#, r#"<definedName name="Run" function="1" vbProcedure="1" comment="A &amp; B""#)
            } else {
                text
            }
        },
        &[],
    );
    let after = parts(&round_trip(&source));
    let workbook = &after["xl/workbook.xml"];
    assert!(workbook.contains(r#"function="1""#) && workbook.contains(r#"vbProcedure="1""#), "{workbook}");
    assert!(workbook.contains(r#"comment="A &amp; B""#), "{workbook}");
}

#[test]
fn the_text_of_a_link() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    ws.cell_mut("A1").set_value("site");
    ws.cell_mut("A1").hyperlink_mut().set_url("https://example.com/").set_display("サイト");
    let book = read(&write(&book));
    let link = book.sheet(0).unwrap().cell("A1").unwrap().hyperlink().unwrap().clone();
    assert_eq!(link.display(), "サイト");
    assert!(parts(&write(&book))["xl/worksheets/sheet1.xml"].contains(r#"display="サイト""#));
}

/// The anchors of the drawing.
fn anchors(drawing: &str) -> usize {
    drawing.matches("<xdr:twoCellAnchor").count() + drawing.matches("<xdr:oneCellAnchor").count()
}

#[test]
fn shapes_are_written_as_they_were_read() {
    // Named colors, the levels of paragraphs, what Excel 2010 hides.
    for (name, what) in [
        ("issue_288.xlsx", r#"<a:prstClr val="black">"#),
        ("issue_188_2.xlsx", "<a:lvl2pPr"),
        ("issue_188.xlsx", "<a14:hiddenFill"),
    ] {
        let before = parts(&file(name));
        let after = parts(&round_trip(&file(name)));
        let drawing = &after["xl/drawings/drawing1.xml"];
        assert_eq!(
            drawing.matches(what).count(),
            before["xl/drawings/drawing1.xml"].matches(what).count(),
            "{name}: {drawing}"
        );
        assert_eq!(anchors(drawing), anchors(&before["xl/drawings/drawing1.xml"]), "{name}");
        ids_resolve(&after, "xl/drawings/drawing1.xml", &["r:embed", "r:id", "r:link"]);
        // And again.
        let again = parts(&round_trip(&round_trip(&file(name))));
        assert_eq!(again["xl/drawings/drawing1.xml"], *drawing, "{name}");
    }
}

#[test]
fn shapes_move_with_rows_and_change_with_the_model() {
    let mut book = read(&file("issue_288.xlsx"));
    let name = book.sheet(0).unwrap().name().to_string();
    book.insert_new_row(&name, 1, 2);
    let after = parts(&write(&book));
    let drawing = &after["xl/drawings/drawing1.xml"];
    // Still as read, two rows down.
    assert!(drawing.contains(r#"<a:prstClr val="black">"#), "{drawing}");
    assert!(drawing.contains("<xdr:from><xdr:col>2</xdr:col><xdr:colOff>139064</xdr:colOff><xdr:row>2</xdr:row>"), "{drawing}");

    // Changed: written from the model.
    let mut book = read(&file("issue_288.xlsx"));
    let anchor = &mut book.sheet_mut(0).unwrap().worksheet_drawing_mut().two_cell_anchor_collection_mut()[0];
    anchor.from_marker_mut().set_col(5);
    let drawing = parts(&write(&book))["xl/drawings/drawing1.xml"].clone();
    assert!(drawing.contains("<xdr:from><xdr:col>5</xdr:col>"), "{drawing}");
}

#[test]
fn pictures_are_written_as_they_were_read() {
    let before = parts(&file("issue_190.xlsx"));
    let after = parts(&round_trip(&file("issue_190.xlsx")));
    let drawing = &after["xl/drawings/drawing1.xml"];
    assert!(drawing.contains("<a14:hiddenFill"), "{drawing}");
    assert!(drawing.contains("<a:picLocks noChangeAspect=\"1\" noChangeArrowheads=\"1\"/>"), "{drawing}");
    ids_resolve(&after, "xl/drawings/drawing1.xml", &["r:embed", "r:id"]);
    let rels = &after[&rels_of("xl/drawings/drawing1.xml")];
    for target in values(rels, "Target").into_iter().filter(|t| t.contains("media")) {
        let path = format!("xl/{}", target.trim_start_matches("../").trim_start_matches("/xl/"));
        assert!(after.contains_key(&path), "{path}");
    }
    assert_eq!(
        after.keys().filter(|k| k.starts_with("xl/media/")).count(),
        before.keys().filter(|k| k.starts_with("xl/media/")).count()
    );
}

#[test]
fn the_theme_is_written_as_it_was_read() {
    let before = parts(&file("aaa.xlsx"));
    let after = parts(&round_trip(&file("aaa.xlsx")));
    let theme = before.keys().find(|k| k.starts_with("xl/theme/")).unwrap();
    assert_eq!(after["xl/theme/theme1.xml"], before[theme]);
    assert!(after["xl/theme/theme1.xml"].contains("<a:fillToRect"));

    // Changed: written from the model.
    let mut book = read(&file("aaa.xlsx"));
    book.theme_mut().set_name("別のテーマ");
    let theme = parts(&write(&book))["xl/theme/theme1.xml"].clone();
    assert!(theme.contains(r#"name="別のテーマ""#), "{theme}");
}

#[test]
fn the_frames_of_comments_are_written_as_they_were_read() {
    let after = parts(&round_trip(&file("libre2.xlsx")));
    let vml = &after["xl/drawings/vmlDrawing1.vml"];
    assert!(vml.contains(r#"startarrow="block""#), "{vml}");
    assert!(vml.contains("<x:TextHAlign>Left</x:TextHAlign>"), "{vml}");
    // Each with an id of its own.
    let ids = values(vml, "id").into_iter().filter(|i| i.starts_with("_x0000_s")).collect::<Vec<_>>();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "{vml}");
    assert!(!vml.contains("shape_0"));

    // They follow their cells.
    let mut book = read(&file("libre2.xlsx"));
    let name = book.sheet(0).unwrap().name().to_string();
    book.insert_new_row(&name, 1, 3);
    let vml = parts(&write(&book))["xl/drawings/vmlDrawing1.vml"].clone();
    assert!(vml.contains(r#"startarrow="block""#), "{vml}");
    assert!(vml.contains("<x:Row>11</x:Row>"), "{vml}");
    assert!(!vml.contains("<x:Row>8</x:Row>"), "{vml}");
}
