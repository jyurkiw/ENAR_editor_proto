//! Boxed text in a section: `<speakAloud>` (read aloud to the players) and `<dialog>`
//! (an NPC's lines, with an optional @speaker). Both are an optional `<trigger>` (when to
//! read the box) followed by `<p>`s.
//!
//! A box has no id, so it's found by its section and its index among the section's
//! children, like a list. Paragraphs are numbered among the box's `<p>`s only.

use serde::{Deserialize, Serialize};

use crate::block::{self, Container};
use crate::edit::{self, Anchor};
use crate::xml::parse;
use crate::StructNode;

pub const TAGS: &[&str] = &["speakAloud", "dialog"];

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BoxAt {
    pub at: Container,
    /// Index among the container's children (title and subtitle included).
    pub child: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BoxView {
    /// "speakAloud" or "dialog".
    pub tag: String,
    /// @speaker (dialog only), as written.
    pub speaker: String,
    pub has_trigger: bool,
    /// The `<trigger>`'s contents as XML.
    pub trigger: String,
    /// Each `<p>`'s contents as XML, dedented.
    pub paras: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum BoxEdit {
    /// Set @speaker (dialog); empty removes it.
    Speaker { at: BoxAt, speaker: String },
    /// Set the `<trigger>`'s contents, adding it first in the box; empty removes it.
    SetTrigger { at: BoxAt, text: String },
    SetP { at: BoxAt, n: usize, text: String },
    /// A new empty paragraph after paragraph `after`, or first (after the trigger).
    AddP { at: BoxAt, after: Option<usize> },
    /// Delete a paragraph; `with_box` deletes the whole box instead (its last paragraph).
    DeleteP { at: BoxAt, n: usize, with_box: bool },
    MoveP { at: BoxAt, n: usize, before: Option<usize> },
    Delete { at: BoxAt },
}

impl BoxEdit {
    pub fn at(&self) -> &BoxAt {
        match self {
            BoxEdit::Speaker { at, .. }
            | BoxEdit::SetTrigger { at, .. }
            | BoxEdit::SetP { at, .. }
            | BoxEdit::AddP { at, .. }
            | BoxEdit::DeleteP { at, .. }
            | BoxEdit::MoveP { at, .. }
            | BoxEdit::Delete { at } => at,
        }
    }
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

pub fn find(source: &str, nodes: &[StructNode], at: &BoxAt) -> Option<usize> {
    let ci = block::find(source, nodes, &at.at)?;
    let b = *nodes[ci].children.get(at.child)?;
    TAGS.contains(&nodes[b].tag.as_str()).then_some(b)
}

/// Where a box node is, if it's directly in a section.
pub fn box_at(source: &str, nodes: &[StructNode], parents: &[Option<usize>], idx: usize) -> Option<BoxAt> {
    if !TAGS.contains(&nodes.get(idx)?.tag.as_str()) {
        return None;
    }
    let ci = parents.get(idx).copied().flatten()?;
    let at = block::container_of(source, nodes, parents, ci)?;
    let child = nodes[ci].children.iter().position(|c| *c == idx)?;
    Some(BoxAt { at, child })
}

fn kids<'a>(nodes: &'a [StructNode], b: &'a StructNode, tag: &'a str) -> Vec<&'a StructNode> {
    b.children.iter().map(|c| &nodes[*c]).filter(|c| c.tag == tag).collect()
}

pub fn read(source: &str, nodes: &[StructNode], bi: usize) -> BoxView {
    let b = &nodes[bi];
    let trigger = kids(nodes, b, "trigger").first().copied();
    BoxView {
        tag: b.tag.clone(),
        speaker: attr(source, b, "speaker").unwrap_or_default(),
        has_trigger: trigger.is_some(),
        trigger: trigger.map(|t| block::p_text(source, t)).unwrap_or_default(),
        paras: kids(nodes, b, "p").into_iter().map(|p| block::p_text(source, p)).collect(),
    }
}

/// A new box with one empty paragraph, as `insert_child` takes it.
pub fn new_box_xml(source: &str, tag: &str) -> String {
    format!("<{tag}>\n{}<p></p>\n</{tag}>", edit::indent_unit(source))
}

pub fn plan(source: &str, e: &BoxEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let bi = find(source, &nodes, e.at()).ok_or("can't find that box any more; reopen the file")?;
    let b = &nodes[bi];
    let ps = kids(&nodes, b, "p");
    let trigger = kids(&nodes, b, "trigger").first().copied();
    let para = |n: usize| ps.get(n).copied().ok_or_else(|| "that paragraph isn't there any more".to_string());
    let splices = match e {
        BoxEdit::Speaker { speaker, .. } => {
            if b.tag != "dialog" {
                return Err("only a <dialog> has a speaker".into());
            }
            edit::set_attr(source, b, "speaker", (!speaker.trim().is_empty()).then_some(speaker.as_str())).into_iter().collect()
        }
        BoxEdit::SetTrigger { text, .. } => match (trigger, text.trim().is_empty()) {
            (Some(t), true) => vec![edit::remove_element(source, t)],
            (Some(t), false) => vec![block::set_inner(source, t, text).ok_or("bad <trigger>")?],
            (None, true) => vec![],
            (None, false) => {
                let anchor = match b.children.first() {
                    Some(f) => Anchor::Before(&nodes[*f]),
                    None => Anchor::Into(b),
                };
                let xml = format!("<trigger>{}</trigger>", text.trim());
                vec![edit::insert_child(source, anchor, &xml).ok_or("can't place the trigger")?]
            }
        },
        BoxEdit::SetP { n, text, .. } => vec![block::set_inner(source, para(*n)?, text).ok_or("bad <p>")?],
        BoxEdit::AddP { after, .. } => {
            let anchor = match (after, ps.first(), trigger) {
                (Some(a), _, _) => Anchor::After(para(*a)?),
                (None, Some(first), _) => Anchor::Before(first),
                (None, None, Some(t)) => Anchor::After(t),
                (None, None, None) => Anchor::Into(b),
            };
            vec![edit::insert_child(source, anchor, "<p></p>").ok_or("can't place the paragraph")?]
        }
        BoxEdit::DeleteP { n, with_box, .. } => {
            let p = para(*n)?;
            if *with_box {
                vec![edit::remove_element(source, b)]
            } else {
                vec![edit::remove_element(source, p)]
            }
        }
        BoxEdit::MoveP { n, before, .. } => {
            let p = para(*n)?;
            if *before == Some(*n) || *before == Some(n + 1) || (before.is_none() && n + 1 == ps.len()) {
                return Ok(source.to_string());
            }
            let target = match before {
                Some(t) => Some(para(*t)?),
                None => None,
            };
            edit::move_element(source, p, target, ps.last().copied())?
        }
        BoxEdit::Delete { .. } => vec![edit::remove_element(source, b)],
    };
    edit::apply(source, &splices)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement>
    <chapter name="c">
        <section name="s">
            <p>Intro.</p>
            <speakAloud>
                <p>Read one.</p>
                <p>Read two.</p>
            </speakAloud>
            <dialog speaker="Garrick">
                <trigger>When asked</trigger>
                <p>Line one.</p>
            </dialog>
        </section>
    </chapter>
</supplement>
"#;

    fn sec() -> Container {
        Container::Section { chapter: "c".into(), name: "s".into(), nth: 0 }
    }
    fn sa() -> BoxAt {
        BoxAt { at: sec(), child: 1 }
    }
    fn dl() -> BoxAt {
        BoxAt { at: sec(), child: 2 }
    }
    fn view(src: &str, at: &BoxAt) -> Option<BoxView> {
        let nodes = parse(src).unwrap().nodes;
        find(src, &nodes, at).map(|i| read(src, &nodes, i))
    }

    #[test]
    fn reads_boxes() {
        let v = view(SRC, &sa()).unwrap();
        assert_eq!((v.tag.as_str(), v.has_trigger, v.paras.len()), ("speakAloud", false, 2));
        let d = view(SRC, &dl()).unwrap();
        assert_eq!((d.speaker.as_str(), d.trigger.as_str()), ("Garrick", "When asked"));
        assert_eq!(d.paras, ["Line one."]);
    }

    #[test]
    fn triggers() {
        let out = plan(SRC, &BoxEdit::SetTrigger { at: sa(), text: "When the PCs <b>enter</b>".into() }).unwrap();
        assert!(out.contains("<speakAloud>\n                <trigger>When the PCs <b>enter</b></trigger>\n                <p>Read one."), "{out}");
        let out = plan(SRC, &BoxEdit::SetTrigger { at: dl(), text: String::new() }).unwrap();
        assert!(!out.contains("<trigger>"));
        let out = plan(SRC, &BoxEdit::SetTrigger { at: dl(), text: "Later".into() }).unwrap();
        assert!(out.contains("<trigger>Later</trigger>"));
    }

    #[test]
    fn paragraphs() {
        let out = plan(SRC, &BoxEdit::AddP { at: dl(), after: None }).unwrap();
        assert!(out.contains("<trigger>When asked</trigger>\n                <p></p>\n                <p>Line one."), "{out}");
        let out = plan(SRC, &BoxEdit::MoveP { at: sa(), n: 1, before: Some(0) }).unwrap();
        assert_eq!(view(&out, &sa()).unwrap().paras, ["Read two.", "Read one."]);
        let out = plan(SRC, &BoxEdit::SetP { at: sa(), n: 0, text: "New.".into() }).unwrap();
        assert_eq!(view(&out, &sa()).unwrap().paras[0], "New.");
        let out = plan(SRC, &BoxEdit::DeleteP { at: dl(), n: 0, with_box: false }).unwrap();
        assert!(view(&out, &dl()).unwrap().paras.is_empty(), "the box can stay empty");
        let out = plan(SRC, &BoxEdit::DeleteP { at: dl(), n: 0, with_box: true }).unwrap();
        assert!(!out.contains("<dialog"));
    }

    #[test]
    fn speaker_and_new_boxes() {
        let out = plan(SRC, &BoxEdit::Speaker { at: dl(), speaker: String::new() }).unwrap();
        assert!(out.contains("<dialog>"));
        assert!(plan(SRC, &BoxEdit::Speaker { at: sa(), speaker: "X".into() }).is_err());
        let out = block::plan(SRC, &block::BlockEdit::AddBox { at: sec(), after: Some(0), tag: "dialog".into() }).unwrap();
        assert!(out.contains("<p>Intro.</p>\n            <dialog>\n                <p></p>\n            </dialog>\n            <speakAloud>"), "{out}");
    }
}
