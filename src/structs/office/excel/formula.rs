// xm:f
use std::{
    io::Cursor,
    vec,
};

use quick_xml::{
    Reader,
    Writer,
    events::BytesStart,
};

use crate::{
    structs::Address,
    writer::driver::{
        write_end_tag,
        write_start_tag,
        write_text_node,
    },
};

#[derive(Default, Debug, Clone)]
pub struct Formula {
    value: Address,
    /// The formula as read (`Sheet1!$A$1:$A$5`, `OFFSET(Sheet1!$A$1,0,0,5)`,
    /// a name) and what `value` was like then: written while `value` stays so.
    text:  Option<(String, String)>,
}
impl Formula {
    #[inline]
    #[must_use]
    pub fn value(&self) -> &Address {
        &self.value
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use val()")]
    pub fn get_value(&self) -> &Address {
        self.value()
    }

    #[inline]
    pub fn value_mut(&mut self) -> &mut Address {
        &mut self.value
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use val()")]
    pub fn get_value_mut(&mut self) -> &mut Address {
        self.value_mut()
    }

    #[inline]
    pub fn set_value(&mut self, value: Address) -> &mut Self {
        self.value = value;
        self.text = None;
        self
    }

    /// The formula as it is written.
    #[must_use]
    pub fn formula(&self) -> String {
        if let Some((text, value)) = &self.text {
            if *value == format!("{:?}", self.value) {
                return text.clone();
            }
        }
        let range = self.value.range().range();
        let sheet = self.value.sheet_name();
        if sheet.is_empty() {
            range
        } else {
            format!("{}!{range}", crate::helper::sheet_rename::quote_sheet_name(sheet))
        }
    }

    pub(crate) fn rename_sheet(&mut self, rename: &crate::helper::sheet_rename::SheetRename) {
        if let Some(new) = rename.formula(&self.formula()) {
            if rename.matches(self.value.sheet_name()) {
                self.value.set_sheet_name(rename.new_name());
            }
            self.text = Some((new, format!("{:?}", self.value)));
        }
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        let mut buf = Vec::new();
        let text = reader.read_text_into(e.name(), &mut buf).unwrap();
        // Written back as read (an address is told from a formula only
        // roughly: `C1>Sheet1!$A$1` would be taken for one on the sheet
        // `C1>Sheet1`).
        let text = crate::helper::utils::unescape_xml_text(&text);
        let mut obj = Address::default();
        if crate::helper::address::is_address(&text) {
            obj.set_address(&text);
            let sheet = obj.sheet_name().replace("''", "'");
            obj.set_sheet_name(sheet);
        }
        self.text = Some((text, format!("{obj:?}")));
        self.value = obj;
    }

    #[inline]
    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        write_start_tag(writer, "xm:f", vec![], false);
        write_text_node(writer, self.formula());
        write_end_tag(writer, "xm:f");
    }
}
