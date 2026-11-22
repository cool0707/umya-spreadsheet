use std::sync::{
    Arc,
    RwLock,
};

use crate::{
    StringValue,
    XlsxError,
    helper::{
        address::split_address,
        coordinate::column_index_from_string,
    },
    reader::xlsx::raw_to_deserialize_by_worksheet,
    structs::{
        Address,
        CalculationProperties,
        CellMetadata,
        PartStore,
        RichData,
        CellValue,
        Cells,
        DefinedName,
        Properties,
        SharedStringTable,
        Stylesheet,
        WorkbookProtection,
        WorkbookView,
        Worksheet,
        drawing::Theme,
    },
    traits::{
        AdjustmentCoordinate,
        AdjustmentCoordinateWithSheet,
    },
};

/// A Workbook Object.
/// The starting point of all struct.
#[derive(Clone, Default, Debug)]
pub struct Workbook {
    properties:            Properties,
    work_sheet_collection: Vec<Worksheet>,
    macros_code:           Option<Vec<u8>>,
    /// The digital signatures of the VBA project read (the relationships of
    /// `xl/vbaProject.bin`): (relationship type, part name, content).
    vba_signatures:        Vec<(String, String, Vec<u8>)>,
    jsa_macros_code:       Option<Vec<u8>>,
    code_name:             StringValue,
    ribbon_xml_data:       StringValue,
    theme:                 Theme,
    stylesheet:            Stylesheet,
    shared_string_table:   Arc<RwLock<SharedStringTable>>,
    workbook_view:         WorkbookView,
    backup_context_types:  Vec<(Box<str>, Box<str>)>,
    pivot_caches:          Vec<(Box<str>, Box<str>, Box<str>)>,
    workbook_protection:   Option<Box<WorkbookProtection>>,
    defined_names:         Vec<DefinedName>,
    date1904:              bool,
    calculation_properties: CalculationProperties,
    cell_metadata:         CellMetadata,
    rich_data:             RichData,
    chart_parts:           PartStore,
    kept_relationships:    crate::structs::KeptRelationships,
    persons:               Vec<crate::structs::office2019::person::Person>,
}

impl Workbook {
    // ************************
    // update Coordinate
    // ************************
    /// Insert new rows.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `row_index` - Specify point of insert. ex) 1
    /// * `num_rows` - Specify number to insert. ex) 2
    /// # Examples
    /// ```
    /// let mut book = umya_spreadsheet::new_file();
    /// book.insert_new_row("Sheet1", 2, 3);
    /// ```
    #[inline]
    pub fn insert_new_row(&mut self, sheet_name: &str, row_index: u32, num_rows: u32) {
        self.adjustment_insert_coordinate_with_sheet(sheet_name, 0, 0, row_index, num_rows);
    }

    /// Insert new columns.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `column` - Specify point of insert. ex) "B"
    /// * `num_columns` - Specify number to insert. ex) 3
    /// # Examples
    /// ```
    /// let mut book = umya_spreadsheet::new_file();
    /// book.insert_new_column("Sheet1", "B", 3);
    /// ```
    #[inline]
    pub fn insert_new_column(&mut self, sheet_name: &str, column: &str, num_columns: u32) {
        self.insert_new_column_by_index(sheet_name, column_index_from_string(column), num_columns);
    }

    /// Insert new columns.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `column_index` - Specify point of insert. ex) 2
    /// * `num_columns` - Specify number to insert. ex) 3
    /// # Examples
    /// ```rust
    /// let mut book = umya_spreadsheet::new_file();
    /// book.insert_new_column_by_index("Sheet1", 2, 3);
    /// ```
    #[inline]
    pub fn insert_new_column_by_index(
        &mut self,
        sheet_name: &str,
        column_index: u32,
        num_columns: u32,
    ) {
        self.adjustment_insert_coordinate_with_sheet(sheet_name, column_index, num_columns, 0, 0);
    }

    /// Remove rows.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `row_index` - Specify point of remove. ex) 1
    /// * `num_rows` - Specify number to remove. ex) &2
    /// # Examples
    /// ```rust
    /// let mut book = umya_spreadsheet::new_file();
    /// book.remove_row("Sheet1", 2, 3);
    /// ```
    #[inline]
    pub fn remove_row(&mut self, sheet_name: &str, row_index: u32, num_rows: u32) {
        self.adjustment_remove_coordinate_with_sheet(sheet_name, 0, 0, row_index, num_rows);
    }

    /// Remove columns.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `column` - Specify point of remove. ex) "B"
    /// * `num_columns` - Specify number to remove. ex) 3
    /// # Examples
    /// ```rust
    /// let mut book = umya_spreadsheet::new_file();
    /// book.remove_column("Sheet1", "B", 3);
    /// ```
    #[inline]
    pub fn remove_column(&mut self, sheet_name: &str, column: &str, num_columns: u32) {
        self.remove_column_by_index(sheet_name, column_index_from_string(column), num_columns);
    }

    /// Remove columns.
    /// # Arguments
    /// * `sheet_name` - Specify the sheet name. ex) "Sheet1"
    /// * `column_index` - Specify point of remove. ex) 2
    /// * `num_columns` - Specify number to remove. ex) 3
    /// # Examples
    /// ```rust
    /// let mut book = umya_spreadsheet::new_file();
    /// book.remove_column_by_index("Sheet1", 2, 3);
    /// ```
    #[inline]
    pub fn remove_column_by_index(
        &mut self,
        sheet_name: &str,
        column_index: u32,
        num_columns: u32,
    ) {
        self.adjustment_remove_coordinate_with_sheet(sheet_name, column_index, num_columns, 0, 0);
    }

