//! What the workbook says besides the cells, read and written again as it
//! was, and readable through the model (instead of from the file, as xljs
//! and RAS did): hyperlinks to a place in another file, the print options and
//! page setup, the sheet view, the attributes of tables and their columns,
//! the table styles of the workbook, the Normal style's font and format.
use std::io::{
    Cursor,
    Read,
    Write,
};

use umya_spreadsheet::{
    EnumTrait,
    Table,
    Workbook,
    reader,
    writer,
};

fn write(book: &Workbook) -> Vec<u8> {
    let mut out = Vec::new();
    writer::xlsx::write_writer(book, &mut out).unwrap();
    out
}

fn read(bytes: &[u8]) -> Workbook {
    reader::xlsx::read_reader(Cursor::new(bytes), true).unwrap()
}

fn part(file: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut data = String::new();
    archive.by_name(name).unwrap().read_to_string(&mut data).unwrap();
    data
}

/// The package with `edit` applied to the text of its parts.
fn edit(file: &[u8], edit: impl Fn(&str, String) -> String) -> Vec<u8> {
    let mut archive = zip::ZipArchive::new(Cursor::new(file)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..archive.len() {
        let mut f = archive.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        f.read_to_end(&mut data).unwrap();
        let data = match String::from_utf8(data) {
            Ok(text) => edit(&name, text).into_bytes(),
            Err(e) => e.into_bytes(),
        };
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&data).unwrap();
    }
    out.finish().unwrap().into_inner()
}

/// A workbook with a table on A1:B3 of its first sheet.
fn with_table() -> Vec<u8> {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    ws.cell_mut("A1").set_value("Name");
    ws.cell_mut("B1").set_value("Count");
    ws.cell_mut("A2").set_value("a");
    ws.cell_mut("B2").set_value_number(1);
    let mut table = Table::new("Table1", ("A1", "B3"));
    table.add_column(umya_spreadsheet::TableColumn::new("Name"));
    table.add_column(umya_spreadsheet::TableColumn::new("Count"));
    ws.add_table(table);
    write(&book)
}

#[test]
fn hyperlinks_to_a_place_in_another_file() {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_mut(0).unwrap();
    ws.cell_mut("A1").hyperlink_mut().set_url("other.xlsx").set_target_location("Sheet2!B3");
    ws.cell_mut("A2").hyperlink_mut().set_url("Sheet1!C1").set_location(true);
    let file = write(&book);
    let sheet = part(&file, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"location="Sheet2!B3""#), "{sheet}");
    assert!(sheet.contains(r#"<hyperlink ref="A2" location="Sheet1!C1""#), "{sheet}");

    let book = read(&file);
    let ws = book.sheet(0).unwrap();
    let a1 = ws.cell("A1").unwrap().hyperlink().unwrap();
    assert_eq!((a1.url(), a1.location(), a1.target_location()), ("other.xlsx", false, "Sheet2!B3"));
    let a2 = ws.cell("A2").unwrap().hyperlink().unwrap();
    assert_eq!((a2.url(), a2.location(), a2.target_location()), ("Sheet1!C1", true, ""));
}

#[test]
fn a_hyperlink_on_a_merged_area_is_read() {
    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/worksheets/sheet1.xml" {
            text.replace(
                "<pageMargins",
                r#"<hyperlinks><hyperlink ref="B2:C3" location="Sheet1!A1"/><hyperlink location="Sheet1!A1"/></hyperlinks><pageMargins"#,
            )
        } else {
            text
        }
    });
    let book = read(&file);
    let link = book.sheet(0).unwrap().cell("B2").unwrap().hyperlink().unwrap();
    assert_eq!(link.url(), "Sheet1!A1");
}

