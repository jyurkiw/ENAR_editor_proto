//! The `<reference>` editor: its entries (data, monster, noun, spell, item, image)
//! edited one at a time, added, deleted and moved.
//!
//! An entry is known by its tag and id: ids are unique per kind (a spell and an item can
//! share one), so `(tag, id)` names exactly one entry. Changing an id also renames every
//! element that points at it.

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::edit::{self, Anchor, Splice};
use crate::xml::parse;
use crate::StructNode;

/// The entry kinds, in the order the Add menu lists them, with a description.
pub const KINDS: &[(&str, &str)] = &[
    ("monster", "A creature: a book stat block, or one from a monster file"),
    ("spell", "A spell: from a book, or a spell file"),
    ("item", "A magic item: from a book, or an item file"),
    ("noun", "A named person, place or thing with no stat block"),
    ("image", "Art: cover art, or an image sections print by id"),
    ("data", "A value or text printed with <dataRef>"),
];

/// One entry, as the form edits it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RefEntry {
    pub tag: String,
    /// Every attribute, in file order; ones the form doesn't know are kept.
    pub attrs: Vec<(String, String)>,
    /// `<data>`'s text.
    pub body: String,
    /// The data body holds markup, so it's shown read-only.
    pub body_markup: bool,
    /// `<monster>`'s `<choose choice option>` picks.
    pub chooses: Vec<(String, String)>,
}

impl RefEntry {
    pub fn new(tag: &str) -> Self {
        RefEntry { tag: tag.into(), ..Default::default() }
    }

    pub fn get(&self, name: &str) -> String {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone()).unwrap_or_default()
    }

    /// Sets an attribute; empty removes it. A new attribute goes at the end.
    pub fn set(&mut self, name: &str, value: String) {
        match self.attrs.iter().position(|(k, _)| k == name) {
            Some(i) if value.is_empty() => {
                self.attrs.remove(i);
            }
            Some(i) => self.attrs[i].1 = value,
            None if value.is_empty() => {}
            None => self.attrs.push((name.into(), value)),
        }
    }

    pub fn id(&self) -> String {
        self.get("id")
    }

    /// A one-line description for the collapsed card.
    pub fn label(&self) -> String {
        let first = |names: &[&str]| names.iter().map(|n| self.get(n)).find(|v| !v.is_empty());
        let file_name = || {
            let f = self.get("file");
            (!f.is_empty()).then(|| f.rsplit(['/', '\\']).next().unwrap_or(&f).to_string())
        };
        match self.tag.as_str() {
            "monster" => {
                let name = first(&["displayName", "name"]).or_else(|| {
                    let m = self.get("monster");
                    file_name().map(|f| if m.is_empty() { f } else { format!("{m} in {f}") })
                });
                let cr = self.get("cr");
                match (name, cr.is_empty()) {
                    (Some(n), false) => format!("{n} (CR {cr})"),
                    (Some(n), true) => n,
                    (None, _) => String::new(),
                }
            }
            "spell" | "item" => first(&["displayName", "name"]).or_else(file_name).unwrap_or_default(),
            "noun" => first(&["displayName", "name"]).unwrap_or_default(),
            "image" => first(&["name"]).map(|n| format!("{n} ({})", self.get("src"))).unwrap_or_else(|| self.get("src")),
            "data" => {
                let v = self.get("value");
                if !v.is_empty() {
                    format!("= {v}")
                } else {
                    // The body is XML: its text, without the tags.
                    let mut plain = String::new();
                    let mut inside = false;
                    for ch in self.body.chars() {
                        match ch {
                            '<' => inside = true,
                            '>' => inside = false,
                            _ if !inside => plain.push(ch),
                            _ => {}
                        }
                    }
                    edit::snippet(&edit::unescape(&plain).split_whitespace().collect::<Vec<_>>().join(" "), 50)
                }
            }
            _ => String::new(),
        }
    }

    /// Attributes the schema requires that are empty, by name.
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.id().trim().is_empty() {
            out.push("id");
        }
        match self.tag.as_str() {
            "noun" if self.get("name").trim().is_empty() => out.push("name"),
            "image" if self.get("src").trim().is_empty() => out.push("src"),
            _ => {}
        }
        if self.chooses.iter().any(|(c, o)| c.trim().is_empty() || o.trim().is_empty()) {
            out.push("choose");
        }
        out
    }
}

