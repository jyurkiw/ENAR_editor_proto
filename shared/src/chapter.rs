//! The `<chapter>` editor: its attributes and title, and its children (sections,
//! sidebars, tables, page ends) as summaries that can be reordered, deleted and added.
//!
//! Each child is moved as a "chunk": its lines plus the blank lines and comments
//! between it and the previous child, so a comment introducing a section moves with it.
//! A renamed chapter's references (chapterOrder, chapterRef, data/chapter) are renamed
//! in the same edit.

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::edit::{self, Anchor, Splice};
use crate::xml::parse;
use crate::StructNode;

/// How many characters a paragraph summary keeps.
const SUMMARY_CHARS: usize = 42;

/// Children a chapter can hold, besides its title.
pub const CHILD_TAGS: &[&str] = &["section", "sidebar", "table", "pageEnd"];

/// `@kind` values; "chapter" is the default.
pub const KINDS: &[(&str, &str)] = &[
    ("chapter", "Numbered 1, 2, 3…"),
    ("appendix", "Lettered A, B, C…, after the numbered chapters"),
    ("unnumbered", "No number"),
    ("license", "A hand-written legal page"),
];

/// One child, as the form lists it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChildItem {
    pub tag: String,
    /// Its position among the chapter's children in the file; None for one being added.
    pub orig: Option<usize>,
    /// The section's @name or the sidebar's/table's @id.
    pub key: String,
    /// Its title, or the start of its first paragraph.
    pub summary: String,
    /// True when the summary isn't a title (a table without one, say).
    pub untitled: bool,
    /// For a child being added: its title.
    pub new_title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChapterModel {
    pub name: String,
    pub kind: String,
    pub columns: String,
    pub credit: String,
    pub title: String,
    pub title_img: String,
    /// The chapterTitle holds markup, so the form shows it read-only.
    pub title_markup: bool,
    pub children: Vec<ChildItem>,
}

/// A change to the supplement's chapters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum ChapterEdit {
    /// Change the chapter currently named `name`.
    Update { name: String, model: ChapterModel },
    /// Add a chapter after the last one, and its `<ch>` to chapterOrder.
    Add { name: String, title: String, kind: String },
    /// Remove a chapter and its `<ch>`.
    Delete { name: String },
    /// Move a chapter before the chapter named `before`, or after the last one, and its
    /// `<ch>` the same way in chapterOrder.
    Move { name: String, before: Option<String> },
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

/// The root's `<chapter>` nodes.
pub fn chapter_nodes(nodes: &[StructNode]) -> Vec<usize> {
    nodes.first().map(|r| r.children.iter().copied().filter(|c| nodes[*c].tag == "chapter").collect()).unwrap_or_default()
}

pub fn find_chapter(source: &str, nodes: &[StructNode], name: &str) -> Option<usize> {
    chapter_nodes(nodes).into_iter().find(|c| attr(source, &nodes[*c], "name").as_deref() == Some(name))
}

/// The chapter's children other than its title, in file order.
fn children(nodes: &[StructNode], chapter: &StructNode) -> Vec<usize> {
    chapter.children.iter().copied().filter(|c| nodes[*c].tag != "chapterTitle").collect()
}

fn first_descendant<'a>(nodes: &'a [StructNode], n: &StructNode, tag: &str) -> Option<&'a StructNode> {
    for c in &n.children {
        if nodes[*c].tag == tag {
            return Some(&nodes[*c]);
        }
        if let Some(d) = first_descendant(nodes, &nodes[*c], tag) {
            return Some(d);
        }
    }
    None
}

