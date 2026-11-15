//! Charts read from a file are written back as they were read: the chart
//! parts (`xl/charts/chart1.xml`) byte for byte, with the parts they refer
//! to (`style1.xml`, `colors1.xml`), whether the model of this crate knows
//! the chart or not. A chart changed through the model is written from the
//! model.
//!
//! - `aaa.xlsx`: charts made by Excel (`xdr:twoCellAnchor`) on Sheet1 and
//!   Sheet2, referring to `Sheet1!$G$7:$G$10` and others.
//! - `issue_190.xlsx`: two charts by Excel with their styles and colors.
//! - `charts_one_cell_anchor.xlsx` (made with openpyxl): twelve charts in
//!   `oneCellAnchor`s, which the model does not read, the first referring to
//!   `'売上'!B1`.
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
    structs::{
        Chart,
        ChartType,
        drawing::spreadsheet::MarkerType,
    },
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

fn parts(file: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut map = BTreeMap::new();
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        map.insert(f.name().to_string(), data);
    }
    map
}

fn text(parts: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
    String::from_utf8(parts[name].clone()).unwrap()
}

/// The contents of the parts whose names start with `prefix` (in
/// `xl/charts/`), sorted: what must be the same before and after.
fn contents(parts: &BTreeMap<String, Vec<u8>>, prefix: &str) -> Vec<Vec<u8>> {
    let mut list: Vec<Vec<u8>> = parts
        .iter()
        .filter(|(name, _)| name.starts_with(prefix) && !name.contains("_rels"))
        .map(|(_, data)| data.clone())
        .collect();
    list.sort();
    list
}

/// The relationships of a part: (id, type, target), the target as a name in
/// the package.
fn relationships(parts: &BTreeMap<String, Vec<u8>>, part: &str) -> Vec<(String, String, String)> {
    let (dir, name) = part.rsplit_once('/').unwrap();
    let Some(data) = parts.get(&format!("{dir}/_rels/{name}.rels")) else {
        return Vec::new();
    };
    let xml = String::from_utf8(data.clone()).unwrap();
    let attr = |tag: &str, key: &str| -> String {
        let start = tag.find(&format!("{key}=\"")).unwrap() + key.len() + 2;
        tag[start..].split('"').next().unwrap().to_string()
    };
    xml.split("<Relationship ")
        .skip(1)
        .map(|tag| {
            let target = attr(tag, "Target");
            let target = match target.strip_prefix('/') {
                Some(t) => t.to_string(),
                None => {
                    let mut path: Vec<&str> = dir.split('/').collect();
                    for seg in target.split('/') {
                        match seg {
                            ".." => {
                                path.pop();
                            }
                            s => path.push(s),
                        }
                    }
                    path.join("/")
                }
            };
            (attr(tag, "Id"), attr(tag, "Type"), target)
        })
        .collect()
}

/// Every relationship of the drawings and charts leads to a part with a
/// content type, and every `r:id` of a drawing to a relationship.
fn assert_linked(parts: &BTreeMap<String, Vec<u8>>) {
    let types = text(parts, "[Content_Types].xml");
    for name in parts.keys() {
        let is_drawing = name.starts_with("xl/drawings/drawing") && name.ends_with(".xml");
        let is_chart = name.starts_with("xl/charts/") && !name.contains("_rels");
        if !is_drawing && !is_chart {
            continue;
        }
        assert!(
            types.contains(&format!("PartName=\"/{name}\"")),
            "no content type for {name}"
        );
        let rels = relationships(parts, name);
        for (id, _, target) in &rels {
            assert!(
                parts.contains_key(target),
                "{name}: {id} -> {target} missing"
            );
        }
        if is_drawing {
            let xml = text(parts, name);
            for piece in xml.split("r:id=\"").skip(1) {
                let id = piece.split('"').next().unwrap();
                assert!(
                    rels.iter().any(|(i, ..)| i == id),
                    "{name}: {id} has no relationship"
                );
            }
        }
    }
}

#[test]
fn charts_by_excel_come_back_as_they_were() {
    for name in ["aaa.xlsx", "issue_190.xlsx"] {
        let before = parts(&file(name));
        let after = parts(&write(&read(&file(name))));
        assert_eq!(
            contents(&before, "xl/charts/"),
            contents(&after, "xl/charts/"),
            "{name}: the chart parts changed"
        );
        assert_linked(&after);
    }
    // The styles and colors are still the chart's.
    let after = parts(&write(&read(&file("issue_190.xlsx"))));
    let rels = relationships(&after, "xl/charts/chart1.xml");
    let kinds: Vec<&str> = rels
        .iter()
        .map(|(_, t, _)| t.rsplit('/').next().unwrap())
        .collect();
    assert!(kinds.contains(&"chartStyle"), "{kinds:?}");
    assert!(kinds.contains(&"chartColorStyle"), "{kinds:?}");
    let types = text(&after, "[Content_Types].xml");
    assert!(
        types.contains("application/vnd.ms-office.chartstyle+xml"),
        "{types}"
    );
    assert!(
        types.contains("application/vnd.ms-office.chartcolorstyle+xml"),
        "{types}"
    );
}

