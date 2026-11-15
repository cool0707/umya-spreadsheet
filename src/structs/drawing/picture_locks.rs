// a:picLocks
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::BytesStart,
};

use crate::{
    reader::driver::get_attribute_value,
    writer::driver::write_start_tag,
};

#[derive(Clone, Default, Debug)]
pub struct PictureLocks {
    no_change_aspect: bool,
    /// The other locks (`noGrp`, `noCrop`...), as read.
    other_attributes: Vec<(Box<str>, Box<str>)>,
}

impl PictureLocks {
    #[inline]
    #[must_use]
    pub fn no_change_aspect(&self) -> bool {
        self.no_change_aspect
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use no_change_aspect()")]
    pub fn get_no_change_aspect(&self) -> bool {
        self.no_change_aspect()
    }

    #[inline]
    pub fn set_no_change_aspect(&mut self, value: bool) -> &mut Self {
        self.no_change_aspect = value;
        self
    }

    #[inline]
    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        _reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        for attr in e.attributes().with_checks(false).flatten() {
            let value = get_attribute_value(&attr).unwrap();
            match attr.key.into_inner() {
                b"noChangeAspect" => self.no_change_aspect = value == "1" || value == "true",
                key => self.other_attributes.push((String::from_utf8_lossy(key).into(), value.into())),
            }
        }
    }

    #[inline]
    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // a:picLocks (a boolean: "2" does not open in Excel)
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.no_change_aspect {
            attributes.push(("noChangeAspect", "1").into());
        }
        for (k, v) in &self.other_attributes {
            attributes.push((&**k, &**v).into());
        }
        write_start_tag(writer, "a:picLocks", attributes, true);
    }
}
