//! The controls of a sheet, read from its parts: form controls (buttons, check
//! boxes, option buttons, list and combo boxes, spin buttons, scroll bars,
//! labels, group boxes, edit boxes), ActiveX controls (Microsoft Forms 2.0:
//! CommandButton, CheckBox, TextBox...) and embedded objects, with what they
//! show and the cells they are linked to. Only read: the structs of this
//! crate keep the parts of the controls as they were read and write them back
//! (`ctrlProps`, `activeX`, the sheet's `controls`), and a program draws the
//! controls itself.
//!
//! A control is in several places of the file: the sheet's `<controls>` (its
//! name, where it is, its properties: `xl/ctrlProps/*.xml` for a form
//! control, `xl/activeX/*.xml` and the persisted control for an ActiveX one),
//! the sheet's VML drawing (where it is, its text and font, its state: what
//! Excel 2007 reads) and, from Excel 2010 on, a hidden mirror in the sheet's
//! drawing (its place among the shapes, its text: [`Control::apply_mirror`]).
//! The ActiveX controls are read from their persisted data (MS-OFORMS): their
//! caption, value, colors and font.

// (Names of Excel and of MS-OFORMS in the documentation; a text and a
// control have their flags.)
#![allow(clippy::doc_markdown, clippy::struct_excessive_bools)]

use std::collections::HashMap;

use super::{
    EMU_PX,
    anchor::{
        Anchor,
        anchor_of,
    },
    color::color_of,
    package::Package,
};
use crate::xml_tree::Node;

/// 1025 of a shape id "_x0000_s1025" (VML, the mirror's `compatExt`), or of
/// "1025".
#[must_use]
pub fn spid_number(s: &str) -> Option<u32> {
    let digits = s.rsplit(|c: char| !c.is_ascii_digit()).next()?;
    digits.parse().ok()
}

/// A text as a control shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlText {
    pub text:      String,
    pub font:      Option<String>,
    /// In points.
    pub size:      Option<f64>,
    /// `#RRGGBB`.
    pub color:     Option<String>,
    pub bold:      bool,
    pub italic:    bool,
    pub underline: bool,
    pub strike:    bool,
    /// "left", "center", "right".
    pub align:     Option<&'static str>,
    /// "top", "center", "bottom".
    pub valign:    Option<&'static str>,
}

/// What an ActiveX control (`xl/activeX/activeXN.xml` and its persisted
/// data) says of itself.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ActiveX {
    /// The class of the control as it is read: "CommandButton", "Label",
    /// "MorphData" (text box, list and combo boxes, check box, option and
    /// toggle buttons: see `display`), "SpinButton", "ScrollBar"; empty for
    /// another one.
    pub class:       &'static str,
    pub caption:     Option<String>,
    pub value:       Option<String>,
    /// `#RRGGBB`.
    pub fore:        Option<String>,
    pub back:        Option<String>,
    /// The back is drawn (BackStyle opaque).
    pub opaque:      bool,
    pub font:        Option<String>,
    /// In points.
    pub size:        Option<f64>,
    pub bold:        bool,
    pub italic:      bool,
    pub underline:   bool,
    pub align:       Option<&'static str>,
    /// `MorphData`'s `DisplayStyle` (1 text, 2 list, 3 combo, 4 check box, 5
    /// option button, 6 toggle button, 7 drop-down list).
    pub display:     u8,
    pub min:         i64,
    pub max:         i64,
    pub position:    i64,
    /// Orientation: 0 vertical, 1 horizontal.
    pub orientation: i64,
}

impl ActiveX {
    /// The Microsoft Forms 2.0 type of the control (`TextBox`; empty for
    /// another control).
    #[must_use]
    pub fn msforms_type(&self) -> &'static str {
        match (self.class, self.display) {
            ("MorphData", 1) => "TextBox",
            ("MorphData", 2) => "ListBox",
            ("MorphData", 3) => "ComboBox",
            ("MorphData", 4) => "CheckBox",
            ("MorphData", 5) => "OptionButton",
            ("MorphData", 6) => "ToggleButton",
            ("MorphData", _) => "",
            (class, _) => class,
        }
    }

    /// Whether the value is on (a check box, an option or toggle button).
    #[must_use]
    pub fn is_on(&self) -> bool {
        matches!(
            self.value.as_deref().map(str::to_ascii_lowercase).as_deref(),
            Some("1" | "-1" | "true")
        )
    }
}

/// The state of a check box or an option button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    /// Neither (a check box linked to a cell holding `#N/A`).
    Mixed,
}

/// A control of a sheet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Control {
    /// The id of its shape (1025 of `_x0000_s1025`), which the sheet's
    /// `controls`, its VML and the mirror in its drawing share.
    pub shape_id: u32,
    pub name:     String,
    /// The form control's object type ("Button", "CheckBox", "Radio",
    /// "Drop", "List", "Spin", "Scroll", "Label", "GBox", "EditBox"...),
    /// "ActiveX", or "Pict" for an embedded object.
    pub kind:     String,
    pub anchor:   Option<Anchor>,
    /// The form control's properties (the attributes of `formControlPr`:
    /// `fmlaLink`, `checked`, `val`...).
    pub props:    HashMap<String, String>,
    /// The values of the VML shape's `x:ClientData` (`Checked`, `FmlaLink`,
    /// `FmlaMacro`...).
    pub client:   HashMap<String, String>,
    pub text:     Option<ControlText>,
    pub activex:  Option<ActiveX>,
    /// The part of the picture Excel shows for it (an embedded object's, an
    /// ActiveX control's preview).
    pub image:    Option<String>,
    /// Hidden (`visibility:hidden` in VML).
    pub hidden:   bool,
}

