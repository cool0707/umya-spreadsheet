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
    ColorScale,
    ConditionalFormatValues,
    ConditionalFormattingOperatorValues,
    DataBar,
    DifferentialFormat,
    DifferentialFormats,
    EnumValue,
    Formula,
    IconSet,
    Int32Value,
    StringValue,
    Style,
    TimePeriodValues,
    UInt32Value,
};
use crate::{
    reader::driver::{
        get_attribute,
        set_string_from_xml,
        xml_read_loop,
    },
    helper::const_str::SHEET_MS_MAIN_NS,
    writer::driver::{
        write_end_tag,
        write_start_tag,
        write_text_node,
    },
};

#[derive(Clone, Default, Debug)]
pub struct ConditionalFormattingRule {
    r#type:        EnumValue<ConditionalFormatValues>,
    operator:      EnumValue<ConditionalFormattingOperatorValues>,
    text:          StringValue,
    priority:      Int32Value,
    percent:       BooleanValue,
    bottom:        BooleanValue,
    rank:          UInt32Value,
    stop_if_true:  BooleanValue,
    std_dev:       Int32Value,
    above_average: BooleanValue,
    equal_average: BooleanValue,
    time_period:   EnumValue<TimePeriodValues>,
    style:         Option<Box<Style>>,
    color_scale:   Option<ColorScale>,
    data_bar:      Option<DataBar>,
    icon_set:      Option<IconSet>,
    formulas:      Vec<Formula>,
    /// x14: the id of an Excel 2010 rule.
    id:            StringValue,
    /// The id of the Excel 2010 rule that carries more of this rule
    /// (`<extLst><ext><x14:id>`), for a data bar.
    ext_id:        StringValue,
}

impl ConditionalFormattingRule {
    #[inline]
    #[must_use]
    pub fn get_type(&self) -> &ConditionalFormatValues {
        self.r#type.value()
    }

