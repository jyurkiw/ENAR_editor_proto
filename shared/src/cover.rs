//! The `<cover>` editor's model: read it from a supplement, and plan the surgical edits
//! that turn the file's cover into an edited model.
//!
//! In the model an empty string means "absent": an empty title removes `<headerTitle>`,
//! an empty colour removes `@color`.

use serde::{Deserialize, Serialize};

use crate::edit::{self, Anchor, Splice};
use crate::xml::parse;
use crate::StructNode;

/// The cover's children, in the order the schema requires.
pub const ORDER: &[&str] =
    &["headerTitle", "headerSubtitle", "coverImage", "footerChaser", "footerSubchaser", "author", "dmsguildLogo", "enigmaLogo"];

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverImage {
    /// @ref: an `<image>` id. Required.
    pub image_ref: String,
    pub margin: String,
    pub width: String,
    pub height: String,
    pub x_offset: String,
    pub y_offset: String,
}

impl CoverImage {
    /// (attribute, value) in the order they're written.
    fn attrs(&self) -> [(&'static str, &str); 6] {
        [
            ("ref", &self.image_ref),
            ("margin", &self.margin),
            ("width", &self.width),
            ("height", &self.height),
            ("xOffset", &self.x_offset),
            ("yOffset", &self.y_offset),
        ]
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverModel {
    /// False when the supplement has no `<cover>`.
    pub present: bool,
    pub color: String,
    pub shadow: bool,
    pub shadow_color: String,
    /// The deprecated `<cover id>`. Cleared when the form converts it to a coverImage.
    pub legacy_id: String,
    pub header_title: String,
    pub header_subtitle: String,
    pub image: Option<CoverImage>,
    pub footer_chaser: String,
    pub footer_subchaser: String,
    pub author: String,
    pub author_override: bool,
    pub dmsguild_logo: bool,
    /// The logo's attributes as written, shown read-only and kept as they are.
    pub logo_attrs: Vec<(String, String)>,
    /// The Enigma Narrative logo.
    pub enigma_logo: bool,
    /// Its @place: "above" (just above the DMs Guild logo), or empty / "opposite" (the
    /// lower-right corner).
    pub enigma_place: String,
    /// Its attributes as written (place among them), shown read-only and kept.
    pub enigma_attrs: Vec<(String, String)>,
    /// `<contents><enigmaLogo/></contents>`: the logo on the contents page instead (or as
    /// well). Not part of the cover, so it's read and kept with or without one.
    pub contents_logo: bool,
    /// Its attributes as written, shown read-only and kept.
    pub contents_attrs: Vec<(String, String)>,
}

fn truthy(v: Option<String>) -> bool {
    matches!(v.as_deref().map(str::trim), Some("true") | Some("1"))
}

/// The `<cover>` node, if the root has one.
pub fn cover_node(nodes: &[StructNode]) -> Option<usize> {
    nodes.first()?.children.iter().copied().find(|c| nodes[*c].tag == "cover")
}

fn child<'a>(nodes: &'a [StructNode], parent: &StructNode, tag: &str) -> Option<&'a StructNode> {
    parent.children.iter().map(|c| &nodes[*c]).find(|n| n.tag == tag)
}

/// The file's cover as a model. A file without a cover gives the empty model.
pub fn read(source: &str) -> Result<CoverModel, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    Ok(read_nodes(source, &nodes))
}

/// The top-level `<contents>` node and its `<enigmaLogo>`, if there are.
fn contents_nodes(nodes: &[StructNode]) -> (Option<usize>, Option<usize>) {
    let Some(c) = nodes.first().and_then(|r| r.children.iter().copied().find(|c| nodes[*c].tag == "contents")) else {
        return (None, None);
    };
    (Some(c), nodes[c].children.iter().copied().find(|k| nodes[*k].tag == "enigmaLogo"))
}

pub fn read_nodes(source: &str, nodes: &[StructNode]) -> CoverModel {
    let logo = contents_nodes(nodes).1.and_then(|n| edit::start_tag(source, nodes[n].start));
    let contents_attrs = logo.as_ref().map(|t| t.attrs.iter().map(|a| (a.name.clone(), a.value.clone())).collect()).unwrap_or_default();
    let m = read_cover(source, nodes);
    CoverModel { contents_logo: logo.is_some(), contents_attrs, ..m }
}

