use super::{
    BooleanValue,
    EnumValue,
    StringValue,
    TotalsRowFunctionValues,
    UInt32Value,
    coordinate::Coordinate,
};
use crate::{AutoFilter, helper::coordinate::CellCoordinates};

// use reader::driver::*;

#[derive(Clone, Default, Debug)]
pub struct Table {
    name:             Box<str>,
    area:             (Coordinate, Coordinate),
    display_name:     Box<str>,
    columns:          Vec<TableColumn>,
    style_info:       Option<Box<TableStyleInfo>>,
    totals_row_shown: BooleanValue,
    totals_row_count: UInt32Value,
    auto_filter:      AutoFilter,
}
impl Table {
    #[inline]
    pub fn new<T>(name: &str, area: (T, T)) -> Self
    where
        T: Into<CellCoordinates>,
    {
        let coord_beg = Self::cell_coord_to_coord(area.0);
        let coord_end = Self::cell_coord_to_coord(area.1);
        let name: Box<str> = name.into();
        let mut auto_filter = AutoFilter::default();
        auto_filter.set_range(format!("{coord_beg}:{coord_end}"));
        Self {
            area:             (coord_beg, coord_end),
            name:             name.clone(),
            display_name:     name,
            columns:          Vec::<TableColumn>::default(),
            style_info:       None,
            totals_row_shown: BooleanValue::default(),
            totals_row_count: UInt32Value::default(),
            auto_filter,
        }
    }

    #[inline]
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !(self.name.is_empty()
            || self.display_name.is_empty()
            || self.area.0.col_num() == 0
            || self.area.0.row_num() == 0
            || self.area.1.col_num() == 0
            || self.area.1.row_num() == 0
            || self.area.0.col_num() > self.area.1.col_num()
            || self.area.0.row_num() > self.area.1.row_num())
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use name()")]
    pub fn get_name(&self) -> &str {
        self.name()
    }

    #[inline]
    pub fn set_name(&mut self, name: &str) {
        self.name = name.into();
        if self.display_name.is_empty() {
            self.display_name = name.into();
        }
    }

    #[inline]
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use display_name()")]
    pub fn get_display_name(&self) -> &str {
        self.display_name()
    }

    #[inline]
    pub fn set_display_name(&mut self, display_name: &str) {
        self.display_name = display_name.into();
    }

