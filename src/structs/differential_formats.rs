// dxfs
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesStart,
        Event,
    },
};

use super::{
    DifferentialFormat,
    Style,
};
use crate::{
    reader::driver::xml_read_loop,
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Clone, Default, Debug)]
pub(crate) struct DifferentialFormats {
    differential_format: Vec<DifferentialFormat>,
}

impl DifferentialFormats {
    #[inline]
    pub(crate) fn differential_format(&self) -> &[DifferentialFormat] {
        &self.differential_format
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use differential_format()")]
    pub(crate) fn get_differential_format(&self) -> &[DifferentialFormat] {
        self.differential_format()
    }

    #[inline]
    pub(crate) fn differential_format_mut(&mut self) -> &mut Vec<DifferentialFormat> {
        &mut self.differential_format
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use differential_format_mut()")]
    pub(crate) fn get_differential_format_mut(&mut self) -> &mut Vec<DifferentialFormat> {
        self.differential_format_mut()
    }

    #[inline]
    pub(crate) fn set_differential_format(&mut self, value: DifferentialFormat) -> &mut Self {
        self.differential_format.push(value);
        self
    }

    #[inline]
    pub(crate) fn style(&self, id: usize) -> Style {
        self.find_style(id)
            .unwrap_or_else(|| panic!("Not found differential format with ID: {id}."))
    }

    pub(crate) fn find_style(&self, id: usize) -> Option<Style> {
        self.differential_format
            .get(id)
            .map(|differential_format| differential_format.clone().style())
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use style()")]
    pub(crate) fn get_style(&self, id: usize) -> Style {
        self.style(id)
    }

    pub(crate) fn set_style(&mut self, style: &Style) -> u32 {
        let mut differential_format = DifferentialFormat::default();
        differential_format.set_style(style);

        let hash_code = differential_format.hash_code();
        let mut id = 0;
        for v in &self.differential_format {
            if v.hash_code() == hash_code {
                return id;
            }
            id += 1;
        }

        self.set_differential_format(differential_format);
        id
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        _e: &BytesStart,
    ) {
        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                if e.name().into_inner() == b"dxf" {
                    let mut obj = DifferentialFormat::default();
                    obj.set_attributes(reader, e);
                    self.set_differential_format(obj);
                }
            },
            // <dxf/> keeps its place so that the ids of the next ones hold.
            Event::Empty(ref e) => {
                if e.name().into_inner() == b"dxf" {
                    self.set_differential_format(DifferentialFormat::default());
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"dxfs" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "dxfs")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        if !self.differential_format.is_empty() {
            // dxfs
            write_start_tag(
                writer,
                "dxfs",
                vec![("count", &self.differential_format.len().to_string()).into()],
                false,
            );

            // dxf
            for (i, differential_format) in self.differential_format.iter().enumerate() {
                differential_format.write_to_tag(writer, "dxf", custom_numfmt_id(i));
            }

            write_end_tag(writer, "dxfs");
        }
    }
}

/// The id written for the number format of its own of the dxf at `index`
/// (an id that the workbook's number formats do not use).
pub(crate) fn custom_numfmt_id(index: usize) -> u32 {
    1000 + u32::try_from(index).unwrap_or(0)
}
