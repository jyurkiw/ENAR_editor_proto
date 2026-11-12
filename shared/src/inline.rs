//! Inline ref tags in a paragraph's XML: reading them out of the text, what each prints
//! (as the compiler resolves it), and the problems found along the way.
//!
//! The paragraph editor keeps the raw XML as the source of truth. This module turns that
//! text into [`Piece`]s for the preview line: runs of text (with their bold/italic) and
//! [`RefTag`]s, each with the byte span it covers in the text, so the editor can select a
//! tag or rewrite it in place.
//!
//! Problems are [`Diagnostic`]s over byte spans of the text, from any [`Checker`]. The
//! ref checker ([`RefCtx`]) is the first; a spell checker can report through the same
//! type later and the preview shows both the same way.

use std::collections::HashMap;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::edit::{self, escape_attr, escape_text};
use crate::StructNode;

/// The compiler's XP by challenge rating (encompile.text.XP).
const XP: &[(&str, &str)] = &[
    ("0", "10"), ("1/8", "25"), ("1/4", "50"), ("1/2", "100"), ("1", "200"), ("2", "450"),
    ("3", "700"), ("4", "1,100"), ("5", "1,800"), ("6", "2,300"), ("7", "2,900"), ("8", "3,900"),
    ("9", "5,000"), ("10", "5,900"), ("11", "7,200"), ("12", "8,400"), ("13", "10,000"),
    ("14", "11,500"), ("15", "13,000"), ("16", "15,000"), ("17", "18,000"), ("18", "20,000"),
    ("19", "22,000"), ("20", "25,000"), ("21", "33,000"), ("22", "41,000"), ("23", "50,000"),
    ("24", "62,000"), ("25", "75,000"), ("26", "90,000"), ("27", "105,000"), ("28", "120,000"),
    ("29", "135,000"), ("30", "155,000"), ("none", "0"),
];

/// How the compiler styles what a tag prints (its house style, before `emphasized`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Bold the first time the name appears in a chapter.
    Creature,
    /// Italic.
    Title,
    Plain,
}

/// One kind of inline ref tag.
pub struct TagSpec {
    pub tag: &'static str,
    /// What the Insert menu and the editor call it.
    pub label: &'static str,
    /// The `<reference>` entry kind its `ref` names, if any.
    pub kind: Option<&'static str>,
    /// The `value` choices (empty: the tag has no `value`).
    pub values: &'static [&'static str],
    /// The tag's text is its content (the core book refs).
    pub has_text: bool,
    /// Takes `emphasized`.
    pub emph: bool,
    pub style: Style,
}

pub const TAGS: &[TagSpec] = &[
    TagSpec { tag: "monsterRef", label: "Monster", kind: Some("monster"), values: &["name", "displayName", "shortName", "cr", "src"], has_text: false, emph: true, style: Style::Creature },
    TagSpec { tag: "nounRef", label: "Noun", kind: Some("noun"), values: &["name", "displayName", "shortName"], has_text: false, emph: true, style: Style::Creature },
    TagSpec { tag: "spellRef", label: "Spell", kind: Some("spell"), values: &["name", "src"], has_text: false, emph: true, style: Style::Title },
    TagSpec { tag: "itemRef", label: "Item", kind: Some("item"), values: &["name", "displayName", "src"], has_text: false, emph: true, style: Style::Title },
    TagSpec { tag: "dataRef", label: "Data value", kind: Some("data"), values: &["value", "body"], has_text: false, emph: true, style: Style::Plain },
    TagSpec { tag: "monster", label: "Inline monster", kind: Some("monster"), values: &[], has_text: false, emph: true, style: Style::Creature },
    TagSpec { tag: "chapterRef", label: "Chapter", kind: None, values: &[], has_text: false, emph: true, style: Style::Plain },
    TagSpec { tag: "figureRef", label: "Figure", kind: None, values: &[], has_text: false, emph: true, style: Style::Plain },
    TagSpec { tag: "coreBookRef", label: "Core book", kind: None, values: &[], has_text: true, emph: false, style: Style::Title },
    TagSpec { tag: "coreBookChapterRef", label: "Core book chapter", kind: None, values: &[], has_text: true, emph: false, style: Style::Plain },
    TagSpec { tag: "coreBookSectionRef", label: "Core book section", kind: None, values: &[], has_text: true, emph: false, style: Style::Plain },
    TagSpec { tag: "coreBookTableRef", label: "Core book table", kind: None, values: &[], has_text: true, emph: false, style: Style::Plain },
];

