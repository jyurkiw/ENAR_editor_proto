//! A stat block for its forms: a `<monster>` read as a tree of [`El`]s (each element's
//! attributes, its text and the elements in it), and edits that turn a changed tree
//! back into surgical splices of the file.
//!
//! The forms keep a card's element as an [`El`], change it, and send it back whole
//! ([`StatEdit::Update`]). The plan compares it with the file: attributes that differ
//! are set or removed, text that differs is replaced, children the El no longer has go,
//! and new ones (no `path`) are written where the schema puts them. Nothing else in the
//! file changes. Adding, deleting and moving items apply at once on their own.
//!
//! An element is found by its path of child indexes from the `<monster>` (in
//! [`parse_all`], which opens inline tags too), and its tag, checked on every edit.
//! Prose (a trait's description, an attack's hit, a save's failure, ...) is one text of
//! XML, inline tags and all; an `<action>`'s text is its prose between its head (uses,
//! cost, trigger) and its response.

use serde::{Deserialize, Serialize};

use crate::block;
use crate::edit::{self, escape_attr, escape_text, Anchor, Splice};
use crate::xml::parse_all;
use crate::StructNode;

pub const STATS: &[&str] = &["str", "dex", "con", "int", "wis", "cha"];
pub const SIZES: &[&str] = &["Tiny", "Small", "Medium", "Large", "Huge", "Gargantuan"];
pub const DAMAGE_TYPES: &[&str] = &[
    "acid", "bludgeoning", "cold", "fire", "force", "lightning", "necrotic", "piercing", "poison", "psychic", "radiant",
    "slashing", "thunder",
];
pub const CONDITIONS: &[&str] = &[
    "blinded", "charmed", "deafened", "exhaustion", "frightened", "grappled", "incapacitated", "invisible", "paralyzed",
    "petrified", "poisoned", "prone", "restrained", "stunned", "unconscious",
];
pub const SKILLS: &[&str] = &[
    "acrobatics", "animal handling", "arcana", "athletics", "deception", "history", "insight", "intimidation",
    "investigation", "medicine", "nature", "perception", "performance", "persuasion", "religion", "sleight of hand",
    "stealth", "survival",
];
/// The ability each skill uses by default.
pub fn skill_stat(skill: &str) -> &'static str {
    match skill {
        "athletics" => "str",
        "acrobatics" | "sleight of hand" | "stealth" => "dex",
        "arcana" | "history" | "investigation" | "nature" | "religion" => "int",
        "animal handling" | "insight" | "medicine" | "perception" | "survival" => "wis",
        _ => "cha",
    }
}
pub const SENSES: &[&str] = &["blindsight", "darkvision", "tremorsense", "truesight"];
pub const SPEEDS: &[&str] = &["ground", "alternate", "burrow", "climb", "flight", "swim"];
pub const AREAS: &[&str] = &["circle", "cone", "cube", "cylinder", "emanation", "line", "sphere", "square", "wall"];
pub const AREA_UNITS: &[&str] = &["feet", "inches", "miles"];
pub const ATTACK_KINDS: &[&str] = &["melee", "ranged", "both"];
pub const CONJUNCTIONS: &[&str] = &["plus", "or", "and"];
pub const HAZARDS: &[&str] = &["burning", "dehydration", "falling", "malnutrition", "suffocation"];
pub const CRS: &[&str] = &[
    "0", "1/8", "1/4", "1/2", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
];

/// The base `child` that means the file's root element itself (a magic item), not a
/// `<monster>` among the root's children.
/// The same number on both sides: the frontend (wasm32) and the backend (64-bit) have
/// different `usize::MAX`es, so it's the 32-bit one.
pub const ROOT: usize = u32::MAX as usize;

/// What an action list can hold.
pub const ITEMS: &[&str] =
    &["action", "attack", "save", "spellcasting", "spell", "actionRef", "attackRef", "saveRef", "spellRef", "speedRef"];

/// Elements whose content is prose: text with inline tags.
const PROSE: &[&str] = &[
    "multiattack", "hit", "miss", "chase", "override", "trigger", "response", "targets", "failure", "success",
    "failureOrSuccess", "cutline", "p", "td", "effect",
    // magic items
    "li", "th", "title", "subtitle", "data",
];

/// Is `<tag>` (in `<parent>`) prose? A trait's `<description>` is; the lore
/// `<description>` (paragraphs) isn't.
pub fn is_prose(parent: &str, tag: &str) -> bool {
    PROSE.contains(&tag) || (tag == "description" && parent == "trait") || (tag == "name" && parent == "magicItem")
}

/// An action's children other than its prose.
const ACTION_KIDS: &[&str] = &["uses", "cost", "trigger", "response"];
const ACTION_HEAD: &[&str] = &["uses", "cost", "trigger"];

/// Children in schema order, by parent; `*` stands for a list's items.
const ORDER: &[(&str, &[&str])] = &[
    (
        "monster",
        &[
            "name", "brief", "habitats", "quoteBox", "image", "description", "distinctionTable", "lairRef", "formulas",
            "heading", "statBlock", "attributes", "traits", "actions", "bonusActions", "reactions", "legendaryActions",
        ],
    ),
    (
        "heading",
        &["sizes", "type", "alignment", "armorClass", "initiativeBonus", "hitDice", "hitPoints", "proficiencyBonus", "speeds"],
    ),
    (
        "attributes",
        &["skills", "vulnerabilities", "resistances", "immunities", "gear", "senses", "languages", "challengeRatings"],
    ),
    ("languages", &["language", "telepathy"]),
    ("habitats", &["habitat", "treasure"]),
    ("quoteBox", &["quote", "attribution"]),
    ("distinctionTable", &["header", "body"]),
    ("trait", &["uses", "description", "save"]),
    ("action", &["uses", "cost", "trigger", "response"]),
    ("attack", &["uses", "cost", "trigger", "rider", "hit", "miss", "response"]),
    ("save", &["uses", "cost", "trigger", "targets", "failure", "success", "failureOrSuccess", "response"]),
    ("spellcasting", &["uses", "cost", "spellTier"]),
    ("rider", &["chase", "override"]),
    ("actions", &["multiattack", "*"]),
    ("bonusActions", &["multiattack", "*"]),
    ("legendaryActions", &["uses", "*"]),
    ("swarm", &["swarmSize", "memberSize"]),
    // magic items
    ("magicItem", &["name", "type", "subtypes", "rarities", "attunement", "description", "reference"]),
    ("table", &["title", "header", "tableBody", "footer"]),
    ("sidebar", &["title", "subtitle", "*"]),
];

