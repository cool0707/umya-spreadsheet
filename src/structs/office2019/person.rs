// person
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::Event,
};

use crate::{
    reader::driver::get_attribute,
    writer::driver::write_start_tag,
};

/// An author of threaded comments (`xl/persons/person.xml`).
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Person {
    pub display_name: String,
    /// What the threaded comments refer to the person by (`personId`).
    pub id:           String,
    pub user_id:      String,
    /// "AD", "Windows Live", "PeoplePicker" or "None" (a user of the computer).
    pub provider_id:  String,
}

fn attribute(e: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> String {
    let v = get_attribute(e, key).unwrap_or_default();
    quick_xml::escape::unescape(&v).map_or_else(|_| v.clone(), |s| s.into_owned())
}

impl Person {
    /// The persons of a personList part.
    pub(crate) fn read_list(data: &[u8]) -> Vec<Person> {
        let mut reader = Reader::from_reader(data);
        let mut list = Vec::new();
        let mut buf = Vec::new();
        loop {
            // A part that cannot be read leaves the persons read so far.
            match reader.read_event_into(&mut buf) {
                Ok(Event::Empty(ref e) | Event::Start(ref e)) if e.name().into_inner() == b"person" => {
                    list.push(Person {
                        display_name: attribute(e, b"displayName"),
                        id:           attribute(e, b"id"),
                        user_id:      attribute(e, b"userId"),
                        provider_id:  attribute(e, b"providerId"),
                    });
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
            buf.clear();
        }
        list
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let mut attributes: crate::structs::AttrCollection = vec![
            ("displayName", self.display_name.as_str()).into(),
            ("id", self.id.as_str()).into(),
        ];
        if !self.user_id.is_empty() {
            attributes.push(("userId", self.user_id.as_str()).into());
        }
        if !self.provider_id.is_empty() {
            attributes.push(("providerId", self.provider_id.as_str()).into());
        }
        write_start_tag(writer, "person", attributes, true);
    }
}
