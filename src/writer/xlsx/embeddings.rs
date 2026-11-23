use std::io;

use super::XlsxError;
use crate::structs::{
    Worksheet,
    WriterManager,
};

pub(crate) fn write<W: io::Seek + io::Write>(
    worksheet: &Worksheet,
    writer_mng: &mut WriterManager<W>,
) -> Result<(Vec<String>, Vec<String>), XlsxError> {
    let mut ole_object_no_list: Vec<String> = Vec::new();
    let mut excel_no_list: Vec<String> = Vec::new();
    for ole_object in worksheet.ole_objects().ole_object() {
        if ole_object.is_bin() {
            let writer = ole_object.object_data().unwrap();
            let file_no = writer_mng.add_file_at_ole_object(writer)?;
            ole_object_no_list.push(file_no.to_string());
        }
        if ole_object.is_xlsx() {
            let writer = ole_object.object_data().unwrap();
            let file_no = writer_mng.add_file_at_excel(writer)?;
            excel_no_list.push(format!("Microsoft_Excel_Worksheet{file_no}.xlsx"));
        } else if ole_object.is_package() {
            // A document of Word, a presentation, a workbook with macros...
            let writer = ole_object.object_data().unwrap_or_default();
            let name = writer_mng.add_file_at_package(writer, ole_object.object_extension())?;
            excel_no_list.push(name);
        }
    }

    Ok((ole_object_no_list, excel_no_list))
}
