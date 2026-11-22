//! Of a chart part (`c:chartSpace`): the automatic colours of its series and
//! points, the colour of a fill or line it gives (`c:spPr`), and the data of
//! a series as the part holds it (the reference to the cells and the cache
//! of their values).

// Theme colours are taken in the map programs keep them in.
#![allow(clippy::implicit_hasher)]

use std::collections::HashMap;

use super::{
    color::{
        base_color,
        color_of,
        modified,
        mods_of,
        office_color,
    },
    package::Package,
};
use crate::xml_tree::Node;

/// The automatic colours of a chart's series and points, as Excel 2013 and
/// later picks them: the chart's colour style (`xl/charts/colorsN.xml`, the
/// palette chosen with "Change Colors") or else the theme's accent1 to
/// accent6. After the last colour they repeat with the next variation
/// (darker, lighter, ...). Monochrome styles are read the same way, which
/// is close to (not exactly) what Excel shows.
#[derive(Clone, Debug, Default)]
pub struct AutoColors {
    colors:     Vec<String>,
    variations: Vec<Vec<(String, f64)>>,
}

impl AutoColors {
    /// The colours of the colour style `style` (the root of a
    /// `chartColorStyle` part), or Office's default ones without it.
    #[must_use]
    pub fn new(style: Option<&Node>, theme: &HashMap<String, String>) -> AutoColors {
        if let Some(st) = style {
            let colors: Vec<String> = st
                .children
                .iter()
                .filter(|c| matches!(c.name.as_str(), "srgbClr" | "schemeClr" | "sysClr"))
                .filter_map(|c| {
                    modified(&base_color(c, theme)?, &mods_of(c)).map(|x| x[1..].to_string())
                })
                .collect();
            if !colors.is_empty() {
                return AutoColors {
                    colors,
                    variations: st.all("variation").map(mods_of).collect(),
                };
            }
        }
        // Office's default colour style (colors1.xml of a new chart).
        let m = |v: &[(&str, f64)]| {
            v.iter()
                .map(|(k, x)| ((*k).to_string(), *x))
                .collect::<Vec<_>>()
        };
        AutoColors {
            colors:     (1..=6)
                .map(|i| format!("accent{i}"))
                .filter_map(|a| {
                    theme
                        .get(&a)
                        .cloned()
                        .or_else(|| office_color(&a).map(str::to_string))
                })
                .collect(),
            variations: vec![
                vec![],
                m(&[("lumMod", 0.6)]),
                m(&[("lumMod", 0.8), ("lumOff", 0.2)]),
                m(&[("lumMod", 0.8)]),
                m(&[("lumMod", 0.6), ("lumOff", 0.4)]),
                m(&[("lumMod", 0.5)]),
                m(&[("lumMod", 0.7), ("lumOff", 0.3)]),
                m(&[("lumMod", 0.7)]),
                m(&[("lumMod", 0.5), ("lumOff", 0.5)]),
            ],
        }
    }

    /// The colours of the chart part `chart_part`: of its colour style (the
    /// part it relates to as `chartColorStyle`), if it has one.
    #[must_use]
    pub fn of_chart<P: Package + ?Sized>(
        parts: &P,
        chart_part: &str,
        theme: &HashMap<String, String>,
    ) -> AutoColors {
        let style = parts
            .rels(chart_part)
            .into_values()
            .find(|(t, _)| t.ends_with("/chartColorStyle"))
            .and_then(|(_, p)| parts.xml(&p));
        AutoColors::new(style.as_ref(), theme)
    }

    /// The colour ("#RRGGBB") of the i-th series (its `c:idx`) or point.
    #[must_use]
    pub fn nth(&self, i: usize) -> Option<String> {
        let n = self.colors.len();
        if n == 0 {
            return None;
        }
        let none = Vec::new();
        let var = if self.variations.is_empty() {
            &none
        } else {
            &self.variations[(i / n) % self.variations.len()]
        };
        modified(&self.colors[i % n], var)
    }
}

