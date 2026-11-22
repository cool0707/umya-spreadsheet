// filterColumn
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

/// What a column of a filter shows.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterCriteria {
    /// The rows whose cell shows one of the values (`filters`); `blank`:
    /// and those whose cell is empty.
    Values { values: Vec<String>, blank: bool },
    /// One or two comparisons (`customFilters`): the operator (`=`, `<>`,
    /// `>`, `>=`, `<`, `<=`) and what it compares with. `and`: both hold.
    Custom { tests: Vec<(String, String)>, and: bool },
    /// The largest (`top`) or smallest values (`top10`): `val` of them, or
    /// that percentage.
    Top10 { top: bool, percent: bool, val: f64 },
}

/// The operator of a `customFilter` as a comparison.
fn operator_symbol(operator: &str) -> &'static str {
    match operator {
        "lessThan" => "<",
        "lessThanOrEqual" => "<=",
        "notEqual" => "<>",
        "greaterThanOrEqual" => ">=",
        "greaterThan" => ">",
        _ => "=",
    }
}

fn operator_name(symbol: &str) -> Option<&'static str> {
    Some(match symbol {
        "<" => "lessThan",
        "<=" => "lessThanOrEqual",
        "<>" => "notEqual",
        ">=" => "greaterThanOrEqual",
        ">" => "greaterThan",
        // `equal` is what a customFilter without an operator is.
        _ => return None,
    })
}

/// The value of an attribute as written (`A&amp;B`): unescaped, since the
/// writer escapes it again.
fn unescaped(v: String) -> String {
    quick_xml::escape::unescape(&v).map_or(v.clone(), |u| u.to_string())
}

fn is_true(v: Option<String>) -> bool {
    matches!(v.as_deref(), Some("1" | "true"))
}

#[derive(Clone, Default, Debug)]
pub struct FilterColumn {
    column_id: UInt32Value,
    hidden_button: BooleanValue,
    /// `showButton="0"`: the button of the column is not shown (autofilters
    /// of sheets; tables say `hiddenButton`).
    show_button: BooleanValue,
    criteria: Option<FilterCriteria>,
    /// What is in the `filterColumn` as read, when it filters by something
    /// `criteria` cannot say (dates grouped by year, month, day: `dateGroupItem`;
    /// a colour, an icon, a dynamic filter such as "this month"): written back
    /// as it was until the criteria are set.
    kept: Option<Box<str>>,
}

impl FilterColumn {
    #[inline]
    #[must_use]
    pub fn column_id(&self) -> u32 {
        self.column_id.value()
    }

    /// The column, counted from the first of the filter (0).
    #[inline]
    pub fn set_column_id(&mut self, value: u32) -> &mut Self {
        self.column_id.set_value(value);
        self
    }

    #[inline]
    #[deprecated(note = "Use set_column_id()")]
    pub fn set_font(&mut self, value: u32) -> &mut Self {
        self.set_column_id(value)
    }

    #[inline]
    #[must_use]
    pub fn hidden_button(&self) -> bool {
        self.hidden_button.value()
    }

    #[inline]
    pub fn set_hidden_button(&mut self, value: bool) -> &mut Self {
        self.hidden_button.set_value(value);
        self
    }

    /// What the column shows: `None` when it filters nothing, or by
    /// something else (colours, icons, dates grouped...).
    #[inline]
    #[must_use]
    pub fn criteria(&self) -> Option<&FilterCriteria> {
        self.criteria.as_ref()
    }

    #[inline]
    pub fn set_criteria(&mut self, value: Option<FilterCriteria>) -> &mut Self {
        self.criteria = value;
        self.kept = None;
        self
    }

    /// Whether the column filters the rows: by `criteria`, or by something
    /// else the file has (dates grouped, a colour, an icon, a dynamic filter).
    #[inline]
    #[must_use]
    pub fn is_filtered(&self) -> bool {
        self.criteria.is_some() || self.kept.is_some()
    }

    /// Shows the rows whose cell shows one of `values` (and the empty
    /// ones, with `blank`).
    #[inline]
    pub fn set_values(&mut self, values: Vec<String>, blank: bool) -> &mut Self {
        self.set_criteria(Some(FilterCriteria::Values { values, blank }))
    }

    /// Shows the rows that pass one or two comparisons (operator, value).
    #[inline]
    pub fn set_custom(&mut self, tests: Vec<(String, String)>, and: bool) -> &mut Self {
        self.set_criteria(Some(FilterCriteria::Custom { tests, and }))
    }

