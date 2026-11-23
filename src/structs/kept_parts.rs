//! Parts of a package kept as they were read and written back without being
//! looked into: the charts of the drawings (`xl/charts/chart1.xml`,
//! `chartEx1.xml`), with the parts they refer to (their styles and colors,
//! the workbook embedded for their data, the drawing of the shapes drawn on
//! them and its pictures).
//!
//! The model of a chart of this crate (`ChartSpace`) leaves out much of what
//! a chart has (and the kinds it does not know entirely), so a chart that was
//! read is kept like this instead.
//!
//! The same for the parts the model has nothing for, which the workbook and
//! its sheets refer to (`KeptRelationships`): external links, slicers and
//! timelines with their caches, the properties of form controls, custom XML.

use std::{
    collections::HashMap,
    io::{
        self,
        Cursor,
        Read,
    },
};

use quick_xml::{
    Reader,
    Writer,
    escape::{
        partial_escape,
        unescape,
    },
    events::{
        BytesDecl,
        BytesText,
        Event,
    },
};

use crate::{
    helper::{
        const_str::{
            CONTENT_TYPES,
            PKG_MEDIA,
            REL_NS,
        },
        sheet_rename::SheetRename,
        formula::{
            FormulaToken,
            FormulaTokenSubTypes,
            FormulaTokenTypes,
            parse_to_tokens,
            render,
        },
    },
    reader::driver::{
        get_attribute,
        join_paths,
        zip_by_name,
    },
    structs::{
        WriterManager,
        XlsxError,
    },
    writer::driver::{
        write_end_tag,
        write_new_line,
        write_start_tag,
    },
};

/// How deep the parts a chart refers to are followed.
const MAX_DEPTH: usize = 8;

/// The folders of the parts kept as they were read (and what those refer to).
const KEPT_FOLDERS: [&str; 13] = [
    "xl/queryTables/",
    "xl/charts/",
    "xl/pivotTables/",
    "xl/pivotCache/",
    "xl/externalLinks/",
    "xl/slicerCaches/",
    "xl/slicers/",
    "xl/timelineCaches/",
    "xl/timelines/",
    "xl/ctrlProps/",
    "xl/activeX/",
    "xl/diagrams/",
    "customXml/",
];

/// The parts of the file read that the charts are made of: everything in
/// `xl/charts/` and what those refer to, with their content types. Read when
/// the workbook is opened, for the drawings of the sheets (which may be read
/// later) to take their charts from.
#[derive(Clone, Debug, Default)]
pub(crate) struct PartStore {
    /// Name in the package -> content.
    files:     HashMap<String, Vec<u8>>,
    /// `Override` of `[Content_Types].xml`: part name (`/xl/...`) -> type.
    overrides: HashMap<String, String>,
    /// `Default` of `[Content_Types].xml`: extension (lower case) -> type.
    defaults:  HashMap<String, String>,
}

/// Where parts kept may be named: the relationships part, the folder of the
/// parts it refers to, which relationships go to parts kept.
type Seed<'a> = (&'a str, &'a str, fn(&str) -> bool);

impl PartStore {
    pub(crate) fn read<R: Read + io::Seek>(arv: &mut zip::read::ZipArchive<R>) -> Self {
        let mut store = PartStore::default();
        let charts: Vec<String> = arv
            .file_names()
            .filter(|name| {
                KEPT_FOLDERS.iter().any(|folder| {
                    name.strip_prefix(folder).is_some_and(|rest| {
                        !rest.contains('/') && extension(rest).eq_ignore_ascii_case("xml")
                    })
                })
            })
            .map(str::to_string)
            .collect();
        // What elements of the sheets kept as read may refer to: the pictures
        // of the ActiveX controls (what Excel shows in their place: the `r:id`
        // of their `controlPr`) and the background picture (`picture`), the
        // printer settings of custom views, custom properties.
        let mut pictures = Vec::new();
        let sheet_rels: Vec<String> = arv
            .file_names()
            .filter(|n| n.starts_with("xl/worksheets/_rels/") && extension(n).eq_ignore_ascii_case("rels"))
            .map(str::to_string)
            .collect();
        for name in sheet_rels {
            let Some(rels) = read_part(arv, &name) else { continue };
            for rel in read_relationships(&rels) {
                if !rel.external && is_sheet_kept_target(&rel.rel_type) {
                    pictures.push((join_paths("xl/worksheets", &rel.target), 0));
                }
            }
        }
        // The parts of the workbook and of the package kept (in folders of
        // their own or not: `xl/connections.xml`, `docMetadata/LabelInfo.xml`),
        // the query tables of the tables.
        let seeds: [Seed<'_>; 2] = [
            ("xl/_rels/workbook.xml.rels", "xl", KeptRelationships::is_kept_type),
            ("_rels/.rels", "", is_package_kept_type),
        ];
        for (rels_name, dir, kept) in seeds {
            let Some(rels) = read_part(arv, rels_name) else { continue };
            for rel in read_relationships(&rels) {
                if !rel.external && kept(&rel.rel_type) {
                    pictures.push((join_part(dir, &rel.target), 0));
                }
            }
        }
        let table_rels: Vec<String> = arv
            .file_names()
            .filter(|n| n.starts_with("xl/tables/_rels/") && extension(n).eq_ignore_ascii_case("rels"))
            .map(str::to_string)
            .collect();
        for name in table_rels {
            let Some(rels) = read_part(arv, &name) else { continue };
            let list = read_relationships(&rels);
            if !list.iter().any(|r| KeptRelationships::is_kept_type(&r.rel_type)) {
                continue;
            }
            for rel in list.iter().filter(|r| !r.external && KeptRelationships::is_kept_type(&r.rel_type)) {
                pictures.push((join_paths("xl/tables", &rel.target), 0));
            }
            store.files.insert(name, rels);
        }
        if charts.is_empty() && pictures.is_empty() {
            return store;
        }
        if let Some(data) = read_part(arv, CONTENT_TYPES) {
            store.read_content_types(&data);
        }
        let mut pending: Vec<(String, usize)> = charts.into_iter().map(|c| (c, 0)).collect();
        pending.extend(pictures);
        while let Some((path, depth)) = pending.pop() {
            if store.files.contains_key(&path) {
                continue;
            }
            let Some(data) = read_part(arv, &path) else {
                continue;
            };
            store.files.insert(path.clone(), data);
            let rels_path = rels_path(&path);
            let Some(rels) = read_part(arv, &rels_path) else {
                continue;
            };
            if depth < MAX_DEPTH {
                for rel in read_relationships(&rels) {
                    if !rel.external {
                        pending.push((join_paths(directory(&path), &rel.target), depth + 1));
                    }
                }
            }
            store.files.insert(rels_path, rels);
        }
        store
    }