/// A change to the reference block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum RefEdit {
    /// Change the entry now known as (tag, id).
    Update { tag: String, id: String, entry: RefEntry },
    /// Add an entry at the end (creating `<reference>` if there's none).
    Add { entry: RefEntry },
    Delete { tag: String, id: String },
    /// Move an entry before another, or to the end.
    Move { tag: String, id: String, before: Option<(String, String)> },
    /// Add an empty `<reference/>` (right before `<chapterOrder>`).
    CreateBlock,
}

fn reference_node(nodes: &[StructNode]) -> Option<usize> {
    nodes.first()?.children.iter().copied().find(|c| nodes[*c].tag == "reference")
}

fn entry_nodes(nodes: &[StructNode]) -> Vec<usize> {
    reference_node(nodes).map(|r| nodes[r].children.clone()).unwrap_or_default()
}

fn read_entry(source: &str, nodes: &[StructNode], n: &StructNode) -> RefEntry {
    let attrs = edit::start_tag(source, n.start).map(|t| t.attrs.into_iter().map(|a| (a.name, a.value)).collect()).unwrap_or_default();
    // A data entry's body is its XML (text and inline tags), edited in a chip box.
    let (body, body_markup) = if n.tag == "data" {
        match edit::inner_span(source, n) {
            Some((a, b)) => (crate::block::p_text(source, n), source[a..b].contains('<')),
            None => (String::new(), false),
        }
    } else {
        (String::new(), false)
    };
    let chooses = n
        .children
        .iter()
        .map(|c| &nodes[*c])
        .filter(|c| c.tag == "choose")
        .filter_map(|c| edit::start_tag(source, c.start))
        .map(|t| (t.value("choice").unwrap_or_default(), t.value("option").unwrap_or_default()))
        .collect();
    RefEntry { tag: n.tag.clone(), attrs, body, body_markup, chooses }
}

/// The entries, in file order, with their node indexes.
pub fn read(source: &str, nodes: &[StructNode]) -> Vec<(usize, RefEntry)> {
    entry_nodes(nodes).into_iter().map(|i| (i, read_entry(source, nodes, &nodes[i]))).collect()
}

pub fn has_block(nodes: &[StructNode]) -> bool {
    reference_node(nodes).is_some()
}

fn find(source: &str, nodes: &[StructNode], tag: &str, id: &str) -> Option<usize> {
    entry_nodes(nodes)
        .into_iter()
        .find(|i| nodes[*i].tag == tag && edit::start_tag(source, nodes[*i].start).and_then(|t| t.value("id")).as_deref() == Some(id))
}

/// Where a pointer can be.
#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    Any,
    /// Not inside `<reference>` (an inline `<monster ref>` in a paragraph).
    Outside,
    /// Directly in `<reference>` (a template's summonerRef).
    Inside,
    /// In `<magicItems>` (a treasure parcel's `<item ref>`).
    Treasure,
    /// A section `<image>` without `@src`: it prints the reference image with its id.
    SectionImage,
}

