use std::io;

use quick_xml::{
    Writer,
    events::{
        BytesDecl,
        Event,
    },
};

use super::{
    XlsxError,
    driver::write_new_line,
};
use crate::structs::{
    Worksheet,
    WriterManager,
    drawing::spreadsheet::KEPT,
};

pub(crate) fn write<W: io::Seek + io::Write>(
    worksheet: &Worksheet,
    writer_mng: &mut WriterManager<W>,
) -> Result<(String, Vec<(String, String)>), XlsxError> {
    let mut rel_list: Vec<(String, String)> = Vec::new();

    if !worksheet.has_drawing_object() {
        return Ok((String::new(), rel_list));
    }

    let mut writer = Writer::new(io::Cursor::new(Vec::new()));
    // XML header
    writer
        .write_event(Event::Decl(BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))
        .unwrap();
    write_new_line(&mut writer);

    // The parts of the charts kept as they were read.
    let kept_relationships = worksheet
        .worksheet_drawing()
        .kept_anchors_to_write()
        .map(|anchor| anchor.write_parts(writer_mng))
        .collect::<Result<Vec<_>, _>>()?;

    worksheet.worksheet_drawing().write_to(
        &mut writer,
        worksheet.ole_objects(),
        &mut rel_list,
        &kept_relationships,
    );
    // The relationships of the kept anchors, in the order they were listed.
    let kept: Vec<&(String, String, bool)> = kept_relationships.iter().flatten().collect();
    for (key, value) in &mut rel_list {
        let fixed = key.strip_prefix(KEPT).and_then(|k| k.strip_prefix('#')).map(str::to_string);
        if key == KEPT || fixed.is_some() {
            if let Some((rel_type, target, external)) =
                value.parse::<usize>().ok().and_then(|n| kept.get(n))
            {
                *key = format!("{KEPT}{}", if *external { "X" } else { "" });
                *value = match &fixed {
                    // (With the id it is written with.)
                    Some(id) => format!("{rel_type}\n{target}\n{id}"),
                    None => format!("{rel_type}\n{target}"),
                };
            }
        }
    }

    let file_no = writer_mng.add_file_at_drawing(writer)?;
    Ok((file_no.to_string(), rel_list))
}
