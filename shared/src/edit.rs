//! Surgical edits to XML source text.
//!
//! Every edit is a [`Splice`]: replace one byte range of the original text. Plans build
//! a list of splices against the original, and [`apply`] makes them all at once, so
//! nothing outside the edited ranges (comments, spacing, attribute order, quoting)
//! ever changes.

use crate::display::{tokenize, TokenKind};
use crate::StructNode;

/// Replace `start..end` of the original text with `text`. An insert has `start == end`.
#[derive(Clone, Debug, PartialEq)]
pub struct Splice {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

impl Splice {
    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Splice { start: at, end: at, text: text.into() }
    }
}

/// Applies splices to `source`. Splices at the same position go in the order given, so
/// a plan that inserts several children at one point lists them in schema order.
/// Overlapping splices are a bug in the plan, reported as an error.
pub fn apply(source: &str, splices: &[Splice]) -> Result<String, String> {
    let mut order: Vec<usize> = (0..splices.len()).collect();
    order.sort_by_key(|&i| (splices[i].start, i));
    let mut out = String::with_capacity(source.len() + 256);
    let mut pos = 0;
    for i in order {
        let s = &splices[i];
        if s.start < pos || s.end < s.start || s.end > source.len() {
            return Err(format!("overlapping edits at byte {}", s.start));
        }
        out.push_str(&source[pos..s.start]);
        out.push_str(&s.text);
        pos = s.end;
    }
    out.push_str(&source[pos..]);
    Ok(out)
}

pub fn escape_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

pub fn escape_attr(s: &str, quote: char) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;");
    if quote == '\'' {
        s.replace('\'', "&apos;")
    } else {
        s.replace('"', "&quot;")
    }
}

pub fn unescape(s: &str) -> String {
    quick_xml::escape::unescape(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string())
}

/// One attribute in a start tag.
#[derive(Clone, Debug, PartialEq)]
pub struct AttrSpan {
    pub name: String,
    /// Unescaped value.
    pub value: String,
    /// The quoted value, quotes included.
    pub value_start: usize,
    pub value_end: usize,
    /// From the whitespace before the name to the end of the value: what removing it removes.
    pub remove_start: usize,
    pub quote: char,
}

/// The parsed start tag of the element at `start`.
#[derive(Clone, Debug, PartialEq)]
pub struct StartTag {
    pub attrs: Vec<AttrSpan>,
    /// Where a new attribute goes: after the last attribute, or after the tag name.
    pub insert_at: usize,
    /// Just past the closing `>` or `/>`.
    pub end: usize,
    /// `<x/>`
    pub self_closing: bool,
    /// Byte offset of `/>` or `>`.
    pub close_at: usize,
}

pub fn start_tag(source: &str, start: usize) -> Option<StartTag> {
    let toks = tokenize(&source[start..]);
    let mut pos = start;
    let mut attrs = Vec::new();
    let mut insert_at = None;
    let mut pending: Option<(String, usize)> = None; // attr name, remove_start
    let mut last_ws_start = None;
    for (i, (kind, text)) in toks.iter().enumerate() {
        let here = pos;
        pos += text.len();
        match kind {
            TokenKind::Punct if i == 0 => {
                if *text != "<" {
                    return None;
                }
            }
            TokenKind::TagName => insert_at = Some(pos),
            TokenKind::Text => {}
            TokenKind::AttrName => pending = Some((text.to_string(), last_ws_start.unwrap_or(here))),
            TokenKind::AttrValue => {
                let (name, remove_start) = pending.take()?;
                let quote = text.chars().next()?;
                attrs.push(AttrSpan {
                    name,
                    value: unescape(&text[1..text.len() - 1]),
                    value_start: here,
                    value_end: pos,
                    remove_start,
                    quote,
                });
                insert_at = Some(pos);
            }
            TokenKind::Punct if *text == ">" || *text == "/>" => {
                return Some(StartTag {
                    attrs,
                    insert_at: insert_at?,
                    end: pos,
                    self_closing: *text == "/>",
                    close_at: here,
                });
            }
            TokenKind::Punct => {}
            _ => return None,
        }
        last_ws_start = if *kind == TokenKind::Text { Some(here) } else { None };
    }
    None
}

impl StartTag {
    pub fn get(&self, name: &str) -> Option<&AttrSpan> {
        self.attrs.iter().find(|a| a.name == name || a.name.rsplit(':').next() == Some(name))
    }
    pub fn value(&self, name: &str) -> Option<String> {
        self.get(name).map(|a| a.value.clone())
    }
}