/// Where `<tag>` goes among `<parent>`'s children: lower first; a list's items share one.
pub fn rank(parent: &str, tag: &str) -> usize {
    let Some((_, o)) = ORDER.iter().find(|(p, _)| *p == parent) else { return 0 };
    o.iter().position(|t| *t == tag).or_else(|| o.iter().position(|t| *t == "*")).unwrap_or(o.len())
}

/// The inline ref tags that can point at an element with this tag, by its id.
fn ref_tags(target: &str) -> &'static [&'static str] {
    match target {
        "action" | "spellcasting" => &["actionRef"],
        "attack" => &["actionRef", "attackRef"],
        "save" => &["actionRef", "saveRef"],
        "spell" => &["spellRef"],
        "speed" => &["speedRef"],
        _ => &[],
    }
}

/// One element of a stat block.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct El {
    pub tag: String,
    /// Child indexes from the `<monster>` to it; None for one that isn't in the file yet.
    pub path: Option<Vec<usize>>,
    /// In the order written.
    pub attrs: Vec<(String, String)>,
    /// Prose as XML; other text as plain text; an action's prose between its head and
    /// its response. Empty for an element of elements.
    pub text: String,
    pub kids: Vec<El>,
}

impl El {
    pub fn new(tag: &str) -> Self {
        El { tag: tag.to_string(), ..Default::default() }
    }
    /// With this attribute (builder).
    pub fn with(mut self, k: &str, v: &str) -> Self {
        self.set(k, v);
        self
    }
    pub fn with_text(mut self, t: &str) -> Self {
        self.text = t.to_string();
        self
    }
    /// With this child last (builder).
    pub fn with_kid(mut self, k: El) -> Self {
        self.kids.push(k);
        self
    }
    pub fn get(&self, k: &str) -> &str {
        self.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("")
    }
    pub fn has(&self, k: &str) -> bool {
        self.attrs.iter().any(|(n, _)| n == k)
    }
    /// Sets an attribute in place or adds it last; empty removes it.
    pub fn set(&mut self, k: &str, v: &str) {
        match (self.attrs.iter().position(|(n, _)| n == k), v.is_empty()) {
            (Some(i), false) => self.attrs[i].1 = v.to_string(),
            (Some(i), true) => {
                self.attrs.remove(i);
            }
            (None, false) => self.attrs.push((k.to_string(), v.to_string())),
            (None, true) => {}
        }
    }
    /// A boolean attribute (absent: `default`).
    pub fn flag(&self, k: &str, default: bool) -> bool {
        match self.get(k) {
            "true" | "1" => true,
            "false" | "0" => false,
            _ => default,
        }
    }
    /// Sets a boolean attribute; one that isn't written stays unwritten at its default.
    pub fn set_flag(&mut self, k: &str, v: bool, default: bool) {
        if self.has(k) || v != default {
            self.set(k, if v { "true" } else { "false" });
        }
    }
    pub fn kid(&self, tag: &str) -> Option<&El> {
        self.kids.iter().find(|k| k.tag == tag)
    }
    pub fn kid_mut(&mut self, tag: &str) -> Option<&mut El> {
        self.kids.iter_mut().find(|k| k.tag == tag)
    }
    /// The children with this tag.
    pub fn all(&self, tag: &str) -> Vec<&El> {
        self.kids.iter().filter(|k| k.tag == tag).collect()
    }
    /// The `n`th child with this tag.
    pub fn nth_mut(&mut self, tag: &str, n: usize) -> Option<&mut El> {
        self.kids.iter_mut().filter(|k| k.tag == tag).nth(n)
    }
    /// A new child where the schema puts it: after the last child that goes before or
    /// with it.
    pub fn insert_kid(&mut self, el: El) -> &mut El {
        let r = rank(&self.tag, &el.tag);
        let at = self.kids.iter().rposition(|k| rank(&self.tag, &k.tag) <= r).map_or(0, |i| i + 1);
        self.kids.insert(at, el);
        &mut self.kids[at]
    }
    /// The child with this tag, made (empty) if there isn't one.
    pub fn ensure(&mut self, tag: &str) -> &mut El {
        match self.kids.iter().position(|k| k.tag == tag) {
            Some(i) => &mut self.kids[i],
            None => self.insert_kid(El::new(tag)),
        }
    }
    pub fn remove_kid(&mut self, tag: &str) {
        self.kids.retain(|k| k.tag != tag);
    }
    /// The text of the child with this tag (empty without one).
    pub fn opt_text(&self, tag: &str) -> String {
        self.kid(tag).map(|k| k.text.clone()).unwrap_or_default()
    }
    /// Sets the text of an optional child: empty removes it (unless `keep`), otherwise
    /// it's made if missing.
    pub fn set_opt_text(&mut self, tag: &str, v: &str, keep: bool) {
        if v.trim().is_empty() && !keep {
            self.remove_kid(tag);
        } else {
            self.ensure(tag).text = v.to_string();
        }
    }
    /// The element `steps` down: each step a child tag and which of those children.
    pub fn at(&self, steps: &[(&str, usize)]) -> Option<&El> {
        let mut cur = self;
        for (t, n) in steps {
            cur = cur.kids.iter().filter(|k| k.tag == *t).nth(*n)?;
        }
        Some(cur)
    }
    pub fn at_mut(&mut self, steps: &[(&str, usize)]) -> Option<&mut El> {
        let mut cur = self;
        for (t, n) in steps {
            cur = cur.kids.iter_mut().filter(|k| k.tag == *t).nth(*n)?;
        }
        Some(cur)
    }
    /// This element and all inside it, depth first.
    pub fn walk(&self) -> Vec<&El> {
        let mut out = vec![self];
        for k in &self.kids {
            out.extend(k.walk());
        }
        out
    }
    /// The items of an action list.
    pub fn items(&self) -> Vec<&El> {
        self.kids.iter().filter(|k| ITEMS.contains(&k.tag.as_str())).collect()
    }
}

