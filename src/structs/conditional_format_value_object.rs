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
    BooleanValue,
    ConditionalFormatValueObjectValues,
    EnumValue,
    StringValue,
};
use crate::{
    reader::driver::{
        get_attribute,
        set_string_from_xml,
        xml_read_loop,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
        write_text_node,
    },
};

/// A threshold of a color scale, data bar or icon set (`cfvo`, and
/// `x14:cfvo` whose value is an `xm:f` child).
#[derive(Clone, Default, Debug)]
pub struct ConditionalFormatValueObject {
    r#type: EnumValue<ConditionalFormatValueObjectValues>,
    val:    StringValue,
    gte:    BooleanValue,
}

impl ConditionalFormatValueObject {
    #[inline]
    #[must_use]
    pub fn get_type(&self) -> &ConditionalFormatValueObjectValues {
        self.r#type.value()
    }

    #[inline]
    pub fn set_type(&mut self, value: ConditionalFormatValueObjectValues) -> &mut Self {
        self.r#type.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn val(&self) -> &str {
        self.val.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use val()")]
    pub fn get_val(&self) -> &str {
        self.val()
    }

    #[inline]
    pub fn set_val<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.val.set_value(value.into());
        self
    }

    /// The value itself reaches the threshold (icon sets; true when not given).
    #[inline]
    #[must_use]
    pub fn gte(&self) -> bool {
        !self.gte.has_value() || self.gte.value()
    }

    #[inline]
    pub fn set_gte(&mut self, value: bool) -> &mut Self {
        self.gte.set_value(value);
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        empty_flg: bool,
    ) {
        set_string_from_xml!(self, e, r#type, "type");
        set_string_from_xml!(self, e, val, "val");
        set_string_from_xml!(self, e, gte, "gte");

        if empty_flg {
            return;
        }

        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                // x14: <xm:f>value</xm:f>
                if e.local_name().into_inner() == b"f" {
                    let mut buf = Vec::new();
                    if let Ok(text) = reader.read_text_into(e.name(), &mut buf) {
                        self.val.set_value(crate::helper::utils::unescape_xml_text(&text));
                    }
                }
            },
            Event::End(ref e) => {
                if e.local_name().into_inner() == b"cfvo" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "cfvo")
        );
    }

    fn attributes(&self, with_val: bool) -> crate::structs::AttrCollection<'_> {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.r#type.has_value() {
            attributes.push(("type", self.r#type.value_string()).into());
        }
        if with_val && self.val.has_value() {
            attributes.push(("val", self.val.value_str()).into());
        }
        if self.gte.has_value() {
            attributes.push(("gte", self.gte.value_string()).into());
        }
        attributes
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // cfvo
        write_start_tag(writer, "cfvo", self.attributes(true), true);
    }

    pub(crate) fn write_to_x14(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // x14:cfvo
        let empty = !self.val.has_value();
        write_start_tag(writer, "x14:cfvo", self.attributes(false), empty);
        if !empty {
            write_start_tag(writer, "xm:f", vec![], false);
            write_text_node(writer, self.val.value_str());
            write_end_tag(writer, "xm:f");
            write_end_tag(writer, "x14:cfvo");
        }
    }
}