/// (key, summary, untitled) for a child.
pub fn summarize(source: &str, nodes: &[StructNode], n: &StructNode) -> (String, String, bool) {
    let title = n
        .children
        .iter()
        .map(|c| &nodes[*c])
        .find(|c| c.tag == "title")
        .map(|t| edit::plain_text(source, t))
        .filter(|t| !t.is_empty());
    let first_p = || {
        first_descendant(nodes, n, "p").map(|p| edit::plain_text(source, p)).filter(|t| !t.is_empty())
    };
    match n.tag.as_str() {
        "section" => {
            let key = attr(source, n, "name").unwrap_or_default();
            match title {
                Some(t) => (key, t, false),
                None => (key, first_p().map(|p| edit::snippet(&p, SUMMARY_CHARS)).unwrap_or_else(|| "(empty)".into()), true),
            }
        }
        "sidebar" => {
            let key = attr(source, n, "id").unwrap_or_default();
            match title {
                Some(t) => (key, t, false),
                None => (key, first_p().map(|p| edit::snippet(&p, SUMMARY_CHARS)).unwrap_or_else(|| "(empty)".into()), true),
            }
        }
        "table" => {
            let key = attr(source, n, "id").unwrap_or_default();
            match title {
                Some(t) => (key, t, false),
                None => (key.clone(), format!("Table {key} (no title)"), true),
            }
        }
        "pageEnd" => {
            let balance = attr(source, n, "balance");
            let text = if matches!(balance.as_deref(), Some("false") | Some("0")) {
                "Page end (columns not balanced)"
            } else {
                "Page end"
            };
            (String::new(), text.into(), false)
        }
        other => (String::new(), format!("<{other}>"), true),
    }
}

pub fn read(source: &str, nodes: &[StructNode], chapter: usize) -> ChapterModel {
    let ch = &nodes[chapter];
    let title = ch.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == "chapterTitle");
    let markup = title.is_some_and(|t| {
        edit::inner_span(source, t).is_some_and(|(a, b)| source[a..b].contains('<'))
    });
    ChapterModel {
        name: attr(source, ch, "name").unwrap_or_default(),
        kind: attr(source, ch, "kind").unwrap_or_default(),
        columns: attr(source, ch, "columns").unwrap_or_default(),
        credit: attr(source, ch, "credit").unwrap_or_default(),
        title: title
            .map(|t| if markup { edit::plain_text(source, t) } else { edit::text_of(source, t) })
            .unwrap_or_default(),
        title_img: title.and_then(|t| attr(source, t, "imgSrc")).unwrap_or_default(),
        title_markup: markup,
        children: children(nodes, ch)
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let (key, summary, untitled) = summarize(source, nodes, &nodes[c]);
                ChildItem { tag: nodes[c].tag.clone(), orig: Some(i), key, summary, untitled, new_title: String::new() }
            })
            .collect(),
    }
}