    #[inline]
    pub fn set_type(&mut self, value: ConditionalFormatValues) -> &mut Self {
        self.r#type.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn operator(&self) -> &ConditionalFormattingOperatorValues {
        self.operator.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use operator()")]
    pub fn get_operator(&self) -> &ConditionalFormattingOperatorValues {
        self.operator()
    }

    #[inline]
    pub fn set_operator(&mut self, value: ConditionalFormattingOperatorValues) -> &mut Self {
        self.operator.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn text(&self) -> &str {
        self.text.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use text()")]
    pub fn get_text(&self) -> &str {
        self.text()
    }

    #[inline]
    pub fn set_text<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.text.set_value(value.into());
        self
    }

    #[inline]
    #[must_use]
    pub fn priority(&self) -> i32 {
        self.priority.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use priority()")]
    pub fn get_priority(&self) -> i32 {
        self.priority()
    }

    #[inline]
    pub fn set_priority(&mut self, value: i32) -> &mut Self {
        self.priority.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn percent(&self) -> bool {
        self.percent.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use percent()")]
    pub fn get_percent(&self) -> bool {
        self.percent()
    }

    #[inline]
    pub fn set_percent(&mut self, value: bool) -> &mut Self {
        self.percent.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn bottom(&self) -> bool {
        self.bottom.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use bottom()")]
    pub fn get_bottom(&self) -> bool {
        self.bottom()
    }

    #[inline]
    pub fn set_bottom(&mut self, value: bool) -> &mut Self {
        self.bottom.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn rank(&self) -> u32 {
        self.rank.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use rank()")]
    pub fn get_rank(&self) -> u32 {
        self.rank()
    }

    #[inline]
    pub fn set_rank(&mut self, value: u32) -> &mut Self {
        self.rank.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn stop_if_true(&self) -> bool {
        self.stop_if_true.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use stop_if_true()")]
    pub fn get_stop_if_true(&self) -> bool {
        self.stop_if_true()
    }

    #[inline]
    pub fn set_stop_if_true(&mut self, value: bool) -> &mut Self {
        self.stop_if_true.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn std_dev(&self) -> i32 {
        self.std_dev.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use std_dev()")]
    pub fn get_std_dev(&self) -> i32 {
        self.std_dev()
    }

    #[inline]
    pub fn set_std_dev(&mut self, value: i32) -> &mut Self {
        self.std_dev.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn above_average(&self) -> bool {
        // true when not given.
        !self.above_average.has_value() || self.above_average.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use above_average()")]
    pub fn get_above_average(&self) -> bool {
        self.above_average()
    }

    #[inline]
    pub fn set_above_average(&mut self, value: bool) -> &mut Self {
        self.above_average.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn equal_average(&self) -> bool {
        self.equal_average.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use equal_average()")]
    pub fn get_equal_average(&self) -> bool {
        self.equal_average()
    }

    #[inline]
    pub fn set_equal_average(&mut self, value: bool) -> &mut Self {
        self.equal_average.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn time_period(&self) -> &TimePeriodValues {
        self.time_period.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use time_period()")]
    pub fn get_time_period(&self) -> &TimePeriodValues {
        self.time_period()
    }

    #[inline]
    pub fn set_time_period(&mut self, value: TimePeriodValues) -> &mut Self {
        self.time_period.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn style(&self) -> Option<&Style> {
        self.style.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use style()")]
    pub fn get_style(&self) -> Option<&Style> {
        self.style()
    }

    #[inline]
    pub fn set_style(&mut self, value: Style) -> &mut Self {
        self.style = Some(Box::new(value));
        self
    }

    #[inline]
    pub fn remove_style(&mut self) -> &mut Self {
        self.style = None;
        self
    }

    #[inline]
    #[must_use]
    pub fn color_scale(&self) -> Option<&ColorScale> {
        self.color_scale.as_ref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use color_scale()")]
    pub fn get_color_scale(&self) -> Option<&ColorScale> {
        self.color_scale()
    }

    #[inline]
    pub fn set_color_scale(&mut self, value: ColorScale) -> &mut Self {
        self.color_scale = Some(value);
        self
    }

    #[inline]
    pub fn remove_color_scale(&mut self) -> &mut Self {
        self.color_scale = None;
        self
    }

    #[inline]
    #[must_use]
    pub fn data_bar(&self) -> Option<&DataBar> {
        self.data_bar.as_ref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use data_bar()")]
    pub fn get_data_bar(&self) -> Option<&DataBar> {
        self.data_bar()
    }

    #[inline]
    pub fn set_data_bar(&mut self, value: DataBar) -> &mut Self {
        self.data_bar = Some(value);
        self
    }

    #[inline]
    pub fn remove_data_bar(&mut self) -> &mut Self {
        self.data_bar = None;
        self
    }

    #[inline]
    #[must_use]
    pub fn icon_set(&self) -> Option<&IconSet> {
        self.icon_set.as_ref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use icon_set()")]
    pub fn get_icon_set(&self) -> Option<&IconSet> {
        self.icon_set()
    }

    #[inline]
    pub fn set_icon_set(&mut self, value: IconSet) -> &mut Self {
        self.icon_set = Some(value);
        self
    }

    #[inline]
    pub fn remove_icon_set(&mut self) -> &mut Self {
        self.icon_set = None;
        self
    }

    /// The first formula.
    #[inline]
    #[must_use]
    pub fn formula(&self) -> Option<&Formula> {
        self.formulas.first()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use formula()")]
    pub fn get_formula(&self) -> Option<&Formula> {
        self.formula()
    }

    /// Sets the only formula.
    #[inline]
    pub fn set_formula(&mut self, value: Formula) -> &mut Self {
        self.formulas = vec![value];
        self
    }

    #[inline]
    pub fn remove_formula(&mut self) -> &mut Self {
        self.formulas.clear();
        self
    }

    /// The formulas (two for between and notBetween).
    #[inline]
    #[must_use]
    pub fn formula_collection(&self) -> &[Formula] {
        &self.formulas
    }

    #[inline]
    pub fn add_formula_collection(&mut self, value: Formula) -> &mut Self {
        self.formulas.push(value);
        self
    }

    /// x14: the id of an Excel 2010 rule ("" for others).
    #[inline]
    #[must_use]
    pub fn id(&self) -> &str {
        self.id.value_str()
    }

    #[inline]
    pub fn set_id<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.id.set_value(value);
        self
    }

    /// The id of the Excel 2010 rule (in the worksheet's
    /// `conditional_formatting_2010_collection`) that carries more of this
    /// rule ("" for none): the data bars Excel 2010 and later write.
    #[inline]
    #[must_use]
    pub fn ext_id(&self) -> &str {
        self.ext_id.value_str()
    }

    #[inline]
    pub fn set_ext_id<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.ext_id.set_value(value);
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        differential_formats: &DifferentialFormats,
        empty_flag: bool,
    ) {
        set_string_from_xml!(self, e, r#type, "type");
        set_string_from_xml!(self, e, operator, "operator");
        if let Some(v) = get_attribute(e, b"text") {
            // The attribute as written (&#12406; ...): unescaped, since the
            // writer escapes it again.
            let v = quick_xml::escape::unescape(&v).map_or(v.clone(), |u| u.to_string());
            self.text.set_value(v);
        }

        if let Some(v) = get_attribute(e, b"dxfId") {
            // A dxfId past the stylesheet's differential formats is ignored.
            if let Some(style) = v
                .parse::<usize>()
                .ok()
                .and_then(|dxf_id| differential_formats.find_style(dxf_id))
            {
                self.set_style(style);
            }
        }

        set_string_from_xml!(self, e, priority, "priority");
        set_string_from_xml!(self, e, percent, "percent");
        set_string_from_xml!(self, e, bottom, "bottom");
        set_string_from_xml!(self, e, rank, "rank");
        set_string_from_xml!(self, e, stop_if_true, "stopIfTrue");
        set_string_from_xml!(self, e, std_dev, "stdDev");
        set_string_from_xml!(self, e, time_period, "timePeriod");
        set_string_from_xml!(self, e, above_average, "aboveAverage");
        set_string_from_xml!(self, e, equal_average, "equalAverage");
        set_string_from_xml!(self, e, id, "id");

        if empty_flag {
            return;
        }

        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                // Also the Excel 2010 rules: x14:dataBar, xm:f, x14:dxf, ...
                match e.local_name().into_inner() {
                    b"colorScale" => {
                        let mut obj = ColorScale::default();
                        obj.set_attributes(reader, e);
                        self.color_scale = Some(obj);
                    }
                    b"dataBar" => {
                        let mut obj = DataBar::default();
                        obj.set_attributes(reader, e);
                        self.data_bar = Some(obj);
                    }
                    b"iconSet" => {
                        let mut obj = IconSet::default();
                        obj.set_attributes(reader, e);
                        self.icon_set = Some(obj);
                    }
                    b"formula" | b"f" => {
                        let mut obj = Formula::default();
                        obj.set_attributes(reader, e);
                        self.formulas.push(obj);
                    }
                    b"dxf" => {
                        let mut obj = DifferentialFormat::default();
                        obj.set_attributes(reader, e);
                        self.set_style(obj.style());
                    }
                    b"id" => {
                        // <extLst><ext><x14:id>{...}</x14:id></ext></extLst>
                        let mut buf = Vec::new();
                        if let Ok(text) = reader.read_text_into(e.name(), &mut buf) {
                            self.ext_id.set_value(crate::helper::utils::unescape_xml_text(&text).trim());
                        }
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.local_name().into_inner() == b"cfRule" {
                    return
                }
            },
            Event::Eof => return
        );
    }

    fn attributes<'a>(&'a self, dxf_id: Option<&'a str>, x14: bool) -> crate::structs::AttrCollection<'a> {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.r#type.has_value() {
            attributes.push(("type", self.r#type.value_string()).into());
        }
        if self.operator.has_value() {
            attributes.push(("operator", self.operator.value_string()).into());
        }
        if let Some(v) = dxf_id {
            attributes.push(("dxfId", v).into());
        }
        if self.priority.has_value() {
            attributes.push(("priority", self.priority.value_string()).into());
        }
        if self.percent.has_value() {
            attributes.push(("percent", self.percent.value_string()).into());
        }
        if self.bottom.has_value() {
            attributes.push(("bottom", self.bottom.value_string()).into());
        }
        if self.rank.has_value() {
            attributes.push(("rank", self.rank.value_string()).into());
        }
        if self.stop_if_true.has_value() {
            attributes.push(("stopIfTrue", self.stop_if_true.value_string()).into());
        }
        if self.std_dev.has_value() {
            attributes.push(("stdDev", self.std_dev.value_string()).into());
        }
        if self.time_period.has_value() {
            attributes.push(("timePeriod", self.time_period.value_string()).into());
        }
        if self.above_average.has_value() {
            attributes.push(("aboveAverage", self.above_average.value_string()).into());
        }
        if self.equal_average.has_value() {
            attributes.push(("equalAverage", self.equal_average.value_string()).into());
        }
        if self.text.has_value() {
            attributes.push(("text", self.text.value_str()).into());
        }
        if x14 && self.id.has_value() {
            attributes.push(("id", self.id.value_str()).into());
        }
        attributes
    }

    pub(crate) fn write_to(
        &self,
        writer: &mut Writer<Cursor<Vec<u8>>>,
        differential_formats: &mut DifferentialFormats,
    ) {
        let is_inner = self.color_scale.is_some()
            || self.data_bar.is_some()
            || self.icon_set.is_some()
            || !self.formulas.is_empty()
            || self.ext_id.has_value();

        // cfRule
        let dxf_id = self
            .style
            .as_deref()
            .map(|v| differential_formats.set_style(v).to_string());
        write_start_tag(writer, "cfRule", self.attributes(dxf_id.as_deref(), false), !is_inner);

        if is_inner {
            // formula
            for v in &self.formulas {
                v.write_to(writer);
            }

            // colorScale
            if let Some(v) = &self.color_scale {
                v.write_to(writer);
            }

            // dataBar
            if let Some(v) = &self.data_bar {
                v.write_to(writer);
            }

            // iconSet
            if let Some(v) = &self.icon_set {
                v.write_to(writer);
            }

            // extLst
            if self.ext_id.has_value() {
                write_start_tag(writer, "extLst", vec![], false);
                write_start_tag(
                    writer,
                    "ext",
                    vec![
                        ("uri", "{B025F937-C7B1-47D3-B67F-A62EFF666E3E}").into(),
                        ("xmlns:x14", SHEET_MS_MAIN_NS).into(),
                    ],
                    false,
                );
                write_start_tag(writer, "x14:id", vec![], false);
                write_text_node(writer, self.ext_id.value_str());
                write_end_tag(writer, "x14:id");
                write_end_tag(writer, "ext");
                write_end_tag(writer, "extLst");
            }

            write_end_tag(writer, "cfRule");
        }
    }

    /// Writes the rule as an Excel 2010 rule (`x14:cfRule`, its format inline).
    pub(crate) fn write_to_x14(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        write_start_tag(writer, "x14:cfRule", self.attributes(None, true), false);

        for v in &self.formulas {
            write_start_tag(writer, "xm:f", vec![], false);
            write_text_node(writer, v.address_str());
            write_end_tag(writer, "xm:f");
        }
        if let Some(v) = &self.color_scale {
            v.write_to_x14(writer);
        }
        if let Some(v) = &self.data_bar {
            v.write_to_x14(writer);
        }
        if let Some(v) = &self.icon_set {
            v.write_to_x14(writer);
        }
        if let Some(v) = &self.style {
            let mut dxf = DifferentialFormat::default();
            dxf.set_style(v);
            dxf.write_to_tag(writer, "x14:dxf", super::differential_formats::custom_numfmt_id(0));
        }

        write_end_tag(writer, "x14:cfRule");
    }
}
