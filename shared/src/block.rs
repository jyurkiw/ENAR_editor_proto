//! Sections and sidebars: the prose containers inside a chapter.
//!
//! Both have a header (section: @name, @credit; sidebar: @id, @style; both: an optional
//! `<title>` and `<subtitle>`) and a body of blocks. This first editor treats each
//! `<p>` as raw XML text, and every other body child as a read-only block. Paragraphs
//! can be edited, added, deleted and moved; any body child can be moved.

use serde::{Deserialize, Serialize};

use crate::chapter;
use crate::display::reindent;
use crate::edit::{self, Anchor, Splice};
use crate::xml::parse;
use crate::StructNode;

/// Which container. Section names aren't unique (two chapters can each have a "setup"),
/// so a section is its chapter, its name, and which section of that name it is there.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Container {
    Section { chapter: String, name: String, nth: usize },
    Sidebar { chapter: String, id: String },
}

impl Container {
    pub fn tag(&self) -> &'static str {
        match self {
            Container::Section { .. } => "section",
            Container::Sidebar { .. } => "sidebar",
        }
    }
}

/// The header fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Header {
    /// Section @name or sidebar @id.
    pub key: String,
    /// Section @credit or sidebar @style.
    pub extra: String,
    pub title: String,
    pub subtitle: String,
}

/// One body child.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Its index among the container's children (title and subtitle included).
    pub child: usize,
    pub tag: String,
    /// A `<p>`'s contents as XML, dedented; for other blocks, a one-line summary.
    pub text: String,
    /// For other blocks: their source, re-indented.
    pub source: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum BlockEdit {
    Header { at: Container, header: Header },
    /// Replace a paragraph's contents (raw XML).
    SetP { at: Container, child: usize, text: String },
    /// A new empty paragraph after child `after`, or first in the body when None.
    AddP { at: Container, after: Option<usize> },
    /// Delete a paragraph.
    DeleteP { at: Container, child: usize },
    /// Move a body child before another, or to the end of the body.
    Move { at: Container, child: usize, before: Option<usize> },
}

