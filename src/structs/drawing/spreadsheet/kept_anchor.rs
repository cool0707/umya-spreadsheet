//! An anchor of a drawing (`xdr:twoCellAnchor`, `xdr:oneCellAnchor`,
//! `xdr:absoluteAnchor`, or the `mc:AlternateContent` around one) that holds
//! a chart, kept as it was read with the parts the chart is made of. See
//! `structs::kept_parts`.

use std::io::{
    self,
    Cursor,
};

use quick_xml::{
    Reader,
    Writer,
    events::{
        BytesText,
        Event,
    },
};

use crate::structs::{
    Chart,
    WriterManager,
    XlsxError,
    drawing::spreadsheet::TwoCellAnchor,
    kept_parts::{
        KeptPart,
        PartStore,
        Shift,
    },
    raw::RawRelationships,
};

/// The `uri` of the `a:graphicData` of a chart, of a chart of the newer
/// kinds (chartEx: waterfall, treemap, sunburst, histogram, box and whisker,
/// funnel, map), of the frames of slicers (of a pivot table, of a table)
/// and timelines, and of SmartArt diagrams, which the model has no part for
/// either.
const CHART_URIS: [&[u8]; 6] = [
    b"http://schemas.openxmlformats.org/drawingml/2006/chart",
    b"http://schemas.microsoft.com/office/drawing/2014/chartex",
    b"http://schemas.microsoft.com/office/drawing/2010/slicer",
    b"http://schemas.microsoft.com/office/drawing/2012/slicer",
    b"http://schemas.microsoft.com/office/drawing/2012/timeslicer",
    b"http://schemas.openxmlformats.org/drawingml/2006/diagram",
];

/// What stands for the `n`-th relationship in the XML kept: replaced by the
/// id it is written with.
fn placeholder(n: usize) -> String {
    format!("\u{1}{n}\u{1}")
}

#[derive(Clone, Debug, Default)]
pub struct KeptAnchor {
    /// The element as it was read, declaring the namespaces it uses, with
    /// the ids of its relationships replaced by placeholders.
    xml:           String,
    relationships: Vec<KeptAnchorRelationship>,
}

#[derive(Clone, Debug, Default)]
struct KeptAnchorRelationship {
    /// Its id in the file read.
    id:       String,
    rel_type: String,
    target:   String,
    external: bool,
    part:     Option<KeptPart>,
}

/// The relationships of a drawing, as read: id -> (type, target, external,
/// name in the package).
type DrawingRelationships = Vec<(String, String, String, bool, String)>;

fn drawing_relationships(rels: Option<&RawRelationships>) -> DrawingRelationships {
    let Some(rels) = rels else {
        return Vec::new();
    };
    let dir = rels
        .file_target()
        .rsplit_once("/_rels/")
        .map_or("", |(dir, _)| dir)
        .to_string();
    rels.relationship_list()
        .iter()
        .map(|r| {
            let external = r.target_mode() == "External";
            let path = if external {
                String::new()
            } else {
                crate::reader::driver::join_paths(&dir, r.target())
            };
            (
                r.id().to_string(),
                r.get_type().to_string(),
                r.target().to_string(),
                external,
                path,
            )
        })
        .collect()
}