    fn read_content_types(&mut self, data: &[u8]) {
        let mut reader = Reader::from_reader(data);
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e) | Event::Empty(e)) => match e.local_name().into_inner() {
                    b"Override" => {
                        if let (Some(name), Some(t)) = (
                            get_attribute(&e, b"PartName"),
                            get_attribute(&e, b"ContentType"),
                        ) {
                            self.overrides.insert(name, t);
                        }
                    }
                    b"Default" => {
                        if let (Some(ext), Some(t)) = (
                            get_attribute(&e, b"Extension"),
                            get_attribute(&e, b"ContentType"),
                        ) {
                            self.defaults.insert(ext.to_ascii_lowercase(), t);
                        }
                    }
                    _ => {}
                },
                Ok(Event::Eof) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }

    fn content_type(&self, path: &str) -> String {
        if let Some(t) = self.overrides.get(&format!("/{path}")) {
            return t.clone();
        }
        let ext = extension(path).to_ascii_lowercase();
        if let Some(t) = self.defaults.get(&ext) {
            return t.clone();
        }
        if ext == "xml" {
            "application/xml".to_string()
        } else {
            "application/octet-stream".to_string()
        }
    }

    /// The relationships of the part at `path` (a table) of types kept, read
    /// from the file: (id, type, the part they go to).
    pub(crate) fn kept_relationships_of(&self, path: &str) -> Vec<(String, String, String)> {
        let Some(rels) = self.files.get(&rels_path(path)) else { return Vec::new() };
        read_relationships(rels)
            .into_iter()
            .filter(|r| !r.external && KeptRelationships::is_kept_type(&r.rel_type))
            .map(|r| {
                let target = join_paths(directory(path), &r.target);
                (r.id, r.rel_type, target)
            })
            .collect()
    }

    /// The part and the parts it refers to, when it is in the store.
    pub(crate) fn part(&self, path: &str) -> Option<KeptPart> {
        self.part_at(path, 0)
    }

    fn part_at(&self, path: &str, depth: usize) -> Option<KeptPart> {
        let data = self.files.get(path)?.clone();
        let mut relationships = Vec::new();
        if let Some(rels) = self.files.get(&rels_path(path)) {
            for rel in read_relationships(rels) {
                let part = if rel.external || depth >= MAX_DEPTH {
                    None
                } else {
                    self.part_at(&join_paths(directory(path), &rel.target), depth + 1)
                };
                relationships.push(KeptRelationship {
                    id:       rel.id,
                    rel_type: rel.rel_type,
                    target:   rel.target,
                    external: rel.external,
                    part:     part.map(Box::new),
                });
            }
        }
        Some(KeptPart {
            path: path.to_string(),
            content_type: self.content_type(path),
            data,
            relationships,
        })
    }
}

/// A part kept as it was read, with the parts it refers to.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeptPart {
    /// Its name in the file read (`xl/charts/chart1.xml`).
    path:          String,
    content_type:  String,
    data:          Vec<u8>,
    relationships: Vec<KeptRelationship>,
}

#[derive(Clone, Debug, Default)]
struct KeptRelationship {
    id:       String,
    rel_type: String,
    /// As it was in the file.
    target:   String,
    external: bool,
    /// The part it refers to, when that was in the file.
    part:     Option<Box<KeptPart>>,
}