/// Every id in the stat block (attributes named id, anywhere in it).
pub fn ids(m: &El) -> Vec<String> {
    m.walk().into_iter().map(|e| e.get("id").to_string()).filter(|i| !i.is_empty()).collect()
}

/// `base`, or `base-2`, `base-3`... whichever isn't one of `taken`.
pub fn fresh(base: &str, taken: &[String]) -> String {
    let base = if base.is_empty() { "new" } else { base };
    if !taken.iter().any(|t| t == base) {
        return base.to_string();
    }
    (2..).map(|k| format!("{base}-{k}")).find(|c| !taken.contains(c)).unwrap()
}

/// A slug from a name: "Fire Breath" -> "fire-breath".
pub fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// Text without its surrounding blank lines and common indentation (as
/// [`block::p_text`] reads a paragraph).
fn dedent(inner: &str) -> String {
    if !inner.contains('\n') {
        return inner.trim().to_string();
    }
    let lines: Vec<&str> = inner.lines().map(|l| l.trim_end_matches('\r')).collect();
    let first = lines.iter().position(|l| !l.trim().is_empty());
    let last = lines.iter().rposition(|l| !l.trim().is_empty());
    let (Some(first), Some(last)) = (first, last) else { return String::new() };
    let body = &lines[first..=last];
    // The first line may follow other markup on its line: don't let it set the indent.
    let indent = body
        .iter()
        .enumerate()
        .filter(|(i, l)| !l.trim().is_empty() && (*i > 0 || body.len() == 1 || l.starts_with([' ', '\t'])))
        .map(|(_, l)| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    body.iter()
        .map(|l| if l.len() >= indent && l[..indent].trim().is_empty() { &l[indent..] } else { l.trim_start() })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// An element's children the forms see, as (index among its children, node).
fn struct_kids(nodes: &[StructNode], parent: &str, n: &StructNode) -> Vec<(usize, usize)> {
    if is_prose(parent, &n.tag) {
        return Vec::new();
    }
    n.children
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, c)| match n.tag.as_str() {
            "action" => ACTION_KIDS.contains(&nodes[*c].tag.as_str()),
            "size" => nodes[*c].tag == "swarm",
            _ => true,
        })
        .collect()
}

/// The span of an action's prose: after its head, before its response.
fn middle(source: &str, nodes: &[StructNode], n: &StructNode) -> Option<(usize, usize)> {
    let (a, b) = edit::inner_span(source, n)?;
    let kids = || n.children.iter().map(|c| &nodes[*c]);
    let start = kids().filter(|k| ACTION_HEAD.contains(&k.tag.as_str())).map(|k| k.end).max().unwrap_or(a);
    let end = kids().filter(|k| k.tag == "response").map(|k| k.start).min().unwrap_or(b);
    Some((start, end.max(start)))
}

fn read_el(source: &str, nodes: &[StructNode], parent: &str, idx: usize, path: Vec<usize>) -> El {
    let n = &nodes[idx];
    let attrs = edit::start_tag(source, n.start)
        .map(|t| t.attrs.into_iter().map(|a| (a.name, a.value)).collect())
        .unwrap_or_default();
    let prose = is_prose(parent, &n.tag);
    let kids: Vec<El> = struct_kids(nodes, parent, n)
        .into_iter()
        .map(|(i, c)| {
            let mut p = path.clone();
            p.push(i);
            read_el(source, nodes, &n.tag, c, p)
        })
        .collect();
    let text = if prose {
        block::p_text(source, n)
    } else if n.tag == "action" {
        middle(source, nodes, n).map(|(a, b)| dedent(&source[a..b])).unwrap_or_default()
    } else if kids.is_empty() && n.children.is_empty() {
        edit::text_of(source, n).trim().to_string()
    } else {
        String::new()
    };
    El { tag: n.tag.clone(), path: Some(path), attrs, text, kids }
}

/// The `<monster>` at the root's child `child` (or the root, for [`ROOT`]).
fn monster_node(nodes: &[StructNode], child: usize) -> Result<usize, String> {
    if child == ROOT {
        return nodes.first().map(|_| 0).ok_or_else(|| "the file is empty".to_string());
    }
    let root = nodes.first().filter(|r| r.tag == "monsters").ok_or("this isn't a monster file")?;
    root.children
        .get(child)
        .copied()
        .filter(|m| nodes[*m].tag == "monster")
        .ok_or_else(|| "that stat block isn't where it was any more; reopen the file".to_string())
}

fn node_at(nodes: &[StructNode], m: usize, path: &[usize], tag: &str) -> Result<usize, String> {
    let mut cur = m;
    for i in path {
        cur = *nodes[cur].children.get(*i).ok_or_else(|| format!("that <{tag}> isn't there any more; reopen the file"))?;
    }
    if nodes[cur].tag != tag {
        return Err(format!("that <{tag}> isn't where it was any more; reopen the file"));
    }
    Ok(cur)
}

/// The stat block at the root's child `child`.
pub fn read(source: &str, child: usize) -> Option<El> {
    let nodes = parse_all(source).ok()?.nodes;
    let m = monster_node(&nodes, child).ok()?;
    let parent = if child == ROOT { "" } else { "monsters" };
    Some(read_el(source, &nodes, parent, m, Vec::new()))
}

/// How many inline refs (and legendary ref items) in the stat block point at `id`.
pub fn refs_to(source: &str, child: usize, tag: &str, id: &str) -> usize {
    let Ok(p) = parse_all(source) else { return 0 };
    let Ok(m) = monster_node(&p.nodes, child) else { return 0 };
    let tags = ref_tags(tag);
    let (a, b) = (p.nodes[m].start, p.nodes[m].end);
    p.nodes
        .iter()
        .filter(|n| n.start > a && n.end <= b && tags.contains(&n.tag.as_str()))
        .filter(|n| edit::start_tag(source, n.start).and_then(|t| t.value("ref")).as_deref() == Some(id))
        .count()
}

