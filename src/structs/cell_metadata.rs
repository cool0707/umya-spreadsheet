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
const RICH_VALUE_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const RICH_VALUE_URI: &str = "{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}";
// The name of the metadata type of rich values (MS-XLSX, "Rich Value
// Metadata").
const RICH_VALUE_TYPE: &str = "XLRICHVALUE";

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

/// The metadata of the cells of a workbook (`xl/metadata.xml`).
///
/// `cellMetadata`: the blocks cells refer to with their `cm` attribute, a
/// 1-based index. Only the dynamic array properties are kept. Cells carry
/// them with their formula ([`CellFormula::dynamic_array`]); this table says
/// what the `cm` of a file means while its sheets are read, and collects the
/// blocks the cells need while a workbook is written.
///
/// `valueMetadata`: the blocks cells refer to with their `vm` attribute,
/// also 1-based: the rich value in the cell (a picture placed in it, for
/// example). Cells keep the `vm` they were read with
/// ([`Cell::value_meta_index`]), so these blocks stay where they were.
///
/// [`CellFormula::dynamic_array`]: crate::structs::CellFormula::dynamic_array
/// [`Cell::value_meta_index`]: crate::structs::Cell::value_meta_index
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CellMetadata {
    blocks:       Vec<CellMetadataBlock>,
    /// The rich value each block of value metadata refers to (an index into
    /// the rich values of the workbook), None for something else.
    value_blocks: Vec<Option<u32>>,
    /// While a workbook is written: cells refer to the value metadata.
    values_used:  bool,
}

impl CellMetadata {
    /// No cell metadata (`cm` says nothing).
    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Whether the value metadata is to be written: it has blocks, and
    /// written cells refer to them.
    #[inline]
    pub(crate) fn has_values(&self) -> bool {
        self.values_used && !self.value_blocks.is_empty()
    }

    /// Nothing to write `xl/metadata.xml` for.
    #[inline]
    pub(crate) fn is_unused(&self) -> bool {
        self.blocks.is_empty() && !self.has_values()
    }

    /// The rich value the block a cell refers to with `vm` stands for.
    #[inline]
    pub(crate) fn rich_value(&self, vm: u32) -> Option<u32> {
        vm.checked_sub(1)
            .and_then(|i| self.value_blocks.get(i as usize))
            .copied()
            .flatten()
    }