#[test]
fn print_options_and_page_setup() {
    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/worksheets/sheet1.xml" {
            text.replace(
                "<pageMargins",
                r#"<printOptions headings="1" gridLines="1"/><pageMargins"#,
            )
            .replace(
                "</worksheet>",
                r#"<pageSetup paperSize="9" fitToHeight="0" pageOrder="overThenDown" blackAndWhite="1" firstPageNumber="5" useFirstPageNumber="1"/></worksheet>"#,
            )
        } else {
            text
        }
    });
    let book = read(&file);
    let ws = book.sheet(0).unwrap();
    assert!(ws.print_options().grid_lines() && ws.print_options().headings());
    let ps = ws.page_setup();
    assert!(!ps.has_fit_to_width() && ps.has_fit_to_height());
    assert_eq!(ps.fit_to_height(), 0);
    assert_eq!(ps.page_order(), "overThenDown");
    assert!(ps.black_and_white());
    assert_eq!(ps.first_page_number(), Some(5));

    let again = write(&book);
    let sheet = part(&again, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"headings="1""#) && sheet.contains(r#"gridLines="1""#), "{sheet}");
    assert!(sheet.contains(r#"pageOrder="overThenDown""#) && sheet.contains(r#"useFirstPageNumber="1""#), "{sheet}");

    let mut book = read(&again);
    let ps = book.sheet_mut(0).unwrap().page_setup_mut();
    ps.set_black_and_white(false).set_page_order("downThenOver").set_first_page_number(None);
    let sheet = part(&write(&book), "xl/worksheets/sheet1.xml");
    assert!(!sheet.contains("blackAndWhite") && !sheet.contains("pageOrder") && !sheet.contains("FirstPageNumber"), "{sheet}");
}

#[test]
fn the_sheet_view() {
    let plain = read(&write(&umya_spreadsheet::new_file()));
    let view = &plain.sheet(0).unwrap().sheets_views().sheet_view_list()[0];
    // Shown unless the file says otherwise.
    assert!(view.show_grid_lines() && view.show_row_col_headers() && view.show_zeros());
    assert_eq!(view.grid_color_index(), None);

    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/worksheets/sheet1.xml" {
            text.replacen(
                "<sheetView ",
                r#"<sheetView showRowColHeaders="0" showZeros="0" rightToLeft="1" showFormulas="1" defaultGridColor="0" colorId="10" showRuler="0" "#,
                1,
            )
        } else {
            text
        }
    });
    let book = read(&file);
    let view = &book.sheet(0).unwrap().sheets_views().sheet_view_list()[0];
    assert!(!view.show_row_col_headers() && !view.show_zeros() && view.right_to_left() && view.show_formulas());
    assert_eq!(view.grid_color_index(), Some(10));
    let sheet = part(&write(&book), "xl/worksheets/sheet1.xml");
    for attr in [
        r#"showRowColHeaders="0""#,
        r#"showZeros="0""#,
        r#"rightToLeft="1""#,
        r#"showFormulas="1""#,
        r#"defaultGridColor="0""#,
        r#"colorId="10""#,
        r#"showRuler="0""#,
    ] {
        assert!(sheet.contains(attr), "{attr}: {sheet}");
    }
}