/// (pointing tag, its attribute, where, the entry kind it points at)
const POINTERS: &[(&str, &str, Ctx, &str)] = &[
    ("monster", "ref", Ctx::Outside, "monster"),
    ("monsterRef", "ref", Ctx::Any, "monster"),
    ("monsterStatblock", "ref", Ctx::Any, "monster"),
    ("monsterEntry", "ref", Ctx::Any, "monster"),
    ("monster", "summonerRef", Ctx::Inside, "monster"),
    ("spellRef", "ref", Ctx::Any, "spell"),
    ("spellBlock", "ref", Ctx::Any, "spell"),
    ("itemRef", "ref", Ctx::Any, "item"),
    ("magicItemBlock", "ref", Ctx::Any, "item"),
    ("item", "ref", Ctx::Treasure, "item"),
    ("coverImage", "ref", Ctx::Any, "image"),
    ("cover", "id", Ctx::Any, "image"),
    ("image", "id", Ctx::SectionImage, "image"),
    ("nounRef", "ref", Ctx::Any, "noun"),
    ("dataRef", "ref", Ctx::Any, "data"),
    // A magic item's creator: any kind of entry (their ids are unique across kinds there).
    ("magicItem", "creator", Ctx::Any, "monster"),
    ("magicItem", "creator", Ctx::Any, "spell"),
    ("magicItem", "creator", Ctx::Any, "item"),
    ("magicItem", "creator", Ctx::Any, "data"),
    // A spell's creator likewise (a <reference><spell> entry has no creator attribute).
    ("spell", "creator", Ctx::Any, "monster"),
    ("spell", "creator", Ctx::Any, "spell"),
    ("spell", "creator", Ctx::Any, "item"),
    ("spell", "creator", Ctx::Any, "data"),
];

/// Every pointer at a reference entry: (kind, id, start-tag offset, attribute).
fn pointers(source: &str) -> Vec<(&'static str, String, usize, &'static str)> {
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut out = Vec::new();
    loop {
        let pos = reader.buffer_position() as usize;
        let ev = match reader.read_event() {
            Ok(Event::Eof) | Err(_) => break,
            Ok(ev) => ev,
        };
        match &ev {
            Event::Start(e) | Event::Empty(e) => {
                let tag = e.name().as_ref().to_vec();
                let in_ref = stack.iter().any(|t| t == b"reference");
                let parent = stack.last().map(|p| p.as_slice());
                let attrs: Vec<(Vec<u8>, String)> = e
                    .attributes()
                    .flatten()
                    .map(|a| (a.key.as_ref().to_vec(), a.unescape_value().map(|v| v.into_owned()).unwrap_or_default()))
                    .collect();
                let has_src = attrs.iter().any(|(k, _)| k == b"src");
                for (ptag, pattr, ctx, kind) in POINTERS {
                    if tag != ptag.as_bytes() {
                        continue;
                    }
                    let ok = match ctx {
                        Ctx::Any => true,
                        Ctx::Outside => !in_ref,
                        Ctx::Inside => parent == Some(b"reference"),
                        Ctx::Treasure => parent == Some(b"magicItems"),
                        Ctx::SectionImage => !in_ref && !has_src,
                    };
                    if !ok {
                        continue;
                    }
                    if let Some((_, v)) = attrs.iter().find(|(k, _)| k == pattr.as_bytes()) {
                        out.push((*kind, v.clone(), pos, *pattr));
                    }
                }
                if matches!(ev, Event::Start(_)) {
                    stack.push(tag);
                }
            }
            Event::End(_) => {
                stack.pop();
            }
            _ => {}
        }
    }
    out
}

/// How many elements point at each (kind, id).
pub fn usage(source: &str) -> HashMap<(String, String), usize> {
    let mut m = HashMap::new();
    for (kind, id, _, _) in pointers(source) {
        *m.entry((kind.to_string(), id)).or_insert(0) += 1;
    }
    m
}

fn rename_pointers(source: &str, kind: &str, old: &str, new: &str) -> Result<String, String> {
    let splices: Vec<Splice> = pointers(source)
        .into_iter()
        .filter(|(k, id, _, _)| *k == kind && id == old)
        .filter_map(|(_, _, pos, attr)| edit::set_attr_at(source, pos, attr, Some(new)))
        .collect();
    edit::apply(source, &splices)
}

fn attrs_xml(attrs: &[(String, String)]) -> String {
    attrs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!(" {k}=\"{}\"", edit::escape_attr(v, '"'))).collect()
}

