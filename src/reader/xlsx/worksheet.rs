use std::{
    collections::HashMap,
    fs::File,
    io,
};

use quick_xml::{
    Reader,
    escape,
    events::Event,
};

use super::{
    XlsxError,
    driver::{
        get_attribute,
        get_attribute_value,
        xml_read_loop,
        zip_by_name,
    },
};
use crate::{
    AutoFilter,
    helper::formula::FormulaToken,
    structs::{
        Cell,
        Cells,
        Columns,
        ConditionalFormatting,
        DataValidations,
        Hyperlink,
        OleObjects,
        Row,
        SharedStringTable,
        SheetProtection,
        Stylesheet,
        Worksheet,
        office2010::excel::DataValidations as DataValidations2010,
        raw::{
            RawRelationships,
            RawWorksheet,
        },
    },
};

pub(crate) fn read(
    worksheet: &mut Worksheet,
    raw_data_of_worksheet: &RawWorksheet,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
) -> Result<(), XlsxError> {
    if let Some(source_file) = raw_data_of_worksheet.worksheet_file().source_file() {
        if !raw_data_of_worksheet.worksheet_file().has_file_data() {
            let file = File::open(source_file)?;
            let mut archive = zip::read::ZipArchive::new(file)?;
            let source = zip_by_name(
                &mut archive,
                raw_data_of_worksheet.worksheet_file().file_target(),
            )?;
            let reader = io::BufReader::new(source);
            return read_from_reader(
                worksheet,
                reader,
                raw_data_of_worksheet,
                shared_string_table,
                stylesheet,
            );
        }
    }

    let data = io::Cursor::new(raw_data_of_worksheet.worksheet_file().file_data());
    read_from_reader(
        worksheet,
        data,
        raw_data_of_worksheet,
        shared_string_table,
        stylesheet,
    )
}

/// The elements of a sheet the model has nothing for, kept as read and
/// written back in their place (`write_kept` of the writer): how the sheet
/// is calculated, the ranges users may edit, scenarios, the sort, the
/// consolidation, custom views, the errors not to flag, smart tags, custom
/// properties, watches, the pictures of the headers and footers, the
/// background picture, publishing to the web.
pub(crate) const KEPT_ELEMENTS: [&[u8]; 13] = [
    b"sheetCalcPr",
    b"protectedRanges",
    b"scenarios",
    b"sortState",
    b"dataConsolidate",
    b"customSheetViews",
    b"ignoredErrors",
    b"smartTags",
    b"customProperties",
    b"cellWatches",
    b"legacyDrawingHF",
    b"picture",
    b"webPublishItems",
];

fn read_sheet_pr(worksheet: &mut Worksheet, e: &quick_xml::events::BytesStart<'_>) {
    for attr in e.attributes().with_checks(false).flatten() {
        let key = attr.key.into_inner();
        let value = crate::reader::driver::unescaped_attribute_value(&attr);
        if key == b"codeName" {
            worksheet.set_code_name(value);
        } else if !key.contains(&b':') && !key.starts_with(b"xmlns") {
            worksheet.add_sheet_pr_attribute(&String::from_utf8_lossy(key), &value);
        }
    }
}