fn read_cover(source: &str, nodes: &[StructNode]) -> CoverModel {
    let Some(ci) = cover_node(nodes) else { return CoverModel::default() };
    let cover = &nodes[ci];
    let Some(tag) = edit::start_tag(source, cover.start) else { return CoverModel::default() };
    let text = |t: &str| child(nodes, cover, t).map(|n| edit::text_of(source, n)).unwrap_or_default();
    let image = child(nodes, cover, "coverImage").and_then(|n| edit::start_tag(source, n.start)).map(|t| CoverImage {
        image_ref: t.value("ref").unwrap_or_default(),
        margin: t.value("margin").unwrap_or_default(),
        width: t.value("width").unwrap_or_default(),
        height: t.value("height").unwrap_or_default(),
        x_offset: t.value("xOffset").unwrap_or_default(),
        y_offset: t.value("yOffset").unwrap_or_default(),
    });
    let author = child(nodes, cover, "author");
    let logo = child(nodes, cover, "dmsguildLogo");
    let enigma = child(nodes, cover, "enigmaLogo").and_then(|n| edit::start_tag(source, n.start));
    CoverModel {
        present: true,
        color: tag.value("color").unwrap_or_default(),
        shadow: truthy(tag.value("shadow")),
        shadow_color: tag.value("shadowColor").unwrap_or_default(),
        legacy_id: tag.value("id").unwrap_or_default(),
        header_title: text("headerTitle"),
        header_subtitle: text("headerSubtitle"),
        image,
        footer_chaser: text("footerChaser"),
        footer_subchaser: text("footerSubchaser"),
        author: author.map(|n| edit::text_of(source, n)).unwrap_or_default(),
        author_override: author.and_then(|n| edit::start_tag(source, n.start)).is_some_and(|t| truthy(t.value("override"))),
        dmsguild_logo: logo.is_some(),
        logo_attrs: logo
            .and_then(|n| edit::start_tag(source, n.start))
            .map(|t| t.attrs.iter().map(|a| (a.name.clone(), a.value.clone())).collect())
            .unwrap_or_default(),
        enigma_logo: enigma.is_some(),
        enigma_place: enigma.as_ref().and_then(|t| t.value("place")).unwrap_or_default(),
        enigma_attrs: enigma.map(|t| t.attrs.iter().map(|a| (a.name.clone(), a.value.clone())).collect()).unwrap_or_default(),
        ..Default::default()
    }
}

fn some(s: &str) -> Option<&str> {
    (!s.trim().is_empty()).then_some(s)
}

/// Is this child present in the model?
fn wanted(m: &CoverModel, tag: &str) -> bool {
    match tag {
        "headerTitle" => some(&m.header_title).is_some(),
        "headerSubtitle" => some(&m.header_subtitle).is_some(),
        "coverImage" => m.image.is_some(),
        "footerChaser" => some(&m.footer_chaser).is_some(),
        "footerSubchaser" => some(&m.footer_subchaser).is_some(),
        "author" => some(&m.author).is_some(),
        "dmsguildLogo" => m.dmsguild_logo,
        "enigmaLogo" => m.enigma_logo,
        _ => false,
    }
}

fn attr_xml(name: &str, value: &str) -> String {
    format!(" {name}=\"{}\"", edit::escape_attr(value, '"'))
}

/// A new child element's XML.
fn child_xml(m: &CoverModel, tag: &str) -> String {
    let text = |t: &str, s: &str| format!("<{t}>{}</{t}>", edit::escape_text(s));
    match tag {
        "headerTitle" => text(tag, &m.header_title),
        "headerSubtitle" => text(tag, &m.header_subtitle),
        "footerChaser" => text(tag, &m.footer_chaser),
        "footerSubchaser" => text(tag, &m.footer_subchaser),
        "coverImage" => {
            let img = m.image.clone().unwrap_or_default();
            let attrs: String = img
                .attrs()
                .iter()
                .filter(|(n, v)| *n == "ref" || some(v).is_some())
                .map(|(n, v)| attr_xml(n, v))
                .collect();
            format!("<coverImage{attrs}/>")
        }
        "author" => {
            let ov = if m.author_override { " override=\"true\"" } else { "" };
            format!("<author{ov}>{}</author>", edit::escape_text(&m.author))
        }
        "dmsguildLogo" => "<dmsguildLogo/>".into(),
        "enigmaLogo" if m.enigma_place == "above" => "<enigmaLogo place=\"above\"/>".into(),
        "enigmaLogo" => "<enigmaLogo/>".into(),
        _ => String::new(),
    }
}

