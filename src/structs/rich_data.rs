use std::io::{
    self,
    Cursor,
    Read,
};

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesDecl,
        Event,
    },
};

use crate::{
    helper::const_str::{
        PKG_MEDIA,
        REL_NS,
    },
    reader::driver::{
        get_attribute,
        join_paths,
        zip_by_name,
    },
    structs::{
        WriterManager,
        XlsxError,
    },
    writer::driver::{
        write_end_tag,
        write_new_line,
        write_start_tag,
    },
};

/// The rich values of a workbook: what the value metadata of its cells
/// (`vm`) refers to. A picture placed in a cell is one, and so are the
/// values of the linked data types and of `IMAGE`.
///
/// They are the parts the workbook refers to with the rich data
/// relationships (`xl/richData/rdrichvalue.xml`, its structures and types,
/// `richValueRel.xml`, ...) and the parts those refer to (the pictures in
/// `xl/media`). This crate does not look into them: they are kept as they
/// were read and written back when cells still refer to them, so the index
/// of a rich value stays what it was.
#[derive(Clone, Debug, Default)]
pub(crate) struct RichData {
    parts: Vec<RichDataPart>,
}

/// A part of the rich data, as the workbook refers to it.
#[derive(Clone, Debug, Default)]
struct RichDataPart {
    /// The type of the workbook's relationship to it.
    rel_type:      String,
    /// Its name in the package (`xl/richData/rdrichvalue.xml`).
    path:          String,
    data:          Vec<u8>,
    /// Its own relationships (`richValueRel.xml` to the pictures).
    relationships: Vec<RichDataRelationship>,
}

#[derive(Clone, Debug, Default)]
struct RichDataRelationship {
    id:          String,
    rel_type:    String,
    /// As it was in the file.
    target:      String,
    target_mode: Option<String>,
    /// The part it refers to (name in the package, content), when that is
    /// in the file.
    part:        Option<(String, Vec<u8>)>,
}

/// Whether a relationship of the workbook is one to a part of the rich
/// data: `.../office/2017/06/relationships/rdRichValue`, `rdRichValueStructure`,
/// `rdRichValueTypes`, `rdArray`, `rdSupportingPropertyBag`, `richStyles`,
/// `.../office/2022/10/relationships/richValueRel`, ...
pub(crate) fn is_rich_data_relationship(rel_type: &str) -> bool {
    let Some(rest) = rel_type.strip_prefix("http://schemas.microsoft.com/office/") else {
        return false;
    };
    let name = rest.rsplit('/').next().unwrap_or_default();
    name.starts_with("rd") || name.starts_with("richValue") || name.starts_with("richStyles")
}

