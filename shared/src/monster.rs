//! Monster files: a `<monsters>` family (name, shortNoun, lore, lairs) holding one or
//! more `<monster>` stat blocks. This first pass edits elements as raw XML (see
//! [`crate::element`]) and adds, deletes and reorders the stat blocks.

use serde::{Deserialize, Serialize};

use crate::edit::{self, escape_attr, escape_text, Anchor};
use crate::xml::parse;
use crate::StructNode;

/// The elements the monster tree opens; every other element is a leaf there.
pub const TREE_OPEN: &[&str] =
    &["monsters", "monster", "lair", "effects", "traits", "actions", "bonusActions", "reactions", "legendaryActions", "formulas"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum MonsterEdit {
    /// A new stat block after the one with id `after` (or after the last).
    Add { after: Option<String>, id: String, name: String },
    Delete { id: String },
    /// Move a stat block before another, or to the end when `before` is None.
    Move { id: String, before: Option<String> },
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

/// The `<monster>` nodes, in file order.
pub fn monster_nodes(nodes: &[StructNode]) -> Vec<usize> {
    nodes.first().map(|r| r.children.iter().copied().filter(|c| nodes[*c].tag == "monster").collect()).unwrap_or_default()
}

/// (id, name) of each stat block, in file order.
pub fn stat_blocks(source: &str, nodes: &[StructNode]) -> Vec<(String, String)> {
    monster_nodes(nodes)
        .into_iter()
        .map(|i| {
            let n = &nodes[i];
            let name = n.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == "name").map(|c| edit::text_of(source, c));
            (attr(source, n, "id").unwrap_or_default(), name.unwrap_or_default())
        })
        .collect()
}

pub fn find_monster(source: &str, nodes: &[StructNode], id: &str) -> Option<usize> {
    monster_nodes(nodes).into_iter().find(|i| attr(source, &nodes[*i], "id").as_deref() == Some(id))
}

/// A new, schema-valid stat block: a Medium creature with every stat 10, CR 0.
/// `unit` is one level of indentation; lines after the first are indented relative to it.
pub fn new_monster_xml(id: &str, name: &str, unit: &str) -> String {
    let stats: String = ["str", "dex", "con", "int", "wis", "cha"]
        .iter()
        .map(|s| format!("\n{unit}{unit}<stat name=\"{s}\" value=\"10\"/>"))
        .collect();
    let u = |n: usize| unit.repeat(n);
    format!(
        "<monster id=\"{id}\">\n\
         {u1}<name>{name}</name>\n\
         {u1}<heading>\n\
         {u2}<sizes>\n{u3}<size>Medium</size>\n{u2}</sizes>\n\
         {u2}<type>Humanoid</type>\n\
         {u2}<alignment>Neutral</alignment>\n\
         {u2}<armorClass>10</armorClass>\n\
         {u2}<initiativeBonus/>\n\
         {u2}<hitDice>1</hitDice>\n\
         {u2}<proficiencyBonus>2</proficiencyBonus>\n\
         {u2}<speeds>\n{u3}<speed id=\"ground\" name=\"ground\" value=\"30\"/>\n{u2}</speeds>\n\
         {u1}</heading>\n\
         {u1}<statBlock>{stats}\n{u1}</statBlock>\n\
         {u1}<attributes>\n\
         {u2}<challengeRatings>\n{u3}<challengeRating value=\"0\"/>\n{u2}</challengeRatings>\n\
         {u1}</attributes>\n\
         </monster>",
        id = escape_attr(id, '"'),
        name = escape_text(name),
        u1 = u(1),
        u2 = u(2),
        u3 = u(3),
    )
}

/// The text of a new monster file: the family and one stat block.
pub fn skeleton(schema_location: &str, family: &str, short_noun: &str, id: &str, name: &str) -> String {
    let unit = "    ";
    let block: String = new_monster_xml(id, name, unit).lines().map(|l| format!("{unit}{l}\n")).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <monsters xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"{loc}\">\n\
         {unit}<name>{family}</name>\n\
         {unit}<shortNoun>{noun}</shortNoun>\n\
         {block}</monsters>\n",
        loc = escape_attr(schema_location, '"'),
        family = escape_text(family),
        noun = escape_text(short_noun),
    )
}

