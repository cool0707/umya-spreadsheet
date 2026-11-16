// threadedComment
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
    Coordinate,
    DateTimeValue,
    StringValue,
    office2019::threaded_comment_text::ThreadedCommentText,
    reader::driver::get_attribute,
    set_string_from_xml,
    traits::AdjustmentCoordinate,
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
    xml_read_loop,
};

#[derive(Clone, Default, Debug)]
pub struct ThreadedComment {
    coordinate:            Coordinate,
    d_t:                   DateTimeValue,
    threaded_comment_text: ThreadedCommentText,
    id:                    StringValue,
    person_id:             Box<str>,
    parent_id:             Box<str>,
    done:                  bool,
    mentions:              Vec<Mention>,
}

/// A mention of a person in the text of a threaded comment (`@name`).
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Mention {
    /// The person (`Person::id`) mentioned.
    pub person_id:   String,
    pub id:          String,
    /// Where the mention is in the text, in UTF-16 code units.
    pub start_index: u32,
    pub length:      u32,
}

impl ThreadedComment {
    #[inline]
    #[must_use]
    pub fn coordinate(&self) -> &Coordinate {
        &self.coordinate
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use coordinate()")]
    pub fn get_coordinate(&self) -> &Coordinate {
        self.coordinate()
    }

    #[inline]
    pub fn coordinate_mut(&mut self) -> &mut Coordinate {
        &mut self.coordinate
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use coordinate_mut()")]
    pub fn get_coordinate_mut(&mut self) -> &mut Coordinate {
        self.coordinate_mut()
    }

    #[inline]
    #[must_use]
    pub fn d_t(&self) -> &str {
        self.d_t.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use d_t()")]
    pub fn get_d_t(&self) -> &str {
        self.d_t()
    }

    #[inline]
    pub fn set_d_t<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.d_t.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn id(&self) -> &str {
        self.id.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use id()")]
    pub fn get_id(&self) -> &str {
        self.id()
    }

    #[inline]
    pub fn set_id<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.id.set_value(value);
        self
    }

    /// The text of the comment.
    #[inline]
    #[must_use]
    pub fn text(&self) -> &str {
        self.threaded_comment_text.value()
    }

    /// Sets the text; the mentions were of the old one and are dropped.
    #[inline]
    pub fn set_text<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.threaded_comment_text.set_value(value);
        self.mentions.clear();
        self
    }

    /// The author (`Person::id` of the workbook's persons).
    #[inline]
    #[must_use]
    pub fn person_id(&self) -> &str {
        &self.person_id
    }

    #[inline]
    pub fn set_person_id<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.person_id = value.into().into_boxed_str();
        self
    }

    /// The comment this one replies to (its `id`); empty for the first of a thread.
    #[inline]
    #[must_use]
    pub fn parent_id(&self) -> &str {
        &self.parent_id
    }

    #[inline]
    pub fn set_parent_id<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.parent_id = value.into().into_boxed_str();
        self
    }

    /// Whether the thread is resolved (on its first comment).
    #[inline]
    #[must_use]
    pub fn done(&self) -> bool {
        self.done
    }

    #[inline]
    pub fn set_done(&mut self, value: bool) -> &mut Self {
        self.done = value;
        self
    }

    #[inline]
    #[must_use]
    pub fn mentions(&self) -> &[Mention] {
        &self.mentions
    }

    #[inline]
    pub fn mentions_mut(&mut self) -> &mut Vec<Mention> {
        &mut self.mentions
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        let coordinate = get_attribute(e, b"ref").unwrap();
        self.coordinate_mut().set_coordinate(coordinate);

        set_string_from_xml!(self, e, d_t, "dT");
        set_string_from_xml!(self, e, id, "id");
        self.set_person_id(get_attribute(e, b"personId").unwrap_or_default());
        self.set_parent_id(get_attribute(e, b"parentId").unwrap_or_default());
        self.done = matches!(get_attribute(e, b"done").as_deref(), Some("1" | "true"));

        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                if e.name().into_inner() == b"text" {
                    self.threaded_comment_text.set_attributes(reader, e);
                }
            },
            Event::Empty(ref e) => {
                if e.name().into_inner() == b"mention" {
                    self.mentions.push(Mention {
                        person_id:   get_attribute(e, b"mentionpersonId").unwrap_or_default(),
                        id:          get_attribute(e, b"mentionId").unwrap_or_default(),
                        start_index: get_attribute(e, b"startIndex").and_then(|v| v.parse().ok()).unwrap_or(0),
                        length:      get_attribute(e, b"length").and_then(|v| v.parse().ok()).unwrap_or(0),
                    });
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"threadedComment" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "threadedComment")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // threadedComment
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        let coordinate = self.coordinate.to_string();
        attributes.push(("ref", &coordinate).into());

        if self.d_t.has_value() {
            attributes.push(("dT", self.d_t.value_str()).into());
        }
        let person_id = if self.person_id.is_empty() {
            "{00000000-0000-0000-0000-000000000000}"
        } else {
            &self.person_id
        };
        attributes.push(("personId", person_id).into());
        if self.id.has_value() {
            attributes.push(("id", self.id.value_str()).into());
        }
        if !self.parent_id.is_empty() {
            attributes.push(("parentId", &*self.parent_id).into());
        }
        if self.done {
            attributes.push(("done", "1").into());
        }
        write_start_tag(writer, "threadedComment", attributes, false);

        // text
        self.threaded_comment_text.write_to(writer);

        // mentions
        if !self.mentions.is_empty() {
            write_start_tag(writer, "mentions", vec![], false);
            for m in &self.mentions {
                let (start, length) = (m.start_index.to_string(), m.length.to_string());
                write_start_tag(
                    writer,
                    "mention",
                    vec![
                        ("mentionpersonId", m.person_id.as_str()).into(),
                        ("mentionId", m.id.as_str()).into(),
                        ("startIndex", start.as_str()).into(),
                        ("length", length.as_str()).into(),
                    ],
                    true,
                );
            }
            write_end_tag(writer, "mentions");
        }

        write_end_tag(writer, "threadedComment");
    }
}
impl AdjustmentCoordinate for ThreadedComment {
    #[inline]
    fn adjustment_insert_coordinate(
        &mut self,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        self.coordinate.adjustment_insert_coordinate(
            root_col_num,
            offset_col_num,
            root_row_num,
            offset_row_num,
        );
    }

    #[inline]
    fn adjustment_remove_coordinate(
        &mut self,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        self.coordinate.adjustment_remove_coordinate(
            root_col_num,
            offset_col_num,
            root_row_num,
            offset_row_num,
        );
    }

    #[inline]
    fn is_remove_coordinate(
        &self,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) -> bool {
        self.coordinate.is_remove_coordinate(
            root_col_num,
            offset_col_num,
            root_row_num,
            offset_row_num,
        )
    }
}