impl KeptPart {
    /// Writes the part and the parts it refers to, each under its own name
    /// when that is free and under the next free one like it when not, and
    /// returns the name it was written under.
    pub(crate) fn write<W: io::Seek + io::Write>(
        &self,
        writer_mng: &mut WriterManager<W>,
    ) -> Result<String, XlsxError> {
        // A pivot cache is the workbook's and its pivot tables': written once.
        let shared = self.path.starts_with("xl/pivotCache/");
        if shared {
            if let Some(written) = writer_mng.shared_part(&self.path) {
                return Ok(written.to_string());
            }
        }
        let path = free_name(writer_mng, &self.path);
        if shared {
            writer_mng.set_shared_part(&self.path, &path);
        }
        writer_mng.add_bin(&path, &self.data)?;
        writer_mng.add_content_type(&path, &self.content_type);
        if self.relationships.is_empty() {
            return Ok(path);
        }

        let mut targets = Vec::with_capacity(self.relationships.len());
        for relationship in &self.relationships {
            targets.push(match &relationship.part {
                Some(part) => format!("/{}", part.write(writer_mng)?),
                None => relationship.target.clone(),
            });
        }

        let list: Vec<(&str, &str, &str, bool)> = self
            .relationships
            .iter()
            .zip(&targets)
            .map(|(r, target)| (r.id.as_str(), r.rel_type.as_str(), target.as_str(), r.external))
            .collect();
        write_relationships_part(writer_mng, &rels_path(&path), &list)?;
        Ok(path)
    }

    /// Moves the references to the cells of the sheet `sheet_name` (in the
    /// formulas of the series, `c:f` and `cx:f`) for rows and columns
    /// inserted (`remove` false) or removed there.
    pub(crate) fn adjust_references(&mut self, sheet_name: &str, shift: &Shift) {
        if extension(&self.path).eq_ignore_ascii_case("xml") {
            if let Some(data) = adjust_formulas(&self.data, sheet_name, shift) {
                self.data = data;
            }
        }
        for relationship in &mut self.relationships {
            if let Some(part) = &mut relationship.part {
                part.adjust_references(sheet_name, shift);
            }
        }
    }

    /// Renames a sheet in the formulas of the part and the parts it refers
    /// to (but external links: their sheets are another workbook's).
    pub(crate) fn rename_sheet(&mut self, rename: &SheetRename) {
        if self.path.starts_with("xl/externalLinks/") {
            return;
        }
        if extension(&self.path).eq_ignore_ascii_case("xml") {
            if let Some(data) = rename.xml(&self.data) {
                self.data = data;
            }
        }
        for relationship in &mut self.relationships {
            if let Some(part) = &mut relationship.part {
                part.rename_sheet(rename);
            }
        }
    }

    /// Rewrites the XML of the part and the parts it refers to (but
    /// external links: their sheets are another workbook's).
    pub(crate) fn rewrite_xml(&mut self, rw: &mut dyn crate::helper::rewrite::XmlRewrite) {
        if self.path.starts_with("xl/externalLinks/") {
            return;
        }
        if extension(&self.path).eq_ignore_ascii_case("xml") {
            if let Some(data) = crate::helper::rewrite::rewrite_xml(&self.data, rw) {
                self.data = data;
            }
        }
        for relationship in &mut self.relationships {
            if let Some(part) = &mut relationship.part {
                part.rewrite_xml(rw);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// The content as text.
    pub(crate) fn text(&self) -> std::borrow::Cow<'_, str> {
        String::from_utf8_lossy(&self.data)
    }

    /// Replaces `from` by `to` in the content.
    pub(crate) fn replace_text(&mut self, from: &str, to: &str) {
        let text = self.text();
        if text.contains(from) {
            self.data = text.replace(from, to).into_bytes();
        }
    }
}

/// Relationships of the workbook or of a sheet to parts the model has
/// nothing for (external links, slicers and timelines with their caches, the
/// properties of form controls, custom XML), kept with those parts, and the
/// elements of the workbook or sheet that refer to them (`externalReferences`,
/// `extLst`, `controls`...) as they were read. They are written back with ids
/// of their own (`rIdKept1`...), which take the place of the ids read in the
/// elements kept.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeptRelationships {
    /// (id read, type, the part).
    list: Vec<(String, String, KeptPart)>,
    /// (name of the element, its XML as read).
    xml:  Vec<(String, String)>,
    /// The namespaces the root of the part read declared (`xmlns:xr2`...):
    /// the elements kept may use them.
    namespaces: Vec<(String, String)>,
}

impl KeptRelationships {
    /// Whether the model leaves relationships of this type out (they are
    /// kept instead).
    pub(crate) fn is_kept_type(rel_type: &str) -> bool {
        matches!(
            rel_type.rsplit('/').next(),
            Some(
                "externalLink" | "slicerCache" | "timelineCache" | "customXml" | "slicer" | "timeline" | "ctrlProp" | "control"
                    | "pivotTable" | "pivotCacheDefinition" | "connections" | "queryTable" | "powerPivotData"
            )
        )
    }

