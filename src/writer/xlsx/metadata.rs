use std::io;

use quick_xml::Writer;

use super::XlsxError;
use crate::{
    helper::const_str::PKG_SHEET_METADATA,
    structs::{
        CellMetadata,
        WriterManager,
    },
};

/// Writes `xl/metadata.xml` when cells refer to it. Returns whether it was
/// written.
pub(crate) fn write<W: io::Seek + io::Write>(
    cell_metadata: &CellMetadata,
    writer_mng: &mut WriterManager<W>,
) -> Result<bool, XlsxError> {
    if cell_metadata.is_empty() {
        return Ok(false);
    }
    let mut writer = Writer::new(io::Cursor::new(Vec::new()));
    cell_metadata.write_to(&mut writer);
    writer_mng.add_writer(PKG_SHEET_METADATA, writer)?;
    Ok(true)
}
