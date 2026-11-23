use std::io;

use quick_xml::{
    Reader,
    events::Event,
};

use super::XlsxError;
use crate::{
    structs::drawing::Theme,
    xml_read_loop,
};

pub fn read<R: io::Read + io::Seek>(
    arv: &mut zip::ZipArchive<R>,
    target: &str,
) -> Result<Theme, XlsxError> {
    let path = super::driver::join_paths("xl", target);
    let mut data = Vec::new();
    io::Read::read_to_end(&mut super::driver::zip_by_name(arv, &path)?, &mut data)?;
    // A theme that refers to other parts (pictures of its fills) is written
    // from the model.
    let refers = crate::structs::read_part(arv, &crate::structs::relationships_path(&path))
        .is_some_and(|rels| !crate::structs::read_relationships(&rels).is_empty());
    let mut reader = Reader::from_reader(io::Cursor::new(data.as_slice()));
    reader.config_mut().trim_text(true);

    let mut theme: Theme = Theme::default();

    xml_read_loop!(
        reader,
        Event::Start(ref e) => {
            if e.name().into_inner() == b"a:theme" {
                theme.set_attributes(&mut reader, e);
            }
        },
        Event::Eof => break,
    );

    if !refers {
        theme.set_kept(data);
    }
    Ok(theme)
}
