//! The family in a monster file: its header (`<name>`, `<shortNoun>`, then an optional
//! `<brief>` and `<habitats>`, once each) and its parts, which follow the header in any
//! order and any number, before the first `<monster>`: `<quoteBox>`, `<image>`,
//! `<description>`, `<distinctionTable>` and `<lair>`. An entry prints the parts in the
//! order written.
//!
//! A part is found by its index among the root's children and its tag (the tree's own
//! path for it), checked on every edit. Plans read the file with [`parse_full`], since
//! the monster tree leaves the parts closed.

use serde::{Deserialize, Serialize};

use crate::block;
use crate::edit::{self, escape_attr, escape_text, Anchor, Splice};
use crate::xml::parse_full;
use crate::StructNode;

/// The header's elements, in schema order.
pub const HEADER: &[&str] = &["name", "shortNoun", "brief", "habitats"];
/// The family's parts.
pub const PARTS: &[&str] = &["quoteBox", "image", "description", "distinctionTable", "lair"];

pub fn part_label(tag: &str) -> &'static str {
    match tag {
        "quoteBox" => "Quote box",
        "image" => "Image",
        "description" => "Description",
        "distinctionTable" => "Distinction table",
        "lair" => "Lair",
        _ => "Part",
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PartAt {
    /// Index among `<monsters>`' children.
    pub child: usize,
    pub tag: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HeaderModel {
    pub name: String,
    pub short_noun: String,
    /// `<shortNoun proper="true">`: a name, printed without "the".
    pub proper: bool,
    /// Empty (with an empty id) leaves the brief out.
    pub brief: String,
    pub brief_id: String,
    /// `<habitat name>`s and `<treasure name>`s; none (and no id) leaves `<habitats>` out.
    pub habitats: Vec<String>,
    pub treasure: Vec<String>,
    pub habitats_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageModel {
    pub src: String,
    pub alt: String,
    pub width: String,
    pub height: String,
    /// For `<imageCredits/>`: the piece's title and its artist.
    pub name: String,
    pub credit: String,
    /// The `<cutline>`'s contents as XML; empty leaves it out.
    pub cutline: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EffectView {
    pub id: String,
    pub name: String,
    /// Contents as XML.
    pub text: String,
}

/// One part, read for its form. Only the fields for its tag are filled.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PartView {
    pub child: usize,
    pub tag: String,
    pub id: String,
    /// One line for the parts list.
    pub summary: String,
    /// Paragraphs as XML: a quote's, a description's, a lair's description.
    pub paras: Vec<String>,
    /// quoteBox: `<attribution name>`.
    pub attribution: Option<String>,
    pub image: ImageModel,
    /// distinctionTable: the `<dh>`s (plain text) and each `<tr>`'s `<td>`s (XML).
    pub heads: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// lair: @name, `<effectEndVerb>`, whether it has a `<description>`, its effects, and
    /// the stat blocks (ids) that name it with `<lairRef>`.
    pub name: String,
    pub end_verb: String,
    pub has_description: bool,
    pub effects: Vec<EffectView>,
    pub named_by: Vec<String>,
}

impl PartView {
    pub fn at(&self) -> PartAt {
        PartAt { child: self.child, tag: self.tag.clone() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Family {
    pub header: HeaderModel,
    pub parts: Vec<PartView>,
    /// Root child index of the last header element: new parts with no `after` go after it.
    pub header_end: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum FamilyEdit {
    Header { model: HeaderModel },
    /// New parts, in order, after root child `after` (None: right after the header).
    AddParts { after: Option<usize>, tags: Vec<String> },
    /// Delete a part. A lair goes with the `<lairRef>`s that name it.
    DeletePart { at: PartAt },
    /// Move a part before the part at root child `before`, or after the last part.
    MovePart { at: PartAt, before: Option<usize> },
    /// Set or clear a part's id. A lair's `<lairRef>`s follow its new id.
    SetId { at: PartAt, id: String },
    SetPara { at: PartAt, n: usize, text: String },
    /// A new empty paragraph after paragraph `after`, or first.
    AddPara { at: PartAt, after: Option<usize> },
    DeletePara { at: PartAt, n: usize },
    MovePara { at: PartAt, n: usize, before: Option<usize> },
    /// A quote box's attribution; empty removes it.
    Attribution { at: PartAt, name: String },
    Image { at: PartAt, model: ImageModel },
    Table { at: PartAt, heads: Vec<String>, rows: Vec<Vec<String>> },
    /// A lair's @name and `<effectEndVerb>` (empty removes either).
    Lair { at: PartAt, name: String, end_verb: String },
    SetEffect { at: PartAt, n: usize, id: String, name: String, text: String },
    AddEffect { at: PartAt, after: Option<usize> },
    DeleteEffect { at: PartAt, n: usize },
    MoveEffect { at: PartAt, n: usize, before: Option<usize> },
}

impl FamilyEdit {
    pub fn at(&self) -> Option<&PartAt> {
        use FamilyEdit::*;
        match self {
            Header { .. } | AddParts { .. } => None,
            DeletePart { at }
            | MovePart { at, .. }
            | SetId { at, .. }
            | SetPara { at, .. }
            | AddPara { at, .. }
            | DeletePara { at, .. }
            | MovePara { at, .. }
            | Attribution { at, .. }
            | Image { at, .. }
            | Table { at, .. }
            | Lair { at, .. }
            | SetEffect { at, .. }
            | AddEffect { at, .. }
            | DeleteEffect { at, .. }
            | MoveEffect { at, .. } => Some(at),
        }
    }

    /// Does it add, remove or reorder parts (so root child indexes after it shift)?
    pub fn reshapes(&self) -> bool {
        matches!(self, FamilyEdit::AddParts { .. } | FamilyEdit::DeletePart { .. } | FamilyEdit::MovePart { .. })
    }
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

fn kids<'a>(nodes: &'a [StructNode], n: &StructNode, tag: &str) -> Vec<&'a StructNode> {
    n.children.iter().map(|c| &nodes[*c]).filter(|c| c.tag == tag).collect()
}

fn first<'a>(nodes: &'a [StructNode], n: &StructNode, tag: &str) -> Option<&'a StructNode> {
    n.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == tag)
}

fn some(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

fn nl(source: &str) -> &'static str {
    if source.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Replaces an element's children with `lines`, one per line, a level deeper than it.
fn set_children(source: &str, n: &StructNode, lines: &[String]) -> Option<Splice> {
    let nl = nl(source);
    let ind = edit::line_indent(source, n.start);
    let cind = format!("{ind}{}", edit::indent_unit(source));
    let body = lines.iter().map(|l| format!("{cind}{l}")).collect::<Vec<_>>().join(nl);
    let inner = if lines.is_empty() { String::new() } else { format!("{nl}{body}{nl}{ind}") };
    match edit::inner_span(source, n) {
        Some((a, b)) => Some(Splice { start: a, end: b, text: inner }),
        None => {
            let t = edit::start_tag(source, n.start)?;
            Some(Splice { start: t.close_at, end: t.end, text: format!(">{inner}</{}>", n.tag) })
        }
    }
}

fn snip(source: &str, n: &StructNode) -> String {
    edit::snippet(&edit::plain_text(source, n), 60)
}

/// Where the paragraphs of a part live: a quote box's `<quote>`, a description itself,
/// a lair's `<description>` (None when the lair has none yet).
fn para_host<'a>(nodes: &'a [StructNode], p: &'a StructNode) -> Option<&'a StructNode> {
    match p.tag.as_str() {
        "quoteBox" => first(nodes, p, "quote"),
        "description" => Some(p),
        "lair" => first(nodes, p, "description"),
        _ => None,
    }
}

fn read_part(source: &str, nodes: &[StructNode], child: usize) -> PartView {
    let n = &nodes[nodes[0].children[child]];
    let mut v = PartView { child, tag: n.tag.clone(), id: attr(source, n, "id").unwrap_or_default(), ..Default::default() };
    if let Some(h) = para_host(nodes, n) {
        v.paras = kids(nodes, h, "p").into_iter().map(|p| block::p_text(source, p)).collect();
    }
    let first_p = || para_host(nodes, n).and_then(|h| first(nodes, h, "p")).map(|p| snip(source, p)).unwrap_or_default();
    match n.tag.as_str() {
        "quoteBox" => {
            v.attribution = first(nodes, n, "attribution").map(|a| attr(source, a, "name").unwrap_or_default());
            v.summary = match &v.attribution {
                Some(a) if !a.is_empty() => format!("\u{201c}{}\u{201d} \u{2014} {a}", first_p()),
                _ => format!("\u{201c}{}\u{201d}", first_p()),
            };
        }
        "image" => {
            let a = |k: &str| attr(source, n, k).unwrap_or_default();
            v.image = ImageModel {
                src: a("src"),
                alt: a("alt"),
                width: a("width"),
                height: a("height"),
                name: a("name"),
                credit: a("credit"),
                cutline: first(nodes, n, "cutline").map(|c| block::p_text(source, c)).unwrap_or_default(),
            };
            v.summary = some(&v.image.name).or(some(&v.image.src)).unwrap_or("(no file)").to_string();
        }
        "description" => {
            v.summary = match v.paras.len() {
                0 | 1 => first_p(),
                k => format!("{} (+{} more)", first_p(), k - 1),
            };
        }
        "distinctionTable" => {
            if let Some(h) = first(nodes, n, "header") {
                v.heads = kids(nodes, h, "dh").into_iter().map(|d| edit::text_of(source, d)).collect();
            }
            if let Some(b) = first(nodes, n, "body") {
                v.rows = kids(nodes, b, "tr")
                    .into_iter()
                    .map(|r| kids(nodes, r, "td").into_iter().map(|d| block::p_text(source, d)).collect())
                    .collect();
            }
            let title = v.heads.iter().skip(1).find(|h| !h.trim().is_empty()).or(v.heads.first()).cloned().unwrap_or_default();
            v.summary = format!("{title} ({} rows)", v.rows.len());
        }
        "lair" => {
            v.name = attr(source, n, "name").unwrap_or_default();
            v.has_description = first(nodes, n, "description").is_some();
            v.end_verb = first(nodes, n, "effectEndVerb").map(|e| edit::text_of(source, e)).unwrap_or_default();
            if let Some(es) = first(nodes, n, "effects") {
                v.effects = kids(nodes, es, "effect")
                    .into_iter()
                    .map(|e| EffectView {
                        id: attr(source, e, "id").unwrap_or_default(),
                        name: attr(source, e, "name").unwrap_or_default(),
                        text: block::p_text(source, e),
                    })
                    .collect();
            }
            if !v.id.is_empty() {
                v.named_by = lair_refs(source, nodes, &v.id)
                    .into_iter()
                    .filter_map(|(m, _)| attr(source, &nodes[m], "id"))
                    .fold(Vec::new(), |mut acc, id| {
                        if !acc.contains(&id) {
                            acc.push(id);
                        }
                        acc
                    });
            }
            let label = some(&v.name).or(some(&v.id)).unwrap_or("(unnamed lair)").to_string();
            v.summary = format!("{label} ({} effect{})", v.effects.len(), if v.effects.len() == 1 { "" } else { "s" });
        }
        _ => {}
    }
    v
}

/// (monster node, lairRef node) for each `<lairRef ref=id>`.
fn lair_refs(source: &str, nodes: &[StructNode], id: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for m in nodes[0].children.iter().copied().filter(|m| nodes[*m].tag == "monster") {
        for r in nodes[m].children.iter().copied().filter(|r| nodes[*r].tag == "lairRef") {
            if attr(source, &nodes[r], "ref").as_deref() == Some(id) {
                out.push((m, r));
            }
        }
    }
    out
}

/// The family's header and parts; None when the file isn't a well-formed `<monsters>`.
pub fn read(source: &str) -> Option<Family> {
    let nodes = parse_full(source).ok()?.nodes;
    let root = nodes.first().filter(|r| r.tag == "monsters")?;
    let text = |tag: &str| first(&nodes, root, tag).map(|n| edit::text_of(source, n)).unwrap_or_default();
    let mut header = HeaderModel { name: text("name"), short_noun: text("shortNoun"), brief: text("brief"), ..Default::default() };
    if let Some(sn) = first(&nodes, root, "shortNoun") {
        header.proper = attr(source, sn, "proper").as_deref() == Some("true");
    }
    if let Some(b) = first(&nodes, root, "brief") {
        header.brief_id = attr(source, b, "id").unwrap_or_default();
    }
    if let Some(h) = first(&nodes, root, "habitats") {
        let names = |tag: &str| kids(&nodes, h, tag).into_iter().map(|k| attr(source, k, "name").unwrap_or_default()).collect();
        header.habitats = names("habitat");
        header.treasure = names("treasure");
        header.habitats_id = attr(source, h, "id").unwrap_or_default();
    }
    let header_end = root.children.iter().rposition(|c| HEADER.contains(&nodes[*c].tag.as_str())).unwrap_or(0);
    let parts = (0..root.children.len()).filter(|i| PARTS.contains(&nodes[root.children[*i]].tag.as_str())).map(|i| read_part(source, &nodes, i)).collect();
    Some(Family { header, parts, header_end })
}

/// Every part id and lair id in the file (one namespace in the schema), and every
/// lair effect id.
fn ids(source: &str, nodes: &[StructNode]) -> (Vec<String>, Vec<String>) {
    let root = &nodes[0];
    let mut parts = Vec::new();
    let mut effects = Vec::new();
    let mut take = |n: &StructNode| {
        if PARTS.contains(&n.tag.as_str()) || n.tag == "brief" || n.tag == "habitats" {
            if let Some(i) = attr(source, n, "id") {
                parts.push(i);
            }
        }
    };
    for c in &root.children {
        let n = &nodes[*c];
        take(n);
        if n.tag == "monster" {
            for k in &n.children {
                take(&nodes[*k]);
            }
        }
        if n.tag == "lair" {
            if let Some(es) = first(nodes, n, "effects") {
                effects.extend(kids(nodes, es, "effect").into_iter().filter_map(|e| attr(source, e, "id")));
            }
        }
    }
    (parts, effects)
}

fn fresh(base: &str, taken: &[String]) -> String {
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..).map(|k| format!("{base}-{k}")).find(|c| !taken.contains(c)).unwrap()
}

/// A new, schema-valid part; lines after the first are indented relative to it.
fn new_part_xml(source: &str, nodes: &[StructNode], tag: &str) -> Result<String, String> {
    let u = edit::indent_unit(source);
    Ok(match tag {
        "quoteBox" => format!("<quoteBox>\n{u}<quote>\n{u}{u}<p></p>\n{u}</quote>\n</quoteBox>"),
        "image" => "<image src=\"\"/>".to_string(),
        "description" => format!("<description>\n{u}<p></p>\n</description>"),
        "distinctionTable" => {
            let rows: String = (1..=4).map(|i| format!("\n{u}{u}<tr><td>{i}</td><td></td></tr>")).collect();
            format!("<distinctionTable>\n{u}<header>\n{u}{u}<dh>1d4</dh>\n{u}{u}<dh></dh>\n{u}</header>\n{u}<body>{rows}\n{u}</body>\n</distinctionTable>")
        }
        "lair" => {
            let (parts, effects) = ids(source, nodes);
            let id = fresh("lair", &parts);
            let eid = fresh("effect", &effects);
            format!("<lair id=\"{id}\">\n{u}<effects>\n{u}{u}<effect id=\"{eid}\" name=\"New Effect\"></effect>\n{u}</effects>\n</lair>")
        }
        other => return Err(format!("<{other}> isn't one of the family's parts")),
    })
}

fn find<'a>(nodes: &'a [StructNode], at: &PartAt) -> Result<&'a StructNode, String> {
    nodes[0]
        .children
        .get(at.child)
        .map(|c| &nodes[*c])
        .filter(|n| n.tag == at.tag)
        .ok_or_else(|| format!("that {} isn't where it was any more; reopen the file", part_label(&at.tag).to_lowercase()))
}

fn header_splices(source: &str, nodes: &[StructNode], m: &HeaderModel) -> Result<Vec<Splice>, String> {
    let root = &nodes[0];
    let name = first(nodes, root, "name").ok_or("the file has no <name>")?;
    let sn = first(nodes, root, "shortNoun").ok_or("the file has no <shortNoun>")?;
    if m.name.trim().is_empty() {
        return Err("the family needs a name".into());
    }
    if m.short_noun.trim().is_empty() {
        return Err("the family needs a short noun (what <shortNounRef/> prints)".into());
    }
    let mut out: Vec<Splice> = Vec::new();
    out.extend(edit::set_text(source, name, m.name.trim()));
    let proper = attr(source, sn, "proper").as_deref() == Some("true");
    if proper != m.proper {
        out.extend(edit::set_attr(source, sn, "proper", m.proper.then_some("true")));
    }
    out.extend(edit::set_text(source, sn, m.short_noun.trim()));

    // New elements go after the last header element before them.
    let brief = first(nodes, root, "brief");
    let want_brief = some(&m.brief).is_some() || some(&m.brief_id).is_some();
    let id_attr = |id: &str| some(id).map(|i| format!(" id=\"{}\"", escape_attr(i, '"'))).unwrap_or_default();
    let mut new_xml: Vec<String> = Vec::new();
    match (brief, want_brief) {
        (Some(b), false) => out.push(edit::remove_element(source, b)),
        (Some(b), true) => {
            out.extend(edit::set_attr(source, b, "id", some(&m.brief_id)));
            out.extend(edit::set_text(source, b, m.brief.trim()));
        }
        (None, true) => new_xml.push(format!("<brief{}>{}</brief>", id_attr(&m.brief_id), escape_text(m.brief.trim()))),
        (None, false) => {}
    }
    let clean = |v: &[String]| v.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect::<Vec<_>>();
    let (habs, tres) = (clean(&m.habitats), clean(&m.treasure));
    let lines: Vec<String> = habs
        .iter()
        .map(|h| format!("<habitat name=\"{}\"/>", escape_attr(h, '"')))
        .chain(tres.iter().map(|t| format!("<treasure name=\"{}\"/>", escape_attr(t, '"'))))
        .collect();
    let want_hab = !lines.is_empty() || some(&m.habitats_id).is_some();
    match (first(nodes, root, "habitats"), want_hab) {
        (Some(h), false) => out.push(edit::remove_element(source, h)),
        (Some(h), true) => {
            out.extend(edit::set_attr(source, h, "id", some(&m.habitats_id)));
            let names = |tag: &str| kids(nodes, h, tag).into_iter().map(|k| attr(source, k, "name").unwrap_or_default()).collect::<Vec<_>>();
            if names("habitat") != habs || names("treasure") != tres {
                out.extend(set_children(source, h, &lines));
            }
        }
        (None, true) => {
            let u = edit::indent_unit(source);
            let body: String = lines.iter().map(|l| format!("\n{u}{l}")).collect();
            new_xml.push(if lines.is_empty() {
                format!("<habitats{}/>", id_attr(&m.habitats_id))
            } else {
                format!("<habitats{}>{body}\n</habitats>", id_attr(&m.habitats_id))
            });
        }
        (None, false) => {}
    }
    if !new_xml.is_empty() {
        // After the brief when only the habitats are new and the brief stays.
        let anchor = match brief {
            Some(b) if want_brief => b,
            _ => sn,
        };
        out.extend(edit::insert_child(source, Anchor::After(anchor), &new_xml.join("\n")));
    }
    Ok(out)
}

fn move_among(source: &str, list: &[&StructNode], n: usize, before: Option<usize>) -> Result<Option<Vec<Splice>>, String> {
    if before == Some(n) || before == Some(n + 1) || (before.is_none() && n + 1 == list.len()) {
        return Ok(None);
    }
    let node = list.get(n).ok_or("that item isn't there any more")?;
    let target = match before {
        Some(b) => Some(*list.get(b).ok_or("the place to move it to isn't there any more")?),
        None => None,
    };
    edit::move_element(source, node, target, list.last().copied()).map(Some)
}

pub fn plan(source: &str, e: &FamilyEdit) -> Result<String, String> {
    let nodes = parse_full(source).map_err(|e| e.message)?.nodes;
    if nodes.first().map(|r| r.tag.as_str()) != Some("monsters") {
        return Err("this isn't a monster file".into());
    }
    let root = &nodes[0];
    let part = |at: &PartAt| find(&nodes, at);
    let splices: Vec<Splice> = match e {
        FamilyEdit::Header { model } => header_splices(source, &nodes, model)?,
        FamilyEdit::AddParts { after, tags } => {
            // One at a time, each after the last.
            let header_end = root.children.iter().rposition(|c| HEADER.contains(&nodes[*c].tag.as_str())).ok_or("the file has no header")?;
            let mut at = after.unwrap_or(header_end);
            let mut cur = source.to_string();
            for tag in tags {
                let ns = parse_full(&cur).map_err(|e| e.message)?.nodes;
                let anchor = ns[0].children.get(at).map(|c| &ns[*c]).ok_or("can't find where to add it")?;
                if anchor.tag == "monster" {
                    return Err("the family's parts go before the stat blocks".into());
                }
                let xml = new_part_xml(&cur, &ns, tag)?;
                let s = edit::insert_child(&cur, Anchor::After(anchor), &xml).ok_or("can't place the new part")?;
                cur = edit::apply(&cur, &[s])?;
                at += 1;
            }
            return Ok(cur);
        }
        FamilyEdit::DeletePart { at } => {
            let p = part(at)?;
            let mut out = vec![edit::remove_element(source, p)];
            if p.tag == "lair" {
                if let Some(id) = attr(source, p, "id") {
                    out.extend(lair_refs(source, &nodes, &id).into_iter().map(|(_, r)| edit::remove_element(source, &nodes[r])));
                }
            }
            out
        }
        FamilyEdit::MovePart { at, before } => {
            let p = part(at)?;
            let list: Vec<&StructNode> = root.children.iter().map(|c| &nodes[*c]).filter(|n| PARTS.contains(&n.tag.as_str())).collect();
            let pos = |child: usize| list.iter().position(|n| std::ptr::eq(*n, &nodes[root.children[child]]));
            let n = list.iter().position(|x| std::ptr::eq(*x, p)).ok_or("that part isn't there any more")?;
            let b = match before {
                Some(c) => Some(root.children.get(*c).and(pos(*c)).ok_or("the place to move it to isn't a part")?),
                None => None,
            };
            match move_among(source, &list, n, b)? {
                Some(s) => s,
                None => return Ok(source.to_string()),
            }
        }
        FamilyEdit::SetId { at, id } => {
            let p = part(at)?;
            let old = attr(source, p, "id").unwrap_or_default();
            let new = id.trim();
            if new == old {
                return Ok(source.to_string());
            }
            if !new.is_empty() && new.contains(char::is_whitespace) {
                return Err("an id can't contain spaces".into());
            }
            let (taken, _) = ids(source, &nodes);
            if !new.is_empty() && taken.iter().any(|t| t == new) {
                return Err(format!("another part or lair already has the id \"{new}\""));
            }
            let refs = if p.tag == "lair" && !old.is_empty() { lair_refs(source, &nodes, &old) } else { vec![] };
            if new.is_empty() && !refs.is_empty() {
                return Err("stat blocks name this lair with <lairRef>, so it needs an id".into());
            }
            let mut out: Vec<Splice> = edit::set_attr(source, p, "id", some(new)).into_iter().collect();
            out.extend(refs.into_iter().filter_map(|(_, r)| edit::set_attr(source, &nodes[r], "ref", Some(new))));
            out
        }
        FamilyEdit::SetPara { at, n, text } => {
            let p = part(at)?;
            let host = para_host(&nodes, p).ok_or("there are no paragraphs there")?;
            let ps = kids(&nodes, host, "p");
            vec![block::set_inner(source, ps.get(*n).ok_or("that paragraph isn't there any more")?, text).ok_or("bad <p>")?]
        }
        FamilyEdit::AddPara { at, after } => {
            let p = part(at)?;
            match para_host(&nodes, p) {
                Some(host) => {
                    let ps = kids(&nodes, host, "p");
                    let anchor = match (after, ps.first()) {
                        (Some(a), _) => Anchor::After(ps.get(*a).ok_or("that paragraph isn't there any more")?),
                        (None, Some(f)) => Anchor::Before(f),
                        (None, None) => Anchor::Into(host),
                    };
                    vec![edit::insert_child(source, anchor, "<p></p>").ok_or("can't place the paragraph")?]
                }
                // A lair without a description: its description comes first.
                None if p.tag == "lair" => {
                    let u = edit::indent_unit(source);
                    let xml = format!("<description>\n{u}<p></p>\n</description>");
                    let anchor = match p.children.first() {
                        Some(f) => Anchor::Before(&nodes[*f]),
                        None => Anchor::Into(p),
                    };
                    vec![edit::insert_child(source, anchor, &xml).ok_or("can't place the description")?]
                }
                None => return Err("there are no paragraphs there".into()),
            }
        }
        FamilyEdit::DeletePara { at, n } => {
            let p = part(at)?;
            let host = para_host(&nodes, p).ok_or("there are no paragraphs there")?;
            let ps = kids(&nodes, host, "p");
            let para = ps.get(*n).ok_or("that paragraph isn't there any more")?;
            if ps.len() == 1 {
                match p.tag.as_str() {
                    "lair" => vec![edit::remove_element(source, host)],
                    "quoteBox" => return Err("a quote needs at least one paragraph; delete the quote box instead".into()),
                    _ => return Err("a description needs at least one paragraph; delete the description instead".into()),
                }
            } else {
                vec![edit::remove_element(source, para)]
            }
        }
        FamilyEdit::MovePara { at, n, before } => {
            let p = part(at)?;
            let host = para_host(&nodes, p).ok_or("there are no paragraphs there")?;
            match move_among(source, &kids(&nodes, host, "p"), *n, *before)? {
                Some(s) => s,
                None => return Ok(source.to_string()),
            }
        }
        FamilyEdit::Attribution { at, name } => {
            let p = part(at)?;
            match (first(&nodes, p, "attribution"), some(name)) {
                (Some(a), None) => vec![edit::remove_element(source, a)],
                (Some(a), Some(v)) => edit::set_attr(source, a, "name", Some(v)).into_iter().collect(),
                (None, Some(v)) => {
                    let q = first(&nodes, p, "quote").ok_or("the quote box has no <quote>")?;
                    let xml = format!("<attribution name=\"{}\"/>", escape_attr(v, '"'));
                    vec![edit::insert_child(source, Anchor::After(q), &xml).ok_or("can't place the attribution")?]
                }
                (None, None) => vec![],
            }
        }
        FamilyEdit::Image { at, model } => {
            let p = part(at)?;
            let mut out: Vec<Splice> = edit::set_attr(source, p, "src", Some(model.src.trim())).into_iter().collect();
            for (k, v) in [("alt", &model.alt), ("width", &model.width), ("height", &model.height), ("name", &model.name), ("credit", &model.credit)] {
                out.extend(edit::set_attr(source, p, k, some(v)));
            }
            let cur = edit::apply(source, &out)?;
            // The cutline second: opening up <image .../> would touch the attributes' tag.
            let ns = parse_full(&cur).map_err(|e| e.message)?.nodes;
            let img = find(&ns, at)?;
            let s = match (first(&ns, img, "cutline"), some(&model.cutline)) {
                (Some(c), None) => Some(edit::remove_element(&cur, c)),
                (Some(c), Some(t)) => (block::p_text(&cur, c) != t).then(|| block::set_inner(&cur, c, t)).flatten(),
                (None, Some(t)) => Some(edit::insert_child(&cur, Anchor::Into(img), &format!("<cutline>{t}</cutline>")).ok_or("can't place the cutline")?),
                (None, None) => None,
            };
            return edit::apply(&cur, &s.into_iter().collect::<Vec<_>>());
        }
        FamilyEdit::Table { at, heads, rows } => {
            let p = part(at)?;
            if heads.is_empty() || rows.is_empty() {
                return Err("a distinction table needs at least one column and one row".into());
            }
            if rows.iter().any(|r| r.len() != heads.len()) {
                return Err("every row needs a cell for each column".into());
            }
            let h = first(&nodes, p, "header").ok_or("the table has no <header>")?;
            let b = first(&nodes, p, "body").ok_or("the table has no <body>")?;
            let dhs = kids(&nodes, h, "dh");
            let trs = kids(&nodes, b, "tr");
            let tds: Vec<Vec<&StructNode>> = trs.iter().map(|r| kids(&nodes, r, "td")).collect();
            let same = dhs.len() == heads.len() && tds.len() == rows.len() && tds.iter().all(|r| r.len() == heads.len());
            if same {
                // Cell by cell, so nothing else in the table moves.
                let mut out: Vec<Splice> = dhs.iter().zip(heads).filter_map(|(d, t)| edit::set_text(source, d, t.trim())).collect();
                for (r, row) in tds.iter().zip(rows) {
                    for (d, t) in r.iter().zip(row) {
                        if block::p_text(source, d) != t.trim() {
                            out.extend(block::set_inner(source, d, t));
                        }
                    }
                }
                out
            } else {
                let hl: Vec<String> = heads.iter().map(|t| format!("<dh>{}</dh>", escape_text(t.trim()))).collect();
                let bl: Vec<String> = rows
                    .iter()
                    .map(|r| format!("<tr>{}</tr>", r.iter().map(|t| format!("<td>{}</td>", t.trim())).collect::<String>()))
                    .collect();
                let mut out: Vec<Splice> = set_children(source, h, &hl).into_iter().collect();
                out.extend(set_children(source, b, &bl));
                out
            }
        }
        FamilyEdit::Lair { at, name, end_verb } => {
            let p = part(at)?;
            let mut out: Vec<Splice> = edit::set_attr(source, p, "name", some(name)).into_iter().collect();
            match (first(&nodes, p, "effectEndVerb"), some(end_verb)) {
                (Some(e), None) => out.push(edit::remove_element(source, e)),
                (Some(e), Some(v)) => out.extend(edit::set_text(source, e, v)),
                (None, Some(v)) => {
                    let es = first(&nodes, p, "effects").ok_or("the lair has no <effects>")?;
                    let xml = format!("<effectEndVerb>{}</effectEndVerb>", escape_text(v));
                    out.push(edit::insert_child(source, Anchor::After(es), &xml).ok_or("can't place the closing line")?);
                }
                (None, None) => {}
            }
            out
        }
        FamilyEdit::SetEffect { at, n, id, name, text } => {
            let p = part(at)?;
            let es = first(&nodes, p, "effects").ok_or("the lair has no <effects>")?;
            let list = kids(&nodes, es, "effect");
            let e = list.get(*n).ok_or("that effect isn't there any more")?;
            let (id, name) = (id.trim(), name.trim());
            if id.is_empty() || name.is_empty() {
                return Err("an effect needs an id and a name".into());
            }
            let old = attr(source, e, "id").unwrap_or_default();
            let (_, taken) = ids(source, &nodes);
            if id != old && taken.iter().any(|t| t == id) {
                return Err(format!("another lair effect already has the id \"{id}\""));
            }
            let mut out: Vec<Splice> = edit::set_attr(source, e, "id", Some(id)).into_iter().collect();
            out.extend(edit::set_attr(source, e, "name", Some(name)));
            if block::p_text(source, e) != text.trim() {
                out.extend(block::set_inner(source, e, text));
            }
            out
        }
        FamilyEdit::AddEffect { at, after } => {
            let p = part(at)?;
            let es = first(&nodes, p, "effects").ok_or("the lair has no <effects>")?;
            let list = kids(&nodes, es, "effect");
            let (_, taken) = ids(source, &nodes);
            let xml = format!("<effect id=\"{}\" name=\"New Effect\"></effect>", fresh("effect", &taken));
            let anchor = match (after, list.first()) {
                (Some(a), _) => Anchor::After(list.get(*a).ok_or("that effect isn't there any more")?),
                (None, Some(f)) => Anchor::Before(f),
                (None, None) => Anchor::Into(es),
            };
            vec![edit::insert_child(source, anchor, &xml).ok_or("can't place the effect")?]
        }
        FamilyEdit::DeleteEffect { at, n } => {
            let p = part(at)?;
            let es = first(&nodes, p, "effects").ok_or("the lair has no <effects>")?;
            let list = kids(&nodes, es, "effect");
            if list.len() <= 1 {
                return Err("a lair needs at least one effect; delete the lair instead".into());
            }
            vec![edit::remove_element(source, list.get(*n).ok_or("that effect isn't there any more")?)]
        }
        FamilyEdit::MoveEffect { at, n, before } => {
            let p = part(at)?;
            let es = first(&nodes, p, "effects").ok_or("the lair has no <effects>")?;
            match move_among(source, &kids(&nodes, es, "effect"), *n, *before)? {
                Some(s) => s,
                None => return Ok(source.to_string()),
            }
        }
    };
    edit::apply(source, &splices)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<?xml version="1.0"?>
<monsters>
    <name>Gnolls</name>
    <shortNoun>gnoll</shortNoun>
    <!-- lore -->
    <description>
        <p>First.</p>
        <p>Second.</p>
    </description>
    <lair id="den" name="The Den">
        <effects>
            <effect id="stink" name="Stink">It reeks.</effect>
        </effects>
    </lair>
    <monster id="g">
        <name>Gnoll</name>
        <lairRef ref="den"/>
    </monster>
</monsters>
"#;

    fn at(child: usize, tag: &str) -> PartAt {
        PartAt { child, tag: tag.into() }
    }
    fn fam(s: &str) -> Family {
        read(s).unwrap()
    }

    #[test]
    fn reads_header_and_parts() {
        let f = fam(SRC);
        assert_eq!(f.header.name, "Gnolls");
        assert_eq!(f.header.short_noun, "gnoll");
        assert_eq!(f.header_end, 1);
        assert_eq!(f.parts.len(), 2);
        assert_eq!(f.parts[0].at(), at(2, "description"));
        assert_eq!(f.parts[0].paras, vec!["First.", "Second."]);
        assert_eq!(f.parts[1].named_by, vec!["g"]);
        assert_eq!(f.parts[1].effects[0].text, "It reeks.");
    }

    #[test]
    fn header_adds_brief_and_habitats_in_order() {
        let mut m = fam(SRC).header;
        m.brief = "Hyenas & worse".into();
        m.habitats = vec!["Hill".into(), " ".into(), "Forest".into()];
        m.treasure = vec!["Armaments".into()];
        m.proper = true;
        let out = plan(SRC, &FamilyEdit::Header { model: m.clone() }).unwrap();
        assert!(out.contains(
            "    <shortNoun proper=\"true\">gnoll</shortNoun>\n    <brief>Hyenas &amp; worse</brief>\n    <habitats>\n        <habitat name=\"Hill\"/>\n        <habitat name=\"Forest\"/>\n        <treasure name=\"Armaments\"/>\n    </habitats>\n    <!-- lore -->"
        ), "{out}");
        let f = fam(&out);
        assert_eq!(f.header.habitats, vec!["Hill", "Forest"]);
        // Habitats alone go after the brief; clearing removes them.
        let mut m2 = f.header.clone();
        m2.brief = String::new();
        m2.habitats.clear();
        m2.treasure.clear();
        let back = plan(&out, &FamilyEdit::Header { model: m2 }).unwrap();
        assert!(!back.contains("<brief") && !back.contains("<habitats"), "{back}");
        let mut m3 = fam(&out).header;
        m3.treasure = vec!["Any".into()];
        let changed = plan(&out, &FamilyEdit::Header { model: m3 }).unwrap();
        assert!(changed.contains("<treasure name=\"Any\"/>") && changed.contains("<brief>Hyenas"), "{changed}");
        assert!(plan(SRC, &FamilyEdit::Header { model: HeaderModel { short_noun: "x".into(), ..Default::default() } }).is_err());
    }

    #[test]
    fn adds_moves_and_deletes_parts() {
        let out = plan(SRC, &FamilyEdit::AddParts { after: None, tags: vec!["quoteBox".into(), "image".into()] }).unwrap();
        let f = fam(&out);
        let tags: Vec<&str> = f.parts.iter().map(|p| p.tag.as_str()).collect();
        assert_eq!(tags, ["quoteBox", "image", "description", "lair"]);
        assert!(out.contains("    <quoteBox>\n        <quote>\n            <p></p>\n        </quote>\n    </quoteBox>\n    <image src=\"\"/>\n"), "{out}");
        // Lair to the front.
        let moved = plan(&out, &FamilyEdit::MovePart { at: at(5, "lair"), before: Some(2) }).unwrap();
        let tags: Vec<String> = fam(&moved).parts.iter().map(|p| p.tag.clone()).collect();
        assert_eq!(tags, ["lair", "quoteBox", "image", "description"]);
        // A new lair gets a fresh id; deleting a named lair drops its lairRefs.
        let more = plan(SRC, &FamilyEdit::AddParts { after: Some(3), tags: vec!["lair".into()] }).unwrap();
        assert!(more.contains("<lair id=\"lair\">") && more.contains("<effect id=\"effect\" name=\"New Effect\">"), "{more}");
        let gone = plan(SRC, &FamilyEdit::DeletePart { at: at(3, "lair") }).unwrap();
        assert!(!gone.contains("lairRef") && !gone.contains("<lair"), "{gone}");
        assert!(plan(SRC, &FamilyEdit::DeletePart { at: at(3, "description") }).is_err());
    }

    #[test]
    fn lair_ids_carry_their_refs() {
        let out = plan(SRC, &FamilyEdit::SetId { at: at(3, "lair"), id: "kennel".into() }).unwrap();
        assert!(out.contains("<lair id=\"kennel\"") && out.contains("<lairRef ref=\"kennel\"/>"), "{out}");
        assert!(plan(SRC, &FamilyEdit::SetId { at: at(3, "lair"), id: String::new() }).is_err());
        let d = plan(SRC, &FamilyEdit::SetId { at: at(2, "description"), id: "den".into() });
        assert!(d.is_err(), "part and lair ids share a namespace");
    }

    #[test]
    fn paragraphs() {
        let a = at(2, "description");
        let out = plan(SRC, &FamilyEdit::SetPara { at: a.clone(), n: 1, text: "Two <i>b</i>.".into() }).unwrap();
        assert!(out.contains("<p>Two <i>b</i>.</p>"));
        let out = plan(&out, &FamilyEdit::MovePara { at: a.clone(), n: 1, before: Some(0) }).unwrap();
        assert_eq!(fam(&out).parts[0].paras, vec!["Two <i>b</i>.", "First."]);
        let out = plan(&out, &FamilyEdit::DeletePara { at: a.clone(), n: 0 }).unwrap();
        assert!(plan(&out, &FamilyEdit::DeletePara { at: a, n: 0 }).is_err());
        // A lair's first paragraph makes its description; its last takes it away.
        let l = at(3, "lair");
        let out = plan(SRC, &FamilyEdit::AddPara { at: l.clone(), after: None }).unwrap();
        assert!(out.contains("    <lair id=\"den\" name=\"The Den\">\n        <description>\n            <p></p>\n        </description>\n        <effects>"), "{out}");
        let back = plan(&out, &FamilyEdit::DeletePara { at: l, n: 0 }).unwrap();
        assert_eq!(back, SRC);
    }

    #[test]
    fn quote_image_table_lair() {
        let src = plan(SRC, &FamilyEdit::AddParts { after: None, tags: vec!["quoteBox".into(), "image".into(), "distinctionTable".into()] }).unwrap();
        let q = at(2, "quoteBox");
        let out = plan(&src, &FamilyEdit::Attribution { at: q.clone(), name: "Garrick".into() }).unwrap();
        assert!(out.contains("        </quote>\n        <attribution name=\"Garrick\"/>\n    </quoteBox>"), "{out}");
        assert_eq!(plan(&out, &FamilyEdit::Attribution { at: q, name: " ".into() }).unwrap(), src);

        let img = ImageModel { src: "art.png".into(), name: "Art".into(), cutline: "A <i>gnoll</i>.".into(), ..Default::default() };
        let out = plan(&src, &FamilyEdit::Image { at: at(3, "image"), model: img.clone() }).unwrap();
        assert!(out.contains("<image src=\"art.png\" name=\"Art\">\n        <cutline>A <i>gnoll</i>.</cutline>\n    </image>"), "{out}");
        assert_eq!(fam(&out).parts[1].image, img);
        let out2 = plan(&out, &FamilyEdit::Image { at: at(3, "image"), model: ImageModel { src: "art.png".into(), ..Default::default() } }).unwrap();
        assert!(out2.contains("<image src=\"art.png\">\n    </image>") || out2.contains("<image src=\"art.png\"></image>") || out2.contains("<image src=\"art.png\">"), "{out2}");

        let t = at(4, "distinctionTable");
        let v = fam(&src).parts[2].clone();
        assert_eq!(v.heads, vec!["1d4", ""]);
        let mut rows = v.rows.clone();
        rows[0][1] = "A <b>bold</b> quirk".into();
        let out = plan(&src, &FamilyEdit::Table { at: t.clone(), heads: vec!["1d4".into(), "Quirks".into()], rows: rows.clone() }).unwrap();
        assert!(out.contains("<dh>Quirks</dh>") && out.contains("<tr><td>1</td><td>A <b>bold</b> quirk</td></tr>"), "{out}");
        rows.truncate(2);
        let out = plan(&out, &FamilyEdit::Table { at: t, heads: vec!["1d2".into(), "Quirks".into()], rows }).unwrap();
        assert!(out.contains("        <body>\n            <tr><td>1</td><td>A <b>bold</b> quirk</td></tr>\n            <tr><td>2</td><td></td></tr>\n        </body>"), "{out}");

        let l = at(6, "lair");
        let out = plan(&src, &FamilyEdit::Lair { at: l.clone(), name: "".into(), end_verb: "dies".into() }).unwrap();
        assert!(out.contains("<lair id=\"den\">") && out.contains("        </effects>\n        <effectEndVerb>dies</effectEndVerb>\n    </lair>"), "{out}");
        let out = plan(&out, &FamilyEdit::AddEffect { at: l.clone(), after: Some(0) }).unwrap();
        assert!(out.contains("<effect id=\"effect\" name=\"New Effect\"></effect>"));
        let out = plan(&out, &FamilyEdit::SetEffect { at: l.clone(), n: 1, id: "howl".into(), name: "Howl".into(), text: "It howls.".into() }).unwrap();
        assert!(out.contains("<effect id=\"howl\" name=\"Howl\">It howls.</effect>"), "{out}");
        assert!(plan(&out, &FamilyEdit::SetEffect { at: l.clone(), n: 1, id: "stink".into(), name: "Howl".into(), text: "x".into() }).is_err());
        let out = plan(&out, &FamilyEdit::MoveEffect { at: l.clone(), n: 1, before: Some(0) }).unwrap();
        assert_eq!(fam(&out).parts[4].effects[0].id, "howl");
        let out = plan(&out, &FamilyEdit::DeleteEffect { at: l.clone(), n: 0 }).unwrap();
        assert!(plan(&out, &FamilyEdit::DeleteEffect { at: l, n: 0 }).is_err());
    }

    #[test]
    fn keeps_crlf() {
        let src = SRC.replace('\n', "\r\n");
        let out = plan(&src, &FamilyEdit::AddParts { after: None, tags: vec!["description".into()] }).unwrap();
        assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
    }
}