pub fn plan(source: &str, e: &MonsterEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let mons = monster_nodes(&nodes);
    let find = |id: &str| find_monster(source, &nodes, id).ok_or_else(|| format!("no stat block \"{id}\" any more"));
    let splices = match e {
        MonsterEdit::Add { after, id, name } => {
            let id = id.trim();
            if id.is_empty() {
                return Err("a stat block needs an id".into());
            }
            if find_monster(source, &nodes, id).is_some() {
                return Err(format!("there's already a stat block \"{id}\" in this file"));
            }
            let anchor = match after {
                Some(a) => find(a)?,
                None => *mons.last().ok_or("the file has no stat block to add after")?,
            };
            let xml = new_monster_xml(id, name.trim(), edit::indent_unit(source));
            vec![edit::insert_child(source, Anchor::After(&nodes[anchor]), &xml).ok_or("can't place the stat block")?]
        }
        MonsterEdit::Delete { id } => {
            if mons.len() <= 1 {
                return Err("a monster file needs at least one stat block".into());
            }
            vec![edit::remove_element(source, &nodes[find(id)?])]
        }
        MonsterEdit::Move { id, before } => {
            let n = find(id)?;
            let pos = mons.iter().position(|m| *m == n).unwrap();
            let target = match before {
                Some(b) => Some(find(b)?),
                None => None,
            };
            let unchanged = match target {
                Some(t) => t == n || mons.get(pos + 1) == Some(&t),
                None => pos + 1 == mons.len(),
            };
            if unchanged {
                return Ok(source.to_string());
            }
            edit::move_element(source, &nodes[n], target.map(|t| &nodes[t]), mons.last().map(|l| &nodes[*l]))?
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
    <monster id="a">
        <name>A</name>
    </monster>
    <monster id="b">
        <name>B</name>
    </monster>
</monsters>
"#;

    fn ids(s: &str) -> Vec<String> {
        stat_blocks(s, &parse(s).unwrap().nodes).into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn tree_stops_at_entries() {
        let s = r#"<monsters><name>F</name><monster id="m"><name>M</name><heading><sizes><size>Medium</size></sizes></heading>
            <traits><trait name="Keen"><description>x</description></trait></traits></monster></monsters>"#;
        let p = parse(s).unwrap();
        let tags: Vec<_> = p.nodes.iter().map(|n| n.tag.as_str()).collect();
        assert_eq!(tags, ["monsters", "name", "monster", "name", "heading", "traits", "trait"]);
        assert!(p.nodes[4].leaf && p.nodes[6].leaf);
        assert_eq!(p.nodes[2].label, "M · m");
        assert_eq!(p.nodes[6].label, "Keen");
    }

    #[test]
    fn add_move_delete() {
        let out = plan(SRC, &MonsterEdit::Add { after: Some("a".into()), id: "c".into(), name: "C & D".into() }).unwrap();
        assert_eq!(ids(&out), ["a", "c", "b"]);
        assert!(out.contains("    <monster id=\"c\">\n        <name>C &amp; D</name>\n        <heading>"), "{out}");
        assert!(plan(SRC, &MonsterEdit::Add { after: None, id: "a".into(), name: String::new() }).is_err());
        let out = plan(SRC, &MonsterEdit::Move { id: "b".into(), before: Some("a".into()) }).unwrap();
        assert_eq!(ids(&out), ["b", "a"]);
        let out = plan(SRC, &MonsterEdit::Move { id: "a".into(), before: None }).unwrap();
        assert_eq!(ids(&out), ["b", "a"]);
        let out = plan(SRC, &MonsterEdit::Delete { id: "a".into() }).unwrap();
        assert_eq!(ids(&out), ["b"]);
        assert!(plan(&out, &MonsterEdit::Delete { id: "b".into() }).is_err());
    }

    #[test]
    fn skeleton_parses() {
        let s = skeleton("../schemas/monster-template-schema.xsd", "Rats", "rat", "giant-rat", "Giant Rat");
        let p = parse(&s).unwrap();
        assert_eq!(p.root_tag, "monsters");
        assert_eq!(ids(&s), ["giant-rat"]);
    }
}