    /// Keeps the relationship `id` to the part at `path` (in the package),
    /// when the store has it.
    pub(crate) fn add(&mut self, store: &PartStore, id: &str, rel_type: &str, path: &str) {
        if let Some(part) = store.part(path) {
            self.list.push((id.to_string(), rel_type.to_string(), part));
        }
    }

    /// Remembers the namespaces the root element `e` declares, but the
    /// default one (the kept elements are in the same).
    pub(crate) fn set_namespaces(&mut self, e: &quick_xml::events::BytesStart<'_>) {
        self.namespaces = root_namespaces(e);
    }

    /// Whether an element kept refers to the relationship `id` (`r:id`).
    pub(crate) fn refers_to(&self, id: &str) -> bool {
        let attr = format!("r:id=\"{id}\"");
        self.xml.iter().any(|(_, xml)| xml.contains(&attr))
    }

    pub(crate) fn add_xml(&mut self, name: &str, xml: String) {
        self.xml.push((name.to_string(), xml));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.list.is_empty() && self.xml.is_empty()
    }

    /// Renames a sheet in the parts and the elements kept.
    pub(crate) fn rename_sheet(&mut self, rename: &SheetRename) {
        for (_, _, part) in &mut self.list {
            part.rename_sheet(rename);
        }
        for (_, xml) in &mut self.xml {
            if let Some(new) = rename.xml(xml.as_bytes()) {
                *xml = String::from_utf8(new).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
            }
        }
    }

    /// Rewrites the parts and the elements kept.
    pub(crate) fn rewrite_xml(&mut self, rw: &mut dyn crate::helper::rewrite::XmlRewrite) {
        for (_, _, part) in &mut self.list {
            part.rewrite_xml(rw);
        }
        self.rewrite_elements(rw);
    }

    /// Rewrites the elements kept (not the parts).
    pub(crate) fn rewrite_elements(&mut self, rw: &mut dyn crate::helper::rewrite::XmlRewrite) {
        for (_, xml) in &mut self.xml {
            if let Some(new) = crate::helper::rewrite::rewrite_xml(xml.as_bytes(), rw) {
                *xml = String::from_utf8(new).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
                if let Some(kept) = crate::helper::rewrite::remove_emptied(xml) {
                    *xml = kept;
                }
            }
        }
    }

    /// Moves the areas and cells of the elements of a sheet kept as read
    /// (custom views, the ranges users may edit, the errors not to flag, the
    /// sort, scenarios, watches, smart tags) for rows or columns inserted or
    /// removed; takes away what goes with the cells removed, and an element
    /// left with nothing in it.
    pub(crate) fn shift_elements(&mut self, edit: &crate::helper::rewrite::LineEdit) {
        const MOVED: [&str; 7] =
            ["protectedRanges", "ignoredErrors", "customSheetViews", "sortState", "scenarios", "cellWatches", "smartTags"];
        self.xml.retain_mut(|(name, xml)| {
            if !MOVED.contains(&name.as_str()) {
                return true;
            }
            let mut rw = crate::helper::rewrite::KeptAreaXml::new(edit);
            if let Some(new) = crate::helper::rewrite::rewrite_xml(xml.as_bytes(), &mut rw) {
                *xml = String::from_utf8(new).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
            }
            if let Some(kept) = crate::helper::rewrite::remove_marked(xml) {
                *xml = kept;
            }
            // (These hold one or more of their children.)
            !matches!(name.as_str(), "protectedRanges" | "ignoredErrors" | "cellWatches" | "smartTags")
                || has_child(xml)
        });
    }

    /// The areas of the pivot tables kept (the `ref` of their `location`).
    pub(crate) fn pivot_locations(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (_, rel_type, part) in &self.list {
            if !rel_type.ends_with("/pivotTable") {
                continue;
            }
            let xml = String::from_utf8_lossy(&part.data);
            if let Some(i) = xml.find("<location ") {
                let rest = &xml[i..];
                let end = rest.find('>').unwrap_or(rest.len());
                if let Some(r) = rest[..end].split(" ref=\"").nth(1).and_then(|r| r.split('"').next()) {
                    out.push(r.to_string());
                }
            }
        }
        out
    }

    /// The id a relationship kept is written with.
    fn new_id(n: usize) -> String {
        format!("rIdKept{}", n + 1)
    }

