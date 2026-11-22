use std::{
    fs::File,
    io,
    path::Path,
    sync::RwLock,
};

use super::driver;
use crate::{
    XlsxError,
    helper::const_str::{
        COMMENTS_NS,
        DRAWINGS_NS,
        PERSION_NS,
        SHEET_METADATA_NS,
        TABLE_NS,
        THEME_NS,
        THREADED_COMMENT_NS,
        VML_DRAWING_NS,
    },
    structs::{
        Cell,
        CellMetadata,
        PartStore,
        RichData,
        SharedStringTable,
        Stylesheet,
        Workbook,
        Worksheet,
        drawing::Theme,
        raw::RawWorksheet,
    },
};

pub(crate) mod chart;
pub(crate) mod comment;
mod content_types;
mod doc_props_app;
mod doc_props_core;
mod doc_props_custom;
pub(crate) mod drawing;
mod jsa_project_bin;
mod pivot_cache;
mod pivot_table;
mod rels;
mod shared_strings;
mod styles;
pub(crate) mod table;
mod theme;
pub(crate) mod threaded_comment;
mod vba_project_bin;
pub(crate) mod vml_drawing;
mod workbook;
mod workbook_rels;
pub(crate) mod worksheet;
mod worksheet_namespace;

/// read spreadsheet from arbitrary reader.
/// # Arguments
/// * `reader` - reader to read from.
/// # Return value
/// * `Result` - OK is `Workbook`. Err is error message.
pub fn read_reader<R: io::Read + io::Seek>(
    reader: R,
    with_sheet_read: bool,
) -> Result<Workbook, XlsxError> {
    read_reader_with_source(reader, with_sheet_read, None)
}

fn read_reader_with_source<R: io::Read + io::Seek>(
    reader: R,
    with_sheet_read: bool,
    source_file: Option<&Path>,
) -> Result<Workbook, XlsxError> {
    let mut arv = zip::read::ZipArchive::new(reader)?;

    let mut book = workbook::read(&mut arv)?;
    doc_props_app::read(&mut arv, &mut book)?;
    doc_props_core::read(&mut arv, &mut book)?;
    doc_props_custom::read(&mut arv, &mut book)?;
    vba_project_bin::read(&mut arv, &mut book)?;
    jsa_project_bin::read(&mut arv, &mut book)?;
    content_types::read(&mut arv, &mut book)?;
    let workbook_rel = workbook_rels::read(&mut arv, &mut book)?;

    book.set_theme(Theme::default_value());
    for (_, type_value, rel_target) in &workbook_rel {
        if type_value == THEME_NS {
            let theme = theme::read(&mut arv, rel_target)?;
            book.set_theme(theme);
        }
    }

    // What the `cm` (dynamic array formulas) and the `vm` (rich values) of
    // the cells mean.
    for (_, type_value, rel_target) in &workbook_rel {
        if type_value == SHEET_METADATA_NS {
            if let Ok(file) = driver::zip_by_name(&mut arv, &format!("xl/{rel_target}")) {
                book.set_cell_metadata(CellMetadata::read(io::BufReader::new(file)));
            }
        }
    }

    // The authors of the threaded comments.
    for (_, type_value, rel_target) in &workbook_rel {
        if type_value == PERSION_NS {
            let path = driver::join_paths("xl", rel_target);
            if let Ok(mut file) = driver::zip_by_name(&mut arv, &path) {
                let mut data = Vec::new();
                if io::Read::read_to_end(&mut file, &mut data).is_ok() {
                    *book.persons_mut() = crate::structs::office2019::person::Person::read_list(&data);
                }
            }
        }
    }

    // What the `vm` of the cells refers to (pictures placed in cells).
    book.set_rich_data(RichData::read(&mut arv, &workbook_rel));

    // What the charts of the drawings are made of (and the other parts kept
    // as they were read).
    let store = PartStore::read(&mut arv);
    for (id, rel_type, target) in &workbook_rel {
        if crate::structs::KeptRelationships::is_kept_type(rel_type) {
            let path = driver::join_paths("xl", target);
            book.kept_relationships_mut().add(&store, id, rel_type, &path);
        }
    }
    book.set_chart_parts(store);

    shared_strings::read(&mut arv, &mut book)?;
    styles::read(&mut arv, &mut book)?;

    for sheet in book.sheet_collection_mut() {
        for (rel_id, _, rel_target) in &workbook_rel {
            if sheet.r_id() != rel_id {
                continue;
            }
            let mut raw_worksheet = RawWorksheet::default();
            match source_file {
                Some(source_file) => raw_worksheet.read_lazy(&mut arv, rel_target, source_file),
                None => raw_worksheet.read(&mut arv, rel_target),
            }
            sheet.set_raw_data_of_worksheet(raw_worksheet);
        }
    }

    if with_sheet_read {
        book.read_sheet_collection();
    }

    Ok(book)
}