pub fn spec(tag: &str) -> Option<&'static TagSpec> {
    TAGS.iter().find(|s| s.tag == tag)
}

/// A ref tag as written in the paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefTag {
    pub tag: String,
    /// In the order written.
    pub attrs: Vec<(String, String)>,
    /// The content, for tags that have some (the core book refs).
    pub text: String,
    /// The bytes of the paragraph text the tag covers.
    pub start: usize,
    pub end: usize,
}

impl RefTag {
    pub fn new(tag: &str) -> Self {
        RefTag { tag: tag.to_string(), attrs: Vec::new(), text: String::new(), start: 0, end: 0 }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    /// Sets an attribute in place, adds it at the end, or (with None) removes it.
    pub fn set(&mut self, name: &str, value: Option<&str>) {
        match (self.attrs.iter().position(|(k, _)| k == name), value) {
            (Some(i), Some(v)) => self.attrs[i].1 = v.to_string(),
            (Some(i), None) => {
                self.attrs.remove(i);
            }
            (None, Some(v)) => self.attrs.push((name.to_string(), v.to_string())),
            (None, None) => {}
        }
    }

    pub fn spec(&self) -> Option<&'static TagSpec> {
        spec(&self.tag)
    }

    /// The tag as XML: `<tag a="v"/>`, or `<tag a="v">text</tag>` for one with content.
    pub fn to_xml(&self) -> String {
        let mut s = format!("<{}", self.tag);
        for (k, v) in &self.attrs {
            s.push_str(&format!(" {k}=\"{}\"", escape_attr(v, '"')));
        }
        if self.spec().is_some_and(|s| s.has_text) || !self.text.is_empty() {
            s.push_str(&format!(">{}</{}>", escape_text(&self.text), self.tag));
        } else {
            s.push_str("/>");
        }
        s
    }
}

/// A run of the paragraph as the preview shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        /// The innermost other inline tag around it (`skillcheck`, `condition`, ...).
        term: Option<String>,
        start: usize,
        end: usize,
    },
    Ref(RefTag),
}

/// The paragraph text isn't well-formed: the message and the byte it stopped at.
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub at: usize,
}

const WRAP_OPEN: &str = "<p>";

fn name_of(e: &BytesStart) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).into_owned()
}

fn attrs_of(e: &BytesStart) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|e| e.to_string())?;
        let k = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let v = a.unescape_value().map_err(|e| e.to_string())?.into_owned();
        out.push((k, v));
    }
    Ok(out)
}

