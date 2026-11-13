//! Names and the things they name, across files: every spell, monster stat block and
//! magic item a file defines, and every `<reference>` entry's name, displayName and
//! shortName. Each name is tied to its *target* (what it refers to), so two entries for
//! the same character ("Thistlewood" in two files, both naming the apprentice-wizard
//! stat block) agree, while a shortName that reads like another thing's name ("Flametongue"
//! for a dragon, against the item "Flame Tongue") clashes.

use serde::{Deserialize, Serialize};

use crate::{edit, reference, xml};

/// A name, compared loosely: letters and digits only, lowercased.
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// One name in one file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameUse {
    /// "monster", "noun", "spell" or "item".
    pub kind: String,
    /// "name", "displayName" or "shortName".
    pub form: String,
    pub text: String,
    /// What it names: "monster|<abs file>|<stat block id>", "spell|<abs file>",
    /// "item|<abs file>", or for a book thing or a noun "<kind>|book|<norm name>".
    pub target: String,
    /// The file it's written in (absolute).
    pub file: String,
    /// For a `<reference>` entry: its id; empty for a definition.
    pub entry: String,
    /// 1-based line of the element it's on.
    pub line: usize,
}

/// A thing's own definition (a spell file, an item file, a stat block).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Def {
    pub kind: String,
    pub id: String,
    pub name: String,
    pub src: String,
    pub target: String,
}

fn book_target(kind: &str, name: &str) -> String {
    format!("{kind}|book|{}", norm(name))
}

/// The definitions and names in one file. `resolve` turns a `file` attribute (relative
/// to this file) into an absolute, canonical path; `this` is the file's own path.
pub fn scan(source: &str, this: &str, resolve: &dyn Fn(&str) -> String) -> (Vec<Def>, Vec<NameUse>) {
    let Ok(p) = xml::parse(source) else { return (vec![], vec![]) };
    let nodes = &p.nodes;
    let Some(root) = nodes.first() else { return (vec![], vec![]) };
    let attr = |n: &crate::StructNode, k: &str| edit::start_tag(source, n.start).and_then(|t| t.value(k)).unwrap_or_default();
    let name_of = |n: &crate::StructNode| {
        n.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == "name").map(|c| edit::plain_text(source, c)).unwrap_or_default()
    };
    let mut defs = Vec::new();
    let mut uses = Vec::new();
    let def = |kind: &str, n: &crate::StructNode, target: String, defs: &mut Vec<Def>, uses: &mut Vec<NameUse>| {
        let name = name_of(n);
        if name.is_empty() {
            return;
        }
        defs.push(Def { kind: kind.into(), id: attr(n, "id"), name: name.clone(), src: attr(n, "src"), target: target.clone() });
        uses.push(NameUse { kind: kind.into(), form: "name".into(), text: name, target, file: this.into(), entry: String::new(), line: n.line });
    };
    match root.tag.as_str() {
        "spell" => def("spell", root, format!("spell|{this}"), &mut defs, &mut uses),
        "magicItem" => def("item", root, format!("item|{this}"), &mut defs, &mut uses),
        "monsters" => {
            for m in root.children.iter().map(|c| &nodes[*c]).filter(|n| n.tag == "monster") {
                def("monster", m, format!("monster|{this}|{}", attr(m, "id")), &mut defs, &mut uses);
            }
        }
        _ => {}
    }
    for (ni, e) in reference::read(source, nodes) {
        let kind = e.tag.clone();
        if !["monster", "noun", "spell", "item"].contains(&kind.as_str()) {
            continue;
        }
        let file = e.get("file");
        let target = match (kind.as_str(), file.is_empty()) {
            ("monster", false) => format!("monster|{}|{}", resolve(&file), e.get("monster")),
            (_, false) => format!("{kind}|{}", resolve(&file)),
            _ => book_target(&kind, &e.get("name")),
        };
        for form in ["name", "displayName", "shortName"] {
            let text = e.get(form);
            if !text.trim().is_empty() {
                uses.push(NameUse {
                    kind: kind.clone(),
                    form: form.into(),
                    text,
                    target: target.clone(),
                    file: this.into(),
                    entry: e.id(),
                    line: nodes[ni].line,
                });
            }
        }
    }
    (defs, uses)
}

