// si
use std::{
    hash::{
        DefaultHasher,
        Hasher,
    },
    io::Cursor,
};

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesStart,
        Event,
    },
};

use super::{
    RichText,
    Text,
    TextElement,
};
use crate::{
    reader::driver::{
        read_raw_element,
        xml_read_loop,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
        write_text_node_no_escape,
    },
};

#[derive(Clone, Default, Debug)]
pub(crate) struct SharedStringItem {
    text:      Option<Text>,
    rich_text: Option<RichText>,
    /// The furigana (`rPh`) and `phoneticPr`, as read.
    phonetic:  Option<Box<str>>,
}

impl SharedStringItem {
    #[inline]
    pub(crate) fn text(&self) -> Option<&Text> {
        self.text.as_ref()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use text()")]
    pub(crate) fn get_text(&self) -> Option<&Text> {
        self.text()
    }

    #[inline]
    pub(crate) fn text_mut(&mut self) -> Option<&mut Text> {
        self.text.as_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use text_mut()")]
    pub(crate) fn get_text_mut(&mut self) -> Option<&mut Text> {
        self.text_mut()
    }

    #[inline]
    pub(crate) fn set_text(&mut self, value: Text) -> &mut Self {
        self.text = Some(value);
        self
    }

    #[inline]
    pub(crate) fn remove_text(&mut self) -> &mut Self {
        self.text = None;
        self
    }

    #[inline]
    pub(crate) fn rich_text(&self) -> Option<&RichText> {
        self.rich_text.as_ref()
    }
    
    #[inline]
    #[deprecated(since = "3.0.0", note = "Use rich_text()")]
    pub(crate) fn get_rich_text(&self) -> Option<&RichText> {
        self.rich_text()
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn rich_text_mut(&mut self) -> Option<&mut RichText> {
        self.rich_text.as_mut()
    }

    #[inline]
    #[allow(dead_code)]
    #[deprecated(since = "3.0.0", note = "Use rich_text_mut()")]
    pub(crate) fn get_rich_text_mut(&mut self) -> Option<&mut RichText> {
        self.rich_text_mut()
    }

    #[inline]
    pub(crate) fn set_rich_text(&mut self, value: RichText) -> &mut Self {
        self.rich_text = Some(value);
        self
    }

    #[inline]
    pub(crate) fn remove_rich_text(&mut self) -> &mut Self {
        self.rich_text = None;
        self
    }

    /// The furigana (`<rPh>...</rPh><phoneticPr .../>`) as the file has them.
    #[inline]
    pub(crate) fn phonetic(&self) -> Option<&str> {
        self.phonetic.as_deref()
    }

    #[inline]
    pub(crate) fn set_phonetic(&mut self, value: Option<&str>) -> &mut Self {
        self.phonetic = value.map(Into::into);
        self
    }

    /// The text of the item, as a cell shows it.
    pub(crate) fn plain_text(&self) -> String {
        match (&self.text, &self.rich_text) {
            (_, Some(r)) => r.text().to_string(),
            (Some(t), None) => t.value().to_string(),
            _ => String::new(),
        }
    }

    pub(crate) fn hash_u64(&self) -> u64 {
        let mut h = DefaultHasher::default();
        let content = format!(
            "{}{}{}",
            self.phonetic.as_deref().unwrap_or(""),
            self.text
                .as_ref()
                .map_or(String::from("NONE"), Text::hash_code),
            self.rich_text
                .as_ref()
                .map_or(String::from("NONE"), RichText::hash_code)
        );
        h.write(content.as_bytes());
        h.finish()
    }

    #[deprecated(since = "3.0.0", note = "Use hash_u64()")]
    pub(crate) fn get_hash_u64(&self) -> u64 {
        self.hash_u64()
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        _e: &BytesStart,
    ) {
        self.read_until(reader, b"si");
    }

    /// Reads the children of `<si>` or of an inline string `<is>` (same
    /// content model) up to the closing `end` tag.
    pub(crate) fn read_until<R: std::io::BufRead>(&mut self, reader: &mut Reader<R>, end: &[u8]) {
        let mut vec_text_element: Vec<TextElement> = Vec::new();
        let mut phonetic = String::new();

        xml_read_loop!(
            reader,
            Event::Empty(ref e) => {
                if e.name().into_inner() == b"phoneticPr" {
                    phonetic.push_str(&read_raw_element(reader, e, true));
                }
            },
            Event::Start(ref e) => {
                match e.name().into_inner() {
                    b"t" => {
                        let mut obj = Text::default();
                        obj.set_attributes(reader, e);
                        self.set_text(obj);
                    }
                    b"r" => {
                        let mut obj = TextElement::default();
                        obj.set_attributes(reader, e);
                        vec_text_element.push(obj);
                    }
                    b"rPh" | b"phoneticPr" => {
                        phonetic.push_str(&read_raw_element(reader, e, false));
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == end {
                    // Only the default without furigana: as if there were none.
                    if phonetic.contains("<rPh") {
                        self.phonetic = Some(phonetic.into());
                    }
                    if !vec_text_element.is_empty() {
                        let mut obj = RichText::default();
                        obj.set_rich_text_elements(vec_text_element);
                        self.set_rich_text(obj);
                    }
                    return;
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", String::from_utf8_lossy(end))
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // si
        write_start_tag(writer, "si", vec![], false);

        // t
        if let Some(v) = &self.text {
            v.write_to(writer);
        }

        // r
        if let Some(v) = &self.rich_text {
            v.write_to(writer);
        }

        // rPh, phoneticPr
        if let Some(v) = &self.phonetic {
            write_text_node_no_escape(writer, &**v);
        } else {
            write_start_tag(writer, "phoneticPr", vec![("fontId", "1").into()], true);
        }

        write_end_tag(writer, "si");
    }
}