impl BlockEdit {
    pub fn container(&self) -> &Container {
        match self {
            BlockEdit::Header { at, .. }
            | BlockEdit::SetP { at, .. }
            | BlockEdit::AddP { at, .. }
            | BlockEdit::DeleteP { at, .. }
            | BlockEdit::Move { at, .. } => at,
        }
    }
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

/// The container's node.
pub fn find(source: &str, nodes: &[StructNode], at: &Container) -> Option<usize> {
    match at {
        Container::Section { chapter, name, nth } => {
            let ci = chapter::find_chapter(source, nodes, chapter)?;
            nodes[ci]
                .children
                .iter()
                .copied()
                .filter(|c| nodes[*c].tag == "section" && attr(source, &nodes[*c], "name").as_deref() == Some(name))
                .nth(*nth)
        }
        Container::Sidebar { chapter, id } => {
            let ci = chapter::find_chapter(source, nodes, chapter)?;
            nodes[ci]
                .children
                .iter()
                .copied()
                .find(|c| nodes[*c].tag == "sidebar" && attr(source, &nodes[*c], "id").as_deref() == Some(id))
        }
    }
}

/// The container a node is, if it's a section or sidebar directly in a top-level chapter.
pub fn container_of(source: &str, nodes: &[StructNode], parents: &[Option<usize>], idx: usize) -> Option<Container> {
    let n = nodes.get(idx)?;
    let ch = parents.get(idx).copied().flatten()?;
    if nodes[ch].tag != "chapter" || !nodes[0].children.contains(&ch) {
        return None;
    }
    let chapter = attr(source, &nodes[ch], "name")?;
    match n.tag.as_str() {
        "section" => {
            let name = attr(source, n, "name").unwrap_or_default();
            let nth = nodes[ch]
                .children
                .iter()
                .take_while(|c| **c != idx)
                .filter(|c| nodes[**c].tag == "section" && attr(source, &nodes[**c], "name").as_deref() == Some(name.as_str()))
                .count();
            Some(Container::Section { chapter, name, nth })
        }
        "sidebar" => Some(Container::Sidebar { chapter, id: attr(source, n, "id").unwrap_or_default() }),
        _ => None,
    }
}

fn is_header(tag: &str) -> bool {
    tag == "title" || tag == "subtitle"
}

/// A paragraph's contents for the text box: one-line contents as written; multi-line
/// contents without their surrounding blank lines and common indentation.
pub fn p_text(source: &str, n: &StructNode) -> String {
    let Some((a, b)) = edit::inner_span(source, n) else { return String::new() };
    let inner = &source[a..b];
    if !inner.contains('\n') {
        return inner.to_string();
    }
    let lines: Vec<&str> = inner.lines().map(|l| l.trim_end_matches('\r')).collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else { return String::new() };
    let body = &lines[first..=last];
    let indent = body.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
    body.iter().map(|l| if l.len() >= indent { &l[indent..] } else { l.trim_start() }).collect::<Vec<_>>().join("\n")
}

/// A one-line description of a non-paragraph block.
fn summary(source: &str, nodes: &[StructNode], n: &StructNode) -> String {
    let a = |k: &str| attr(source, n, k).filter(|v| !v.is_empty());
    let first_p = || {
        n.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == "p").map(|p| edit::snippet(&edit::plain_text(source, p), 60))
    };
    let count = |tag: &str| n.children.iter().filter(|c| nodes[**c].tag == tag).count();
    match n.tag.as_str() {
        "dialog" => {
            let who = a("speaker").map(|s| format!("{s}: ")).unwrap_or_default();
            format!("{who}{}", first_p().unwrap_or_default())
        }
        "speakAloud" => first_p().unwrap_or_default(),
        "list" => {
            let kind = if a("bullet").as_deref() == Some("number") { "numbered" } else { "bulleted" };
            format!("{} {kind} item(s)", count("li"))
        }
        "image" | "figure" => [a("id"), a("name"), a("src")].into_iter().flatten().collect::<Vec<_>>().join(" · "),
        "table" => n
            .children
            .iter()
            .map(|c| &nodes[*c])
            .find(|c| c.tag == "title")
            .map(|t| edit::plain_text(source, t))
            .or_else(|| a("id").map(|i| format!("{i} (no title)")))
            .unwrap_or_default(),
        "pageEnd" => "Page end".into(),
        _ => [a("ref"), a("id"), a("name")].into_iter().flatten().next().unwrap_or_else(|| edit::snippet(&edit::plain_text(source, n), 60)),
    }
}

/// The header and body of the container at node `ci`.
pub fn read(source: &str, nodes: &[StructNode], ci: usize) -> (Header, Vec<Block>) {
    let c = &nodes[ci];
    let (key_attr, extra_attr) = if c.tag == "sidebar" { ("id", "style") } else { ("name", "credit") };
    let text = |tag: &str| c.children.iter().map(|k| &nodes[*k]).find(|k| k.tag == tag).map(|k| edit::text_of(source, k)).unwrap_or_default();
    let header = Header {
        key: attr(source, c, key_attr).unwrap_or_default(),
        extra: attr(source, c, extra_attr).unwrap_or_default(),
        title: text("title"),
        subtitle: text("subtitle"),
    };
    let blocks = c
        .children
        .iter()
        .enumerate()
        .filter(|(_, k)| !is_header(&nodes[**k].tag))
        .map(|(i, k)| {
            let n = &nodes[*k];
            if n.tag == "p" {
                Block { child: i, tag: "p".into(), text: p_text(source, n), source: String::new() }
            } else {
                Block { child: i, tag: n.tag.clone(), text: summary(source, nodes, n), source: reindent(source, n.start, n.end) }
            }
        })
        .collect();
    (header, blocks)
}

fn some(s: &str) -> Option<&str> {
    (!s.trim().is_empty()).then_some(s)
}

/// New paragraph contents, laid out like the old: one line stays one line; text with
/// line breaks (or a paragraph that was written over several lines) goes on its own
/// lines, one level deeper than the `<p>`.
fn p_inner(source: &str, n: &StructNode, text: &str) -> String {
    let multi = text.contains('\n') || edit::inner_span(source, n).is_some_and(|(a, b)| source[a..b].contains('\n'));
    if !multi || text.trim().is_empty() {
        return text.trim().to_string();
    }
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let ind = edit::line_indent(source, n.start);
    let inner = format!("{ind}{}", edit::indent_unit(source));
    let body: Vec<String> = text.trim_matches('\n').lines().map(|l| if l.trim().is_empty() { String::new() } else { format!("{inner}{l}") }).collect();
    format!("{nl}{}{nl}{ind}", body.join(nl))
}

