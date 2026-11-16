//! Renaming a sheet in what refers to it: the formulas (`元!A1`,
//! `'売上 表'!$B$2:$B$5`, `Sheet1:Sheet3!A1`) and the XML of the parts kept
//! as they were read (the `f` elements of charts and sparklines, the
//! `fmlaRange` of form controls, the `sheet` of the source of a pivot cache).
//!
//! As in Excel, the name is compared without regard to case, references to
//! the sheets of other workbooks (`[1]Sheet1!A1`) and the text of strings
//! (`INDIRECT("Sheet1!A1")`) are left alone.

use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    escape::{
        partial_escape,
        unescape,
    },
    events::{
        BytesStart,
        BytesText,
        Event,
        attributes::Attribute,
    },
};

/// A sheet renamed from `old` to `new`.
#[derive(Clone, Debug)]
pub(crate) struct SheetRename {
    old:       String,
    old_lower: String,
    new:       String,
}

/// The attributes of the parts kept that hold a formula: those of form
/// controls (`ctrlProps`).
const FORMULA_ATTRIBUTES: [&[u8]; 4] = [b"fmlaLink", b"fmlaRange", b"fmlaGroup", b"fmlaTxbx"];

impl SheetRename {
    pub(crate) fn new(old: &str, new: &str) -> Self {
        Self {
            old:       old.to_string(),
            old_lower: old.to_lowercase(),
            new:       new.to_string(),
        }
    }

    /// Whether `name` (unquoted) is the sheet renamed.
    pub(crate) fn matches(&self, name: &str) -> bool {
        !self.old.is_empty() && name.to_lowercase() == self.old_lower
    }

    pub(crate) fn new_name(&self) -> &str {
        &self.new
    }

