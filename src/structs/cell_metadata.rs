use std::io::{
    self,
    Cursor,
};

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesDecl,
        BytesStart,
        Event,
    },
};

use crate::{
    reader::driver::get_attribute,
    writer::driver::{
        write_end_tag,
        write_new_line,
        write_start_tag,
    },
};

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const DYNAMIC_ARRAY_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray";
const DYNAMIC_ARRAY_URI: &str = "{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}";
// The name of the metadata type of dynamic array formulas (MS-XLSX,
// "Dynamic Array Properties").
const DYNAMIC_ARRAY_TYPE: &str = "XLDAPR";

/// What a block of cell metadata says about the cells that refer to it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CellMetadataBlock {
    /// Something this crate does not keep (the MDX metadata of cube
    /// functions, for example).
    #[default]
    Other,
    /// The formula of the cell is a dynamic array formula
    /// (`xda:dynamicArrayProperties fDynamic="1"`).
    DynamicArray { collapsed: bool },
}

/// The cell metadata of a workbook (`cellMetadata` of `xl/metadata.xml`):
/// the blocks cells refer to with their `cm` attribute, a 1-based index.
///
/// Only the dynamic array properties are kept. Cells carry them with their
/// formula ([`CellFormula::dynamic_array`]); this table says what the `cm`
/// of a file means while its sheets are read, and collects the blocks the
/// cells need while a workbook is written. Value metadata (`vm`, rich
/// values) is not kept.
///
/// [`CellFormula::dynamic_array`]: crate::structs::CellFormula::dynamic_array
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellMetadata {
    blocks: Vec<CellMetadataBlock>,
}

impl CellMetadata {
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// What the block a cell refers to with `cm` says.
    #[inline]
    pub(crate) fn block(&self, cm: u32) -> CellMetadataBlock {
        cm.checked_sub(1)
            .and_then(|i| self.blocks.get(i as usize))
            .copied()
            .unwrap_or_default()
    }

    /// The `cm` of a dynamic array formula: the block that says so, added
    /// when there is none yet.
    pub(crate) fn dynamic_array_index(&mut self, collapsed: bool) -> u32 {
        let wanted = CellMetadataBlock::DynamicArray { collapsed };
        let i = match self.blocks.iter().position(|b| *b == wanted) {
            Some(i) => i,
            None => {
                self.blocks.push(wanted);
                self.blocks.len() - 1
            }
        };
        u32::try_from(i + 1).unwrap()
    }

    /// The table to write a workbook with. `keep_indices`: some sheets are
    /// written as they were read, with the `cm` of the file, so every block
    /// stays where it was; otherwise only the blocks the written cells ask
    /// for are in it.
    pub(crate) fn for_writing(&self, keep_indices: bool) -> Self {
        if keep_indices {
            self.clone()
        } else {
            Self::default()
        }
    }