/// The element as XML, its lines after the first indented one `unit` per level.
pub fn to_xml(el: &El, parent: &str, unit: &str) -> String {
    let mut s = format!("<{}", el.tag);
    for (k, v) in &el.attrs {
        s.push_str(&format!(" {k}=\"{}\"", escape_attr(v, '"')));
    }
    let text = if is_prose(parent, &el.tag) || el.tag == "action" {
        el.text.trim().to_string()
    } else {
        escape_text(el.text.trim())
    };
    if el.kids.is_empty() {
        return if text.is_empty() { format!("{s}/>") } else { format!("{s}>{text}</{}>", el.tag) };
    }
    let mut blocks: Vec<String> = Vec::new();
    let tail = |k: &El| el.tag == "action" && k.tag == "response";
    blocks.extend(el.kids.iter().filter(|k| !tail(k)).map(|k| to_xml(k, &el.tag, unit)));
    if !text.is_empty() {
        blocks.push(text);
    }
    blocks.extend(el.kids.iter().filter(|k| tail(k)).map(|k| to_xml(k, &el.tag, unit)));
    let body: Vec<String> = blocks.join("\n").lines().map(|l| format!("{unit}{l}")).collect();
    format!("{s}>\n{}\n</{}>", body.join("\n"), el.tag)
}

/// Replaces an action's prose, keeping the layout around it.
fn set_middle(source: &str, nodes: &[StructNode], n: &StructNode, text: &str) -> Option<Splice> {
    let text = text.trim();
    let tag = edit::start_tag(source, n.start)?;
    if tag.self_closing {
        return (!text.is_empty()).then(|| Splice { start: tag.close_at, end: tag.end, text: format!(">{text}</action>") });
    }
    let (a, b) = middle(source, nodes, n)?;
    let span = &source[a..b];
    if dedent(span) == text {
        return None;
    }
    let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let cind = format!("{}{}", edit::line_indent(source, n.start), edit::indent_unit(source));
    let lined = span.contains('\n') || n.children.iter().any(|c| ACTION_KIDS.contains(&nodes[*c].tag.as_str()));
    let body = if lined { text.lines().collect::<Vec<_>>().join(&format!("{nl}{cind}")) } else { text.to_string() };
    let lead = &span[..span.len() - span.trim_start().len()];
    let trail = &span[span.trim_end().len()..];
    let new = if span.trim().is_empty() {
        match (text.is_empty(), lined) {
            (true, _) => span.to_string(),
            (false, true) => format!("{nl}{cind}{body}{}", if span.is_empty() { format!("{nl}{}", edit::line_indent(source, n.start)) } else { span.to_string() }),
            (false, false) => body,
        }
    } else if text.is_empty() {
        trail.to_string()
    } else {
        format!("{lead}{body}{trail}")
    };
    Some(Splice { start: a, end: b, text: new })
}

#[derive(Clone, Copy, PartialEq)]
enum At {
    After(usize),
    Before(usize),
    Into,
}

struct Diff<'a> {
    source: &'a str,
    nodes: &'a [StructNode],
    unit: &'static str,
    attrs: Vec<Splice>,
    texts: Vec<Splice>,
    inserts: Vec<Splice>,
    removes: Vec<Splice>,
    /// (target tag, old id, new id)
    renames: Vec<(String, String, String)>,
}

impl Diff<'_> {
    fn el(&mut self, parent: &str, idx: usize, want: &El, scope: Option<&[String]>, defer_middle: bool) -> Result<(), String> {
        let (source, nodes) = (self.source, self.nodes);
        let n = &nodes[idx];
        if n.tag != want.tag {
            return Err(format!("expected <{}> but found <{}>; reopen the file", want.tag, n.tag));
        }
        let have = edit::start_tag(source, n.start).ok_or("bad start tag")?;
        // An action whose head or response comes or goes is written again whole: its prose
        // sits between them.
        if n.tag == "action" {
            let have_kids = struct_kids(nodes, parent, n);
            let same = want.kids.len() == have_kids.len()
                && want.kids.iter().zip(&have_kids).all(|(w, (i, c))| w.path.as_ref().and_then(|p| p.last()) == Some(i) && nodes[*c].tag == w.tag);
            if !same {
                if let (Some(old), new) = (have.value("id"), want.get("id")) {
                    if !old.is_empty() && !new.is_empty() && old != new {
                        self.renames.push((n.tag.clone(), old, new.to_string()));
                    }
                }
                let ind = edit::line_indent(source, n.start);
                let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
                let xml = to_xml(want, parent, self.unit).lines().collect::<Vec<_>>().join(&format!("{nl}{ind}"));
                self.texts.push(Splice { start: n.start, end: n.end, text: xml });
                return Ok(());
            }
        }
        // Attributes.
        for (k, v) in &want.attrs {
            let old = have.value(k);
            if old.as_deref() == Some(v.as_str()) || (old.is_none() && v.is_empty()) {
                continue;
            }
            if k == "id" && !v.is_empty() && old.as_deref().is_some_and(|o| !o.is_empty()) {
                self.renames.push((n.tag.clone(), old.clone().unwrap(), v.clone()));
            }
            self.attrs.extend(edit::set_attr(source, n, k, (!v.is_empty()).then_some(v.as_str())));
        }
        for a in &have.attrs {
            if !want.has(&a.name) {
                self.attrs.extend(edit::set_attr(source, n, &a.name, None));
            }
        }
        // Text.
        let prose = is_prose(parent, &n.tag);
        if prose {
            if block::p_text(source, n).trim() != want.text.trim() {
                self.texts.extend(block::set_inner(source, n, want.text.trim()));
            }
            return Ok(());
        }
        let have_kids = struct_kids(nodes, parent, n);
        if n.tag == "action" {
            if !defer_middle {
                if let Some(s) = middle(source, nodes, n) {
                    if dedent(&source[s.0..s.1]) != want.text.trim() {
                        self.texts.extend(set_middle(source, nodes, n, &want.text));
                    }
                }
            }
        } else if want.kids.is_empty() && n.children.is_empty() && edit::text_of(source, n).trim() != want.text.trim() {
            self.texts.extend(edit::set_text(source, n, want.text.trim()));
        }
        // Children.
        let managed = |tag: &str| scope.is_none_or(|s| s.iter().any(|t| t == tag));
        let mut kept: Vec<(usize, usize)> = Vec::new(); // (index among children, want kid)
        for (j, wk) in want.kids.iter().enumerate() {
            if let Some(p) = &wk.path {
                let i = *p.last().ok_or("a child has an empty path")?;
                match have_kids.iter().find(|(x, _)| *x == i) {
                    Some((_, c)) if nodes[*c].tag == wk.tag => kept.push((i, j)),
                    _ => return Err(format!("that <{}> isn't where it was any more; reopen the file", wk.tag)),
                }
            }
        }
        // Reordered: the managed children are written again, in the new order.
        let reorder = kept.windows(2).any(|w| w[0].0 > w[1].0);
        if reorder {
            kept.clear();
        }
        let is_kept = |i: usize| kept.iter().find(|(x, _)| *x == i).map(|(_, j)| *j);
        for (i, c) in &have_kids {
            if managed(&nodes[*c].tag) && is_kept(*i).is_none() {
                self.removes.push(edit::remove_element(source, &nodes[*c]));
            }
        }
        for (i, c) in &have_kids {
            if let Some(j) = is_kept(*i) {
                self.el(&n.tag, *c, &want.kids[j], None, false)?;
            }
        }
        // New children, each after the last staying child that goes before it.
        let staying: Vec<(usize, Option<usize>)> = have_kids
            .iter()
            .filter(|(i, c)| !managed(&nodes[*c].tag) || is_kept(*i).is_some())
            .map(|(i, c)| (*c, is_kept(*i)))
            .collect();
        let mut groups: Vec<(At, Vec<String>)> = Vec::new();
        for (j, wk) in want.kids.iter().enumerate() {
            if is_new(wk, &kept, j) {
                let r = rank(&n.tag, &wk.tag);
                let before = staying.iter().filter(|(c, w)| {
                    let rk = rank(&n.tag, &nodes[*c].tag);
                    rk < r || (rk == r && w.is_none_or(|w| w < j))
                });
                let at = match before.last() {
                    // An action's response follows its prose: last.
                    _ if n.tag == "action" && wk.tag == "response" => At::Into,
                    Some((c, _)) => At::After(*c),
                    None => staying.first().map_or(At::Into, |(c, _)| At::Before(*c)),
                };
                let xml = to_xml(wk, &n.tag, self.unit);
                match groups.last_mut() {
                    Some((a, v)) if *a == at => v.push(xml),
                    _ => groups.push((at, vec![xml])),
                }
            }
        }
        for (at, xml) in groups {
            let anchor = match at {
                At::After(c) => Anchor::After(&nodes[c]),
                At::Before(c) => Anchor::Before(&nodes[c]),
                At::Into => Anchor::Into(n),
            };
            self.inserts.push(edit::insert_child(source, anchor, &xml.join("\n")).ok_or("can't place a new element")?);
        }
        Ok(())
    }
}