fn what(u: &NameUse) -> String {
    let form = match u.form.as_str() {
        "shortName" => "short name",
        "displayName" => "display name",
        _ => "name",
    };
    if u.entry.is_empty() {
        format!("the {} {form} \u{201c}{}\u{201d}", u.kind, u.text)
    } else {
        format!("the {form} \u{201c}{}\u{201d} of the {} entry \u{201c}{}\u{201d}", u.text, u.kind, u.entry)
    }
}

/// The names a shortName collides with: the same loose name on a different target.
pub fn clashes<'a>(short: &NameUse, all: impl IntoIterator<Item = &'a NameUse>) -> Vec<&'a NameUse> {
    let n = norm(&short.text);
    // A template's placeholder ("[NPC Creator Short Name]") isn't a name.
    let t = short.text.trim();
    if n.is_empty() || (t.starts_with('[') && t.ends_with(']')) {
        return vec![];
    }
    all.into_iter().filter(|u| u.target != short.target && norm(&u.text) == n).collect()
}

/// A message for a shortName and what it collides with. `here` is the file the short
/// name is in, so collisions in it say "this file".
pub fn message(short: &NameUse, with: &[&NameUse], here: &str) -> String {
    let others: Vec<String> = with
        .iter()
        .take(3)
        .map(|u| {
            let place = if u.file == here { "in this file".to_string() } else { format!("in {}", short_path(&u.file)) };
            format!("{} {place}", what(u))
        })
        .collect();
    let more = if with.len() > 3 { format!(" (and {} more)", with.len() - 3) } else { String::new() };
    format!(
        "The short name \u{201c}{}\u{201d} of the {} entry \u{201c}{}\u{201d} reads the same as {}{more}. Short names must not clash with another thing's name.",
        short.text,
        short.kind,
        short.entry,
        others.join("; ")
    )
}

fn short_path(p: &str) -> String {
    let p = p.replace('\\', "/");
    let parts: Vec<&str> = p.split('/').collect();
    parts[parts.len().saturating_sub(3)..].join("/")
}

/// The shortNames in `new` that are new or changed since `old` (both from one file's
/// scan), by entry: only those are checked when an edit is applied.
pub fn changed_short_names<'a>(old: &[NameUse], new: &'a [NameUse]) -> Vec<&'a NameUse> {
    new.iter()
        .filter(|u| u.form == "shortName")
        .filter(|u| !old.iter().any(|o| o.form == "shortName" && o.entry == u.entry && o.text == u.text && o.target == u.target))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: &str = r#"<magicItem id="flame-tongue"><name>Flame Tongue</name></magicItem>"#;
    const SUPP: &str = r#"<supplement><reference>
        <monster id="red" name="Red Dragon" cr="17" shortName="Flametongue"/>
        <monster id="corvin" file="m.xml" monster="wiz" shortName="Thistlewood"/>
        <noun id="tamsin" name="Tamsin Reed" shortName="Tamsin"/>
    </reference></supplement>"#;
    const OTHER: &str = r#"<magicItem id="book"><name>Book</name><reference>
        <monster id="c" file="../m.xml" monster="wiz" shortName="thistle wood"/>
    </reference></magicItem>"#;

    #[test]
    fn finds_clashes_by_target() {
        let r = |f: &str| format!("/p/{}", f.trim_start_matches("../"));
        let (defs, item) = scan(ITEM, "/p/flame-tongue.xml", &r);
        assert_eq!(defs[0].target, "item|/p/flame-tongue.xml");
        let (_, supp) = scan(SUPP, "/p/a/s.xml", &r);
        let (_, other) = scan(OTHER, "/p/a/b.xml", &r);
        let all: Vec<&NameUse> = item.iter().chain(&supp).chain(&other).collect();
        let short = |e: &str| supp.iter().find(|u| u.form == "shortName" && u.entry == e).unwrap();
        // "Flametongue" reads as the item "Flame Tongue".
        let c = clashes(short("red"), all.iter().copied());
        assert_eq!(c.len(), 1, "{c:?}");
        assert!(message(short("red"), &c, "/p/a/s.xml").contains("Flame Tongue"));
        // Two entries for the same stat block agree, however they're spaced.
        assert!(clashes(short("corvin"), all.iter().copied()).is_empty());
        assert!(clashes(short("tamsin"), all.iter().copied()).is_empty());
        // Only changed short names are rechecked.
        let mut edited = supp.clone();
        edited.iter_mut().find(|u| u.entry == "tamsin" && u.form == "shortName").unwrap().text = "Tam".into();
        let ch = changed_short_names(&supp, &edited);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].text, "Tam");
    }
}
