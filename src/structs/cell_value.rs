use std::{
    borrow::Cow,
    str::FromStr,
};

use super::{
    RichText,
    SharedStringItem,
    Text,
};
use crate::{
    CellErrorType,
    structs::{
        CellFormula,
        CellRawValue,
    },
    traits::AdjustmentCoordinateWith2Sheet,
};

#[derive(Clone, Default, Debug, PartialEq, PartialOrd)]
pub struct CellValue {
    pub(crate) raw_value: CellRawValue,
    pub(crate) formula:   Option<Box<CellFormula>>,
    /// The furigana of the shared string read, with the text they are of:
    /// written back while the cell still has that text.
    pub(crate) phonetic:  Option<(Box<str>, Box<str>)>,
}
impl CellValue {
    #[inline]
    #[must_use]
    pub fn data_type(&self) -> &str {
        self.raw_value.data_type()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use data_type()")]
    pub fn get_data_type(&self) -> &str {
        self.data_type()
    }

    #[inline]
    #[must_use]
    pub fn raw_value(&self) -> &CellRawValue {
        &self.raw_value
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use raw_value()")]
    pub fn get_raw_value(&self) -> &CellRawValue {
        self.raw_value()
    }

    #[inline]
    pub(crate) fn data_type_crate(&self) -> &str {
        match &self.formula {
            Some(_) => match &self.raw_value {
                CellRawValue::String(_) | CellRawValue::RichText(_) | CellRawValue::Lazy(_) => {
                    "str"
                }
                CellRawValue::Bool(_) => "b",
                CellRawValue::Error(_) => "e",
                CellRawValue::Numeric(_) => "n",
                CellRawValue::Empty => "",
            },
            None => self.raw_value.data_type(),
        }
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use data_type_crate()")]
    pub(crate) fn get_data_type_crate(&self) -> &str {
        self.data_type_crate()
    }

    #[inline]
    #[must_use]
    pub fn value(&self) -> Cow<'static, str> {
        self.raw_value.to_string().into()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use value()")]
    pub fn get_value(&self) -> Cow<'static, str> {
        self.value()
    }

    #[inline]
    #[must_use]
    pub fn value_number(&self) -> Option<f64> {
        self.raw_value.number()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use value_number()")]
    pub fn get_value_number(&self) -> Option<f64> {
        self.value_number()
    }

    #[inline]
    pub fn value_lazy(&mut self) -> Cow<'static, str> {
        if let CellRawValue::Lazy(v) = &self.raw_value {
            self.raw_value = Self::guess_typed_data(v);
        }
        self.remove_formula();
        self.raw_value.to_string().into()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use value_lazy()")]
    pub fn get_value_lazy(&mut self) -> Cow<'static, str> {
        self.value_lazy()
    }

    #[inline]
    pub(crate) fn text(&self) -> Option<Text> {
        self.raw_value.text()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use text()")]
    pub(crate) fn get_text(&self) -> Option<Text> {
        self.text()
    }

    #[inline]
    pub(crate) fn rich_text(&self) -> Option<RichText> {
        self.raw_value.rich_text()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use rich_text()")]
    pub(crate) fn get_rich_text(&self) -> Option<RichText> {
        self.rich_text()
    }

    /// Set the raw value after trying to convert `value` into one of the
    /// supported data types. <br />
    /// Types that `value` may be converted to:
    /// - `Empty` - if the string was `""`
    /// - `Numeric` - if the string can be parsed to an `f64`
    /// - `Bool` - if the string was either `"TRUE"` or `"FALSE"`
    /// - `Error` - if the string was either
    ///   `"#VALUE!"`,`"#REF!"`,`"#NUM!"`,`"#NULL!"`,`"#NAME?"`,`"#N/A"`,`"#
    ///   DATA!"` or `"#DIV/0!"`
    /// - `String` - if the string does not fulfill any of the other conditions
    #[inline]
    pub fn set_value<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = Self::guess_typed_data(&value.into());
        self.remove_formula();
        self
    }

    #[inline]
    pub(crate) fn set_value_crate<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = Self::guess_typed_data(&value.into());
        self
    }

    #[inline]
    pub fn set_value_lazy<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = CellRawValue::Lazy(value.into().into_boxed_str());
        self
    }

    #[inline]
    pub fn set_value_string<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = CellRawValue::String(value.into().into_boxed_str());
        self.remove_formula();
        self
    }

