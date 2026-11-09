// dxf
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
    Alignment,
    Borders,
    Fill,
    Font,
    NumberingFormat,
    Style,
};
use crate::{
    reader::driver::{
        get_attribute,
        xml_read_loop,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Clone, Default, Debug)]
pub(crate) struct DifferentialFormat {
    font:      Option<Box<Font>>,
    fill:      Option<Fill>,
    borders:   Option<Box<Borders>>,
    alignment: Option<Alignment>,
    numbering_format: Option<NumberingFormat>,
}

impl DifferentialFormat {
    #[inline]
    pub(crate) fn font(&self) -> Option<&Font> {
        self.font.as_deref()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use font()")]
    pub(crate) fn get_font(&self) -> Option<&Font> {
        self.font()
    }

    #[inline]
    pub(crate) fn font_mut(&mut self) -> Option<&mut Font> {
        self.font.as_deref_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use font_mut()")]
    pub(crate) fn get_font_mut(&mut self) -> Option<&mut Font> {
        self.font_mut()
    }

    #[inline]
    pub(crate) fn set_font(&mut self, value: Font) -> &mut Self {
        self.font = Some(Box::new(value));
        self
    }

    #[inline]
    pub(crate) fn fill(&self) -> Option<&Fill> {
        self.fill.as_ref()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use fill()")]
    pub(crate) fn get_fill(&self) -> Option<&Fill> {
        self.fill()
    }

    #[inline]
    pub(crate) fn fill_mut(&mut self) -> Option<&mut Fill> {
        self.fill.as_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use fill_mut()")]
    pub(crate) fn get_fill_mut(&mut self) -> Option<&mut Fill> {
        self.fill_mut()
    }

    #[inline]
    pub(crate) fn set_fill(&mut self, value: Fill) -> &mut Self {
        self.fill = Some(value);
        self
    }

    #[inline]
    pub(crate) fn borders(&self) -> Option<&Borders> {
        self.borders.as_deref()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use borders()")]
    pub(crate) fn get_borders(&self) -> Option<&Borders> {
        self.borders()
    }

    #[inline]
    pub(crate) fn borders_mut(&mut self) -> Option<&mut Borders> {
        self.borders.as_deref_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use borders_mut()")]
    pub(crate) fn get_borders_mut(&mut self) -> Option<&mut Borders> {
        self.borders_mut()
    }

    #[inline]
    pub(crate) fn set_borders(&mut self, value: Borders) -> &mut Self {
        self.borders = Some(Box::new(value));
        self
    }

    #[inline]
    pub(crate) fn alignment(&self) -> Option<&Alignment> {
        self.alignment.as_ref()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use alignment()")]
    pub(crate) fn get_alignment(&self) -> Option<&Alignment> {
        self.alignment()
    }

    #[inline]
    pub(crate) fn alignment_mut(&mut self) -> Option<&mut Alignment> {
        self.alignment.as_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use alignment_mut()")]
    pub(crate) fn get_alignment_mut(&mut self) -> Option<&mut Alignment> {
        self.alignment_mut()
    }

    #[inline]
    pub(crate) fn set_alignment(&mut self, value: Alignment) -> &mut Self {
        self.alignment = Some(value);
        self
    }

    #[inline]
    pub(crate) fn style(&self) -> Style {
        let mut style = Style::default();
        style.set_font_crate(self.font.as_deref().cloned());
        style.set_fill_crate(self.fill.clone());
        style.set_borders_crate(self.borders.as_deref().cloned());
        style.set_alignment_crate(self.alignment.clone());
        if let Some(v) = &self.numbering_format {
            style.set_numbering_format(v.clone());
        }
        style
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use style()")]
    pub(crate) fn get_style(&self) -> Style {
        self.style()
    }

    #[inline]
    pub(crate) fn set_style(&mut self, style: &Style) {
        self.font = style.font().cloned().map(Box::new);
        self.fill = style.fill().cloned();
        self.borders = style.borders().cloned().map(Box::new);
        self.alignment = style.alignment().cloned();
        self.numbering_format = style.numbering_format().cloned();
    }

    pub(crate) fn hash_code(&self) -> String {
        crate::helper::utils::md5_hash(format!(
            "{}{}{}{}{}",
            match &self.font {
                Some(v) => {
                    v.hash_code()
                }
                None => {
                    "None".into()
                }
            },
            match &self.fill {
                Some(v) => {
                    v.hash_code()
                }
                None => {
                    "None".into()
                }
            },
            match &self.borders {
                Some(v) => {
                    v.hash_code()
                }
                None => {
                    "None".into()
                }
            },
            match &self.alignment {
                Some(v) => {
                    v.hash_code()
                }
                None => {
                    "None".into()
                }
            },
            match &self.numbering_format {
                Some(v) => {
                    v.hash_code()
                }
                None => {
                    "None".into()
                }
            },
        ))
    }

    #[deprecated(since = "3.0.0", note = "Use hash_code()")]
    pub(crate) fn get_hash_code(&self) -> String {
        self.hash_code()
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        _e: &BytesStart,
    ) {
        xml_read_loop!(
            reader,
            Event::Empty(ref e) => {
                if e.local_name().into_inner() == b"numFmt" {
                    self.numbering_format = Self::numbering_format_of(e);
                }
            },
            Event::Start(ref e) => {
                match e.local_name().into_inner() {
                    b"font" => {
                        let mut obj = Font::default();
                        obj.set_attributes(reader, e);
                        self.set_font(obj);
                    }
                    b"numFmt" => {
                        self.numbering_format = Self::numbering_format_of(e);
                    }
                    b"fill" => {
                        let mut obj = Fill::default();
                        obj.set_attributes(reader, e);
                        self.set_fill(obj);
                    }
                    b"border" => {
                        let mut obj = Borders::default();
                        obj.set_attributes(reader, e, false);
                        self.set_borders(obj);
                    }
                    b"alignment" => {
                        let mut obj = Alignment::default();
                        obj.set_attributes(reader, e);
                        self.set_alignment(obj);
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.local_name().into_inner() == b"dxf" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "dxf")
        );
    }

    /// `<numFmt numFmtId=".." formatCode=".."/>` of a dxf.
    fn numbering_format_of(e: &BytesStart) -> Option<NumberingFormat> {
        let id = get_attribute(e, b"numFmtId").and_then(|v| v.parse::<u32>().ok());
        let code = get_attribute(e, b"formatCode")
            .and_then(|v| quick_xml::escape::unescape(&v).ok().map(|c| c.to_string()));
        let mut obj = NumberingFormat::default();
        match (id, code) {
            (Some(id), Some(code)) => {
                obj.set_format_code(code);
                obj.set_number_format_id_crate(id);
            }
            (_, Some(code)) => {
                obj.set_format_code(code);
            }
            (Some(id), None) => {
                obj.set_number_format_id_crate(id);
                if let Some(code) = super::numbering_format::FILL_BUILT_IN_FORMAT_CODES.get(&id) {
                    obj.set_format_code_crate(*code);
                }
            }
            (None, None) => return None,
        }
        Some(obj)
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        self.write_to_tag(writer, "dxf", 0);
    }

    /// Writes the format as `tag` (dxf, x14:dxf); a number format of its own
    /// gets the id `custom_id` unless it was read with one.
    pub(crate) fn write_to_tag(
        &self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        tag: &str,
        custom_id: u32,
    ) {
        write_start_tag(writer, tag, vec![], false);

        // font
        if let Some(v) = &self.font {
            v.write_to_font(writer);
        }

        // numFmt
        if let Some(v) = &self.numbering_format {
            let id = match v.number_format_id() {
                999_999 => custom_id,
                id => id,
            };
            v.write_to(writer, id);
        }

        // fill
        if let Some(v) = &self.fill {
            v.write_to(writer);
        }

        // alignment
        if let Some(v) = &self.alignment {
            v.write_to(writer);
        }

        // border
        if let Some(v) = &self.borders {
            v.write_to(writer);
        }

        write_end_tag(writer, tag);
    }
}
