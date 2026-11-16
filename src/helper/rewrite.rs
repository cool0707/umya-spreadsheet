//! Rewriting what refers to cells: the formulas of the workbook (to rename a
//! sheet, or to follow rows and columns inserted and removed) and the XML of
//! the parts kept as they were read.

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

use crate::helper::coordinate::{
    column_index_from_string,
    string_from_column_index,
};

/// A function that rewrites formulas: given the sheet the references
/// without a sheet name refer to (`None` when they refer to none, as in the
/// names of the workbook and the series of charts) and a formula as the file
/// holds it (no `=`), it returns the new formula, or `None` to leave it as
/// it is.
pub type FormulaFn<'a> = dyn FnMut(Option<&str>, &str) -> Option<String> + 'a;

/// The attributes of the parts kept that hold a formula: those of form
/// controls (`ctrlProps`).
const FORMULA_ATTRIBUTES: [&[u8]; 4] = [b"fmlaLink", b"fmlaRange", b"fmlaGroup", b"fmlaTxbx"];

/// What [`rewrite_xml`] changes.
pub(crate) trait XmlRewrite {
    /// The text of an `f` element or a formula attribute of a form control.
    fn formula(&mut self, formula: &str) -> Option<String>;

    /// The `sheet` of a `worksheetSource` (the source of a pivot cache).
    fn source_sheet(&mut self, _sheet: &str) -> Option<String> {
        None
    }

    /// An area of cells: of the sheet `sheet` (the `ref` of a
    /// `worksheetSource`), or of the sheet of the XML when `None` (the text of
    /// an `sqref` element, then `sqref` is true, or the `ref` of the
    /// `location` of a pivot table).
    fn area(&mut self, _sheet: Option<&str>, _area: &str, _sqref: bool) -> Option<String> {
        None
    }
}