    #[inline]
    pub(crate) fn set_value_string_crate<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = CellRawValue::String(value.into().into_boxed_str());
        self
    }

    #[inline]
    pub(crate) fn set_rich_text_crate(&mut self, value: RichText) -> &mut Self {
        self.raw_value = CellRawValue::RichText(value);
        self
    }

    #[inline]
    pub fn set_value_bool(&mut self, value: bool) -> &mut Self {
        self.raw_value = CellRawValue::Bool(value);
        self.remove_formula();
        self
    }

    #[inline]
    pub(crate) fn set_value_bool_crate(&mut self, value: bool) -> &mut Self {
        self.raw_value = CellRawValue::Bool(value);
        self
    }

    #[inline]
    pub fn set_value_number<T>(&mut self, value: T) -> &mut Self
    where
        T: Into<f64>,
    {
        self.raw_value = Self::numeric_raw_value(value.into());
        self.remove_formula();
        self
    }

    #[inline]
    pub fn set_rich_text(&mut self, value: RichText) -> &mut Self {
        self.raw_value = CellRawValue::RichText(value);
        self.remove_formula();
        self
    }

    #[inline]
    pub fn set_blank(&mut self) -> &mut Self {
        self.raw_value = CellRawValue::Empty;
        self.remove_formula();
        self
    }

    #[inline]
    #[must_use]
    pub fn is_formula(&self) -> bool {
        self.formula.is_some()
    }

    #[inline]
    #[must_use]
    pub fn formula(&self) -> &str {
        match &self.formula {
            Some(v) => v.text(),
            None => "",
        }
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use formula()")]
    pub fn get_formula(&self) -> &str {
        self.formula()
    }

    #[inline]
    #[must_use]
    pub fn formula_obj(&self) -> Option<&CellFormula> {
        self.formula.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use formula_obj()")]
    pub fn get_formula_obj(&self) -> Option<&CellFormula> {
        self.formula_obj()
    }

    #[inline]
    pub fn set_formula<S: Into<String>>(&mut self, value: S) -> &mut Self {
        let mut obj = CellFormula::default();
        obj.set_text(value.into());
        self.formula = Some(Box::new(obj));
        self
    }

    #[inline]
    pub fn set_formula_obj(&mut self, value: CellFormula) -> &mut Self {
        self.formula = Some(Box::new(value));
        self
    }

    #[inline]
    pub fn remove_formula(&mut self) -> &mut Self {
        self.formula = None;
        self
    }

    #[inline]
    pub fn set_formula_result_default<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.set_value_crate(value);
        self
    }

    /// Sets a formula cached result as a numeric value.
    ///
    /// This method only updates the cached value (`<v>`) and does not remove
    /// the formula object (`<f>`).
    #[inline]
    pub fn set_formula_result_number<T>(&mut self, value: T) -> &mut Self
    where
        T: Into<f64>,
    {
        self.raw_value = Self::numeric_raw_value(value.into());
        self
    }

    /// Sets a formula cached result as a boolean value.
    ///
    /// This method only updates the cached value (`<v>`) and does not remove
    /// the formula object (`<f>`).
    #[inline]
    pub fn set_formula_result_bool(&mut self, value: bool) -> &mut Self {
        self.raw_value = CellRawValue::Bool(value);
        self
    }

    /// Sets a formula cached result as an Excel error value.
    ///
    /// This method only updates the cached value (`<v>`) and does not remove
    /// the formula object (`<f>`).
    #[inline]
    pub fn set_formula_result_error(&mut self, value: CellErrorType) -> &mut Self {
        self.raw_value = CellRawValue::Error(value);
        self
    }

    /// Sets a formula cached result as a string value.
    ///
    /// This method only updates the cached value (`<v>`) and does not remove
    /// the formula object (`<f>`).
    #[inline]
    pub fn set_formula_result_string<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.raw_value = CellRawValue::String(value.into().into_boxed_str());
        self
    }

    /// Sets a formula cached result as blank.
    ///
    /// This method only updates the cached value (`<v>`) and does not remove
    /// the formula object (`<f>`).
    #[inline]
    pub fn set_formula_result_blank(&mut self) -> &mut Self {
        self.raw_value = CellRawValue::Empty;
        self
    }

    #[inline]
    pub fn set_error<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.set_value_crate(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.raw_value.is_error()
    }

    #[inline]
    pub(crate) fn set_shared_string_item(&mut self, value: &SharedStringItem) -> &mut Self {
        if let Some(v) = value.text() {
            self.set_value_string(v.value());
        }
        if let Some(v) = value.rich_text() {
            self.set_rich_text(v.clone());
        }
        self.phonetic = value.phonetic().map(|p| (value.plain_text().into(), p.into()));
        self
    }

    /// The furigana read for the cell, while it has the text they are of.
    pub(crate) fn phonetic(&self) -> Option<&str> {
        let (text, raw) = self.phonetic.as_ref()?;
        let now = match &self.raw_value {
            CellRawValue::String(s) => s.to_string(),
            CellRawValue::RichText(r) => r.text().to_string(),
            _ => return None,
        };
        (now == **text).then_some(&**raw)
    }

    /// The text of the cell, when it has one (a string or rich text).
    fn current_text(&self) -> Option<String> {
        match &self.raw_value {
            CellRawValue::String(s) => Some(s.to_string()),
            CellRawValue::RichText(r) => Some(r.text().to_string()),
            _ => None,
        }
    }

    /// The furigana of the text (`rPh`): for each run, the range of the
    /// text it is the reading of (`start` and `end`, offsets in UTF-16 code
    /// units) and the reading. Empty when it has none.
    #[must_use]
    pub fn phonetic_runs(&self) -> Vec<(u32, u32, String)> {
        let Some(raw) = self.phonetic() else { return Vec::new() };
        let mut reader = quick_xml::Reader::from_str(raw);
        let mut runs = Vec::new();
        let mut current: Option<(u32, u32, String)> = None;
        let mut in_t = false;
        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Start(e)) => match e.name().as_ref() {
                    b"rPh" => {
                        let num = |key: &[u8]| {
                            crate::reader::driver::get_attribute(&e, key).and_then(|v| v.parse().ok()).unwrap_or(0)
                        };
                        current = Some((num(b"sb"), num(b"eb"), String::new()));
                    }
                    b"t" => in_t = true,
                    _ => {}
                },
                Ok(quick_xml::events::Event::Text(t)) if in_t => {
                    if let Some((_, _, text)) = &mut current {
                        text.push_str(&crate::helper::utils::unescape_ooxml_string(crate::helper::utils::unescape_xml_text(&t)));
                    }
                }
                Ok(quick_xml::events::Event::End(e)) => match e.name().as_ref() {
                    b"t" => in_t = false,
                    b"rPh" => runs.extend(current.take()),
                    _ => {}
                },
                Ok(quick_xml::events::Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
        runs
    }

    /// How the furigana are written (`type` of `phoneticPr`:
    /// `fullwidthKatakana` when none is given, `halfwidthKatakana`,
    /// `Hiragana`, `noConversion`). None when the text has no furigana.
    #[must_use]
    pub fn phonetic_type(&self) -> Option<String> {
        let raw = self.phonetic()?;
        let at = raw.find("<phoneticPr")?;
        let tag = &raw[at..at + raw[at..].find('>')?];
        let key = " type=\"";
        Some(tag.find(key).map_or_else(
            || "fullwidthKatakana".to_string(),
            |i| {
                let rest = &tag[i + key.len()..];
                rest[..rest.find('"').unwrap_or(rest.len())].to_string()
            },
        ))
    }

    /// How the furigana of the text are shown (its Phonetic Settings: the
    /// kind of characters, the alignment, the font). None when the text has
    /// neither furigana nor settings of its own (the sheet's are used).
    #[must_use]
    pub fn phonetic_properties(&self) -> Option<crate::structs::PhoneticProperties> {
        let raw = self.phonetic()?;
        let at = raw.find("<phoneticPr")?;
        let tag = &raw[at..at + raw[at..].find('>')?];
        Some(crate::structs::PhoneticProperties::parse(tag))
    }

    /// Sets how the furigana of the text are shown, keeping its furigana
    /// (`None`: the default, and no settings of its own when it has no
    /// furigana). A cell without text takes none.
    pub fn set_phonetic_properties(&mut self, value: Option<&crate::structs::PhoneticProperties>) -> &mut Self {
        let Some(text) = self.current_text() else {
            self.phonetic = None;
            return self;
        };
        let runs: String = self.phonetic().map(|raw| raw[..raw.find("<phoneticPr").unwrap_or(raw.len())].to_string()).unwrap_or_default();
        let raw = match value {
            Some(p) => format!("{runs}{}", p.to_xml()),
            None if runs.is_empty() => {
                self.phonetic = None;
                return self;
            }
            None => format!("{runs}{}", crate::structs::PhoneticProperties::default().to_xml()),
        };
        self.phonetic = Some((text.into(), raw.into()));
        self
    }

    /// Gives the text of the cell these furigana: runs of (`start`, `end`
    /// in UTF-16 code units of the text, reading), written as `kind` (see
    /// [`CellValue::phonetic_type`]; None for the default). No runs remove
    /// them. They stay while the cell keeps this text; a cell without text
    /// takes none.
    pub fn set_phonetic_runs(&mut self, runs: &[(u32, u32, String)], kind: Option<&str>) -> &mut Self {
        let text = match self.current_text() {
            Some(text) if !runs.is_empty() => text,
            _ => {
                self.phonetic = None;
                return self;
            }
        };
        let mut raw = String::new();
        for (start, end, reading) in runs {
            raw.push_str(&format!(
                "<rPh sb=\"{start}\" eb=\"{end}\"><t>{}</t></rPh>",
                quick_xml::escape::escape(crate::helper::utils::escape_ooxml_string(reading).as_ref())
            ));
        }
        // The settings the text has (alignment, font) stay; the kind is `kind`.
        let mut props = self.phonetic_properties().unwrap_or_default();
        props.set_phonetic_type(
            kind.and_then(crate::structs::PhoneticType::parse)
                .unwrap_or_default(),
        );
        raw.push_str(&props.to_xml());
        self.phonetic = Some((text.into(), raw.into()));
        self
    }

    /// xlsx can't store NaN or infinity as a number, so they become `#NUM!`
    /// like the result of an invalid calculation in Excel.
    #[inline]
    fn numeric_raw_value(value: f64) -> CellRawValue {
        if value.is_finite() {
            CellRawValue::Numeric(value)
        } else {
            CellRawValue::Error(CellErrorType::Num)
        }
    }

    #[inline]
    pub(crate) fn guess_typed_data(value: &str) -> CellRawValue {
        let uppercase_value = value.to_uppercase();

        match uppercase_value.as_str() {
            "" => CellRawValue::Empty,
            "TRUE" => CellRawValue::Bool(true),
            "FALSE" => CellRawValue::Bool(false),
            _ => {
                if let Ok(error_type) = CellErrorType::from_str(&uppercase_value) {
                    CellRawValue::Error(error_type)
                } else if let Some(f) = value.parse::<f64>().ok().filter(|f| f.is_finite()) {
                    CellRawValue::Numeric(f)
                } else {
                    CellRawValue::String(value.into())
                }
            }
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.is_value_empty() && self.is_formula_empty()
    }

    #[inline]
    pub(crate) fn is_value_empty(&self) -> bool {
        self.raw_value.is_empty()
    }

    #[inline]
    pub(crate) fn is_formula_empty(&self) -> bool {
        !self.is_formula()
    }

    // When opened in software such as Excel, it is visually blank.
    #[inline]
    pub(crate) fn is_visually_empty(&self) -> bool {
        self.value() == "" && self.is_formula_empty()
    }
}
impl AdjustmentCoordinateWith2Sheet for CellValue {
    #[inline]
    fn adjustment_insert_coordinate_with_2sheet(
        &mut self,
        self_sheet_name: &str,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        if let Some(v) = &mut self.formula {
            v.adjustment_insert_coordinate_with_2sheet(
                self_sheet_name,
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
        }
    }

    #[inline]
    fn adjustment_remove_coordinate_with_2sheet(
        &mut self,
        self_sheet_name: &str,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        if let Some(v) = &mut self.formula {
            v.adjustment_remove_coordinate_with_2sheet(
                self_sheet_name,
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_value() {
        let mut obj = CellValue::default();

        obj.set_value_string(String::from("TEST"));
        assert_eq!(obj.value(), "TEST");
        assert!(obj.value_number().is_none());

        obj.set_value_string("TEST");
        assert_eq!(obj.value(), "TEST");

        obj.set_value_bool(true);
        assert_eq!(obj.value(), "TRUE");

        obj.set_value_number(1);
        assert_eq!(obj.value(), "1");

        obj.set_blank();
        assert_eq!(obj.value(), "");

        obj.set_error("#NUM!");
        assert_eq!(obj.value(), "#NUM!");
    }

    #[test]
    fn error_checking() {
        let path = std::path::Path::new("./tests/test_files/pr_204.xlsx");
        let book = crate::reader::xlsx::read(path).unwrap();
        let sheet = book.sheet(0).unwrap();

        let cell = sheet.cell_value("A1");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Div0));

        let cell = sheet.cell_value("A2");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Name));

        let cell = sheet.cell_value("A3");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Ref));

        let cell = sheet.cell_value("A4");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Value));

        let cell = sheet.cell_value("A5");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::NA));

        let cell = sheet.cell_value("A6");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Num));

        let cell = sheet.cell_value("A7");
        assert!(cell.raw_value.is_error());
        assert_eq!(cell.raw_value, CellRawValue::Error(CellErrorType::Null));
    }

    #[test]
    fn formula_cached_value_data_type_tracks_raw_value_kind() {
        let mut obj = CellValue::default();

        obj.set_formula("1+1").set_formula_result_default("2");
        assert_eq!(obj.data_type_crate(), "n");

        obj.set_formula_result_default("TRUE");
        assert_eq!(obj.data_type_crate(), "b");

        obj.set_formula_result_default("#N/A");
        assert_eq!(obj.data_type_crate(), "e");

        obj.set_formula_result_default("OK");
        assert_eq!(obj.data_type_crate(), "str");

        obj.set_formula_result_default("");
        assert_eq!(obj.data_type_crate(), "");

        obj.remove_formula();
        obj.set_value_string("OK");
        assert_eq!(obj.data_type_crate(), "s");
    }

    #[test]
    fn typed_formula_result_helpers_preserve_formula() {
        let mut obj = CellValue::default();
        obj.set_formula("A1+1");

        obj.set_formula_result_number(3.5);
        assert_eq!(obj.formula(), "A1+1");
        assert_eq!(obj.raw_value, CellRawValue::Numeric(3.5));

        obj.set_formula_result_bool(false);
        assert_eq!(obj.formula(), "A1+1");
        assert_eq!(obj.raw_value, CellRawValue::Bool(false));

        obj.set_formula_result_error(CellErrorType::Ref);
        assert_eq!(obj.formula(), "A1+1");
        assert_eq!(obj.raw_value, CellRawValue::Error(CellErrorType::Ref));

        obj.set_formula_result_string("OK");
        assert_eq!(obj.formula(), "A1+1");
        assert!(matches!(
            &obj.raw_value,
            CellRawValue::String(value) if value.as_ref() == "OK"
        ));

        obj.set_formula_result_blank();
        assert_eq!(obj.formula(), "A1+1");
        assert_eq!(obj.raw_value, CellRawValue::Empty);
    }
}
