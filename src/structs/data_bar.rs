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

/// A data bar (`dataBar`, and the Excel 2010 `x14:dataBar` with negative
/// values, borders and an axis).
#[derive(Clone, Default, Debug)]
pub struct DataBar {
    cfvo_collection:  Vec<ConditionalFormatValueObject>,
    color_collection: Vec<Color>,
    min_length:       UInt32Value,
    max_length:       UInt32Value,
    show_value:       BooleanValue,
    // x14
    gradient:         BooleanValue,
    border:           BooleanValue,
    direction:        StringValue,
    negative_bar_color_same_as_positive: BooleanValue,
    negative_bar_border_color_same_as_positive: BooleanValue,
    axis_position:    StringValue,
    fill_color:       Option<Color>,
    border_color:     Option<Color>,
    negative_fill_color: Option<Color>,
    negative_border_color: Option<Color>,
    axis_color:       Option<Color>,
}

macro_rules! color_accessors {
    ($get:ident, $set:ident, $field:ident) => {
        #[inline]
        #[must_use]
        pub fn $get(&self) -> Option<&Color> {
            self.$field.as_ref()
        }

        #[inline]
        pub fn $set(&mut self, value: Option<Color>) -> &mut Self {
            self.$field = value;
            self
        }
    };
}

impl DataBar {
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
    pub fn set_cfvo_collection(&mut self, value: Vec<ConditionalFormatValueObject>) -> &mut Self {
        self.cfvo_collection = value;
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

    /// The length of the shortest bar in percent of the cell (None: 10).
    #[inline]
    #[must_use]
    pub fn min_length(&self) -> Option<u32> {
        self.min_length.has_value().then(|| self.min_length.value())
    }

    #[inline]
    pub fn set_min_length(&mut self, value: u32) -> &mut Self {
        self.min_length.set_value(value);
        self
    }

    /// The length of the longest bar in percent of the cell (None: 90, or
    /// 100 for an Excel 2010 data bar).
    #[inline]
    #[must_use]
    pub fn max_length(&self) -> Option<u32> {
        self.max_length.has_value().then(|| self.max_length.value())
    }

    #[inline]
    pub fn set_max_length(&mut self, value: u32) -> &mut Self {
        self.max_length.set_value(value);
        self
    }

    /// The cell's value shows with the bar (true when not given).
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

    /// x14: the bar fades out (true when not given).
    #[inline]
    #[must_use]
    pub fn gradient(&self) -> bool {
        !self.gradient.has_value() || self.gradient.value()
    }

    #[inline]
    pub fn set_gradient(&mut self, value: bool) -> &mut Self {
        self.gradient.set_value(value);
        self
    }

    /// x14: the bar has a border (`border_color`).
    #[inline]
    #[must_use]
    pub fn border(&self) -> bool {
        self.border.value()
    }

    #[inline]
    pub fn set_border(&mut self, value: bool) -> &mut Self {
        self.border.set_value(value);
        self
    }

    /// x14: context, leftToRight or rightToLeft ("" when not given).
    #[inline]
    #[must_use]
    pub fn direction(&self) -> &str {
        self.direction.value_str()
    }

    #[inline]
    pub fn set_direction<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.direction.set_value(value);
        self
    }

    /// x14: negative values use the positive fill color (false when not given).
    #[inline]
    #[must_use]
    pub fn negative_bar_color_same_as_positive(&self) -> bool {
        self.negative_bar_color_same_as_positive.value()
    }

    #[inline]
    pub fn set_negative_bar_color_same_as_positive(&mut self, value: bool) -> &mut Self {
        self.negative_bar_color_same_as_positive.set_value(value);
        self
    }

    /// x14: negative values use the positive border color (true when not given).
    #[inline]
    #[must_use]
    pub fn negative_bar_border_color_same_as_positive(&self) -> bool {
        !self.negative_bar_border_color_same_as_positive.has_value()
            || self.negative_bar_border_color_same_as_positive.value()
    }

    #[inline]
    pub fn set_negative_bar_border_color_same_as_positive(&mut self, value: bool) -> &mut Self {
        self.negative_bar_border_color_same_as_positive.set_value(value);
        self
    }

    /// x14: automatic, middle or none ("" when not given: automatic).
    #[inline]
    #[must_use]
    pub fn axis_position(&self) -> &str {
        self.axis_position.value_str()
    }