/// The cover's start-tag attributes in the model, in writing order.
fn cover_attrs(m: &CoverModel) -> Vec<(&'static str, String)> {
    let mut v = Vec::new();
    if let Some(id) = some(&m.legacy_id) {
        v.push(("id", id.to_string()));
    }
    if let Some(c) = some(&m.color) {
        v.push(("color", c.to_string()));
    }
    if m.shadow {
        v.push(("shadow", "true".to_string()));
    }
    if let Some(c) = some(&m.shadow_color) {
        v.push(("shadowColor", c.to_string()));
    }
    v
}

/// A whole new `<cover>` element, children indented one level.
fn cover_xml(m: &CoverModel, unit: &str) -> String {
    let attrs: String = cover_attrs(m).iter().map(|(n, v)| attr_xml(n, v)).collect();
    let children: Vec<String> = ORDER.iter().filter(|t| wanted(m, t)).map(|t| format!("{unit}{}", child_xml(m, t))).collect();
    if children.is_empty() {
        format!("<cover{attrs}/>")
    } else {
        format!("<cover{attrs}>\n{}\n</cover>", children.join("\n"))
    }
}

/// The source with its cover changed to `m`. Unchanged parts of the file, including
/// comments inside the cover, are kept exactly.
pub fn plan(source: &str, m: &CoverModel) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let splices = plan_splices(source, &nodes, m)?;
    edit::apply(source, &splices)
}

fn plan_splices(source: &str, nodes: &[StructNode], m: &CoverModel) -> Result<Vec<Splice>, String> {
    let mut out = cover_splices(source, nodes, m)?;
    out.extend(contents_splices(source, nodes, m)?);
    Ok(out)
}

/// Adds or removes `<contents><enigmaLogo/></contents>`. A new `<contents>` goes right after
/// the cover (or first, where there's none); removing the logo leaves `<contents>` and any
/// comments in it.
fn contents_splices(source: &str, nodes: &[StructNode], m: &CoverModel) -> Result<Vec<Splice>, String> {
    let (contents, logo) = contents_nodes(nodes);
    Ok(match (m.contents_logo, contents, logo) {
        (false, _, Some(l)) => vec![edit::remove_element(source, &nodes[l])],
        (true, Some(c), None) => {
            vec![edit::insert_child(source, Anchor::Into(&nodes[c]), "<enigmaLogo/>").ok_or("can't place <enigmaLogo>")?]
        }
        (true, None, _) => {
            let unit = edit::indent_unit(source);
            let xml = format!("<contents>\n{unit}<enigmaLogo/>\n</contents>");
            let root = &nodes[0];
            let mut s = match (cover_node(nodes), root.children.first()) {
                (Some(c), _) => {
                    let mut s = edit::insert_child(source, Anchor::After(&nodes[c]), &xml).ok_or("can't place <contents>")?;
                    s.text.insert(0, '\n');
                    s
                }
                (None, Some(first)) => edit::insert_child(source, Anchor::Before(&nodes[*first]), &xml).ok_or("can't place <contents>")?,
                (None, None) => edit::insert_child(source, Anchor::Into(root), &xml).ok_or("can't place <contents>")?,
            };
            if cover_node(nodes).is_none() && !root.children.is_empty() {
                s.text.push('\n');
            }
            vec![s]
        }
        _ => vec![],
    })
}

