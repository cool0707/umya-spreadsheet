// xdr:nvSpPr
use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesStart,
        Event,
    },
};

use super::NonVisualDrawingProperties;
use crate::{
    reader::driver::{
        get_attribute_value,
        xml_read_loop,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Clone, Default, Debug)]
pub struct NonVisualShapeProperties {
    non_visual_drawing_properties: NonVisualDrawingProperties,
    /// The attributes of `xdr:cNvSpPr` (`txBox="1"` for a text box), as read.
    shape_attributes:              Vec<(Box<str>, Box<str>)>,
    /// The attributes of its `a:spLocks`, as read.
    shape_locks:                   Option<Vec<(Box<str>, Box<str>)>>,
}

fn attributes_of(e: &BytesStart) -> Vec<(Box<str>, Box<str>)> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .map(|a| (String::from_utf8_lossy(a.key.into_inner()).into(), get_attribute_value(&a).unwrap().into()))
        .collect()
}

impl NonVisualShapeProperties {
    #[inline]
    #[must_use]
    pub fn non_visual_drawing_properties(&self) -> &NonVisualDrawingProperties {
        &self.non_visual_drawing_properties
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use non_visual_drawing_properties()")]
    pub fn get_non_visual_drawing_properties(&self) -> &NonVisualDrawingProperties {
        self.non_visual_drawing_properties()
    }

    #[inline]
    pub fn non_visual_drawing_properties_mut(&mut self) -> &mut NonVisualDrawingProperties {
        &mut self.non_visual_drawing_properties
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use non_visual_drawing_properties_mut()")]
    pub fn get_non_visual_drawing_properties_mut(&mut self) -> &mut NonVisualDrawingProperties {
        self.non_visual_drawing_properties_mut()
    }

    #[inline]
    pub fn set_non_visual_drawing_properties(&mut self, value: NonVisualDrawingProperties) {
        self.non_visual_drawing_properties = value;
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        _e: &BytesStart,
    ) {
        xml_read_loop!(
            reader,
            Event::Empty(ref e) => {
                match e.name().into_inner() {
                    b"xdr:cNvPr" => {
                        self.non_visual_drawing_properties
                            .set_attributes(reader, e, true);
                    }
                    b"xdr:cNvSpPr" => self.shape_attributes = attributes_of(e),
                    b"a:spLocks" => self.shape_locks = Some(attributes_of(e)),
                    _ => (),
                }
            },
            Event::Start(ref e) => {
                match e.name().into_inner() {
                    b"xdr:cNvPr" => {
                        self.non_visual_drawing_properties
                            .set_attributes(reader, e, false);
                    }
                    b"xdr:cNvSpPr" => self.shape_attributes = attributes_of(e),
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"xdr:nvSpPr" {
                    return;
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "xdr:nvSpPr")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>, ole_id: usize) {
        // xdr:nvSpPr
        write_start_tag(writer, "xdr:nvSpPr", vec![], false);

        // xdr:cNvPr
        self.non_visual_drawing_properties.write_to(writer, ole_id);

        // xdr:cNvSpPr
        let attrs = self.shape_attributes.iter().map(|(k, v)| (&**k, &**v).into()).collect();
        if let Some(locks) = &self.shape_locks {
            write_start_tag(writer, "xdr:cNvSpPr", attrs, false);
            let locks = locks.iter().map(|(k, v)| (&**k, &**v).into()).collect();
            write_start_tag(writer, "a:spLocks", locks, true);
            write_end_tag(writer, "xdr:cNvSpPr");
        } else {
            write_start_tag(writer, "xdr:cNvSpPr", attrs, true);
        }

        write_end_tag(writer, "xdr:nvSpPr");
    }
}