/// XML (of a part, or an element) rewritten by `rw`; `None` when nothing
/// changed.
pub(crate) fn rewrite_xml(data: &[u8], rw: &mut dyn XmlRewrite) -> Option<Vec<u8>> {
    let mut reader = Reader::from_reader(data);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut changed = false;
    // The text of the open `f` or `sqref` element (escaped), when one is open.
    let mut text: Option<(bool, String)> = None;
    loop {
        buf.clear();
        let event = match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(event) => event,
            Err(_) => return None,
        };
        if let Some((_, t)) = &mut text {
            match &event {
                Event::Text(e) => {
                    t.push_str(&String::from_utf8_lossy(e));
                    continue;
                }
                Event::GeneralRef(r) => {
                    t.push('&');
                    t.push_str(&String::from_utf8_lossy(r));
                    t.push(';');
                    continue;
                }
                _ => {
                    let (formula, raw) = text.take().unwrap_or_default();
                    let new = match &event {
                        Event::End(e) if matches!(e.local_name().into_inner(), b"f" | b"sqref") => {
                            unescape(&raw).ok().and_then(|plain| {
                                if formula {
                                    rw.formula(&plain)
                                } else {
                                    rw.area(None, &plain, true)
                                }
                            })
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
                match e.local_name().into_inner() {
                    b"f" => text = Some((true, String::new())),
                    b"sqref" => text = Some((false, String::new())),
                    _ => {}
                }
                match attributes(rw, &e) {
                    Some(e) => {
                        changed = true;
                        Event::Start(e)
                    }
                    None => Event::Start(e),
                }
            }
            Event::Empty(e) => match attributes(rw, &e) {
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

/// The start tag with its attributes rewritten, or `None` when they are not.
fn attributes(rw: &mut dyn XmlRewrite, e: &BytesStart<'_>) -> Option<BytesStart<'static>> {
    let element = e.local_name().into_inner();
    let source = element == b"worksheetSource";
    let location = element == b"location";
    let mut values: Vec<(Vec<u8>, String)> = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.ok()?;
        let raw = crate::reader::driver::get_attribute_value(&a).ok()?;
        values.push((a.key.local_name().into_inner().to_vec(), unescape(&raw).ok()?.into_owned()));
    }
    let sheet = values.iter().find(|(k, _)| k == b"sheet").map(|(_, v)| v.clone());
    let mut changed = false;
    let mut news: Vec<Option<String>> = Vec::with_capacity(values.len());
    for (local, value) in &values {
        let new = if source && local == b"sheet" {
            rw.source_sheet(value)
        } else if source && local == b"ref" {
            sheet.as_deref().and_then(|sheet| rw.area(Some(sheet), value, false))
        } else if location && local == b"ref" {
            rw.area(None, value, false)
        } else if FORMULA_ATTRIBUTES.contains(&local.as_slice()) {
            rw.formula(value)
        } else {
            None
        };
        changed |= new.is_some();
        news.push(new);
    }
    if !changed {
        return None;
    }
    let mut out = BytesStart::new(String::from_utf8_lossy(e.name().into_inner()).into_owned());
    for (a, new) in e.attributes().with_checks(false).flatten().zip(news) {
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

/// Formulas rewritten in XML by a [`FormulaFn`], the references without a
/// sheet name referring to `host`.
pub(crate) struct FormulaXml<'a, 'b> {
    pub(crate) host: Option<&'a str>,
    pub(crate) f:    &'a mut FormulaFn<'b>,
}

impl XmlRewrite for FormulaXml<'_, '_> {
    fn formula(&mut self, formula: &str) -> Option<String> {
        (self.f)(self.host, formula)
    }
}

// ---- rows and columns inserted and removed ----

/// Rows or columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineAxis {
    Rows,
    Columns,
}

/// Rows or columns inserted into or removed from a sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineEdit {
    pub axis:   LineAxis,
    /// The first row or column (1-based) inserted (the ones there move
    /// down or right) or removed.
    pub at:     u32,
    pub count:  u32,
    pub insert: bool,
    /// The names columns inserted into a table get: this and a number
    /// (`Column1`, `Column2`...).
    pub table_column: String,
}

pub(crate) const MAX_ROWS: u32 = 1_048_576;
pub(crate) const MAX_COLS: u32 = 16_384;

impl LineEdit {
    #[must_use]
    pub fn insert_rows(at: u32, count: u32) -> Self {
        Self::new(LineAxis::Rows, at, count, true)
    }

    #[must_use]
    pub fn remove_rows(at: u32, count: u32) -> Self {
        Self::new(LineAxis::Rows, at, count, false)
    }

    #[must_use]
    pub fn insert_columns(at: u32, count: u32) -> Self {
        Self::new(LineAxis::Columns, at, count, true)
    }

    #[must_use]
    pub fn remove_columns(at: u32, count: u32) -> Self {
        Self::new(LineAxis::Columns, at, count, false)
    }

    fn new(axis: LineAxis, at: u32, count: u32, insert: bool) -> Self {
        Self {
            axis,
            at: at.max(1),
            count,
            insert,
            table_column: "Column".to_string(),
        }
    }

    #[must_use]
    pub fn with_table_column(mut self, name: &str) -> Self {
        self.table_column = name.to_string();
        self
    }

    pub(crate) fn max(&self) -> u32 {
        match self.axis {
            LineAxis::Rows => MAX_ROWS,
            LineAxis::Columns => MAX_COLS,
        }
    }

    /// The new place of a row or column (`None`: removed, or pushed off the
    /// sheet).
    pub(crate) fn index(&self, v: u32) -> Option<u32> {
        if self.insert {
            if v < self.at {
                Some(v)
            } else {
                let n = u64::from(v) + u64::from(self.count);
                (n <= u64::from(self.max())).then_some(n as u32)
            }
        } else if v < self.at {
            Some(v)
        } else if u64::from(v) >= u64::from(self.at) + u64::from(self.count) {
            Some(v - self.count)
        } else {
            None
        }
    }

    /// The new span `lo..=hi` of rows or columns, as Excel moves the
    /// references to it: inserting inside widens it, removing narrows it;
    /// `None` when all of it is removed.
    pub(crate) fn span(&self, lo: u32, hi: u32) -> Option<(u32, u32)> {
        let (lo, hi) = (lo.min(hi), lo.max(hi));
        if self.insert {
            let shift = |v: u32| if v >= self.at { u64::from(v) + u64::from(self.count) } else { u64::from(v) };
            let max = u64::from(self.max());
            let (lo, hi) = (shift(lo), shift(hi).min(max));
            (lo <= hi).then_some((lo as u32, hi as u32))
        } else {
            let end = u64::from(self.at) + u64::from(self.count); // exclusive
            let (l, h) = (u64::from(lo), u64::from(hi));
            if l >= u64::from(self.at) && h < end {
                return None;
            }
            let lo2 = if l < u64::from(self.at) {
                l
            } else if l >= end {
                l - u64::from(self.count)
            } else {
                u64::from(self.at)
            };
            let hi2 = if h < u64::from(self.at) {
                h
            } else if h >= end {
                h - u64::from(self.count)
            } else {
                u64::from(self.at) - 1
            };
            Some((lo2 as u32, hi2.max(lo2) as u32))
        }
    }

    /// An area (`A1`, `A1:C5`, `A:C`, `1:3`, with or without `$`) moved;
    /// `None` when it is removed (or not an area).
    pub(crate) fn area(&self, area: &str) -> Option<String> {
        let (a, b) = area.split_once(':').unwrap_or((area, area));
        let (a, b) = (parse_point(a)?, parse_point(b)?);
        let (c0, r0) = (a.0.unwrap_or(1), a.1.unwrap_or(1));
        let (c1, r1) = (b.0.unwrap_or(MAX_COLS), b.1.unwrap_or(MAX_ROWS));
        let ((c0, c1), (r0, r1)) = match self.axis {
            LineAxis::Rows if a.1.is_some() => ((c0, c1), self.span(r0, r1)?),
            LineAxis::Columns if a.0.is_some() => (self.span(c0, c1)?, (r0, r1)),
            _ => ((c0, c1), (r0, r1)),
        };
        let point = |c: Option<u32>, r: Option<u32>| {
            format!("{}{}", c.map(string_from_column_index).unwrap_or_default(), r.map(|r| r.to_string()).unwrap_or_default())
        };
        let (pa, pb) = (point(a.0.map(|_| c0), a.1.map(|_| r0)), point(b.0.map(|_| c1), b.1.map(|_| r1)));
        Some(if pa == pb && !area.contains(':') { pa } else { format!("{pa}:{pb}") })
    }

    /// Areas separated by spaces (an `sqref`) moved; `None` when all of
    /// them are removed.
    pub(crate) fn areas(&self, sqref: &str) -> Option<String> {
        let list: Vec<String> = sqref.split_whitespace().filter_map(|a| self.area(a)).collect();
        (!list.is_empty()).then(|| list.join(" "))
    }
}

/// `B3` → (Some(2), Some(3)), `B` → (Some(2), None), `3` → (None, Some(3)).
fn parse_point(s: &str) -> Option<(Option<u32>, Option<u32>)> {
    let s = s.replace('$', "");
    let letters = s.chars().take_while(char::is_ascii_alphabetic).count();
    let (l, d) = s.split_at(letters);
    if (l.is_empty() && d.is_empty()) || !d.chars().all(|c| c.is_ascii_digit()) || letters > 3 {
        return None;
    }
    let col = (!l.is_empty()).then(|| column_index_from_string(l));
    let row = if d.is_empty() { None } else { Some(d.parse::<u32>().ok().filter(|r| *r >= 1)?) };
    Some((col, row))
}

/// The areas of the sheet edited rewritten in XML.
pub(crate) struct AreaXml<'a> {
    pub(crate) sheet: &'a str,
    pub(crate) edit:  &'a LineEdit,
    /// Whether the XML is the sheet's (its `sqref` and the `location` of its
    /// pivot tables are on it); otherwise only the sources of pivot caches
    /// that name the sheet move.
    pub(crate) own:   bool,
}

impl XmlRewrite for AreaXml<'_> {
    fn formula(&mut self, _formula: &str) -> Option<String> {
        None
    }

    fn area(&mut self, sheet: Option<&str>, area: &str, sqref: bool) -> Option<String> {
        match sheet {
            Some(s) if !s.eq_ignore_ascii_case(self.sheet) => return None,
            None if !self.own => return None,
            _ => {}
        }
        match self.edit.areas(area) {
            Some(new) => (new != area).then_some(new),
            // An sqref all removed is left empty (remove_emptied takes what
            // has it away); other areas are left as they are.
            None => sqref.then(String::new),
        }
    }
}

/// XML kept with the sparklines whose cells are all removed (an empty
/// `sqref`) taken away, and the groups left with none; `None` when there
/// are none.
pub(crate) fn remove_emptied(xml: &str) -> Option<String> {
    let empty = |x: &str| x.contains("<xm:sqref></xm:sqref>") || x.contains("<xm:sqref/>");
    if !empty(xml) {
        return None;
    }
    let mut out = xml.to_string();
    let mut changed = false;
    for (open, close) in [("<x14:sparkline>", "</x14:sparkline>")] {
        let mut from = 0;
        while let Some(i) = out[from..].find(open).map(|i| i + from) {
            let Some(j) = out[i..].find(close).map(|j| i + j + close.len()) else { break };
            if empty(&out[i..j]) {
                out.replace_range(i..j, "");
                changed = true;
            } else {
                from = j;
            }
        }
    }
    // Groups with no sparkline left.
    let mut from = 0;
    while let Some(i) = out[from..].find("<x14:sparklineGroup ").map(|i| i + from) {
        let close = "</x14:sparklineGroup>";
        let Some(j) = out[i..].find(close).map(|j| i + j + close.len()) else { break };
        if out[i..j].contains("<x14:sparklines></x14:sparklines>") || out[i..j].contains("<x14:sparklines/>") {
            out.replace_range(i..j, "");
            changed = true;
        } else {
            from = j;
        }
    }
    changed.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_move_as_in_excel() {
        let ins = LineEdit::insert_rows(3, 2);
        assert_eq!(ins.area("A1:B2").as_deref(), Some("A1:B2"));
        assert_eq!(ins.area("A2:B3").as_deref(), Some("A2:B5"));
        assert_eq!(ins.area("A3:B4").as_deref(), Some("A5:B6"));
        assert_eq!(ins.area("C7").as_deref(), Some("C9"));
        assert_eq!(ins.area("A:B").as_deref(), Some("A:B"));
        assert_eq!(ins.area("$2:$3").as_deref(), Some("2:5"));
        let del = LineEdit::remove_rows(3, 2);
        assert_eq!(del.area("A3:B4"), None);
        assert_eq!(del.area("A2:B5").as_deref(), Some("A2:B3"));
        assert_eq!(del.area("A4:B8").as_deref(), Some("A3:B6"));
        assert_eq!(del.area("A6").as_deref(), Some("A4"));
        assert_eq!(del.areas("A3 B1:B10 C4").as_deref(), Some("B1:B8"));
        let cols = LineEdit::remove_columns(2, 1);
        assert_eq!(cols.area("A1:C3").as_deref(), Some("A1:B3"));
        assert_eq!(cols.area("B:B"), None);
        assert_eq!(cols.area("1:1").as_deref(), Some("1:1"));
        assert_eq!(LineEdit::insert_columns(1, 1).area("XFD1"), None);
    }

    #[test]
    fn xml_areas_and_sources_move() {
        let edit = LineEdit::insert_rows(2, 1);
        let mut rw = AreaXml { sheet: "元", edit: &edit, own: true };
        let xml = r#"<x14:sparkline><xm:f>元!A1:A5</xm:f><xm:sqref>B1:B5</xm:sqref></x14:sparkline>"#;
        assert_eq!(
            String::from_utf8(rewrite_xml(xml.as_bytes(), &mut rw).unwrap()).unwrap(),
            r#"<x14:sparkline><xm:f>元!A1:A5</xm:f><xm:sqref>B1:B6</xm:sqref></x14:sparkline>"#
        );
        let xml = r#"<worksheetSource ref="A1:B5" sheet="元"/>"#;
        assert_eq!(
            String::from_utf8(rewrite_xml(xml.as_bytes(), &mut rw).unwrap()).unwrap(),
            r#"<worksheetSource ref="A1:B6" sheet="元"/>"#
        );
        assert!(rewrite_xml(br#"<worksheetSource ref="A1:B5" sheet="Other"/>"#, &mut rw).is_none());
    }
}