impl Control {
    /// A property of the form control, from `formControlPr` (`prop`) or else
    /// from VML (`client`); None when empty.
    #[must_use]
    pub fn property(&self, prop: &str, client: &str) -> Option<&str> {
        self.props
            .get(prop)
            .or_else(|| self.client.get(client))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    fn number(&self, prop: &str, client: &str) -> Option<i64> {
        self.property(prop, client).and_then(|v| v.trim().parse().ok())
    }

    /// The cell the control is linked to (`$A$1`, `Sheet1!$B$2`).
    #[must_use]
    pub fn linked_cell(&self) -> Option<&str> {
        self.property("fmlaLink", "FmlaLink")
    }

    /// The range a list or combo box takes its items from.
    #[must_use]
    pub fn list_range(&self) -> Option<&str> {
        self.property("fmlaRange", "FmlaRange")
    }

    /// The macro it runs (without the `[0]!` of the workbook).
    #[must_use]
    pub fn macro_name(&self) -> Option<&str> {
        self.client
            .get("FmlaMacro")
            .map(|m| m.trim_start_matches("[0]!"))
            .filter(|m| !m.is_empty())
    }

    /// The state saved with a check box or an option button (that of a cell
    /// it is linked to is the cell's).
    #[must_use]
    pub fn checked(&self) -> CheckState {
        match self.property("checked", "Checked") {
            Some("Checked" | "1") => CheckState::Checked,
            Some("Mixed" | "2") => CheckState::Mixed,
            _ => CheckState::Unchecked,
        }
    }

    /// The item selected in a list or combo box (1-based; 0 none).
    #[must_use]
    pub fn selected(&self) -> i64 {
        self.number("sel", "Sel").unwrap_or(0)
    }

    /// The value of a spin button or scroll bar, and its least and greatest.
    #[must_use]
    pub fn value_range(&self) -> (i64, i64, i64) {
        (
            self.number("val", "Val").unwrap_or(0),
            self.number("min", "Min").unwrap_or(0),
            self.number("max", "Max").unwrap_or(100),
        )
    }

    /// Whether a spin button or scroll bar is horizontal.
    #[must_use]
    pub fn is_horizontal(&self) -> bool {
        self.props.get("horiz").is_some_and(|h| h == "1" || h == "true") || self.client.contains_key("Horiz")
    }

    /// Whether the control is drawn flat (without 3D shading).
    #[must_use]
    pub fn is_flat(&self) -> bool {
        self.props.contains_key("noThreeD") || self.client.contains_key("NoThreeD")
    }

    /// Takes what the hidden mirror of the control in the sheet's drawing
    /// (an `xdr:sp`) says: its name when the control has none, its text (read
    /// better from DrawingML than from VML), its macro. `theme`: the theme's
    /// colours ([`Package::theme_colors`]).
    pub fn apply_mirror(&mut self, sp: &Node, theme: &HashMap<String, String>) {
        if self.name.is_empty() {
            self.name = sp
                .find("cNvPr")
                .and_then(|x| x.attr("name"))
                .unwrap_or("")
                .to_string();
        }
        if self.kind != "ActiveX" {
            if let Some(t) = sp.child("txBody").and_then(|t| mirror_text(t, theme)) {
                let old = self.text.take().unwrap_or_default();
                self.text = Some(ControlText {
                    align: t.align.or(old.align),
                    valign: old.valign.or(t.valign),
                    ..t
                });
            }
        }
        if self.client.get("FmlaMacro").is_none_or(String::is_empty) {
            if let Some(m) = sp.attr("macro").filter(|m| !m.is_empty()) {
                self.client.insert("FmlaMacro".into(), m.to_string());
            }
        }
    }
}

/// The controls (and embedded objects) of the sheet whose part is
/// `sheet_part` ("xl/worksheets/sheet1.xml"), in the order of the sheet;
/// comments are left out. Where they are is the anchor of the sheet's
/// `controls` or of VML; the place of a control's mirror in the drawing, when
/// it has one, is better ([`Control::apply_mirror`] for the rest of it).
pub fn controls<P: Package + ?Sized>(pkg: &P, sheet_part: &str) -> Vec<Control> {
    // The control of the shape `id`, added when there is none yet.
    fn at(list: &mut Vec<Control>, id: u32, kind: &str) -> usize {
        if let Some(i) = list.iter().position(|x| x.shape_id == id) {
            return i;
        }
        list.push(Control {
            shape_id: id,
            kind: kind.to_string(),
            ..Default::default()
        });
        list.len() - 1
    }
    let mut list: Vec<Control> = Vec::new();
    let rels = pkg.rels(sheet_part);
    // The sheet's <controls>: the name, the anchor and the properties.
    if let Some(root) = pkg.sheet_element(sheet_part, "controls") {
        let mut found = Vec::new();
        root.find_all("control", &mut found);
        for c in found {
            let Some(id) = c.attr("shapeId").and_then(spid_number) else {
                continue;
            };
            let i = at(&mut list, id, "");
            let ctl = &mut list[i];
            if ctl.name.is_empty() {
                ctl.name = c.attr("name").unwrap_or("").to_string();
            }
            if ctl.anchor.is_none() {
                if let Some(a) = c.find("anchor").map(anchor_of).filter(|a| a.from.is_some()) {
                    ctl.anchor = Some(a);
                }
            }
            // The preview of an ActiveX control.
            if ctl.image.is_none() {
                if let Some((_, part)) = c.find("controlPr").and_then(|p| p.attr("id")).and_then(|r| rels.get(r)) {
                    ctl.image = Some(part.clone());
                }
            }
            let Some((kind, part)) = c.attr("id").and_then(|r| rels.get(r)) else {
                continue;
            };
            if kind.ends_with("/ctrlProp") {
                if let Some(p) = pkg.xml(part) {
                    ctl.props = p.attrs.iter().cloned().collect();
                    if let Some(t) = p.attr("objectType") {
                        ctl.kind = t.to_string();
                    }
                }
            } else if kind.ends_with("/control") && ctl.activex.is_none() {
                ctl.kind = "ActiveX".into();
                ctl.activex = Some(activex(pkg, part));
            }
        }
    }
    // Embedded objects (OLE): where they are and their pictures.
    if let Some(root) = pkg.sheet_element(sheet_part, "oleObjects") {
        let mut found = Vec::new();
        root.find_all("oleObject", &mut found);
        for o in found {
            let Some(id) = o.attr("shapeId").and_then(spid_number) else {
                continue;
            };
            let i = at(&mut list, id, "Pict");
            let ctl = &mut list[i];
            if ctl.name.is_empty() {
                ctl.name = o.attr("progId").unwrap_or("").to_string();
            }
            if let Some(pr) = o.child("objectPr") {
                if ctl.anchor.is_none() {
                    if let Some(a) = pr.find("anchor").map(anchor_of).filter(|a| a.from.is_some()) {
                        ctl.anchor = Some(a);
                    }
                }
                if ctl.image.is_none() {
                    if let Some((_, part)) = pr.attr("id").and_then(|r| rels.get(r)) {
                        ctl.image = Some(part.clone());
                    }
                }
            }
        }
    }
    // The VML drawing: anchors, texts, the state of form controls.
    let mut vml: Vec<String> = rels
        .values()
        .filter(|(t, _)| t.ends_with("/vmlDrawing"))
        .map(|(_, p)| p.clone())
        .collect();
    vml.sort();
    for part in vml {
        let Some(root) = pkg.xml(&part) else {
            continue;
        };
        let vml_rels = pkg.rels(&part);
        let mut shapes = Vec::new();
        root.find_all("shape", &mut shapes);
        for sh in shapes {
            let Some(data) = sh.child("ClientData") else {
                continue;
            };
            let object = data.attr("ObjectType").unwrap_or("");
            if object == "Note" {
                continue;
            }
            let Some(id) = sh.attr("spid").or_else(|| sh.attr("id")).and_then(spid_number) else {
                continue;
            };
            let i = at(&mut list, id, "");
            let ctl = &mut list[i];
            if ctl.kind.is_empty() {
                ctl.kind = form_kind(object).to_string();
            }
            ctl.client = data
                .children
                .iter()
                .map(|c| (c.name.clone(), c.all_text().trim().to_string()))
                .collect();
            ctl.hidden = sh
                .attr("style")
                .is_some_and(|s| s.replace(' ', "").contains("visibility:hidden"));
            if ctl.anchor.is_none() {
                ctl.anchor = ctl.client.get("Anchor").and_then(|a| vml_anchor(a));
            }
            if let Some(tb) = sh.child("textbox") {
                ctl.text = Some(vml_text(tb, &ctl.client));
            }
            // The picture of an object or an ActiveX control.
            if ctl.image.is_none() {
                if let Some((_, p)) = sh.child("imagedata").and_then(|d| d.attr("relid")).and_then(|r| vml_rels.get(r)) {
                    ctl.image = Some(p.clone());
                }
            }
        }
    }
    list
}

/// The object type of `formControlPr` for a VML `ObjectType`.
fn form_kind(vml: &str) -> &str {
    match vml {
        "Checkbox" => "CheckBox",
        "Edit" => "EditBox",
        other => other,
    }
}

/// A VML `x:Anchor` (columns and rows with offsets in pixels) as a drawing
/// anchor.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
fn vml_anchor(s: &str) -> Option<Anchor> {
    let v: Vec<i64> = s
        .split(',')
        .map(|x| x.trim().parse::<i64>())
        .collect::<Result<_, _>>()
        .ok()?;
    if v.len() != 8 || v.iter().any(|x| *x < 0) {
        return None;
    }
    let px = |p: i64| (p as f64 * EMU_PX) as i64;
    Some(Anchor {
        from: Some((v[0] as u32, px(v[1]), v[2] as u32, px(v[3]))),
        to: Some((v[4] as u32, px(v[5]), v[6] as u32, px(v[7]))),
        ..Default::default()
    })
}

fn align_of(s: &str) -> Option<&'static str> {
    match s.to_ascii_lowercase().as_str() {
        "left" | "l" | "justify" | "distributed" => Some("left"),
        "center" | "ctr" | "centered" => Some("center"),
        "right" | "r" => Some("right"),
        "top" | "t" => Some("top"),
        "bottom" | "b" => Some("bottom"),
        _ => None,
    }
}