fn is_new(wk: &El, kept: &[(usize, usize)], j: usize) -> bool {
    wk.path.is_none() || !kept.iter().any(|(_, k)| *k == j)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum StatEdit {
    /// Make the element at `el.path` in the stat block at root child `child` look like
    /// `el`. With `scope`, only its attributes, its text and its children with these
    /// tags are compared; other children stay as they are.
    Update { child: usize, el: El, scope: Option<Vec<String>> },
    /// A new element in the element at `parent`: after its child `after`, or (None)
    /// where the schema puts it, after the last child that goes before or with it.
    Insert { child: usize, parent: Vec<usize>, parent_tag: String, el: El, after: Option<usize> },
    Delete { child: usize, path: Vec<usize>, tag: String },
    /// Move an element before its sibling `before` (a child index), or (None) after the
    /// last of its kind.
    Move { child: usize, path: Vec<usize>, tag: String, before: Option<usize> },
}

impl StatEdit {
    pub fn child(&self) -> usize {
        match self {
            StatEdit::Update { child, .. }
            | StatEdit::Insert { child, .. }
            | StatEdit::Delete { child, .. }
            | StatEdit::Move { child, .. } => *child,
        }
    }
    /// Does it add, remove or reorder children, so paths after them shift?
    pub fn reshapes(&self) -> bool {
        !matches!(self, StatEdit::Update { scope: None, .. })
    }
}

/// Renames refs inside the stat block from old ids to new.
fn rename_refs(source: &str, child: usize, renames: &[(String, String, String)]) -> Result<String, String> {
    if renames.is_empty() {
        return Ok(source.to_string());
    }
    let nodes = parse_all(source).map_err(|e| e.message)?.nodes;
    let m = monster_node(&nodes, child)?;
    let (a, b) = (nodes[m].start, nodes[m].end);
    let mut out = Vec::new();
    for n in nodes.iter().filter(|n| n.start > a && n.end <= b) {
        for (tag, old, new) in renames {
            if ref_tags(tag).contains(&n.tag.as_str())
                && edit::start_tag(source, n.start).and_then(|t| t.value("ref")).as_deref() == Some(old.as_str())
            {
                out.extend(edit::set_attr(source, n, "ref", Some(new)));
            }
        }
    }
    edit::apply(source, &out)
}

pub fn plan(source: &str, e: &StatEdit) -> Result<String, String> {
    let nodes = parse_all(source).map_err(|e| e.message)?.nodes;
    let m = monster_node(&nodes, e.child())?;
    let unit = edit::indent_unit(source);
    match e {
        StatEdit::Update { child, el, scope } => {
            let path = el.path.clone().ok_or("that element isn't in the file yet")?;
            let idx = node_at(&nodes, m, &path, &el.tag)?;
            let parent = match (path.is_empty(), *child == ROOT) {
                (true, true) => String::new(),
                (true, false) => "monsters".to_string(),
                _ => tag_at(&nodes, m, &path[..path.len() - 1]),
            };
            let mut d = Diff {
                source,
                nodes: &nodes,
                unit,
                attrs: vec![],
                texts: vec![],
                inserts: vec![],
                removes: vec![],
                renames: vec![],
            };
            d.el(&parent, idx, el, scope.as_deref(), true)?;
            let renames = std::mem::take(&mut d.renames);
            let splices: Vec<Splice> = [d.attrs, d.texts, d.inserts, d.removes].concat();
            let cur = edit::apply(source, &splices)?;
            // The action's own prose second: its head may just have changed around it.
            let cur = if el.tag == "action" {
                let ns = parse_all(&cur).map_err(|e| e.message)?.nodes;
                let m2 = monster_node(&ns, *child)?;
                let a = node_at(&ns, m2, &path, "action")?;
                let s: Vec<Splice> = set_middle(&cur, &ns, &ns[a], &el.text).into_iter().collect();
                edit::apply(&cur, &s)?
            } else {
                cur
            };
            rename_refs(&cur, *child, &renames)
        }
        StatEdit::Insert { parent, parent_tag, el, after, .. } => {
            let p = node_at(&nodes, m, parent, parent_tag)?;
            let pn = &nodes[p];
            let ptag = pn.tag.clone();
            let kids = struct_kids(&nodes, "", pn);
            let xml = to_xml(el, &ptag, unit);
            let anchor = match after {
                Some(a) => Anchor::After(&nodes[*pn.children.get(*a).ok_or("the place to add it isn't there any more")?]),
                None => {
                    let r = rank(&ptag, &el.tag);
                    match kids.iter().rev().find(|(_, c)| rank(&ptag, &nodes[*c].tag) <= r) {
                        Some((_, c)) => Anchor::After(&nodes[*c]),
                        None => match kids.first() {
                            Some((_, c)) => Anchor::Before(&nodes[*c]),
                            None => Anchor::Into(pn),
                        },
                    }
                }
            };
            let s = edit::insert_child(source, anchor, &xml).ok_or("can't place the new element")?;
            edit::apply(source, &[s])
        }
        StatEdit::Delete { path, tag, .. } => {
            if path.is_empty() {
                if e.child() == ROOT {
                    return Err("can't delete the file's root".into());
                }
                return Err("delete a stat block from the stat block list".into());
            }
            let n = node_at(&nodes, m, path, tag)?;
            edit::apply(source, &[edit::remove_element(source, &nodes[n])])
        }
        StatEdit::Move { path, tag, before, .. } => {
            let (last, up) = path.split_last().ok_or("can't move the stat block from here")?;
            let n = node_at(&nodes, m, path, tag)?;
            let p = node_at(&nodes, m, up, &tag_at(&nodes, m, up))?;
            let pn = &nodes[p];
            let r = rank(&pn.tag, tag);
            let list: Vec<usize> =
                (0..pn.children.len()).filter(|i| rank(&pn.tag, &nodes[pn.children[*i]].tag) == r).collect();
            let pos = list.iter().position(|i| i == last).ok_or("can't find it among its kind")?;
            let b = match before {
                Some(b) => Some(list.iter().position(|i| i == b).ok_or("the place to move it to isn't one of its kind")?),
                None => None,
            };
            if b == Some(pos) || b == Some(pos + 1) || (b.is_none() && pos + 1 == list.len()) {
                return Ok(source.to_string());
            }
            let target = b.map(|b| &nodes[pn.children[list[b]]]);
            let lastn = list.last().map(|i| &nodes[pn.children[*i]]);
            let s = edit::move_element(source, &nodes[n], target, lastn)?;
            edit::apply(source, &s)
        }
    }
}

fn tag_at(nodes: &[StructNode], m: usize, path: &[usize]) -> String {
    let mut cur = m;
    for i in path {
        match nodes[cur].children.get(*i) {
            Some(c) => cur = *c,
            None => return String::new(),
        }
    }
    nodes[cur].tag.clone()
}

/// One attribute of an inline tag, for the Insert menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Field {
    Text,
    Int,
    Dec,
    Bool,
    Pick(&'static [&'static str]),
    /// An id of something in this stat block with one of these tags.
    Ref(&'static [&'static str]),
}