/// Reads a paragraph's inner XML into pieces.
pub fn parse(text: &str) -> Result<Vec<Piece>, ParseError> {
    let wrapped = format!("{WRAP_OPEN}{text}</p>");
    let off = WRAP_OPEN.len();
    let clamp = |p: usize| p.saturating_sub(off).min(text.len());
    let mut reader = Reader::from_str(&wrapped);
    reader.config_mut().trim_text(false);
    let mut out = Vec::new();
    // Open non-ref tags, outermost first.
    let mut stack: Vec<String> = Vec::new();
    // An open ref tag with content: the tag so far, and the depth of tags inside it.
    let mut open_ref: Option<(RefTag, usize)> = None;
    let mut started = false;
    loop {
        let before = reader.buffer_position() as usize;
        let ev = reader.read_event().map_err(|e| ParseError { message: e.to_string(), at: clamp(reader.error_position() as usize) })?;
        let after = reader.buffer_position() as usize;
        let bad = |m: String| ParseError { message: m, at: clamp(before) };
        if !started {
            // The wrapper's own <p>.
            started = true;
            continue;
        }
        match ev {
            Event::Start(e) => {
                let tag = name_of(&e);
                if let Some((_, depth)) = open_ref.as_mut() {
                    *depth += 1;
                    continue;
                }
                if spec(&tag).is_some() {
                    let attrs = attrs_of(&e).map_err(bad)?;
                    open_ref = Some((RefTag { tag, attrs, text: String::new(), start: clamp(before), end: 0 }, 0));
                } else {
                    stack.push(tag);
                }
            }
            Event::Empty(e) => {
                if open_ref.is_some() {
                    continue;
                }
                let tag = name_of(&e);
                if spec(&tag).is_some() {
                    let attrs = attrs_of(&e).map_err(bad)?;
                    out.push(Piece::Ref(RefTag { tag, attrs, text: String::new(), start: clamp(before), end: clamp(after) }));
                }
            }
            Event::End(_) => {
                if let Some((mut t, depth)) = open_ref.take() {
                    if depth > 0 {
                        open_ref = Some((t, depth - 1));
                    } else {
                        t.end = clamp(after);
                        out.push(Piece::Ref(t));
                    }
                    continue;
                }
                if stack.pop().is_none() {
                    // The wrapper's </p>: the end.
                    break;
                }
            }
            Event::Text(t) => {
                let s = t.unescape().map_err(|e| bad(e.to_string()))?.into_owned();
                if let Some((r, _)) = open_ref.as_mut() {
                    r.text.push_str(&s);
                    continue;
                }
                let bold = stack.iter().any(|t| t == "b");
                let italic = stack.iter().any(|t| t == "i");
                let term = stack.iter().rev().find(|t| *t != "b" && *t != "i").cloned();
                out.push(Piece::Text { text: s, bold, italic, term, start: clamp(before), end: clamp(after) });
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(&t).into_owned();
                out.push(Piece::Text { text: s, bold: false, italic: false, term: None, start: clamp(before), end: clamp(after) });
            }
            Event::Eof => return Err(ParseError { message: "unexpected end".into(), at: text.len() }),
            _ => {}
        }
    }
    // Anything after the wrapper's </p> means the text closed a tag it never opened.
    let rest = reader.buffer_position() as usize;
    if rest < wrapped.len() {
        return Err(ParseError { message: "a closing tag here has no opening tag".into(), at: clamp(rest) });
    }
    Ok(out)
}

/// Every ref tag in the pieces, in order. The editor names a tag by its position here.
pub fn refs(pieces: &[Piece]) -> impl Iterator<Item = &RefTag> {
    pieces.iter().filter_map(|p| match p {
        Piece::Ref(t) => Some(t),
        _ => None,
    })
}

/// The text with the ref tag at `start..end` replaced.
pub fn replace(text: &str, start: usize, end: usize, with: &str) -> String {
    format!("{}{}{}", &text[..start], with, &text[end..])
}

// ------------------------------------------------------------------ diagnostics

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// The compiler stops on it.
    Error,
    Warning,
}

/// A problem with part of a text, from any checker.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    /// The bytes of the text it's about.
    pub start: usize,
    pub end: usize,
    pub severity: Severity,
    /// Which checker found it ("refs", later "spelling").
    pub source: &'static str,
    pub message: String,
}

/// Something that reports problems with a paragraph.
pub trait Checker {
    fn check(&self, text: &str, pieces: &[Piece]) -> Vec<Diagnostic>;
}

// ------------------------------------------------------------------ resolution

/// What the compiler prints for each value of each `<reference>` entry, from the
/// backend (which reads the entries' files with the compiler's loader).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RefIndex {
    pub entries: HashMap<String, IndexEntry>,
    /// Loading problems (a missing file, a monster not in its file).
    pub problems: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct IndexEntry {
    pub kind: String,
    pub values: HashMap<String, String>,
    pub errors: HashMap<String, String>,
}

/// A chapter a `<chapterRef>` can name.
#[derive(Clone, Debug, PartialEq)]
pub struct ChapterInfo {
    pub name: String,
    pub title: String,
    /// What a chapterRef prints, or why the compiler refuses it.
    pub printed: Result<String, String>,
}

/// A figure a `<figureRef>` can name.
#[derive(Clone, Debug, PartialEq)]
pub struct FigureInfo {
    pub id: String,
    pub name: String,
    pub printed: Result<String, String>,
}