/// The text of a VML text box: its lines (`div`) and the font of its first
/// run.
fn vml_text(tb: &Node, client: &HashMap<String, String>) -> ControlText {
    let mut divs = Vec::new();
    tb.find_all("div", &mut divs);
    let lines: Vec<String> = if divs.is_empty() {
        vec![tb.all_text()]
    } else {
        divs.iter().map(|d| d.all_text()).collect()
    };
    let mut t = ControlText {
        text: lines.join("\n").trim_matches(['\r', '\n']).to_string(),
        ..Default::default()
    };
    if let Some(f) = tb.find("font") {
        t.font = f.attr("face").map(str::to_string);
        // In twentieths of a point.
        t.size = f
            .attr("size")
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|v| *v > 0.0)
            .map(|v| v / 20.0);
        t.color = f
            .attr("color")
            .filter(|c| c.starts_with('#') && c.len() == 7)
            .map(str::to_string);
        t.bold = f.find("b").is_some();
        t.italic = f.find("i").is_some();
        t.underline = f.find("u").is_some();
        t.strike = f.find("s").is_some();
    }
    let style = divs.first().and_then(|d| d.attr("style")).unwrap_or("");
    t.align = style
        .split(';')
        .find_map(|p| p.trim().strip_prefix("text-align:"))
        .and_then(|a| align_of(a.trim()));
    if let Some(a) = client.get("TextHAlign").and_then(|a| align_of(a)) {
        t.align = Some(a);
    }
    t.valign = client.get("TextVAlign").and_then(|a| align_of(a));
    t
}