/// The span between an element's start and end tags. None for `<x/>`.
pub fn inner_span(source: &str, node: &StructNode) -> Option<(usize, usize)> {
    let tag = start_tag(source, node.start)?;
    if tag.self_closing {
        return None;
    }
    let close = node.start + source[node.start..node.end].rfind("</")?;
    Some((tag.end, close))
}

/// The element's text content, unescaped. Empty for `<x/>`.
pub fn text_of(source: &str, node: &StructNode) -> String {
    inner_span(source, node).map(|(a, b)| unescape(&source[a..b])).unwrap_or_default()
}

/// Sets, changes or removes one attribute. None when nothing changes.
pub fn set_attr(source: &str, node: &StructNode, name: &str, value: Option<&str>) -> Option<Splice> {
    set_attr_at(source, node.start, name, value)
}

/// [`set_attr`] for the element whose start tag begins at `start` (for elements the
/// tree doesn't hold, like inline tags inside a paragraph).
pub fn set_attr_at(source: &str, start: usize, name: &str, value: Option<&str>) -> Option<Splice> {
    let tag = start_tag(source, start)?;
    match (tag.get(name), value) {
        (Some(a), Some(v)) if a.value == v => None,
        (Some(a), Some(v)) => Some(Splice {
            start: a.value_start,
            end: a.value_end,
            text: format!("{q}{}{q}", escape_attr(v, a.quote), q = a.quote),
        }),
        (Some(a), None) => Some(Splice { start: a.remove_start, end: a.value_end, text: String::new() }),
        (None, Some(v)) => Some(Splice::insert(tag.insert_at, format!(" {name}=\"{}\"", escape_attr(v, '"')))),
        (None, None) => None,
    }
}

/// Replaces a text-only element's content.
pub fn set_text(source: &str, node: &StructNode, text: &str) -> Option<Splice> {
    let tag = start_tag(source, node.start)?;
    if tag.self_closing {
        return Some(Splice {
            start: tag.close_at,
            end: tag.end,
            text: format!(">{}</{}>", escape_text(text), node.tag),
        });
    }
    let (a, b) = inner_span(source, node)?;
    (unescape(&source[a..b]) != text).then(|| Splice { start: a, end: b, text: escape_text(text) })
}

fn line_start(source: &str, pos: usize) -> usize {
    source[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// The whitespace at the start of the line holding `pos`.
pub fn line_indent(source: &str, pos: usize) -> &str {
    let ls = line_start(source, pos);
    let rest = &source[ls..];
    &rest[..rest.len() - rest.trim_start_matches([' ', '\t']).len()]
}

/// One level of indentation, guessed from the file: a tab if its lines start with tabs.
pub fn indent_unit(source: &str) -> &'static str {
    let tabs = source.lines().filter(|l| l.starts_with('\t')).count();
    let spaces = source.lines().filter(|l| l.starts_with("  ")).count();
    if tabs > spaces { "\t" } else { "    " }
}

/// Is `source[ls..pos]` only spaces and tabs?
fn blank_before(source: &str, pos: usize) -> bool {
    source[line_start(source, pos)..pos].chars().all(|c| c == ' ' || c == '\t')
}

/// The end of an element's line: past a comment that follows it on the same line and
/// the newline, when the rest of the line is only that. None when other markup follows.
pub fn line_end_after(source: &str, end: usize) -> Option<usize> {
    let rest = &source[end..];
    let ws = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let mut p = end + ws;
    if source[p..].starts_with("<!--") {
        p += source[p..].find("-->")? + 3;
        let rest = &source[p..];
        p += rest.len() - rest.trim_start_matches([' ', '\t']).len();
    }
    if p == source.len() {
        return Some(p);
    }
    if source[p..].starts_with("\r\n") {
        return Some(p + 2);
    }
    if source[p..].starts_with('\n') {
        return Some(p + 1);
    }
    None
}

fn newline(source: &str) -> &'static str {
    if source.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Removes an element. When it's alone on its line (with at most a trailing comment),
/// the whole line goes.
pub fn remove_element(source: &str, node: &StructNode) -> Splice {
    if blank_before(source, node.start) {
        if let Some(end) = line_end_after(source, node.end) {
            return Splice { start: line_start(source, node.start), end, text: String::new() };
        }
    }
    Splice { start: node.start, end: node.end, text: String::new() }
}

/// An element's text with its markup stripped and whitespace squashed.
pub fn plain_text(source: &str, node: &StructNode) -> String {
    let mut reader = quick_xml::Reader::from_str(&source[node.start..node.end]);
    let mut out = String::new();
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Text(t)) => {
                out.push_str(&t.unescape().map(|c| c.into_owned()).unwrap_or_default());
                out.push(' ');
            }
            Ok(quick_xml::events::Event::CData(t)) => out.push_str(&String::from_utf8_lossy(&t)),
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Cuts text to about `n` characters at a word boundary, with an ellipsis.
pub fn snippet(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_string();
    }
    let cut: String = text.chars().take(n).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > n / 2 => &cut[..i],
        _ => &cut[..],
    };
    format!("{}…", cut.trim_end_matches([' ', ',', ';', ':', '.']))
}