fn read_from_reader<R: io::BufRead>(
    worksheet: &mut Worksheet,
    data: R,
    raw_data_of_worksheet: &RawWorksheet,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
) -> Result<(), XlsxError> {
    let mut reader = Reader::from_reader(
        super::worksheet_namespace::WorksheetNamespaceReader::new(data),
    );
    reader.config_mut().trim_text(true);
    let mut formula_shared_list: HashMap<u32, (String, Vec<FormulaToken>)> = HashMap::new();
    xml_read_loop!(
        reader,
        Event::Start(ref e) => match e.name().into_inner() {
            b"sheetPr" => {
                read_sheet_pr(worksheet, e);
            }
            b"sheetViews" => {
                worksheet
                    .sheet_views_mut()
                    .set_attributes(&mut reader, e);
            }
            b"sheetFormatPr" => {
                worksheet
                    .sheet_format_properties_mut()
                    .set_attributes(&mut reader, e);
            }
            b"selection" => {
                for a in e.attributes().with_checks(false) {
                    match a {
                        Ok(ref attr) if attr.key.0 == b"activeCell" => {
                            worksheet.set_active_cell(get_attribute_value(attr)?);
                        }
                        Ok(_) | Err(_) => {}
                    }
                }
            }
            b"row" => {
                let mut obj = Row::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    worksheet.cells_crate_mut(),
                    shared_string_table,
                    stylesheet,
                    &mut formula_shared_list,
                    false,
                );
                worksheet.set_row_dimension(obj);
            }
            b"autoFilter" => {
                let mut obj = AutoFilter::default();
                obj.set_attributes(&mut reader, e, false);
                worksheet.set_auto_filter_obj(obj);
            }
            b"cols" => {
                let mut obj = Columns::default();
                obj.set_attributes(&mut reader, e, stylesheet);
                worksheet.set_column_dimensions_crate(obj);
            }
            b"mergeCells" => {
                worksheet
                    .merge_cells_crate_mut()
                    .set_attributes(&mut reader, e);
            }
            b"conditionalFormatting" => {
                let mut obj = ConditionalFormatting::default();
                obj.set_attributes(&mut reader, e, stylesheet.differential_formats());
                worksheet.add_conditional_formatting_collection(obj);
            }
            b"worksheet" => {
                worksheet.kept_relationships_mut().set_namespaces(e);
            }
            b"controls" => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, false);
                worksheet.kept_relationships_mut().add_xml("controls", xml);
            }
            name if KEPT_ELEMENTS.contains(&name) => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, false);
                let name = String::from_utf8_lossy(name).to_string();
                worksheet.kept_relationships_mut().add_xml(&name, xml);
            }
            // An extension of the sheet the model does not read (sparklines,
            // slicers, timelines...): kept as read. Those of conditional
            // formats and data validations are read below.
            b"ext" if !matches!(
                get_attribute(e, b"uri").as_deref(),
                Some("{78C0D931-6437-407d-A8EE-F0AAD7539E65}" | "{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}")
            ) =>
            {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, false);
                worksheet.kept_relationships_mut().add_xml("ext", xml);
            }
            b"x14:conditionalFormattings" => {
                for obj in ConditionalFormatting::read_x14_collection(
                    &mut reader,
                    stylesheet.differential_formats(),
                ) {
                    worksheet.add_conditional_formatting_2010_collection(obj);
                }
            }
            b"dataValidations" => {
                let mut obj = DataValidations::default();
                obj.set_attributes(&mut reader, e);
                worksheet.set_data_validations(obj);
            }
            b"x14:dataValidations" => {
                let mut obj = DataValidations2010::default();
                obj.set_attributes(&mut reader, e);
                worksheet.set_data_validations_2010(obj);
            }
            b"oleObjects" => {
                let mut obj = OleObjects::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    raw_data_of_worksheet.worksheet_relationships().unwrap(),
                );
                worksheet.set_ole_objects(obj);
            }
            b"headerFooter" => {
                worksheet
                    .header_footer_mut()
                    .set_attributes(&mut reader, e);
            }
            b"rowBreaks" => {
                worksheet
                    .row_breaks_mut()
                    .set_attributes(&mut reader, e);
            }
            b"colBreaks" => {
                worksheet
                    .column_breaks_mut()
                    .set_attributes(&mut reader, e);
            }
            _ => (),
        },
        Event::Empty(ref e) => match e.name().into_inner() {
            b"sheetPr" => {
                read_sheet_pr(worksheet, e);
            }
            b"tabColor" => {
                worksheet
                    .tab_color_mut()
                    .set_attributes(&mut reader, e, true);
            }
            // Where the summary rows and columns of the groups are, and how
            // the outline is styled: kept as read.
            b"outlinePr" => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, true);
                worksheet.kept_relationships_mut().add_xml("outlinePr", xml);
            }
            b"pageSetUpPr" => {
                if let Some(v) = get_attribute(e, b"fitToPage") {
                    worksheet.page_setup_mut().set_fit_to_page_str(&v);
                }
            }
            b"sheetFormatPr" => {
                worksheet
                    .sheet_format_properties_mut()
                    .set_attributes(&mut reader, e);
            }
            b"selection" => {
                for a in e.attributes().with_checks(false) {
                    match a {
                        Ok(ref attr) if attr.key.0 == b"activeCell" => {
                            worksheet.set_active_cell(get_attribute_value(attr)?);
                        }
                        Ok(_) | Err(_) => {}
                    }
                }
            }
            b"row" => {
                let mut obj = Row::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    worksheet.cells_crate_mut(),
                    shared_string_table,
                    stylesheet,
                    &mut formula_shared_list,
                    true,
                );
                worksheet.set_row_dimension(obj);
            }
            b"autoFilter" => {
                let mut obj = AutoFilter::default();
                obj.set_attributes(&mut reader, e, true);
                worksheet.set_auto_filter_obj(obj);
            }
            b"pageMargins" => {
                worksheet
                    .page_margins_mut()
                    .set_attributes(&mut reader, e);
            }
            b"hyperlink" => {
                if let Some((coor, hyperlink)) = get_hyperlink(
                    e,
                    raw_data_of_worksheet.worksheet_relationships()
                ) {
                    worksheet.cell_mut(coor).set_hyperlink(hyperlink);
                }
            }
            b"headerFooter" => {
                worksheet.header_footer_mut().set_options(e);
            }
            name if KEPT_ELEMENTS.contains(&name) => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, true);
                let name = String::from_utf8_lossy(name).to_string();
                worksheet.kept_relationships_mut().add_xml(&name, xml);
            }
            // How the furigana of the sheet are written (type, alignment):
            // kept as read.
            b"phoneticPr" => {
                let xml = crate::reader::driver::read_raw_element(&mut reader, e, true);
                worksheet.kept_relationships_mut().add_xml("phoneticPr", xml);
            }
            b"printOptions" => {
                worksheet
                    .print_options_mut()
                    .set_attributes(&mut reader, e);
            }
            b"pageSetup" => {
                worksheet.page_setup_mut().set_attributes(
                    &mut reader,
                    e,
                    raw_data_of_worksheet.worksheet_relationships(),
                );
            }
            b"sheetProtection" => {
                let mut obj = SheetProtection::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                );
                worksheet.set_sheet_protection(obj);
            }
            _ => (),
        },
        Event::Eof => break,
    );

    Ok(())
}