    /// Reads `xl/metadata.xml`.
    pub(crate) fn read<R: io::BufRead>(data: R) -> Self {
        let mut reader = Reader::from_reader(data);
        reader.config_mut().trim_text(true);

        // metadataType names, in order (`rc t` is a 1-based index into them).
        let mut types: Vec<String> = Vec::new();
        // The blocks of the dynamic array futureMetadata: (fDynamic, fCollapsed).
        let mut properties: Vec<(bool, bool)> = Vec::new();
        // The records of each cellMetadata block: (t, v).
        let mut cells: Vec<Vec<(usize, usize)>> = Vec::new();

        #[derive(PartialEq)]
        enum Section {
            None,
            DynamicArray,
            Cell,
        }
        let mut section = Section::None;
        let flag = |e: &BytesStart, name: &[u8]| {
            get_attribute(e, name).is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        };
        let index = |e: &BytesStart, name: &[u8]| {
            get_attribute(e, name).and_then(|v| v.parse::<usize>().ok())
        };

        let mut buf = Vec::new();
        loop {
            buf.clear();
            let (e, is_empty) = match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => (e, false),
                Ok(Event::Empty(e)) => (e, true),
                Ok(Event::End(e)) => {
                    if matches!(
                        e.local_name().into_inner(),
                        b"futureMetadata" | b"cellMetadata"
                    ) {
                        section = Section::None;
                    }
                    continue;
                }
                // A part that cannot be read says nothing about the cells.
                Ok(Event::Eof) | Err(_) => break,
                Ok(_) => continue,
            };
            match e.local_name().into_inner() {
                b"metadataType" => {
                    types.push(get_attribute(&e, b"name").unwrap_or_default());
                }
                b"futureMetadata" if !is_empty => {
                    let name = get_attribute(&e, b"name").unwrap_or_default();
                    if name == DYNAMIC_ARRAY_TYPE {
                        section = Section::DynamicArray;
                    }
                }
                b"cellMetadata" if !is_empty => section = Section::Cell,
                b"bk" => match section {
                    Section::DynamicArray => properties.push((false, false)),
                    Section::Cell => cells.push(Vec::new()),
                    Section::None => {}
                },
                b"dynamicArrayProperties" if section == Section::DynamicArray => {
                    if let Some(p) = properties.last_mut() {
                        *p = (flag(&e, b"fDynamic"), flag(&e, b"fCollapsed"));
                    }
                }
                b"rc" if section == Section::Cell => {
                    if let (Some(block), Some(t), Some(v)) =
                        (cells.last_mut(), index(&e, b"t"), index(&e, b"v"))
                    {
                        block.push((t, v));
                    }
                }
                _ => {}
            }
        }

        let blocks = cells
            .iter()
            .map(|records| {
                records
                    .iter()
                    .find_map(|(t, v)| {
                        let name = types.get(t.checked_sub(1)?)?;
                        let (dynamic, collapsed) = *properties.get(*v)?;
                        (name == DYNAMIC_ARRAY_TYPE && dynamic)
                            .then_some(CellMetadataBlock::DynamicArray { collapsed })
                    })
                    .unwrap_or_default()
            })
            .collect();
        Self { blocks }
    }

    /// Writes `xl/metadata.xml`. Every block refers to dynamic array
    /// properties; one that was something else in the file read (and that
    /// only sheets written as they were read can refer to) says that the
    /// formula is not a dynamic array.
    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // The distinct properties, and the one each block refers to.
        let mut properties: Vec<(bool, bool)> = Vec::new();
        let records: Vec<usize> = self
            .blocks
            .iter()
            .map(|b| {
                let p = match b {
                    CellMetadataBlock::DynamicArray { collapsed } => (true, *collapsed),
                    CellMetadataBlock::Other => (false, false),
                };
                properties.iter().position(|x| *x == p).unwrap_or_else(|| {
                    properties.push(p);
                    properties.len() - 1
                })
            })
            .collect();

        writer
            .write_event(Event::Decl(BytesDecl::new(
                "1.0",
                Some("UTF-8"),
                Some("yes"),
            )))
            .unwrap();
        write_new_line(writer);

        write_start_tag(
            writer,
            "metadata",
            vec![
                ("xmlns", MAIN_NS).into(),
                ("xmlns:xda", DYNAMIC_ARRAY_NS).into(),
            ],
            false,
        );

        // metadataTypes
        write_start_tag(writer, "metadataTypes", vec![("count", "1").into()], false);
        write_start_tag(
            writer,
            "metadataType",
            vec![
                ("name", DYNAMIC_ARRAY_TYPE).into(),
                ("minSupportedVersion", "120000").into(),
                ("copy", "1").into(),
                ("pasteAll", "1").into(),
                ("pasteValues", "1").into(),
                ("merge", "1").into(),
                ("splitFirst", "1").into(),
                ("rowColShift", "1").into(),
                ("clearFormats", "1").into(),
                ("clearComments", "1").into(),
                ("assign", "1").into(),
                ("coerce", "1").into(),
                ("cellMeta", "1").into(),
            ],
            true,
        );
        write_end_tag(writer, "metadataTypes");

        // futureMetadata
        let count = properties.len().to_string();
        write_start_tag(
            writer,
            "futureMetadata",
            vec![
                ("name", DYNAMIC_ARRAY_TYPE).into(),
                ("count", &count).into(),
            ],
            false,
        );
        let bit = |v: bool| if v { "1" } else { "0" };
        for (dynamic, collapsed) in &properties {
            write_start_tag(writer, "bk", vec![], false);
            write_start_tag(writer, "extLst", vec![], false);
            write_start_tag(writer, "ext", vec![("uri", DYNAMIC_ARRAY_URI).into()], false);
            write_start_tag(
                writer,
                "xda:dynamicArrayProperties",
                vec![
                    ("fDynamic", bit(*dynamic)).into(),
                    ("fCollapsed", bit(*collapsed)).into(),
                ],
                true,
            );
            write_end_tag(writer, "ext");
            write_end_tag(writer, "extLst");
            write_end_tag(writer, "bk");
        }
        write_end_tag(writer, "futureMetadata");

        // cellMetadata
        let count = records.len().to_string();
        write_start_tag(writer, "cellMetadata", vec![("count", &count).into()], false);
        for v in records {
            let v = v.to_string();
            write_start_tag(writer, "bk", vec![], false);
            write_start_tag(writer, "rc", vec![("t", "1").into(), ("v", &v).into()], true);
            write_end_tag(writer, "bk");
        }
        write_end_tag(writer, "cellMetadata");

        write_end_tag(writer, "metadata");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray">
