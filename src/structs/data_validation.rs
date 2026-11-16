// dataValidation
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
    DataValidationErrorStyleValues,
    DataValidationImeModeValues,
    DataValidationOperatorValues,
    DataValidationValues,
    EnumValue,
    SequenceOfReferences,
    StringValue,
};
use crate::{
    Formula1,
    Formula2,
    reader::driver::{
        xml_read_loop,
        get_attribute,
    },
    writer::driver::{
        write_end_tag,
        write_start_tag,
    },
};

#[derive(Default, Debug, Clone)]
pub struct DataValidation {
    r#type:                 EnumValue<DataValidationValues>,
    error_style:            EnumValue<DataValidationErrorStyleValues>,
    ime_mode:               EnumValue<DataValidationImeModeValues>,
    operator:               EnumValue<DataValidationOperatorValues>,
    allow_blank:            BooleanValue,
    show_drop_down:         BooleanValue,
    show_input_message:     BooleanValue,
    show_error_message:     BooleanValue,
    prompt_title:           StringValue,
    prompt:                 StringValue,
    error_title:            StringValue,
    error_messsage:         StringValue,
    sequence_of_references: SequenceOfReferences,
    formula1:               Box<Formula1>,
    formula2:               Box<Formula2>,
}
impl DataValidation {
    pub(crate) fn rename_sheet(&mut self, rename: &crate::helper::sheet_rename::SheetRename) {
        if let Some(new) = rename.formula(self.formula1()) {
            self.set_formula1(new);
        }
        if let Some(new) = rename.formula(self.formula2()) {
            self.set_formula2(new);
        }
    }

    #[inline]
    #[must_use]
    pub fn get_type(&self) -> &DataValidationValues {
        self.r#type.value()
    }

    #[inline]
    pub fn set_type(&mut self, value: DataValidationValues) -> &mut Self {
        self.r#type.set_value(value);
        self
    }

    /// The style of the error alert: stop (the default), warning or information.
    #[inline]
    #[must_use]
    pub fn error_style(&self) -> &DataValidationErrorStyleValues {
        self.error_style.value()
    }

    #[inline]
    pub fn set_error_style(&mut self, value: DataValidationErrorStyleValues) -> &mut Self {
        self.error_style.set_value(value);
        self
    }

    /// The input method mode for the cells (noControl by default).
    #[inline]
    #[must_use]
    pub fn ime_mode(&self) -> &DataValidationImeModeValues {
        self.ime_mode.value()
    }

    #[inline]
    pub fn set_ime_mode(&mut self, value: DataValidationImeModeValues) -> &mut Self {
        self.ime_mode.set_value(value);
        self
    }

    /// `showDropDown`: true hides the in-cell drop-down of a list (the name is the
    /// other way round in the file format).
    #[inline]
    #[must_use]
    pub fn show_drop_down(&self) -> bool {
        self.show_drop_down.value()
    }