/// Takes the anchors that hold charts out of a drawing (`xl/drawings/
/// drawing1.xml`): returns the drawing without them, for the rest to be read
/// into the model, and the anchors. `None` when there is no chart in it.
pub(crate) fn take_charts(
    data: &[u8],
    rels: Option<&RawRelationships>,
    store: &PartStore,
) -> Option<(Vec<u8>, Vec<KeptAnchor>)> {
    if !CHART_URIS
        .iter()
        .any(|uri| data.windows(uri.len()).any(|w| w == *uri))
    {
        return None;
    }
    let relationships = drawing_relationships(rels);
    let mut reader = Reader::from_reader(data);
    let mut buf = Vec::new();
    let mut depth = 0usize;
    // The namespaces declared on the root: (attribute, value).
    let mut root_namespaces: Vec<(String, String)> = Vec::new();
    // The child of the root being read: where it starts, whether it holds a
    // chart, the attributes that refer to relationships.
    let mut child_start = 0u64;
    let mut has_chart = false;
    let mut rel_attributes: Vec<(String, String)> = Vec::new();
    let mut taken: Vec<(usize, usize, KeptAnchor)> = Vec::new();

    loop {
        let before = reader.buffer_position();
        buf.clear();
        let event = match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Ok(event) => event,
            Err(_) => return None,
        };
        match &event {
            Event::Start(e) | Event::Empty(e) => {
                let empty = matches!(event, Event::Empty(_));
                if depth == 0 {
                    for a in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(a.key.as_ref()).to_string();
                        if key == "xmlns" || key.starts_with("xmlns:") {
                            // As written (escaped).
                            let value = String::from_utf8_lossy(&a.value).to_string();
                            root_namespaces.push((key, value));
                        }
                    }
                } else {
                    if depth == 1 {
                        child_start = before;
                        has_chart = false;
                        rel_attributes.clear();
                    }
                    if e.local_name().into_inner() == b"graphicData" {
                        if let Some(uri) =
                            e.attributes().flatten().find(|a| a.key.as_ref() == b"uri")
                        {
                            if CHART_URIS.contains(&uri.value.as_ref()) {
                                has_chart = true;
                            }
                        }
                    }
                    for a in e.attributes().flatten() {
                        let key = a.key.as_ref();
                        // r:id, r:embed, r:link, ... (prefixed attributes whose
                        // value is the id of a relationship of the drawing).
                        if key.starts_with(b"xmlns") || !key.contains(&b':') {
                            continue;
                        }
                        let value = String::from_utf8_lossy(&a.value).to_string();
                        if relationships.iter().any(|r| r.0 == value) {
                            rel_attributes.push((String::from_utf8_lossy(key).to_string(), value));
                        }
                    }
                }
                if !empty {
                    depth += 1;
                } else if depth == 1 && has_chart {
                    let end = reader.buffer_position();
                    // (One that refers to a part the file does not have is
                    // left to the model.)
                    if let Some(anchor) = KeptAnchor::new(
                        &data[usize::try_from(child_start).ok()?..usize::try_from(end).ok()?],
                        &root_namespaces,
                        &rel_attributes,
                        &relationships,
                        store,
                    ) {
                        taken.push((
                            usize::try_from(child_start).ok()?,
                            usize::try_from(end).ok()?,
                            anchor,
                        ));
                    }
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 1 && has_chart {
                    let end = reader.buffer_position();
                    let start = usize::try_from(child_start).ok()?;
                    let end = usize::try_from(end).ok()?;
                    if let Some(anchor) = KeptAnchor::new(
                        &data[start..end],
                        &root_namespaces,
                        &rel_attributes,
                        &relationships,
                        store,
                    ) {
                        taken.push((start, end, anchor));
                    }
                    has_chart = false;
                }
            }
            _ => {}
        }
    }
    if taken.is_empty() {
        return None;
    }
    let mut rest = Vec::with_capacity(data.len());
    let mut at = 0;
    let mut anchors = Vec::with_capacity(taken.len());
    for (start, end, anchor) in taken {
        rest.extend_from_slice(&data[at..start]);
        at = end;
        anchors.push(anchor);
    }
    rest.extend_from_slice(&data[at..]);
    Some((rest, anchors))
}

impl KeptAnchor {
    fn new(
        raw: &[u8],
        root_namespaces: &[(String, String)],
        rel_attributes: &[(String, String)],
        relationships: &DrawingRelationships,
        store: &PartStore,
    ) -> Option<Self> {
        let mut xml = std::str::from_utf8(raw).ok()?.to_string();

        // Declare on the element the namespaces it has from the root (that
        // it does not declare itself), for it to stand alone.
        let tag_end = xml.find(|c: char| c.is_whitespace() || c == '>' || c == '/')?;
        let start_tag_end = xml.find('>')?;
        let start_tag = &xml[..start_tag_end];
        let mut declarations = String::new();
        for (key, value) in root_namespaces {
            let declared =
                start_tag.contains(&format!("{key}=\"")) || start_tag.contains(&format!("{key}='"));
            if !declared {
                declarations.push(' ');
                declarations.push_str(key);
                declarations.push_str("=\"");
                declarations.push_str(value);
                declarations.push('"');
            }
        }
        xml.insert_str(tag_end, &declarations);

        // The relationships it refers to.
        let mut list: Vec<KeptAnchorRelationship> = Vec::new();
        let mut ids: Vec<&str> = Vec::new();
        for (key, id) in rel_attributes {
            let n = if let Some(n) = ids.iter().position(|i| i == id) {
                n
            } else {
                let (_, rel_type, target, external, path) =
                    relationships.iter().find(|r| &r.0 == id)?;
                let part = if *external {
                    None
                } else {
                    Some(store.part(path)?)
                };
                list.push(KeptAnchorRelationship {
                    id: id.clone(),
                    rel_type: rel_type.clone(),
                    target: target.clone(),
                    external: *external,
                    part,
                });
                ids.push(id);
                ids.len() - 1
            };
            for quote in ['"', '\''] {
                xml = xml.replace(
                    &format!("{key}={quote}{id}{quote}"),
                    &format!("{key}=\"{}\"", placeholder(n)),
                );
            }
        }
        Some(KeptAnchor {
            xml,
            relationships: list,
        })
    }

