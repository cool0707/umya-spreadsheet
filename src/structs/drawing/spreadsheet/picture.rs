// xdr:pic
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
    BlipFill,
    NonVisualPictureProperties,
    ShapeProperties,
};
use crate::{
    reader::driver::xml_read_loop,
    structs::raw::RawRelationships,
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Clone, Default, Debug)]
pub struct Picture {
    non_visual_picture_properties: NonVisualPictureProperties,
    blip_fill:                     BlipFill,
    shape_properties:              ShapeProperties,
    r#macro:                       Option<Box<str>>,
}

impl Picture {
    #[inline]
    #[must_use]
    pub fn non_visual_picture_properties(&self) -> &NonVisualPictureProperties {
        &self.non_visual_picture_properties
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use non_visual_picture_properties()")]
    pub fn get_non_visual_picture_properties(&self) -> &NonVisualPictureProperties {
        self.non_visual_picture_properties()
    }

    #[inline]
    pub fn non_visual_picture_properties_mut(&mut self) -> &mut NonVisualPictureProperties {
        &mut self.non_visual_picture_properties
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use non_visual_picture_properties_mut()")]
    pub fn get_non_visual_picture_properties_mut(&mut self) -> &mut NonVisualPictureProperties {
        self.non_visual_picture_properties_mut()
    }

    #[inline]
    pub fn set_non_visual_picture_properties(&mut self, value: NonVisualPictureProperties) {
        self.non_visual_picture_properties = value;
    }

    #[inline]
    #[must_use]
    pub fn blip_fill(&self) -> &BlipFill {
        &self.blip_fill
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use blip_fill()")]
    pub fn get_blip_fill(&self) -> &BlipFill {
        self.blip_fill()
    }

    #[inline]
    pub fn blip_fill_mut(&mut self) -> &mut BlipFill {
        &mut self.blip_fill
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use blip_fill_mut()")]
    pub fn get_blip_fill_mut(&mut self) -> &mut BlipFill {
        self.blip_fill_mut()
    }

    #[inline]
    pub fn set_blip_fill(&mut self, value: BlipFill) {
        self.blip_fill = value;
    }

    #[inline]
    #[must_use]
    pub fn shape_properties(&self) -> &ShapeProperties {
        &self.shape_properties
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use shape_properties()")]
    pub fn get_shape_properties(&self) -> &ShapeProperties {
        self.shape_properties()
    }

    #[inline]
    pub fn shape_properties_mut(&mut self) -> &mut ShapeProperties {
        &mut self.shape_properties
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use shape_properties_mut()")]
    pub fn get_shape_properties_mut(&mut self) -> &mut ShapeProperties {
        self.shape_properties_mut()
    }

    #[inline]
    pub fn set_shape_properties(&mut self, value: ShapeProperties) {
        self.shape_properties = value;
    }

    /// The macro run when the object is clicked (`macro`).
    #[inline]
    #[must_use]
    pub fn get_macro(&self) -> &str {
        self.r#macro.as_deref().unwrap_or("")
    }

    #[inline]
    pub fn set_macro<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.r#macro = Some(value.into().into_boxed_str());
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        drawing_relationships: Option<&RawRelationships>,
    ) {
        // As written (escaped): the writer escapes it again.
        self.r#macro = crate::reader::driver::get_attribute(e, b"macro").map(|v| {
            quick_xml::escape::unescape(&v).map_or_else(|_| v.clone(), std::borrow::Cow::into_owned).into_boxed_str()
        });
        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                match e.name().into_inner() {
                    b"xdr:nvPicPr" | b"nvPicPr" => {
                        self.non_visual_picture_properties.set_attributes(reader, e);
                    }
                    b"xdr:blipFill" | b"blipFill" => {
                        self.blip_fill
                            .set_attributes(reader, e, drawing_relationships);
                        }
                    b"xdr:spPr" | b"spPr" => {
                        self.shape_properties.set_attributes(reader, e, drawing_relationships);
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if matches!(e.name().into_inner(), b"xdr:pic" | b"pic") {
                    return;
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "pic")
        );
    }

    pub(crate) fn write_to(
        &self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        rel_list: &mut Vec<(String, String)>,
    ) {
        // xdr:pic
        let attributes: crate::structs::AttrCollection = match &self.r#macro {
            Some(v) => vec![("macro", &**v).into()],
            None => vec![],
        };
        write_start_tag(writer, "xdr:pic", attributes, false);

        // xdr:nvPicPr
        self.non_visual_picture_properties.write_to(writer);

        // xdr:blipFill
        self.blip_fill.write_to(writer, rel_list);

        // xdr:spPr
        self.shape_properties.write_to(writer, rel_list);

        write_end_tag(writer, "xdr:pic");
    }
}