    /// Gets the cell value by specifying an address.
    /// # Arguments
    /// * `address` - address. ex) "Sheet1!A1:C5"
    /// # Return value
    /// *`Vec<&CellValue>` - `CellValue` List.
    /// # Examples
    /// ```rust
    /// let mut book = umya_spreadsheet::new_file();
    /// let mut cell_value_List = book.cell_value_by_address("Sheet1!A1:C5");
    /// ```
    #[inline]
    #[must_use]
    pub fn cell_value_by_address(&self, address: &str) -> Vec<&CellValue> {
        let (sheet_name, range) = split_address(address);
        self.sheet_by_name(sheet_name)
            .unwrap()
            .cell_value_by_range(range)
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use cell_value_by_address()")]
    pub fn get_cell_value_by_address(&self, address: &str) -> Vec<&CellValue> {
        self.cell_value_by_address(address)
    }

    /// (This method is crate only.)
    /// Gets the cell value by specifying an Address Object.
    /// # Arguments
    /// * `address` - Address Object
    /// # Return value
    /// *`Vec<&CellValue>` - `CellValue` List.
    #[inline]
    pub(crate) fn cell_value_by_address_crate(&self, address: &Address) -> Vec<&CellValue> {
        self.sheet_by_name(address.sheet_name())
            .unwrap()
            .cell_value_by_range(&address.range().range())
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use cell_value_by_address_crate()")]
    pub(crate) fn get_cell_value_by_address_crate(&self, address: &Address) -> Vec<&CellValue> {
        self.cell_value_by_address_crate(address)
    }

    /// Get Theme.
    #[inline]
    #[must_use]
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use theme()")]
    pub fn get_theme(&self) -> &Theme {
        self.theme()
    }

    /// Get Theme in mutable.
    #[inline]
    pub fn theme_mut(&mut self) -> &mut Theme {
        &mut self.theme
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use theme_mut()")]
    pub fn get_theme_mut(&mut self) -> &mut Theme {
        self.theme_mut()
    }

    /// Set Theme.
    /// # Arguments
    /// * `value` - Theme
    #[inline]
    pub fn set_theme(&mut self, value: Theme) -> &mut Self {
        self.theme = value;
        self
    }

    /// Get Properties.
    #[inline]
    #[must_use]
    pub fn properties(&self) -> &Properties {
        &self.properties
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use properties()")]
    pub fn get_properties(&self) -> &Properties {
        self.properties()
    }

    /// Get Properties in mutable.
    #[inline]
    pub fn properties_mut(&mut self) -> &mut Properties {
        &mut self.properties
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use properties_mut()")]
    pub fn get_properties_mut(&mut self) -> &mut Properties {
        self.properties_mut()
    }

    /// Set Properties.
    /// # Arguments
    /// * `value` - Properties
    #[inline]
    pub fn set_properties(&mut self, value: Properties) -> &mut Self {
        self.properties = value;
        self
    }

    /// Get Macros Code.
    /// # Return value
    /// * `Option<&Vec<u8>>` - Macros Code Raw Data.
    #[inline]
    #[must_use]
    pub fn macros_code(&self) -> Option<&[u8]> {
        self.macros_code.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use macros_code()")]
    pub fn get_macros_code(&self) -> Option<&[u8]> {
        self.macros_code()
    }

    /// Set Macros Code.
    /// # Arguments
    /// * `value` - Macros Code Raw Data.
    ///
    /// The signatures of the project read are taken off: they sign that one.
    #[inline]
    pub fn set_macros_code(&mut self, value: impl Into<Vec<u8>>) -> &mut Self {
        self.macros_code = Some(value.into());
        self.vba_signatures.clear();
        self
    }

    /// Remove Macros Code
    #[inline]
    pub fn remove_macros_code(&mut self) -> &mut Self {
        self.macros_code = None;
        self.vba_signatures.clear();
        self
    }

    /// Whether the VBA project is signed (its signatures as read are kept
    /// while it is not replaced).
    #[inline]
    #[must_use]
    pub fn has_vba_signature(&self) -> bool {
        !self.vba_signatures.is_empty()
    }

    /// The signatures of the VBA project: (relationship type, part name,
    /// content).
    #[inline]
    pub(crate) fn vba_signatures(&self) -> &[(String, String, Vec<u8>)] {
        &self.vba_signatures
    }

    #[inline]
    pub(crate) fn set_vba_signatures(&mut self, value: Vec<(String, String, Vec<u8>)>) {
        self.vba_signatures = value;
    }

    /// Has Macros Code
    #[inline]
    #[must_use]
    pub fn has_macros(&self) -> bool {
        self.macros_code.is_some()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use has_macros()")]
    pub fn get_has_macros(&self) -> bool {
        self.has_macros()
    }

    /// Get Macros Code.
    /// # Return value
    /// * `Option<&Vec<u8>>` - Macros Code Raw Data.
    #[inline]
    #[must_use]
    pub fn jsa_macros_code(&self) -> Option<&[u8]> {
        self.jsa_macros_code.as_deref()
    }

    /// Get Macros Code.
    /// # Return value
    /// * `Option<&Vec<u8>>` - Macros Code Raw Data.
    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use jsa_macros_code()")]
    pub fn get_jsa_macros_code(&self) -> Option<&[u8]> {
        self.jsa_macros_code()
    }

    /// Set Macros Code.
    /// # Arguments
    /// * `value` - Macros Code Raw Data.
    #[inline]
    pub fn set_jsa_macros_code(&mut self, value: impl Into<Vec<u8>>) -> &mut Self {
        self.jsa_macros_code = Some(value.into());
        self
    }

    /// Remove Macros Code
    #[inline]
    pub fn remove_jsa_macros_code(&mut self) -> &mut Self {
        self.jsa_macros_code = None;
        self
    }

    /// Has Macros Code
    #[inline]
    #[must_use]
    pub fn has_jsa_macros(&self) -> bool {
        self.jsa_macros_code.is_some()
    }

    /// Has Macros Code
    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use has_jsa_macros()")]
    pub fn get_has_jsa_macros(&self) -> bool {
        self.has_jsa_macros()
    }

    /// Set codeName property of workbook
    ///
    /// May be useful when importing VBA/macros code from another workbook
    /// and only used when writing book with macros code
    ///
    /// Default one is `ThisWorkbook`.
    ///
    /// Excel often uses `Workbook________` (8 underscores).
    #[inline]
    pub fn set_code_name<S: Into<String>>(&mut self, codename: S) -> &mut Self {
        self.code_name.set_value(codename);
        self
    }

    /// Get codeName property of workbook
    ///
    /// Must to be the same in workbook with VBA/macros code from this workbook
    /// for that code in Workbook object to work out of the box without
    /// adjustments
    #[inline]
    #[must_use]
    pub fn code_name(&self) -> Option<&str> {
        self.code_name.value()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use code_name()")]
    pub fn get_code_name(&self) -> Option<&str> {
        self.code_name()
    }

    /// The font of the Normal style (the first font of the stylesheet): its
    /// name and size. Cells without a font of their own show it.
    #[inline]
    #[must_use]
    pub fn default_font(&self) -> Option<(String, f64)> {
        self.stylesheet.default_font()
    }

    /// Sets the font of the Normal style. It is written as the first font of
    /// the stylesheet when the workbook is saved, as a font of its own (not
    /// the theme's), for every cell that shows the Normal style's font.
    #[inline]
    pub fn set_default_font(&mut self, name: &str, size: f64) -> &mut Self {
        self.stylesheet.set_default_font(name, size);
        self
    }

    /// The format of the first cell format (`cellXfs`): what a cell without a
    /// format of its own shows (its `style()` is empty then), such as the
    /// alignment of the Normal style.
    #[inline]
    #[must_use]
    pub fn normal_style(&self) -> crate::structs::Style {
        self.stylesheet.style(0)
    }

    /// The table styles the workbook defines (`tableStyles` in
    /// `styles.xml`). Their elements refer to `differential_format`.
    #[inline]
    #[must_use]
    pub fn table_styles(&self) -> Vec<crate::structs::CustomTableStyle> {
        self.stylesheet.table_styles().2
    }

    /// The style a new table gets, when the workbook sets one
    /// (`defaultTableStyle`).
    #[inline]
    #[must_use]
    pub fn default_table_style(&self) -> Option<String> {
        self.stylesheet.table_styles().0
    }

    /// The style a new pivot table gets, when the workbook sets one
    /// (`defaultPivotStyle`).
    #[inline]
    #[must_use]
    pub fn default_pivot_style(&self) -> Option<String> {
        self.stylesheet.table_styles().1
    }

    /// The differential format (`dxf`) with this id, as a style: what table
    /// styles, conditional formats and tables lay over the cells.
    #[inline]
    #[must_use]
    pub fn differential_format(&self, id: u32) -> Option<crate::structs::Style> {
        self.stylesheet.differential_formats().find_style(id as usize)
    }

    /// (This method is crate only.)
    /// Get Stylesheet.
    #[inline]
    pub(crate) fn stylesheet(&self) -> &Stylesheet {
        &self.stylesheet
    }

    /// (This method is crate only.)
    /// Get Stylesheet.
    #[inline]
    pub(crate) fn stylesheet_mut(&mut self) -> &mut Stylesheet {
        &mut self.stylesheet
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use stylesheet()")]
    pub(crate) fn get_stylesheet(&self) -> &Stylesheet {
        self.stylesheet()
    }

    /// (This method is crate only.)
    /// Set Stylesheet.
    /// # Arguments
    /// * `value` - Stylesheet
    #[inline]
    pub(crate) fn set_stylesheet(&mut self, value: Stylesheet) -> &mut Self {
        self.stylesheet = value;
        self
    }

    /// (This method is crate only.)
    /// Set Default Value Stylesheet.
    #[inline]
    pub(crate) fn set_stylesheet_default_value(&mut self) -> &mut Self {
        self.stylesheet.set_defalut_value();
        self
    }

    /// (This method is crate only.)
    /// Get Shared String Table.
    #[inline]
    pub(crate) fn shared_string_table(&self) -> Arc<RwLock<SharedStringTable>> {
        self.shared_string_table.clone()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use shared_string_table()")]
    pub(crate) fn get_shared_string_table(&self) -> Arc<RwLock<SharedStringTable>> {
        self.shared_string_table()
    }

    /// (This method is crate only.)
    /// Set Shared String Table.
    /// # Arguments
    /// * `value` - Shared String Table
    #[inline]
    pub(crate) fn set_shared_string_table(&mut self, value: SharedStringTable) -> &mut Self {
        self.shared_string_table = Arc::new(RwLock::new(value));
        self
    }

    /// Get Work Sheet List.
    #[must_use]
    pub fn sheet_collection(&self) -> &[Worksheet] {
        for worksheet in &self.work_sheet_collection {
            assert!(
                worksheet.is_deserialized(),
                "This Worksheet is Not Deserialized. Please exec to read_sheet(&mut self, index: \
                 usize);"
            );
        }
        &self.work_sheet_collection
    }

    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use sheet_collection()")]
    pub fn get_sheet_collection(&self) -> &[Worksheet] {
        self.sheet_collection()
    }

    /// Get Work Sheet List.
    /// No check deserialized.
    #[inline]
    #[must_use]
    pub fn sheet_collection_no_check(&self) -> &[Worksheet] {
        &self.work_sheet_collection
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use sheet_collection_no_check()")]
    pub fn get_sheet_collection_no_check(&self) -> &[Worksheet] {
        self.sheet_collection_no_check()
    }

    /// Get Work Sheet List in mutable.
    #[inline]
    pub fn sheet_collection_mut(&mut self) -> &mut [Worksheet] {
        self.read_sheet_collection();
        &mut self.work_sheet_collection
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use sheet_collection_mut()")]
    pub fn get_sheet_collection_mut(&mut self) -> &mut [Worksheet] {
        self.sheet_collection_mut()
    }

    /// Get Work Sheet Count.
    /// # Return value
    /// * `usize` - Work Sheet Count.
    #[inline]
    #[must_use]
    pub fn sheet_count(&self) -> usize {
        self.work_sheet_collection.len()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use sheet_count()")]
    pub fn get_sheet_count(&self) -> usize {
        self.sheet_count()
    }

    /// deserialize by all worksheet.
    #[inline]
    pub fn read_sheet_collection(&mut self) -> &mut Self {
        if self
            .work_sheet_collection
            .iter()
            .all(Worksheet::is_deserialized)
        {
            return self;
        }
        let shared_string_table = self.shared_string_table();
        let stylesheet = self.stylesheet().clone();
        let cell_metadata = &self.cell_metadata;
        let chart_parts = &self.chart_parts;
        for worksheet in &mut self.work_sheet_collection {
            if !worksheet.is_deserialized() {
                raw_to_deserialize_by_worksheet(
                    worksheet,
                    &shared_string_table,
                    &stylesheet,
                    cell_metadata,
                    chart_parts,
                );
            }
        }
        self
    }

    /// deserialize a worksheet.
    #[inline]
    pub fn read_sheet(&mut self, index: usize) -> &mut Self {
        let is_deserialized = self.work_sheet_collection.get(index).unwrap().is_deserialized();
        if is_deserialized {
            return self;
        }
        let shared_string_table = self.shared_string_table();
        let stylesheet = self.stylesheet().clone();
        let worksheet = self.work_sheet_collection.get_mut(index).unwrap();
        raw_to_deserialize_by_worksheet(
            worksheet,
            &shared_string_table,
            &stylesheet,
            &self.cell_metadata,
            &self.chart_parts,
        );
        self
    }

    /// deserialize a worksheet.
    #[inline]
    pub fn read_sheet_by_name(&mut self, sheet_name: &str) -> &mut Self {
        let index = self.find_sheet_index_by_name(sheet_name).unwrap();
        self.read_sheet(index)
    }

    #[inline]
    pub(crate) fn find_sheet_index_by_name(&self, sheet_name: &str) -> Result<usize, XlsxError> {
        self.work_sheet_collection
            .iter()
            .position(|sheet| sheet.name() == sheet_name)
            .ok_or(XlsxError::NotFound())
    }

    /// Get Work Sheet.
    /// # Arguments
    /// * `index` - sheet index
    /// # Return value
    /// * `Result<&Worksheet, XlsxError>`.
    #[inline]
    pub fn sheet(&self, index: usize) -> Result<&Worksheet, XlsxError> {
        match self.work_sheet_collection.get(index) {
            Some(v) => {
                if v.is_deserialized() {
                    Ok(v)
                } else {
                    Err(XlsxError::NotDeserialized())
                }
            }
            None => Err(XlsxError::NotFound()),
        }
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use sheet()")]
    pub fn get_sheet(&self, index: &usize) -> Result<&Worksheet, XlsxError> {
        self.sheet(*index)
    }

    /// Get Work Sheet.
    /// # Arguments
    /// * `sheet_name` - sheet name
    /// # Return value
    /// * `Result<&Worksheet, XlsxError>`.
    #[inline]
    pub fn sheet_by_name(&self, sheet_name: &str) -> Result<&Worksheet, XlsxError> {
        self.find_sheet_index_by_name(sheet_name)
            .and_then(|index| self.sheet(index))
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use sheet_by_name()")]
    pub fn get_sheet_by_name(&self, sheet_name: &str) -> Result<&Worksheet, XlsxError> {
        self.sheet_by_name(sheet_name)
    }

    pub fn lazy_read_sheet_cells(&self, index: usize) -> Result<Cells, XlsxError> {
        let shared_string_table = self.shared_string_table();
        self.work_sheet_collection
            .get(index)
            .map(|v| v.cells_stream(&shared_string_table.read().unwrap(), self.stylesheet()))
            .ok_or(XlsxError::NotFound())
    }

    #[deprecated(since = "3.0.0", note = "Use lazy_read_sheet_cells()")]
    pub fn get_lazy_read_sheet_cells(&self, index: &usize) -> Result<Cells, XlsxError> {
        self.lazy_read_sheet_cells(*index)
    }

    /// Get Work Sheet in mutable.
    /// # Arguments
    /// * `index` - sheet index
    /// # Return value
    /// * `Result<&mut Worksheet, XlsxError>`.
    pub fn sheet_mut(&mut self, index: usize) -> Result<&mut Worksheet, XlsxError> {
        let is_deserialized = self
            .work_sheet_collection
            .get(index)
            .map(Worksheet::is_deserialized)
            .ok_or(XlsxError::NotFound())?;
        if is_deserialized {
            return Ok(self.work_sheet_collection.get_mut(index).unwrap());
        }
        let shared_string_table = self.shared_string_table();
        let stylesheet = self.stylesheet().clone();
        let worksheet = self.work_sheet_collection.get_mut(index).unwrap();
        raw_to_deserialize_by_worksheet(
            worksheet,
            &shared_string_table,
            &stylesheet,
            &self.cell_metadata,
            &self.chart_parts,
        );
        Ok(worksheet)
    }

    #[deprecated(since = "3.0.0", note = "Use sheet_mut()")]
    pub fn get_sheet_mut(&mut self, index: &usize) -> Result<&mut Worksheet, XlsxError> {
        self.sheet_mut(*index)
    }

    /// Get Work Sheet in mutable.
    /// # Arguments
    /// * `sheet_name` - sheet name
    /// # Return value
    /// * `Result<&mut Worksheet, XlsxError>`.
    #[inline]
    pub fn sheet_by_name_mut(&mut self, sheet_name: &str) -> Result<&mut Worksheet, XlsxError> {
        self.find_sheet_index_by_name(sheet_name)
            .and_then(move |index| self.sheet_mut(index))
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use sheet_by_name_mut()")]
    pub fn get_sheet_by_name_mut(&mut self, sheet_name: &str) -> Result<&mut Worksheet, XlsxError> {
        self.sheet_by_name_mut(sheet_name)
    }

    #[inline]
    pub fn set_active_sheet(&mut self, index: u32) -> &mut Self {
        self.workbook_view_mut().set_active_tab(index);
        self
    }

    /// Get Active Work Sheet.
    /// # Return value
    /// * `&Worksheet` - Work sheet.
    #[inline]
    #[must_use]
    pub fn active_sheet(&self) -> &Worksheet {
        let index = self.workbook_view().active_tab();
        self.sheet(index as usize).unwrap()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use active_sheet()")]
    pub fn get_active_sheet(&self) -> &Worksheet {
        self.active_sheet()
    }

    /// Get Active Work Sheet in mutable.
    /// # Return value
    /// * `&mut Worksheet` - Work sheet.
    #[inline]
    pub fn active_sheet_mut(&mut self) -> &mut Worksheet {
        let index = self.workbook_view().active_tab();
        self.sheet_mut(index as usize).unwrap()
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use active_sheet_mut()")]
    pub fn get_active_sheet_mut(&mut self) -> &mut Worksheet {
        self.active_sheet_mut()
    }

    /// Add Work Sheet.
    /// # Arguments
    /// * `value` - Work Sheet
    /// # Return value
    /// * `Result<&mut Worksheet, XlsxError>` - OK:added work sheet. Err:Error.
    #[inline]
    pub fn add_sheet(&mut self, value: Worksheet) -> Result<&mut Worksheet, XlsxError> {
        let title = value.name();
        Workbook::check_sheet_name(self, title)?;
        self.work_sheet_collection.push(value);
        Ok(self.work_sheet_collection.last_mut().unwrap())
    }

    /// Remove and return all worksheets, leaving the collection empty.
    /// Used by the streaming writer to take ownership of pre-built sheets while
    /// keeping the workbook-level configuration.
    #[inline]
    pub(crate) fn take_all_sheets(&mut self) -> Vec<Worksheet> {
        std::mem::take(&mut self.work_sheet_collection)
    }

    /// Remove Work Sheet.
    /// # Arguments
    /// * `index` - sheet index
    /// # Return value
    /// * `Result<(), XlsxError>` - OK:removed worksheet. Err:Error.
    #[inline]
    pub fn remove_sheet(&mut self, index: usize) -> Result<(), XlsxError> {
        if self.work_sheet_collection.len() <= index {
            return Err(XlsxError::NotFound());
        }
        self.work_sheet_collection.remove(index);
        Ok(())
    }

    /// Remove Work Sheet.
    /// # Arguments
    /// * `sheet_name` - sheet name
    /// # Return value
    /// * `Result<(), XlsxError>` - OK:removed worksheet. Err:Error.
    pub fn remove_sheet_by_name(&mut self, sheet_name: &str) -> Result<(), XlsxError> {
        let cnt_before = self.work_sheet_collection.len();
        self.work_sheet_collection
            .retain(|x| x.name() != sheet_name);
        let cnt_after = self.work_sheet_collection.len();
        if cnt_before == cnt_after {
            return Err(XlsxError::NotFound());
        }
        Ok(())
    }

    /// Add New Work Sheet.
    /// # Arguments
    /// * `sheet_title` - sheet title
    /// # Return value
    /// * `Result<&mut Worksheet, XlsxError>` - OK:added work sheet. Err:Error.
    #[inline]
    pub fn new_sheet<S: Into<String>>(
        &mut self,
        sheet_title: S,
    ) -> Result<&mut Worksheet, XlsxError> {
        let v = sheet_title.into();
        Workbook::check_sheet_name(self, &v)?;
        let sheet_id = (self.work_sheet_collection.len() + 1).to_string();
        Ok(Workbook::add_new_sheet_crate(self, sheet_id, v.clone()))
    }

    /// (This method is crate only.)
    /// Add New Work Sheet.
    /// # Arguments
    /// * `index` - sheet index
    /// * `sheet_title` - sheet title
    /// # Return value
    /// * `&mut Worksheet` - added work sheet.
    pub(crate) fn add_new_sheet_crate<S: Into<String>>(
        &mut self,
        sheet_id: S,
        sheet_title: S,
    ) -> &mut Worksheet {
        let mut worksheet = Worksheet::default();
        worksheet.set_sheet_id(sheet_id);
        worksheet.set_name(sheet_title.into());
        worksheet.sheet_format_properties_mut().set_defalut_value();
        self.work_sheet_collection.push(worksheet);
        self.work_sheet_collection.last_mut().unwrap()
    }

    /// Rewrites every formula of the workbook: of the cells, the names, the
    /// data validations, the conditional formats, the tables, the charts and
    /// the parts kept as they were read (sparklines, form controls, external
    /// links aside). `f` gets the sheet the references without a sheet name
    /// refer to (`None` for the names of the workbook and what is kept with
    /// it) and the formula as the file holds it (no `=`), and returns the
    /// new formula or `None` to leave it as it is.
    pub fn rewrite_formulas(&mut self, f: &mut crate::helper::rewrite::FormulaFn<'_>) {
        self.read_sheet_collection();
        self.rewrite_all(f, None);
    }

    fn rewrite_all(&mut self, f: &mut crate::helper::rewrite::FormulaFn<'_>, unshare: Option<usize>) {
        let names: Vec<String> = self.work_sheet_collection.iter().map(|s| s.name().to_string()).collect();
        for defined_name in &mut self.defined_names {
            let host = if defined_name.has_local_sheet_id() {
                names.get(defined_name.local_sheet_id() as usize).map(String::as_str)
            } else {
                None
            };
            defined_name.rewrite_formulas(host, f);
        }
        self.kept_relationships.rewrite_xml(&mut crate::helper::rewrite::FormulaXml { host: None, f });
        for (i, sheet) in self.work_sheet_collection.iter_mut().enumerate() {
            sheet.rewrite_formulas_inner(f, unshare == Some(i));
        }
    }

    /// Inserts or removes rows or columns of a sheet as Excel does. What is
    /// on the sheet moves (cells, sizes, merged cells, filters, the ranges of
    /// data validations, conditional formats, tables, sparklines and pivot
    /// tables, page breaks, comments, drawings), and `f` rewrites every
    /// formula of the workbook first (see [`Self::rewrite_formulas`]): it is
    /// to move the references to the sheet (named, or without a name when
    /// the sheet it gets is the one edited) as the edit moves the cells.
    /// The sources of pivot caches on the sheet move too.
    /// # Errors
    /// `NotFound` when there is no such sheet.
    pub fn edit_lines(
        &mut self,
        sheet_name: &str,
        edit: &crate::helper::rewrite::LineEdit,
        f: &mut crate::helper::rewrite::FormulaFn<'_>,
    ) -> Result<(), XlsxError> {
        self.read_sheet_collection();
        let index = self
            .work_sheet_collection
            .iter()
            .position(|s| s.name() == sheet_name)
            .ok_or(XlsxError::NotFound())?;
        self.rewrite_all(f, Some(index));
        let title = self.work_sheet_collection[index].name().to_string();
        let mut others = crate::helper::rewrite::AreaXml { sheet: &title, edit, own: false };
        self.kept_relationships.rewrite_xml(&mut others);
        for (i, sheet) in self.work_sheet_collection.iter_mut().enumerate() {
            if i != index {
                sheet.kept_relationships_mut().rewrite_xml(&mut others);
            }
        }
        self.work_sheet_collection[index].shift_lines(edit);
        Ok(())
    }

    /// Copies a sheet as Excel does, giving the copy the name `sheet_name`
    /// and the place `position` (0 = first). The copy's names refer to it,
    /// and so do its charts, sparklines and form controls; its formulas keep
    /// the sheets they name. Its tables get names of their own (the name and
    /// `_2`, `_3`...), in the order of the original's; formulas that use
    /// them by name are left to the caller ([`Worksheet::rewrite_formulas`]).
    /// # Errors
    /// `NotFound` when there is no sheet at `index`, or an error when the
    /// name can not be a sheet's.
    pub fn copy_sheet(&mut self, index: usize, sheet_name: &str, position: usize) -> Result<&mut Worksheet, XlsxError> {
        Workbook::check_sheet_name(self, sheet_name)?;
        self.read_sheet_collection();
        let source = self.work_sheet_collection.get(index).ok_or(XlsxError::NotFound())?;
        let old = source.name().to_string();
        let mut copy = source.clone();
        copy.set_name(sheet_name);
        copy.make_copy_of(&old);
        let position = position.min(self.work_sheet_collection.len());
        // The names local to sheets refer to them by place.
        let shift = |d: &mut DefinedName| {
            if d.has_local_sheet_id() && d.local_sheet_id() as usize >= position {
                d.set_local_sheet_id(d.local_sheet_id() + 1);
            }
        };
        self.defined_names.iter_mut().for_each(shift);
        for sheet in &mut self.work_sheet_collection {
            sheet.defined_names_crate_mut().iter_mut().for_each(shift);
        }
        for d in copy.defined_names_crate_mut() {
            if d.has_local_sheet_id() {
                d.set_local_sheet_id(position as u32);
            }
        }
        // Tables have names of their own in the workbook.
        let mut taken: Vec<String> = self
            .work_sheet_collection
            .iter()
            .flat_map(|s| s.tables().iter().flat_map(|t| [t.name().to_lowercase(), t.display_name().to_lowercase()]))
            .collect();
        for table in copy.tables_mut() {
            let base = if table.display_name().is_empty() { table.name().to_string() } else { table.display_name().to_string() };
            let mut n = 2;
            let name = loop {
                let name = format!("{base}_{n}");
                if !taken.contains(&name.to_lowercase()) {
                    break name;
                }
                n += 1;
            };
            taken.push(name.to_lowercase());
            table.set_name(&name);
            table.set_display_name(&name);
        }
        let active = self.workbook_view.active_tab() as usize;
        if active >= position && active < self.work_sheet_collection.len() {
            self.workbook_view.set_active_tab(active as u32 + 1);
        }
        self.work_sheet_collection.insert(position, copy);
        Ok(&mut self.work_sheet_collection[position])
    }

    /// Set Sheet Name.
    /// # Arguments
    /// * `index` - target sheet index
    /// * `sheet_name` - sheet name
    /// # Return value
    /// * `Result<(), XlsxError>` - OK:Success  Err:Error.
    pub fn set_sheet_name<S: Into<String>>(
        &mut self,
        index: usize,
        sheet_name: S,
    ) -> Result<(), XlsxError> {
        let sheet_name_str = sheet_name.into();
        Workbook::check_sheet_name(self, sheet_name_str.as_ref())?;
        if index >= self.work_sheet_collection.len() {
            return Err(XlsxError::NotFound());
        }
        // What refers to the sheet is renamed too, as in Excel.
        self.read_sheet_collection();
        let old = self.work_sheet_collection[index].name().to_string();
        self.work_sheet_collection[index].set_name(sheet_name_str);
        let rename = crate::helper::sheet_rename::SheetRename::new(&old, self.work_sheet_collection[index].name());
        for defined_name in &mut self.defined_names {
            defined_name.rename_sheet(&rename, false);
        }
        self.kept_relationships.rename_sheet(&rename);
        for sheet in &mut self.work_sheet_collection {
            sheet.rename_sheet_references(&rename);
        }
        Ok(())
    }

    /// (This method is crate only.)
    /// Check for duplicate sheet name.
    pub(crate) fn check_sheet_name(&self, value: &str) -> Result<(), XlsxError> {
        if self
            .work_sheet_collection
            .iter()
            .any(|work_sheet| value == work_sheet.name())
        {
            Err(XlsxError::NameDuplicate())
        } else {
            Ok(())
        }
    }

    /// (This method is crate only.)
    /// Has Ribbon XML Data.
    #[inline]
    pub(crate) fn has_ribbon(&self) -> bool {
        self.ribbon_xml_data.has_value()
    }

    /// Get Workbook View.
    #[inline]
    #[must_use]
    pub fn workbook_view(&self) -> &WorkbookView {
        &self.workbook_view
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use workbook_view()")]
    pub fn get_workbook_view(&self) -> &WorkbookView {
        self.workbook_view()
    }

    /// Get Workbook View in mutable.
    #[inline]
    pub fn workbook_view_mut(&mut self) -> &mut WorkbookView {
        &mut self.workbook_view
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use workbook_view_mut()")]
    pub fn get_workbook_view_mut(&mut self) -> &mut WorkbookView {
        self.workbook_view_mut()
    }

    /// Set Workbook View.
    /// # Arguments
    /// * `value` - `WorkbookView`
    #[inline]
    pub fn set_workbook_view(&mut self, value: WorkbookView) -> &mut Self {
        self.workbook_view = value;
        self
    }

    /// (This method is crate only.)
    /// Has Defined Names.
    #[inline]
    pub(crate) fn has_defined_names(&self) -> bool {
        if !self.defined_names.is_empty() {
            return true;
        }
        self.sheet_collection_no_check()
            .iter()
            .any(Worksheet::has_defined_names)
    }

    #[inline]
    pub(crate) fn backup_context_types(&self) -> &[(Box<str>, Box<str>)] {
        &self.backup_context_types
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use backup_context_types()")]
    pub(crate) fn get_backup_context_types(&self) -> &[(Box<str>, Box<str>)] {
        self.backup_context_types()
    }

    #[inline]
    pub(crate) fn set_backup_context_types(
        &mut self,
        value: impl Into<Vec<(String, String)>>,
    ) -> &mut Self {
        self.backup_context_types = value
            .into()
            .into_iter()
            .map(|(a, b)| (a.into_boxed_str(), b.into_boxed_str()))
            .collect();
        self
    }

    #[must_use]
    pub fn pivot_caches(&self) -> Vec<(String, String, String)> {
        let mut result: Vec<(String, String, String)> = Vec::new();
        for (val1, val2, val3) in &self.pivot_caches {
            let val3_up = format!("xl/{val3}");

            // Check if this cache is referenced by any worksheet's pivot cache definitions
            // (for existing files)
            let mut found_in_raw_data = false;
            for worksheet in self.sheet_collection_no_check() {
                for pivot_cache_definition in worksheet.pivot_cache_definition_collection() {
                    if val3_up == pivot_cache_definition
                        && !result.iter().any(|(_, _, r_val3)| r_val3 == &**val3)
                    {
                        result.push((val1.to_string(), val2.to_string(), val3.to_string()));
                        found_in_raw_data = true;
                        break;
                    }
                }
                if found_in_raw_data {
                    break;
                }
            }

            // If not found in raw data, check if any worksheet has pivot tables (for newly
            // created pivot tables)
            if !found_in_raw_data {
                for worksheet in self.sheet_collection_no_check() {
                    if !worksheet.pivot_tables().is_empty()
                        && !result.iter().any(|(_, _, r_val3)| r_val3 == &**val3)
                    {
                        result.push((val1.to_string(), val2.to_string(), val3.to_string()));
                        break;
                    }
                }
            }
        }
        result
    }

    #[deprecated(since = "3.0.0", note = "Use pivot_caches()")]
    pub(crate) fn get_pivot_caches(&self) -> Vec<(String, String, String)> {
        self.pivot_caches()
    }

    #[inline]
    pub fn add_pivot_caches(&mut self, value: (String, String, String)) -> &mut Self {
        self.pivot_caches.push((
            value.0.into_boxed_str(),
            value.1.into_boxed_str(),
            value.2.into_boxed_str(),
        ));
        self
    }

    #[inline]
    pub fn update_pivot_caches(&mut self, key: &str, value: &str) -> &mut Self {
        self.pivot_caches.iter_mut().for_each(|(val1, _, val3)| {
            if &**val1 == key {
                *val3 = value.to_owned().into_boxed_str();
            }
        });
        self
    }

    #[inline]
    #[must_use]
    pub fn workbook_protection(&self) -> Option<&WorkbookProtection> {
        self.workbook_protection.as_deref()
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use workbook_protection()")]
    pub fn get_workbook_protection(&self) -> Option<&WorkbookProtection> {
        self.workbook_protection()
    }

    #[inline]
    pub fn workbook_protection_mut(&mut self) -> &mut WorkbookProtection {
        self.workbook_protection
            .get_or_insert(Box::new(WorkbookProtection::default()))
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use workbook_protection_mut()")]
    pub fn get_workbook_protection_mut(&mut self) -> &mut WorkbookProtection {
        self.workbook_protection_mut()
    }

    #[inline]
    pub fn set_workbook_protection(&mut self, value: WorkbookProtection) -> &mut Self {
        self.workbook_protection = Some(Box::new(value));
        self
    }

    #[inline]
    pub fn remove_workbook_protection(&mut self) -> &mut Self {
        self.workbook_protection = None;
        self
    }

    /// Whether the workbook uses the 1904 date system (`workbookPr/@date1904`).
    ///
    /// In the 1904 system, serial 0 is 1904-01-01 and serial 1 is 1904-01-02;
    /// in the default 1900 system, serial 1 is 1900-01-01. From 1900-03-01 on
    /// (past Excel's fictitious 1900-02-29), the same serial is a date 1,462
    /// days later in the 1904 system than in the 1900 system.
    #[inline]
    #[must_use]
    pub fn date1904(&self) -> bool {
        self.date1904
    }

    /// Set whether the workbook uses the 1904 date system.
    ///
    /// This only changes how serial numbers are interpreted as dates; the
    /// stored cell values are not converted.
    #[inline]
    pub fn set_date1904(&mut self, value: bool) -> &mut Self {
        self.date1904 = value;
        self
    }

    /// Get the calculation properties of the workbook (`calcPr`).
    #[inline]
    #[must_use]
    pub fn calculation_properties(&self) -> &CalculationProperties {
        &self.calculation_properties
    }

    /// Get the calculation properties of the workbook in mutable.
    #[inline]
    pub fn calculation_properties_mut(&mut self) -> &mut CalculationProperties {
        &mut self.calculation_properties
    }

    /// Set the calculation properties of the workbook.
    /// # Arguments
    /// * `value` - `CalculationProperties`
    #[inline]
    pub fn set_calculation_properties(&mut self, value: CalculationProperties) -> &mut Self {
        self.calculation_properties = value;
        self
    }

    /// The cell metadata of the file the workbook was read from (what the
    /// `cm` of its cells means).
    #[inline]
    pub(crate) fn cell_metadata(&self) -> &CellMetadata {
        &self.cell_metadata
    }

    #[inline]
    pub(crate) fn set_cell_metadata(&mut self, value: CellMetadata) -> &mut Self {
        self.cell_metadata = value;
        self
    }

    /// The rich values of the file the workbook was read from (what the
    /// value metadata of its cells, `vm`, refers to).
    #[inline]
    pub(crate) fn rich_data(&self) -> &RichData {
        &self.rich_data
    }

    #[inline]
    pub(crate) fn set_rich_data(&mut self, value: RichData) -> &mut Self {
        self.rich_data = value;
        self
    }

    /// The authors of the threaded comments (`ThreadedComment::person_id`).
    #[inline]
    #[must_use]
    pub fn persons(&self) -> &[crate::structs::office2019::person::Person] {
        &self.persons
    }

    #[inline]
    pub fn persons_mut(&mut self) -> &mut Vec<crate::structs::office2019::person::Person> {
        &mut self.persons
    }

    /// The relationships to parts the model has nothing for, kept with those
    /// parts and the elements that refer to them.
    #[inline]
    pub(crate) fn kept_relationships(&self) -> &crate::structs::KeptRelationships {
        &self.kept_relationships
    }

    #[inline]
    pub(crate) fn kept_relationships_mut(&mut self) -> &mut crate::structs::KeptRelationships {
        &mut self.kept_relationships
    }

    /// The parts of the file read that its charts are made of, for the
    /// drawings of the sheets to keep their charts as they were.
    #[inline]
    pub(crate) fn set_chart_parts(&mut self, value: PartStore) -> &mut Self {
        self.chart_parts = value;
        self
    }

    /// Get Defined Name (Vec).
    #[inline]
    #[must_use]
    pub fn defined_names(&self) -> &[DefinedName] {
        &self.defined_names
    }

    #[inline]
    #[must_use]
    #[deprecated(since = "3.0.0", note = "Use defined_names()")]
    pub fn get_defined_names(&self) -> &[DefinedName] {
        self.defined_names()
    }

    /// Get Defined Name (Vec) in mutable.
    #[inline]
    pub fn defined_names_mut(&mut self) -> &mut Vec<DefinedName> {
        &mut self.defined_names
    }

    #[inline]
    #[deprecated(since = "3.0.0", note = "Use defined_names_mut()")]
    pub fn get_defined_names_mut(&mut self) -> &mut Vec<DefinedName> {
        self.defined_names_mut()
    }

    /// Set Defined Name (Vec).
    /// # Arguments
    /// * `value` - Vec<DefinedName>.
    #[inline]
    pub fn set_defined_names(&mut self, value: impl Into<Vec<DefinedName>>) {
        self.defined_names = value.into();
    }

    /// Add Defined Name.
    /// # Arguments
    /// * `value` - `DefinedName`.
    #[inline]
    pub fn add_defined_names(&mut self, value: DefinedName) {
        self.defined_names.push(value);
    }

    /// Has `ThreadedComments`.
    #[inline]
    #[must_use]
    pub fn has_threaded_comments(&self) -> bool {
        for worksheet in &self.work_sheet_collection {
            if worksheet.has_threaded_comments() {
                return true;
            }
        }
        false
    }
}
impl AdjustmentCoordinateWithSheet for Workbook {
    fn adjustment_insert_coordinate_with_sheet(
        &mut self,
        sheet_name: &str,
        root_col_num: u32,
        offset_col_num: u32,
        root_row_num: u32,
        offset_row_num: u32,
    ) {
        self.read_sheet_collection();
        for worksheet in &mut self.work_sheet_collection {
            worksheet.adjustment_insert_coordinate(
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
            worksheet.adjustment_insert_coordinate_with_sheet(
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
        self.read_sheet_collection();
        for worksheet in &mut self.work_sheet_collection {
            worksheet.adjustment_remove_coordinate(
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
            worksheet.adjustment_remove_coordinate_with_sheet(
                sheet_name,
                root_col_num,
                offset_col_num,
                root_row_num,
                offset_row_num,
            );
        }
    }
}