/// The text of a control's mirror (`txBody`): its paragraphs and the font of
/// its first run.
fn mirror_text(tx: &Node, theme: &HashMap<String, String>) -> Option<ControlText> {
    let mut lines = Vec::new();
    let mut first: Option<&Node> = None;
    for p in tx.all("p") {
        let mut line = String::new();
        for r in &p.children {
            match r.name.as_str() {
                "r" | "fld" => {
                    line.push_str(&r.child("t").map(|t| t.text.clone()).unwrap_or_default());
                    first = first.or(r.child("rPr"));
                }
                "br" => line.push('\n'),
                _ => {}
            }
        }
        lines.push(line);
    }
    let text = lines.join("\n");
    if text.trim().is_empty() {
        return None;
    }
    let mut t = ControlText {
        text,
        ..Default::default()
    };
    t.align = tx
        .all("p")
        .next()
        .and_then(|p| p.child("pPr"))
        .and_then(|p| p.attr("algn"))
        .and_then(align_of);
    t.valign = tx.child("bodyPr").and_then(|b| b.attr("anchor")).and_then(align_of);
    if let Some(r) = first {
        t.size = r.attr("sz").and_then(|v| v.parse::<f64>().ok()).map(|v| v / 100.0);
        t.bold = matches!(r.attr("b"), Some("1" | "true"));
        t.italic = matches!(r.attr("i"), Some("1" | "true"));
        t.underline = r.attr("u").is_some_and(|u| u != "none");
        t.strike = r.attr("strike").is_some_and(|s| s != "noStrike");
        t.font = ["ea", "latin"].iter().find_map(|k| {
            r.child(k)
                .and_then(|c| c.attr("typeface"))
                .filter(|f| !f.is_empty() && !f.starts_with('+'))
                .map(str::to_string)
        });
        t.color = r.child("solidFill").and_then(|f| color_of(f, theme));
    }
    Some(t)
}

/// An `OLE_COLOR` as `#RRGGBB`: a system color (0x80 in the high byte; as
/// Windows 10 has them) or 0x00BBGGRR.
fn ole_color(v: u32) -> String {
    if v & 0xff00_0000 == 0x8000_0000 {
        return match v & 0xff {
            0x00 => "#c8c8c8",
            0x04 | 0x0f => "#f0f0f0",
            0x05 | 0x0e | 0x14 => "#ffffff",
            0x06 => "#646464",
            0x0a => "#b4b4b4",
            0x0b => "#f4f7fc",
            0x0c => "#ababab",
            0x0d => "#0078d7",
            0x10 => "#a0a0a0",
            0x11 => "#6d6d6d",
            0x15 => "#696969",
            0x16 => "#e3e3e3",
            0x18 => "#ffffe1",
            _ => "#000000",
        }
        .to_string();
    }
    format!("#{:02x}{:02x}{:02x}", v & 0xff, (v >> 8) & 0xff, (v >> 16) & 0xff)
}

