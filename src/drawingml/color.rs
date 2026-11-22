//! The colours of `DrawingML` as "#RRGGBB": colour elements (`a:srgbClr`,
//! `a:schemeClr`, `a:sysClr`, `a:prstClr`) resolved with the theme's colour
//! scheme, the modifiers Office applies to them (lumMod, lumOff, shade,
//! tint), the fill of a shape and the colours of its style.
//!
//! Theme colours are passed as a map of the scheme's names (dk1, lt1,
//! accent1, ...) to RRGGBB ([`theme_colors`]); a name missing from it is
//! taken from Office's default theme.

// Theme colours are taken in the map programs keep them in.
#![allow(clippy::implicit_hasher)]

use std::collections::HashMap;

use crate::xml_tree::Node;

/// The colours of a theme's colour scheme (`a:clrScheme` of a theme part)
/// by name (accent1, dk1, ...) as RRGGBB.
#[must_use]
pub fn theme_colors(theme: &Node) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(cs) = theme.find("clrScheme") {
        for c in &cs.children {
            let v = c
                .child("srgbClr")
                .and_then(|x| x.attr("val"))
                .or_else(|| c.child("sysClr").and_then(|x| x.attr("lastClr")));
            if let Some(v) = v {
                out.insert(c.name.clone(), v.to_uppercase());
            }
        }
    }
    out
}

/// Office's default theme (when the workbook has none): the colour named
/// `name` as RRGGBB.
#[must_use]
pub fn office_color(name: &str) -> Option<&'static str> {
    Some(match name {
        "dk1" => "000000",
        "lt1" => "FFFFFF",
        "dk2" => "44546A",
        "lt2" => "E7E6E6",
        "accent1" => "4472C4",
        "accent2" => "ED7D31",
        "accent3" => "A5A5A5",
        "accent4" => "FFC000",
        "accent5" => "5B9BD5",
        "accent6" => "70AD47",
        _ => return None,
    })
}

/// A colour element (`a:srgbClr`, `a:schemeClr`, `a:sysClr`, `a:prstClr`)
/// as RRGGBB, before its modifiers.
#[must_use]
pub fn base_color(c: &Node, theme: &HashMap<String, String>) -> Option<String> {
    Some(match c.name.as_str() {
        "srgbClr" => c.attr("val")?.to_uppercase(),
        "sysClr" => c.attr("lastClr")?.to_uppercase(),
        "prstClr" => match c.attr("val")? {
            "black" => "000000",
            "white" => "FFFFFF",
            "red" => "FF0000",
            "green" => "008000",
            "blue" => "0000FF",
            "yellow" => "FFFF00",
            "gray" | "grey" => "808080",
            _ => return None,
        }
        .to_string(),
        "schemeClr" => {
            let name = match c.attr("val")? {
                "tx1" => "dk1",
                "bg1" => "lt1",
                "tx2" => "dk2",
                "bg2" => "lt2",
                v => v,
            };
            theme
                .get(name)
                .cloned()
                .or_else(|| office_color(name).map(str::to_string))?
        }
        _ => return None,
    })
}

/// The colour modifiers among the children of `n` (lumMod, lumOff, shade,
/// tint) as fractions.
#[must_use]
pub fn mods_of(n: &Node) -> Vec<(String, f64)> {
    n.children
        .iter()
        .filter(|c| matches!(c.name.as_str(), "lumMod" | "lumOff" | "shade" | "tint"))
        .filter_map(|c| {
            Some((
                c.name.clone(),
                c.attr("val")?.parse::<f64>().ok()? / 100_000.0,
            ))
        })
        .collect()
}