    /// The formula (with or without the leading `=`) with the references to
    /// the sheet renamed, or `None` when it has none.
    pub(crate) fn formula(&self, formula: &str) -> Option<String> {
        if self.old.is_empty() || !formula.contains('!') {
            return None;
        }
        let chars: Vec<char> = formula.chars().collect();
        let mut out = String::with_capacity(formula.len() + self.new.len());
        let mut changed = false;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            match c {
                '"' => {
                    // A string: up to the closing quote ("" inside).
                    let start = i;
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '"' {
                            if chars.get(i + 1) == Some(&'"') {
                                i += 2;
                                continue;
                            }
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    out.extend(&chars[start..i]);
                }
                '\'' => {
                    let start = i;
                    let mut name = String::new();
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '\'' {
                            if chars.get(i + 1) == Some(&'\'') {
                                name.push('\'');
                                i += 2;
                                continue;
                            }
                            i += 1;
                            break;
                        }
                        name.push(chars[i]);
                        i += 1;
                    }
                    match (chars.get(i) == Some(&'!')).then(|| self.prefix(&name, true)).flatten() {
                        Some(new) => {
                            out.push_str(&new);
                            changed = true;
                        }
                        None => out.extend(&chars[start..i]),
                    }
                }
                '#' => {
                    // An error (`#REF!`, `#DIV/0!`, `#N/A`) or a spill (`A1#`).
                    let start = i;
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '/') {
                        i += 1;
                    }
                    if i < chars.len() && (chars[i] == '!' || chars[i] == '?') {
                        i += 1;
                    }
                    out.extend(&chars[start..i]);
                }
                c if is_delimiter(c) => {
                    out.push(c);
                    i += 1;
                }
                _ => {
                    // A run of a name, a reference or a number; brackets
                    // (`[1]`, `表[[#This Row],[金額]]`) are taken whole.
                    let start = i;
                    let mut bracket = false;
                    while i < chars.len() {
                        match chars[i] {
                            '[' => {
                                bracket = true;
                                let mut depth = 0;
                                while i < chars.len() {
                                    match chars[i] {
                                        '[' => depth += 1,
                                        ']' => {
                                            depth -= 1;
                                            if depth == 0 {
                                                i += 1;
                                                break;
                                            }
                                        }
                                        _ => {}
                                    }
                                    i += 1;
                                }
                            }
                            c if c == ':' || !is_delimiter(c) => i += 1,
                            _ => break,
                        }
                    }
                    let run: String = chars[start..i].iter().collect();
                    let new = (!bracket && chars.get(i) == Some(&'!'))
                        .then(|| self.prefix(&run, false))
                        .flatten();
                    match new {
                        Some(new) => {
                            out.push_str(&new);
                            changed = true;
                        }
                        None => out.push_str(&run),
                    }
                }
            }
        }
        changed.then_some(out)
    }

    /// The sheet part of a reference (before the `!`, unquoted) written with
    /// the new name, or `None` when it is not the sheet renamed.
    fn prefix(&self, name: &str, quoted: bool) -> Option<String> {
        if name.starts_with('[') {
            return None;
        }
        if let Some((first, last)) = name.split_once(':') {
            // `A1:Sheet1` (the end of a range) when unquoted, or the sheets
            // of a 3-D reference.
            if !quoted && looks_like_reference(first) {
                return self.matches(last).then(|| format!("{first}:{}", quote_sheet_name(&self.new)));
            }
            if !self.matches(first) && !self.matches(last) {
                return None;
            }
            let first = if self.matches(first) { self.new.as_str() } else { first };
            let last = if self.matches(last) { self.new.as_str() } else { last };
            let both = format!("{first}:{last}");
            return Some(if needs_quoting(first) || needs_quoting(last) {
                format!("'{}'", both.replace('\'', "''"))
            } else {
                both
            });
        }
        self.matches(name).then(|| quote_sheet_name(&self.new))
    }

    /// XML (of a part, or an element) with the sheet renamed in the text of
    /// the `f` elements, in the formulas of form controls and in the `sheet`
    /// of `worksheetSource`; `None` when nothing changed.
    pub(crate) fn xml(&self, data: &[u8]) -> Option<Vec<u8>> {
        // Leave the XML that does not have the name (as it is, or escaped) as it is.
        let mentioned = [
            self.old_lower.clone(),
            partial_escape(&self.old_lower).into_owned(),
            quick_xml::escape::escape(&self.old_lower).into_owned(),
        ]
        .iter()
        .any(|name| contains_ignore_ascii_case(data, name.as_bytes()));
        if self.old.is_empty() || !mentioned {
            return None;
        }
        let mut reader = Reader::from_reader(data);
        let mut writer = Writer::new(Cursor::new(Vec::new()));
        let mut buf = Vec::new();
        let mut changed = false;
        // The text of the open `f` element (escaped), when one is open.
        let mut formula: Option<String> = None;
        loop {
            buf.clear();
            let event = match reader.read_event_into(&mut buf) {
                Ok(Event::Eof) => break,
                Ok(event) => event,
                Err(_) => return None,
            };
            if let Some(text) = &mut formula {
                match &event {
                    Event::Text(t) => {
                        text.push_str(&String::from_utf8_lossy(t));
                        continue;
                    }
                    Event::GeneralRef(r) => {
                        text.push('&');
                        text.push_str(&String::from_utf8_lossy(r));
                        text.push(';');
                        continue;
                    }
                    _ => {
                        let raw = formula.take().unwrap_or_default();
                        let new = match &event {
                            Event::End(e) if e.local_name().into_inner() == b"f" => {
                                unescape(&raw).ok().and_then(|plain| self.formula(&plain))
                            }
                            _ => None,
                        };
                        match new {
                            Some(new) => {
                                changed = true;
                                writer
                                    .write_event(Event::Text(BytesText::from_escaped(partial_escape(&new))))
                                    .ok()?;
                            }
                            None if raw.is_empty() => {}
                            None => writer.write_event(Event::Text(BytesText::from_escaped(raw))).ok()?,
                        }
                    }
                }
            }
            let event = match event {
                Event::Start(e) => {
                    if e.local_name().into_inner() == b"f" {
                        formula = Some(String::new());
                    }
                    match self.attributes(&e) {
                        Some(e) => {
                            changed = true;
                            Event::Start(e)
                        }
                        None => Event::Start(e),
                    }
                }
                Event::Empty(e) => match self.attributes(&e) {
                    Some(e) => {
                        changed = true;
                        Event::Empty(e)
                    }
                    None => Event::Empty(e),
                },
                event => event,
            };
            writer.write_event(event).ok()?;
        }
        changed.then(|| writer.into_inner().into_inner())
    }

    /// The start tag with the sheet renamed in its attributes, or `None`
    /// when they do not refer to it.
    fn attributes(&self, e: &BytesStart<'_>) -> Option<BytesStart<'static>> {
        let source = e.local_name().into_inner() == b"worksheetSource";
        let mut changed = false;
        let mut values: Vec<(Vec<u8>, Option<String>)> = Vec::new();
        for a in e.attributes().with_checks(false) {
            let a = a.ok()?;
            let key = a.key.as_ref();
            let local = a.key.local_name().into_inner();
            let raw = crate::reader::driver::get_attribute_value(&a).ok()?;
            let value = unescape(&raw).ok()?;
            let new = if source && local == b"sheet" {
                self.matches(&value).then(|| self.new.clone())
            } else if FORMULA_ATTRIBUTES.contains(&local) {
                self.formula(&value)
            } else {
                None
            };
            changed |= new.is_some();
            values.push((key.to_vec(), new));
        }
        if !changed {
            return None;
        }
        let mut out = BytesStart::new(String::from_utf8_lossy(e.name().into_inner()).into_owned());
        for (a, (_, new)) in e.attributes().with_checks(false).flatten().zip(values) {
            match new {
                Some(new) => {
                    let value = partial_escape(&new).replace('"', "&quot;");
                    out.push_attribute(Attribute {
                        key:   a.key,
                        value: value.into_bytes().into(),
                    });
                }
                None => out.push_attribute(a),
            }
        }
        Some(out)
    }
}