/// read spreadsheet file.
/// # Arguments
/// * `path` - file path to read.
/// # Return value
/// * `Result` - OK is Workbook. Err is error message.
/// # Examples
/// ```
/// let path = std::path::Path::new("./tests/test_files/aaa.xlsx");
/// let mut book = umya_spreadsheet::reader::xlsx::read(path).unwrap();
/// ```
#[inline]
pub fn read<P: AsRef<Path>>(path: P) -> Result<Workbook, XlsxError> {
    let file = File::open(path)?;
    read_reader(file, true)
}

/// lazy read spreadsheet file.
/// Delays the loading of the worksheet until it is needed.
/// When loading a file with a large amount of data, response improvement can be
/// expected. # Arguments
/// * `path` - file path to read.
/// # Return value
/// * `Result` - OK is Workbook. Err is error message.
/// # Examples
/// ```
/// let path = std::path::Path::new("./tests/test_files/aaa.xlsx");
/// let mut book = umya_spreadsheet::reader::xlsx::lazy_read(path).unwrap();
/// ```
#[inline]
pub fn lazy_read(path: &Path) -> Result<Workbook, XlsxError> {
    let file = File::open(path)?;
    read_reader_with_source(file, false, Some(path))
}

/// Stream cells from a worksheet without deserializing the worksheet into
/// memory.
///
/// This is intended for very large sheets where building a full [`Worksheet`]
/// would allocate too much memory. The callback receives one parsed [`Cell`] at
/// a time; the cell is dropped before the next cell is parsed.
pub fn read_sheet_by_name_stream<P, F>(
    path: P,
    sheet_name: &str,
    callback: F,
) -> Result<(), XlsxError>
where
    P: AsRef<Path>,
    F: FnMut(&Cell),
{
    let book = lazy_read(path.as_ref())?;
    let Some(worksheet) = book
        .sheet_collection_no_check()
        .iter()
        .find(|worksheet| worksheet.name() == sheet_name)
    else {
        return Err(XlsxError::CellError(format!(
            "Worksheet '{sheet_name}' not found."
        )));
    };
    let shared_string_table = book.shared_string_table();
    let shared_string_table = &*shared_string_table.read().unwrap();
    worksheet::read_cells_stream(
        worksheet.raw_data_of_worksheet(),
        shared_string_table,
        book.stylesheet(),
        callback,
    )
}

pub(crate) fn raw_to_deserialize_by_worksheet(
    worksheet: &mut Worksheet,
    shared_string_table: &RwLock<SharedStringTable>,
    stylesheet: &Stylesheet,
    cell_metadata: &CellMetadata,
    chart_parts: &PartStore,
) {
    if worksheet.is_deserialized() {
        return;
    }

    let mut raw_data_of_worksheet = worksheet.take_raw_data_of_worksheet();
    let shared_string_table = &*shared_string_table.read().unwrap();
    worksheet::read(
        worksheet,
        &raw_data_of_worksheet,
        shared_string_table,
        stylesheet,
    )
    .unwrap();
    if !cell_metadata.is_empty() {
        for cell in worksheet.cells_crate_mut().collection_mut() {
            cell.resolve_cell_metadata(cell_metadata);
        }
    }
    raw_data_of_worksheet
        .load_relationship_file_data_from_source()
        .unwrap();

    if let Some(v) = raw_data_of_worksheet.worksheet_relationships() {
        for relationship in v.relationship_list() {
            match relationship.get_type() {
                // drawing, chart
                DRAWINGS_NS => {
                    drawing::read(
                        worksheet,
                        relationship.raw_file(),
                        raw_data_of_worksheet.drawing_relationships(),
                        chart_parts,
                    );
                }
                // comment
                COMMENTS_NS => {
                    comment::read(worksheet, relationship.raw_file());
                }
                // threaded_comment
                THREADED_COMMENT_NS => {
                    threaded_comment::read(worksheet, relationship.raw_file());
                }
                // table
                TABLE_NS => {
                    table::read(worksheet, relationship.raw_file()).unwrap();
                }
                rel_type
                    if crate::structs::KeptRelationships::is_kept_type(rel_type)
                        // The picture of an ActiveX control (its controlPr).
                        || (rel_type.ends_with("/image")
                            && worksheet.kept_relationships().refers_to(relationship.id())) =>
                {
                    let sheet_path = raw_data_of_worksheet.worksheet_file().file_target();
                    let dir = sheet_path.rsplit_once('/').map_or("", |(d, _)| d);
                    let path = driver::join_paths(dir, relationship.target());
                    worksheet
                        .kept_relationships_mut()
                        .add(chart_parts, relationship.id(), rel_type, &path);
                }
                _ => {}
            }
        }
        for relationship in v.relationship_list() {
            // vmlDrawing
            if relationship.get_type() == VML_DRAWING_NS {
                vml_drawing::read(
                    worksheet,
                    relationship.raw_file(),
                    raw_data_of_worksheet.vml_drawing_relationships(),
                );
            }
        }
    }
}
