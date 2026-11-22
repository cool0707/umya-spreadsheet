//! Where a drawing object is: its anchor on the sheet (`xdr:twoCellAnchor`,
//! `xdr:oneCellAnchor`, `xdr:absoluteAnchor`; the anchor of a control in a
//! sheet's XML is the same) and the transform of a shape, picture or group
//! (`a:xfrm`).

use super::EMU_PT;
use crate::xml_tree::Node;

/// Where a drawing object is on its sheet: the cell markers (zero-based
/// column and row, offsets in EMU) and/or the size and position in EMU.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Anchor {
    pub from: Option<(u32, i64, u32, i64)>,
    pub to:   Option<(u32, i64, u32, i64)>,
    pub ext:  Option<(i64, i64)>,
    pub pos:  Option<(i64, i64)>,
}

/// The anchor of a drawing object (the anchor element around it).
#[must_use]
pub fn anchor_of(a: &Node) -> Anchor {
    // A column or row out of range is taken as the nearest one.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = |v: i64| v.clamp(0, i64::from(u32::MAX)) as u32;
    let marker = |n: &str| -> Option<(u32, i64, u32, i64)> {
        let m = a.child(n)?;
        let num = |k: &str| {
            m.child(k)
                .and_then(|c| c.all_text().trim().parse::<i64>().ok())
        };
        Some((
            index(num("col")?),
            num("colOff").unwrap_or(0),
            index(num("row")?),
            num("rowOff").unwrap_or(0),
        ))
    };
    let pair = |n: &str, x: &str, y: &str| -> Option<(i64, i64)> {
        let e = a.child(n)?;
        Some((e.attr(x)?.parse().ok()?, e.attr(y)?.parse().ok()?))
    };
    Anchor {
        from: marker("from"),
        to:   marker("to"),
        ext:  pair("ext", "cx", "cy"),
        pos:  pair("pos", "x", "y"),
    }
}

/// The cells (column, row; 1-based) of a drawing anchor
/// (`xdr:twoCellAnchor`, `xdr:oneCellAnchor`): its from and, when it has
/// one, its to. None for an absolute anchor and markers out of range.
#[must_use]
#[allow(clippy::type_complexity)]
pub fn anchor_cells(a: &Node) -> Option<((u32, u32), Option<(u32, u32)>)> {
    let at = |n: &str| -> Option<(u32, u32)> {
        let m = a.child(n)?;
        let num = |k: &str| {
            m.child(k)
                .and_then(|c| c.all_text().trim().parse::<u32>().ok())
        };
        Some((num("col")?.checked_add(1)?, num("row")?.checked_add(1)?))
    };
    Some((at("from")?, at("to")))
}

/// The transform of a shape, picture or group (pt): its offset and size,
/// those of its children's coordinates (a group's `chOff`, `chExt`), its
/// rotation (degrees, clockwise) and flips.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xfrm {
    pub off:    (f64, f64),
    pub ext:    (f64, f64),
    pub ch_off: (f64, f64),
    pub ch_ext: (f64, f64),
    pub rot:    f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// The transform of a shape, connector, picture or group (the `a:xfrm` of
/// its `spPr` or `grpSpPr`); None when it has none.
#[must_use]
pub fn xfrm(obj: &Node) -> Option<Xfrm> {
    let props = obj
        .children
        .iter()
        .find(|c| matches!(c.name.as_str(), "spPr" | "grpSpPr"))?;
    let x = props.child("xfrm")?;
    let pair = |n: &str, a: &str, b: &str| -> (f64, f64) {
        x.child(n).map_or((0.0, 0.0), |e| {
            let g = |k: &str| e.attr(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / EMU_PT;
            (g(a), g(b))
        })
    };
    let flag = |k: &str| matches!(x.attr(k), Some("1" | "true"));
    Some(Xfrm {
        off:    pair("off", "x", "y"),
        ext:    pair("ext", "cx", "cy"),
        ch_off: pair("chOff", "x", "y"),
        ch_ext: pair("chExt", "cx", "cy"),
        rot:    x
            .attr("rot")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / 60000.0,
        flip_h: flag("flipH"),
        flip_v: flag("flipV"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_tree::parse;

    #[test]
    fn anchors() {
        let a = parse(br#"<xdr:twoCellAnchor xmlns:xdr="x"><xdr:from><xdr:col>4</xdr:col><xdr:colOff>12700</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>11</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>17</xdr:row><xdr:rowOff>-5</xdr:rowOff></xdr:to><xdr:sp/></xdr:twoCellAnchor>"#).unwrap();
        assert_eq!(
            anchor_of(&a),
            Anchor {
                from: Some((4, 12700, 1, 0)),
                to:   Some((11, 0, 17, -5)),
                ext:  None,
                pos:  None,
            }
        );
        assert_eq!(anchor_cells(&a), Some(((5, 2), Some((12, 18)))));
        let a = parse(
            br#"<absoluteAnchor><pos x="10" y="20"/><ext cx="300" cy="400"/></absoluteAnchor>"#,
        )
        .unwrap();
        let an = anchor_of(&a);
        assert_eq!(
            (an.pos, an.ext, an.from),
            (Some((10, 20)), Some((300, 400)), None)
        );
        assert_eq!(anchor_cells(&a), None);
        // Markers out of range.
        let a = parse(
            br"<oneCellAnchor><from><col>-3</col><row>99999999999</row></from></oneCellAnchor>",
        )
        .unwrap();
        assert_eq!(anchor_of(&a).from, Some((0, 0, u32::MAX, 0)));
        assert_eq!(anchor_cells(&a), None);
    }

    #[test]
    fn transforms() {
        let sp = parse(br#"<grpSp><grpSpPr><xfrm rot="5400000" flipH="1"><off x="12700" y="25400"/><ext cx="127000" cy="63500"/><chOff x="0" y="0"/><chExt cx="254000" cy="127000"/></xfrm></grpSpPr></grpSp>"#).unwrap();
        let x = xfrm(&sp).unwrap();
        assert_eq!(
            x,
            Xfrm {
                off:    (1.0, 2.0),
                ext:    (10.0, 5.0),
                ch_off: (0.0, 0.0),
                ch_ext: (20.0, 10.0),
                rot:    90.0,
                flip_h: true,
                flip_v: false,
            }
        );
        assert!(xfrm(&parse(b"<sp><spPr/></sp>").unwrap()).is_none());
    }
}