/// The characters that end a name or reference in a formula.
fn is_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '(' | ')' | '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | ',' | ';' | '{' | '}' | '"' | '\'' | '!'
                | '%' | '#' | '@' | ':'
        )
}

/// `A1`, `$B$2`, `C`, `3`: the start of a range rather than a sheet.
fn looks_like_reference(text: &str) -> bool {
    let t = text.replace('$', "");
    let letters = t.chars().take_while(char::is_ascii_alphabetic).count();
    let rest = &t[letters..];
    !t.is_empty() && letters <= 3 && rest.chars().all(|c| c.is_ascii_digit())
}

/// Whether a sheet name must be quoted in a formula.
pub(crate) fn needs_quoting(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return true;
    };
    if !(first.is_alphabetic() || first == '_') {
        return true;
    }
    if name.chars().any(|c| !(c.is_alphanumeric() || c == '_' || c == '.')) {
        return true;
    }
    // What would be read as a reference (`A1`, `R1C1`, `R`, `C2`) or a value.
    let upper = name.to_ascii_uppercase();
    if upper.is_ascii() {
        let letters = upper.bytes().take_while(u8::is_ascii_alphabetic).count();
        if letters <= 3 && letters < upper.len() && upper[letters..].bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
        let r1c1 = |s: &str| {
            let s = s.strip_prefix('R').map_or(s, |r| r.trim_start_matches(|c: char| c.is_ascii_digit()));
            let s = s.strip_prefix('C').map_or(s, |c| c.trim_start_matches(|c: char| c.is_ascii_digit()));
            s.is_empty()
        };
        if r1c1(&upper) || upper == "TRUE" || upper == "FALSE" {
            return true;
        }
    }
    false
}

/// A sheet name as it is written before the `!` of a reference.
pub(crate) fn quote_sheet_name(name: &str) -> String {
    if needs_quoting(name) {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_string()
    }
}