/// Everything a paragraph's refs resolve against.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RefCtx {
    /// The file's `<reference>` entries as (kind, id), in file order.
    pub entries: Vec<(String, String)>,
    /// The entries' printed values; None until the backend has sent them.
    pub index: Option<RefIndex>,
    pub chapters: Vec<ChapterInfo>,
    pub figures: Vec<FigureInfo>,
}

/// What a ref tag prints and what's wrong with it.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    /// The text as printed; the id when it can't be worked out.
    pub text: String,
    /// Printed text was found (not just the id).
    pub known: bool,
    pub style: Style,
    pub problems: Vec<String>,
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start).and_then(|t| t.value(name))
}

fn kids<'a>(nodes: &'a [StructNode], n: &'a StructNode, tag: &'a str) -> impl Iterator<Item = &'a StructNode> + 'a {
    n.children.iter().map(move |c| &nodes[*c]).filter(move |c| c.tag == tag)
}

impl RefCtx {
    /// Chapters, figures and entries from the open file; `index` from the backend.
    pub fn from_doc(source: &str, nodes: &[StructNode], index: Option<RefIndex>) -> Self {
        let Some(root) = nodes.first() else { return RefCtx { index, ..Default::default() } };
        let entries = kids(nodes, root, "reference")
            .flat_map(|r| r.children.iter().map(|c| &nodes[*c]))
            .filter_map(|e| attr(source, e, "id").map(|id| (e.tag.clone(), id)))
            .collect();
        let order: Vec<String> = kids(nodes, root, "chapterOrder")
            .flat_map(|o| kids(nodes, o, "ch"))
            .filter_map(|c| attr(source, c, "name"))
            .collect();
        let chapter_nodes: Vec<&StructNode> = kids(nodes, root, "chapter").collect();
        let find = |name: &str| chapter_nodes.iter().copied().find(|c| attr(source, c, "name").as_deref() == Some(name));
        let kind_of = |c: &StructNode| attr(source, c, "kind").unwrap_or_else(|| "chapter".into());

        // Numbers as the compiler gives them: 1, 2, ... then A, B, ...; none for the rest.
        let mut number: HashMap<String, Option<String>> = HashMap::new();
        let (mut n, mut letter) = (0, b'A');
        for name in &order {
            let Some(c) = find(name) else { continue };
            let num = match kind_of(c).as_str() {
                "chapter" => {
                    n += 1;
                    Some(n.to_string())
                }
                "appendix" => {
                    let l = (letter as char).to_string();
                    letter = letter.saturating_add(1);
                    Some(l)
                }
                _ => None,
            };
            number.entry(name.clone()).or_insert(num);
        }

        let mut chapters = Vec::new();
        let mut figures = Vec::new();
        for c in &chapter_nodes {
            let Some(name) = attr(source, c, "name") else { continue };
            let title = kids(nodes, c, "chapterTitle").next().map(|t| edit::plain_text(source, t)).unwrap_or_default();
            let kind = kind_of(c);
            let printed = match number.get(&name) {
                None => Err(format!(
                    "chapter \"{name}\" isn't in <chapterOrder>, so it has no number; add <ch name=\"{name}\"/>"
                )),
                Some(None) => Ok(title.clone()),
                Some(Some(num)) => Ok(format!("{} {num}: {title}", if kind == "appendix" { "Appendix" } else { "Chapter" })),
            };
            let figs = kids(nodes, c, "section").flat_map(|s| kids(nodes, s, "figure"));
            for (i, f) in figs.enumerate() {
                let Some(id) = attr(source, f, "id") else { continue };
                let fname = attr(source, f, "name").unwrap_or_default();
                let printed = match number.get(&name) {
                    None => Err(format!("figure \"{id}\" is in chapter \"{name}\", which isn't in <chapterOrder>")),
                    Some(None) => Err(format!("figure \"{id}\" is in unnumbered chapter \"{name}\", so it has no number")),
                    Some(Some(num)) => {
                        let base = format!("Figure {num}.{:02}", i + 1);
                        Ok(if fname.is_empty() { base } else { format!("{base}: {fname}") })
                    }
                };
                figures.push(FigureInfo { id, name: fname, printed });
            }
            chapters.push(ChapterInfo { name, title, printed });
        }
        RefCtx { entries, index, chapters, figures }
    }