    #[inline]
    pub fn set_show_drop_down(&mut self, value: bool) -> &mut Self {
        self.show_drop_down.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn operator(&self) -> &DataValidationOperatorValues {
        self.operator.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use operator()")]
    pub fn get_operator(&self) -> &DataValidationOperatorValues {
        self.operator()
    }

    #[inline]
    pub fn set_operator(&mut self, value: DataValidationOperatorValues) -> &mut Self {
        self.operator.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn allow_blank(&self) -> bool {
        self.allow_blank.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use allow_blank()")]
    pub fn get_allow_blank(&self) -> bool {
        self.allow_blank()
    }

    #[inline]
    pub fn set_allow_blank(&mut self, value: bool) -> &mut Self {
        self.allow_blank.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn show_input_message(&self) -> bool {
        self.show_input_message.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use show_input_message()")]
    pub fn get_show_input_message(&self) -> bool {
        self.show_input_message()
    }

    #[inline]
    pub fn set_show_input_message(&mut self, value: bool) -> &mut Self {
        self.show_input_message.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn show_error_message(&self) -> bool {
        self.show_error_message.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use show_error_message()")]
    pub fn get_show_error_message(&self) -> bool {
        self.show_error_message()
    }

    #[inline]
    pub fn set_show_error_message(&mut self, value: bool) -> &mut Self {
        self.show_error_message.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn prompt_title(&self) -> &str {
        self.prompt_title.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use prompt_title()")]
    pub fn get_prompt_title(&self) -> &str {
        self.prompt_title()
    }

    #[inline]
    pub fn set_prompt_title<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.prompt_title.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn error_title(&self) -> &str {
        self.error_title.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use error_title()")]
    pub fn get_error_title(&self) -> &str {
        self.error_title()
    }

    #[inline]
    pub fn set_error_title<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.error_title.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn error_message(&self) -> &str {
        self.error_messsage.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use error_message()")]
    pub fn get_error_message(&self) -> &str {
        self.error_message()
    }

    #[inline]
    pub fn set_error_message<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.error_messsage.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn prompt(&self) -> &str {
        self.prompt.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use prompt()")]
    pub fn get_prompt(&self) -> &str {
        self.prompt()
    }

    #[inline]
    pub fn set_prompt<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.prompt.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn sequence_of_references(&self) -> &SequenceOfReferences {
        &self.sequence_of_references
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use sequence_of_references()")]
    pub fn get_sequence_of_references(&self) -> &SequenceOfReferences {
        self.sequence_of_references()
    }

    #[inline]
    pub fn sequence_of_references_mut(&mut self) -> &mut SequenceOfReferences {
        &mut self.sequence_of_references
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use sequence_of_references_mut()")]
    pub fn get_sequence_of_references_mut(&mut self) -> &mut SequenceOfReferences {
        self.sequence_of_references_mut()
    }

    #[inline]
    pub fn set_sequence_of_references(&mut self, value: SequenceOfReferences) -> &mut Self {
        self.sequence_of_references = value;
        self
    }

    #[inline]
    #[must_use]
    pub fn formula1(&self) -> &str {
        self.formula1.text()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use formula1()")]
    pub fn get_formula1(&self) -> &str {
        self.formula1()
    }

    #[inline]
    pub fn set_formula1<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.formula1.set_text(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn formula2(&self) -> &str {
        self.formula2.text()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use formula2()")]
    pub fn get_formula2(&self) -> &str {
        self.formula2()
    }

    #[inline]
    pub fn set_formula2<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.formula2.set_text(value);
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        empty_flg: bool,
    ) {
        if let Some(v) = get_attribute(e, b"type") {
            self.r#type.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"errorStyle") {
            self.error_style.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"imeMode") {
            self.ime_mode.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"operator") {
            self.operator.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"allowBlank") {
            self.allow_blank.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"showDropDown") {
            self.show_drop_down.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"showInputMessage") {
            self.show_input_message.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"showErrorMessage") {
            self.show_error_message.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"errorTitle") {
            self.error_title.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"error") {
            self.error_messsage.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"promptTitle") {
            self.prompt_title.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"prompt") {
            self.prompt.set_value_string(v);
        }

        if let Some(v) = get_attribute(e, b"sqref") {
            self.sequence_of_references.set_sqref(v);
        }

        if empty_flg {
            return;
        }

        xml_read_loop!(
            reader,
            Event::Start(ref e) => {
                match e.name().into_inner() {
                    b"formula1" => {
                        let mut obj = Formula1::default();
                        obj.set_attributes(reader, e);
                        *self.formula1 = obj;
                    }
                    b"formula2" => {
                        let mut obj = Formula2::default();
                        obj.set_attributes(reader, e);
                        *self.formula2 = obj;
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"dataValidation" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "dataValidation")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let is_inner = !self.formula1.text().is_empty() || !self.formula2.text().is_empty();

        // dataValidation
        let mut attributes: crate::structs::AttrCollection = Vec::new();

        if self.r#type.has_value() {
            attributes.push(("type", self.r#type.value_string()).into());
        }

        if self.error_style.has_value() {
            attributes.push(("errorStyle", self.error_style.value_string()).into());
        }

        if self.ime_mode.has_value() {
            attributes.push(("imeMode", self.ime_mode.value_string()).into());
        }

        if self.allow_blank.has_value() {
            attributes.push(("allowBlank", self.allow_blank.value_string()).into());
        }

        if self.show_drop_down.has_value() {
            attributes.push(("showDropDown", self.show_drop_down.value_string()).into());
        }

        if self.show_input_message.has_value() {
            attributes.push(
                (
                    "showInputMessage",
                    self.show_input_message.value_string(),
                )
                    .into(),
            );
        }

        if self.operator.has_value() {
            attributes.push(("operator", self.operator.value_string()).into());
        }

        if self.show_error_message.has_value() {
            attributes.push(
                (
                    "showErrorMessage",
                    self.show_error_message.value_string(),
                )
                    .into(),
            );
        }

        if self.error_title.has_value() {
            attributes.push(("errorTitle", self.error_title.value_str()).into());
        }

        if self.error_messsage.has_value() {
            attributes.push(("error", self.error_messsage.value_str()).into());
        }

        if self.prompt_title.has_value() {
            attributes.push(("promptTitle", self.prompt_title.value_str()).into());
        }

        if self.prompt.has_value() {
            attributes.push(("prompt", self.prompt.value_str()).into());
        }

        let sequence_of_references = &self.sequence_of_references.get_sqref();
        if !sequence_of_references.is_empty() {
            attributes.push(("sqref", sequence_of_references).into());
        }

        write_start_tag(writer, "dataValidation", attributes, !is_inner);
        if is_inner {
            // formula1
            if !self.formula1.text().is_empty() {
                self.formula1.write_to(writer);
            }
            // formula2
            if !self.formula2.text().is_empty() {
                self.formula2.write_to(writer);
            }
            write_end_tag(writer, "dataValidation");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::Reader;

    #[test]
    fn formula1_preserves_numeric_character_references() {
        // Excel/LibreOffice may emit en-dashes as `&#8211;`. quick-xml surfaces
        // those as GeneralRef events between Text fragments; we must append both.
        let xml = concat!(
            r#"<dataValidation type="list" sqref="D41" allowBlank="1">"#,
            r#"<formula1>"0&#8211;3 years ago,4&#8211;5 years ago,6&#8211;7 years ago"</formula1>"#,
            r#"</dataValidation>"#,
        );
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let Event::Start(e) = reader.read_event_into(&mut buf).unwrap() else {
            panic!("expected Start dataValidation");
        };
        let mut dv = DataValidation::default();
        dv.set_attributes(&mut reader, &e, false);
        assert_eq!(
            dv.formula1(),
            "\"0–3 years ago,4–5 years ago,6–7 years ago\"",
            "full list formula must survive entity split"
        );
        assert_eq!(dv.sequence_of_references().get_sqref(), "D41");
        assert_eq!(dv.get_type(), &DataValidationValues::List);
    }

    #[test]
    fn formula1_plain_text_still_works() {
        let xml = concat!(
            r#"<dataValidation type="list" sqref="C5" allowBlank="1">"#,
            r#"<formula1>"petrol,diesel,LPG,electric"</formula1>"#,
            r#"</dataValidation>"#,
        );
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let Event::Start(e) = reader.read_event_into(&mut buf).unwrap() else {
            panic!("expected Start dataValidation");
        };
        let mut dv = DataValidation::default();
        dv.set_attributes(&mut reader, &e, false);
        assert_eq!(dv.formula1(), "\"petrol,diesel,LPG,electric\"");
    }

    fn read(xml: &str) -> DataValidation {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        let (e, empty) = match reader.read_event_into(&mut buf).unwrap() {
            Event::Start(e) => (e.into_owned(), false),
            Event::Empty(e) => (e.into_owned(), true),
            _ => panic!("expected dataValidation"),
        };
        let mut dv = DataValidation::default();
        dv.set_attributes(&mut reader, &e, empty);
        dv
    }

    fn write(dv: &DataValidation) -> String {
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        dv.write_to(&mut writer);
        String::from_utf8(writer.into_inner().into_inner()).unwrap()
    }

    #[test]
    fn keeps_both_formulas_of_between() {
        let dv = read(concat!(
            r#"<dataValidation type="whole" operator="between" allowBlank="1" sqref="C2:C11">"#,
            r#"<formula1>1</formula1><formula2>100</formula2></dataValidation>"#,
        ));
        let xml = write(&dv);
        assert!(xml.contains("<formula1>1</formula1>"), "{xml}");
        assert!(xml.contains("<formula2>100</formula2>"), "{xml}");
        assert!(xml.contains(r#"operator="between""#), "{xml}");
        assert_eq!(read(&xml).formula2(), "100");
    }

    #[test]
    fn keeps_error_style_ime_mode_and_drop_down() {
        let dv = read(concat!(
            r#"<dataValidation type="custom" errorStyle="warning" imeMode="hiragana" "#,
            r#"showDropDown="1" sqref="A2:A6"><formula1>COUNTIF($A:$A,A2)=1</formula1>"#,
            r#"</dataValidation>"#,
        ));
        assert_eq!(dv.error_style(), &DataValidationErrorStyleValues::Warning);
        assert_eq!(dv.ime_mode(), &DataValidationImeModeValues::Hiragana);
        assert!(dv.show_drop_down());
        let xml = write(&dv);
        assert!(xml.contains(r#"errorStyle="warning""#), "{xml}");
        assert!(xml.contains(r#"imeMode="hiragana""#), "{xml}");
        assert!(xml.contains(r#"showDropDown="1""#), "{xml}");
        assert!(xml.contains("<formula1>COUNTIF($A:$A,A2)=1</formula1>"), "{xml}");
    }

    #[test]
    fn writes_no_formula_element_when_there_is_none() {
        let dv = read(r#"<dataValidation type="none" sqref="B1"/>"#);
        let xml = write(&dv);
        assert!(xml.ends_with("/>"), "{xml}");
        assert!(!xml.contains("formula"), "{xml}");
    }
}