fn contains_ignore_ascii_case(data: &[u8], needle: &[u8]) -> bool {
    needle.is_empty()
        || data.len() >= needle.len()
            && data.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rename(f: &str, old: &str, new: &str) -> Option<String> {
        SheetRename::new(old, new).formula(f)
    }

    #[test]
    fn references_to_the_sheet_are_renamed() {
        assert_eq!(rename("=元!B2+1", "元", "元データ").as_deref(), Some("=元データ!B2+1"));
        assert_eq!(rename("SUM(元!$B$2:$B$5)", "元", "元データ").as_deref(), Some("SUM(元データ!$B$2:$B$5)"));
        assert_eq!(rename("元!$1:$1", "元", "元データ").as_deref(), Some("元データ!$1:$1"));
        assert_eq!(rename("'売上 表'!A1*2", "売上 表", "売上").as_deref(), Some("売上!A1*2"));
        assert_eq!(rename("Sheet1!A1", "sheet1", "新 表").as_deref(), Some("'新 表'!A1"));
        assert_eq!(rename("Sheet1!A1", "Sheet1", "It's").as_deref(), Some("'It''s'!A1"));
        assert_eq!(rename("'It''s'!A1", "It's", "X").as_deref(), Some("X!A1"));
        assert_eq!(rename("(Sheet1!$A$1,Sheet1!$C$1)", "Sheet1", "S").as_deref(), Some("(S!$A$1,S!$C$1)"));
        assert_eq!(rename("Sheet1!A1:Sheet1!B2", "Sheet1", "S").as_deref(), Some("S!A1:S!B2"));
        assert_eq!(rename("Sheet1!A1#", "Sheet1", "S").as_deref(), Some("S!A1#"));
    }

    #[test]
    fn three_d_references_are_renamed() {
        assert_eq!(rename("SUM(Sheet1:Sheet3!A1)", "Sheet3", "End").as_deref(), Some("SUM(Sheet1:End!A1)"));
        assert_eq!(rename("SUM(Sheet1:Sheet3!A1)", "Sheet1", "a b").as_deref(), Some("SUM('a b:Sheet3'!A1)"));
        assert_eq!(rename("SUM('a b:Sheet3'!A1)", "a b", "Start").as_deref(), Some("SUM(Start:Sheet3!A1)"));
    }

    #[test]
    fn other_things_are_left_alone() {
        assert_eq!(rename("Other!A1", "Sheet1", "S"), None);
        assert_eq!(rename("Sheet10!A1", "Sheet1", "S"), None);
        assert_eq!(rename("INDIRECT(\"Sheet1!A1\")", "Sheet1", "S"), None);
        assert_eq!(rename("[1]Sheet1!A1", "Sheet1", "S"), None);
        assert_eq!(rename("'[Book.xlsx]Sheet1'!A1", "Sheet1", "S"), None);
        assert_eq!(rename("#REF!+REF!A1", "REF", "S").as_deref(), Some("#REF!+S!A1"));
        assert_eq!(rename("表[[#This Row],[金額]]+Sheet1!A1", "Sheet1", "S").as_deref(), Some("表[[#This Row],[金額]]+S!A1"));
        assert_eq!(rename("Sheet1", "Sheet1", "S"), None);
    }

    #[test]
    fn names_are_quoted_when_they_must_be() {
        for name in ["元データ", "Sheet1x", "_a", "a.b", "集計2026"] {
            assert!(!needs_quoting(name), "{name}");
        }
        for name in ["a b", "1月", "A1", "xfd10", "R1C1", "R", "C", "TRUE", "a-b", "（集計）", "a,b", ""] {
            assert!(needs_quoting(name), "{name}");
        }
    }

    #[test]
    fn xml_formulas_and_attributes_are_renamed() {
        let r = SheetRename::new("元", "A&B");
        let xml = r#"<c:ser><c:f>元!$B$2:$B$5</c:f><c:v>元</c:v></c:ser>"#;
        assert_eq!(
            String::from_utf8(r.xml(xml.as_bytes()).unwrap()).unwrap(),
            r#"<c:ser><c:f>'A&amp;B'!$B$2:$B$5</c:f><c:v>元</c:v></c:ser>"#
        );
        let xml = r#"<formControlPr xmlns="x" objectType="Drop" fmlaLink="元!$C$2" fmlaRange="元!$A$2:$A$5" sel="0"/>"#;
        assert_eq!(
            String::from_utf8(r.xml(xml.as_bytes()).unwrap()).unwrap(),
            r#"<formControlPr xmlns="x" objectType="Drop" fmlaLink="'A&amp;B'!$C$2" fmlaRange="'A&amp;B'!$A$2:$A$5" sel="0"/>"#
        );
        let xml = r#"<cacheSource type="worksheet"><worksheetSource ref="A1:B5" sheet="元"/></cacheSource>"#;
        assert_eq!(
            String::from_utf8(r.xml(xml.as_bytes()).unwrap()).unwrap(),
            r#"<cacheSource type="worksheet"><worksheetSource ref="A1:B5" sheet="A&amp;B"/></cacheSource>"#
        );
        let r2 = SheetRename::new("A&B", "C");
        assert_eq!(
            String::from_utf8(r2.xml(br#"<worksheetSource ref="A1:B5" sheet="A&amp;B"/>"#).unwrap()).unwrap(),
            r#"<worksheetSource ref="A1:B5" sheet="C"/>"#
        );
        assert!(r.xml(br#"<c:f>Other!A1</c:f>"#).is_none());
        assert!(SheetRename::new("Sheet1", "S").xml(br#"<xm:f>SHEET1!A1</xm:f>"#).is_some());
    }
}