#[test]
fn charts_the_model_does_not_read_are_kept() {
    let name = "charts_one_cell_anchor.xlsx";
    let book = read(&file(name));
    let sheet = book.sheet_by_name("売上").unwrap();
    assert!(sheet.worksheet_drawing().chart_collection().is_empty());
    assert_eq!(sheet.worksheet_drawing().kept_anchor_collection().len(), 9);

    let before = parts(&file(name));
    let after = parts(&write(&book));
    assert_eq!(
        contents(&before, "xl/charts/"),
        contents(&after, "xl/charts/")
    );
    assert_linked(&after);
    // The anchors are as they were, declaring the namespace they had from
    // the root of the drawing.
    let drawing = text(&after, "xl/drawings/drawing1.xml");
    assert_eq!(
        drawing.matches("<oneCellAnchor xmlns=").count(),
        9,
        "{drawing}"
    );
    assert!(drawing.contains("<from><col>5</col><colOff>0</colOff><row>1</row>"));

    // And once more.
    let again = parts(&write(&read(&write(&book))));
    assert_eq!(
        contents(&before, "xl/charts/"),
        contents(&again, "xl/charts/")
    );
}

#[test]
fn a_chart_changed_through_the_model_is_written_from_it() {
    let mut book = read(&file("issue_190.xlsx"));
    let mut changed = 0;
    for sheet in book.sheet_collection_mut() {
        if let Some(chart) = sheet.chart_collection_mut().first_mut() {
            chart
                .chart_space_mut()
                .chart_mut()
                .set_legend_present(false);
            changed += 1;
        }
    }
    assert_eq!(changed, 2);
    let after = parts(&write(&book));
    let charts: Vec<String> = after
        .keys()
        .filter(|n| n.starts_with("xl/charts/chart"))
        .map(|n| text(&after, n))
        .collect();
    assert_eq!(charts.len(), 2);
    for chart in &charts {
        assert!(!chart.contains("<c:legend>"), "{chart}");
    }
    assert_linked(&after);
}

#[test]
fn a_new_chart_beside_kept_ones() {
    let mut book = read(&file("charts_one_cell_anchor.xlsx"));
    let mut from = MarkerType::default();
    from.set_coordinate("K2");
    let mut to = MarkerType::default();
    to.set_coordinate("P12");
    let mut chart = Chart::default();
    chart.new_chart(&ChartType::LineChart, from, to, vec!["売上!$B$2:$B$4"]);
    book.sheet_by_name_mut("売上").unwrap().add_chart(chart);

    let before = parts(&file("charts_one_cell_anchor.xlsx"));
    let after = parts(&write(&book));
    let charts = after
        .keys()
        .filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml") && !n.contains("_rels"))
        .count();
    assert_eq!(charts, 13);
    // The kept ones are all there, as they were.
    let kept = contents(&before, "xl/charts/");
    let written = contents(&after, "xl/charts/");
    for part in &kept {
        assert!(written.contains(part));
    }
    assert_linked(&after);
}

#[test]
fn inserted_rows_move_kept_charts_and_their_references() {
    // openpyxl: the first chart is at F2 (row 1, 0-based) and refers to
    // '売上'!B1 and '売上'!$B$2:$B$4.
    let mut book = read(&file("charts_one_cell_anchor.xlsx"));
    book.insert_new_row("売上", 2, 3);
    let after = parts(&write(&book));
    let drawing = text(&after, "xl/drawings/drawing1.xml");
    assert!(
        drawing.contains("<from><col>5</col><colOff>0</colOff><row>4</row>"),
        "{drawing}"
    );
    let rels = relationships(&after, "xl/drawings/drawing1.xml");
    let first = drawing
        .split("r:id=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap();
    let chart = &rels.iter().find(|(id, ..)| id == first).unwrap().2;
    let chart = text(&after, chart);
    assert!(chart.contains("<f>'売上'!B1</f>"), "{chart}");
    assert!(chart.contains("<f>'売上'!$B$5:$B$7</f>"), "{chart}");

    // Excel's charts (in the model too) on another sheet than the cells.
    let mut book = read(&file("aaa.xlsx"));
    book.insert_new_row("Sheet1", 8, 2);
    let after = parts(&write(&book));
    let all: String = after
        .iter()
        .filter(|(n, _)| n.starts_with("xl/charts/chart"))
        .map(|(_, d)| String::from_utf8_lossy(d).to_string())
        .collect();
    assert!(all.contains("<c:f>Sheet1!$G$7:$G$12</c:f>"), "{all}");
    assert!(!all.contains("<c:f>Sheet1!$G$7:$G$10</c:f>"));
    assert_linked(&after);
}

#[test]
fn inserted_rows_keep_the_rest_of_excel_charts() {
    // Only the references change: the rest of the chart is as it was read,
    // not as the model would write it.
    let before = parts(&file("aaa.xlsx"));
    let mut book = read(&file("aaa.xlsx"));
    book.insert_new_row("Sheet1", 8, 2);
    let after = parts(&write(&book));
    let moved = |data: &[u8]| {
        String::from_utf8_lossy(data)
            .replace("$10<", "$12<")
            .replace("$10,", "$12,")
    };
    let mut expected: Vec<String> = contents(&before, "xl/charts/")
        .iter()
        .map(|d| moved(d))
        .collect();
    let mut written: Vec<String> = contents(&after, "xl/charts/")
        .iter()
        .map(|d| String::from_utf8_lossy(d).to_string())
        .collect();
    expected.sort();
    written.sort();
    assert_eq!(expected.len(), written.len());
    for (e, w) in expected.iter().zip(&written) {
        assert_eq!(e.len(), w.len());
    }
}
