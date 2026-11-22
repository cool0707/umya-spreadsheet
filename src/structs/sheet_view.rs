// sheetView
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
    EnumValue,
    Pane,
    Selection,
    SheetViewValues,
    StringValue,
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

#[derive(Clone, Default, Debug)]
pub struct SheetView {
    show_grid_lines:              BooleanValue,
    tab_selected:                 BooleanValue,
    workbook_view_id:             UInt32Value,
    pane:                         Option<Box<Pane>>,
    view:                         EnumValue<SheetViewValues>,
    zoom_scale:                   UInt32Value,
    zoom_scale_normal:            UInt32Value,
    zoom_scale_page_layout_view:  UInt32Value,
    zoom_scale_sheet_layout_view: UInt32Value,
    top_left_cell:                StringValue,
    selection:                    Vec<Selection>,
    show_formulas:                BooleanValue,
    show_row_col_headers:         BooleanValue,
    show_zeros:                   BooleanValue,
    right_to_left:                BooleanValue,
    show_outline_symbols:         BooleanValue,
    default_grid_color:           BooleanValue,
    color_id:                     UInt32Value,
    /// The attributes not modelled above (`windowProtection`, `showRuler`,
    /// `showWhiteSpace`, `zoomToFit`...), as read.
    other_attributes:             Vec<(Box<str>, Box<str>)>,
}

