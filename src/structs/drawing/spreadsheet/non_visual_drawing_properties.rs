// xdr:cNvPr
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesStart,
        Event,
    },
};

use super::super::super::{
    BooleanValue,
    StringValue,
    UInt32Value,
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
    },
};

#[derive(Clone, Default, Debug)]
pub struct NonVisualDrawingProperties {
    id:     UInt32Value,
    name:   StringValue,
    hidden: BooleanValue,
    /// The alternative text (`descr`) and its title.
    descr:  StringValue,
    title:  StringValue,
}

impl NonVisualDrawingProperties {
    #[inline]
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use id()")]
    pub fn get_id(&self) -> u32 {
        self.id()
    }

    #[inline]
    pub fn set_id(&mut self, value: u32) -> &mut Self {
        self.id.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use name()")]
    pub fn get_name(&self) -> &str {
        self.name()
    }

    #[inline]
    pub fn set_name<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.name.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use hidden()")]
    pub fn get_hidden(&self) -> bool {
        self.hidden()
    }

    #[inline]
    pub fn set_hidden(&mut self, value: bool) -> &mut Self {
        self.hidden.set_value(value);
        self
    }

    /// The alternative text of the object (what a screen reader says).
    #[inline]
    #[must_use]
    pub fn description(&self) -> &str {
        self.descr.value_str()
    }

    #[inline]
    pub fn set_description<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.descr.set_value(value);
        self
    }

    /// The title of the alternative text.
    #[inline]
    #[must_use]
    pub fn title(&self) -> &str {
        self.title.value_str()
    }

    #[inline]
    pub fn set_title<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.title.set_value(value);
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        empty_flg: bool,
    ) {
        self.id.set_value_string(get_attribute(e, b"id").unwrap());
        self.name
            .set_value_string(get_attribute(e, b"name").unwrap());
        set_string_from_xml!(self, e, hidden, "hidden");
        // As written (escaped): the writer escapes it again.
        let unescaped = |v: String| quick_xml::escape::unescape(&v).map(std::borrow::Cow::into_owned).unwrap_or(v);
        if let Some(v) = get_attribute(e, b"descr") {
            self.descr.set_value(unescaped(v));
        }
        if let Some(v) = get_attribute(e, b"title") {
            self.title.set_value(unescaped(v));
        }

        if empty_flg {
            return;
        }

        xml_read_loop!(
            reader,
            Event::End(ref e) => {
                if matches!(e.name().into_inner(), b"xdr:cNvPr" | b"cNvPr") {
                    return;
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "cNvPr")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>, ole_id: usize) {
        let with_inner = ole_id > 0;
        // xdr:cNvPr
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        let id = self.id.value_string();
        attributes.push(("id", &id).into());
        attributes.push(("name", self.name.value_str()).into());
        if self.hidden.has_value() {
            attributes.push(("hidden", self.hidden.value_string()).into());
        }
        if self.descr.has_value() {
            attributes.push(("descr", self.descr.value_str()).into());
        }
        if self.title.has_value() {
            attributes.push(("title", self.title.value_str()).into());
        }
        write_start_tag(writer, "xdr:cNvPr", attributes, !with_inner);

        if with_inner {
            let spid = format!("_x0000_s{ole_id}");
            write_start_tag(writer, "a:extLst", vec![], false);
            write_start_tag(
                writer,
                "a:ext",
                vec![("uri", "{63B3BB69-23CF-44E3-9099-C40C66FF867C}").into()],
                false,
            );
            write_start_tag(writer, "a14:compatExt", vec![("spid", &spid).into()], true);

            write_end_tag(writer, "a:ext");
            write_end_tag(writer, "a:extLst");
            write_end_tag(writer, "xdr:cNvPr");
        }
    }
}