    /// Ids of the entries of one kind.
    pub fn ids(&self, kind: &str) -> Vec<String> {
        self.entries.iter().filter(|(k, _)| k == kind).map(|(_, id)| id.clone()).collect()
    }

    /// An entry's printed value: None while unknown, Err when the compiler can't print it.
    pub fn value(&self, id: &str, value: &str) -> Option<Result<String, String>> {
        let e = self.index.as_ref()?.entries.get(id)?;
        if let Some(v) = e.values.get(value) {
            return Some(Ok(v.clone()));
        }
        e.errors.get(value).map(|m| Err(m.clone()))
    }

    /// The entry's name for lists: displayName, else name, else None.
    pub fn entry_label(&self, id: &str) -> Option<String> {
        let e = self.index.as_ref()?.entries.get(id)?;
        let v = e.values.get("displayName").or_else(|| e.values.get("name")).or_else(|| e.values.get("value"))?;
        Some(v.clone())
    }

    /// Checks `id` names an entry of `kind`; a problem message if not.
    fn check_entry(&self, tag: &str, kind: &str, id: Option<&str>) -> Option<String> {
        let Some(id) = id.filter(|i| !i.is_empty()) else { return Some(format!("<{tag}> needs ref=\"…\"")) };
        if self.entries.iter().any(|(k, i)| k == kind && i == id) {
            return None;
        }
        Some(match self.entries.iter().find(|(_, i)| i == id) {
            Some((k, _)) => format!("\"{id}\" is a <{k}> entry, not a <{kind}>"),
            None => format!("no <reference><{kind} id=\"{id}\"> in this file"),
        })
    }

    pub fn resolve(&self, t: &RefTag) -> Resolved {
        let Some(spec) = t.spec() else {
            return Resolved { text: t.tag.clone(), known: false, style: Style::Plain, problems: vec![] };
        };
        let mut problems = Vec::new();
        let mut style = spec.style;
        let id = t.get("ref").unwrap_or_default().to_string();
        let mut known = false;
        let text = match spec.tag {
            "chapterRef" => {
                let name = t.get("name").unwrap_or_default();
                match self.chapters.iter().find(|c| c.name == name) {
                    _ if name.is_empty() => {
                        problems.push("<chapterRef> needs name=\"…\"".into());
                        "chapter?".into()
                    }
                    None => {
                        problems.push(format!("no chapter named \"{name}\""));
                        name.to_string()
                    }
                    Some(c) => match &c.printed {
                        Ok(s) => {
                            known = true;
                            s.clone()
                        }
                        Err(m) => {
                            problems.push(m.clone());
                            c.title.clone()
                        }
                    },
                }
            }
            "figureRef" => match self.figures.iter().find(|f| f.id == id) {
                _ if id.is_empty() => {
                    problems.push("<figureRef> needs ref=\"…\"".into());
                    "figure?".into()
                }
                None => {
                    problems.push(format!("no <figure id=\"{id}\"> in a chapter's section"));
                    id.clone()
                }
                Some(f) => match &f.printed {
                    Ok(s) => {
                        known = true;
                        s.clone()
                    }
                    Err(m) => {
                        problems.push(m.clone());
                        id.clone()
                    }
                },
            },
            "coreBookRef" => {
                known = true;
                t.text.clone()
            }
            "coreBookChapterRef" => {
                known = true;
                let num = t.get("num").unwrap_or_default();
                if num.is_empty() {
                    problems.push("<coreBookChapterRef> needs num=\"…\"".into());
                }
                format!("Chapter {num}: {}", t.text)
            }
            "coreBookSectionRef" => {
                known = true;
                format!("{} section", t.text)
            }
            "coreBookTableRef" => {
                known = true;
                format!("{} table", t.text)
            }
            "monster" => self.inline_monster(t, &mut problems, &mut known),
            _ => {
                let kind = spec.kind.unwrap_or_default();
                let value = t.get("value").unwrap_or_default();
                if let Some(p) = self.check_entry(spec.tag, kind, Some(&id)) {
                    problems.push(p);
                }
                if value.is_empty() {
                    problems.push(format!("<{}> needs value=\"…\" ({})", spec.tag, spec.values.join(", ")));
                } else if !spec.values.contains(&value) {
                    problems.push(format!("value=\"{value}\" isn't one of {}", spec.values.join(", ")));
                }
                if matches!(value, "cr" | "src") {
                    style = Style::Plain;
                }
                match self.value(&id, value) {
                    Some(Ok(s)) if problems.is_empty() => {
                        known = true;
                        s
                    }
                    Some(Err(m)) if problems.is_empty() => {
                        problems.push(format!("can't print {value} of \"{id}\": {m}"));
                        id.clone()
                    }
                    _ => id.clone(),
                }
            }
        };
        match t.get("emphasized") {
            Some("true") => style = if spec.style == Style::Plain { Style::Creature } else { spec.style },
            Some("false") => style = Style::Plain,
            _ => {}
        }
        Resolved { text, known, style, problems }
    }