    /// The elements kept with this name, with the ids they are written with.
    pub(crate) fn xml(&self, name: &str) -> Vec<String> {
        self.xml
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, xml)| self.with_new_ids(&self.with_namespaces(xml)))
            .collect()
    }

    /// The XML with the ids read (`r:id="rId3"`) replaced by those the
    /// relationships are written with, in one pass (an id read may be one
    /// written: `rIdKept2`).
    fn with_new_ids(&self, xml: &str) -> String {
        const ATTR: &str = "r:id=\"";
        let mut out = String::with_capacity(xml.len());
        let mut rest = xml;
        while let Some(i) = rest.find(ATTR) {
            let start = i + ATTR.len();
            out.push_str(&rest[..start]);
            rest = &rest[start..];
            let end = rest.find('"').unwrap_or(rest.len());
            let id = &rest[..end];
            match self.list.iter().position(|(read, _, _)| read == id) {
                Some(n) => out.push_str(&Self::new_id(n)),
                None => out.push_str(id),
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    fn with_namespaces(&self, xml: &str) -> String {
        declare_namespaces(xml, &self.namespaces)
    }

    /// Writes the parts; returns the relationships to write: (id, type,
    /// target).
    pub(crate) fn write_parts<W: io::Seek + io::Write>(
        &self,
        writer_mng: &mut WriterManager<W>,
    ) -> Result<Vec<(String, String, String)>, XlsxError> {
        let mut out = Vec::with_capacity(self.list.len());
        for (n, (_, rel_type, part)) in self.list.iter().enumerate() {
            let path = part.write(writer_mng)?;
            out.push((Self::new_id(n), rel_type.clone(), format!("/{path}")));
        }
        Ok(out)
    }
}

/// Whether the package's relationship (`_rels/.rels`) of this type goes to
/// a part the model has nothing for, kept as read: the sensitivity label
/// (`docMetadata/LabelInfo.xml`), the customized ribbon (`customUI`), add-ins
/// of the task pane... All but the workbook and its properties, which the
/// model writes, the thumbnail and the digital signatures (which would not
/// match what is written).
pub(crate) fn is_package_kept_type(rel_type: &str) -> bool {
    !matches!(
        rel_type.rsplit('/').next(),
        Some("officeDocument" | "core-properties" | "extended-properties" | "custom-properties" | "thumbnail" | "origin")
    )
}

/// The part a relationship of a part in `dir` goes to ("" for the package).
pub(crate) fn join_part(dir: &str, target: &str) -> String {
    if dir.is_empty() {
        normalize_part(target.trim_start_matches('/'))
    } else {
        join_paths(dir, target)
    }
}

fn normalize_part(path: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// The relationships of a sheet kept when an element kept as read refers to
/// them (`r:id`): pictures, printer settings (of custom views), custom
/// properties, the drawings of the pictures of headers and footers
/// (`legacyDrawingHF`). The model reads others of the same types for itself.
pub(crate) fn is_sheet_kept_target(rel_type: &str) -> bool {
    matches!(
        rel_type.rsplit('/').next(),
        Some("image" | "printerSettings" | "customProperty" | "vmlDrawing")
    )
}

/// Whether an element (as XML) has an element in it.
fn has_child(xml: &str) -> bool {
    let Some(end) = xml.find('>') else { return false };
    if xml[..end].ends_with('/') {
        return false;
    }
    xml[end + 1..].trim_start().find('<').is_some_and(|i| !xml[end + 1..].trim_start()[i..].starts_with("</"))
}

/// The namespaces (`xmlns:xr9`...) a root element declares, but the default one.
pub(crate) fn root_namespaces(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .filter(|a| a.key.as_ref().starts_with(b"xmlns:"))
        .map(|a| (String::from_utf8_lossy(a.key.as_ref()).to_string(), String::from_utf8_lossy(&a.value).to_string()))
        .collect()
}

/// An element kept as read (`xml`), declaring the namespaces of the root it
/// was read in that it uses and does not declare itself, for it to stand
/// alone in the part written.
pub(crate) fn declare_namespaces(xml: &str, namespaces: &[(String, String)]) -> String {
    let Some(end) = xml.find('>') else { return xml.to_string() };
    let end = if xml[..end].ends_with('/') { end - 1 } else { end };
    let (tag, rest) = xml.split_at(end);
    let mut out = tag.to_string();
    for (key, value) in namespaces {
        let prefix = &key["xmlns:".len()..];
        // As the prefix of an element or attribute, or named in an attribute (mc:Choice Requires="x14").
        let used = xml.contains(&format!("<{prefix}:")) || xml.contains(&format!(" {prefix}:")) || xml.contains(&format!("\"{prefix}\""));
        if used && !tag.contains(&format!("{key}=")) {
            out.push_str(&format!(" {key}=\"{value}\""));
        }
    }
    out.push_str(rest);
    out
}

/// Rows and columns inserted or removed: from `root_col` / `root_row`
/// (1-based), `col` / `row` of them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Shift {
    pub(crate) root_col: u32,
    pub(crate) col:      u32,
    pub(crate) root_row: u32,
    pub(crate) row:      u32,
    pub(crate) remove:   bool,
}

impl Shift {
    /// A column or row number (1-based) after the shift; `None` when it was
    /// removed. `end` is whether it is the end of a range, which, when
    /// removed, becomes the one before the removed ones (and the start the
    /// one after them).
    fn number(&self, num: u32, root: u32, offset: u32, end: bool) -> Option<u32> {
        if offset == 0 || root == 0 || num < root {
            return Some(num);
        }
        if !self.remove {
            return Some(num.saturating_add(offset));
        }
        if num >= root + offset {
            return Some(num - offset);
        }
        // Inside the removed ones.
        if end {
            if root > 1 { Some(root - 1) } else { None }
        } else {
            Some(root)
        }
    }

    pub(crate) fn col(&self, num: u32, end: bool) -> Option<u32> {
        self.number(num, self.root_col, self.col, end)
    }

    pub(crate) fn row(&self, num: u32, end: bool) -> Option<u32> {
        self.number(num, self.root_row, self.row, end)
    }

    /// For a position (0-based) of a drawing: removed ones go to the first
    /// one after them.
    pub(crate) fn marker(&self, num: u32, root: u32, offset: u32) -> u32 {
        self.number(num + 1, root, offset, false).unwrap_or(1) - 1
    }
}

/// The content with the text of the `f` elements (formulas of the series)
/// moved, or `None` when nothing changed.
fn adjust_formulas(data: &[u8], sheet_name: &str, shift: &Shift) -> Option<Vec<u8>> {
    // Leave parts with no formulas (`<c:f>`, `<f>`) as they are.
    if !data.windows(2).any(|w| w == b"f>") {
        return None;
    }
    let mut reader = Reader::from_reader(data);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut changed = false;
    // The text of the open `f` element (escaped), when one is open.
    let mut formula: Option<String> = None;
    loop {
        buf.clear();
        let event = match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(event) => event,
            Err(_) => return None,
        };
        if let Some(text) = &mut formula {
            match &event {
                Event::Text(t) => {
                    text.push_str(&String::from_utf8_lossy(t));
                    continue;
                }
                Event::GeneralRef(r) => {
                    text.push('&');
                    text.push_str(&String::from_utf8_lossy(r));
                    text.push(';');
                    continue;
                }
                Event::End(e) if e.local_name().into_inner() == b"f" => {
                    let raw = formula.take().unwrap_or_default();
                    let new = unescape(&raw)
                        .ok()
                        .and_then(|plain| adjust_formula(&plain, sheet_name, shift));
                    match new {
                        Some(new) => {
                            changed = true;
                            writer
                                .write_event(Event::Text(BytesText::from_escaped(partial_escape(
                                    &new,
                                ))))
                                .ok()?;
                        }
                        None => {
                            writer
                                .write_event(Event::Text(BytesText::from_escaped(raw)))
                                .ok()?;
                        }
                    }
                }
                _ => {
                    // Something else inside: keep the formula as it was.
                    let raw = formula.take().unwrap_or_default();
                    writer
                        .write_event(Event::Text(BytesText::from_escaped(raw)))
                        .ok()?;
                }
            }
        } else if let Event::Start(e) = &event {
            if e.local_name().into_inner() == b"f" {
                formula = Some(String::new());
            }
        }
        writer.write_event(event).ok()?;
    }
    changed.then(|| writer.into_inner().into_inner())
}

/// A formula (as in `c:f`: `Sheet1!$B$2:$B$5`, `(Sheet1!$A$1,Sheet1!$C$1)`)
/// with its references to the cells of the sheet moved, or `None` when
/// nothing changed. Unlike the formulas of the cells, absolute references
/// move too (as they do in Excel).
pub(crate) fn adjust_formula(formula: &str, sheet_name: &str, shift: &Shift) -> Option<String> {
    let mut tokens: Vec<FormulaToken> = parse_to_tokens(format!("={formula}"));
    let mut changed = false;
    for token in &mut tokens {
        if token.get_token_type() != &FormulaTokenTypes::Operand
            || token.get_token_sub_type() != &FormulaTokenSubTypes::Range
        {
            continue;
        }
        let value = token.get_value().to_string();
        let Some((sheet, range)) = value.rsplit_once('!') else {
            continue;
        };
        if unquote_sheet(sheet) != sheet_name {
            continue;
        }
        let new_range = adjust_range(range, shift);
        if new_range != range {
            changed = true;
            token.set_value(format!("{sheet}!{new_range}"));
        }
    }
    changed.then(|| render(&tokens))
}

fn unquote_sheet(sheet: &str) -> String {
    match sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => sheet.to_string(),
    }
}

