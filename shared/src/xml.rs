//! Reading a supplement file into the file-structure tree.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::{AppError, StructNode};

/// Mixed-content elements (paragraphType and kin) the tree never opens, even when they
/// hold only inline tags. Any other element with text directly inside is a leaf too.
const MIXED: &[&str] = &["p", "li", "trigger", "td", "rd", "chapterTitle"];

/// Child elements whose text titles their parent in the tree.
const HEADING_CHILDREN: &[&str] = &["chapterTitle", "title", "headerTitle", "name"];

/// Attributes that identify an element, in the order they're tried.
const ID_ATTRS: &[&str] = &["name", "id", "ref", "type", "speaker", "file"];

/// Snippets are cut to this many characters.
const SNIPPET: usize = 60;

#[derive(Debug)]
pub struct Parsed {
    pub root_tag: String,
    /// The root's xsi:noNamespaceSchemaLocation.
    pub schema_location: Option<String>,
    pub nodes: Vec<StructNode>,
}

struct Frame {
    idx: usize,
    /// Text found directly inside this element (not in a child).
    direct_text: bool,
    /// All text inside, cut short; used for snippets and headings.
    text: String,
    /// Inside a MIXED element: no nodes are made for descendants.
    suppress: bool,
}

/// 1-based line of a byte offset.
pub fn line_of(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset.min(source.len())].iter().filter(|b| **b == b'\n').count() + 1
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn attr(e: &BytesStart, wanted: &str) -> Option<String> {
    e.attributes().flatten().find(|a| local(a.key.as_ref()) == wanted).and_then(|a| {
        a.unescape_value().ok().map(|v| v.into_owned())
    })
}