/// Replaces a mixed-content element's contents (raw XML), laid out like before; opens
/// up `<x/>`.
pub fn set_inner(source: &str, n: &StructNode, text: &str) -> Option<Splice> {
    match edit::inner_span(source, n) {
        Some((a, b)) => Some(Splice { start: a, end: b, text: p_inner(source, n, text) }),
        None => {
            let t = edit::start_tag(source, n.start)?;
            Some(Splice { start: t.close_at, end: t.end, text: format!(">{}</{}>", text.trim(), n.tag) })
        }
    }
}

fn header_splices(source: &str, nodes: &[StructNode], ci: usize, h: &Header) -> Result<Vec<Splice>, String> {
    let c = &nodes[ci];
    let (old, _) = read(source, nodes, ci);
    let (key_attr, extra_attr) = if c.tag == "sidebar" { ("id", "style") } else { ("name", "credit") };
    let mut out = Vec::new();
    if h.key != old.key {
        if h.key.trim().is_empty() {
            return Err(format!("a {} needs {}", c.tag, if c.tag == "sidebar" { "an id" } else { "a name" }));
        }
        out.extend(edit::set_attr(source, c, key_attr, Some(&h.key)));
    }
    if h.extra != old.extra {
        out.extend(edit::set_attr(source, c, extra_attr, some(&h.extra)));
    }
    let find = |tag: &str| c.children.iter().map(|k| &nodes[*k]).find(|k| k.tag == tag);
    let (title, subtitle) = (find("title"), find("subtitle"));
    let first_body = c.children.iter().map(|k| &nodes[*k]).find(|k| !is_header(&k.tag));
    // Title: edit, remove, or add first.
    match (title, some(&h.title)) {
        (Some(t), Some(v)) if v != old.title => out.extend(edit::set_text(source, t, v)),
        (Some(t), None) => out.push(edit::remove_element(source, t)),
        (None, Some(v)) => {
            let xml = format!("<title>{}</title>", edit::escape_text(v));
            let anchor = match c.children.first() {
                Some(f) => Anchor::Before(&nodes[*f]),
                None => Anchor::Into(c),
            };
            out.push(edit::insert_child(source, anchor, &xml).ok_or("can't place the title")?);
        }
        _ => {}
    }
    // Subtitle: edit, remove, or add after the title (or first).
    match (subtitle, some(&h.subtitle)) {
        (Some(s), Some(v)) if v != old.subtitle => out.extend(edit::set_text(source, s, v)),
        (Some(s), None) => out.push(edit::remove_element(source, s)),
        (None, Some(v)) => {
            let xml = format!("<subtitle>{}</subtitle>", edit::escape_text(v));
            let keeps_title = title.is_some() && some(&h.title).is_some();
            // After a title that stays; otherwise before the first block (a new title
            // inserted at the same point goes first, as it was pushed first).
            let anchor = match (keeps_title, first_body) {
                (true, _) => Anchor::After(title.unwrap()),
                (false, Some(b)) => Anchor::Before(b),
                (false, None) => Anchor::Into(c),
            };
            out.push(edit::insert_child(source, anchor, &xml).ok_or("can't place the subtitle")?);
        }
        _ => {}
    }
    Ok(out)
}

/// The renamed sidebar's `<sidebarLink>`s.
fn rename_sidebar_links(source: &str, old: &str, new: &str) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let splices: Vec<Splice> = nodes
        .iter()
        .filter(|n| n.tag == "sidebarLink" && attr(source, n, "id").as_deref() == Some(old))
        .filter_map(|n| edit::set_attr(source, n, "id", Some(new)))
        .collect();
    edit::apply(source, &splices)
}

/// How many `<sidebarLink>`s name this sidebar.
pub fn sidebar_links(nodes: &[StructNode], source: &str, id: &str) -> usize {
    nodes.iter().filter(|n| n.tag == "sidebarLink" && attr(source, n, "id").as_deref() == Some(id)).count()
}

