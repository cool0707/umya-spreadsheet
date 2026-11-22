// headerFooter
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesStart,
        Event,
    },
};

use crate::{
    reader::driver::xml_read_loop,
    structs::{
        OddFooter,
        OddHeader,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Clone, Default, Debug)]
pub struct HeaderFooter {
    odd_header: OddHeader,
    odd_footer: OddFooter,
    /// `differentOddEven`, `differentFirst`, `scaleWithDoc`,
    /// `alignWithMargins`, as read.
    other_attributes: Vec<(Box<str>, Box<str>)>,
    /// The headers and footers of even pages and of the first page
    /// (`evenHeader`, `evenFooter`, `firstHeader`, `firstFooter`) that have
    /// something in them, as read.
    kept: Vec<Box<str>>,
}

impl HeaderFooter {
    #[inline]
    #[must_use]
    pub fn odd_header(&self) -> &OddHeader {
        &self.odd_header
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use odd_header()")]
    pub fn get_odd_header(&self) -> &OddHeader {
        self.odd_header()
    }

    #[inline]
    pub fn odd_header_mut(&mut self) -> &mut OddHeader {
        &mut self.odd_header
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use odd_header_mut()")]
    pub fn get_odd_header_mut(&mut self) -> &mut OddHeader {
        self.odd_header_mut()
    }

    #[inline]
    pub fn set_odd_header(&mut self, value: OddHeader) -> &mut Self {
        self.odd_header = value;
        self
    }

    #[inline]
    #[must_use]
    pub fn odd_footer(&self) -> &OddFooter {
        &self.odd_footer
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use odd_footer()")]
    pub fn get_odd_footer(&self) -> &OddFooter {
        self.odd_footer()
    }

    #[inline]
    pub fn odd_footer_mut(&mut self) -> &mut OddFooter {
        &mut self.odd_footer
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use odd_footer_mut()")]
    pub fn get_odd_footer_mut(&mut self) -> &mut OddFooter {
        self.odd_footer_mut()
    }

    #[inline]
    pub fn set_odd_footer(&mut self, value: OddFooter) -> &mut Self {
        self.odd_footer = value;
        self
    }

    #[inline]
    pub(crate) fn has_param(&self) -> bool {
        self.odd_header.has_param()
            || self.odd_footer.has_param()
            || !self.other_attributes.is_empty()
            || !self.kept.is_empty()
    }

    /// Reads the attributes of `headerFooter` (also when it is empty).
    pub(crate) fn set_options(&mut self, e: &BytesStart) {
        for attr in e.attributes().with_checks(false).flatten() {
            let key = attr.key.into_inner();
            if matches!(key, b"differentOddEven" | b"differentFirst" | b"scaleWithDoc" | b"alignWithMargins") {
                if let Ok(value) = crate::reader::driver::get_attribute_value(&attr) {
                    self.other_attributes.push((String::from_utf8_lossy(key).into(), value.into()));
                }
            }
        }
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        self.set_options(e);
        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                match e.name().into_inner() {
                b"oddHeader" => {
                    self.odd_header.set_attributes(reader, e);
                }
                b"oddFooter" => {
                    self.odd_footer.set_attributes(reader, e);
                }
                b"evenHeader" | b"evenFooter" | b"firstHeader" | b"firstFooter" => {
                    let xml = crate::reader::driver::read_raw_element(reader, e, false);
                    self.kept.push(xml.into());
                }
                _ => (),
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"headerFooter" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "headerFooter")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        if self.has_param() {
            // headerFooter
            let attributes: crate::structs::AttrCollection =
                self.other_attributes.iter().map(|(k, v)| (&**k, &**v).into()).collect();
            write_start_tag(writer, "headerFooter", attributes, false);

            // oddHeader
            self.odd_header().write_to(writer);

            // oddFooter
            self.odd_footer().write_to(writer);

            // evenHeader, evenFooter, firstHeader, firstFooter (as read, in order)
            for xml in &self.kept {
                crate::writer::driver::write_text_node_no_escape(writer, &**xml);
            }

            write_end_tag(writer, "headerFooter");
        }
    }
}