#[test]
fn the_attributes_of_tables() {
    let file = edit(&with_table(), |name, text| {
        if name.starts_with("xl/tables/") {
            text.replacen(r#" ref="A1:B3""#, r#" ref="A1:B3" headerRowCount="0" dataDxfId="2" insertRow="1""#, 1)
                .replacen(r#"name="Count""#, r#"name="Count" dataDxfId="1""#, 1)
                .replacen(
                    r#"totalsRowFunction="none"/>"#,
                    r#"totalsRowFunction="none"><totalsRowFormula>SUM(Table1[Count])</totalsRowFormula></tableColumn>"#,
                    2,
                )
        } else {
            text
        }
    });
    let book = read(&file);
    let table = &book.sheet(0).unwrap().tables()[0];
    assert_eq!(table.header_row_count(), 0);
    assert!(table.other_attributes().iter().any(|(k, v)| &**k == "dataDxfId" && &**v == "2"));
    assert_eq!(table.columns()[1].totals_row_formula(), Some(("SUM(Table1[Count])", false)));

    let again = write(&book);
    let xml = part(&again, "xl/tables/table1.xml");
    for attr in [r#"headerRowCount="0""#, r#"dataDxfId="2""#, r#"insertRow="1""#, r#"dataDxfId="1""#, "<totalsRowFormula>SUM(Table1[Count])</totalsRowFormula>"] {
        assert!(xml.contains(attr), "{attr}: {xml}");
    }
}

#[test]
fn the_table_styles_of_the_workbook() {
    let file = edit(&with_table(), |name, text| {
        if name == "xl/styles.xml" {
            let dxfs = r#"<dxfs count="2"><dxf><font><b/><color rgb="FFFF0000"/></font></dxf><dxf><font><b val="0"/></font><fill><patternFill><bgColor theme="4" tint="0.4"/></patternFill></fill></dxf></dxfs>"#;
            let styles = r#"<tableStyles count="1" defaultTableStyle="Mine" defaultPivotStyle="PivotStyleLight16"><tableStyle name="Mine" pivot="0" count="2"><tableStyleElement type="headerRow" dxfId="0"/><tableStyleElement type="firstRowStripe" size="2" dxfId="1"/></tableStyle></tableStyles>"#;
            let text = match text.find("<dxfs") {
                Some(i) => {
                    let end = text[i..].find("</dxfs>").map(|j| i + j + 7).unwrap_or_else(|| i + text[i..].find("/>").unwrap() + 2);
                    format!("{}{}{}", &text[..i], dxfs, &text[end..])
                }
                None => text.replacen("<tableStyles", &format!("{dxfs}<tableStyles"), 1),
            };
            match text.find("<tableStyles") {
                Some(i) => {
                    let end = text[i..].find("/>").map(|j| i + j + 2).unwrap();
                    format!("{}{}{}", &text[..i], styles, &text[end..])
                }
                None => text.replace("</dxfs>", &format!("</dxfs>{styles}")),
            }
        } else {
            text
        }
    });
    let book = read(&file);
    assert_eq!(book.default_table_style().as_deref(), Some("Mine"));
    assert_eq!(book.default_pivot_style().as_deref(), Some("PivotStyleLight16"));
    let styles = book.table_styles();
    assert_eq!(styles.len(), 1);
    let style = &styles[0];
    assert_eq!(style.name(), "Mine");
    assert!(!style.pivot() && style.table());
    let kinds: Vec<_> = style.elements().iter().map(|e| (e.kind(), e.size(), e.dxf_id())).collect();
    assert_eq!(kinds, [("headerRow", 1, Some(0)), ("firstRowStripe", 2, Some(1))]);
    let header = book.differential_format(0).unwrap();
    assert_eq!(header.font().unwrap().bold_value(), Some(true));
    assert_eq!(header.font().unwrap().italic_value(), None);
    let stripe = book.differential_format(1).unwrap();
    assert_eq!(stripe.font().unwrap().bold_value(), Some(false));
    assert!(book.differential_format(2).is_none());
}

#[test]
fn the_font_of_the_normal_style() {
    let mut book = umya_spreadsheet::new_file();
    assert_eq!(book.default_font(), Some(("Calibri".to_string(), 11.0)));
    book.set_default_font("游ゴシック", 11.0);
    book.sheet_mut(0).unwrap().cell_mut("A1").set_value("x");
    book.sheet_mut(0).unwrap().cell_mut("A2").style_mut().font_mut().set_bold(true);
    let file = write(&book);
    let styles = part(&file, "xl/styles.xml");
    let first = &styles[styles.find("<font>").unwrap()..styles.find("</font>").unwrap()];
    assert!(first.contains(r#"<name val="游ゴシック"/>"#) && !first.contains("scheme"), "{styles}");
    assert_eq!(read(&file).default_font(), Some(("游ゴシック".to_string(), 11.0)));
}

#[test]
fn the_format_of_the_normal_style() {
    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/styles.xml" {
            let i = text.find("<cellXfs").unwrap();
            let j = i + text[i..].find("<xf ").unwrap();
            let k = j + text[j..].find("/>").unwrap();
            format!(r#"{}{} applyAlignment="1"><alignment vertical="center"/></xf>{}"#, &text[..j], &text[j..k], &text[k + 2..])
        } else {
            text
        }
    });
    let book = read(&file);
    let normal = book.normal_style();
    assert_eq!(normal.alignment().unwrap().vertical().value_string(), "center");
}

#[test]
fn filters_by_what_the_criteria_cannot_say() {
    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/worksheets/sheet1.xml" {
            text.replace(
                "<pageMargins",
                r#"<autoFilter ref="A1:C9"><filterColumn colId="0"><filters><dateGroupItem year="2024" month="3" dateTimeGrouping="month"/></filters></filterColumn><filterColumn colId="1"><colorFilter dxfId="0"/></filterColumn><filterColumn colId="2"><filters><filter val="a"/></filters></filterColumn></autoFilter><pageMargins"#,
            )
        } else {
            text
        }
    });
    let book = read(&file);
    let columns = book.sheet(0).unwrap().auto_filter().unwrap().filter_columns();
    assert!(columns.iter().all(|c| c.is_filtered()));
    assert!(columns[0].criteria().is_none() || columns[0].is_filtered());
    let sheet = part(&write(&book), "xl/worksheets/sheet1.xml");
    for kept in [
        r#"<dateGroupItem year="2024" month="3" dateTimeGrouping="month"/>"#,
        r#"<colorFilter dxfId="0"/>"#,
        r#"<filter val="a"/>"#,
    ] {
        assert!(sheet.contains(kept), "{kept}: {sheet}");
    }
}

#[test]
fn panes_frozen_on_columns() {
    use umya_spreadsheet::structs::{Pane, PaneStateValues, PaneValues};
    let mut book = umya_spreadsheet::new_file();
    let view = &mut book.sheet_mut(0).unwrap().sheet_views_mut().sheet_view_list_mut()[0];
    let mut pane = Pane::default();
    pane.set_horizontal_split(1.0).set_active_pane(PaneValues::TopRight).set_state(PaneStateValues::Frozen);
    view.set_pane(pane);
    let file = write(&book);
    let sheet = part(&file, "xl/worksheets/sheet1.xml");
    assert!(sheet.contains(r#"activePane="topRight""#), "{sheet}");
    let book = read(&file);
    let view = &book.sheet(0).unwrap().sheets_views().sheet_view_list()[0];
    assert!(matches!(view.pane().unwrap().active_pane(), PaneValues::TopRight));
}

#[test]
fn the_palette_of_the_workbook() {
    let colors = r#"<colors><indexedColors><rgbColor rgb="00000000"/><rgbColor rgb="00FFFFFF"/><rgbColor rgb="00FF0000"/></indexedColors><mruColors><color rgb="FF3366FF"/></mruColors></colors>"#;
    let file = edit(&write(&umya_spreadsheet::new_file()), |name, text| {
        if name == "xl/styles.xml" {
            text.replacen("<tableStyles", &format!("{colors}<tableStyles"), 1)
        } else {
            text
        }
    });
    assert!(part(&file, "xl/styles.xml").contains(colors));
    let styles = part(&write(&read(&file)), "xl/styles.xml");
    assert!(styles.contains(colors), "{styles}");
}