/// `$B$2:$B$5`, `A1`, `$A:$A`, `3:3` after the shift; `#REF!` when what it
/// refers to was removed.
fn adjust_range(range: &str, shift: &Shift) -> String {
    let parts: Vec<&str> = range.split(':').collect();
    if parts.len() > 2 {
        return range.to_string();
    }
    let single = parts.len() == 1;
    let mut out = Vec::with_capacity(parts.len());
    let mut numbers = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let Some(cell) = CellRef::parse(part) else {
            return range.to_string();
        };
        let end = i == 1;
        let col = match cell.col {
            Some(c) => match shift.col(c, end) {
                Some(c) => Some(c),
                None => return "#REF!".to_string(),
            },
            None => None,
        };
        let row = match cell.row {
            Some(r) => match shift.row(r, end) {
                Some(r) => Some(r),
                None => return "#REF!".to_string(),
            },
            None => None,
        };
        // A single cell that was removed.
        if single && shift.remove {
            let gone = |num: Option<u32>, root: u32, offset: u32| {
                num.is_some_and(|n| offset != 0 && root != 0 && n >= root && n < root + offset)
            };
            if gone(cell.col, shift.root_col, shift.col)
                || gone(cell.row, shift.root_row, shift.row)
            {
                return "#REF!".to_string();
            }
        }
        numbers.push((col, row));
        out.push(CellRef { col, row, ..cell }.to_string());
    }
    // A range all of whose rows (or columns) were removed.
    if numbers.len() == 2 {
        let (start, end) = (numbers[0], numbers[1]);
        let reversed =
            |a: Option<u32>, b: Option<u32>| matches!((a, b), (Some(a), Some(b)) if a > b);
        if reversed(start.0, end.0) || reversed(start.1, end.1) {
            return "#REF!".to_string();
        }
    }
    out.join(":")
}