fn chooses_xml(chooses: &[(String, String)]) -> Vec<String> {
    chooses
        .iter()
        .map(|(c, o)| format!("<choose choice=\"{}\" option=\"{}\"/>", edit::escape_attr(c, '"'), edit::escape_attr(o, '"')))
        .collect()
}

/// A whole entry's XML (children indented one level).
fn entry_xml(e: &RefEntry, unit: &str) -> String {
    let attrs = attrs_xml(&e.attrs);
    if e.tag == "data" && !e.body.is_empty() {
        return format!("<data{attrs}>{}</data>", e.body.trim());
    }
    if e.tag == "monster" && !e.chooses.is_empty() {
        let kids: Vec<String> = chooses_xml(&e.chooses).into_iter().map(|c| format!("{unit}{c}")).collect();
        return format!("<monster{attrs}>\n{}\n</monster>", kids.join("\n"));
    }
    format!("<{}{attrs}/>", e.tag)
}

/// Attribute edits: removed, changed, then added in the model's order.
fn attr_splices(source: &str, n: &StructNode, old: &RefEntry, new: &RefEntry) -> Vec<Splice> {
    let mut out = Vec::new();
    for (k, _) in &old.attrs {
        if new.get(k).is_empty() {
            out.extend(edit::set_attr(source, n, k, None));
        }
    }
    for (k, v) in &new.attrs {
        if !v.is_empty() && old.get(k) != *v {
            out.extend(edit::set_attr(source, n, k, Some(v)));
        }
    }
    out
}

/// Content edits: the data body and a monster's picks.
fn content_splices(source: &str, nodes: &[StructNode], ni: usize, new: &RefEntry) -> Result<Vec<Splice>, String> {
    let n = &nodes[ni];
    let old = read_entry(source, nodes, n);
    let mut out = Vec::new();
    // Data body.
    if n.tag == "data" && new.body.trim() != old.body.trim() {
        let tag = edit::start_tag(source, n.start).ok_or("bad <data> tag")?;
        if new.body.trim().is_empty() && !tag.self_closing {
            let (a, b) = edit::inner_span(source, n).ok_or("bad <data>")?;
            out.push(Splice { start: a, end: b, text: String::new() });
        } else if !new.body.trim().is_empty() {
            out.extend(crate::block::set_inner(source, n, new.body.trim()));
        }
    }
    // Monster picks: the children are rewritten when they change.
    if n.tag == "monster" && new.chooses != old.chooses {
        let tag = edit::start_tag(source, n.start).ok_or("bad <monster> tag")?;
        let ind = edit::line_indent(source, n.start).to_string();
        let unit = edit::indent_unit(source);
        let nl = if source.contains("\r\n") { "\r\n" } else { "\n" };
        let kids = chooses_xml(&new.chooses);
        let inner = if kids.is_empty() {
            String::new()
        } else {
            format!("{nl}{}{nl}{ind}", kids.iter().map(|k| format!("{ind}{unit}{k}")).collect::<Vec<_>>().join(nl))
        };
        if tag.self_closing {
            if !kids.is_empty() {
                out.push(Splice { start: tag.close_at, end: tag.end, text: format!(">{inner}</monster>") });
            }
        } else {
            let (a, b) = edit::inner_span(source, n).ok_or("bad <monster>")?;
            if kids.is_empty() {
                // <monster ...></monster> -> <monster .../>
                out.push(Splice { start: tag.close_at, end: n.end, text: "/>".into() });
            } else {
                out.push(Splice { start: a, end: b, text: inner });
            }
        }
    }
    Ok(out)
}

/// Where a new `<reference>` goes (schema order): right before a supplement's
/// `<chapterOrder>`, or last in a magic item (after its `<description>`).
fn place_block(source: &str, nodes: &[StructNode], block: &str) -> Result<Splice, String> {
    let root = &nodes[0];
    if root.tag == "magicItem" || root.tag == "spell" {
        let last = root.children.last().map(|c| &nodes[*c]).ok_or("the item is empty")?;
        return edit::insert_child(source, Anchor::After(last), block).ok_or_else(|| "can't place <reference>".to_string());
    }
    let order = root.children.iter().map(|c| &nodes[*c]).find(|n| n.tag == "chapterOrder").ok_or("the supplement has no <chapterOrder>")?;
    let mut s = edit::insert_child(source, Anchor::Before(order), block).ok_or("can't place <reference>")?;
    s.text.push('\n');
    Ok(s)
}

