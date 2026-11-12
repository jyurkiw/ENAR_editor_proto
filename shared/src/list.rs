//! Lists: a `<list>` in a section or sidebar, and its `<li>` items.
//!
//! A list has no id, so it's found by its container and its index among the container's
//! children (the same index the section/sidebar editor uses). Each item is raw XML text,
//! like a paragraph. A list needs at least one item: deleting the last removes the list.

use serde::{Deserialize, Serialize};

use crate::block::{self, Container};
use crate::edit::{self, Anchor};
use crate::xml::parse;
use crate::StructNode;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ListAt {
    pub at: Container,
    /// Index among the container's children (title and subtitle included).
    pub child: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ListView {
    /// "bullet" (the default) or "number".
    pub bullet: String,
    /// @style, as written (the compiler doesn't use it).
    pub style: String,
    /// Each `<li>`'s contents as XML, dedented.
    pub items: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum ListEdit {
    /// Set @bullet ("bullet" removes it: the default) and @style (empty removes it).
    Attrs { at: ListAt, bullet: String, style: String },
    /// Replace an item's contents (raw XML).
    SetItem { at: ListAt, n: usize, text: String },
    /// A new empty item after item `after`, or first when None.
    AddItem { at: ListAt, after: Option<usize> },
    /// Delete an item; the last one takes the list with it.
    DeleteItem { at: ListAt, n: usize },
    /// Move item `n` before item `before`, or to the end.
    MoveItem { at: ListAt, n: usize, before: Option<usize> },
    /// Delete the whole list.
    Delete { at: ListAt },
}

impl ListEdit {
    pub fn at(&self) -> &ListAt {
        match self {
            ListEdit::Attrs { at, .. }
            | ListEdit::SetItem { at, .. }
            | ListEdit::AddItem { at, .. }
            | ListEdit::DeleteItem { at, .. }
            | ListEdit::MoveItem { at, .. }
            | ListEdit::Delete { at } => at,
        }
    }

    /// Does this edit remove the list (given how many items it has)?
    pub fn removes_list(&self, items: usize) -> bool {
        matches!(self, ListEdit::Delete { .. }) || (matches!(self, ListEdit::DeleteItem { .. }) && items <= 1)
    }
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

/// The list's node.
pub fn find(source: &str, nodes: &[StructNode], at: &ListAt) -> Option<usize> {
    let ci = block::find(source, nodes, &at.at)?;
    let li = *nodes[ci].children.get(at.child)?;
    (nodes[li].tag == "list").then_some(li)
}

/// Where a list node is, if it's directly in a section or sidebar.
pub fn list_at(source: &str, nodes: &[StructNode], parents: &[Option<usize>], idx: usize) -> Option<ListAt> {
    if nodes.get(idx)?.tag != "list" {
        return None;
    }
    let ci = parents.get(idx).copied().flatten()?;
    let at = block::container_of(source, nodes, parents, ci)?;
    let child = nodes[ci].children.iter().position(|c| *c == idx)?;
    Some(ListAt { at, child })
}

fn items<'a>(nodes: &'a [StructNode], list: &'a StructNode) -> Vec<&'a StructNode> {
    list.children.iter().map(|c| &nodes[*c]).filter(|c| c.tag == "li").collect()
}

pub fn read(source: &str, nodes: &[StructNode], li: usize) -> ListView {
    let n = &nodes[li];
    ListView {
        bullet: if attr(source, n, "bullet").as_deref() == Some("number") { "number".into() } else { "bullet".into() },
        style: attr(source, n, "style").unwrap_or_default(),
        items: items(nodes, n).into_iter().map(|i| block::p_text(source, i)).collect(),
    }
}

/// A new one-item list, as `insert_child` takes it.
pub fn new_list_xml(source: &str) -> String {
    format!("<list>\n{}<li></li>\n</list>", edit::indent_unit(source))
}

pub fn plan(source: &str, e: &ListEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let li = find(source, &nodes, e.at()).ok_or("can't find that list any more; reopen the file")?;
    let list = &nodes[li];
    let its = items(&nodes, list);
    let item = |n: usize| its.get(n).copied().ok_or_else(|| "that item isn't there any more".to_string());
    let splices = match e {
        ListEdit::Attrs { bullet, style, .. } => {
            let old = read(source, &nodes, li);
            let mut out = Vec::new();
            if *bullet != old.bullet {
                let v = (bullet == "number").then_some("number");
                out.extend(edit::set_attr(source, list, "bullet", v));
            }
            if *style != old.style {
                out.extend(edit::set_attr(source, list, "style", (!style.trim().is_empty()).then_some(style.as_str())));
            }
            out
        }
        ListEdit::SetItem { n, text, .. } => vec![block::set_inner(source, item(*n)?, text).ok_or("bad <li>")?],
        ListEdit::AddItem { after, .. } => {
            let anchor = match after {
                Some(a) => Anchor::After(item(*a)?),
                None => Anchor::Before(item(0)?),
            };
            vec![edit::insert_child(source, anchor, "<li></li>").ok_or("can't place the item")?]
        }
        ListEdit::DeleteItem { n, .. } if its.len() <= 1 => {
            item(*n)?;
            vec![edit::remove_element(source, list)]
        }
        ListEdit::DeleteItem { n, .. } => vec![edit::remove_element(source, item(*n)?)],
        ListEdit::MoveItem { n, before, .. } => {
            let node = item(*n)?;
            if *before == Some(*n) || *before == Some(n + 1) || (before.is_none() && n + 1 == its.len()) {
                return Ok(source.to_string());
            }
            let target = match before {
                Some(b) => Some(item(*b)?),
                None => None,
            };
            edit::move_element(source, node, target, its.last().copied())?
        }
        ListEdit::Delete { .. } => vec![edit::remove_element(source, list)],
    };
    edit::apply(source, &splices)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement>
    <chapter name="c">
        <section name="s">
            <title>T</title>
            <p>Intro.</p>
            <list>
                <li>One <b>bold</b>.</li>
                <li>Two.</li>
                <li>Three.</li>
            </list>
        </section>
    </chapter>
</supplement>
"#;

    fn at() -> ListAt {
        ListAt { at: Container::Section { chapter: "c".into(), name: "s".into(), nth: 0 }, child: 2 }
    }

    fn view(src: &str) -> Option<ListView> {
        let nodes = parse(src).unwrap().nodes;
        find(src, &nodes, &at()).map(|i| read(src, &nodes, i))
    }

    #[test]
    fn reads_and_locates() {
        let v = view(SRC).unwrap();
        assert_eq!(v.bullet, "bullet");
        assert_eq!(v.items, ["One <b>bold</b>.", "Two.", "Three."]);
        let p = parse(SRC).unwrap();
        let mut parents = vec![None; p.nodes.len()];
        for (i, n) in p.nodes.iter().enumerate() {
            for c in &n.children {
                parents[*c] = Some(i);
            }
        }
        let idx = p.nodes.iter().position(|n| n.tag == "list").unwrap();
        assert_eq!(list_at(SRC, &p.nodes, &parents, idx), Some(at()));
    }

    #[test]
    fn edits_items() {
        let out = plan(SRC, &ListEdit::SetItem { at: at(), n: 1, text: "Deux.".into() }).unwrap();
        assert_eq!(view(&out).unwrap().items[1], "Deux.");
        let out = plan(SRC, &ListEdit::AddItem { at: at(), after: Some(0) }).unwrap();
        assert!(out.contains("<li>One <b>bold</b>.</li>\n                <li></li>\n                <li>Two.</li>"), "{out}");
        let out = plan(SRC, &ListEdit::AddItem { at: at(), after: None }).unwrap();
        assert_eq!(view(&out).unwrap().items[0], "");
        let out = plan(SRC, &ListEdit::DeleteItem { at: at(), n: 0 }).unwrap();
        assert_eq!(view(&out).unwrap().items, ["Two.", "Three."]);
        let out = plan(SRC, &ListEdit::MoveItem { at: at(), n: 2, before: Some(0) }).unwrap();
        assert_eq!(view(&out).unwrap().items, ["Three.", "One <b>bold</b>.", "Two."]);
        let out = plan(SRC, &ListEdit::MoveItem { at: at(), n: 0, before: None }).unwrap();
        assert_eq!(view(&out).unwrap().items, ["Two.", "Three.", "One <b>bold</b>."]);
        assert_eq!(plan(SRC, &ListEdit::MoveItem { at: at(), n: 0, before: Some(1) }).unwrap(), SRC);
    }

    #[test]
    fn last_item_takes_the_list() {
        let mut s = SRC.to_string();
        for _ in 0..2 {
            s = plan(&s, &ListEdit::DeleteItem { at: at(), n: 0 }).unwrap();
        }
        assert_eq!(view(&s).unwrap().items, ["Three."]);
        let s = plan(&s, &ListEdit::DeleteItem { at: at(), n: 0 }).unwrap();
        assert!(!s.contains("<list"), "{s}");
        assert!(s.contains("<p>Intro.</p>\n        </section>"), "{s}");
    }

    #[test]
    fn attrs() {
        let out = plan(SRC, &ListEdit::Attrs { at: at(), bullet: "number".into(), style: String::new() }).unwrap();
        assert!(out.contains(r#"<list bullet="number">"#));
        let back = plan(&out, &ListEdit::Attrs { at: at(), bullet: "bullet".into(), style: "tight".into() }).unwrap();
        assert!(back.contains(r#"<list style="tight">"#), "{back}");
    }

    #[test]
    fn new_list_goes_in_a_section() {
        let e = block::BlockEdit::AddList { at: at().at, after: Some(1) };
        let out = block::plan(SRC, &e).unwrap();
        assert!(out.contains("<p>Intro.</p>\n            <list>\n                <li></li>\n            </list>\n            <list>"), "{out}");
    }
}