fn child<'a>(nodes: &'a [StructNode], c: &StructNode, i: usize) -> Result<&'a StructNode, String> {
    c.children.get(i).map(|k| &nodes[*k]).ok_or_else(|| "that block isn't there any more; reopen the file".to_string())
}

pub fn plan(source: &str, e: &BlockEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let at = e.container();
    let ci = find(source, &nodes, at).ok_or_else(|| format!("can't find that {} any more", at.tag()))?;
    let c = &nodes[ci];
    match e {
        BlockEdit::Header { header, .. } => {
            let text = edit::apply(source, &header_splices(source, &nodes, ci, header)?)?;
            match at {
                Container::Sidebar { id, .. } if *id != header.key => rename_sidebar_links(&text, id, &header.key),
                _ => Ok(text),
            }
        }
        BlockEdit::SetP { child: i, text, .. } => {
            let p = child(&nodes, c, *i)?;
            if p.tag != "p" {
                return Err("that block isn't a paragraph".into());
            }
            edit::apply(source, &[set_inner(source, p, text).ok_or("bad <p>")?])
        }
        BlockEdit::AddP { after, .. } => {
            let anchor = match after {
                Some(i) => Anchor::After(child(&nodes, c, *i)?),
                None => {
                    // First in the body: after the title/subtitle, before the first block.
                    match c.children.iter().map(|k| &nodes[*k]).find(|k| !is_header(&k.tag)) {
                        Some(b) => Anchor::Before(b),
                        None => match c.children.last() {
                            Some(l) => Anchor::After(&nodes[*l]),
                            None => Anchor::Into(c),
                        },
                    }
                }
            };
            edit::apply(source, &[edit::insert_child(source, anchor, "<p></p>").ok_or("can't place the paragraph")?])
        }
        BlockEdit::DeleteP { child: i, .. } => {
            let p = child(&nodes, c, *i)?;
            if p.tag != "p" {
                return Err("only paragraphs can be deleted here".into());
            }
            edit::apply(source, &[edit::remove_element(source, p)])
        }
        BlockEdit::Move { child: i, before, .. } => {
            let n = child(&nodes, c, *i)?;
            if before == &Some(*i) || before.is_some_and(|b| b == i + 1) {
                return Ok(source.to_string());
            }
            if before.is_none() && c.children.len() == i + 1 {
                return Ok(source.to_string());
            }
            let target = match before {
                Some(b) => Some(child(&nodes, c, *b)?),
                None => None,
            };
            let last = c.children.last().map(|l| &nodes[*l]);
            edit::apply(source, &edit::move_element(source, n, target, last)?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement>
    <chapter name="c">
        <chapterTitle>C</chapterTitle>
        <section name="s" credit="A">
            <title>Town</title>
            <p>One <b>bold</b> line.</p>
            <speakAloud>
                <p>Read this aloud.</p>
            </speakAloud>
            <p>
                Two lines,
                  indented more.
            </p>
            <sidebarLink id="sb"/>
        </section>
        <section name="s"><p>Second s.</p></section>
        <sidebar id="sb">
            <p>Aside.</p>
        </sidebar>
    </chapter>
    <chapterOrder><ch name="c"/></chapterOrder>
</supplement>
"#;

    fn sec(nth: usize) -> Container {
        Container::Section { chapter: "c".into(), name: "s".into(), nth }
    }

    fn blocks(src: &str, at: &Container) -> (Header, Vec<Block>) {
        let nodes = parse(src).unwrap().nodes;
        read(src, &nodes, find(src, &nodes, at).unwrap())
    }

    #[test]
    fn reads_header_and_blocks() {
        let (h, b) = blocks(SRC, &sec(0));
        assert_eq!((h.key.as_str(), h.extra.as_str(), h.title.as_str()), ("s", "A", "Town"));
        let tags: Vec<_> = b.iter().map(|b| b.tag.as_str()).collect();
        assert_eq!(tags, ["p", "speakAloud", "p", "sidebarLink"]);
        assert_eq!(b[0].text, "One <b>bold</b> line.");
        assert_eq!(b[1].text, "Read this aloud.");
        assert_eq!(b[2].text, "Two lines,\n  indented more.");
        assert_eq!(b[3].text, "sb");
        assert_eq!(b[0].child, 1, "child indexes count the title");
        let (_, b2) = blocks(SRC, &sec(1));
        assert_eq!(b2[0].text, "Second s.");
    }

    #[test]
    fn container_of_counts_duplicate_names() {
        let p = parse(SRC).unwrap();
        let mut parents = vec![None; p.nodes.len()];
        for (i, n) in p.nodes.iter().enumerate() {
            for c in &n.children {
                parents[*c] = Some(i);
            }
        }
        let secs: Vec<_> = (0..p.nodes.len()).filter_map(|i| container_of(SRC, &p.nodes, &parents, i)).collect();
        assert_eq!(secs, vec![sec(0), sec(1), Container::Sidebar { chapter: "c".into(), id: "sb".into() }]);
    }

    #[test]
    fn set_paragraph_keeps_layout() {
        let out = plan(SRC, &BlockEdit::SetP { at: sec(0), child: 1, text: "One <i>new</i> line.".into() }).unwrap();
        assert!(out.contains("<p>One <i>new</i> line.</p>"));
        let out = plan(SRC, &BlockEdit::SetP { at: sec(0), child: 3, text: "Now\none more.".into() }).unwrap();
        assert!(out.contains("            <p>\n                Now\n                one more.\n            </p>"), "{out}");
        // Unchanged text gives the same file back.
        let (_, b) = blocks(SRC, &sec(0));
        assert_eq!(plan(SRC, &BlockEdit::SetP { at: sec(0), child: 3, text: b[2].text.clone() }).unwrap(), SRC);
    }

    #[test]
    fn add_delete_move() {
        let out = plan(SRC, &BlockEdit::AddP { at: sec(0), after: Some(2) }).unwrap();
        assert!(out.contains("            </speakAloud>\n            <p></p>\n            <p>\n"), "{out}");
        let out = plan(SRC, &BlockEdit::AddP { at: sec(0), after: None }).unwrap();
        assert!(out.contains("<title>Town</title>\n            <p></p>\n            <p>One"), "{out}");
        let out = plan(SRC, &BlockEdit::DeleteP { at: sec(0), child: 3 }).unwrap();
        assert!(!out.contains("Two lines"));
        assert!(plan(SRC, &BlockEdit::DeleteP { at: sec(0), child: 2 }).is_err(), "speakAloud isn't a paragraph");
        // Move the multi-line paragraph before the first.
        let out = plan(SRC, &BlockEdit::Move { at: sec(0), child: 3, before: Some(1) }).unwrap();
        let (_, b) = blocks(&out, &sec(0));
        assert_eq!(b.iter().map(|b| b.tag.as_str()).collect::<Vec<_>>(), ["p", "p", "speakAloud", "sidebarLink"]);
        assert_eq!(b[0].text, "Two lines,\n  indented more.");
        // To the end.
        let out = plan(SRC, &BlockEdit::Move { at: sec(0), child: 1, before: None }).unwrap();
        let (_, b) = blocks(&out, &sec(0));
        assert_eq!(b.last().unwrap().text, "One <b>bold</b> line.");
    }

    #[test]
    fn header_edits_and_sidebar_rename() {
        let h = Header { key: "s".into(), extra: String::new(), title: "Village".into(), subtitle: "By the river".into() };
        let out = plan(SRC, &BlockEdit::Header { at: sec(0), header: h }).unwrap();
        assert!(out.contains("<section name=\"s\">\n            <title>Village</title>\n            <subtitle>By the river</subtitle>\n            <p>One"), "{out}");

        let side = Container::Sidebar { chapter: "c".into(), id: "sb".into() };
        let h = Header { key: "aside".into(), extra: "boxed".into(), title: "Note".into(), subtitle: "Small".into() };
        let out = plan(SRC, &BlockEdit::Header { at: side, header: h }).unwrap();
        assert!(out.contains("<sidebar id=\"aside\" style=\"boxed\">\n            <title>Note</title>\n            <subtitle>Small</subtitle>\n            <p>Aside.</p>"), "{out}");
        assert!(out.contains("<sidebarLink id=\"aside\"/>"));

        let (h0, _) = blocks(SRC, &sec(0));
        let cleared = Header { title: String::new(), ..h0 };
        let out = plan(SRC, &BlockEdit::Header { at: sec(0), header: cleared }).unwrap();
        assert!(!out.contains("<title>Town</title>"));
    }
}
