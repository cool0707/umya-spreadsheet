// xdr:clientData
use quick_xml::events::BytesStart;

use crate::reader::driver::get_attribute;

/// What an anchor of a drawing says about its object besides where it is
/// (`xdr:clientData`): whether it is printed with the sheet and locked with
/// it. None where the file does not say (it is then printed and locked).
#[derive(Clone, Default, Debug)]
pub struct ClientData {
    locks_with_sheet:  Option<bool>,
    prints_with_sheet: Option<bool>,
}

impl ClientData {
    /// Whether the object is printed with the sheet (false: "印刷しない",
    /// "Print object" off).
    #[inline]
    #[must_use]
    pub fn prints_with_sheet(&self) -> bool {
        self.prints_with_sheet.unwrap_or(true)
    }

    #[inline]
    pub fn set_prints_with_sheet(&mut self, value: bool) -> &mut Self {
        self.prints_with_sheet = (!value).then_some(false);
        self
    }

    /// Whether the object is locked when the sheet is protected.
    #[inline]
    #[must_use]
    pub fn locks_with_sheet(&self) -> bool {
        self.locks_with_sheet.unwrap_or(true)
    }

    #[inline]
    pub fn set_locks_with_sheet(&mut self, value: bool) -> &mut Self {
        self.locks_with_sheet = (!value).then_some(false);
        self
    }

    pub(crate) fn set_attributes(&mut self, e: &BytesStart) {
        let flag = |name: &[u8]| get_attribute(e, name).map(|v| !matches!(v.as_str(), "0" | "false"));
        self.locks_with_sheet = flag(b"fLocksWithSheet");
        self.prints_with_sheet = flag(b"fPrintsWithSheet");
    }

    pub(crate) fn attributes(&self) -> crate::structs::AttrCollection<'static> {
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        let text = |v: bool| if v { "1" } else { "0" };
        if let Some(v) = self.locks_with_sheet {
            attributes.push(("fLocksWithSheet", text(v)).into());
        }
        if let Some(v) = self.prints_with_sheet {
            attributes.push(("fPrintsWithSheet", text(v)).into());
        }
        attributes
    }
}
