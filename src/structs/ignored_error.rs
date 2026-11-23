//! The errors not flagged in cells (`ignoredErrors/ignoredError`): Excel's
//! "Ignore Error" of the error checking (the green triangle in the corner of
//! a cell), for each kind of error.

use std::io::Cursor;

use quick_xml::{
    Writer,
    events::BytesStart,
};

use crate::writer::driver::{
    write_end_tag,
    write_start_tag,
};

/// A kind of error the error checking flags (an attribute of
/// `ignoredError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IgnoredErrorKind {
    /// A formula that results in an error (`evalError`).
    EvalError,
    /// A date as text with a two-digit year (`twoDigitTextYear`).
    TwoDigitTextYear,
    /// A number stored as text (`numberStoredAsText`).
    NumberStoredAsText,
    /// A formula unlike those around it (`formula`).
    Formula,
    /// A formula that leaves out cells next to its range (`formulaRange`).
    FormulaRange,
    /// A formula in a cell that is not locked (`unlockedFormula`).
    UnlockedFormula,
    /// A formula that refers to empty cells (`emptyCellReference`).
    EmptyCellReference,
    /// A value a data validation does not allow (`listDataValidation`).
    ListDataValidation,
    /// A formula unlike the rest of its table column (`calculatedColumn`).
    CalculatedColumn,
}

impl IgnoredErrorKind {
    pub const ALL: [Self; 9] = [
        Self::EvalError,
        Self::TwoDigitTextYear,
        Self::NumberStoredAsText,
        Self::Formula,
        Self::FormulaRange,
        Self::UnlockedFormula,
        Self::EmptyCellReference,
        Self::ListDataValidation,
        Self::CalculatedColumn,
    ];

    /// The attribute of `ignoredError` (`numberStoredAsText`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EvalError => "evalError",
            Self::TwoDigitTextYear => "twoDigitTextYear",
            Self::NumberStoredAsText => "numberStoredAsText",
            Self::Formula => "formula",
            Self::FormulaRange => "formulaRange",
            Self::UnlockedFormula => "unlockedFormula",
            Self::EmptyCellReference => "emptyCellReference",
            Self::ListDataValidation => "listDataValidation",
            Self::CalculatedColumn => "calculatedColumn",
        }
    }

    /// From the attribute (`numberStoredAsText`).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == value)
    }
}

/// Cells whose errors of some kinds are not flagged.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IgnoredError {
    /// The cells (`A1 B2:C5`).
    sqref: String,
    kinds: Vec<IgnoredErrorKind>,
}

impl IgnoredError {
    /// The errors `kinds` not flagged in the cells `sqref` (`A1 B2:C5`).
    #[must_use]
    pub fn new(sqref: &str, kinds: &[IgnoredErrorKind]) -> Self {
        let mut out = Self {
            sqref: sqref.to_string(),
            kinds: Vec::new(),
        };
        for kind in kinds {
            out.set_ignored(*kind, true);
        }
        out
    }

    /// The cells (`A1 B2:C5`).
    #[must_use]
    pub fn sqref(&self) -> &str {
        &self.sqref
    }

    pub fn set_sqref<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.sqref = value.into();
        self
    }

    /// The kinds of errors not flagged, in the order of the schema.
    #[must_use]
    pub fn kinds(&self) -> &[IgnoredErrorKind] {
        &self.kinds
    }

    #[must_use]
    pub fn is_ignored(&self, kind: IgnoredErrorKind) -> bool {
        self.kinds.contains(&kind)
    }

    pub fn set_ignored(&mut self, kind: IgnoredErrorKind, value: bool) -> &mut Self {
        self.kinds.retain(|k| *k != kind);
        if value {
            self.kinds.push(kind);
            self.kinds.sort_by_key(|k| IgnoredErrorKind::ALL.iter().position(|a| a == k));
        }
        self
    }

    pub(crate) fn read(e: &BytesStart<'_>) -> Self {
        let mut out = Self::default();
        for a in e.attributes().with_checks(false).flatten() {
            let key = String::from_utf8_lossy(a.key.local_name().as_ref()).to_string();
            let value = crate::reader::driver::unescaped_attribute_value(&a);
            if key == "sqref" {
                out.sqref = value;
            } else if let Some(kind) = IgnoredErrorKind::parse(&key) {
                out.set_ignored(kind, matches!(value.as_str(), "1" | "true"));
            }
        }
        out
    }

    /// Writes `ignoredErrors` (nothing when there are none, or none of them
    /// has cells and errors).
    pub(crate) fn write_list(list: &[IgnoredError], writer: &mut Writer<Cursor<Vec<u8>>>) {
        let list: Vec<&IgnoredError> = list
            .iter()
            .filter(|e| !e.sqref.trim().is_empty() && !e.kinds.is_empty())
            .collect();
        if list.is_empty() {
            return;
        }
        write_start_tag(writer, "ignoredErrors", vec![], false);
        for e in list {
            let mut attributes: crate::structs::AttrCollection = vec![("sqref", e.sqref.as_str()).into()];
            for kind in &e.kinds {
                attributes.push((kind.as_str(), "1").into());
            }
            write_start_tag(writer, "ignoredError", attributes, true);
        }
        write_end_tag(writer, "ignoredErrors");
    }
}