/// An inline tag a stat block's prose can hold.
pub struct InlineSpec {
    pub tag: &'static str,
    pub label: &'static str,
    /// (attribute, kind, required)
    pub attrs: &'static [(&'static str, Field, bool)],
    /// The tag's content, if it has any; Pick: one of these words.
    pub text: Option<Field>,
}

const ACTIONISH: &[&str] = &["action", "attack", "save", "spellcasting"];
const HP: &[(&str, Field, bool)] = &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Int, false)];

pub const INLINE: &[InlineSpec] = &[
    InlineSpec { tag: "shortNounRef", label: "Short noun (the creature)", attrs: &[], text: None },
    InlineSpec { tag: "damage", label: "Damage", attrs: &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Int, false), ("stat", Field::Pick(STATS), false), ("type", Field::Pick(DAMAGE_TYPES), true)], text: None },
    InlineSpec { tag: "condition", label: "Condition", attrs: &[], text: Some(Field::Pick(CONDITIONS)) },
    InlineSpec { tag: "savingthrow", label: "Saving throw", attrs: &[("stat", Field::Pick(STATS), true), ("dc", Field::Text, true)], text: None },
    InlineSpec { tag: "skillcheck", label: "Skill check", attrs: &[("stat", Field::Pick(STATS), true), ("skill", Field::Pick(SKILLS), true), ("dc", Field::Int, false), ("table", Field::Bool, false)], text: None },
    InlineSpec { tag: "area", label: "Area of effect", attrs: &[("name", Field::Pick(AREAS), true), ("size", Field::Dec, false), ("radius", Field::Dec, false), ("diameter", Field::Dec, false), ("length", Field::Dec, false), ("width", Field::Dec, false), ("height", Field::Dec, false), ("thickness", Field::Dec, false), ("unit", Field::Pick(AREA_UNITS), false), ("thicknessUnit", Field::Pick(AREA_UNITS), false)], text: None },
    InlineSpec { tag: "healing", label: "Healing", attrs: HP, text: None },
    InlineSpec { tag: "temphp", label: "Temporary hit points", attrs: HP, text: None },
    InlineSpec { tag: "hazard", label: "Hazard", attrs: &[("name", Field::Pick(HAZARDS), false)], text: Some(Field::Text) },
    InlineSpec { tag: "actionRef", label: "Action ref", attrs: &[("ref", Field::Ref(ACTIONISH), true)], text: None },
    InlineSpec { tag: "attackRef", label: "Attack ref", attrs: &[("ref", Field::Ref(&["attack"]), true)], text: None },
    InlineSpec { tag: "saveRef", label: "Save ref", attrs: &[("ref", Field::Ref(&["save"]), true)], text: None },
    InlineSpec { tag: "spellRef", label: "Spell ref", attrs: &[("ref", Field::Ref(&["spell"]), true)], text: None },
    InlineSpec { tag: "speedRef", label: "Speed ref", attrs: &[("ref", Field::Ref(&["speed"]), true)], text: None },
    InlineSpec { tag: "speed", label: "Speed name", attrs: &[("name", Field::Pick(SPEEDS), true)], text: None },
    InlineSpec { tag: "rule", label: "Rules term", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "spellName", label: "Spell name", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "itemName", label: "Item name", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "creatureName", label: "Creature name", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "b", label: "Bold", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "i", label: "Italic", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookRef", label: "Core book", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookChapterRef", label: "Core book chapter", attrs: &[("num", Field::Int, true)], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookSectionRef", label: "Core book section", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookTableRef", label: "Core book table", attrs: &[], text: Some(Field::Text) },
];

