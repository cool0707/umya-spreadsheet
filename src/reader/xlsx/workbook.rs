use std::io;

use quick_xml::{
    Reader,
    events::Event,
};

use super::{
    XlsxError,
    driver::get_attribute,
};
use crate::{
    helper::const_str::PKG_WORKBOOK,
    structs::{
        CalculationProperties,
        DefinedName,
        Workbook,
        WorkbookProtection,
        WorkbookView,
        Worksheet,
    },
    xml_read_loop,
};

pub(crate) fn read<R: io::Read + io::Seek>(
    arv: &mut zip::read::ZipArchive<R>,
) -> Result<Workbook, XlsxError> {
    let r = io::BufReader::new(super::driver::zip_by_name(arv, PKG_WORKBOOK)?);
    let mut reader = Reader::from_reader(r);
    reader.config_mut().trim_text(true);
    let mut wb = Workbook::default();

    let mut defined_names: Vec<DefinedName> = Vec::new();

    xml_read_loop!(
        reader,
        ref n @ (Event::Empty(ref e) | Event::Start(ref e)) => {
            let is_empty = matches!(n, Event::Empty(_));
            match e.name().into_inner() {
                b"workbookView" => {
                    let mut obj = WorkbookView::default();
                    obj.set_attributes(&mut reader, e);
                    wb.set_workbook_view(obj);
                }
                b"workbookPr" => {
                    for a in e.attributes().with_checks(false).flatten() {
                        let key = a.key.into_inner();
                        let v = crate::reader::driver::unescaped_attribute_value(&a);
                        match key {
                            b"date1904" => {
                                wb.set_date1904(v == "1" || v == "true");
                            }
                            b"codeName" => {
                                wb.set_code_name(v.to_string());
                            }
                            b"filterPrivacy" => {}
                            key if !key.contains(&b':') => {
                                let key = String::from_utf8_lossy(key).to_string();
                                wb.set_workbook_pr_attribute(&key, Some(&v));
                            }
                            _ => {}
                        }
                    }
                }
                b"workbookProtection" => {
                    let mut obj = WorkbookProtection::default();
                    obj.set_attributes(&mut reader, e);
                    wb.set_workbook_protection(obj);
                }
                b"sheet" => {
                    let mut worksheet = Worksheet::default();
                    worksheet.set_attributes_from_wookbook(&mut reader, e, is_empty);
                    wb.add_sheet(worksheet).unwrap();
                }
                b"calcPr" => {
                    let mut obj = CalculationProperties::default();
                    obj.set_attributes(&mut reader, e);
                    wb.set_calculation_properties(obj);
                }
                b"pivotCache" => {
                    let cache_id = get_attribute(e, b"cacheId").unwrap();
                    let r_id = get_attribute(e, b"r:id").unwrap();
                    wb.add_pivot_caches((r_id, cache_id, String::new()));
                }
                b"workbook" => {
                    wb.kept_relationships_mut().set_namespaces(e);
                }
                b"externalReferences" | b"extLst" | b"pivotCaches" if !is_empty => {
                    let name = String::from_utf8_lossy(e.name().into_inner()).to_string();
                    let xml = crate::reader::driver::read_raw_element(&mut reader, e, false);
                    wb.kept_relationships_mut().add_xml(&name, xml);
                }
                // What the model has nothing for, kept as read: the version of
                // the application that wrote the file, the password
                // to write and the recommendation to open read-only, custom
                // views, the size of the OLE object of the workbook, smart
                // tags, publishing to the web, how the file was recovered.
                b"fileVersion" | b"fileSharing" | b"functionGroups" | b"oleSize" | b"customWorkbookViews" | b"smartTagPr"
                | b"smartTagTypes" | b"webPublishing" | b"fileRecoveryPr" | b"webPublishObjects" => {
                    let name = String::from_utf8_lossy(e.name().into_inner()).to_string();
                    let xml = crate::reader::driver::read_raw_element(&mut reader, e, is_empty);
                    wb.kept_relationships_mut().add_xml(&name, xml);
                }
                b"definedName" => {
                    let mut obj = DefinedName::default();
                    obj.set_attributes(&mut reader, e);
                    defined_names.push(obj);
                }
                _ => (),
            }
        },
        Event::Eof => break
    );

    for defined_name in &defined_names {
        if defined_name.has_local_sheet_id() {
            let local_sheet_id = defined_name.local_sheet_id() as usize;
            // A name scoped to a sheet that does not exist is dropped (Excel
            // removes it when it repairs the file) instead of aborting the
            // read.
            if let Ok(sheet) = wb.sheet_mut(local_sheet_id) {
                sheet.add_defined_names(defined_name.clone());
            }
        } else {
            if let Some(v) = defined_name.address_obj().first() {
                if let Ok(s) = wb.sheet_by_name_mut(v.sheet_name()) {
                    s.add_defined_names(defined_name.clone());
                    continue;
                }
            }
            wb.add_defined_names(defined_name.clone());
        }
    }

    Ok(wb)
}