    #[inline]
    #[must_use]
    pub fn area(&self) -> &(Coordinate, Coordinate) {
        &self.area
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use area()")]
    pub fn get_area(&self) -> &(Coordinate, Coordinate) {
        self.area()
    }

    #[inline]
    pub fn set_area<T>(&mut self, area: (T, T))
    where
        T: Into<CellCoordinates>,
    {
        let coord_beg = Self::cell_coord_to_coord(area.0);
        let coord_end = Self::cell_coord_to_coord(area.1);
        self.area = (coord_beg, coord_end);
    }

    /// Moves the table as rows or columns are inserted or removed, as Excel
    /// does: inserting inside widens it (a column gets a name of its own),
    /// removing narrows it. Returns the header cells of the columns added
    /// (column, row, name), or `None` when all of the table is removed.
    pub(crate) fn shift_lines(&mut self, edit: &crate::helper::rewrite::LineEdit) -> Option<Vec<(u32, u32, String)>> {
        use crate::helper::rewrite::LineAxis;
        let (c1, r1) = (self.area.0.col_num(), self.area.0.row_num());
        let (c2, r2) = (self.area.1.col_num(), self.area.1.row_num());
        let totals = self.totals_row_count.value();
        let mut headers = Vec::new();
        let (nc1, nr1, nc2, mut nr2) = match edit.axis {
            LineAxis::Rows => {
                let (a, b) = edit.span(r1, r2)?;
                if !edit.insert && totals > 0 && edit.index(r2).is_none() {
                    // The totals row removed.
                    self.totals_row_count = UInt32Value::default();
                }
                (c1, a, c2, b)
            }
            LineAxis::Columns => {
                let (a, b) = edit.span(c1, c2)?;
                if edit.insert && edit.at > c1 && edit.at <= c2 {
                    let mut taken: Vec<String> = self.columns.iter().map(|c| c.name.to_lowercase()).collect();
                    let mut n = 0;
                    let mut added = Vec::new();
                    for i in 0..edit.count {
                        let name = loop {
                            n += 1;
                            let name = format!("{}{n}", edit.table_column);
                            if !taken.contains(&name.to_lowercase()) {
                                break name;
                            }
                        };
                        taken.push(name.to_lowercase());
                        headers.push((edit.at + i, r1, name.clone()));
                        added.push(TableColumn::new(&name));
                    }
                    let at = (edit.at - c1) as usize;
                    let at = at.min(self.columns.len());
                    self.columns.splice(at..at, added);
                } else if !edit.insert {
                    for col in (c1..=c2).rev() {
                        if edit.index(col).is_none() && ((col - c1) as usize) < self.columns.len() {
                            self.columns.remove((col - c1) as usize);
                        }
                    }
                }
                (a, r1, b, r2)
            }
        };
        if nr2 < nr1 {
            nr2 = nr1;
        }
        self.set_area(((nc1, nr1), (nc2, nr2)));
        if !self.auto_filter.range().range().is_empty() {
            let last = nr2.saturating_sub(self.totals_row_count.value()).max(nr1);
            self.auto_filter.set_range(format!(
                "{}{nr1}:{}{last}",
                crate::helper::coordinate::string_from_column_index(nc1),
                crate::helper::coordinate::string_from_column_index(nc2)
            ));
        }
        Some(headers)
    }

    #[inline]
    pub fn add_column(&mut self, col: TableColumn) {
        self.columns.push(col);
    }

    #[inline]
    #[must_use]
    pub fn columns(&self) -> &[TableColumn] {
        &self.columns
    }

    #[inline]
    #[must_use]
    pub fn columns_mut(&mut self) -> &mut [TableColumn] {
        &mut self.columns
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use columns()")]
    pub fn get_columns(&self) -> &[TableColumn] {
        self.columns()
    }

    #[inline]
    pub(crate) fn has_style_info(&self) -> bool {
        self.style_info.is_some()
    }

    #[inline]
    #[must_use]
    pub fn style_info(&self) -> Option<&TableStyleInfo> {
        self.style_info.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use style_info()")]
    pub fn get_style_info(&self) -> Option<&TableStyleInfo> {
        self.style_info()
    }

    #[inline]
    pub fn set_style_info(&mut self, style_info: Option<TableStyleInfo>) {
        self.style_info = style_info.map(Box::new);
    }

    #[inline]
    pub(crate) fn has_totals_row_shown(&self) -> bool {
        self.totals_row_shown.has_value()
    }

    #[inline]
    #[must_use]
    pub fn totals_row_shown(&self) -> bool {
        self.totals_row_shown.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use totals_row_shown()")]
    pub fn get_totals_row_shown(&self) -> bool {
        self.totals_row_shown()
    }

    #[inline]
    pub(crate) fn totals_row_shown_str(&self) -> &str {
        self.totals_row_shown.value_string()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use totals_row_shown_str()")]
    pub(crate) fn get_totals_row_shown_str(&self) -> &str {
        self.totals_row_shown_str()
    }

    #[inline]
    pub fn set_totals_row_shown(&mut self, value: bool) {
        self.totals_row_shown.set_value(value);
    }

    #[inline]
    pub(crate) fn set_totals_row_shown_str(&mut self, value: &str) {
        self.totals_row_shown.set_value_string(value);
    }

    #[inline]
    pub(crate) fn has_totals_row_count(&self) -> bool {
        self.totals_row_count.has_value()
    }

    #[inline]
    #[must_use]
    pub fn totals_row_count(&self) -> u32 {
        self.totals_row_count.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use totals_row_count()")]
    pub fn get_totals_row_count(&self) -> u32 {
        self.totals_row_count()
    }

    #[inline]
    pub(crate) fn totals_row_count_str(&self) -> String {
        self.totals_row_count.value_string()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use totals_row_count_str()")]
    pub(crate) fn get_totals_row_count_str(&self) -> String {
        self.totals_row_count_str()
    }

    #[inline]
    pub fn set_totals_row_count(&mut self, value: u32) {
        self.totals_row_count.set_value(value);
    }

    #[inline]
    pub(crate) fn set_totals_row_count_str(&mut self, value: &str) {
        self.totals_row_count.set_value_string(value);
    }

    #[inline]
    #[must_use]
    pub fn auto_filter(&self) -> &AutoFilter {
        &self.auto_filter
    }

    #[inline]
    pub fn auto_filter_mut(&mut self) -> &mut AutoFilter {
        &mut self.auto_filter
    }

    #[inline]
    pub fn set_auto_filter<S: Into<String>>(&mut self, range: S) {
        let mut auto_filter = AutoFilter::default();
        auto_filter.set_range(range);
        self.auto_filter = auto_filter;
    }

    #[inline]
    pub(crate) fn set_auto_filter_obj(&mut self, value: AutoFilter) {
        self.auto_filter = value;
    }

    #[inline]
    fn cell_coord_to_coord<T>(cc: T) -> Coordinate
    where
        T: Into<CellCoordinates>,
    {
        let cell_coord: CellCoordinates = cc.into();
        let mut coord: Coordinate = Coordinate::default();
        coord.set_col_num(cell_coord.col);
        coord.set_row_num(cell_coord.row);
        coord
    }
}

#[derive(Clone, Default, Debug)]
pub struct TableColumn {
    name:                      String,
    totals_row_label:          StringValue,
    totals_row_function:       EnumValue<TotalsRowFunctionValues>,
    calculated_column_formula: Option<String>,
}
impl TableColumn {
    pub(crate) fn rename_sheet(&mut self, rename: &crate::helper::sheet_rename::SheetRename) {
        if let Some(new) = self.calculated_column_formula.as_deref().and_then(|f| rename.formula(f)) {
            self.calculated_column_formula = Some(new);
        }
    }

    pub(crate) fn rewrite_formulas(&mut self, host: Option<&str>, f: &mut crate::helper::rewrite::FormulaFn<'_>) {
        if let Some(new) = self.calculated_column_formula.as_deref().and_then(|v| f(host, v)) {
            self.calculated_column_formula = Some(new);
        }
    }

    #[inline]
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name:                      name.to_string(),
            totals_row_label:          StringValue::default(),
            totals_row_function:       EnumValue::default(),
            calculated_column_formula: None,
        }
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use name()")]
    pub fn get_name(&self) -> &str {
        self.name()
    }

    #[inline]
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn has_totals_row_label(&self) -> bool {
        self.totals_row_label.has_value()
    }

    #[inline]
    #[must_use]
    pub fn totals_row_label(&self) -> Option<&str> {
        self.totals_row_label.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use totals_row_label()")]
    pub fn get_totals_row_label(&self) -> Option<&str> {
        self.totals_row_label()
    }

    #[inline]
    pub(crate) fn totals_row_label_str(&self) -> &str {
        self.totals_row_label.value_str()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use totals_row_label_str()")]
    pub(crate) fn get_totals_row_label_str(&self) -> &str {
        self.totals_row_label_str()
    }

    #[inline]
    pub fn set_totals_row_label(&mut self, value: &str) {
        self.totals_row_label.set_value(value);
    }

    #[inline]
    pub(crate) fn set_totals_row_label_str(&mut self, value: &str) {
        self.totals_row_label.set_value_string(value);
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn has_totals_row_function(&self) -> bool {
        self.totals_row_function.has_value()
    }

    #[inline]
    #[must_use]
    pub fn totals_row_function(&self) -> &TotalsRowFunctionValues {
        self.totals_row_function.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use totals_row_function()")]
    pub fn get_totals_row_function(&self) -> &TotalsRowFunctionValues {
        self.totals_row_function()
    }

    #[inline]
    pub(crate) fn totals_row_function_str(&self) -> &str {
        self.totals_row_function.value_string()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use totals_row_function_str()")]
    pub(crate) fn get_totals_row_function_str(&self) -> &str {
        self.totals_row_function_str()
    }

    #[inline]
    pub fn set_totals_row_function(&mut self, value: TotalsRowFunctionValues) {
        self.totals_row_function.set_value(value);
    }

    #[inline]
    pub(crate) fn set_totals_row_function_str(&mut self, value: &str) {
        self.totals_row_function.set_value_string(value);
    }

    #[inline]
    #[must_use]
    pub fn calculated_column_formula(&self) -> Option<&String> {
        self.calculated_column_formula.as_ref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use calculated_column_formula()")]
    pub fn get_calculated_column_formula(&self) -> Option<&String> {
        self.calculated_column_formula()
    }

    #[inline]
    pub(crate) fn set_calculated_column_formula(&mut self, value: String) {
        self.calculated_column_formula = Some(value);
    }
}

#[derive(Clone, Debug)]
pub struct TableStyleInfo {
    name:             String,
    show_first_col:   ShowColumn,
    show_last_col:    ShowColumn,
    show_row_stripes: ShowStripes,
    show_col_stripes: ShowStripes,
}

#[derive(Debug, Clone, Copy)]
pub enum ShowColumn {
    Show,
    Hide,
}

#[derive(Debug, Clone, Copy)]
pub enum ShowStripes {
    Show,
    Hide,
}

impl Default for TableStyleInfo {
    fn default() -> Self {
        Self {
            name:             String::new(), // Default name can be an empty string
            show_first_col:   ShowColumn::Hide, // Default to Hide
            show_last_col:    ShowColumn::Hide, // Default to Hide
            show_row_stripes: ShowStripes::Hide, // Default to Hide
            show_col_stripes: ShowStripes::Hide, // Default to Hide
        }
    }
}

impl TableStyleInfo {
    #[inline]
    #[must_use]
    pub fn new(
        name: &str,
        show_first_col: ShowColumn,
        show_last_col: ShowColumn,
        show_row_stripes: ShowStripes,
        show_col_stripes: ShowStripes,
    ) -> Self {
        Self {
            name: name.to_string(),
            show_first_col,
            show_last_col,
            show_row_stripes,
            show_col_stripes,
        }
    }

    #[inline]
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use name()")]
    pub fn get_name(&self) -> &str {
        self.name()
    }

    #[inline]
    #[must_use]
    pub fn is_show_first_col(&self) -> bool {
        matches!(self.show_first_col, ShowColumn::Show)
    }

    #[inline]
    #[must_use]
    pub fn is_show_last_col(&self) -> bool {
        matches!(self.show_last_col, ShowColumn::Show)
    }

    #[inline]
    #[must_use]
    pub fn is_show_row_stripes(&self) -> bool {
        matches!(self.show_row_stripes, ShowStripes::Show)
    }

    #[inline]
    #[must_use]
    pub fn is_show_col_stripes(&self) -> bool {
        matches!(self.show_col_stripes, ShowStripes::Show)
    }
}