/// The class of an ActiveX control by its CLSID, with `MorphData`'s
/// `DisplayStyle` (`TextBox` 1, `ListBox` 2, `ComboBox` 3, `CheckBox` 4,
/// `OptionButton` 5, `ToggleButton` 6).
fn class_of(clsid: &str) -> (&'static str, u8) {
    match clsid.trim_matches(['{', '}']).to_ascii_uppercase().as_str() {
        "D7053240-CE69-11CD-A777-00DD01143C57" => ("CommandButton", 0),
        "978C9E23-D4B0-11CE-BF2D-00AA003F40D0" => ("Label", 0),
        "8BD21D10-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 1),
        "8BD21D20-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 2),
        "8BD21D30-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 3),
        "8BD21D40-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 4),
        "8BD21D50-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 5),
        "8BD21D60-EC42-11CE-9E0D-00AA006002F3" => ("MorphData", 6),
        "79176FB0-B7F2-11CE-97EF-00AA006D2776" => ("SpinButton", 0),
        "DFD181E0-5E2F-11CE-A449-00AA004A803D" => ("ScrollBar", 0),
        _ => ("", 0),
    }
}

/// What an ActiveX control (`xl/activeX/activeXN.xml`) says of itself.
fn activex<P: Package + ?Sized>(pkg: &P, part: &str) -> ActiveX {
    let Some(ocx) = pkg.xml(part) else {
        return ActiveX::default();
    };
    let (class, display) = class_of(ocx.attr("classid").unwrap_or(""));
    let mut ax = ActiveX {
        class,
        display,
        max: 100,
        ..Default::default()
    };
    // persistPropertyBag: the properties as they are.
    for p in ocx.all("ocxPr") {
        let v = p.attr("value").unwrap_or("").to_string();
        match p.attr("name").unwrap_or("") {
            "Caption" => ax.caption = Some(v),
            "Value" => ax.value = Some(v),
            _ => {}
        }
    }
    let bin = ocx
        .attr("id")
        .and_then(|id| pkg.rels(part).remove(id))
        .and_then(|(_, p)| pkg.bytes(&p));
    if let Some(data) = bin {
        // persistStream: the CLSID, then the control's stream (MS-OFORMS); a
        // storage (CFB) is not read.
        let stream = if data.len() > 16 && data[16..].starts_with(&[0, 2]) {
            &data[16..]
        } else {
            &data[..]
        };
        read_stream(&mut ax, stream);
    }
    ax
}

/// The persisted data of an ActiveX control (its stream, MS-OFORMS) read into
/// `ax`, whose class is known.
pub fn read_activex_stream(ax: &mut ActiveX, data: &[u8]) {
    read_stream(ax, data);
}