/// Moves an element before `before`, or after `last` (the last sibling) when `before` is
/// None. An element alone on its line moves with the whole line (indent, trailing
/// comment); comments on lines of their own stay where they are.
pub fn move_element(
    source: &str,
    node: &StructNode,
    before: Option<&StructNode>,
    last: Option<&StructNode>,
) -> Result<Vec<Splice>, String> {
    let own_line = |n: &StructNode| blank_before(source, n.start) && line_end_after(source, n.end).is_some();
    let (from, to) = if own_line(node) {
        (line_start(source, node.start), line_end_after(source, node.end).unwrap())
    } else {
        (node.start, node.end)
    };
    let mut text = source[from..to].to_string();
    let lined = own_line(node);
    if lined && !text.ends_with('\n') {
        text.push('\n');
    }
    let at = match (before, last) {
        (Some(b), _) if lined && blank_before(source, b.start) => line_start(source, b.start),
        (Some(b), _) => b.start,
        (None, Some(l)) if lined => line_end_after(source, l.end).unwrap_or(l.end),
        (None, Some(l)) => l.end,
        (None, None) => return Err("nowhere to move it".into()),
    };
    if at > from && at < to {
        return Err("can't move an element into itself".into());
    }
    Ok(vec![Splice { start: from, end: to, text: String::new() }, Splice::insert(at, text)])
}

/// Moves a block-level element that sits on lines of its own with a blank line after it
/// (a chapter), blank line and all: before `before`, or after `last` when `before` is
/// None, so the blank lines between blocks stay one each.
pub fn move_block(source: &str, node: &StructNode, before: Option<&StructNode>, last: &StructNode) -> Result<Vec<Splice>, String> {
    let lines = |n: &StructNode| -> Result<(usize, usize), String> {
        if !blank_before(source, n.start) {
            return Err("the element doesn't start its line".into());
        }
        Ok((line_start(source, n.start), line_end_after(source, n.end).ok_or("something follows the element on its line")?))
    };
    // Past one blank line after the block.
    let with_blank = |end: usize| {
        let rest = &source[end..];
        let pad = rest.len() - rest.trim_start_matches([' ', '\t', '\r']).len();
        if rest[pad..].starts_with('\n') { end + pad + 1 } else { end }
    };
    let (from, end) = lines(node)?;
    let to = with_blank(end);
    let body = &source[from..end];
    let (at, text) = match before {
        Some(b) => (lines(b)?.0, format!("{body}\n")),
        None => {
            if last.start == node.start {
                return Ok(Vec::new());
            }
            (lines(last)?.1, format!("\n{body}"))
        }
    };
    if at > from && at < to {
        return Err("can't move an element into itself".into());
    }
    Ok(vec![Splice { start: from, end: to, text: String::new() }, Splice::insert(at, text)])
}