impl SheetView {
    #[inline]
    #[must_use]
    pub fn show_grid_lines(&self) -> bool {
        // Absent means shown.
        !self.show_grid_lines.has_value() || self.show_grid_lines.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use show_grid_lines()")]
    pub fn get_show_grid_lines(&self) -> bool {
        self.show_grid_lines()
    }

    #[inline]
    pub fn set_show_grid_lines(&mut self, value: bool) -> &mut Self {
        self.show_grid_lines.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn tab_selected(&self) -> bool {
        self.tab_selected.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use tab_selected()")]
    pub fn get_tab_selected(&self) -> bool {
        self.tab_selected()
    }

    #[inline]
    pub fn set_tab_selected(&mut self, value: bool) -> &mut Self {
        self.tab_selected.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn workbook_view_id(&self) -> u32 {
        self.workbook_view_id.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use workbook_view_id()")]
    pub fn get_workbook_view_id(&self) -> u32 {
        self.workbook_view_id()
    }

    #[inline]
    pub fn set_workbook_view_id(&mut self, value: u32) -> &mut Self {
        self.workbook_view_id.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn pane(&self) -> Option<&Pane> {
        self.pane.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use pane()")]
    pub fn get_pane(&self) -> Option<&Pane> {
        self.pane()
    }

    #[inline]
    pub fn pane_mut(&mut self) -> Option<&mut Pane> {
        self.pane.as_deref_mut()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use pane_mut()")]
    pub fn get_pane_mut(&mut self) -> Option<&mut Pane> {
        self.pane_mut()
    }

    #[inline]
    pub fn set_pane(&mut self, value: Pane) -> &mut Self {
        self.pane = Some(Box::new(value));
        self
    }

    #[inline]
    #[must_use]
    pub fn view(&self) -> &SheetViewValues {
        self.view.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use view()")]
    pub fn get_view(&self) -> &SheetViewValues {
        self.view()
    }

    #[inline]
    pub fn set_view(&mut self, value: SheetViewValues) -> &mut Self {
        self.view.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn zoom_scale(&self) -> u32 {
        self.zoom_scale.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use zoom_scale()")]
    pub fn get_zoom_scale(&self) -> u32 {
        self.zoom_scale()
    }

    #[inline]
    pub fn set_zoom_scale(&mut self, value: u32) -> &mut Self {
        self.zoom_scale.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn zoom_scale_normal(&self) -> u32 {
        self.zoom_scale_normal.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use zoom_scale_normal()")]
    pub fn get_zoom_scale_normal(&self) -> u32 {
        self.zoom_scale_normal()
    }

    #[inline]
    pub fn set_zoom_scale_normal(&mut self, value: u32) -> &mut Self {
        self.zoom_scale_normal.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn zoom_scale_page_layout_view(&self) -> u32 {
        self.zoom_scale_page_layout_view.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use zoom_scale_page_layout_view()")]
    pub fn get_zoom_scale_page_layout_view(&self) -> u32 {
        self.zoom_scale_page_layout_view()
    }

    #[inline]
    pub fn set_zoom_scale_page_layout_view(&mut self, value: u32) -> &mut Self {
        self.zoom_scale_page_layout_view.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn zoom_scale_sheet_layout_view(&self) -> u32 {
        self.zoom_scale_sheet_layout_view.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use zoom_scale_sheet_layout_view()")]
    pub fn get_zoom_scale_sheet_layout_view(&self) -> u32 {
        self.zoom_scale_sheet_layout_view()
    }

    #[inline]
    pub fn set_zoom_scale_sheet_layout_view(&mut self, value: u32) -> &mut Self {
        self.zoom_scale_sheet_layout_view.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn top_left_cell(&self) -> &str {
        self.top_left_cell.value_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use top_left_cell()")]
    pub fn get_top_left_cell(&self) -> &str {
        self.top_left_cell()
    }

    #[inline]
    pub fn set_top_left_cell<S: Into<String>>(&mut self, value: S) -> &mut Self {
        self.top_left_cell.set_value(value);
        self
    }

    #[inline]
    #[must_use]
    pub fn selection(&self) -> &[Selection] {
        &self.selection
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use selection()")]
    pub fn get_selection(&self) -> &[Selection] {
        self.selection()
    }

    #[inline]
    pub fn selection_mut(&mut self) -> &mut Vec<Selection> {
        &mut self.selection
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use selection_mut()")]
    pub fn get_selection_mut(&mut self) -> &mut Vec<Selection> {
        self.selection_mut()
    }

    #[inline]
    pub fn set_selection(&mut self, value: Selection) -> &mut Self {
        self.selection.push(value);
        self
    }

    /// Whether the cells show their formulas instead of their values.
    #[inline]
    #[must_use]
    pub fn show_formulas(&self) -> bool {
        self.show_formulas.value()
    }

    #[inline]
    pub fn set_show_formulas(&mut self, value: bool) -> &mut Self {
        self.show_formulas.set_value(value);
        self
    }

    /// Whether the row and column headings are shown.
    #[inline]
    #[must_use]
    pub fn show_row_col_headers(&self) -> bool {
        !self.show_row_col_headers.has_value() || self.show_row_col_headers.value()
    }

    #[inline]
    pub fn set_show_row_col_headers(&mut self, value: bool) -> &mut Self {
        self.show_row_col_headers.set_value(value);
        self
    }

    /// Whether cells whose value is zero show it (false: they look empty).
    #[inline]
    #[must_use]
    pub fn show_zeros(&self) -> bool {
        !self.show_zeros.has_value() || self.show_zeros.value()
    }

    #[inline]
    pub fn set_show_zeros(&mut self, value: bool) -> &mut Self {
        self.show_zeros.set_value(value);
        self
    }

    /// Whether the sheet is shown from right to left (column A on the right).
    #[inline]
    #[must_use]
    pub fn right_to_left(&self) -> bool {
        self.right_to_left.value()
    }

    #[inline]
    pub fn set_right_to_left(&mut self, value: bool) -> &mut Self {
        self.right_to_left.set_value(value);
        self
    }

    /// Whether the outline symbols of grouped rows and columns are shown.
    #[inline]
    #[must_use]
    pub fn show_outline_symbols(&self) -> bool {
        !self.show_outline_symbols.has_value() || self.show_outline_symbols.value()
    }

    #[inline]
    pub fn set_show_outline_symbols(&mut self, value: bool) -> &mut Self {
        self.show_outline_symbols.set_value(value);
        self
    }

    /// The color of the gridlines: None for the automatic one, or the index
    /// in the palette (`defaultGridColor="0"` with `colorId`).
    #[inline]
    #[must_use]
    pub fn grid_color_index(&self) -> Option<u32> {
        let automatic = !self.default_grid_color.has_value() || self.default_grid_color.value();
        (!automatic).then(|| if self.color_id.has_value() { self.color_id.value() } else { 64 })
    }

    #[inline]
    pub fn set_grid_color_index(&mut self, value: Option<u32>) -> &mut Self {
        match value {
            Some(index) => {
                self.default_grid_color.set_value(false);
                self.color_id.set_value(index);
            }
            None => {
                self.default_grid_color = BooleanValue::default();
                self.color_id = UInt32Value::default();
            }
        }
        self
    }

    /// Removes the pane (frozen or split rows and columns).
    #[inline]
    pub fn remove_pane(&mut self) -> &mut Self {
        self.pane = None;
        self
    }

    pub(crate) fn set_attributes<R: std::io::BufRead>(
        &mut self,
        reader: &mut Reader<R>,
        e: &BytesStart,
        empty_flag: bool,
    ) {
        set_string_from_xml!(self, e, show_grid_lines, "showGridLines");
        set_string_from_xml!(self, e, tab_selected, "tabSelected");
        set_string_from_xml!(self, e, workbook_view_id, "workbookViewId");
        set_string_from_xml!(self, e, view, "view");
        set_string_from_xml!(self, e, zoom_scale, "zoomScale");
        set_string_from_xml!(self, e, zoom_scale_normal, "zoomScaleNormal");
        set_string_from_xml!(
            self,
            e,
            zoom_scale_page_layout_view,
            "zoomScalePageLayoutView"
        );
        set_string_from_xml!(
            self,
            e,
            zoom_scale_sheet_layout_view,
            "zoomScaleSheetLayoutView"
        );
        set_string_from_xml!(self, e, top_left_cell, "topLeftCell");
        set_string_from_xml!(self, e, show_formulas, "showFormulas");
        set_string_from_xml!(self, e, show_row_col_headers, "showRowColHeaders");
        set_string_from_xml!(self, e, show_zeros, "showZeros");
        set_string_from_xml!(self, e, right_to_left, "rightToLeft");
        set_string_from_xml!(self, e, show_outline_symbols, "showOutlineSymbols");
        set_string_from_xml!(self, e, default_grid_color, "defaultGridColor");
        set_string_from_xml!(self, e, color_id, "colorId");
        for attr in e.attributes().with_checks(false).flatten() {
            let key = attr.key.into_inner();
            if matches!(
                key,
                b"windowProtection" | b"showRuler" | b"showWhiteSpace" | b"zoomToFit"
            ) {
                if let Ok(value) = crate::reader::driver::get_attribute_value(&attr) {
                    self.other_attributes.push((String::from_utf8_lossy(key).into(), value.into()));
                }
            }
        }

        if empty_flag {
            return;
        }

        xml_read_loop!(
            reader,
            Event::Empty(ref e) => {
                match e.name().into_inner() {
                    b"pane" => {
                        let mut obj = Pane::default();
                        obj.set_attributes(reader, e);
                        self.set_pane(obj);
                    }
                    b"selection" => {
                        let mut obj = Selection::default();
                        obj.set_attributes(reader, e);
                        self.set_selection(obj);
                    }
                    _ => (),
                }
            },
            Event::End(ref e) => {
                if e.name().into_inner() == b"sheetView" {
                    return
                }
            },
            Event::Eof => panic!("Error: Could not find {} end element", "sheetView")
        );
    }

    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>) {
        let empty_flag = self.pane.is_none() && self.selection.is_empty();

        // sheetView
        let mut attributes: crate::structs::AttrCollection = Vec::new();
        if self.show_grid_lines.has_value() {
            attributes.push(("showGridLines", self.show_grid_lines.value_string()).into());
        }
        if self.tab_selected.value() {
            attributes.push(("tabSelected", self.tab_selected.value_string()).into());
        }
        if self.view.has_value() {
            attributes.push(("view", self.view.value_string()).into());
        }
        let zoom_scale = self.zoom_scale.value_string();
        if self.zoom_scale.has_value() {
            attributes.push(("zoomScale", &zoom_scale).into());
        }
        let zoom_scale_normal = self.zoom_scale_normal.value_string();
        if self.zoom_scale_normal.has_value() {
            attributes.push(("zoomScaleNormal", &zoom_scale_normal).into());
        }
        let zoom_scale_page_layout_view = self.zoom_scale_page_layout_view.value_string();
        if self.zoom_scale_page_layout_view.has_value() {
            attributes.push(("zoomScalePageLayoutView", &zoom_scale_page_layout_view).into());
        }
        let zoom_scale_sheet_layout_view = self.zoom_scale_sheet_layout_view.value_string();
        if self.zoom_scale_sheet_layout_view.has_value() {
            attributes.push(("zoomScaleSheetLayoutView", &zoom_scale_sheet_layout_view).into());
        }
        let top_left_cell = self.top_left_cell.value_str();
        if self.top_left_cell.has_value() {
            attributes.push(("topLeftCell", top_left_cell).into());
        }
        for (name, value) in [
            ("showFormulas", &self.show_formulas),
            ("showRowColHeaders", &self.show_row_col_headers),
            ("showZeros", &self.show_zeros),
            ("rightToLeft", &self.right_to_left),
            ("showOutlineSymbols", &self.show_outline_symbols),
            ("defaultGridColor", &self.default_grid_color),
        ] {
            if value.has_value() {
                attributes.push((name, value.value_string()).into());
            }
        }
        let color_id = self.color_id.value_string();
        if self.color_id.has_value() {
            attributes.push(("colorId", &color_id).into());
        }
        for (k, v) in &self.other_attributes {
            attributes.push((&**k, &**v).into());
        }
        let workbook_view_id = self.workbook_view_id.value_string();
        attributes.push(("workbookViewId", &workbook_view_id).into());

        write_start_tag(writer, "sheetView", attributes, empty_flag);

        if empty_flag {
            return;
        }
        // pane
        if let Some(v) = &self.pane {
            v.write_to(writer);
        }
        // selection
        for obj in &self.selection {
            obj.write_to(writer);
        }
        write_end_tag(writer, "sheetView");
    }
}