/// A field of a record's `DataBlock`: (mask bit, name, size).
type Field = (u8, &'static str, usize);

/// How a class's record is read: its fields, whether its mask has 64 bits,
/// the strings and sizes of its `ExtraDataBlock` (mask bit, name), the mask
/// bits of its pictures.
type Spec = (&'static [Field], bool, &'static [(u8, &'static str)], &'static [u8]);

const COMMAND_BUTTON: &[Field] = &[
    (0, "ForeColor", 4),
    (1, "BackColor", 4),
    (2, "VariousPropertyBits", 4),
    (3, "Caption", 4),
    (4, "PicturePosition", 4),
    (6, "MousePointer", 1),
    (7, "Picture", 2),
    (8, "Accelerator", 2),
    (10, "MouseIcon", 2),
];
const LABEL: &[Field] = &[
    (0, "ForeColor", 4),
    (1, "BackColor", 4),
    (2, "VariousPropertyBits", 4),
    (3, "Caption", 4),
    (4, "PicturePosition", 4),
    (6, "MousePointer", 1),
    (7, "BorderColor", 4),
    (8, "BorderStyle", 2),
    (9, "SpecialEffect", 2),
    (10, "Picture", 2),
    (11, "Accelerator", 2),
    (12, "MouseIcon", 2),
];
const MORPH_DATA: &[Field] = &[
    (0, "VariousPropertyBits", 4),
    (1, "BackColor", 4),
    (2, "ForeColor", 4),
    (3, "MaxLength", 4),
    (4, "BorderStyle", 1),
    (5, "ScrollBars", 1),
    (6, "DisplayStyle", 1),
    (7, "MousePointer", 1),
    (9, "PasswordChar", 2),
    (10, "ListWidth", 4),
    (11, "BoundColumn", 2),
    (12, "TextColumn", 2),
    (13, "ColumnCount", 2),
    (14, "ListRows", 2),
    (15, "cColumnInfo", 2),
    (16, "MatchEntry", 1),
    (17, "ListStyle", 1),
    (18, "ShowDropButtonWhen", 1),
    (20, "DropButtonStyle", 1),
    (21, "MultiSelect", 1),
    (22, "Value", 4),
    (23, "Caption", 4),
    (24, "PicturePosition", 4),
    (25, "BorderColor", 4),
    (26, "SpecialEffect", 4),
    (27, "MouseIcon", 2),
    (28, "Picture", 2),
    (29, "Accelerator", 2),
    (32, "GroupName", 4),
];
const SPIN_BUTTON: &[Field] = &[
    (0, "ForeColor", 4),
    (1, "BackColor", 4),
    (2, "VariousPropertyBits", 4),
    (5, "Min", 4),
    (6, "Max", 4),
    (7, "Position", 4),
    (8, "PrevEnabled", 4),
    (9, "NextEnabled", 4),
    (10, "SmallChange", 4),
    (11, "Orientation", 4),
];
const SCROLL_BAR: &[Field] = &[
    (0, "ForeColor", 4),
    (1, "BackColor", 4),
    (2, "VariousPropertyBits", 4),
    (4, "MousePointer", 1),
    (5, "Min", 4),
    (6, "Max", 4),
    (7, "Position", 4),
    (9, "PrevEnabled", 4),
    (10, "NextEnabled", 4),
    (11, "SmallChange", 4),
    (12, "LargeChange", 4),
    (13, "Orientation", 4),
];
const TEXT_PROPS: &[Field] = &[
    (0, "FontName", 4),
    (1, "FontEffects", 4),
    (2, "FontHeight", 4),
    (4, "FontCharSet", 1),
    (5, "FontPitchAndFamily", 1),
    (6, "ParagraphAlign", 1),
    (7, "FontWeight", 2),
];

/// A record read: its values, its strings (in the order of `strings`), where
/// it ends.
struct Record {
    mask:    u64,
    values:  HashMap<&'static str, u32>,
    strings: HashMap<&'static str, String>,
    /// Where the `StreamData` after the record starts.
    end:     usize,
}

/// Reads a record of MS-OFORMS (minor 0, major 2): its `DataBlock`, then the
/// strings of its `ExtraDataBlock` (`extra`: the strings and sizes in their
/// order, a size as "#Size").
fn record(data: &[u8], fields: &[Field], mask64: bool, extra: &[(u8, &'static str)]) -> Option<Record> {
    let u16_at = |i: usize| data.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |i: usize| data.get(i..i + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if data.len() < 8 || data[0] != 0 || data[1] != 2 {
        return None;
    }
    let cb = u16_at(2)? as usize;
    let end = 4 + cb;
    if end > data.len() {
        return None;
    }
    let mut mask = u64::from(u32_at(4)?);
    let mut pos: usize = 8;
    if mask64 {
        mask |= u64::from(u32_at(8)?) << 32;
        pos = 12;
    }
    let mut values = HashMap::new();
    for &(bit, name, size) in fields {
        if mask & (1u64 << bit) == 0 {
            continue;
        }
        pos = pos.next_multiple_of(size);
        let v = match size {
            1 => u32::from(*data.get(pos)?),
            2 => u32::from(u16_at(pos)?),
            _ => u32_at(pos)?,
        };
        values.insert(name, v);
        pos += size;
    }
    pos = pos.next_multiple_of(4);
    let mut strings = HashMap::new();
    for &(bit, name) in extra {
        if mask & (1u64 << bit) == 0 {
            continue;
        }
        if name == "#Size" {
            pos += 8;
            continue;
        }
        let packed = values.get(name).copied().unwrap_or(0);
        let len = (packed & 0x7fff_ffff) as usize;
        let raw = data.get(pos..pos.checked_add(len)?)?;
        let s = if packed & 0x8000_0000 != 0 {
            raw.iter().map(|b| char::from(*b)).collect()
        } else {
            let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&units)
        };
        strings.insert(name, s);
        pos = (pos + len).next_multiple_of(4);
    }
    if pos > end {
        return None;
    }
    Some(Record {
        mask,
        values,
        strings,
        end,
    })
}

/// Skips the pictures (`GuidAndPicture`) of a record's `StreamData`; returns
/// where they end.
fn skip_pictures(data: &[u8], mut pos: usize, count: usize) -> Option<usize> {
    for _ in 0..count {
        let size = data
            .get(pos + 20..pos + 24)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))? as usize;
        pos = pos.checked_add(24 + size)?;
        if pos > data.len() {
            return None;
        }
    }
    Some(pos)
}

/// The control's stream: its caption, value, colors and font.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
fn read_stream(ax: &mut ActiveX, data: &[u8]) {
    const SIZE: &str = "#Size";
    let (fields, mask64, extra, pictures): Spec = match ax.class {
        "CommandButton" => (COMMAND_BUTTON, false, &[(3, "Caption"), (5, SIZE)], &[7, 10]),
        "Label" => (LABEL, false, &[(3, "Caption"), (5, SIZE)], &[10, 12]),
        "MorphData" => (MORPH_DATA, true, &[(8, SIZE), (22, "Value"), (23, "Caption"), (32, "GroupName")], &[27, 28]),
        "SpinButton" => (SPIN_BUTTON, false, &[(3, SIZE)], &[]),
        "ScrollBar" => (SCROLL_BAR, false, &[(3, SIZE)], &[]),
        _ => return,
    };
    let Some(r) = record(data, fields, mask64, extra) else {
        return;
    };
    let v = |k: &str| r.values.get(k).copied();
    ax.caption = r.strings.get("Caption").cloned().or(ax.caption.take());
    ax.value = r.strings.get("Value").cloned().or(ax.value.take());
    ax.fore = v("ForeColor").map(ole_color);
    ax.back = v("BackColor").map(ole_color);
    // fBackStyle: opaque unless said otherwise (the default bits of each class
    // have it).
    ax.opaque = v("VariousPropertyBits").is_none_or(|b| b & 0x8 != 0);
    if ax.class == "MorphData" {
        ax.display = v("DisplayStyle").map_or(ax.display, |d| d as u8);
    }
    let signed = |k: &str| v(k).map(|x| i64::from(x as i32));
    ax.min = signed("Min").unwrap_or(0);
    ax.max = signed("Max").unwrap_or(if ax.class == "ScrollBar" { 32767 } else { 100 });
    ax.position = signed("Position").unwrap_or(0);
    // Automatic (-1): by the shape of the control, which is not known here;
    // vertical.
    ax.orientation = signed("Orientation").filter(|o| *o != -1).unwrap_or(0);
    // TextProps after the pictures.
    if !matches!(ax.class, "CommandButton" | "Label" | "MorphData") {
        return;
    }
    let count = pictures.iter().filter(|b| r.mask & (1u64 << **b) != 0).count();
    let Some(at) = skip_pictures(data, r.end, count) else {
        return;
    };
    let Some(t) = record(&data[at..], TEXT_PROPS, false, &[(0, "FontName")]) else {
        return;
    };
    ax.font = t.strings.get("FontName").cloned().filter(|s| !s.is_empty());
    ax.size = t
        .values
        .get("FontHeight")
        .map(|h| f64::from(*h) / 20.0)
        .filter(|s| *s > 0.0);
    if let Some(e) = t.values.get("FontEffects") {
        ax.bold = e & 1 != 0;
        ax.italic = e & 2 != 0;
        ax.underline = e & 4 != 0;
    }
    if let Some(w) = t.values.get("FontWeight") {
        ax.bold = *w > 550;
    }
    ax.align = match t.values.get("ParagraphAlign") {
        Some(1) => Some("left"),
        Some(2) => Some("right"),
        Some(3) => Some("center"),
        _ => None,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_tree::parse_lenient;

    #[test]
    fn shape_ids() {
        assert_eq!(spid_number("_x0000_s1025"), Some(1025));
        assert_eq!(spid_number("2049"), Some(2049));
        assert_eq!(spid_number("CommandButton1"), Some(1));
        assert_eq!(spid_number("x"), None);
    }

    #[test]
    fn vml_anchors() {
        let a = vml_anchor(" 5, 10, 4, 0, 7, 0, 6, 12").unwrap();
        assert_eq!(a.from, Some((5, 95_250, 4, 0)));
        assert_eq!(a.to, Some((7, 0, 6, 114_300)));
        assert!(vml_anchor("1, 2, 3").is_none());
    }

    /// The stream of a `CommandButton` (MS-OFORMS 2.2.1) with a caption and a
    /// font.
    #[allow(clippy::cast_possible_truncation)]
    fn command_button(caption: &str, font: &str, height: u32) -> Vec<u8> {
        let utf16 = |s: &str| s.encode_utf16().flat_map(u16::to_le_bytes).collect::<Vec<u8>>();
        let pad = |v: &mut Vec<u8>| {
            while !v.len().is_multiple_of(4) {
                v.push(0);
            }
        };
        let cap = utf16(caption);
        let mask: u32 = 0b10_1111;
        let mut block = Vec::new();
        block.extend(0x8000_0012u32.to_le_bytes());
        block.extend(0x8000_000Fu32.to_le_bytes());
        block.extend(0x0000_001Bu32.to_le_bytes());
        block.extend((cap.len() as u32).to_le_bytes());
        block.extend(&cap);
        pad(&mut block);
        block.extend(2000i32.to_le_bytes());
        block.extend(600i32.to_le_bytes());
        let mut out = vec![0, 2];
        out.extend(((4 + block.len()) as u16).to_le_bytes());
        out.extend(mask.to_le_bytes());
        out.extend(block);
        let name = utf16(font);
        let mut tp = Vec::new();
        tp.extend((name.len() as u32).to_le_bytes());
        tp.extend(1u32.to_le_bytes());
        tp.extend(height.to_le_bytes());
        tp.extend(700u16.to_le_bytes());
        pad(&mut tp);
        tp.extend(&name);
        pad(&mut tp);
        out.extend([0, 2]);
        out.extend(((4 + tp.len()) as u16).to_le_bytes());
        out.extend(0b1000_0111u32.to_le_bytes());
        out.extend(tp);
        out
    }

    #[test]
    fn command_button_stream() {
        let data = command_button("実行", "ＭＳ Ｐゴシック", 220);
        let mut ax = ActiveX {
            class: "CommandButton",
            ..Default::default()
        };
        read_stream(&mut ax, &data);
        assert_eq!(ax.caption.as_deref(), Some("実行"));
        assert_eq!((ax.fore.as_deref(), ax.back.as_deref()), (Some("#000000"), Some("#f0f0f0")));
        assert_eq!((ax.font.as_deref(), ax.size), (Some("ＭＳ Ｐゴシック"), Some(11.0)));
        assert!(ax.bold);
        assert_eq!(ax.msforms_type(), "CommandButton");
    }

    struct Files(HashMap<String, Vec<u8>>);

    impl Package for Files {
        fn xml(&self, part: &str) -> Option<Node> {
            parse_lenient(self.0.get(part)?)
        }

        fn bytes(&self, part: &str) -> Option<Vec<u8>> {
            self.0.get(part).cloned()
        }
    }

    #[test]
    fn form_and_activex_controls_of_a_sheet() {
        let rel = |id: &str, t: &str, target: &str| {
            format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}" Target="{target}"/>"#)
        };
        let mut files = HashMap::new();
        let mut add = |name: &str, data: Vec<u8>| {
            files.insert(name.to_string(), data);
        };
        add(
            "xl/worksheets/sheet1.xml",
            br#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing"><sheetData/><controls><control shapeId="1025" r:id="rId2" name="Check Box 1"><controlPr><anchor><from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></from><to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>0</xdr:rowOff></to></anchor></controlPr></control><control shapeId="1026" r:id="rId3" name="CommandButton1"/></controls></worksheet>"#.to_vec(),
        );
        add(
            "xl/worksheets/_rels/sheet1.xml.rels",
            format!(
                "<Relationships>{}{}{}</Relationships>",
                rel("rId1", "vmlDrawing", "../drawings/vmlDrawing1.vml"),
                rel("rId2", "ctrlProp", "../ctrlProps/ctrlProp1.xml"),
                rel("rId3", "control", "../activeX/activeX1.xml"),
            )
            .into_bytes(),
        );
        add(
            "xl/ctrlProps/ctrlProp1.xml",
            br#"<formControlPr xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" objectType="CheckBox" checked="Checked" fmlaLink="$D$3" noThreeD="1"/>"#.to_vec(),
        );
        add(
            "xl/drawings/vmlDrawing1.vml",
            "<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><v:shape id=\"_x0000_s1025\" style=\"position:absolute\"><v:textbox><div style='text-align:left'><font face=\"ＭＳ Ｐゴシック\" size=\"220\">確認済み<br></font></div></v:textbox><x:ClientData ObjectType=\"Checkbox\"><x:Anchor>1, 0, 2, 0, 3, 0, 3, 0</x:Anchor><x:FmlaMacro>[0]!Checked</x:FmlaMacro><x:Checked>1</x:Checked></x:ClientData></v:shape><v:shape id=\"_x0000_s1027\"><x:ClientData ObjectType=\"Note\"><x:Row>1</x:Row></x:ClientData></v:shape></xml>".as_bytes().to_vec(),
        );
        add(
            "xl/activeX/activeX1.xml",
            br#"<ax:ocx xmlns:ax="http://schemas.microsoft.com/office/2006/activeX" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" ax:classid="{D7053240-CE69-11CD-A777-00DD01143C57}" ax:persistence="persistStreamInit" r:id="rId1"/>"#.to_vec(),
        );
        add("xl/activeX/_rels/activeX1.xml.rels", format!("<Relationships>{}</Relationships>", rel("rId1", "activeXControlBinary", "activeX1.bin")).into_bytes());
        let mut bin = vec![0u8; 16];
        bin.extend(command_button("実行", "Meiryo UI", 180));
        add("xl/activeX/activeX1.bin", bin);
        let pkg = Files(files);

        let list = controls(&pkg, "xl/worksheets/sheet1.xml");
        assert_eq!(list.len(), 2, "{list:?}");
        let check = &list[0];
        assert_eq!((check.shape_id, check.name.as_str(), check.kind.as_str()), (1025, "Check Box 1", "CheckBox"));
        assert_eq!(check.linked_cell(), Some("$D$3"));
        assert_eq!(check.checked(), CheckState::Checked);
        assert_eq!(check.macro_name(), Some("Checked"));
        assert!(check.is_flat());
        assert_eq!(check.anchor.as_ref().and_then(|a| a.from), Some((1, 0, 2, 0)));
        let text = check.text.as_ref().unwrap();
        assert_eq!((text.text.as_str(), text.font.as_deref(), text.size, text.align), ("確認済み", Some("ＭＳ Ｐゴシック"), Some(11.0), Some("left")));
        let button = &list[1];
        assert_eq!(button.kind, "ActiveX");
        let ax = button.activex.as_ref().unwrap();
        assert_eq!((ax.caption.as_deref(), ax.font.as_deref(), ax.size), (Some("実行"), Some("Meiryo UI"), Some(9.0)));

        // The mirror in the drawing: its text, and the name of a control
        // without one.
        let mut check = check.clone();
        let sp = parse_lenient(r#"<xdr:sp xmlns:xdr="x" xmlns:a="a" macro=""><xdr:nvSpPr><xdr:cNvPr id="1025" name="Check Box 1"/></xdr:nvSpPr><xdr:txBody><a:bodyPr anchor="ctr"/><a:p><a:pPr algn="l"/><a:r><a:rPr sz="1000" b="1"/><a:t>確認済み</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#.as_bytes()).unwrap();
        check.apply_mirror(&sp, &HashMap::new());
        let text = check.text.unwrap();
        assert_eq!((text.size, text.bold, text.align), (Some(10.0), true, Some("left")));
        assert_eq!(check.client.get("FmlaMacro").map(String::as_str), Some("[0]!Checked"));
    }
}
