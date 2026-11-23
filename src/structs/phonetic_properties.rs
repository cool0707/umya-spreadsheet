//! How furigana are shown (`phoneticPr`): Excel's "Phonetic Settings"
//! (ふりがなの設定): the kind of characters, the alignment, the font. A sheet
//! has one for its cells (the default of what is typed), and each text with
//! furigana one of its own.

/// The characters furigana are shown in (`type`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PhoneticType {
    /// Half-width katakana (半角カタカナ).
    HalfwidthKatakana,
    /// Full-width katakana (全角カタカナ): the default.
    #[default]
    FullwidthKatakana,
    /// Hiragana (ひらがな).
    Hiragana,
    /// As typed, without conversion.
    NoConversion,
}

impl PhoneticType {
    /// As the file has it (`Hiragana`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HalfwidthKatakana => "halfwidthKatakana",
            Self::FullwidthKatakana => "fullwidthKatakana",
            Self::Hiragana => "Hiragana",
            Self::NoConversion => "noConversion",
        }
    }

    /// From the file (`Hiragana`); None for another value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "halfwidthKatakana" => Some(Self::HalfwidthKatakana),
            "fullwidthKatakana" => Some(Self::FullwidthKatakana),
            "Hiragana" => Some(Self::Hiragana),
            "noConversion" => Some(Self::NoConversion),
            _ => None,
        }
    }
}

/// How furigana are aligned over their text (`alignment`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PhoneticAlignment {
    /// Not aligned (割り付けなし).
    NoControl,
    /// Left (左寄せ): the default.
    #[default]
    Left,
    /// Centered (中央揃え).
    Center,
    /// Spread over the text (均等割り付け).
    Distributed,
}

impl PhoneticAlignment {
    /// As the file has it (`center`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoControl => "noControl",
            Self::Left => "left",
            Self::Center => "center",
            Self::Distributed => "distributed",
        }
    }

    /// From the file (`center`); None for another value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "noControl" => Some(Self::NoControl),
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "distributed" => Some(Self::Distributed),
            _ => None,
        }
    }
}

/// How furigana are shown (`phoneticPr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PhoneticProperties {
    font_id:        u32,
    phonetic_type:  PhoneticType,
    alignment:      PhoneticAlignment,
}

impl Default for PhoneticProperties {
    /// Full-width katakana, left, the font Excel gives furigana (the second
    /// of the workbook's fonts).
    fn default() -> Self {
        Self {
            font_id:       1,
            phonetic_type: PhoneticType::default(),
            alignment:     PhoneticAlignment::default(),
        }
    }
}

impl PhoneticProperties {
    #[must_use]
    pub fn phonetic_type(&self) -> PhoneticType {
        self.phonetic_type
    }

    pub fn set_phonetic_type(&mut self, value: PhoneticType) -> &mut Self {
        self.phonetic_type = value;
        self
    }

    #[must_use]
    pub fn alignment(&self) -> PhoneticAlignment {
        self.alignment
    }

    pub fn set_alignment(&mut self, value: PhoneticAlignment) -> &mut Self {
        self.alignment = value;
        self
    }

    /// The font of the furigana: its place in the fonts of the workbook's
    /// styles (`Stylesheet`).
    #[must_use]
    pub fn font_id(&self) -> u32 {
        self.font_id
    }

    pub fn set_font_id(&mut self, value: u32) -> &mut Self {
        self.font_id = value;
        self
    }

    /// From the start tag of a `phoneticPr` (`<phoneticPr fontId="1"
    /// type="Hiragana"/>`); what it does not say is the default.
    #[must_use]
    pub fn parse(tag: &str) -> Self {
        let attr = |name: &str| {
            let key = format!(" {name}=\"");
            let i = tag.find(&key)? + key.len();
            Some(&tag[i..i + tag[i..].find('"')?])
        };
        let mut out = Self::default();
        if let Some(v) = attr("fontId").and_then(|v| v.parse().ok()) {
            out.font_id = v;
        }
        if let Some(v) = attr("type").and_then(PhoneticType::parse) {
            out.phonetic_type = v;
        }
        if let Some(v) = attr("alignment").and_then(PhoneticAlignment::parse) {
            out.alignment = v;
        }
        out
    }

    pub(crate) fn read(e: &quick_xml::events::BytesStart<'_>) -> Self {
        let mut out = Self::default();
        for a in e.attributes().with_checks(false).flatten() {
            let value = crate::reader::driver::unescaped_attribute_value(&a);
            match a.key.local_name().as_ref() {
                b"fontId" => {
                    if let Ok(v) = value.parse() {
                        out.font_id = v;
                    }
                }
                b"type" => {
                    if let Some(v) = PhoneticType::parse(&value) {
                        out.phonetic_type = v;
                    }
                }
                b"alignment" => {
                    if let Some(v) = PhoneticAlignment::parse(&value) {
                        out.alignment = v;
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// As the file has it, leaving out what is the default.
    #[must_use]
    pub fn to_xml(&self) -> String {
        use std::fmt::Write as _;
        let mut out = format!("<phoneticPr fontId=\"{}\"", self.font_id);
        if self.phonetic_type != PhoneticType::default() {
            let _ = write!(out, " type=\"{}\"", self.phonetic_type.as_str());
        }
        if self.alignment != PhoneticAlignment::default() {
            let _ = write!(out, " alignment=\"{}\"", self.alignment.as_str());
        }
        out.push_str("/>");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_written() {
        let p = PhoneticProperties::parse(r#"<phoneticPr fontId="3" type="Hiragana" alignment="distributed"/>"#);
        assert_eq!((p.font_id(), p.phonetic_type(), p.alignment()), (3, PhoneticType::Hiragana, PhoneticAlignment::Distributed));
        assert_eq!(p.to_xml(), r#"<phoneticPr fontId="3" type="Hiragana" alignment="distributed"/>"#);
        assert_eq!(PhoneticProperties::parse("<phoneticPr fontId=\"1\"/>"), PhoneticProperties::default());
        assert_eq!(PhoneticProperties::default().to_xml(), r#"<phoneticPr fontId="1"/>"#);
    }
}