    #[inline]
    pub fn set_axis_position<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.axis_position.set_value(value);
        self
    }

    color_accessors!(fill_color, set_fill_color, fill_color);
    color_accessors!(border_color, set_border_color, border_color);
    color_accessors!(negative_fill_color, set_negative_fill_color, negative_fill_color);
    color_accessors!(negative_border_color, set_negative_border_color, negative_border_color);
    color_accessors!(axis_color, set_axis_color, axis_color);

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        set_string_from_xml!(self, e, min_length, "minLength");
        set_string_from_xml!(self, e, max_length, "maxLength");
        set_string_from_xml!(self, e, show_value, "showValue");
        set_string_from_xml!(self, e, gradient, "gradient");
        set_string_from_xml!(self, e, border, "border");
        set_string_from_xml!(self, e, direction, "direction");
        set_string_from_xml!(
            self,
            e,
            negative_bar_color_same_as_positive,
            "negativeBarColorSameAsPositive"
        );
        set_string_from_xml!(
            self,
            e,
            negative_bar_border_color_same_as_positive,
            "negativeBarBorderColorSameAsPositive"
        );
        set_string_from_xml!(self, e, axis_position, "axisPosition");

        xml_read_loop!(
            reader,
            ref n @ (Event::Empty(ref e) | Event::Start(ref e)) => {
                let is_empty = matches!(n, Event::Empty(_));
                let color = |reader: &mut Reader<R>| {
                    let mut obj = Color::default();
                    obj.set_attributes(reader, e, is_empty);
                    obj
                };
                match e.local_name().into_inner() {
                    b"cfvo" => {
                        let mut obj = ConditionalFormatValueObject::default();
                        obj.set_attributes(reader, e, is_empty);
                        self.cfvo_collection.push(obj);
                    }
                    b"color" => {
                        let obj = color(reader);
                        self.color_collection.push(obj);
                    }
                    b"fillColor" => self.fill_color = Some(color(reader)),
                    b"borderColor" => self.border_color = Some(color(reader)),
                    b"negativeFillColor" => self.negative_fill_color = Some(color(reader)),
                    b"negativeBorderColor" => self.negative_border_color = Some(color(reader)),
                    b"axisColor" => self.axis_color = Some(color(reader)),
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.local_name().into_inner() == b"dataBar" {
                    return
                }
            },
            Event::Eof => return
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // dataBar
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        let min_length = self.min_length.value_string();
        if self.min_length.has_value() {
            attributes.push(("minLength", &min_length).into());
        }
        let max_length = self.max_length.value_string();
        if self.max_length.has_value() {
            attributes.push(("maxLength", &max_length).into());
        }
        if self.show_value.has_value() {
            attributes.push(("showValue", self.show_value.value_string()).into());
        }
        write_start_tag(writer, "dataBar", attributes, false);

        // cfvo
        for v in &self.cfvo_collection {
            v.write_to(writer);
        }

        // color
        for v in &self.color_collection {
            v.write_to_color(writer);
        }

        write_end_tag(writer, "dataBar");
    }

    pub(crate) fn write_to_x14(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // x14:dataBar
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        let min_length = self.min_length.value_string();
        if self.min_length.has_value() {
            attributes.push(("minLength", &min_length).into());
        }
        let max_length = self.max_length.value_string();
        if self.max_length.has_value() {
            attributes.push(("maxLength", &max_length).into());
        }
        let bools = [
            ("showValue", &self.show_value),
            ("border", &self.border),
            ("gradient", &self.gradient),
        ];
        for (name, v) in bools {
            if v.has_value() {
                attributes.push((name, v.value_string()).into());
            }
        }
        if self.direction.has_value() {
            attributes.push(("direction", self.direction.value_str()).into());
        }
        let bools = [
            (
                "negativeBarColorSameAsPositive",
                &self.negative_bar_color_same_as_positive,
            ),
            (
                "negativeBarBorderColorSameAsPositive",
                &self.negative_bar_border_color_same_as_positive,
            ),
        ];
        for (name, v) in bools {
            if v.has_value() {
                attributes.push((name, v.value_string()).into());
            }
        }
        if self.axis_position.has_value() {
            attributes.push(("axisPosition", self.axis_position.value_str()).into());
        }
        write_start_tag(writer, "x14:dataBar", attributes, false);

        for v in &self.cfvo_collection {
            v.write_to_x14(writer);
        }
        let colors = [
            ("x14:fillColor", &self.fill_color),
            ("x14:borderColor", &self.border_color),
            ("x14:negativeFillColor", &self.negative_fill_color),
            ("x14:negativeBorderColor", &self.negative_border_color),
            ("x14:axisColor", &self.axis_color),
        ];
        for (name, c) in colors {
            if let Some(c) = c {
                c.write_to(writer, name);
            }
        }

        write_end_tag(writer, "x14:dataBar");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_data_bar(xml: &str) -> DataBar {
        let mut reader = Reader::from_reader(std::io::BufReader::new(xml.as_bytes()));
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e) | Event::Empty(ref e))
                    if e.name().into_inner() == b"dataBar" =>
                {
                    let mut obj = DataBar::default();
                    obj.set_attributes(&mut reader, e);
                    return obj;
                }
                Ok(Event::Eof) => panic!("dataBar element not found"),
                _ => (),
            }
            buf.clear();
        }
    }

    #[test]
    fn read_child_elements_with_end_tags() {
        // Writers such as openpyxl emit <cfvo .../> children as
        // <cfvo ...></cfvo> (Start + End events instead of Empty).
        let obj = read_data_bar(
            r#"<dataBar showValue="1" minLength="10" maxLength="90"><cfvo type="num" val="0"></cfvo><cfvo type="num" val="1400"></cfvo><color rgb="FF1E2761"></color></dataBar>"#,
        );
        assert_eq!(obj.cfvo_collection().len(), 2);
        assert_eq!(obj.color_collection().len(), 1);
        assert_eq!(obj.color_collection()[0].argb_str(), "FF1E2761");
    }

    #[test]
    fn read_child_elements_self_closing() {
        let obj = read_data_bar(
            r#"<dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar>"#,
        );
        assert_eq!(obj.cfvo_collection().len(), 2);
        assert_eq!(obj.color_collection().len(), 1);
        assert_eq!(obj.color_collection()[0].argb_str(), "FF638EC6");
    }
}