fn first_id(e: &BytesStart) -> Option<String> {
    ID_ATTRS.iter().find_map(|a| attr(e, a))
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn cut(s: &str) -> String {
    let s = squash(s);
    if s.chars().count() > SNIPPET {
        format!("{}…", s.chars().take(SNIPPET).collect::<String>().trim_end())
    } else {
        s
    }
}

fn push_text(frames: &mut [Frame], text: &str) {
    if text.trim().is_empty() {
        return;
    }
    if let Some(top) = frames.last_mut() {
        top.direct_text = true;
    }
    for f in frames.iter_mut() {
        if f.text.len() < 400 {
            f.text.push_str(text);
            f.text.push(' ');
        }
    }
}

pub fn parse(source: &str) -> Result<Parsed, AppError> {
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(false);
    let mut nodes: Vec<StructNode> = Vec::new();
    let mut ids: Vec<Option<String>> = Vec::new();
    let mut texts: Vec<String> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let mut root: Option<(String, Option<String>)> = None;
    // Elements inside a suppressed (MIXED) element: just a depth count.
    let mut suppressed_depth = 0usize;

    let syntax = |pos: usize, e: &dyn std::fmt::Display| {
        AppError::new(format!("The file isn't well-formed XML (line {}): {e}", line_of(source, pos)))
    };

    loop {
        let before = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| syntax(reader.error_position() as usize, &e))?;
        let after = reader.buffer_position() as usize;
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(event, Event::Empty(_));
                let tag = local(e.name().as_ref());
                if root.is_none() {
                    root = Some((tag.clone(), attr(e, "noNamespaceSchemaLocation")));
                } else if frames.is_empty() {
                    return Err(syntax(before, &"more than one root element"));
                }
                if suppressed_depth > 0 || frames.last().is_some_and(|f| f.suppress) {
                    if !empty {
                        suppressed_depth += 1;
                    }
                    continue;
                }
                // Which elements the tree opens: in a monster file, the family, its lairs
                // and stat blocks and their entry lists; elsewhere, all but mixed content.
                let open = if root.as_ref().is_some_and(|(t, _)| t == "monsters") {
                    crate::monster::TREE_OPEN.contains(&tag.as_str())
                } else {
                    !MIXED.contains(&tag.as_str())
                };
                let idx = nodes.len();
                if let Some(parent) = frames.last() {
                    nodes[parent.idx].children.push(idx);
                }
                nodes.push(StructNode {
                    tag: tag.clone(),
                    label: String::new(),
                    start: before,
                    end: after,
                    line: line_of(source, before),
                    children: Vec::new(),
                    leaf: !open,
                });
                ids.push(first_id(e));
                texts.push(String::new());
                if !empty {
                    frames.push(Frame {
                        idx,
                        direct_text: false,
                        text: String::new(),
                        suppress: !open,
                    });
                }
            }
            Event::End(_) => {
                if suppressed_depth > 0 {
                    suppressed_depth -= 1;
                    continue;
                }
                let f = frames.pop().ok_or_else(|| syntax(before, &"unexpected end tag"))?;
                let node = &mut nodes[f.idx];
                node.end = after;
                if f.direct_text {
                    node.leaf = true;
                }
                if node.leaf {
                    node.children.clear();
                }
                texts[f.idx] = f.text;
            }
            Event::Text(t) => {
                let text = t.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                push_text(&mut frames, &text);
            }
            Event::CData(t) => push_text(&mut frames, &String::from_utf8_lossy(&t)),
            Event::Eof => break,
            _ => {}
        }
    }
    if !frames.is_empty() {
        return Err(syntax(source.len(), &format!("<{}> is never closed", nodes[frames[0].idx].tag)));
    }
    let (root_tag, schema_location) = root.ok_or_else(|| AppError::new("The file has no root element"))?;

    // Labels: a heading child's text, then an identifying attribute; leaves show their text.
    let labels: Vec<String> = (0..nodes.len())
        .map(|i| {
            let n = &nodes[i];
            let heading = n
                .children
                .iter()
                .find(|c| HEADING_CHILDREN.contains(&nodes[**c].tag.as_str()))
                .map(|c| cut(&texts[*c]))
                .filter(|s| !s.is_empty());
            let id = ids[i].clone();
            match (heading, id) {
                (Some(h), Some(id)) => format!("{h} · {id}"),
                (Some(h), None) => h,
                (None, Some(id)) => id,
                (None, None) if n.leaf || n.children.is_empty() => cut(&texts[i]),
                (None, None) => String::new(),
            }
        })
        .collect();
    for (n, l) in nodes.iter_mut().zip(labels) {
        n.label = l;
    }
    Ok(Parsed { root_tag, schema_location, nodes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<?xml version="1.0"?>
<!-- a comment -->
<supplement type="adventure" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="../s/supplement-template-schema.xsd">
    <chapter name="intro">
        <chapterTitle>Introduction</chapterTitle>
        <section name="s1">
            <title>What &amp; Why</title>
            <p>Some <b>bold</b> text that is long enough to need cutting because it goes on and on and on.</p>
            <p><monster ref="x"/></p>
        </section>
    </chapter>
    <chapterOrder><ch name="intro"/></chapterOrder>
</supplement>"#;

    #[test]
    fn builds_block_level_tree() {
        let p = parse(SRC).unwrap();
        assert_eq!(p.root_tag, "supplement");
        assert_eq!(p.schema_location.as_deref(), Some("../s/supplement-template-schema.xsd"));
        let root = &p.nodes[0];
        let tags: Vec<_> = root.children.iter().map(|c| p.nodes[*c].tag.as_str()).collect();
        assert_eq!(tags, vec!["chapter", "chapterOrder"]);
        let chapter = &p.nodes[root.children[0]];
        assert_eq!(chapter.label, "Introduction · intro");
        let section = &p.nodes[chapter.children[1]];
        assert_eq!(section.label, "What & Why · s1");
        // Both paragraphs are leaves; inline tags made no nodes.
        let paras: Vec<_> = section.children.iter().map(|c| &p.nodes[*c]).filter(|n| n.tag == "p").collect();
        assert_eq!(paras.len(), 2);
        assert!(paras.iter().all(|n| n.leaf && n.children.is_empty()));
        assert!(paras[0].label.starts_with("Some bold text"));
        assert!(paras[0].label.ends_with('…'));
        assert!(!p.nodes.iter().any(|n| n.tag == "b" || n.tag == "monster"));
    }

    #[test]
    fn spans_cover_whole_element() {
        let p = parse(SRC).unwrap();
        let chapter = &p.nodes[p.nodes[0].children[0]];
        let text = &SRC[chapter.start..chapter.end];
        assert!(text.starts_with("<chapter name=\"intro\">"));
        assert!(text.ends_with("</chapter>"));
        assert_eq!(chapter.line, 4);
        assert_eq!(&SRC[p.nodes[0].start..p.nodes[0].end].len(), &(SRC.len() - SRC.find("<supplement").unwrap()));
    }

    #[test]
    fn reports_malformed_xml_with_line() {
        let err = parse("<a>\n<b>\n</a>").unwrap_err();
        assert!(err.message.contains("well-formed"), "{}", err.message);
        assert!(parse("<a>").is_err());
    }
}