    /// The chart as the model of this crate has it, for an anchor of two
    /// cells holding a chart of a kind it knows (keeping this anchor for
    /// writing while the model is not changed). `None` for the others, which
    /// are only kept.
    pub(crate) fn into_model_chart(self, rels: Option<&RawRelationships>) -> Result<Chart, Self> {
        let mut xml = self.xml.clone();
        for (n, r) in self.relationships.iter().enumerate() {
            xml = xml.replace(&placeholder(n), &r.id);
        }
        let mut reader = Reader::from_str(&xml);
        reader.config_mut().trim_text(true);
        let start = match reader.read_event() {
            Ok(Event::Start(e)) => e.into_owned(),
            _ => return Err(self),
        };
        if start.local_name().into_inner() != b"twoCellAnchor" {
            return Err(self);
        }
        // The model reads the elements by their names with the prefix that
        // Excel uses (or none).
        let prefix = start.name().prefix().map(|p| p.into_inner().to_vec());
        if !matches!(prefix.as_deref(), None | Some(b"xdr")) {
            return Err(self);
        }
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut anchor = TwoCellAnchor::default();
            anchor.set_attributes(&mut reader, &start, rels);
            anchor
        }));
        let Ok(anchor) = read else {
            return Err(self);
        };
        if !anchor.is_support() || !anchor.is_chart() {
            return Err(self);
        }
        let mut chart = Chart::default();
        chart.set_two_cell_anchor(anchor);
        chart.set_kept(self);
        Ok(chart)
    }

    /// Writes the parts of the chart; returns, for each relationship of the
    /// anchor, (type, target, external) to write in the relationships of
    /// the drawing.
    pub(crate) fn write_parts<W: io::Seek + io::Write>(
        &self,
        writer_mng: &mut WriterManager<W>,
    ) -> Result<Vec<(String, String, bool)>, XlsxError> {
        let mut list = Vec::with_capacity(self.relationships.len());
        for r in &self.relationships {
            let target = match &r.part {
                Some(part) => format!("/{}", part.write(writer_mng)?),
                None => r.target.clone(),
            };
            list.push((r.rel_type.clone(), target, r.external));
        }
        Ok(list)
    }

    /// Writes the anchor; `ids` are the ids its relationships are written
    /// with.
    pub(crate) fn write_to(&self, writer: &mut Writer<Cursor<Vec<u8>>>, ids: &[String]) {
        let mut xml = self.xml.clone();
        for (n, id) in ids.iter().enumerate() {
            xml = xml.replace(&placeholder(n), id);
        }
        writer
            .write_event(Event::Text(BytesText::from_escaped(xml)))
            .unwrap();
    }

    /// Moves the anchor for rows and columns inserted or removed in its
    /// sheet (the cells of `xdr:from` and `xdr:to`).
    pub(crate) fn move_position(&mut self, shift: &Shift) {
        if let Some(xml) = move_markers(&self.xml, shift) {
            self.xml = xml;
        }
    }

    /// Whether the anchor goes with rows or columns removed: when all of it
    /// is in them.
    pub(crate) fn is_removed(&self, shift: &Shift) -> bool {
        let markers = markers(&self.xml);
        let Some(from) = markers.iter().find(|m| m.0 == "from") else {
            return false;
        };
        let gone = |num: u32, root: u32, offset: u32| {
            offset != 0 && root != 0 && num + 1 >= root && num + 1 < root + offset
        };
        let in_removed = |m: &(String, u32, u32)| {
            gone(m.1, shift.root_col, shift.col) || gone(m.2, shift.root_row, shift.row)
        };
        match markers.iter().find(|m| m.0 == "to") {
            Some(to) => {
                // Both corners in the same removed columns or rows.
                (gone(from.1, shift.root_col, shift.col) && gone(to.1, shift.root_col, shift.col))
                    || (gone(from.2, shift.root_row, shift.row)
                        && gone(to.2, shift.root_row, shift.row))
            }
            None => in_removed(from),
        }
    }

    /// Moves the references of the chart to the cells of `sheet_name`.
    pub(crate) fn adjust_references(&mut self, sheet_name: &str, shift: &Shift) {
        for r in &mut self.relationships {
            if let Some(part) = &mut r.part {
                part.adjust_references(sheet_name, shift);
            }
        }
    }

    /// Renames a sheet in the references of the chart.
    pub(crate) fn rename_sheet(&mut self, rename: &crate::helper::sheet_rename::SheetRename) {
        for r in &mut self.relationships {
            if let Some(part) = &mut r.part {
                part.rename_sheet(rename);
            }
        }
    }

    /// Rewrites the parts the anchor refers to.
    pub(crate) fn rewrite_xml(&mut self, rw: &mut dyn crate::helper::rewrite::XmlRewrite) {
        for r in &mut self.relationships {
            if let Some(part) = &mut r.part {
                part.rewrite_xml(rw);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn xml(&self) -> &str {
        &self.xml
    }

    #[cfg(test)]
    pub(crate) fn part(&self, n: usize) -> Option<&KeptPart> {
        self.relationships.get(n)?.part.as_ref()
    }
}

/// The cells of the `from` and `to` of the anchors in the XML: (name, col,
/// row), 0-based.
fn markers(xml: &str) -> Vec<(String, u32, u32)> {
    let mut list = Vec::new();
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut path: Vec<Vec<u8>> = Vec::new();
    let mut current: Option<(String, u32, u32)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.local_name().into_inner().to_vec();
                if matches!(name.as_slice(), b"from" | b"to") && current.is_none() {
                    current = Some((String::from_utf8_lossy(&name).to_string(), 0, 0));
                }
                path.push(name);
            }
            Ok(Event::Text(t)) => {
                if let (Some(m), Some(last)) = (&mut current, path.last()) {
                    let value = String::from_utf8_lossy(&t).trim().parse().unwrap_or(0);
                    match last.as_slice() {
                        b"col" => m.1 = value,
                        b"row" => m.2 = value,
                        _ => {}
                    }
                }
            }
            Ok(Event::End(e)) => {
                path.pop();
                if matches!(e.local_name().into_inner(), b"from" | b"to") {
                    if let Some(m) = current.take() {
                        list.push(m);
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    list
}

/// The XML with the cells of `from` and `to` moved, or `None` when nothing
/// changed.
fn move_markers(xml: &str, shift: &Shift) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut path: Vec<Vec<u8>> = Vec::new();
    let mut in_marker = false;
    let mut changed = false;
    loop {
        let event = match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(event) => event,
            Err(_) => return None,
        };
        match &event {
            Event::Start(e) => {
                let name = e.local_name().into_inner().to_vec();
                if matches!(name.as_slice(), b"from" | b"to") {
                    in_marker = true;
                }
                path.push(name);
            }
            Event::End(e) => {
                path.pop();
                if matches!(e.local_name().into_inner(), b"from" | b"to") {
                    in_marker = false;
                }
            }
            Event::Text(t) if in_marker => {
                let (root, offset) = match path.last().map(Vec::as_slice) {
                    Some(b"col") => (shift.root_col, shift.col),
                    Some(b"row") => (shift.root_row, shift.row),
                    _ => (0, 0),
                };
                if offset != 0 {
                    if let Ok(num) = String::from_utf8_lossy(t).trim().parse::<u32>() {
                        let new = shift.marker(num, root, offset);
                        if new != num {
                            changed = true;
                            writer
                                .write_event(Event::Text(BytesText::new(&new.to_string())))
                                .ok()?;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        writer.write_event(event).ok()?;
    }
    if !changed {
        return None;
    }
    String::from_utf8(writer.into_inner().into_inner()).ok()
}

/// The relationships of a drawing written by this crate that refer to the
/// kept parts are listed in `rel_list` as ("KEPT", n): the n-th of
/// `kept_targets`.
pub(crate) const KEPT: &str = "KEPT";