/// Where a new child goes.
pub enum Anchor<'a> {
    /// After this sibling (and its same-line comment).
    After(&'a StructNode),
    /// Before this sibling.
    Before(&'a StructNode),
    /// The parent has no remaining children: as its last content.
    Into(&'a StructNode),
}

/// Inserts `xml` (one element, possibly several lines already indented one level
/// relative to its own first line) as a child, on its own line, matching the indentation.
pub fn insert_child(source: &str, anchor: Anchor, xml: &str) -> Option<Splice> {
    let nl = newline(source);
    let indent_lines = |xml: &str, indent: &str| -> String {
        xml.lines().map(|l| format!("{indent}{l}")).collect::<Vec<_>>().join(nl)
    };
    Some(match anchor {
        Anchor::After(sib) => match line_end_after(source, sib.end) {
            Some(end) if blank_before(source, sib.start) => {
                let indent = line_indent(source, sib.start);
                // At the end of the file there may be no newline to follow.
                let lead = if end == source.len() && !source.ends_with('\n') { nl } else { "" };
                Splice::insert(end, format!("{lead}{}{nl}", indent_lines(xml, indent)))
            }
            _ => Splice::insert(sib.end, xml.to_string()),
        },
        Anchor::Before(sib) => {
            if blank_before(source, sib.start) {
                let indent = line_indent(source, sib.start);
                Splice::insert(line_start(source, sib.start), format!("{}{nl}", indent_lines(xml, indent)))
            } else {
                Splice::insert(sib.start, xml.to_string())
            }
        }
        Anchor::Into(parent) => {
            let pind = line_indent(source, parent.start);
            let cind = format!("{pind}{}", indent_unit(source));
            let tag = start_tag(source, parent.start)?;
            if tag.self_closing {
                Splice {
                    start: tag.close_at,
                    end: tag.end,
                    text: format!(">{nl}{}{nl}{pind}</{}>", indent_lines(xml, &cind), parent.tag),
                }
            } else {
                let (_, close) = inner_span(source, parent)?;
                if blank_before(source, close) {
                    Splice::insert(line_start(source, close), format!("{}{nl}", indent_lines(xml, &cind)))
                } else {
                    Splice::insert(close, format!("{nl}{}{nl}{pind}", indent_lines(xml, &cind)))
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse;

    fn node<'a>(nodes: &'a [StructNode], tag: &str) -> &'a StructNode {
        nodes.iter().find(|n| n.tag == tag).unwrap()
    }

    #[test]
    fn start_tag_finds_attributes() {
        let src = r#"<a x="1"  y='t&amp;u' z = "3" />"#;
        let t = start_tag(src, 0).unwrap();
        assert!(t.self_closing);
        assert_eq!(t.value("y").as_deref(), Some("t&u"));
        assert_eq!(t.get("z").unwrap().quote, '"');
        assert_eq!(&src[t.close_at..t.end], "/>");
    }

    #[test]
    fn attr_change_add_remove_keeps_everything_else() {
        let src = "<r>\n  <c a=\"1\"  b='2'><!-- keep --></c>\n</r>";
        let nodes = parse(src).unwrap().nodes;
        let c = node(&nodes, "c");
        let out = apply(src, &[
            set_attr(src, c, "a", Some("x\"y")).unwrap(),
            set_attr(src, c, "b", None).unwrap(),
            set_attr(src, c, "d", Some("4")).unwrap(),
        ])
        .unwrap();
        assert_eq!(out, "<r>\n  <c a=\"x&quot;y\" d=\"4\"><!-- keep --></c>\n</r>");
        assert!(set_attr(src, c, "a", Some("1")).is_none());
    }

    #[test]
    fn text_set_on_empty_and_full_elements() {
        let src = "<r><t>old &amp; gone</t><e/></r>";
        let nodes = parse(src).unwrap().nodes;
        let out = apply(src, &[
            set_text(src, node(&nodes, "t"), "new <b>").unwrap(),
            set_text(src, node(&nodes, "e"), "now").unwrap(),
        ])
        .unwrap();
        assert_eq!(out, "<r><t>new &lt;b&gt;</t><e>now</e></r>");
        assert_eq!(text_of(src, node(&nodes, "t")), "old & gone");
    }

    #[test]
    fn remove_takes_line_and_trailing_comment() {
        let src = "<r>\n    <a/>\n    <b>x</b>  <!-- about b\n         continued -->\n    <c/>\n</r>";
        let nodes = parse(src).unwrap().nodes;
        let out = apply(src, &[remove_element(src, node(&nodes, "b"))]).unwrap();
        assert_eq!(out, "<r>\n    <a/>\n    <c/>\n</r>");
    }

    #[test]
    fn insert_after_before_and_into() {
        let src = "<r>\n    <a/> <!-- a -->\n    <c/>\n</r>";
        let nodes = parse(src).unwrap().nodes;
        let out = apply(src, &[
            insert_child(src, Anchor::After(node(&nodes, "a")), "<b/>").unwrap(),
            insert_child(src, Anchor::After(node(&nodes, "a")), "<b2/>").unwrap(),
            insert_child(src, Anchor::Before(node(&nodes, "a")), "<z>\n    <y/>\n</z>").unwrap(),
        ])
        .unwrap();
        assert_eq!(out, "<r>\n    <z>\n        <y/>\n    </z>\n    <a/> <!-- a -->\n    <b/>\n    <b2/>\n    <c/>\n</r>");

        let src = "<r>\n    <p/>\n    <q>\n    </q>\n</r>";
        let nodes = parse(src).unwrap().nodes;
        let out = apply(src, &[
            insert_child(src, Anchor::Into(node(&nodes, "p")), "<x/>").unwrap(),
            insert_child(src, Anchor::Into(node(&nodes, "q")), "<y/>").unwrap(),
        ])
        .unwrap();
        assert_eq!(out, "<r>\n    <p>\n        <x/>\n    </p>\n    <q>\n        <y/>\n    </q>\n</r>");
    }

    #[test]
    fn overlapping_splices_are_refused() {
        let s = [Splice { start: 0, end: 3, text: "x".into() }, Splice { start: 2, end: 4, text: "y".into() }];
        assert!(apply("abcdef", &s).is_err());
    }
}