fn read_part<R: Read + io::Seek>(
    arv: &mut zip::read::ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    zip_by_name(arv, path).ok()?.read_to_end(&mut data).ok()?;
    Some(data)
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl RichData {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    /// The relationships of the workbook to the parts: (type, target).
    pub(crate) fn workbook_relationships(&self) -> impl Iterator<Item = (&str, &str)> {
        self.parts.iter().map(|part| {
            (
                part.rel_type.as_str(),
                part.path.strip_prefix("xl/").unwrap_or(&part.path),
            )
        })
    }

    /// Reads the parts the relationships of the workbook (id, type, target
    /// from `xl/`) refer to. A part that cannot be read is left out.
    pub(crate) fn read<R: Read + io::Seek>(
        arv: &mut zip::read::ZipArchive<R>,
        workbook_rel: &[(String, String, String)],
    ) -> Self {
        let mut parts = Vec::new();
        for (_, rel_type, target) in workbook_rel {
            if !is_rich_data_relationship(rel_type) {
                continue;
            }
            let path = join_paths("xl", target);
            let Some(data) = read_part(arv, &path) else {
                continue;
            };
            let dir = directory(&path).to_string();
            let rels_path = format!("{dir}/_rels/{}.rels", file_name(&path));
            let mut relationships = read_part(arv, &rels_path)
                .map(|rels| read_relationships(&rels))
                .unwrap_or_default();
            for relationship in &mut relationships {
                if relationship.target_mode.as_deref() == Some("External") {
                    continue;
                }
                let target_path = join_paths(&dir, &relationship.target);
                relationship.part = read_part(arv, &target_path).map(|data| (target_path, data));
            }
            parts.push(RichDataPart {
                rel_type: rel_type.clone(),
                path,
                data,
                relationships,
            });
        }
        Self { parts }
    }

    /// Writes the parts. A picture whose name another one of the workbook
    /// already has gets a new name.
    pub(crate) fn write<W: io::Seek + io::Write>(
        &self,
        writer_mng: &mut WriterManager<W>,
    ) -> Result<(), XlsxError> {
        // The parts written so far for the relationships: as read -> as written.
        let mut written: Vec<(&str, String)> = Vec::new();
        for part in &self.parts {
            writer_mng.add_bin(&part.path, &part.data)?;
            if part.relationships.is_empty() {
                continue;
            }

            let mut writer = Writer::new(Cursor::new(Vec::new()));
            writer
                .write_event(Event::Decl(BytesDecl::new(
                    "1.0",
                    Some("UTF-8"),
                    Some("yes"),
                )))
                .unwrap();
            write_new_line(&mut writer);
            write_start_tag(
                &mut writer,
                "Relationships",
                vec![("xmlns", REL_NS).into()],
                false,
            );
            for relationship in &part.relationships {
                let mut target = relationship.target.clone();
                if let Some((path, data)) = &relationship.part {
                    let new_path = match written.iter().find(|(read, _)| read == path) {
                        Some((_, new_path)) => new_path.clone(),
                        None => {
                            let new_path = free_name(writer_mng, path);
                            writer_mng.add_bin(&new_path, data)?;
                            written.push((path, new_path.clone()));
                            new_path
                        }
                    };
                    if new_path != *path {
                        target = format!("/{new_path}");
                    }
                }
                let mut attributes: crate::structs::AttrCollection = vec![
                    ("Id", &relationship.id).into(),
                    ("Type", &relationship.rel_type).into(),
                    ("Target", &target).into(),
                ];
                if let Some(mode) = &relationship.target_mode {
                    attributes.push(("TargetMode", mode).into());
                }
                write_start_tag(&mut writer, "Relationship", attributes, true);
            }
            write_end_tag(&mut writer, "Relationships");
            let rels_path = format!(
                "{}/_rels/{}.rels",
                directory(&part.path),
                file_name(&part.path)
            );
            writer_mng.add_writer(&rels_path, writer)?;
        }
        Ok(())
    }
}

/// The name to write a part under: its own, or for a picture whose name is
/// taken (by a picture of a sheet, written before) the next free one.
fn free_name<W: io::Seek + io::Write>(writer_mng: &mut WriterManager<W>, path: &str) -> String {
    if !writer_mng.check_file_exist(path) || directory(path) != PKG_MEDIA {
        return path.to_string();
    }
    let extension = file_name(path).rsplit_once('.').map_or("", |(_, ext)| ext);
    let mut index = 0;
    loop {
        index += 1;
        let new_path = format!("{PKG_MEDIA}/image{index}.{extension}");
        if !writer_mng.check_file_exist(&new_path) {
            return new_path;
        }
    }
}

fn read_relationships(data: &[u8]) -> Vec<RichDataRelationship> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);
    let mut list = Vec::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if e.local_name().into_inner() != b"Relationship" {
                    continue;
                }
                let (Some(id), Some(rel_type), Some(target)) = (
                    get_attribute(&e, b"Id"),
                    get_attribute(&e, b"Type"),
                    get_attribute(&e, b"Target"),
                ) else {
                    continue;
                };
                list.push(RichDataRelationship {
                    id,
                    rel_type,
                    target,
                    target_mode: get_attribute(&e, b"TargetMode"),
                    part: None,
                });
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rich_data_relationship_types() {
        for name in [
            "2017/06/relationships/rdRichValue",
            "2017/06/relationships/rdRichValueStructure",
            "2017/06/relationships/rdRichValueTypes",
            "2017/06/relationships/rdArray",
            "2017/06/relationships/richStyles",
            "2017/06/relationships/rdSupportingPropertyBag",
            "2022/10/relationships/richValueRel",
            "2020/07/relationships/rdRichValueWebImage",
        ] {
            assert!(is_rich_data_relationship(&format!(
                "http://schemas.microsoft.com/office/{name}"
            )));
        }
        assert!(!is_rich_data_relationship(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata"
        ));
        assert!(!is_rich_data_relationship(
            "http://schemas.microsoft.com/office/2017/10/relationships/person"
        ));
    }

    #[test]
    fn relationships_of_a_part() {
        let list = read_relationships(
            br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="t/image" Target="../media/image1.png"/><Relationship Id="rId2" Type="t/hyperlink" Target="https://example.com/a.png" TargetMode="External"/></Relationships>"#,
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].target, "../media/image1.png");
        assert_eq!(list[1].target_mode.as_deref(), Some("External"));
    }
}