    fn inline_monster(&self, t: &RefTag, problems: &mut Vec<String>, known: &mut bool) -> String {
        let (lead, name, cr, src);
        if let Some(id) = t.get("ref").filter(|r| !r.is_empty()) {
            if let Some(p) = self.check_entry("monster", "monster", Some(id)) {
                problems.push(p);
                return id.to_string();
            }
            let mut v = |k: &str| match self.value(id, k) {
                Some(Ok(s)) => Some(s),
                Some(Err(m)) => {
                    problems.push(format!("can't print {k} of \"{id}\": {m}"));
                    None
                }
                None => None,
            };
            let (n, c, s, d) = (v("name"), v("cr"), v("src"), v("displayName"));
            let Some(n) = n else { return id.to_string() };
            name = n;
            cr = c.unwrap_or_default();
            src = s.unwrap_or_default();
            lead = t.get("display").map(str::to_string).or(d).unwrap_or_else(|| name.clone());
        } else {
            let Some(n) = t.get("name").filter(|n| !n.is_empty()) else {
                problems.push("an inline <monster> needs ref=\"…\" or name=\"…\"".into());
                return "monster?".into();
            };
            name = n.to_string();
            cr = t.get("cr").unwrap_or_default().to_string();
            src = t.get("src").unwrap_or_default().to_string();
            lead = t.get("display").unwrap_or(n).to_string();
        }
        *known = true;
        let xp = XP.iter().find(|(c, _)| *c == cr).map(|(_, x)| *x).unwrap_or("?");
        let mut inner: Vec<String> = Vec::new();
        if lead != name {
            inner.push(name);
        }
        inner.push(format!("CR {cr}"));
        inner.push(format!("{xp} XP"));
        if !src.is_empty() {
            inner.push(src);
        }
        format!("{lead} ({})", inner.join(", "))
    }
}