/// Start-tag offsets of every element that names chapter `name`: `<chapterRef>`
/// anywhere, `<chapterOrder><ch>` and `<data><chapter>`.
pub fn name_refs(source: &str, name: &str) -> Vec<usize> {
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut out = Vec::new();
    loop {
        let pos = reader.buffer_position() as usize;
        let ev = match reader.read_event() {
            Ok(ev) => ev,
            Err(_) => break,
        };
        match &ev {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.name().as_ref().to_vec();
                let parent = stack.last().map(|p| p.as_slice());
                let wanted = tag == b"chapterRef"
                    || (tag == b"ch" && parent == Some(b"chapterOrder"))
                    || (tag == b"chapter" && parent == Some(b"data"));
                if wanted {
                    let n = e.attributes().flatten().find(|a| a.key.as_ref() == b"name").and_then(|a| a.unescape_value().ok());
                    if n.as_deref() == Some(name) {
                        out.push(pos);
                    }
                }
                if matches!(ev, Event::Start(_)) {
                    stack.push(tag);
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    out
}

fn some(s: &str) -> Option<&str> {
    (!s.trim().is_empty()).then_some(s)
}

/// A new child's XML, one level of indent per nesting level.
fn stub(item: &ChildItem, unit: &str) -> String {
    let title = some(&item.new_title).map(|t| format!("\n{unit}<title>{}</title>", edit::escape_text(t))).unwrap_or_default();
    let key = edit::escape_attr(&item.key, '"');
    match item.tag.as_str() {
        "section" => format!("<section name=\"{key}\">{title}\n{unit}<p></p>\n</section>"),
        "sidebar" => format!("<sidebar id=\"{key}\">{title}\n{unit}<p></p>\n</sidebar>"),
        "table" => format!(
            "<table id=\"{key}\">{title}\n{unit}<tableBody>\n{unit}{unit}<tr><td></td></tr>\n{unit}</tableBody>\n</table>"
        ),
        _ => "<pageEnd/>".into(),
    }
}

fn indent_lines(xml: &str, indent: &str, nl: &str) -> String {
    xml.lines().map(|l| format!("{indent}{l}")).collect::<Vec<_>>().join(nl)
}

/// Splices that turn the chapter into `m`, except for renaming references elsewhere.
fn update_splices(source: &str, nodes: &[StructNode], ci: usize, m: &ChapterModel) -> Result<Vec<Splice>, String> {
    let old = read(source, nodes, ci);
    let ch = &nodes[ci];
    let mut out = Vec::new();

    // Attributes. Defaults (kind="chapter", columns="2") stay as written if unchanged.
    for (name, new, old_v) in [
        ("name", &m.name, &old.name),
        ("kind", &m.kind, &old.kind),
        ("columns", &m.columns, &old.columns),
        ("credit", &m.credit, &old.credit),
    ] {
        if new != old_v {
            if name == "name" && new.trim().is_empty() {
                return Err("a chapter needs a name".into());
            }
            out.extend(edit::set_attr(source, ch, name, some(new)));
        }
    }

    // Title.
    let title = ch.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == "chapterTitle");
    match title {
        Some(t) => {
            if !old.title_markup && m.title != old.title {
                out.extend(edit::set_text(source, t, &m.title));
            }
            if m.title_img != old.title_img {
                out.extend(edit::set_attr(source, t, "imgSrc", some(&m.title_img)));
            }
        }
        None => {
            let img = some(&m.title_img).map(|i| format!(" imgSrc=\"{}\"", edit::escape_attr(i, '"'))).unwrap_or_default();
            let xml = format!("<chapterTitle{img}>{}</chapterTitle>", edit::escape_text(&m.title));
            let anchor = match ch.children.first() {
                Some(f) => Anchor::Before(&nodes[*f]),
                None => Anchor::Into(ch),
            };
            out.push(edit::insert_child(source, anchor, &xml).ok_or("can't place the chapter title")?);
        }
    }

    // Children: unchanged order and nothing new means no edit.
    let kids = children(nodes, ch);
    let identity = m.children.len() == kids.len() && m.children.iter().enumerate().all(|(i, c)| c.orig == Some(i));
    if identity {
        return Ok(out);
    }
    for c in &m.children {
        if c.orig.is_some_and(|o| o >= kids.len()) {
            return Err("the chapter's contents changed; revert and try again".into());
        }
        if c.orig.is_none() && c.tag != "pageEnd" && c.key.trim().is_empty() {
            return Err(format!("a new <{}> needs {}", c.tag, if c.tag == "section" { "a name" } else { "an id" }));
        }
    }
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let unit = edit::indent_unit(source);
    let head_end = match title {
        Some(t) => edit::line_end_after(source, t.end).unwrap_or(t.end),
        None => edit::start_tag(source, ch.start).ok_or("bad chapter tag")?.end,
    };
    // Chunk i: from the end of the previous child's line to the end of its own.
    let mut bounds = Vec::new();
    let mut prev = head_end;
    for k in &kids {
        let n = &nodes[*k];
        let end = edit::line_end_after(source, n.end).unwrap_or(n.end);
        bounds.push((prev, end));
        prev = end;
    }
    let indent = match kids.first() {
        Some(k) => edit::line_indent(source, nodes[*k].start).to_string(),
        None => match title {
            Some(t) => edit::line_indent(source, t.start).to_string(),
            None => format!("{}{unit}", edit::line_indent(source, ch.start)),
        },
    };
    let mut text = String::new();
    for c in &m.children {
        match c.orig {
            Some(o) => {
                let (a, b) = bounds[o];
                text.push_str(&source[a..b]);
            }
            None => {
                text.push_str(nl);
                text.push_str(&indent_lines(&stub(c, unit), &indent, nl));
                text.push_str(nl);
            }
        }
    }
    let (start, end) = match (bounds.first(), bounds.last()) {
        (Some(f), Some(l)) => (f.0, l.1),
        _ => (head_end, head_end),
    };
    out.push(Splice { start, end, text });
    Ok(out)
}

/// Renames every reference to chapter `old`.
fn rename_refs(source: &str, old: &str, new: &str) -> Result<String, String> {
    let splices: Vec<Splice> = name_refs(source, old).into_iter().filter_map(|p| edit::set_attr_at(source, p, "name", Some(new))).collect();
    edit::apply(source, &splices)
}

pub fn plan(source: &str, change: &ChapterEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    match change {
        ChapterEdit::Update { name, model } => {
            let ci = find_chapter(source, &nodes, name).ok_or_else(|| format!("there's no chapter named \"{name}\""))?;
            let text = edit::apply(source, &update_splices(source, &nodes, ci, model)?)?;
            if model.name != *name {
                rename_refs(&text, name, &model.name)
            } else {
                Ok(text)
            }
        }
        ChapterEdit::Add { name, title, kind } => {
            if name.trim().is_empty() {
                return Err("a chapter needs a name".into());
            }
            if find_chapter(source, &nodes, name).is_some() {
                return Err(format!("there's already a chapter named \"{name}\""));
            }
            let root = &nodes[0];
            let kind_attr = some(kind)
                .filter(|k| *k != "chapter")
                .map(|k| format!(" kind=\"{}\"", edit::escape_attr(k, '"')))
                .unwrap_or_default();
            let unit = edit::indent_unit(source);
            let xml = format!(
                "<chapter name=\"{}\"{kind_attr}>\n{unit}<chapterTitle>{}</chapterTitle>\n</chapter>",
                edit::escape_attr(name, '"'),
                edit::escape_text(title)
            );
            let last = root.children.iter().copied().filter(|c| matches!(nodes[*c].tag.as_str(), "chapter" | "cover" | "contents" | "productDescription")).last();
            let mut s = match last {
                Some(l) => edit::insert_child(source, Anchor::After(&nodes[l]), &xml),
                None => match root.children.first() {
                    Some(f) => edit::insert_child(source, Anchor::Before(&nodes[*f]), &xml),
                    None => edit::insert_child(source, Anchor::Into(root), &xml),
                },
            }
            .ok_or("can't place the chapter")?;
            if last.is_some() {
                s.text.insert(0, '\n');
            }
            let order = root.children.iter().map(|c| &nodes[*c]).find(|n| n.tag == "chapterOrder").ok_or("the supplement has no <chapterOrder>")?;
            let ch_xml = format!("<ch name=\"{}\"/>", edit::escape_attr(name, '"'));
            let last_ch = order.children.iter().map(|c| &nodes[*c]).filter(|n| n.tag == "ch").last();
            let o = match last_ch {
                Some(c) => edit::insert_child(source, Anchor::After(c), &ch_xml),
                None => edit::insert_child(source, Anchor::Into(order), &ch_xml),
            }
            .ok_or("can't add the chapter to <chapterOrder>")?;
            edit::apply(source, &[s, o])
        }
        ChapterEdit::Move { name, before } => {
            let ci = find_chapter(source, &nodes, name).ok_or_else(|| format!("there's no chapter named \"{name}\""))?;
            let target = match before {
                Some(b) => Some(find_chapter(source, &nodes, b).ok_or_else(|| format!("there's no chapter named \"{b}\""))?),
                None => None,
            };
            if target == Some(ci) {
                return Ok(source.to_string());
            }
            let chapters: Vec<usize> = nodes[0].children.iter().copied().filter(|c| nodes[*c].tag == "chapter").collect();
            let last = *chapters.last().ok_or("no chapters")?;
            let mut splices = edit::move_block(source, &nodes[ci], target.map(|t| &nodes[t]), &nodes[last])?;
            // Its <ch> follows it in chapterOrder, before the target's <ch> (or last).
            let order = nodes[0].children.iter().map(|c| &nodes[*c]).find(|n| n.tag == "chapterOrder");
            if let Some(order) = order {
                let chs: Vec<&StructNode> = order.children.iter().map(|c| &nodes[*c]).filter(|c| c.tag == "ch").collect();
                let ch = |n: &str| chs.iter().copied().find(|c| attr(source, c, "name").as_deref() == Some(n));
                if let Some(me) = ch(name) {
                    let to = before.as_deref().and_then(ch);
                    if to.map(|t| t.start) != Some(me.start) {
                        splices.extend(edit::move_element(source, me, to, chs.last().copied())?);
                    }
                }
            }
            edit::apply(source, &splices)
        }
        ChapterEdit::Delete { name } => {
            let ci = find_chapter(source, &nodes, name).ok_or_else(|| format!("there's no chapter named \"{name}\""))?;
            let mut splices = vec![edit::remove_element(source, &nodes[ci])];
            let order = nodes[0].children.iter().map(|c| &nodes[*c]).find(|n| n.tag == "chapterOrder");
            if let Some(order) = order {
                for c in order.children.iter().map(|c| &nodes[*c]) {
                    if c.tag == "ch" && attr(source, c, "name").as_deref() == Some(name) {
                        splices.push(edit::remove_element(source, c));
                    }
                }
            }
            edit::apply(source, &splices)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement type="adventure">
    <chapter name="intro" credit="A">
        <chapterTitle>Introduction</chapterTitle>

        <section name="s1">
            <title>First</title>
            <p>One <chapterRef name="intro"/>.</p>
        </section>

        <!-- about s2 -->
        <section name="s2">
            <p>A section with no title but a <b>long</b> opening paragraph that keeps going.</p>
        </section>
        <table id="t1">
            <tableBody><tr><td>x</td></tr></tableBody>
        </table>
        <pageEnd balance="false"/>
    </chapter>

    <chapter name="other">
        <chapterTitle>Other <i>Things</i></chapterTitle>
        <section name="o1"><p>See <chapterRef name="intro"/>.</p></section>
    </chapter>

    <data>
        <chapter name="intro"/>
    </data>
    <chapterOrder>
        <ch name="intro"/>
        <ch name="other"/>
    </chapterOrder>
</supplement>
"#;

    fn model(src: &str, name: &str) -> ChapterModel {
        let nodes = parse(src).unwrap().nodes;
        read(src, &nodes, find_chapter(src, &nodes, name).unwrap())
    }

    #[test]
    fn move_a_chapter_and_its_ch() {
        let names = |src: &str| {
            let nodes = parse(src).unwrap().nodes;
            let of = |tag: &str| -> Vec<String> {
                nodes.iter().filter(|n| n.tag == tag).filter_map(|n| attr(src, n, "name")).collect()
            };
            (of("chapter"), of("ch"))
        };
        // Up: "other" before "intro"; the <ch>s follow, and the blank lines stay one each.
        let up = plan(SRC, &ChapterEdit::Move { name: "other".into(), before: Some("intro".into()) }).unwrap();
        assert_eq!(names(&up).1, vec!["other", "intro"], "{up}");
        assert!(up.starts_with("<supplement type=\"adventure\">\n    <chapter name=\"other\">"), "{up}");
        assert!(up.contains("    </chapter>\n\n    <chapter name=\"intro\""), "{up}");
        assert!(up.contains("<pageEnd balance=\"false\"/>\n    </chapter>\n\n    <data>"), "{up}");
        assert!(!up.contains("\n\n\n"), "{up}");
        // And back down, after the last: the file as it was.
        let down = plan(&up, &ChapterEdit::Move { name: "other".into(), before: None }).unwrap();
        assert_eq!(down, SRC);
        // The data block's <chapter name="intro"> isn't a chapter and stays put.
        assert_eq!(names(&down).0, vec!["intro", "other", "intro"]);
    }

    #[test]
    fn summaries() {
        let m = model(SRC, "intro");
        let s: Vec<_> = m.children.iter().map(|c| (c.tag.as_str(), c.key.as_str(), c.summary.as_str(), c.untitled)).collect();
        assert_eq!(s[0], ("section", "s1", "First", false));
        assert_eq!(s[1], ("section", "s2", "A section with no title but a long…", true));
        assert_eq!(s[2], ("table", "t1", "Table t1 (no title)", true));
        assert_eq!(s[3], ("pageEnd", "", "Page end (columns not balanced)", false));
        let o = model(SRC, "other");
        assert!(o.title_markup);
        assert_eq!(o.title, "Other Things");
    }

    #[test]
    fn unchanged_is_identity() {
        let m = model(SRC, "intro");
        assert_eq!(plan(SRC, &ChapterEdit::Update { name: "intro".into(), model: m }).unwrap(), SRC);
    }

    #[test]
    fn reorder_moves_comments_with_their_child() {
        let mut m = model(SRC, "intro");
        m.children.swap(0, 1);
        let out = plan(SRC, &ChapterEdit::Update { name: "intro".into(), model: m.clone() }).unwrap();
        let a = out.find("<!-- about s2 -->").unwrap();
        let b = out.find("<section name=\"s2\">").unwrap();
        let c = out.find("<section name=\"s1\">").unwrap();
        assert!(a < b && b < c, "{out}");
        assert_eq!(model(&out, "intro").children.iter().map(|c| c.key.clone()).collect::<Vec<_>>(), ["s2", "s1", "t1", ""]);
    }

    #[test]
    fn delete_and_add_children() {
        let mut m = model(SRC, "intro");
        m.children.remove(2); // the table
        m.children.push(ChildItem { tag: "sidebar".into(), key: "sb".into(), new_title: "Aside".into(), ..Default::default() });
        m.children.push(ChildItem { tag: "table".into(), key: "t2".into(), ..Default::default() });
        let out = plan(SRC, &ChapterEdit::Update { name: "intro".into(), model: m }).unwrap();
        assert!(!out.contains("t1"));
        assert!(out.contains(
            "        <pageEnd balance=\"false\"/>\n\n        <sidebar id=\"sb\">\n            <title>Aside</title>\n            <p></p>\n        </sidebar>\n\n        <table id=\"t2\">\n            <tableBody>\n                <tr><td></td></tr>\n            </tableBody>\n        </table>\n    </chapter>"
        ), "{out}");
    }

    #[test]
    fn rename_updates_every_reference() {
        assert_eq!(name_refs(SRC, "intro").len(), 4);
        let mut m = model(SRC, "intro");
        m.name = "opening".into();
        m.title = "Opening & Close".into();
        m.kind = "appendix".into();
        let out = plan(SRC, &ChapterEdit::Update { name: "intro".into(), model: m }).unwrap();
        assert!(out.contains(r#"<chapter name="opening" credit="A" kind="appendix">"#));
        assert!(out.contains("<chapterTitle>Opening &amp; Close</chapterTitle>"));
        assert_eq!(name_refs(&out, "intro").len(), 0);
        assert_eq!(name_refs(&out, "opening").len(), 4);
        assert!(out.contains(r#"<ch name="opening"/>"#));
    }

    #[test]
    fn markup_title_is_left_alone() {
        let mut m = model(SRC, "other");
        m.title = "changed".into();
        m.title_img = "art.png".into();
        let out = plan(SRC, &ChapterEdit::Update { name: "other".into(), model: m }).unwrap();
        assert!(out.contains(r#"<chapterTitle imgSrc="art.png">Other <i>Things</i></chapterTitle>"#));
    }

    #[test]
    fn add_and_delete_chapters() {
        let out = plan(SRC, &ChapterEdit::Add { name: "app".into(), title: "Monsters".into(), kind: "appendix".into() }).unwrap();
        assert!(out.contains("    </chapter>\n\n    <chapter name=\"app\" kind=\"appendix\">\n        <chapterTitle>Monsters</chapterTitle>\n    </chapter>\n\n    <data>"), "{out}");
        assert!(out.contains("<ch name=\"other\"/>\n        <ch name=\"app\"/>\n    </chapterOrder>"));
        assert!(plan(SRC, &ChapterEdit::Add { name: "intro".into(), title: "x".into(), kind: String::new() }).is_err());

        let out = plan(&out, &ChapterEdit::Delete { name: "other".into() }).unwrap();
        assert!(!out.contains("name=\"other\""));
        assert!(!out.contains("Things"));
    }
}