/// `rgb` (RRGGBB) with modifiers applied, as "#RRGGBB". lumMod / lumOff
/// work on the luminance (HSL), as Office does; shade / tint darken and
/// lighten.
#[must_use]
#[allow(clippy::many_single_char_names)]
pub fn modified(rgb: &str, mods: &[(String, f64)]) -> Option<String> {
    let v = u32::from_str_radix(rgb, 16).ok()?;
    let (mut r, mut g, mut b) = (
        f64::from((v >> 16) & 255) / 255.0,
        f64::from((v >> 8) & 255) / 255.0,
        f64::from(v & 255) / 255.0,
    );
    let get = |k: &str| mods.iter().find(|(n, _)| n == k).map(|(_, x)| *x);
    if get("lumMod").is_some() || get("lumOff").is_some() {
        let (h, s, l) = to_hsl(r, g, b);
        let l = (l * get("lumMod").unwrap_or(1.0) + get("lumOff").unwrap_or(0.0)).clamp(0.0, 1.0);
        (r, g, b) = from_hsl(h, s, l);
    }
    if let Some(x) = get("shade") {
        (r, g, b) = (r * x, g * x, b * x);
    }
    if let Some(x) = get("tint") {
        (r, g, b) = (
            1.0 - (1.0 - r) * x,
            1.0 - (1.0 - g) * x,
            1.0 - (1.0 - b) * x,
        );
    }
    Some(format!(
        "#{:02X}{:02X}{:02X}",
        channel(r),
        channel(g),
        channel(b)
    ))
}

/// A channel (0 to 1) as a byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel(v: f64) -> u8 {
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

#[allow(clippy::float_cmp, clippy::many_single_char_names)]
fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = f64::midpoint(max, min);
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

#[allow(clippy::float_cmp, clippy::many_single_char_names)]
fn from_hsl(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l, l, l);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// The colour element among the children of `n`.
fn color_element(n: &Node) -> Option<&Node> {
    n.children.iter().find(|c| {
        matches!(
            c.name.as_str(),
            "srgbClr" | "schemeClr" | "sysClr" | "prstClr"
        )
    })
}

/// The colour of a fill (`a:solidFill` and the like) or of anything with a
/// colour element (a gradient stop, an effect, a style reference) as
/// "#RRGGBB".
#[must_use]
pub fn color_of(fill: &Node, theme: &HashMap<String, String>) -> Option<String> {
    let c = color_element(fill)?;
    modified(&base_color(c, theme)?, &mods_of(c))
}