fn cover_splices(source: &str, nodes: &[StructNode], m: &CoverModel) -> Result<Vec<Splice>, String> {
    let old = read_nodes(source, nodes);
    let mut out = Vec::new();

    let Some(ci) = cover_node(nodes) else {
        if !m.present {
            return Ok(out);
        }
        // A new cover goes first in <supplement>, followed by a blank line.
        let root = &nodes[0];
        let xml = cover_xml(m, edit::indent_unit(source));
        let s = match root.children.first() {
            Some(first) => {
                let mut s = edit::insert_child(source, Anchor::Before(&nodes[*first]), &xml).ok_or("can't place the cover")?;
                s.text.push('\n');
                s
            }
            None => edit::insert_child(source, Anchor::Into(root), &xml).ok_or("can't place the cover")?,
        };
        out.push(s);
        return Ok(out);
    };
    let cover = &nodes[ci];

    if !m.present {
        out.push(edit::remove_element(source, cover));
        return Ok(out);
    }

    // Start-tag attributes.
    let shadow_attr = if m.shadow == old.shadow {
        None // keep whatever is written (shadow="false" stays)
    } else if m.shadow {
        Some(Some("true"))
    } else {
        Some(None)
    };
    for (name, new) in [
        ("id", Some(some(&m.legacy_id))),
        ("color", Some(some(&m.color))),
        ("shadow", shadow_attr),
        ("shadowColor", Some(some(&m.shadow_color))),
    ] {
        if let Some(value) = new {
            out.extend(edit::set_attr(source, cover, name, value));
        }
    }

    // Children, in schema order. Which existing children stay decides where new ones go.
    let existing: Vec<Option<&StructNode>> = ORDER.iter().map(|t| child(nodes, cover, t)).collect();
    let stays: Vec<bool> = ORDER.iter().zip(&existing).map(|(t, e)| e.is_some() && wanted(m, t)).collect();
    let mut into: Vec<String> = Vec::new();
    for (i, tag) in ORDER.iter().enumerate() {
        match (existing[i], wanted(m, tag)) {
            (Some(n), false) => out.push(edit::remove_element(source, n)),
            (None, true) => {
                let xml = child_xml(m, tag);
                let anchor = if let Some(j) = (0..i).rev().find(|j| stays[*j]) {
                    Anchor::After(existing[j].unwrap())
                } else if let Some(j) = (i + 1..ORDER.len()).find(|j| stays[*j]) {
                    Anchor::Before(existing[j].unwrap())
                } else {
                    // No sibling stays: every new child goes in together, below.
                    into.push(xml);
                    continue;
                };
                out.push(edit::insert_child(source, anchor, &xml).ok_or_else(|| format!("can't place <{tag}>"))?);
            }
            (Some(n), true) => out.extend(edit_child(source, n, m, &old, tag)),
            (None, false) => {}
        }
    }
    if !into.is_empty() {
        out.push(edit::insert_child(source, Anchor::Into(cover), &into.join("
")).ok_or("can't fill the cover")?);
    }
    // Several inserts can share a point (all after the same sibling); they must stay in
    // schema order, which they are, because they were pushed in it. Removals and inserts
    // that share a boundary are fine: an insert is zero-width.
    Ok(out)
}

/// Edits to a child that stays.
fn edit_child(source: &str, n: &StructNode, m: &CoverModel, old: &CoverModel, tag: &str) -> Vec<Splice> {
    let mut out = Vec::new();
    match tag {
        "headerTitle" => out.extend(edit::set_text(source, n, &m.header_title)),
        "headerSubtitle" => out.extend(edit::set_text(source, n, &m.header_subtitle)),
        "footerChaser" => out.extend(edit::set_text(source, n, &m.footer_chaser)),
        "footerSubchaser" => out.extend(edit::set_text(source, n, &m.footer_subchaser)),
        "author" => {
            if m.author_override != old.author_override {
                out.extend(edit::set_attr(source, n, "override", m.author_override.then_some("true")));
            }
            out.extend(edit::set_text(source, n, &m.author));
        }
        "coverImage" => {
            let img = m.image.clone().unwrap_or_default();
            for (name, value) in img.attrs() {
                let value = if name == "ref" { Some(value) } else { some(value) };
                out.extend(edit::set_attr(source, n, name, value));
            }
        }
        "enigmaLogo" => {
            // Only @place is edited; "opposite" is the default, so it isn't written.
            if m.enigma_place != old.enigma_place {
                out.extend(edit::set_attr(source, n, "place", (m.enigma_place == "above").then_some("above")));
            }
        }
        _ => {} // dmsguildLogo: kept exactly as written
    }
    out
}

#[cfg(test)]
mod contents_tests {
    use super::*;

    const SRC: &str = "<supplement type=\"rules\">\n    <cover>\n        <headerTitle>T</headerTitle>\n    </cover>\n\n    <chapter name=\"a\"/>\n</supplement>\n";

    #[test]
    fn add_and_remove_the_contents_logo() {
        let mut m = read(SRC).unwrap();
        assert!(!m.contents_logo);
        m.contents_logo = true;
        let out = plan(SRC, &m).unwrap();
        assert_eq!(
            out,
            "<supplement type=\"rules\">\n    <cover>\n        <headerTitle>T</headerTitle>\n    </cover>\n\n    <contents>\n        <enigmaLogo/>\n    </contents>\n\n    <chapter name=\"a\"/>\n</supplement>\n"
        );
        let mut m = read(&out).unwrap();
        assert!(m.contents_logo && m.contents_attrs.is_empty());
        assert_eq!(plan(&out, &m).unwrap(), out, "unchanged");
        m.contents_logo = false;
        let gone = plan(&out, &m).unwrap();
        assert!(!gone.contains("enigmaLogo") && gone.contains("<contents>"), "{gone}");
        // Back into the now-empty <contents>.
        let mut m = read(&gone).unwrap();
        m.contents_logo = true;
        assert!(read(&plan(&gone, &m).unwrap()).unwrap().contents_logo);
    }

    #[test]
    fn kept_without_a_cover_and_when_the_cover_goes() {
        let src = "<supplement type=\"rules\">\n    <cover/>\n    <contents>\n        <enigmaLogo width=\"1in\"/>\n    </contents>\n    <chapter name=\"a\"/>\n</supplement>\n";
        let m = read(src).unwrap();
        assert_eq!(m.contents_attrs, vec![("width".to_string(), "1in".to_string())]);
        // Removing the cover keeps the contents logo.
        let removed = CoverModel { contents_logo: m.contents_logo, contents_attrs: m.contents_attrs.clone(), ..Default::default() };
        let out = plan(src, &removed).unwrap();
        assert!(!out.contains("<cover") && out.contains("<enigmaLogo width=\"1in\"/>"), "{out}");
        assert!(read(&out).unwrap().contents_logo);
        // No cover at all: a new <contents> goes first.
        let bare = "<supplement type=\"rules\">\n    <chapter name=\"a\"/>\n</supplement>\n";
        let m = CoverModel { contents_logo: true, ..Default::default() };
        assert_eq!(
            plan(bare, &m).unwrap(),
            "<supplement type=\"rules\">\n    <contents>\n        <enigmaLogo/>\n    </contents>\n\n    <chapter name=\"a\"/>\n</supplement>\n"
        );
    }
}

#[cfg(test)]
mod enigma_tests {
    use super::*;

    const SRC: &str = "<supplement type=\"rules\">\n    <cover>\n        <headerTitle>T</headerTitle>\n        <dmsguildLogo/>\n    </cover>\n\n    <chapter name=\"a\"/>\n</supplement>\n";

    #[test]
    fn add_move_and_remove_the_enigma_logo() {
        let old = read(SRC).unwrap();
        assert!(!old.enigma_logo);
        let mut m = old.clone();
        m.enigma_logo = true;
        let out = plan(SRC, &m).unwrap();
        assert!(out.contains("        <dmsguildLogo/>\n        <enigmaLogo/>\n    </cover>"), "{out}");
        // Above the DMs Guild logo.
        let old = read(&out).unwrap();
        assert!(old.enigma_logo && old.enigma_place.is_empty());
        let mut m = old.clone();
        m.enigma_place = "above".into();
        let out2 = plan(&out, &m).unwrap();
        assert!(out2.contains("<enigmaLogo place=\"above\"/>"), "{out2}");
        assert_eq!(read(&out2).unwrap().enigma_place, "above");
        // Back to opposite drops the attribute; unticking drops the element.
        let mut m = read(&out2).unwrap();
        m.enigma_place = "opposite".into();
        assert!(plan(&out2, &m).unwrap().contains("<enigmaLogo/>"));
        m.enigma_logo = false;
        assert!(!plan(&out2, &m).unwrap().contains("enigmaLogo"));
        // A new cover with it above.
        let mut m = read(SRC).unwrap();
        m.enigma_logo = true;
        m.enigma_place = "above".into();
        assert!(plan(SRC, &m).unwrap().contains("<dmsguildLogo/>\n        <enigmaLogo place=\"above\"/>"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<?xml version="1.0"?>
<supplement type="adventure">
    <cover color="titlered" shadow="true" shadowColor="black!40">
        <headerTitle>Template Adventure</headerTitle>               <!-- Main title -->
        <headerSubtitle>Templates of Adventure</headerSubtitle>     <!-- Line under the title -->
        <!-- COVERIMAGE: cover art -->
        <coverImage ref="img-cover" margin="0.5in" yOffset="-1in"/>
        <footerChaser>A template adventure for 5e</footerChaser>
        <author>Author Name</author>  <!-- Bottom line of the cover.
                                           override="true" prints the text exactly -->
        <!-- The DMs Guild logo -->
        <dmsguildLogo width="1in" margin="0.5in"/>
    </cover>

    <chapterOrder><ch name="x"/></chapterOrder>
</supplement>
"#;

    #[test]
    fn reads_every_field() {
        let m = read(SRC).unwrap();
        assert!(m.present && m.shadow && m.dmsguild_logo);
        assert_eq!(m.color, "titlered");
        assert_eq!(m.shadow_color, "black!40");
        assert_eq!(m.header_subtitle, "Templates of Adventure");
        assert_eq!(m.footer_subchaser, "");
        assert_eq!(m.author, "Author Name");
        let img = m.image.unwrap();
        assert_eq!((img.image_ref.as_str(), img.y_offset.as_str(), img.width.as_str()), ("img-cover", "-1in", ""));
        assert_eq!(m.logo_attrs.len(), 2);
    }

    #[test]
    fn unchanged_model_changes_nothing() {
        assert_eq!(plan(SRC, &read(SRC).unwrap()).unwrap(), SRC);
    }

    #[test]
    fn edits_keep_comments() {
        let mut m = read(SRC).unwrap();
        m.header_title = "Rats & Ruin".into();
        m.shadow = false;
        m.footer_subchaser = "Part 2".into();
        m.author_override = true;
        m.image.as_mut().unwrap().margin.clear();
        m.image.as_mut().unwrap().width = "6in".into();
        let out = plan(SRC, &m).unwrap();
        assert!(out.contains("<headerTitle>Rats &amp; Ruin</headerTitle>               <!-- Main title -->"));
        assert!(out.contains(r#"<cover color="titlered" shadowColor="black!40">"#));
        assert!(out.contains("        <footerChaser>A template adventure for 5e</footerChaser>\n        <footerSubchaser>Part 2</footerSubchaser>\n        <author override=\"true\">"));
        assert!(out.contains(r#"<coverImage ref="img-cover" yOffset="-1in" width="6in"/>"#));
        assert!(out.contains("<!-- COVERIMAGE: cover art -->"));
        assert_eq!(read(&out).unwrap(), m);
    }

    #[test]
    fn removing_children_drops_their_lines() {
        let mut m = read(SRC).unwrap();
        m.dmsguild_logo = false;
        m.logo_attrs.clear();
        m.author.clear();
        m.image = None;
        let out = plan(SRC, &m).unwrap();
        assert!(!out.contains("dmsguildLogo"));
        assert!(!out.contains("<author"));
        assert!(!out.contains("override=\"true\" prints"), "the author's trailing comment goes with it");
        assert!(!out.contains("<coverImage"));
        assert!(out.contains("<!-- The DMs Guild logo -->"), "comments on their own lines stay");
        assert_eq!(read(&out).unwrap(), m);
    }

    #[test]
    fn adds_cover_to_file_without_one() {
        let src = "<supplement type=\"rules\">\n    <chapter name=\"a\"/>\n</supplement>\n";
        let m = CoverModel { present: true, header_title: "New".into(), dmsguild_logo: true, ..Default::default() };
        let out = plan(src, &m).unwrap();
        assert_eq!(
            out,
            "<supplement type=\"rules\">\n    <cover>\n        <headerTitle>New</headerTitle>\n        <dmsguildLogo/>\n    </cover>\n\n    <chapter name=\"a\"/>\n</supplement>\n"
        );
        assert_eq!(read(&out).unwrap().header_title, "New");
    }

    #[test]
    fn fills_empty_cover_and_converts_legacy_id() {
        let src = "<supplement>\n    <cover id=\"img\"/>\n    <chapterOrder/>\n</supplement>";
        let mut m = read(src).unwrap();
        assert_eq!(m.legacy_id, "img");
        m.legacy_id.clear();
        m.image = Some(CoverImage { image_ref: "img".into(), ..Default::default() });
        m.header_title = "T".into();
        let out = plan(src, &m).unwrap();
        assert_eq!(
            out,
            "<supplement>\n    <cover>\n        <headerTitle>T</headerTitle>\n        <coverImage ref=\"img\"/>\n    </cover>\n    <chapterOrder/>\n</supplement>"
        );
    }
}