pub fn plan(source: &str, change: &RefEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let unit = edit::indent_unit(source);
    match change {
        RefEdit::Update { tag, id, entry } => {
            let ni = find(source, &nodes, tag, id).ok_or_else(|| format!("there's no <{tag} id=\"{id}\"> in <reference>"))?;
            // Content first, then attributes against the result: a monster's
            // self-closing rewrite and an attribute edit can share its start tag.
            let text = edit::apply(source, &content_splices(source, &nodes, ni, entry)?)?;
            let nodes2 = parse(&text).map_err(|e| e.message)?.nodes;
            let ni2 = find(&text, &nodes2, tag, id).ok_or("lost the entry")?;
            let old = read_entry(&text, &nodes2, &nodes2[ni2]);
            let text = edit::apply(&text, &attr_splices(&text, &nodes2[ni2], &old, entry))?;
            let new_id = entry.id();
            if new_id != *id {
                rename_pointers(&text, tag, id, &new_id)
            } else {
                Ok(text)
            }
        }
        RefEdit::Add { entry } => {
            let xml = entry_xml(entry, unit);
            match reference_node(&nodes) {
                Some(r) => {
                    let s = match nodes[r].children.last() {
                        Some(last) => edit::insert_child(source, Anchor::After(&nodes[*last]), &xml),
                        None => edit::insert_child(source, Anchor::Into(&nodes[r]), &xml),
                    }
                    .ok_or("can't place the entry")?;
                    edit::apply(source, &[s])
                }
                None => {
                    let block = format!("<reference>\n{}\n</reference>", xml.lines().map(|l| format!("{unit}{l}")).collect::<Vec<_>>().join("\n"));
                    edit::apply(source, &[place_block(source, &nodes, &block)?])
                }
            }
        }
        RefEdit::CreateBlock => {
            if reference_node(&nodes).is_some() {
                return Ok(source.to_string());
            }
            edit::apply(source, &[place_block(source, &nodes, "<reference/>")?])
        }
        RefEdit::Delete { tag, id } => {
            let ni = find(source, &nodes, tag, id).ok_or_else(|| format!("there's no <{tag} id=\"{id}\">"))?;
            edit::apply(source, &[edit::remove_element(source, &nodes[ni])])
        }
        RefEdit::Move { tag, id, before } => {
            let ni = find(source, &nodes, tag, id).ok_or_else(|| format!("there's no <{tag} id=\"{id}\">"))?;
            let target = match before {
                Some((t, i)) => Some(find(source, &nodes, t, i).ok_or_else(|| format!("there's no <{t} id=\"{i}\">"))?),
                None => None,
            };
            if target == Some(ni) {
                return Ok(source.to_string());
            }
            let r = reference_node(&nodes).ok_or("no <reference>")?;
            let kids = &nodes[r].children;
            if target.is_none() && kids.last() == Some(&ni) {
                return Ok(source.to_string());
            }
            if let Some(t) = target {
                let pos = kids.iter().position(|k| *k == t);
                if pos.is_some_and(|p| p > 0 && kids[p - 1] == ni) {
                    return Ok(source.to_string()); // already just before it
                }
            }
            edit::apply(source, &edit::move_element(source, &nodes[ni], target.map(|t| &nodes[t]), kids.last().map(|l| &nodes[*l]))?)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement>
    <chapter name="c">
        <chapterTitle>C</chapterTitle>
        <section name="s">
            <p>The <monsterRef ref="gob"/> and <monster ref="gob"/> cast <spellRef ref="sleep"/>, see <dataRef ref="toll" value="value"/>.</p>
            <image id="art"/>
            <monsterStatblock ref="gob"/>
        </section>
    </chapter>
    <reference>
        <!-- Monsters -->
        <monster id="gob" name="Goblin" cr="1/4" src="MM"/>
        <monster id="spirit" file="spirit.xml" monster="hearth" summonerRef="gob" spellLevel="2">
            <choose choice="warmth" option="fire"/>
        </monster>
        <!-- Spells -->
        <spell id="sleep" file="sleep.xml"/>  <!-- SRD -->
        <item id="sleep" name="Sleep Potion"/>
        <image id="art" src="art.png" name="Art"/>
        <data id="toll" value="5 gp"/>
        <data id="warn">Beware &amp; flee.</data>
    </reference>
    <chapterOrder><ch name="c"/></chapterOrder>
</supplement>
"#;

    fn entries(src: &str) -> Vec<RefEntry> {
        let nodes = parse(src).unwrap().nodes;
        read(src, &nodes).into_iter().map(|(_, e)| e).collect()
    }

    fn entry(src: &str, tag: &str, id: &str) -> RefEntry {
        entries(src).into_iter().find(|e| e.tag == tag && e.id() == id).unwrap()
    }

    #[test]
    fn reads_entries_and_labels() {
        let es = entries(SRC);
        assert_eq!(es.len(), 7);
        assert_eq!(es[0].label(), "Goblin (CR 1/4)");
        assert_eq!(es[1].label(), "hearth in spirit.xml");
        assert_eq!(es[1].chooses, vec![("warmth".to_string(), "fire".to_string())]);
        assert_eq!(es[4].label(), "Art (art.png)");
        assert_eq!(es[5].label(), "= 5 gp");
        // The body is XML: entities as written.
        assert_eq!(es[6].body, "Beware &amp; flee.");
    }

    #[test]
    fn usage_counts_every_pointer_kind() {
        let u = usage(SRC);
        let get = |k: &str, id: &str| u.get(&(k.to_string(), id.to_string())).copied().unwrap_or(0);
        assert_eq!(get("monster", "gob"), 4, "monsterRef, inline monster, statblock, summonerRef");
        assert_eq!(get("spell", "sleep"), 1);
        assert_eq!(get("item", "sleep"), 0, "a spell pointer doesn't count for the item");
        assert_eq!(get("image", "art"), 1, "the section image without src");
        assert_eq!(get("data", "toll"), 1);
    }

    #[test]
    fn unchanged_update_is_identity() {
        for e in entries(SRC) {
            let out = plan(SRC, &RefEdit::Update { tag: e.tag.clone(), id: e.id(), entry: e }).unwrap();
            assert_eq!(out, SRC);
        }
    }

    #[test]
    fn rename_cascades_to_the_right_kind_only() {
        let mut e = entry(SRC, "spell", "sleep");
        e.set("id", "slumber".into());
        let out = plan(SRC, &RefEdit::Update { tag: "spell".into(), id: "sleep".into(), entry: e }).unwrap();
        assert!(out.contains(r#"<spell id="slumber" file="sleep.xml"/>  <!-- SRD -->"#));
        assert!(out.contains(r#"<spellRef ref="slumber"/>"#));
        assert!(out.contains(r#"<item id="sleep" name="Sleep Potion"/>"#), "the item of the same id stays");

        let mut g = entry(SRC, "monster", "gob");
        g.set("id", "goblin".into());
        let out = plan(SRC, &RefEdit::Update { tag: "monster".into(), id: "gob".into(), entry: g }).unwrap();
        assert_eq!(usage(&out).get(&("monster".into(), "goblin".into())), Some(&4));
        assert!(out.contains(r#"summonerRef="goblin""#));
    }

    #[test]
    fn attributes_body_and_chooses() {
        let mut e = entry(SRC, "monster", "gob");
        e.set("cr", String::new());
        e.set("displayName", "Snik".into());
        e.chooses.push(("form".into(), "wolf".into()));
        let out = plan(SRC, &RefEdit::Update { tag: "monster".into(), id: "gob".into(), entry: e.clone() }).unwrap();
        assert!(out.contains("        <monster id=\"gob\" name=\"Goblin\" src=\"MM\" displayName=\"Snik\">\n            <choose choice=\"form\" option=\"wolf\"/>\n        </monster>"), "{out}");
        assert_eq!(entry(&out, "monster", "gob"), e);

        let mut s = entry(SRC, "monster", "spirit");
        s.chooses.clear();
        let out = plan(SRC, &RefEdit::Update { tag: "monster".into(), id: "spirit".into(), entry: s }).unwrap();
        assert!(out.contains(r#"<monster id="spirit" file="spirit.xml" monster="hearth" summonerRef="gob" spellLevel="2"/>"#), "{out}");

        let mut d = entry(SRC, "data", "toll");
        d.body = "Five <b>gold</b> &amp; more".into();
        let out = plan(SRC, &RefEdit::Update { tag: "data".into(), id: "toll".into(), entry: d }).unwrap();
        assert!(out.contains(r#"<data id="toll" value="5 gp">Five <b>gold</b> &amp; more</data>"#), "{out}");
    }

    #[test]
    fn add_delete_move() {
        let mut n = RefEntry::new("noun");
        n.set("id", "mill".into());
        n.set("name", "Reed Mill".into());
        n.set("kind", "place".into());
        let out = plan(SRC, &RefEdit::Add { entry: n }).unwrap();
        assert!(out.contains("        <data id=\"warn\">Beware &amp; flee.</data>\n        <noun id=\"mill\" name=\"Reed Mill\" kind=\"place\"/>\n    </reference>"));

        let out = plan(SRC, &RefEdit::Delete { tag: "item".into(), id: "sleep".into() }).unwrap();
        assert!(!out.contains("Sleep Potion"));

        // Move the spell (and its trailing comment) to the top; the "Spells" comment stays.
        let out = plan(SRC, &RefEdit::Move {
            tag: "spell".into(),
            id: "sleep".into(),
            before: Some(("monster".into(), "gob".into())),
        })
        .unwrap();
        let ids: Vec<_> = entries(&out).iter().map(|e| format!("{}:{}", e.tag, e.id())).collect();
        assert_eq!(ids[0], "spell:sleep");
        assert!(out.contains("<!-- Monsters -->\n        <spell id=\"sleep\" file=\"sleep.xml\"/>  <!-- SRD -->\n        <monster id=\"gob\""), "{out}");
        // To the end.
        let out = plan(SRC, &RefEdit::Move { tag: "monster".into(), id: "spirit".into(), before: None }).unwrap();
        assert_eq!(entries(&out).last().unwrap().id(), "spirit");
        assert!(out.contains("<data id=\"warn\">Beware &amp; flee.</data>\n        <monster id=\"spirit\""));
    }

    #[test]
    fn add_creates_the_block() {
        let src = "<supplement>\n    <chapter name=\"c\"><chapterTitle>C</chapterTitle></chapter>\n    <chapterOrder><ch name=\"c\"/></chapterOrder>\n</supplement>";
        let mut d = RefEntry::new("data");
        d.set("id", "x".into());
        let out = plan(src, &RefEdit::Add { entry: d }).unwrap();
        assert!(out.contains("    <reference>\n        <data id=\"x\"/>\n    </reference>\n\n    <chapterOrder>"), "{out}");
        // An empty block, then an entry added into it.
        let out = plan(src, &RefEdit::CreateBlock).unwrap();
        assert!(out.contains("    <reference/>\n\n    <chapterOrder>"), "{out}");
        let mut d = RefEntry::new("data");
        d.set("id", "y".into());
        let out = plan(&out, &RefEdit::Add { entry: d }).unwrap();
        assert!(out.contains("    <reference>\n        <data id=\"y\"/>\n    </reference>"), "{out}");
    }
}