pub(crate) fn read_lite(
    raw_data_of_worksheet: &RawWorksheet,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
) -> Cells {
    if let Some(source_file) = raw_data_of_worksheet.worksheet_file().source_file() {
        if !raw_data_of_worksheet.worksheet_file().has_file_data() {
            let file = File::open(source_file).unwrap();
            let mut archive = zip::read::ZipArchive::new(file).unwrap();
            let source = zip_by_name(
                &mut archive,
                raw_data_of_worksheet.worksheet_file().file_target(),
            )
            .unwrap();
            let reader = io::BufReader::new(source);
            return read_lite_from_reader(reader, shared_string_table, stylesheet);
        }
    }

    let data = io::Cursor::new(raw_data_of_worksheet.worksheet_file().file_data());
    read_lite_from_reader(data, shared_string_table, stylesheet)
}

fn read_lite_from_reader<R: io::BufRead>(
    data: R,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
) -> Cells {
    let mut reader = Reader::from_reader(
        super::worksheet_namespace::WorksheetNamespaceReader::new(data),
    );
    reader.config_mut().trim_text(true);

    let mut cells = Cells::default();
    let mut formula_shared_list: HashMap<u32, (String, Vec<FormulaToken>)> = HashMap::new();
    xml_read_loop!(
        reader,
        Event::Start(ref e) => {
            if e.name().into_inner() == b"row" {
                let mut obj = Row::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    &mut cells,
                    shared_string_table,
                    stylesheet,
                    &mut formula_shared_list,
                    false,
                );
            }
        },
        Event::Empty(ref e) => {
            if e.name().into_inner() == b"row" {
                let mut obj = Row::default();
                obj.set_attributes(
                    &mut reader,
                    e,
                    &mut cells,
                    shared_string_table,
                    stylesheet,
                    &mut formula_shared_list,
                    true,
                );
            }
        },
        Event::Eof => break,
    );

    cells
}

pub(crate) fn read_cells_stream<F>(
    raw_data_of_worksheet: &RawWorksheet,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
    callback: F,
) -> Result<(), XlsxError>
where
    F: FnMut(&Cell),
{
    if let Some(source_file) = raw_data_of_worksheet.worksheet_file().source_file() {
        if !raw_data_of_worksheet.worksheet_file().has_file_data() {
            let file = File::open(source_file)?;
            let mut archive = zip::read::ZipArchive::new(file)?;
            let source = zip_by_name(
                &mut archive,
                raw_data_of_worksheet.worksheet_file().file_target(),
            )?;
            let reader = io::BufReader::new(source);
            read_cells_stream_from_reader(reader, shared_string_table, stylesheet, callback);
            return Ok(());
        }
    }

    let data = io::Cursor::new(raw_data_of_worksheet.worksheet_file().file_data());
    read_cells_stream_from_reader(data, shared_string_table, stylesheet, callback);
    Ok(())
}