pub fn inline_spec(tag: &str) -> Option<&'static InlineSpec> {
    INLINE.iter().find(|s| s.tag == tag)
}

/// An inline tag as XML: attributes in spec order (empty ones left out), and its text.
pub fn inline_xml(tag: &str, attrs: &[(String, String)], text: &str) -> String {
    build_inline(tag, inline_spec(tag).is_some_and(|sp| sp.text.is_some()), attrs, text)
}

/// [`inline_xml`] for a spec from any list (a magic item's tags, say).
pub fn inline_xml_with(spec: &InlineSpec, attrs: &[(String, String)], text: &str) -> String {
    build_inline(spec.tag, spec.text.is_some(), attrs, text)
}

fn build_inline(tag: &str, has_text: bool, attrs: &[(String, String)], text: &str) -> String {
    let mut s = format!("<{tag}");
    for (k, v) in attrs.iter().filter(|(_, v)| !v.trim().is_empty()) {
        s.push_str(&format!(" {k}=\"{}\"", escape_attr(v.trim(), '"')));
    }
    if has_text {
        format!("{s}>{}</{tag}>", escape_text(text))
    } else {
        format!("{s}/>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<?xml version="1.0"?>
<monsters>
    <name>Gnolls</name>
    <shortNoun>gnoll</shortNoun>
    <monster id="g">
        <name>Gnoll</name>
        <heading>
            <sizes>
                <size>Medium</size>
            </sizes>
            <type>Fiend</type>
            <armorClass>15</armorClass>
            <initiativeBonus/>
            <hitDice flatBonus="">5</hitDice>
            <speeds>
                <speed id="ground" name="ground" value="30"/>
            </speeds>
        </heading>
        <traits>
            <trait name="Rampage">
                <description>After it kills, it moves.</description>
            </trait>
        </traits>
        <actions>
            <attack id="bite" name="Bite" type="melee" reach="5" damage="1d6" damageType="piercing">
                <hit>Ow.</hit>
            </attack>
            <action id="howl" name="Howl">The <shortNounRef/> makes a <attackRef ref="bite"/>.</action>
        </actions>
        <reactions>
            <action id="parry" name="Parry">
                <uses number="3" frequency="day" note=""/>
                It parries.
            </action>
        </reactions>
    </monster>
</monsters>
"#;

    fn m(s: &str) -> El {
        read(s, 2).unwrap()
    }
    fn up(s: &str, el: El) -> String {
        plan(s, &StatEdit::Update { child: 2, el, scope: None }).unwrap()
    }

    #[test]
    fn reads_the_tree() {
        let g = m(SRC);
        assert_eq!(g.get("id"), "g");
        let heading = g.kid("heading").unwrap();
        assert_eq!(heading.kid("type").unwrap().text, "Fiend");
        assert_eq!(heading.at(&[("sizes", 0), ("size", 0)]).unwrap().text, "Medium");
        let acts = g.kid("actions").unwrap();
        assert_eq!(acts.items().len(), 2);
        assert_eq!(acts.kids[1].text, "The <shortNounRef/> makes a <attackRef ref=\"bite\"/>.");
        assert!(acts.kids[1].kids.is_empty(), "inline tags aren't children");
        let parry = g.at(&[("reactions", 0), ("action", 0)]).unwrap();
        assert_eq!(parry.text, "It parries.");
        assert_eq!(parry.kids[0].get("note"), "");
        assert_eq!(parry.kids[0].path, Some(vec![4, 0, 0]));
        assert_eq!(g.at(&[("traits", 0), ("trait", 0), ("description", 0)]).unwrap().text, "After it kills, it moves.");
        assert_eq!(refs_to(SRC, 2, "attack", "bite"), 1);
    }

    #[test]
    fn unchanged_is_a_no_op() {
        let g = m(SRC);
        for k in &g.kids {
            assert_eq!(up(SRC, k.clone()), SRC);
        }
    }

    #[test]
    fn heading_fields_and_new_children() {
        let mut h = m(SRC).kid("heading").unwrap().clone();
        h.ensure("type").text = "Fiend (Gnoll)".into();
        h.ensure("alignment").text = "Chaotic Evil".into();
        h.ensure("proficiencyBonus").text = "2".into();
        h.ensure("initiativeBonus").set_flag("advantage", true, false);
        h.kid_mut("hitDice").unwrap().set("flatBonus", "");
        h.at_mut(&[("speeds", 0)]).unwrap().kids.push(El::new("speed").with("id", "climb").with("name", "climb").with("value", "20"));
        let out = up(SRC, h);
        assert!(out.contains("            <type>Fiend (Gnoll)</type>\n            <alignment>Chaotic Evil</alignment>\n            <armorClass>15</armorClass>\n            <initiativeBonus advantage=\"true\"/>\n            <hitDice>5</hitDice>\n            <proficiencyBonus>2</proficiencyBonus>\n            <speeds>\n                <speed id=\"ground\" name=\"ground\" value=\"30\"/>\n                <speed id=\"climb\" name=\"climb\" value=\"20\"/>\n            </speeds>"), "{out}");
        // A size reordered: the list is written again.
        let mut h = m(&out).kid("heading").unwrap().clone();
        h.kid_mut("sizes").unwrap().kids.push(El::new("size").with_text("Small"));
        h.kid_mut("sizes").unwrap().kids.reverse();
        let out2 = up(&out, h);
        assert!(out2.contains("            <sizes>\n                <size>Small</size>\n                <size>Medium</size>\n            </sizes>"), "{out2}");
    }

    #[test]
    fn attack_riders_and_prose() {
        let mut a = m(SRC).at(&[("actions", 0), ("attack", 0)]).unwrap().clone();
        a.set("damageType", "slashing");
        a.insert_kid(El::new("rider").with("damage", "1d4").with("damageType", "fire").with_kid(El::new("chase").with_text("if <condition>prone</condition>")));
        a.set_opt_text("miss", "Half <b>damage</b>.", false);
        a.set_opt_text("hit", "", false);
        let out = up(SRC, a);
        assert!(out.contains("<attack id=\"bite\" name=\"Bite\" type=\"melee\" reach=\"5\" damage=\"1d6\" damageType=\"slashing\">\n                <rider damage=\"1d4\" damageType=\"fire\">\n                    <chase>if <condition>prone</condition></chase>\n                </rider>\n                <miss>Half <b>damage</b>.</miss>\n            </attack>"), "{out}");
    }

    #[test]
    fn action_prose_and_head() {
        let mut p = m(SRC).at(&[("reactions", 0), ("action", 0)]).unwrap().clone();
        p.text = "It parries <i>well</i>.".into();
        p.set_opt_text("trigger", "It's hit.", false);
        p.set_opt_text("response", "AC +2.", false);
        let out = up(SRC, p);
        assert!(out.contains("<action id=\"parry\" name=\"Parry\">\n                <uses number=\"3\" frequency=\"day\" note=\"\"/>\n                <trigger>It's hit.</trigger>\n                It parries <i>well</i>.\n                <response>AC +2.</response>\n            </action>"), "{out}");
        let back = m(&out).at(&[("reactions", 0), ("action", 0)]).unwrap().clone();
        assert_eq!(back.text, "It parries <i>well</i>.");
        // A one-line action given a trigger opens up, its prose after the trigger.
        let mut h = m(SRC).at(&[("actions", 0), ("action", 0)]).unwrap().clone();
        h.set_opt_text("trigger", "Night falls.", false);
        let out = up(SRC, h);
        assert!(out.contains("            <action id=\"howl\" name=\"Howl\">\n                <trigger>Night falls.</trigger>\n                The <shortNounRef/> makes a <attackRef ref=\"bite\"/>.\n            </action>\n        </actions>"), "{out}");
        // One-line action: stays on one line.
        let mut h = m(SRC).at(&[("actions", 0), ("action", 0)]).unwrap().clone();
        h.text = "It howls.".into();
        let out = up(SRC, h);
        assert!(out.contains("<action id=\"howl\" name=\"Howl\">It howls.</action>"), "{out}");
    }

    #[test]
    fn renaming_an_id_renames_its_refs() {
        let mut a = m(SRC).at(&[("actions", 0), ("attack", 0)]).unwrap().clone();
        a.set("id", "jaws");
        let out = up(SRC, a);
        assert!(out.contains("<attack id=\"jaws\"") && out.contains("<attackRef ref=\"jaws\"/>"), "{out}");
    }

    #[test]
    fn scoped_update_leaves_the_rest() {
        let mut g = m(SRC);
        g.set("src", "Homebrew");
        g.kid_mut("name").unwrap().text = "Gnoll Warrior".into();
        g.kids.retain(|k| k.tag == "name");
        g.insert_kid(El::new("brief").with_text("Hyena-men"));
        let out = plan(SRC, &StatEdit::Update { child: 2, el: g, scope: Some(vec!["name".into(), "brief".into()]) }).unwrap();
        assert!(out.contains("<monster id=\"g\" src=\"Homebrew\">\n        <name>Gnoll Warrior</name>\n        <brief>Hyena-men</brief>\n        <heading>"), "{out}");
        // A multiattack goes before the items.
        let mut acts = m(SRC).kid("actions").unwrap().clone();
        acts.kids.retain(|k| k.tag == "multiattack");
        acts.set_opt_text("multiattack", "Two bites.", false);
        let out = plan(SRC, &StatEdit::Update { child: 2, el: acts, scope: Some(vec!["multiattack".into()]) }).unwrap();
        assert!(out.contains("        <actions>\n            <multiattack>Two bites.</multiattack>\n            <attack id=\"bite\""), "{out}");
    }

    #[test]
    fn insert_delete_move() {
        let t = El::new("trait").with("name", "Keen Smell").with_kid(El::new("description").with_text("It smells."));
        let out = plan(SRC, &StatEdit::Insert { child: 2, parent: vec![2], parent_tag: "traits".into(), el: t, after: None }).unwrap();
        assert!(out.contains("            </trait>\n            <trait name=\"Keen Smell\">\n                <description>It smells.</description>\n            </trait>\n        </traits>"), "{out}");
        let ba = El::new("bonusActions").with_kid(El::new("action").with("id", "dash").with("name", "Dash").with_text("It dashes."));
        let out = plan(SRC, &StatEdit::Insert { child: 2, parent: vec![], parent_tag: "monster".into(), el: ba, after: None }).unwrap();
        assert!(out.contains("        </actions>\n        <bonusActions>\n            <action id=\"dash\" name=\"Dash\">It dashes.</action>\n        </bonusActions>\n        <reactions>"), "{out}");
        let out = plan(SRC, &StatEdit::Move { child: 2, path: vec![3, 1], tag: "action".into(), before: Some(0) }).unwrap();
        assert_eq!(m(&out).kid("actions").unwrap().kids[0].get("id"), "howl");
        let out = plan(SRC, &StatEdit::Delete { child: 2, path: vec![2], tag: "traits".into() }).unwrap();
        assert!(!out.contains("<traits>"));
        assert!(plan(SRC, &StatEdit::Delete { child: 2, path: vec![2], tag: "actions".into() }).is_err());
    }

    #[test]
    fn crlf_and_new_xml() {
        let src = SRC.replace('\n', "\r\n");
        let mut a = m(&src).at(&[("reactions", 0), ("action", 0)]).unwrap().clone();
        a.set_opt_text("trigger", "Hit.", false);
        a.text = "Two\nlines.".into();
        let out = up(&src, a);
        assert!(!out.replace("\r\n", "").contains('\n'), "{out:?}");
        assert_eq!(inline_xml("damage", &[("num".into(), "2".into()), ("mod".into(), " ".into()), ("type".into(), "fire".into())], ""), "<damage num=\"2\" type=\"fire\"/>");
        assert_eq!(inline_xml("condition", &[], "prone"), "<condition>prone</condition>");
    }
}