<metadataTypes count="2"><metadataType name="XLRICHVALUE" minSupportedVersion="120000"/><metadataType name="XLDAPR" minSupportedVersion="120000" cellMeta="1"/></metadataTypes>
<futureMetadata name="XLRICHVALUE" count="1"><bk><extLst><ext uri="{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"><xlrd:rvb i="0"/></ext></extLst></bk></futureMetadata>
<futureMetadata name="XLDAPR" count="2"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="1"/></ext></extLst></bk></futureMetadata>
<cellMetadata count="3"><bk><rc t="2" v="0"/></bk><bk><rc t="1" v="0"/></bk><bk><rc t="2" v="1"/></bk></cellMetadata>
<valueMetadata count="1"><bk><rc t="1" v="0"/></bk></valueMetadata>
</metadata>"#;

    fn written(table: &CellMetadata) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        table.write_to(&mut writer);
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    #[test]
    fn reads_the_dynamic_array_blocks() {
        let table = CellMetadata::read(FILE.as_bytes());
        assert_eq!(
            table.block(1),
            CellMetadataBlock::DynamicArray { collapsed: false }
        );
        // A record of another type (the rich value type here).
        assert_eq!(table.block(2), CellMetadataBlock::Other);
        assert_eq!(
            table.block(3),
            CellMetadataBlock::DynamicArray { collapsed: true }
        );
        // No block: `cm` is 1-based, and the value metadata is not cell metadata.
        assert_eq!(table.block(0), CellMetadataBlock::Other);
        assert_eq!(table.block(4), CellMetadataBlock::Other);
    }

    #[test]
    fn writes_what_it_reads() {
        let table = CellMetadata::read(FILE.as_bytes());
        let xml = written(&table);
        assert_eq!(CellMetadata::read(xml.as_bytes()), table);
        assert!(xml.contains(r#"<cellMetadata count="3">"#));
        // The block that was something else says "not a dynamic array".
        assert!(xml.contains(r#"<xda:dynamicArrayProperties fDynamic="0" fCollapsed="0"/>"#));
    }

    #[test]
    fn adds_blocks_as_cells_ask_for_them() {
        let mut table = CellMetadata::default();
        assert!(table.is_empty());
        assert_eq!(table.dynamic_array_index(false), 1);
        assert_eq!(table.dynamic_array_index(true), 2);
        assert_eq!(table.dynamic_array_index(false), 1);
        let xml = written(&table);
        assert!(xml.contains(r#"<futureMetadata name="XLDAPR" count="2">"#));
        assert!(xml.contains(r#"<bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk>"#));

        // Sheets written as they were read keep their indices.
        let read = CellMetadata::read(FILE.as_bytes());
        let mut kept = read.for_writing(true);
        assert_eq!(kept.dynamic_array_index(false), 1);
        assert_eq!(kept.dynamic_array_index(true), 3);
        let mut fresh = read.for_writing(false);
        assert_eq!(fresh.dynamic_array_index(true), 1);
    }
}