#[derive(Clone, Copy, Debug)]
struct CellRef {
    col_lock: bool,
    col:      Option<u32>,
    row_lock: bool,
    row:      Option<u32>,
}

impl CellRef {
    fn parse(text: &str) -> Option<CellRef> {
        let bytes = text.as_bytes();
        let mut i = 0;
        let col_lock = bytes.get(i) == Some(&b'$');
        if col_lock {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let letters = &text[start..i];
        let mut row_lock = false;
        if bytes.get(i) == Some(&b'$') {
            row_lock = true;
            i += 1;
        }
        let digits = &text[i..];
        if letters.len() > 3 || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        if letters.is_empty() && digits.is_empty() {
            return None;
        }
        let col = if letters.is_empty() {
            None
        } else {
            Some(crate::helper::coordinate::column_index_from_string(
                letters.to_ascii_uppercase(),
            ))
        };
        let row = if digits.is_empty() {
            None
        } else {
            Some(digits.parse().ok()?)
        };
        if letters.is_empty() && col_lock {
            // `$3` alone: the `$` is the row's.
            return Some(CellRef {
                col_lock: false,
                col,
                row_lock: true,
                row,
            });
        }
        Some(CellRef {
            col_lock,
            col,
            row_lock,
            row,
        })
    }
}

impl std::fmt::Display for CellRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(col) = self.col {
            if self.col_lock {
                f.write_str("$")?;
            }
            f.write_str(&crate::helper::coordinate::string_from_column_index(col))?;
        }
        if let Some(row) = self.row {
            if self.row_lock {
                f.write_str("$")?;
            }
            write!(f, "{row}")?;
        }
        Ok(())
    }
}

/// The name to write a part under: its own when it is free, or the next
/// free one like it (`chart3.xml` -> `chart4.xml`). Pictures get names of
/// their own (`xl/media/chartImage1.png`), as the pictures of the sheets are
/// written after the drawings and would take theirs.
fn free_name<W: io::Seek + io::Write>(writer_mng: &mut WriterManager<W>, path: &str) -> String {
    let dir = directory(path);
    let ext = extension(path);
    let name = file_name(path);
    let stem = name.strip_suffix(&format!(".{ext}")).unwrap_or(name);
    let is_media = dir == PKG_MEDIA;
    if !is_media && !writer_mng.check_file_exist(path) {
        return path.to_string();
    }
    let stem = if is_media {
        "chartImage"
    } else {
        stem.trim_end_matches(|c: char| c.is_ascii_digit())
    };
    let dot_ext = if ext.is_empty() {
        String::new()
    } else {
        format!(".{ext}")
    };
    let mut index = 0;
    loop {
        index += 1;
        let new_path = format!("{dir}/{stem}{index}{dot_ext}");
        if !writer_mng.check_file_exist(&new_path) {
            return new_path;
        }
    }
}

/// Writes a relationships part (`.rels`) at `path`: (id, type, target,
/// external).
pub(crate) fn write_relationships_part<W: io::Seek + io::Write>(
    writer_mng: &mut WriterManager<W>,
    path: &str,
    list: &[(&str, &str, &str, bool)],
) -> Result<(), XlsxError> {
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    writer
        .write_event(Event::Decl(BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))
        .unwrap();
    write_new_line(&mut writer);
    write_start_tag(
        &mut writer,
        "Relationships",
        vec![("xmlns", REL_NS).into()],
        false,
    );
    for (id, rel_type, target, external) in list {
        let mut attributes: crate::structs::AttrCollection = vec![
            ("Id", *id).into(),
            ("Type", *rel_type).into(),
            ("Target", *target).into(),
        ];
        if *external {
            attributes.push(("TargetMode", "External").into());
        }
        write_start_tag(&mut writer, "Relationship", attributes, true);
    }
    write_end_tag(&mut writer, "Relationships");
    writer_mng.add_writer(path, writer)
}

/// The name of the relationships part of a part (`xl/tables/table1.xml` ->
/// `xl/tables/_rels/table1.xml.rels`).
pub(crate) fn relationships_path(path: &str) -> String {
    rels_path(path)
}

