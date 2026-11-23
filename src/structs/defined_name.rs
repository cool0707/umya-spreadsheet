use std::io::Cursor;

use quick_xml::{
    Reader,
    Writer,
    events::BytesStart,
};

use super::{
    Address,
    BooleanValue,
    StringValue,
    UInt32Value,
};
use crate::{
    helper::address::is_address,
    reader::driver::{
        get_attribute,
        set_string_from_xml,
    },
    traits::AdjustmentCoordinateWithSheet,
    writer::driver::{
        write_end_tag,
        write_start_tag,
        write_text_node_conversion,
    },
};

#[derive(Clone, Default, Debug)]
pub struct DefinedName {
    name:           StringValue,
    address:        Vec<Address>,
    string_value:   StringValue,
    local_sheet_id: UInt32Value,
    hidden:         BooleanValue,
    /// The other attributes (`comment`, `function`, `vbProcedure`...), as
    /// read or set (the accessors below), written back as they are.
    other_attributes: Vec<(Box<str>, Box<str>)>,
}
impl DefinedName {
    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use name()")]
    pub fn get_name(&self) -> &str {
        self.name()
    }

    #[inline]
    pub fn set_name<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.name.set_value(value);
        self
    }

    #[must_use]
    pub fn address(&self) -> String {
        if self.string_value.has_value() {
            return self.string_value.value_str().to_string();
        }
        let mut result: Vec<String> = Vec::with_capacity(self.address.len());
        for row in &self.address {
            result.push(row.address_ptn2());
        }
        result.join(",")
    }

    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use address()")]
    pub fn get_address(&self) -> String {
        self.address()
    }

    /// What the name refers to: areas (`Sheet1!$A$1:$B$5`, several separated
    /// by commas) or any formula or constant. Takes the place of what it
    /// referred to (it was added to it).
    pub fn set_address<S: Into<String>>(&mut self, value: S) -> &mut Self {
        let value = value.into();
        self.address.clear();
        self.string_value.remove_value();
        let list = Self::split_str(value.as_str());
        // A reference to a sheet removed (`#REF!$A$1`) is not an area.
        if !list.is_empty() && list.iter().all(|v| is_address(v) && !v.starts_with('#')) {
            for v in &list {
                self.add_address(v);
            }
        } else {
            self.set_string_value(value);
        }
        self
    }

    pub fn add_address<S: Into<String>>(&mut self, value: S) -> &mut Self {
        let mut obj = Address::default();
        obj.set_address(value.into().replace("''", "'"));
        self.address.push(obj);
        self
    }

    #[inline]
    pub(crate) fn address_obj(&self) -> &[Address] {
        &self.address
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use address_obj()")]
    pub(crate) fn get_address_obj(&self) -> &[Address] {
        self.address_obj()
    }

    #[inline]
    pub(crate) fn set_string_value<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.address.clear();
        self.string_value.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn has_local_sheet_id(&self) -> bool {
        self.local_sheet_id.has_value()
    }

    #[inline]
    #[must_use]
    pub fn local_sheet_id(&self) -> u32 {
        self.local_sheet_id.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use local_sheet_id()")]
    pub fn get_local_sheet_id(&self) -> u32 {
        self.local_sheet_id()
    }

    #[inline]
    pub fn set_local_sheet_id(&mut self, value: u32) {
        self.local_sheet_id.set_value(value);
    }

    #[inline]
    #[must_use]
    pub fn hidden(&self) -> bool {
        self.hidden.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use hidden()")]
    pub fn get_hidden(&self) -> bool {
        self.hidden()
    }

    #[inline]
    pub fn set_hidden(&mut self, value: bool) {
        self.hidden.set_value(value);
    }

    /// Makes the name the workbook's (no sheet of its own).
    #[inline]
    pub fn remove_local_sheet_id(&mut self) -> &mut Self {
        self.local_sheet_id.remove_value();
        self
    }

    /// An attribute of the name as the file has it (`comment`, `function`...);
    /// None when it has none.
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.other_attributes.iter().find(|(k, _)| &**k == name).map(|(_, v)| &**v)
    }

    /// Sets (`Some`) or removes (`None`) an attribute of the name other than
    /// `name`, `localSheetId` and `hidden`.
    pub fn set_attribute(&mut self, name: &str, value: Option<&str>) -> &mut Self {
        match self.other_attributes.iter_mut().find(|(k, _)| &**k == name) {
            Some(entry) => match value {
                Some(v) => entry.1 = v.into(),
                None => self.other_attributes.retain(|(k, _)| &**k != name),
            },
            None => {
                if let Some(v) = value {
                    self.other_attributes.push((name.into(), v.into()));
                }
            }
        }
        self
    }

    fn flag(&self, name: &str) -> bool {
        matches!(self.attribute(name), Some("1" | "true"))
    }

    fn set_flag(&mut self, name: &str, value: bool) -> &mut Self {
        self.set_attribute(name, value.then_some("1"))
    }

    /// The comment of the name (Excel's Name Manager).
    #[must_use]
    pub fn comment(&self) -> Option<&str> {
        self.attribute("comment")
    }

    pub fn set_comment(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("comment", value.filter(|v| !v.is_empty()))
    }

    /// Whether the name is of a function or command (a macro of Excel 4.0
    /// or VBA registered as a name: `function`).
    #[must_use]
    pub fn function(&self) -> bool {
        self.flag("function")
    }

    pub fn set_function(&mut self, value: bool) -> &mut Self {
        self.set_flag("function", value)
    }

    /// Whether the function or command is a procedure of VBA (`vbProcedure`).
    #[must_use]
    pub fn vb_procedure(&self) -> bool {
        self.flag("vbProcedure")
    }

    pub fn set_vb_procedure(&mut self, value: bool) -> &mut Self {
        self.set_flag("vbProcedure", value)
    }

    /// Whether the name is a macro of Excel 4.0 (XLM: `xlm`).
    #[must_use]
    pub fn xlm(&self) -> bool {
        self.flag("xlm")
    }

    pub fn set_xlm(&mut self, value: bool) -> &mut Self {
        self.set_flag("xlm", value)
    }

    /// The category of the function in Insert Function (`functionGroupId`:
    /// 14 is "User Defined").
    #[must_use]
    pub fn function_group_id(&self) -> Option<u32> {
        self.attribute("functionGroupId").and_then(|v| v.parse().ok())
    }

    pub fn set_function_group_id(&mut self, value: Option<u32>) -> &mut Self {
        let value = value.map(|v| v.to_string());
        self.set_attribute("functionGroupId", value.as_deref())
    }

    /// The keyboard shortcut of the command (`shortcutKey`: the letter of
    /// Ctrl+letter).
    #[must_use]
    pub fn shortcut_key(&self) -> Option<&str> {
        self.attribute("shortcutKey")
    }

    pub fn set_shortcut_key(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("shortcutKey", value)
    }

    /// The description of the function in Insert Function (`description`).
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.attribute("description")
    }

    pub fn set_description(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("description", value)
    }

    /// The help topic of the function (`help`).
    #[must_use]
    pub fn help(&self) -> Option<&str> {
        self.attribute("help")
    }

    pub fn set_help(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("help", value)
    }

    /// The text of the status bar for the command (`statusBar`).
    #[must_use]
    pub fn status_bar(&self) -> Option<&str> {
        self.attribute("statusBar")
    }

    pub fn set_status_bar(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("statusBar", value)
    }

    /// The text of the menu of the command (`customMenu`).
    #[must_use]
    pub fn custom_menu(&self) -> Option<&str> {
        self.attribute("customMenu")
    }

    pub fn set_custom_menu(&mut self, value: Option<&str>) -> &mut Self {
        self.set_attribute("customMenu", value)
    }

    /// Whether the name is published to a server with the workbook (Excel
    /// Services: `publishToServer`).
    #[must_use]
    pub fn publish_to_server(&self) -> bool {
        self.flag("publishToServer")
    }

    pub fn set_publish_to_server(&mut self, value: bool) -> &mut Self {
        self.set_flag("publishToServer", value)
    }

    /// Whether the name is a parameter of the workbook on a server (Excel
    /// Services: `workbookParameter`).
    #[must_use]
    pub fn workbook_parameter(&self) -> bool {
        self.flag("workbookParameter")
    }

    pub fn set_workbook_parameter(&mut self, value: bool) -> &mut Self {
        self.set_flag("workbookParameter", value)
    }

    fn split_str<S: Into<String>>(value: S) -> Vec<String> {
        let value = value.into();
        let char_list = value.chars();
        let mut is_pass_s = false;
        let mut is_pass_d = false;
        let mut is_pass_b = 0;
        let mut result: Vec<String> = Vec::new();
        let mut string = String::new();
        for c in char_list {
            match c {
                '(' => {
                    is_pass_b += 1;
                    string.push(c);
                }
                ')' => {
                    is_pass_b -= 1;
                    string.push(c);
                }
                '\'' => {
                    is_pass_s = !is_pass_s;
                    string.push(c);
                }
                '"' => {
                    is_pass_d = !is_pass_d;
                    if is_pass_s || is_pass_b != 0 {
                        string.push(c);
                    }
                }
                ',' => {
                    if !is_pass_s && !is_pass_d && is_pass_b == 0 {
                        result.push(std::mem::take(&mut string));
                    } else {
                        string.push(c);
                    }
                }
                _ => {
                    string.push(c);
                }
            }
        }
        if !string.is_empty() {
            result.push(string);
        }
        result
    }

    /// Renames a sheet in what the name refers to; `own` is whether the
    /// name is the sheet's, whose addresses with no sheet are then the
    /// sheet's.
    pub(crate) fn rename_sheet(&mut self, rename: &crate::helper::sheet_rename::SheetRename, own: bool) {
        if self.string_value.has_value() {
            if let Some(new) = rename.formula(self.string_value.value_str()) {
                self.string_value.set_value(new);
            }
            return;
        }
        for address in &mut self.address {
            if rename.matches(address.sheet_name()) || (own && address.sheet_name().is_empty()) {
                address.set_sheet_name(rename.new_name());
            }
        }
    }

    /// Rewrites what the name refers to; `host` is the sheet of the name's
    /// addresses with no sheet.
    pub(crate) fn rewrite_formulas(&mut self, host: Option<&str>, f: &mut crate::helper::rewrite::FormulaFn<'_>) {
        let text = self.address();
        if text.is_empty() {
            return;
        }
        if let Some(new) = f(host, &text) {
            // As it is read from a file.
            self.address.clear();
            self.string_value.remove_value();
            self.set_address(new);
        }
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
    ) {
        set_string_from_xml!(self, e, name, "name");
        set_string_from_xml!(self, e, local_sheet_id, "localSheetId");
        set_string_from_xml!(self, e, hidden, "hidden");
        for a in e.attributes().with_checks(false).flatten() {
            let key = a.key.into_inner();
            if matches!(key, b"name" | b"localSheetId" | b"hidden") || key.contains(&b':') {
                continue;
            }
            let value = crate::reader::driver::unescaped_attribute_value(&a);
            self.other_attributes.push((String::from_utf8_lossy(key).into(), value.into()));
        }

        let mut buf = Vec::new();
        let text = reader
                .read_text_into(e.name(), &mut buf).unwrap();
        self.set_address(crate::helper::utils::unescape_xml_text(&text));
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        // definedName
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        attributes.push(("name", self.name()).into());
        let local_sheet_id_str = self.local_sheet_id.value_string();
        if self.local_sheet_id.has_value() {
            attributes.push(("localSheetId", &local_sheet_id_str).into());
        }
        let hidden_str = self.hidden.value_string();
        if self.hidden.has_value() {
            attributes.push(("hidden", hidden_str).into());
        }
        for (k, v) in &self.other_attributes {
            attributes.push((&**k, &**v).into());
        }
        write_start_tag(writer, "definedName", attributes, false);
        write_text_node_conversion(writer, self.address());
        write_end_tag(writer, "definedName");
    }
}
impl AdjustmentCoordinateWithSheet for DefinedName {
    fn adjustment_insert_coordinate_with_sheet(
        &mut self,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        for address in &mut self.address {
            address.adjustment_insert_coordinate_with_sheet(
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
        }
    }

    fn adjustment_remove_coordinate_with_sheet(
        &mut self,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        self.address.retain(|x| {
            !(x.is_remove_coordinate_with_sheet(
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            ))
        });
        for address in &mut self.address {
            address.adjustment_remove_coordinate_with_sheet(
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
        }
    }

    #[inline]
    #[allow(unused_variables)]
    fn is_remove_coordinate_with_sheet(
        &self,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) -> bool {
        if self.string_value.has_value() {
            return false;
        }
        if self.address.is_empty() {
            return true;
        }
        false
    }
}
