//! The relationships between the parts of a workbook file (`_rels/*.rels`)
//! and the parts the drawings of a sheet are reached through.

use std::collections::HashMap;

use super::color::theme_colors;
use crate::xml_tree::Node;

/// `target` relative to the folder `dir` ("../drawings/drawing1.xml" from
/// "xl/worksheets"), or absolute ("/xl/...").
#[must_use]
pub fn resolve(dir: &str, target: &str) -> String {
    if let Some(t) = target.strip_prefix('/') {
        return t.to_string();
    }
    let mut parts: Vec<&str> = if dir.is_empty() {
        vec![]
    } else {
        dir.split('/').collect()
    };
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// The relationships of a `.rels` part (its root, `Relationships`) of a part
/// in the folder `dir`: id -> (type, part it points to). External ones
/// (links to files and web pages) are left out.
#[must_use]
pub fn relationships(rels: &Node, dir: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    for r in rels.all("Relationship") {
        let (Some(id), Some(target)) = (r.attr("Id"), r.attr("Target")) else {
            continue;
        };
        if r.attr("TargetMode") == Some("External") {
            continue;
        }
        out.insert(
            id.to_string(),
            (
                r.attr("Type").unwrap_or("").to_string(),
                resolve(dir, target),
            ),
        );
    }
    out
}

/// The parts of a workbook file (or those of them a program keeps), read as
/// XML: what the readers of drawings need of it. A program implements
/// [`Package::xml`] (how it holds the parts, how strictly it reads them)
/// and gets the rest.
pub trait Package {
    /// The part named `part` ("xl/drawings/drawing1.xml") as XML; None when
    /// there is no such part or it cannot be read.
    fn xml(&self, part: &str) -> Option<Node>;

    /// The part named `part` as it is (a picture, the persisted data of an
    /// ActiveX control); None when there is no such part or the program does
    /// not keep it.
    fn bytes(&self, _part: &str) -> Option<Vec<u8>> {
        None
    }

    /// The child `name` of the root of the part `part` (the `controls` of a
    /// sheet). A program that does not keep whole sheets (they are large)
    /// keeps the elements it needs and gives them here.
    fn sheet_element(&self, part: &str, name: &str) -> Option<Node> {
        self.xml(part)?.child(name).cloned()
    }

    /// The relationships of a part: id -> (type, part it points to).
    fn rels(&self, part: &str) -> HashMap<String, (String, String)> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        self.xml(&format!("{dir}/_rels/{file}.rels"))
            .map(|root| relationships(&root, dir))
            .unwrap_or_default()
    }

    /// The part of the worksheet named `sheet`.
    fn sheet_part(&self, sheet: &str) -> Option<String> {
        let wb = self.xml("xl/workbook.xml")?;
        let s = wb
            .child("sheets")?
            .all("sheet")
            .find(|s| s.attr("name") == Some(sheet))?;
        let id = s.attr("id")?;
        self.rels("xl/workbook.xml").remove(id).map(|(_, p)| p)
    }

    /// The theme's colours by name (accent1, dk1, ...) as RRGGBB.
    fn theme_colors(&self) -> HashMap<String, String> {
        self.xml("xl/theme/theme1.xml")
            .map(|t| theme_colors(&t))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml_tree::parse;

    struct Files(HashMap<&'static str, &'static str>);

    impl Package for Files {
        fn xml(&self, part: &str) -> Option<Node> {
            parse(self.0.get(part)?.as_bytes())
        }
    }

    #[test]
    fn targets() {
        assert_eq!(
            resolve("xl/worksheets", "../drawings/drawing1.xml"),
            "xl/drawings/drawing1.xml"
        );
        assert_eq!(
            resolve("xl", "worksheets/./sheet1.xml"),
            "xl/worksheets/sheet1.xml"
        );
        assert_eq!(resolve("xl/charts", "/xl/media/a.png"), "xl/media/a.png");
        assert_eq!(resolve("", "xl/workbook.xml"), "xl/workbook.xml");
    }

    #[test]
    fn parts_of_a_sheet() {
        let files = Files(HashMap::from([
            (
                "xl/workbook.xml",
                r#"<workbook xmlns:r="r"><sheets><sheet name="売上" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="t/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships><Relationship Id="rId1" Type="t/drawing" Target="../drawings/drawing1.xml"/><Relationship Id="rId2" Type="t/hyperlink" Target="https://example.com/" TargetMode="External"/></Relationships>"#,
            ),
            (
                "xl/theme/theme1.xml",
                r#"<a:theme xmlns:a="a"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:accent1><a:srgbClr val="4472c4"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#,
            ),
        ]));
        let sheet = files.sheet_part("売上").unwrap();
        assert_eq!(sheet, "xl/worksheets/sheet1.xml");
        assert!(files.sheet_part("other").is_none());
        let rels = files.rels(&sheet);
        assert_eq!(rels.len(), 1);
        assert_eq!(
            rels["rId1"],
            (
                "t/drawing".to_string(),
                "xl/drawings/drawing1.xml".to_string()
            )
        );
        assert!(files.rels("xl/drawings/drawing1.xml").is_empty());
        let theme = files.theme_colors();
        assert_eq!(
            (theme["dk1"].as_str(), theme["accent1"].as_str()),
            ("000000", "4472C4")
        );
    }
}