    #[inline]
    pub fn set_top10(&mut self, top: bool, percent: bool, val: f64) -> &mut Self {
        self.set_criteria(Some(FilterCriteria::Top10 { top, percent, val }))
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        empty_flg: bool,
    ) {
        set_string_from_xml!(self, e, column_id, "colId");
        set_string_from_xml!(self, e, hidden_button, "hiddenButton");
        set_string_from_xml!(self, e, show_button, "showButton");
        if empty_flg {
            return;
        }
        let raw = crate::reader::driver::read_raw_element(reader, e, false);
        let mut reader = Reader::from_str(&raw);
        // Past the start tag of filterColumn itself.
        if reader.read_event().is_err() {
            return;
        }
        let mut known = true;
        let reader = &mut reader;
        xml_read_loop!(
            reader,
            Event::Empty(ref e) | Event::Start(ref e) => {
                match e.name().into_inner() {
                    b"filters" => {
                        self.criteria = Some(FilterCriteria::Values { values: Vec::new(), blank: is_true(get_attribute(e, b"blank")) });
                    }
                    b"filter" => {
                        if let (Some(FilterCriteria::Values { values, .. }), Some(v)) = (&mut self.criteria, get_attribute(e, b"val")) {
                            values.push(unescaped(v));
                        }
                    }
                    b"customFilters" => {
                        self.criteria = Some(FilterCriteria::Custom { tests: Vec::new(), and: is_true(get_attribute(e, b"and")) });
                    }
                    b"customFilter" => {
                        if let Some(FilterCriteria::Custom { tests, .. }) = &mut self.criteria {
                            let op = get_attribute(e, b"operator").unwrap_or_default();
                            tests.push((operator_symbol(&op).to_string(), unescaped(get_attribute(e, b"val").unwrap_or_default())));
                        }
                    }
                    b"top10" => {
                        let top = get_attribute(e, b"top").is_none_or(|v| v == "1" || v == "true");
                        self.criteria = Some(FilterCriteria::Top10 {
                            top,
                            percent: is_true(get_attribute(e, b"percent")),
                            val: get_attribute(e, b"val").and_then(|v| v.parse().ok()).unwrap_or(10.0),
                        });
                    }
                    _ => known = false,
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"filterColumn" {
                    break
                }
            },
            Event::Eof => break
        );
        // Kept only when what it holds needs no namespace of its own (an
        // extension's prefix would be written without its declaration).
        if !known {
            let start = raw.find('>').map_or(0, |i| i + 1);
            let end = raw.rfind("</").unwrap_or(raw.len()).max(start);
            let inner = &raw[start..end];
            let prefixed = inner.split('<').skip(1).any(|t| t.split([' ', '>', '/']).next().is_some_and(|n| n.contains(':')));
            if !prefixed {
                self.kept = Some(inner.into());
            }
        }
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.column_id.has_value() {
            attributes.push(("colId", self.column_id.value_string()).into());
        }
        if self.hidden_button.has_value() {
            attributes.push(("hiddenButton", self.hidden_button.value_string()).into());
        }
        if self.show_button.has_value() {
            attributes.push(("showButton", self.show_button.value_string()).into());
        }

        // filterColumn
        if let Some(kept) = &self.kept {
            write_start_tag(writer, "filterColumn", attributes, false);
            writer
                .write_event(Event::Text(quick_xml::events::BytesText::from_escaped(&**kept)))
                .unwrap();
            write_end_tag(writer, "filterColumn");
            return;
        }
        write_start_tag(writer, "filterColumn", attributes, self.criteria.is_none());
        let Some(criteria) = &self.criteria else { return };
        match criteria {
            FilterCriteria::Values { values, blank } => {
                let mut attributes: crate::structs::AttrCollection = Vec::new();
                if *blank {
                    attributes.push(("blank", "1").into());
                }
                write_start_tag(writer, "filters", attributes, values.is_empty());
                for v in values {
                    write_start_tag(writer, "filter", vec![("val", v.as_str()).into()], true);
                }
                if !values.is_empty() {
                    write_end_tag(writer, "filters");
                }
            }
            FilterCriteria::Custom { tests, and } => {
                let mut attributes: crate::structs::AttrCollection = Vec::new();
                if *and {
                    attributes.push(("and", "1").into());
                }
                write_start_tag(writer, "customFilters", attributes, tests.is_empty());
                for (op, val) in tests {
                    let mut attributes: crate::structs::AttrCollection = Vec::new();
                    if let Some(name) = operator_name(op) {
                        attributes.push(("operator", name).into());
                    }
                    attributes.push(("val", val.as_str()).into());
                    write_start_tag(writer, "customFilter", attributes, true);
                }
                if !tests.is_empty() {
                    write_end_tag(writer, "customFilters");
                }
            }
            FilterCriteria::Top10 { top, percent, val } => {
                let val = val.to_string();
                let mut attributes: crate::structs::AttrCollection = Vec::new();
                if !*top {
                    attributes.push(("top", "0").into());
                }
                if *percent {
                    attributes.push(("percent", "1").into());
                }
                attributes.push(("val", val.as_str()).into());
                write_start_tag(writer, "top10", attributes, true);
            }
        }
        write_end_tag(writer, "filterColumn");
    }
}
