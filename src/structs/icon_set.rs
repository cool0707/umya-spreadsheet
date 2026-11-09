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
    Color,
    ConditionalFormatValueObject,
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

/// An icon set (`iconSet`, and the Excel 2010 `x14:iconSet` with more sets
/// and icons chosen one by one).
#[derive(Clone, Default, Debug)]
pub struct IconSet {
    cfvo_collection:  Vec<ConditionalFormatValueObject>,
    color_collection: Vec<Color>,
    icon_set:         StringValue,
    show_value:       BooleanValue,
    percent:          BooleanValue,
    reverse:          BooleanValue,
    custom:           BooleanValue,
    cf_icon_collection: Vec<CfIcon>,
}

/// An icon of a custom icon set (`x14:cfIcon`): the set and the icon's index
/// in it (from the lowest).
#[derive(Clone, Default, Debug)]
pub struct CfIcon {
    icon_set: StringValue,
    icon_id:  UInt32Value,
}

impl CfIcon {
    /// The icon set, such as `3TrafficLights1`, or `NoIcons`.
    #[inline]
    #[must_use]
    pub fn icon_set(&self) -> &str {
        self.icon_set.value_str()
    }

    #[inline]
    pub fn set_icon_set<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.icon_set.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn icon_id(&self) -> u32 {
        self.icon_id.value()
    }

    #[inline]
    pub fn set_icon_id(&mut self, value: u32) -> &mut Self {
        self.icon_id.set_value(value);
        self
    }

    fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let id = self.icon_id.value_string();
        write_start_tag(
            writer,
            "x14:cfIcon",
            vec![("iconSet", self.icon_set.value_str()).into(), ("iconId", &id).into()],
            true,
        );
    }
}

impl IconSet {
    #[inline]
    #[must_use]
    pub fn cfvo_collection(&self) -> &[ConditionalFormatValueObject] {
        &self.cfvo_collection
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use cfvo_collection()")]
    pub fn get_cfvo_collection(&self) -> &[ConditionalFormatValueObject] {
        self.cfvo_collection()
    }

    #[inline]
    pub fn set_cfvo_collection(
        &mut self,
        value: impl Into<Vec<ConditionalFormatValueObject>>,
    ) -> &mut Self {
        self.cfvo_collection = value.into();
        self
    }

    #[inline]
    pub fn add_cfvo_collection(&mut self, value: ConditionalFormatValueObject) -> &mut Self {
        self.cfvo_collection.push(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn color_collection(&self) -> &[Color] {
        &self.color_collection
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use color_collection()")]
    pub fn get_color_collection(&self) -> &[Color] {
        self.color_collection()
    }

    #[inline]
    pub fn set_color_collection(&mut self, value: impl Into<Vec<Color>>) -> &mut Self {
        self.color_collection = value.into();
        self
    }

    #[inline]
    pub fn add_color_collection(&mut self, value: Color) -> &mut Self {
        self.color_collection.push(value);
        self
    }

    /// The kind of icons, such as `3TrafficLights1` or `5Arrows` (when not given:
    /// `3TrafficLights1`).
    #[inline]
    #[must_use]
    pub fn icon_set(&self) -> &str {
        if self.icon_set.has_value() {
            self.icon_set.value_str()
        } else {
            "3TrafficLights1"
        }
    }

    #[inline]
    pub fn set_icon_set<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.icon_set.set_value(value);
        self
    }

    /// The cell's value shows with the icon (true when not given).
    #[inline]
    #[must_use]
    pub fn show_value(&self) -> bool {
        !self.show_value.has_value() || self.show_value.value()
    }

    #[inline]
    pub fn set_show_value(&mut self, value: bool) -> &mut Self {
        self.show_value.set_value(value);
        self
    }

    /// The thresholds are percents (true when not given).
    #[inline]
    #[must_use]
    pub fn percent(&self) -> bool {
        !self.percent.has_value() || self.percent.value()
    }

    #[inline]
    pub fn set_percent(&mut self, value: bool) -> &mut Self {
        self.percent.set_value(value);
        self
    }

    /// The icons go from high to low.
    #[inline]
    #[must_use]
    pub fn reverse(&self) -> bool {
        self.reverse.value()
    }

    #[inline]
    pub fn set_reverse(&mut self, value: bool) -> &mut Self {
        self.reverse.set_value(value);
        self
    }

    /// x14: the icons are chosen one by one (`cf_icon_collection`).
    #[inline]
    #[must_use]
    pub fn custom(&self) -> bool {
        self.custom.value()
    }

    #[inline]
    pub fn set_custom(&mut self, value: bool) -> &mut Self {
        self.custom.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn cf_icon_collection(&self) -> &[CfIcon] {
        &self.cf_icon_collection
    }

    #[inline]
    pub fn set_cf_icon_collection(&mut self, value: impl Into<Vec<CfIcon>>) -> &mut Self {
        self.cf_icon_collection = value.into();
        self
    }

    #[inline]
    pub fn add_cf_icon_collection(&mut self, value: CfIcon) -> &mut Self {
        self.cf_icon_collection.push(value);
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        set_string_from_xml!(self, e, icon_set, "iconSet");
        set_string_from_xml!(self, e, show_value, "showValue");
        set_string_from_xml!(self, e, percent, "percent");
        set_string_from_xml!(self, e, reverse, "reverse");
        set_string_from_xml!(self, e, custom, "custom");

        xml_read_loop!(
            reader,
            ref n @ (Event::Empty(ref e) | Event::Start(ref e)) => {
                let is_empty = matches!(n, Event::Empty(_));
                match e.local_name().into_inner() {
                    b"cfvo" => {
                        let mut obj = ConditionalFormatValueObject::default();
                        obj.set_attributes(reader, e, is_empty);
                        self.cfvo_collection.push(obj);
                    }
                    b"color" => {
                        let mut obj = Color::default();
                        obj.set_attributes(reader, e, is_empty);
                        self.color_collection.push(obj);
                    }
                    b"cfIcon" => {
                        let mut obj = CfIcon::default();
                        set_string_from_xml!(obj, e, icon_set, "iconSet");
                        set_string_from_xml!(obj, e, icon_id, "iconId");
                        self.cf_icon_collection.push(obj);
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.local_name().into_inner() == b"iconSet" {
                    return
                }
            },
            Event::Eof => return
        );
    }

    fn attributes(&self, x14: bool) -> crate::structs::AttrCollection<'_> {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.icon_set.has_value() {
            attributes.push(("iconSet", self.icon_set.value_str()).into());
        }
        let mut bools = vec![
            ("showValue", &self.show_value),
            ("percent", &self.percent),
            ("reverse", &self.reverse),
        ];
        if x14 {
            bools.push(("custom", &self.custom));
        }
        for (name, v) in bools {
            if v.has_value() {
                attributes.push((name, v.value_string()).into());
            }
        }
        attributes
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // iconSet
        write_start_tag(writer, "iconSet", self.attributes(false), false);

        // cfvo
        for v in &self.cfvo_collection {
            v.write_to(writer);
        }

        write_end_tag(writer, "iconSet");
    }

    pub(crate) fn write_to_x14(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // x14:iconSet
        write_start_tag(writer, "x14:iconSet", self.attributes(true), false);

        for v in &self.cfvo_collection {
            v.write_to_x14(writer);
        }
        for v in &self.cf_icon_collection {
            v.write_to(writer);
        }

        write_end_tag(writer, "x14:iconSet");
    }
}