/// The alpha of a fill's colour, 0 to 1 (alpha 100000 = opaque).
#[must_use]
pub fn alpha_of(fill: &Node) -> f64 {
    color_element(fill)
        .and_then(|c| c.child("alpha"))
        .and_then(|a| a.attr("val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(1.0, |v| (v / 100_000.0).clamp(0.0, 1.0))
}

/// The fill of shape properties (`spPr`), or a line (`a:ln`): "none" for
/// `a:noFill`, else the colour ("#RRGGBB") of `a:solidFill` or of the first
/// stop of `a:gradFill`, with its alpha; None when it does not say.
#[must_use]
pub fn fill_color(props: Option<&Node>, theme: &HashMap<String, String>) -> Option<(String, f64)> {
    let p = props?;
    if p.child("noFill").is_some() {
        return Some(("none".into(), 1.0));
    }
    let fill = p
        .child("solidFill")
        .or_else(|| p.child("gradFill").and_then(|g| g.find("gs")))?;
    let color = color_of(fill, theme)?;
    let alpha = fill
        .children
        .first()
        .and_then(|c| c.child("alpha"))
        .and_then(|a| a.attr("val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(1.0, |v| v / 100_000.0);
    Some((color, alpha))
}

/// The colour ("#RRGGBB") and index of a shape's style reference
/// (`a:lnRef`, `a:fillRef`, `a:fontRef` of its `style`); None for a line or
/// fill reference of index 0 (none).
#[must_use]
pub fn style_ref(
    sp: &Node,
    name: &str,
    theme: &HashMap<String, String>,
) -> Option<(String, usize)> {
    let r = sp.child("style")?.child(name)?;
    let idx = r
        .attr("idx")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let color = color_of(r, theme)?;
    if name != "fontRef" && idx == 0 {
        return None;
    }
    Some((color, idx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_tree::parse;

    fn m(v: &[(&str, f64)]) -> Vec<(String, f64)> {
        v.iter().map(|(k, x)| ((*k).to_string(), *x)).collect()
    }

    #[test]
    fn color_modifiers() {
        // As Excel shows them (Office 2013+ theme): accent1 darker 40% /
        // lighter 40%, accent2 darker 25%.
        assert_eq!(
            modified("4472C4", &m(&[("lumMod", 0.6)])).unwrap(),
            "#264478"
        );
        assert_eq!(
            modified("4472C4", &m(&[("lumMod", 0.6), ("lumOff", 0.4)])).unwrap(),
            "#8FAADC"
        );
        assert_eq!(
            modified("ED7D31", &m(&[("lumMod", 0.75)])).unwrap(),
            "#C55A11"
        );
        assert_eq!(modified("70AD47", &[]).unwrap(), "#70AD47");
        assert_eq!(
            modified("808080", &m(&[("shade", 0.5)])).unwrap(),
            "#404040"
        );
        assert_eq!(modified("000000", &m(&[("tint", 0.5)])).unwrap(), "#808080");
        assert!(modified("xyz", &[]).is_none());
    }

    #[test]
    fn elements() {
        let theme = HashMap::from([("accent1".to_string(), "112233".to_string())]);
        let fill = |xml: &str| parse(xml.as_bytes()).unwrap();
        let c = |xml: &str| color_of(&fill(xml), &theme);
        assert_eq!(
            c(r#"<solidFill><srgbClr val="ff0000"/></solidFill>"#).unwrap(),
            "#FF0000"
        );
        assert_eq!(
            c(r#"<solidFill><schemeClr val="accent1"/></solidFill>"#).unwrap(),
            "#112233"
        );
        // Not in the theme: Office's.
        assert_eq!(
            c(r#"<solidFill><schemeClr val="tx2"/></solidFill>"#).unwrap(),
            "#44546A"
        );
        assert_eq!(
            c(r#"<solidFill><prstClr val="grey"/></solidFill>"#).unwrap(),
            "#808080"
        );
        assert_eq!(
            c(r#"<solidFill><sysClr val="window" lastClr="ffffff"><lumMod val="50000"/></sysClr></solidFill>"#)
                .unwrap(),
            "#808080"
        );
        assert!(c("<solidFill/>").is_none());
        let a =
            fill(r#"<solidFill><srgbClr val="000000"><alpha val="40000"/></srgbClr></solidFill>"#);
        assert!((alpha_of(&a) - 0.4).abs() < 1e-9);
        assert!((alpha_of(&fill("<solidFill/>")) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn fills_and_styles() {
        let theme = HashMap::new();
        let sp = parse(br#"<sp><spPr><gradFill><gsLst><gs pos="0"><srgbClr val="00ff00"><alpha val="50000"/></srgbClr></gs></gsLst></gradFill><ln><noFill/></ln></spPr><style><lnRef idx="0"><schemeClr val="accent1"/></lnRef><fillRef idx="1"><schemeClr val="accent2"/></fillRef><fontRef idx="minor"><schemeClr val="lt1"/></fontRef></style></sp>"#).unwrap();
        let props = sp.child("spPr");
        assert_eq!(
            fill_color(props, &theme),
            Some(("#00FF00".to_string(), 0.5))
        );
        assert_eq!(
            fill_color(props.and_then(|p| p.child("ln")), &theme),
            Some(("none".to_string(), 1.0))
        );
        assert_eq!(fill_color(sp.child("style"), &theme), None);
        assert_eq!(style_ref(&sp, "lnRef", &theme), None);
        assert_eq!(
            style_ref(&sp, "fillRef", &theme),
            Some(("#ED7D31".to_string(), 1))
        );
        assert_eq!(
            style_ref(&sp, "fontRef", &theme),
            Some(("#FFFFFF".to_string(), 0))
        );
    }

    #[test]
    fn themes() {
        let t = parse(br#"<a:theme xmlns:a="a"><a:themeElements><a:clrScheme name="x"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:accent2><a:srgbClr val="ed7d31"/></a:accent2></a:clrScheme></a:themeElements></a:theme>"#).unwrap();
        let colors = theme_colors(&t);
        assert_eq!(colors.len(), 2);
        assert_eq!(colors["accent2"], "ED7D31");
    }
}