fn read_cells_stream_from_reader<R, F>(
    data: R,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
    mut callback: F,
) where
    R: io::BufRead,
    F: FnMut(&Cell),
{
    let mut reader = Reader::from_reader(
        super::worksheet_namespace::WorksheetNamespaceReader::new(data),
    );
    reader.config_mut().trim_text(true);
    let mut formula_shared_list: HashMap<u32, (String, Vec<FormulaToken>)> = HashMap::new();

    xml_read_loop!(
        reader,
        Event::Start(ref e) => {
            if e.name().into_inner() == b"row" {
                read_row_cells_stream(
                    &mut reader,
                    shared_string_table,
                    stylesheet,
                    &mut formula_shared_list,
                    &mut callback,
                );
            }
        },
        Event::Eof => break,
    );
}

fn read_row_cells_stream<R, F>(
    reader: &mut Reader<R>,
    shared_string_table: &SharedStringTable,
    stylesheet: &Stylesheet,
    formula_shared_list: &mut HashMap<u32, (String, Vec<FormulaToken>)>,
    callback: &mut F,
) where
    R: io::BufRead,
    F: FnMut(&Cell),
{
    xml_read_loop!(
        reader,
        Event::Empty(ref e) => {
            if e.name().into_inner() == b"c" {
                let mut obj = Cell::default();
                obj.set_attributes(
                    reader,
                    e,
                    shared_string_table,
                    stylesheet,
                    true,
                    formula_shared_list,
                );
                callback(&obj);
            }
        },
        Event::Start(ref e) => {
            if e.name().into_inner() == b"c" {
                let mut obj = Cell::default();
                obj.set_attributes(
                    reader,
                    e,
                    shared_string_table,
                    stylesheet,
                    false,
                    formula_shared_list,
                );
                callback(&obj);
            }
        },
        Event::End(ref e) => {
            if e.name().into_inner() == b"row" {
                return
            }
        },
        Event::Eof => panic!("Error: Could not find {} end element", "row")
    );
}

/// Reads a hyperlink and the cell it belongs to. `ref` can be a range (Excel
/// writes the area of a merged cell), so the hyperlink is kept on its top-left
/// cell; a hyperlink without a cell is skipped. A link with an address
/// (`r:id`) and a `location` (a sheet and cell of another workbook, an anchor)
/// keeps both.
fn get_hyperlink(
    e: &quick_xml::events::BytesStart<'_>,
    raw_relationships: Option<&RawRelationships>,
) -> Option<(String, Hyperlink)> {
    let mut hyperlink = Hyperlink::default();

    let coordinate = get_attribute(e, b"ref").unwrap_or_default();
    let coordinate = coordinate.split(':').next().unwrap_or_default().trim().to_string();
    if coordinate.is_empty() {
        return None;
    }
    let location = get_attribute(e, b"location")
        .map(|v| escape::unescape(&v).map(|v| v.to_string()).unwrap_or(v));
    if let Some(v) = get_attribute(e, b"tooltip") {
        hyperlink.set_tooltip(escape::unescape(&v).unwrap().to_string());
    }
    if let Some(v) = get_attribute(e, b"display") {
        hyperlink.set_display(escape::unescape(&v).map(|v| v.to_string()).unwrap_or(v));
    }
    // A dangling r:id (no relationship, or no relationships part at all)
    // leaves the hyperlink without a URL instead of aborting the read.
    let target = get_attribute(e, b"r:id").and_then(|v| {
        raw_relationships
            .and_then(|rels| rels.find_relationship_by_rid(&v))
            .map(|relationship| escape::unescape(relationship.target()).unwrap().to_string())
    });
    match (target, location) {
        (Some(target), location) => {
            hyperlink.set_url(target);
            hyperlink.set_target_location(location.unwrap_or_default());
        }
        (None, Some(location)) => {
            hyperlink.set_url(location);
            hyperlink.set_location(true);
        }
        (None, None) => {}
    }
    Some((coordinate, hyperlink))
}