/// The fill (or, for lines, the line) of a `c:spPr`: its colour
/// ("#RRGGBB"), and whether it has none (`a:noFill`).
#[must_use]
pub fn paint(
    sp: Option<&Node>,
    line: bool,
    theme: &HashMap<String, String>,
) -> (Option<String>, bool) {
    let Some(sp) = sp else { return (None, false) };
    let src = if line {
        sp.child("ln").unwrap_or(sp)
    } else {
        sp
    };
    if src.child("noFill").is_some() {
        return (None, true);
    }
    let color = src.child("solidFill").and_then(|f| color_of(f, theme));
    let color = color.or_else(|| {
        src.child("gradFill")
            .and_then(|g| g.find("gs"))
            .and_then(|gs| color_of(gs, theme))
    });
    (color, false)
}

/// The areas of a series reference ("'売上 2026'!$B$2:$B$13",
/// "(Sheet1!$A$1,Sheet1!$A$3)"), each as it is written.
#[must_use]
pub fn ref_areas(formula: &str) -> Vec<String> {
    let f = formula.trim();
    let f = f
        .strip_prefix('(')
        .and_then(|x| x.strip_suffix(')'))
        .unwrap_or(f);
    let mut parts = Vec::new();
    let (mut cur, mut quoted) = (String::new(), false);
    for ch in f.chars() {
        match ch {
            '\'' => {
                quoted = !quoted;
                cur.push(ch);
            }
            ',' if !quoted => parts.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    parts.push(cur);
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// The most points of a cache read (a count or index beyond it is taken as
/// it).
pub const MAX_POINTS: usize = 100_000;

/// A point of a chart's cache (or of a literal): its value as written and
/// its number format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CachedPoint {
    pub text:   String,
    pub format: String,
}

/// The data of a series' `c:cat`, `c:val`, `c:xVal`, `c:yVal`,
/// `c:bubbleSize` or `c:tx` as the chart part holds it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataSource {
    /// How: numRef, strRef, multiLvlStrRef (references to cells), numLit or
    /// strLit (values of its own).
    pub kind:    String,
    /// The reference to the cells (`c:f`), empty without one.
    pub formula: String,
    /// The number format of the cache.
    pub format:  String,
    /// The cache (or the literal values) by point index; None for points
    /// not in it. Of the levels of categories (multiLvlStrCache), the
    /// innermost.
    pub points:  Vec<Option<CachedPoint>>,
}

impl DataSource {
    /// The data of `n` (`c:cat` and the like); None when it has none.
    #[must_use]
    pub fn read(n: &Node) -> Option<DataSource> {
        let r = n.children.iter().find(|c| {
            matches!(
                c.name.as_str(),
                "numRef" | "strRef" | "multiLvlStrRef" | "numLit" | "strLit"
            )
        })?;
        let formula = r
            .child("f")
            .map(|f| f.text.trim().to_string())
            .unwrap_or_default();
        let cache = r
            .children
            .iter()
            .find(|c| {
                matches!(
                    c.name.as_str(),
                    "numCache" | "strCache" | "multiLvlStrCache"
                )
            })
            .unwrap_or(r);
        let cache = if cache.name == "multiLvlStrCache" {
            cache.all("lvl").next().unwrap_or(cache)
        } else {
            cache
        };
        let format = cache
            .child("formatCode")
            .map(|f| f.text.trim().to_string())
            .unwrap_or_default();
        let count = cache
            .val("ptCount")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
            .min(MAX_POINTS);
        let mut points = vec![None; count];
        for p in cache.all("pt") {
            let idx = p
                .attr("idx")
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(points.len())
                .min(MAX_POINTS);
            if idx >= points.len() {
                points.resize(idx + 1, None);
            }
            points[idx] = Some(CachedPoint {
                text:   p.child("v").map(|v| v.text.clone()).unwrap_or_default(),
                format: p
                    .attr("formatCode")
                    .map_or_else(|| format.clone(), str::to_string),
            });
        }
        Some(DataSource {
            kind: r.name.clone(),
            formula,
            format,
            points,
        })
    }

    /// Whether the values are numbers (numRef, numLit).
    #[must_use]
    pub fn numeric(&self) -> bool {
        matches!(self.kind.as_str(), "numRef" | "numLit")
    }

    /// The areas of the reference ([`ref_areas`]).
    #[must_use]
    pub fn areas(&self) -> Vec<String> {
        ref_areas(&self.formula)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_tree::parse;

    #[test]
    fn default_auto_colors() {
        let a = AutoColors::new(None, &HashMap::new());
        let got: Vec<String> = [0, 1, 5, 6, 12]
            .iter()
            .map(|i| a.nth(*i).unwrap())
            .collect();
        assert_eq!(got, ["#4472C4", "#ED7D31", "#70AD47", "#264478", "#698ED0"]);
        assert!(AutoColors::default().nth(0).is_none());
    }

    #[test]
    fn color_styles() {
        let st = parse(br#"<cs:colorStyle xmlns:cs="c" xmlns:a="a" meth="cycle" id="10"><a:schemeClr val="accent2"/><a:srgbClr val="00ff00"/><cs:variation/><cs:variation><a:lumMod val="60000"/></cs:variation></cs:colorStyle>"#).unwrap();
        let a = AutoColors::new(Some(&st), &HashMap::new());
        let got: Vec<String> = (0..4).map(|i| a.nth(i).unwrap()).collect();
        assert_eq!(got, ["#ED7D31", "#00FF00", "#9E480E", "#009900"]);
    }

    #[test]
    fn paints() {
        let theme = HashMap::new();
        let sp = parse(
            br#"<spPr><solidFill><srgbClr val="FF0000"/></solidFill><ln><noFill/></ln></spPr>"#,
        )
        .unwrap();
        assert_eq!(
            paint(Some(&sp), false, &theme),
            (Some("#FF0000".into()), false)
        );
        assert_eq!(paint(Some(&sp), true, &theme), (None, true));
        assert_eq!(paint(None, true, &theme), (None, false));
    }

    #[test]
    fn areas() {
        assert_eq!(
            ref_areas("'売上 2026'!$B$2:$B$13"),
            ["'売上 2026'!$B$2:$B$13"]
        );
        assert_eq!(
            ref_areas(" ('a,b'!$A$1, Sheet1!$A$3,) "),
            ["'a,b'!$A$1", "Sheet1!$A$3"]
        );
        assert!(ref_areas("").is_empty());
    }

    #[test]
    fn sources() {
        let n = parse(br#"<val><numRef><f> Sheet1!$B$2:$B$4 </f><numCache><formatCode>0.0</formatCode><ptCount val="3"/><pt idx="0"><v>1.5</v></pt><pt idx="2" formatCode="0%"><v>0.25</v></pt></numCache></numRef></val>"#).unwrap();
        let s = DataSource::read(&n).unwrap();
        assert!(s.numeric());
        assert_eq!(
            (s.formula.as_str(), s.format.as_str()),
            ("Sheet1!$B$2:$B$4", "0.0")
        );
        assert_eq!(s.areas(), ["Sheet1!$B$2:$B$4"]);
        let pt = |t: &str, f: &str| {
            Some(CachedPoint {
                text:   t.into(),
                format: f.into(),
            })
        };
        assert_eq!(s.points, [pt("1.5", "0.0"), None, pt("0.25", "0%")]);
        // Categories with levels: the innermost; literals.
        let n = parse(br#"<cat><multiLvlStrRef><f>S!$A$2:$B$3</f><multiLvlStrCache><ptCount val="2"/><lvl><pt idx="1"><v>b</v></pt></lvl><lvl><pt idx="0"><v>X</v></pt></lvl></multiLvlStrCache></multiLvlStrRef></cat>"#).unwrap();
        assert_eq!(DataSource::read(&n).unwrap().points, [None, pt("b", "")]);
        let n =
            parse(br#"<tx><strLit><ptCount val="1"/><pt idx="0"><v>Name</v></pt></strLit></tx>"#)
                .unwrap();
        let s = DataSource::read(&n).unwrap();
        assert!(!s.numeric() && s.formula.is_empty() && s.points == [pt("Name", "")]);
        assert!(DataSource::read(&parse(b"<val/>").unwrap()).is_none());
        // A count beyond what is read.
        let n = parse(br#"<val><numLit><ptCount val="4000000000"/><pt idx="4000000001"><v>1</v></pt></numLit></val>"#).unwrap();
        let s = DataSource::read(&n).unwrap();
        assert_eq!(s.points.len(), MAX_POINTS + 1);
    }
}