impl Checker for RefCtx {
    fn check(&self, _text: &str, pieces: &[Piece]) -> Vec<Diagnostic> {
        refs(pieces)
            .flat_map(|t| {
                self.resolve(t).problems.into_iter().map(|m| Diagnostic {
                    start: t.start,
                    end: t.end,
                    severity: Severity::Error,
                    source: "refs",
                    message: m,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    #[test]
    fn parses_text_and_refs_with_spans() {
        let t = r#"A <b>big</b> <monsterRef ref="krugga" value="name"/> &amp; <coreBookRef>DMG</coreBookRef>."#;
        let p = parse(t).unwrap();
        let r: Vec<&RefTag> = refs(&p).collect();
        assert_eq!(r.len(), 2);
        assert_eq!(&t[r[0].start..r[0].end], r#"<monsterRef ref="krugga" value="name"/>"#);
        assert_eq!(&t[r[1].start..r[1].end], "<coreBookRef>DMG</coreBookRef>");
        assert_eq!(r[1].text, "DMG");
        assert!(p.iter().any(|x| matches!(x, Piece::Text { text, bold: true, .. } if text == "big")));
        assert!(p.iter().any(|x| matches!(x, Piece::Text { text, .. } if text == " & ")));
    }

    #[test]
    fn reports_bad_xml() {
        assert!(parse("a <b>b").is_err());
        assert!(parse("a </b> c").is_err());
        assert!(parse("a & b").is_err());
        assert!(parse("").unwrap().is_empty());
    }

    #[test]
    fn round_trips_tags() {
        let t = r#"<itemRef ref="bag" value="name" emphasized="false"/>"#;
        let p = parse(t).unwrap();
        let mut r = refs(&p).next().unwrap().clone();
        assert_eq!(r.to_xml(), t);
        r.set("emphasized", None);
        r.set("value", Some("src"));
        assert_eq!(r.to_xml(), r#"<itemRef ref="bag" value="src"/>"#);
        let mut c = RefTag::new("coreBookRef");
        c.text = "Player's Handbook & more".into();
        assert_eq!(c.to_xml(), "<coreBookRef>Player's Handbook &amp; more</coreBookRef>");
    }

    const DOC: &str = r#"<supplement>
  <chapterOrder><ch name="intro"/><ch name="app"/></chapterOrder>
  <chapter name="intro"><chapterTitle>Introduction</chapterTitle>
    <section name="s"><figure id="fig-a" name="A Map"><image ref="x"/></figure><p>x</p></section>
  </chapter>
  <chapter name="app" kind="appendix"><chapterTitle>Monsters</chapterTitle></chapter>
  <chapter name="loose"><chapterTitle>Loose</chapterTitle></chapter>
  <reference>
    <monster id="krugga" displayName="Krugga"/>
    <noun id="garrick" name="Garrick"/>
  </reference>
</supplement>"#;

    fn ctx() -> RefCtx {
        let parsed = xml::parse(DOC).unwrap();
        let mut index = RefIndex::default();
        let mut e = IndexEntry { kind: "monster".into(), ..Default::default() };
        for (k, v) in [("name", "Gnoll Warrior"), ("displayName", "Krugga"), ("cr", "1/2"), ("src", "5.2.1 SRD")] {
            e.values.insert(k.into(), v.into());
        }
        index.entries.insert("krugga".into(), e);
        RefCtx::from_doc(DOC, &parsed.nodes, Some(index))
    }

    fn one(c: &RefCtx, xml: &str) -> Resolved {
        let p = parse(xml).unwrap();
        let r = c.resolve(refs(&p).next().unwrap());
        r
    }

    #[test]
    fn resolves_like_the_compiler() {
        let c = ctx();
        assert_eq!(one(&c, r#"<monsterRef ref="krugga" value="displayName"/>"#).text, "Krugga");
        assert_eq!(one(&c, r#"<chapterRef name="intro"/>"#).text, "Chapter 1: Introduction");
        assert_eq!(one(&c, r#"<chapterRef name="app"/>"#).text, "Appendix A: Monsters");
        assert_eq!(one(&c, r#"<figureRef ref="fig-a"/>"#).text, "Figure 1.01: A Map");
        assert_eq!(
            one(&c, r#"<monster ref="krugga"/>"#).text,
            "Krugga (Gnoll Warrior, CR 1/2, 100 XP, 5.2.1 SRD)"
        );
        assert_eq!(one(&c, r#"<coreBookChapterRef num="4">Creating Adventures</coreBookChapterRef>"#).text, "Chapter 4: Creating Adventures");
    }

    #[test]
    fn finds_broken_refs() {
        let c = ctx();
        assert!(!one(&c, r#"<monsterRef ref="krug" value="name"/>"#).problems.is_empty());
        let wrong_kind = one(&c, r#"<monsterRef ref="garrick" value="name"/>"#);
        assert!(wrong_kind.problems[0].contains("<noun>"), "{:?}", wrong_kind.problems);
        assert!(!one(&c, r#"<monsterRef ref="krugga" value="nickname"/>"#).problems.is_empty());
        assert!(!one(&c, r#"<chapterRef name="loose"/>"#).problems.is_empty());
        assert!(!one(&c, r#"<figureRef ref="nope"/>"#).problems.is_empty());
        // Known entry, values not loaded yet: shows the id, no complaint.
        let r = one(&c, r#"<nounRef ref="garrick" value="name"/>"#);
        assert!(r.problems.is_empty() && !r.known && r.text == "garrick");
        let p = parse(r#"x <spellRef ref="zzz" value="name"/>"#).unwrap();
        let d = c.check("", &p);
        assert_eq!((d.len(), d[0].start, d[0].source), (1, 2, "refs"));
    }
}