pub(crate) fn read_part<R: Read + io::Seek>(
    arv: &mut zip::read::ZipArchive<R>,
    path: &str,
) -> Option<Vec<u8>> {
    let mut data = Vec::new();
    zip_by_name(arv, path).ok()?.read_to_end(&mut data).ok()?;
    Some(data)
}

fn directory(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn extension(path: &str) -> &str {
    file_name(path).rsplit_once('.').map_or("", |(_, ext)| ext)
}

fn rels_path(path: &str) -> String {
    let dir = directory(path);
    if dir.is_empty() {
        format!("_rels/{}.rels", file_name(path))
    } else {
        format!("{dir}/_rels/{}.rels", file_name(path))
    }
}

pub(crate) struct ReadRelationship {
    pub(crate) id:       String,
    pub(crate) rel_type: String,
    pub(crate) target:   String,
    pub(crate) external: bool,
}

pub(crate) fn read_relationships(data: &[u8]) -> Vec<ReadRelationship> {
    let mut reader = Reader::from_reader(data);
    reader.config_mut().trim_text(true);
    let mut list = Vec::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e) | Event::Empty(e)) => {
                if e.local_name().into_inner() != b"Relationship" {
                    continue;
                }
                let (Some(id), Some(rel_type), Some(target)) = (
                    get_attribute(&e, b"Id"),
                    get_attribute(&e, b"Type"),
                    get_attribute(&e, b"Target"),
                ) else {
                    continue;
                };
                let external = get_attribute(&e, b"TargetMode").as_deref() == Some("External");
                list.push(ReadRelationship {
                    id,
                    rel_type,
                    target,
                    external,
                });
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert_rows(root: u32, n: u32) -> Shift {
        Shift {
            root_col: 0,
            col:      0,
            root_row: root,
            row:      n,
            remove:   false,
        }
    }

    fn remove_rows(root: u32, n: u32) -> Shift {
        Shift {
            remove: true,
            ..insert_rows(root, n)
        }
    }

    #[test]
    fn formulas_of_series_move_with_rows() {
        let s = insert_rows(3, 2);
        assert_eq!(
            adjust_formula("Sheet1!$B$2:$B$5", "Sheet1", &s).as_deref(),
            Some("Sheet1!$B$2:$B$7")
        );
        assert_eq!(
            adjust_formula("'売上 表'!$A$4", "売上 表", &s).as_deref(),
            Some("'売上 表'!$A$6")
        );
        assert_eq!(adjust_formula("Other!$B$2:$B$5", "Sheet1", &s), None);
        assert_eq!(adjust_formula("Sheet1!$B$1", "Sheet1", &s), None);
        assert_eq!(
            adjust_formula("(Sheet1!$A$3,Sheet1!$C$1)", "Sheet1", &s).as_deref(),
            Some("(Sheet1!$A$5,Sheet1!$C$1)")
        );
    }

    #[test]
    fn formulas_of_series_shrink_with_removed_rows() {
        let s = remove_rows(3, 2);
        assert_eq!(
            adjust_formula("Sheet1!$B$2:$B$10", "Sheet1", &s).as_deref(),
            Some("Sheet1!$B$2:$B$8")
        );
        assert_eq!(
            adjust_formula("Sheet1!$B$3:$B$4", "Sheet1", &s).as_deref(),
            Some("Sheet1!#REF!")
        );
        assert_eq!(
            adjust_formula("Sheet1!$A$3", "Sheet1", &s).as_deref(),
            Some("Sheet1!#REF!")
        );
        assert_eq!(
            adjust_formula("Sheet1!$B$3:$B$9", "Sheet1", &s).as_deref(),
            Some("Sheet1!$B$3:$B$7")
        );
    }

    #[test]
    fn whole_columns_move_with_columns() {
        let s = Shift {
            root_col: 2,
            col:      1,
            root_row: 0,
            row:      0,
            remove:   false,
        };
        assert_eq!(
            adjust_formula("Sheet1!$B:$B", "Sheet1", &s).as_deref(),
            Some("Sheet1!$C:$C")
        );
        assert_eq!(
            adjust_formula("Sheet1!$A$1:$D$1", "Sheet1", &s).as_deref(),
            Some("Sheet1!$A$1:$E$1")
        );
    }

    #[test]
    fn only_the_text_of_the_formulas_changes() {
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            "\n",
            r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:ser>"#,
            r#"<c:tx><c:strRef><c:f>'A&amp;B'!$B$1</c:f></c:strRef></c:tx>"#,
            r#"<c:val><c:numRef><c:f>'A&amp;B'!$B$2:$B$4</c:f><c:numCache><c:formatCode>General</c:formatCode></c:numCache></c:numRef></c:val>"#,
            r#"</c:ser></c:chartSpace>"#,
        );
        let out = adjust_formulas(xml.as_bytes(), "A&B", &insert_rows(2, 1)).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert_eq!(out, xml.replace("$B$2:$B$4", "$B$3:$B$5"));
        assert!(adjust_formulas(xml.as_bytes(), "Other", &insert_rows(2, 1)).is_none());
    }
}