    /// While writing: whether a cell may keep its `vm` (it refers to a rich
    /// value). The value metadata is then written.
    #[inline]
    pub(crate) fn use_value(&mut self, vm: u32) -> bool {
        let found = self.rich_value(vm).is_some();
        self.values_used |= found;
        found
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
    /// for are in it. The value metadata always stays as it was (cells keep
    /// their `vm`); it is written when cells refer to it, which the sheets
    /// written as they were read may do.
    pub(crate) fn for_writing(&self, keep_indices: bool) -> Self {
        Self {
            blocks:       if keep_indices {
                self.blocks.clone()
            } else {
                Vec::new()
            },
            value_blocks: self.value_blocks.clone(),
            values_used:  keep_indices,
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
        // The blocks of the rich value futureMetadata: the rich value.
        let mut rich_values: Vec<Option<u32>> = Vec::new();
        // The records of each cellMetadata block: (t, v).
        let mut cells: Vec<Vec<(usize, usize)>> = Vec::new();
        // The same for valueMetadata.
        let mut values: Vec<Vec<(usize, usize)>> = Vec::new();

        #[derive(PartialEq)]
        enum Section {
            None,
            DynamicArray,
            RichValue,
            Cell,
            Value,
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
                        b"futureMetadata" | b"cellMetadata" | b"valueMetadata"
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
                    } else if name == RICH_VALUE_TYPE {
                        section = Section::RichValue;
                    }
                }
                b"cellMetadata" if !is_empty => section = Section::Cell,
                b"valueMetadata" if !is_empty => section = Section::Value,
                b"bk" => match section {
                    Section::DynamicArray => properties.push((false, false)),
                    Section::RichValue => rich_values.push(None),
                    Section::Cell => cells.push(Vec::new()),
                    Section::Value => values.push(Vec::new()),
                    Section::None => {}
                },
                b"rvb" if section == Section::RichValue => {
                    if let Some(v) = rich_values.last_mut() {
                        *v = get_attribute(&e, b"i").and_then(|v| v.parse::<u32>().ok());
                    }
                }
                b"dynamicArrayProperties" if section == Section::DynamicArray => {
                    if let Some(p) = properties.last_mut() {
                        *p = (flag(&e, b"fDynamic"), flag(&e, b"fCollapsed"));
                    }
                }
                b"rc" if section == Section::Cell || section == Section::Value => {
                    let list = if section == Section::Cell {
                        &mut cells
                    } else {
                        &mut values
                    };
                    if let (Some(block), Some(t), Some(v)) =
                        (list.last_mut(), index(&e, b"t"), index(&e, b"v"))
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
        let value_blocks = values
            .iter()
            .map(|records| {
                records.iter().find_map(|(t, v)| {
                    let name = types.get(t.checked_sub(1)?)?;
                    (name == RICH_VALUE_TYPE)
                        .then(|| rich_values.get(*v).copied().flatten())
                        .flatten()
                })
            })
            .collect();
        Self {
            blocks,
            value_blocks,
            values_used: false,
        }
    }

    /// Writes `xl/metadata.xml`. Every block of cell metadata refers to
    /// dynamic array properties; one that was something else in the file
    /// read (and that only sheets written as they were read can refer to)
    /// says that the formula is not a dynamic array. The blocks of value
    /// metadata refer to the rich values they did, one that was something
    /// else to none.
    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let has_cells = !self.blocks.is_empty();
        let has_values = self.has_values();
        // `rc t`: the 1-based index of the metadata type.
        let dynamic_array_type = "1";
        let rich_value_type = if has_cells { "2" } else { "1" };

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

        let mut attributes: crate::structs::AttrCollection = vec![("xmlns", MAIN_NS).into()];
        if has_values {
            attributes.push(("xmlns:xlrd", RICH_VALUE_NS).into());
        }
        if has_cells {
            attributes.push(("xmlns:xda", DYNAMIC_ARRAY_NS).into());
        }
        write_start_tag(writer, "metadata", attributes, false);

        // metadataTypes
        let count = (usize::from(has_cells) + usize::from(has_values)).to_string();
        write_start_tag(writer, "metadataTypes", vec![("count", &count).into()], false);
        for (name, cell_meta) in [
            (DYNAMIC_ARRAY_TYPE, true),
            (RICH_VALUE_TYPE, false),
        ] {
            if !(if cell_meta { has_cells } else { has_values }) {
                continue;
            }
            let mut attributes: crate::structs::AttrCollection = vec![
                ("name", name).into(),
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
            ];
            if cell_meta {
                attributes.push(("cellMeta", "1").into());
            }
            write_start_tag(writer, "metadataType", attributes, true);
        }
        write_end_tag(writer, "metadataTypes");

        // futureMetadata of the dynamic arrays
        if has_cells {
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
        }

        // futureMetadata of the rich values: one block for each block of
        // value metadata.
        if has_values {
            let count = self.value_blocks.len().to_string();
            write_start_tag(
                writer,
                "futureMetadata",
                vec![("name", RICH_VALUE_TYPE).into(), ("count", &count).into()],
                false,
            );
            for rich_value in &self.value_blocks {
                let Some(rich_value) = rich_value else {
                    write_start_tag(writer, "bk", vec![], true);
                    continue;
                };
                let rich_value = rich_value.to_string();
                write_start_tag(writer, "bk", vec![], false);
                write_start_tag(writer, "extLst", vec![], false);
                write_start_tag(writer, "ext", vec![("uri", RICH_VALUE_URI).into()], false);
                write_start_tag(writer, "xlrd:rvb", vec![("i", &rich_value).into()], true);
                write_end_tag(writer, "ext");
                write_end_tag(writer, "extLst");
                write_end_tag(writer, "bk");
            }
            write_end_tag(writer, "futureMetadata");
        }

        // cellMetadata
        if has_cells {
            let count = records.len().to_string();
            write_start_tag(writer, "cellMetadata", vec![("count", &count).into()], false);
            for v in records {
                let v = v.to_string();
                write_start_tag(writer, "bk", vec![], false);
                write_start_tag(
                    writer,
                    "rc",
                    vec![("t", dynamic_array_type).into(), ("v", &v).into()],
                    true,
                );
                write_end_tag(writer, "bk");
            }
            write_end_tag(writer, "cellMetadata");
        }

        // valueMetadata
        if has_values {
            let count = self.value_blocks.len().to_string();
            write_start_tag(writer, "valueMetadata", vec![("count", &count).into()], false);
            for v in 0..self.value_blocks.len() {
                let v = v.to_string();
                write_start_tag(writer, "bk", vec![], false);
                write_start_tag(
                    writer,
                    "rc",
                    vec![("t", rich_value_type).into(), ("v", &v).into()],
                    true,
                );
                write_end_tag(writer, "bk");
            }
            write_end_tag(writer, "valueMetadata");
        }

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
    fn reads_and_writes_the_value_metadata() {
        let file = FILE
            .replace(
                r#"<xlrd:rvb i="0"/></ext></extLst></bk>"#,
                r#"<xlrd:rvb i="0"/></ext></extLst></bk><bk><extLst><ext uri="{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"><xlrd:rvb i="5"/></ext></extLst></bk>"#,
            )
            .replace(
                r#"<valueMetadata count="1"><bk><rc t="1" v="0"/></bk>"#,
                r#"<valueMetadata count="3"><bk><rc t="1" v="1"/></bk><bk><rc t="2" v="0"/></bk><bk><rc t="1" v="0"/></bk>"#,
            );
        let table = CellMetadata::read(file.as_bytes());
        assert_eq!(table.rich_value(0), None);
        assert_eq!(table.rich_value(1), Some(5));
        // A record of another type.
        assert_eq!(table.rich_value(2), None);
        assert_eq!(table.rich_value(3), Some(0));
        assert_eq!(table.rich_value(4), None);

        // Not written until a cell refers to it.
        let mut writing = table.for_writing(false);
        assert!(writing.is_unused());
        assert!(!writing.use_value(2));
        assert!(writing.is_unused());
        assert!(writing.use_value(3));
        assert!(!writing.is_unused());
        let xml = written(&writing);
        assert!(xml.contains(r#"<metadataTypes count="1"><metadataType name="XLRICHVALUE""#));
        assert!(xml.contains(concat!(
            r#"<futureMetadata name="XLRICHVALUE" count="3">"#,
            r#"<bk><extLst><ext uri="{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"><xlrd:rvb i="5"/></ext></extLst></bk>"#,
            r#"<bk/>"#,
            r#"<bk><extLst><ext uri="{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}"><xlrd:rvb i="0"/></ext></extLst></bk>"#,
            r#"</futureMetadata>"#
        )));
        assert!(xml.contains(r#"<valueMetadata count="3"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk><bk><rc t="1" v="2"/></bk></valueMetadata>"#));
        let again = CellMetadata::read(xml.as_bytes());
        assert_eq!((1..=3).map(|vm| again.rich_value(vm)).collect::<Vec<_>>(), vec![Some(5), None, Some(0)]);

        // With cell metadata, the rich values are the second type.
        writing.dynamic_array_index(false);
        let xml = written(&writing);
        assert!(xml.contains(r#"<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata>"#));
        assert!(xml.contains(r#"<valueMetadata count="3"><bk><rc t="2" v="0"/></bk>"#));
        let again = CellMetadata::read(xml.as_bytes());
        assert_eq!(again.rich_value(1), Some(5));
        assert_eq!(again.block(1), CellMetadataBlock::DynamicArray { collapsed: false });
    }

    #[test]
    fn writes_what_it_reads() {
        let table = CellMetadata::read(FILE.as_bytes());
        // As for a workbook with sheets written as they were read: every
        // block stays, and the value metadata is written.
        let xml = written(&table.for_writing(true));
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
