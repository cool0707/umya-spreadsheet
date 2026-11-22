use std::io;

use super::XlsxError;
use crate::{
    helper::const_str::PKG_VBA_PROJECT,
    structs::{
        Workbook,
        WriterManager,
    },
};

pub(crate) fn write<W: io::Seek + io::Write>(
    wb: &Workbook,
    writer_mng: &mut WriterManager<W>,
) -> Result<(), XlsxError> {
    if !wb.has_macros() {
        return Ok(());
    }
    let writer = wb.macros_code().unwrap();
    writer_mng.add_bin(PKG_VBA_PROJECT, writer)?;
    // The signatures of the project as it was read.
    if wb.vba_signatures().is_empty() {
        return Ok(());
    }
    let mut rels = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
    );
    for (n, (rel_type, path, data)) in wb.vba_signatures().iter().enumerate() {
        writer_mng.add_bin(path, data)?;
        // application/vnd.ms-office.vbaProjectSignature, ...Agile, ...V3.
        let kind = rel_type.rsplit('/').next().unwrap_or_default();
        writer_mng.add_content_type(path, &format!("application/vnd.ms-office.{kind}"));
        let target = path.strip_prefix("xl/").unwrap_or(path);
        rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"{rel_type}\" Target=\"{target}\"/>",
            n + 1
        ));
    }
    rels.push_str("</Relationships>");
    writer_mng.add_bin("xl/_rels/vbaProject.bin.rels", rels.as_bytes())
}
