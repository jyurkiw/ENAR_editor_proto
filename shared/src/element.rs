//! Replacing one element with new XML text: the placeholder editor for files (and
//! elements) that don't have a form yet.
//!
//! The element is found by its path of child indexes from the root in the file tree,
//! and must still have the tag the editor showed. The text is what the editor showed:
//! the element re-indented to start at column 0; it goes back at the element's own
//! indentation.

use serde::{Deserialize, Serialize};

use crate::edit::{self, Splice};
use crate::xml::parse;
use crate::StructNode;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ElementEdit {
    /// Child indexes from the root (empty: the root itself).
    pub path: Vec<usize>,
    pub tag: String,
    /// The element's new XML: one element, with the same tag.
    pub xml: String,
}

/// The node at `path`.
pub fn at_path(nodes: &[StructNode], path: &[usize]) -> Option<usize> {
    let mut cur = 0;
    for k in path {
        cur = *nodes.get(cur)?.children.get(*k)?;
    }
    nodes.get(cur).map(|_| cur)
}

/// The text with lines after the first indented by `indent` (blank lines left blank).
fn indent_rest(text: &str, indent: &str, nl: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| if i == 0 || l.trim().is_empty() { l.trim_end().to_string() } else { format!("{indent}{}", l.trim_end()) })
        .collect::<Vec<_>>()
        .join(nl)
}

pub fn plan(source: &str, e: &ElementEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let i = at_path(&nodes, &e.path).ok_or("that element isn't in the file any more")?;
    let n = &nodes[i];
    if n.tag != e.tag {
        return Err(format!("the element there is a <{}> now, not a <{}>", n.tag, e.tag));
    }
    let xml = e.xml.trim();
    let new = parse(xml).map_err(|err| format!("the new text isn't one well-formed element: {}", err.message))?;
    if new.root_tag != e.tag {
        return Err(format!("the new text is a <{}>; it must stay a <{}>", new.root_tag, e.tag));
    }
    if xml.starts_with("<?") {
        return Err("write just the element, without an <?xml ...?> declaration".into());
    }
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let text = indent_rest(xml, edit::line_indent(source, n.start), nl);
    edit::apply(source, &[Splice { start: n.start, end: n.end, text }])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::display::reindent;

    const SRC: &str = "<monsters>\n    <name>F</name>\n    <monster id=\"m\">\n        <name>M</name>\n    </monster>\n</monsters>\n";

    #[test]
    fn replaces_at_its_indentation() {
        let nodes = parse(SRC).unwrap().nodes;
        let m = at_path(&nodes, &[1]).unwrap();
        let shown = reindent(SRC, nodes[m].start, nodes[m].end);
        assert_eq!(shown, "<monster id=\"m\">\n    <name>M</name>\n</monster>");
        // Unchanged text gives the file back.
        let same = plan(SRC, &ElementEdit { path: vec![1], tag: "monster".into(), xml: shown.clone() }).unwrap();
        assert_eq!(same, SRC);
        let out = plan(SRC, &ElementEdit { path: vec![1], tag: "monster".into(), xml: shown.replace("M<", "N<") }).unwrap();
        assert!(out.contains("    <monster id=\"m\">\n        <name>N</name>\n    </monster>"), "{out}");
    }

    #[test]
    fn refuses_other_tags_and_bad_xml() {
        let e = |xml: &str| plan(SRC, &ElementEdit { path: vec![0], tag: "name".into(), xml: xml.into() });
        assert!(e("<brief>x</brief>").is_err());
        assert!(e("<name>x").is_err());
        assert!(e("<name>a</name><name>b</name>").is_err());
        assert!(e("<name>G</name>").unwrap().contains("<name>G</name>"));
    }
}
