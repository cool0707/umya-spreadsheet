//! The table styles a workbook defines (`tableStyles` in `styles.xml`), as
//! the workbook's own tables and pivot tables can use them.
//!
//! They are kept as read (the stylesheet writes them back as they were, see
//! `Stylesheet`); this is what can be read of them.

use quick_xml::{
    Reader,
    events::Event,
};

use crate::reader::driver::get_attribute;

/// A table style the workbook defines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomTableStyle {
    name:     String,
    pivot:    bool,
    table:    bool,
    elements: Vec<CustomTableStyleElement>,
}

/// One part of a table style (`tableStyleElement`): what it formats, how
/// many rows or columns a stripe of it spans, and its differential format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CustomTableStyleElement {
    kind:   String,
    size:   u32,
    dxf_id: Option<u32>,
}

impl CustomTableStyle {
    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether pivot tables can use it.
    #[inline]
    #[must_use]
    pub fn pivot(&self) -> bool {
        self.pivot
    }

    /// Whether tables can use it.
    #[inline]
    #[must_use]
    pub fn table(&self) -> bool {
        self.table
    }

    #[inline]
    #[must_use]
    pub fn elements(&self) -> &[CustomTableStyleElement] {
        &self.elements
    }
}

impl CustomTableStyleElement {
    /// What it formats: `wholeTable`, `headerRow`, `totalRow`, `firstColumn`,
    /// `lastColumn`, `firstRowStripe`, `secondRowStripe`, `firstColumnStripe`,
    /// `secondColumnStripe`, `firstHeaderCell`, ... (`ST_TableStyleType`).
    #[inline]
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The rows or columns of a stripe (1 when the file does not say).
    #[inline]
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The differential format (`Workbook::differential_format`).
    #[inline]
    #[must_use]
    pub fn dxf_id(&self) -> Option<u32> {
        self.dxf_id
    }
}

/// The default styles of tables and pivot tables, and the styles of
/// `tableStyles` (its XML as kept).
pub(crate) fn read(xml: &str) -> (Option<String>, Option<String>, Vec<CustomTableStyle>) {
    let mut reader = Reader::from_str(xml);
    let mut styles = Vec::new();
    let (mut default_table, mut default_pivot) = (None, None);
    let mut current: Option<CustomTableStyle> = None;
    loop {
        let (e, empty) = match reader.read_event() {
            Ok(Event::Start(e)) => (e, false),
            Ok(Event::Empty(e)) => (e, true),
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"tableStyle" {
                    styles.extend(current.take());
                }
                continue;
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => continue,
        };
        match e.local_name().as_ref() {
            b"tableStyles" => {
                default_table = get_attribute(&e, b"defaultTableStyle");
                default_pivot = get_attribute(&e, b"defaultPivotStyle");
            }
            b"tableStyle" => {
                let on = |name: &[u8]| !matches!(get_attribute(&e, name).as_deref(), Some("0" | "false"));
                let style = CustomTableStyle {
                    name:     get_attribute(&e, b"name").unwrap_or_default(),
                    pivot:    on(b"pivot"),
                    table:    on(b"table"),
                    elements: Vec::new(),
                };
                if empty {
                    styles.push(style);
                } else {
                    current = Some(style);
                }
            }
            b"tableStyleElement" => {
                if let Some(style) = &mut current {
                    style.elements.push(CustomTableStyleElement {
                        kind:   get_attribute(&e, b"type").unwrap_or_default(),
                        size:   get_attribute(&e, b"size").and_then(|v| v.parse().ok()).unwrap_or(1),
                        dxf_id: get_attribute(&e, b"dxfId").and_then(|v| v.parse().ok()),
                    });
                }
            }
            _ => {}
        }
    }
    (default_table, default_pivot, styles)
}
