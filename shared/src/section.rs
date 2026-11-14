//! What a section's blocks can point at in a supplement (its reference entries, its
//! chapters' tables and sidebars, its treasure parcels), and new blocks pointing at the
//! first of each. Shared by the block pages and the tests.

use crate::statblock::{self as sb, El, StatEdit, ROOT};
use crate::Change;

/// What the file offers a block's pointers.
#[derive(Clone, Default, PartialEq)]
pub struct Targets {
    /// `<reference>` entries by kind: (id, label, file).
    pub monsters: Vec<(String, String, String)>,
    pub items: Vec<(String, String, String)>,
    pub spells: Vec<(String, String, String)>,
    pub images: Vec<(String, String, String)>,
    /// Chapter tables and sidebars: (id, label).
    pub tables: Vec<(String, String)>,
    pub sidebars: Vec<(String, String)>,
    /// Treasure parcels: (number, label).
    pub parcels: Vec<(String, String)>,
    /// Every id in the file, for fresh ones.
    pub ids: Vec<String>,
}

pub fn entry_label(e: &El) -> String {
    let n = ["name", "displayName"].iter().map(|k| e.get(k)).find(|v| !v.is_empty()).unwrap_or("");
    let n = if n.is_empty() { e.get("monster") } else { n };
    let n = if n.is_empty() { e.get("src") } else { n };
    let id = e.get("id");
    if n.is_empty() || n == id { id.to_string() } else { format!("{n} ({id})") }
}

pub fn targets(root: &El) -> Targets {
    let mut t = Targets { ids: sb::ids(root), ..Default::default() };
    if let Some(r) = root.kid("reference") {
        for e in &r.kids {
            let v = (e.get("id").to_string(), entry_label(e), e.get("file").to_string());
            match e.tag.as_str() {
                "monster" => t.monsters.push(v),
                "item" => t.items.push(v),
                "spell" => t.spells.push(v),
                "image" => t.images.push(v),
                _ => {}
            }
        }
    }
    for ch in root.all("chapter") {
        let cname = ch.get("name").to_string();
        for k in &ch.kids {
            let title = k.kid("title").map(|x| x.text.trim().to_string()).unwrap_or_default();
            let lab = if title.is_empty() { format!("{} ({cname})", k.get("id")) } else { format!("{title} ({cname})") };
            match k.tag.as_str() {
                "table" => t.tables.push((k.get("id").to_string(), lab)),
                "sidebar" => t.sidebars.push((k.get("id").to_string(), lab)),
                _ => {}
            }
        }
    }
    if let Some(tr) = root.kid("treasure") {
        for p in tr.all("parcel") {
            let n = p.get("number").to_string();
            let d = p.kid("description").map(|x| x.text.trim().chars().take(50).collect::<String>()).unwrap_or_default();
            t.parcels.push((n.clone(), if d.is_empty() { format!("Parcel {n}") } else { format!("Parcel {n}: {d}") }));
        }
    }
    t
}

pub fn first3(v: &[(String, String, String)]) -> Option<String> {
    v.first().map(|x| x.0.clone())
}

/// A new block of this kind for a section in chapter `chapter` (root child index), with
/// its pointers set to the first thing the file has. Some(changes before it) when it needs
/// a new table or sidebar first; None when there's nothing for it to point at.
pub fn new_block(tag: &str, root: &El, chapter: usize) -> Option<(Vec<Change>, El)> {
    let t = targets(root);
    let ch = |what: &str| Change::Stat { edit: StatEdit::Insert { child: ROOT, parent: vec![chapter], parent_tag: "chapter".into(), el: new_target(what, &sb::fresh(&format!("new-{what}"), &t.ids)), after: None } };
    let id_of = |c: &Change| match c {
        Change::Stat { edit: StatEdit::Insert { el, .. } } => el.get("id").to_string(),
        _ => String::new(),
    };
    Some(match tag {
        "image" => match first3(&t.images) {
            Some(id) => (vec![], El::new("image").with("id", &id)),
            None => (vec![], El::new("image").with("id", &sb::fresh("image", &t.ids)).with("src", "image.png")),
        },
        "figure" => {
            let (pre, inner) = new_block("image", root, chapter)?;
            (pre, El::new("figure").with("id", &sb::fresh("fig-new", &t.ids)).with("name", "New figure").with_kid(inner))
        }
        "tableLink" | "sidebarLink" => {
            let (list, what) = if tag == "tableLink" { (&t.tables, "table") } else { (&t.sidebars, "sidebar") };
            match list.first() {
                Some((id, _)) => (vec![], El::new(tag).with("id", id)),
                None => {
                    let c = ch(what);
                    let id = id_of(&c);
                    (vec![c], El::new(tag).with("id", &id))
                }
            }
        }
        "parcelLink" => (vec![], El::new(tag).with("number", &t.parcels.first()?.0)),
        "monsterStatblock" | "monsterEntry" => (vec![], El::new(tag).with("ref", &first3(&t.monsters)?)),
        "magicItemBlock" => (vec![], El::new(tag).with("ref", &first3(&t.items)?)),
        "spellBlock" => (vec![], El::new(tag).with("ref", &first3(&t.spells)?)),
        "skillcheckResultTable" => (vec![], El::new(tag).with_kid(El::new("result").with("dc", "10").with_kid(El::new("rd")))),
        _ => (vec![], El::new(tag)),
    })
}

/// Why a block can't be added yet, if it can't.
pub fn missing(tag: &str, t: &Targets) -> Option<&'static str> {
    match tag {
        "parcelLink" if t.parcels.is_empty() => Some("no treasure parcels"),
        "monsterStatblock" | "monsterEntry" if t.monsters.is_empty() => Some("no <reference> monsters"),
        "magicItemBlock" if t.items.is_empty() => Some("no <reference> items"),
        "spellBlock" if t.spells.is_empty() => Some("no <reference> spells"),
        _ => None,
    }
}

/// A new, empty chapter table or sidebar.
pub fn new_target(what: &str, id: &str) -> El {
    if what == "table" {
        El::new("table").with("id", id).with_kid(El::new("title").with_text("New Table")).with_kid(El::new("tableBody").with_kid(El::new("tr").with_kid(El::new("td")).with_kid(El::new("td"))))
    } else {
        El::new("sidebar").with("id", id).with_kid(El::new("title").with_text("New Sidebar")).with_kid(El::new("p"))
    }
}

