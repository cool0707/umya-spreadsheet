use std::{
    io,
    io::Read,
};

use super::XlsxError;
use crate::{
    helper::const_str::PKG_VBA_PROJECT,
    structs::Workbook,
};

pub(crate) fn read<R: Read + io::Seek>(
    arv: &mut zip::ZipArchive<R>,
    wb: &mut Workbook,
) -> Result<(), XlsxError> {
    let mut r = io::BufReader::new(match super::driver::zip_by_name(arv, PKG_VBA_PROJECT) {
        Ok(v) => v,
        Err(zip::result::ZipError::FileNotFound) => {
            return Ok(());
        }
        Err(e) => {
            return Err(e.into());
        }
    });
    let mut buf = Vec::new();
    r.read_to_end(&mut buf)?;
    drop(r);

    wb.set_macros_code(buf);

    // Its signatures (vbaProjectSignature.bin, ...Agile.bin, ...V3.bin).
    let mut signatures = Vec::new();
    if let Some(rels) = crate::structs::read_part(arv, "xl/_rels/vbaProject.bin.rels") {
        for rel in crate::structs::read_relationships(&rels) {
            if rel.external || !rel.rel_type.contains("vbaProjectSignature") {
                continue;
            }
            let path = super::driver::join_paths("xl", &rel.target);
            if let Some(data) = crate::structs::read_part(arv, &path) {
                signatures.push((rel.rel_type, path, data));
            }
        }
    }
    wb.set_vba_signatures(signatures);

    Ok(())
}
