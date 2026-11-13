//! Prose without its tags showing: inline XML read into runs of text, bold and italic
//! spans and tags (each shown as a chip with what it prints), the tags each kind of
//! file's prose takes, and what a library (SRD and project files) can offer as refs.

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::edit::{escape_attr, escape_text};
use crate::statblock::{Field, InlineSpec, AREAS, AREA_UNITS, CONDITIONS, HAZARDS, SKILLS, STATS};

/// One piece of prose.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Text(String),
    /// `<b>` or `<i>` opening or closing: formatting, not a chip.
    Open(String),
    Close(String),
    /// Any other element, whole: one chip.
    Tag { tag: String, attrs: Vec<(String, String)>, text: String, xml: String },
}

impl Tok {
    pub fn attr(&self, k: &str) -> &str {
        match self {
            Tok::Tag { attrs, .. } => attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or(""),
            _ => "",
        }
    }
}

const WRAP: &str = "<__p>";

/// Reads prose XML into pieces. A tag with markup inside (other than plain text) is one
/// chip all the same; its `text` is its plain text.
pub fn tokenize(xml: &str) -> Result<Vec<Tok>, String> {
    let src = format!("{WRAP}{xml}</__p>");
    let mut r = Reader::from_str(&src);
    r.config_mut().trim_text(false);
    let mut out = Vec::new();
    // Depth inside a chip element, with its start and plain text.
    let mut chip: Option<(usize, usize, String, Vec<(String, String)>, String)> = None;
    loop {
        let before = r.buffer_position() as usize;
        let ev = r.read_event().map_err(|e| e.to_string())?;
        let after = r.buffer_position() as usize;
        match ev {
            // Markup inside a chip: part of it.
            Event::Start(_) if chip.is_some() => {
                if let Some(c) = chip.as_mut() {
                    c.0 += 1;
                }
            }
            Event::Empty(_) if chip.is_some() => {}
            Event::Start(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if name == "__p" {
                    continue;
                }
                if name == "b" || name == "i" {
                    out.push(Tok::Open(name));
                } else {
                    chip = Some((1, before, name, attrs_of(&e), String::new()));
                }
            }
            Event::Empty(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                out.push(Tok::Tag { tag: name, attrs: attrs_of(&e), text: String::new(), xml: src[before..after].to_string() });
            }
            Event::End(e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if let Some(c) = chip.as_mut() {
                    c.0 -= 1;
                    if c.0 == 0 {
                        let (_, start, tag, attrs, text) = chip.take().unwrap();
                        out.push(Tok::Tag { tag, attrs, text, xml: src[start..after].to_string() });
                    }
                } else if name == "b" || name == "i" {
                    out.push(Tok::Close(name));
                }
            }
            Event::Text(t) => {
                let s = t.unescape().map(|c| c.into_owned()).unwrap_or_else(|_| String::from_utf8_lossy(&t).into_owned());
                match chip.as_mut() {
                    Some(c) => c.4.push_str(&s),
                    None => match out.last_mut() {
                        Some(Tok::Text(prev)) => prev.push_str(&s),
                        _ => out.push(Tok::Text(s)),
                    },
                }
            }
            Event::CData(t) => {
                let s = String::from_utf8_lossy(&t).into_owned();
                match chip.as_mut() {
                    Some(c) => c.4.push_str(&s),
                    None => out.push(Tok::Text(s)),
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

fn attrs_of(e: &quick_xml::events::BytesStart) -> Vec<(String, String)> {
    e.attributes()
        .flatten()
        .map(|a| (String::from_utf8_lossy(a.key.as_ref()).into_owned(), a.unescape_value().map(|v| v.into_owned()).unwrap_or_default()))
        .collect()
}

/// A tag's XML: attributes in order, empty ones left out, and its text if `has_text`.
pub fn tag_xml(tag: &str, attrs: &[(String, String)], text: &str, has_text: bool) -> String {
    let mut s = format!("<{tag}");
    for (k, v) in attrs.iter().filter(|(_, v)| !v.trim().is_empty()) {
        s.push_str(&format!(" {k}=\"{}\"", escape_attr(v.trim(), '"')));
    }
    if has_text || !text.is_empty() {
        format!("{s}>{}</{tag}>", escape_text(text))
    } else {
        format!("{s}/>")
    }
}

fn cap(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn stat_name(s: &str) -> &'static str {
    match s {
        "str" => "Strength",
        "dex" => "Dexterity",
        "con" => "Constitution",
        "int" => "Intelligence",
        "wis" => "Wisdom",
        "cha" => "Charisma",
        _ => "",
    }
}

fn dice(t: &Tok) -> String {
    let (num, sides, m) = (t.attr("num"), t.attr("sides"), t.attr("mod"));
    let d = match (num, sides) {
        ("", "") => String::new(),
        (n, s) => format!("{n}d{s}"),
    };
    let m = match m {
        "" => String::new(),
        "spellcasting" => " + mod".into(),
        m if m.starts_with('-') => format!(" − {}", &m[1..]),
        m => format!(" + {m}"),
    };
    let stat = match t.attr("stat") {
        "" => String::new(),
        s => format!(" + {}", s.to_uppercase()),
    };
    let s = format!("{d}{stat}{m}");
    s.trim_start_matches(" + ").to_string()
}

/// What a non-ref tag prints, briefly, for its chip. Refs are named by the page, which
/// knows the entries; for those this gives the id.
pub fn label(t: &Tok) -> String {
    let Tok::Tag { tag, text, .. } = t else { return String::new() };
    let a = |k: &str| t.attr(k);
    let txt = text.trim();
    match tag.as_str() {
        "damage" => {
            let ty = if a("type").is_empty() { String::new() } else { format!(" {}", cap(a("type"))) };
            format!("{}{ty}", dice(t)).trim().to_string()
        }
        "healing" => format!("{} HP", dice(t)),
        "temphp" => format!("{} temp HP", dice(t)),
        "roll" => dice(t),
        "condition" => cap(txt),
        "hazard" => if txt.is_empty() { cap(a("name")) } else { txt.to_string() },
        "savingthrow" => match a("dc") {
            "" => format!("{} save", stat_name(a("stat"))),
            "spell" => format!("{} save vs. spell DC", stat_name(a("stat"))),
            dc => format!("DC {dc} {} save", stat_name(a("stat"))),
        },
        "skillcheck" => {
            let skill = if a("skill").is_empty() { String::new() } else { format!(" ({})", cap(&a("skill").replace(',', ", "))) };
            match a("dc") {
                "" => format!("{}{skill} check", stat_name(a("stat"))),
                "spell" => format!("{}{skill} check vs. spell DC", stat_name(a("stat"))),
                dc => format!("DC {dc} {}{skill} check", stat_name(a("stat"))),
            }
        }
        "spellAttack" => format!("{} spell attack", a("type")),
        "area" => {
            let n = ["size", "radius", "diameter", "length"].iter().map(|k| a(k)).find(|v| !v.is_empty()).unwrap_or("");
            if n.is_empty() { cap(a("name")) } else { format!("{n}-ft {}", cap(a("name"))) }
        }
        "speed" => format!("{} Speed", cap(a("name"))),
        "shortNounRef" => "creature".into(),
        "formulaRef" => format!("formula {}", a("ref")),
        "choiceRef" => format!("choice {}", a("ref")),
        "coreBookChapterRef" => format!("Chapter {}: {txt}", a("num")),
        "coreBookSectionRef" => format!("{txt} section"),
        "coreBookTableRef" => format!("{txt} table"),
        _ if !txt.is_empty() => txt.to_string(),
        _ => {
            let id = a("ref");
            if id.is_empty() { tag.clone() } else { id.to_string() }
        }
    }
}

/// How a chip looks: an entry ref, a rules term, dice, or something else.
pub fn chip_kind(tag: &str) -> &'static str {
    match tag {
        "monsterRef" | "nounRef" | "spellRef" | "itemRef" | "dataRef" | "chapterRef" | "figureRef" | "monster" | "actionRef"
        | "attackRef" | "saveRef" | "speedRef" | "shortNounRef" | "formulaRef" | "choiceRef" => "ref",
        "damage" | "healing" | "temphp" | "roll" | "savingthrow" | "skillcheck" | "spellAttack" | "area" | "speed" => "dice",
        "condition" | "hazard" | "rule" | "spellName" | "itemName" | "creatureName" => "term",
        _ => "other",
    }
}

/// The prose of one kind of file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Palette {
    /// A monster file: a stat block's prose, or the family's lore.
    StatBlock,
    MagicItem,
    Supplement,
}

impl Palette {
    /// The tags the Insert menu offers (entry refs come from the generator's search).
    pub fn tags(self) -> &'static [InlineSpec] {
        match self {
            Palette::StatBlock => crate::statblock::INLINE,
            Palette::MagicItem => crate::item::INLINE,
            Palette::Supplement => SUPPLEMENT,
        }
    }
    /// The spec for a tag, refs included (for a chip's edit form).
    pub fn spec(self, tag: &str) -> Option<&'static InlineSpec> {
        self.tags().iter().chain(REFS.iter()).find(|s| s.tag == tag)
    }
    /// The ref tag that points at an entry of this kind, if the file's prose has one.
    pub fn ref_tag(self, kind: &str) -> Option<&'static str> {
        match (self, kind) {
            (Palette::StatBlock, _) => None,
            (_, "monster") => Some("monsterRef"),
            (_, "spell") => Some("spellRef"),
            (_, "item") => Some("itemRef"),
            (_, "data") => Some("dataRef"),
            (Palette::Supplement, "noun") => Some("nounRef"),
            _ => None,
        }
    }
    /// The term tag that names a library thing without an entry (a stat block's prose
    /// has no entries).
    pub fn term_tag(self, kind: &str) -> Option<&'static str> {
        match kind {
            "monster" => Some("creatureName"),
            "spell" => Some("spellName"),
            "item" => Some("itemName"),
            _ => None,
        }
    }
}

const EMPH: (&str, Field, bool) = ("emphasized", Field::Pick(&["true", "false"]), false);

/// Entry refs: their fields, for a chip's edit form. `ref` picks an entry by kind.
pub const REFS: &[InlineSpec] = &[
    InlineSpec { tag: "monsterRef", label: "Monster ref", attrs: &[("ref", Field::Ref(&["monster"]), true), ("value", Field::Pick(&["name", "displayName", "shortName", "cr", "src"]), true), EMPH], text: None },
    InlineSpec { tag: "nounRef", label: "Noun ref", attrs: &[("ref", Field::Ref(&["noun"]), true), ("value", Field::Pick(&["name", "displayName", "shortName"]), true), EMPH], text: None },
    InlineSpec { tag: "spellRef", label: "Spell ref", attrs: &[("ref", Field::Ref(&["spell"]), true), ("value", Field::Pick(&["name", "src"]), true), EMPH], text: None },
    InlineSpec { tag: "itemRef", label: "Item ref", attrs: &[("ref", Field::Ref(&["item"]), true), ("value", Field::Pick(&["name", "displayName", "src"]), true), EMPH], text: None },
    InlineSpec { tag: "dataRef", label: "Data ref", attrs: &[("ref", Field::Ref(&["data"]), true), ("value", Field::Pick(&["value", "body"]), true), EMPH], text: None },
    InlineSpec { tag: "chapterRef", label: "Chapter ref", attrs: &[("name", Field::Ref(&["chapter"]), true), EMPH], text: None },
    InlineSpec { tag: "figureRef", label: "Figure ref", attrs: &[("ref", Field::Ref(&["figure"]), true), EMPH], text: None },
    InlineSpec { tag: "monster", label: "Inline monster", attrs: &[("ref", Field::Ref(&["monster"]), false), ("name", Field::Text, false), ("cr", Field::Text, false), ("src", Field::Text, false), ("display", Field::Text, false), EMPH], text: None },
];

/// A supplement paragraph's tags besides its entry refs.
pub const SUPPLEMENT: &[InlineSpec] = &[
    InlineSpec { tag: "condition", label: "Condition", attrs: &[], text: Some(Field::Pick(CONDITIONS)) },
    InlineSpec { tag: "savingthrow", label: "Saving throw", attrs: &[("stat", Field::Pick(STATS), true), ("dc", Field::Text, true)], text: None },
    InlineSpec { tag: "skillcheck", label: "Ability check", attrs: &[("stat", Field::Pick(STATS), true), ("skill", Field::Pick(SKILLS), true), ("dc", Field::Int, false), ("table", Field::Bool, false)], text: None },
    InlineSpec { tag: "hazard", label: "Hazard", attrs: &[("name", Field::Pick(HAZARDS), false)], text: Some(Field::Text) },
    InlineSpec { tag: "rule", label: "Rules term", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "spellName", label: "Spell name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "itemName", label: "Item name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "creatureName", label: "Creature name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookRef", label: "Core book", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookChapterRef", label: "Core book chapter", attrs: &[("num", Field::Int, true)], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookSectionRef", label: "Core book section", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookTableRef", label: "Core book table", attrs: &[], text: Some(Field::Text) },
];

// Area and unit lists are re-exported for forms that build area tags.
pub const AREA_SHAPES: &[&str] = AREAS;
pub const UNITS: &[&str] = AREA_UNITS;

/// A spell, monster stat block or magic item a ref can be made to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibEntry {
    /// "spell", "monster" or "item".
    pub kind: String,
    /// The spell's or item's id, or the stat block's id.
    pub id: String,
    pub name: String,
    /// Absolute path of its file.
    pub file: String,
    pub src: String,
    /// From the shared SRD library (else a project file).
    pub srd: bool,
    /// What it is, as [`crate::names::NameUse::target`] says it.
    pub target: String,
    /// Other names `<reference>` entries elsewhere give it: (form, text), form being
    /// "displayName" or "shortName".
    pub aliases: Vec<(String, String)>,
}

/// `to` relative to the folder holding `from` (both absolute files), with `/`.
pub fn rel_path(from: &str, to: &str) -> String {
    let norm = |p: &str| p.replace('\\', "/");
    let (from, to) = (norm(from), norm(to));
    let fdir: Vec<&str> = from.split('/').collect::<Vec<_>>();
    let fdir = &fdir[..fdir.len().saturating_sub(1)];
    let tparts: Vec<&str> = to.split('/').collect();
    let same = fdir.iter().zip(&tparts).take_while(|(a, b)| a.eq_ignore_ascii_case(b)).count();
    if same == 0 {
        return to;
    }
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), fdir.len() - same).collect();
    out.extend(tparts[same..].iter().map(|s| s.to_string()));
    out.join("/")
}

/// Rank words for a search: how well `name` matches `q` (higher first); None: no match.
pub fn score(name: &str, q: &str) -> Option<u32> {
    let (n, q) = (name.to_lowercase(), q.trim().to_lowercase());
    if q.is_empty() {
        return Some(1);
    }
    if n == q {
        Some(100)
    } else if n.starts_with(&q) {
        Some(80)
    } else if n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) {
        Some(60)
    } else if n.contains(&q) {
        Some(40)
    } else {
        None
    }
}

/// The kind of thing a table cell's column heading suggests ("Spell" → spells).
pub fn hint_kind(hint: &str) -> Option<&'static str> {
    let h = hint.to_lowercase();
    if h.contains("spell") || h.contains("cantrip") {
        Some("spell")
    } else if h.contains("monster") || h.contains("creature") || h.contains("foe") || h.contains("encounter") {
        Some("monster")
    } else if h.contains("item") || h.contains("treasure") || h.contains("loot") {
        Some("item")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_text_format_and_tags() {
        let t = tokenize("A <b>big</b> <spellRef ref=\"sleep\" value=\"name\"/> deals <damage num=\"2\" sides=\"6\" type=\"fire\"/> &amp; <rule name=\"Magic\">a Magic action</rule>.").unwrap();
        assert_eq!(t[0], Tok::Text("A ".into()));
        assert_eq!(t[1], Tok::Open("b".into()));
        assert_eq!(t[3], Tok::Close("b".into()));
        assert!(matches!(&t[5], Tok::Tag { tag, xml, .. } if tag == "spellRef" && xml == "<spellRef ref=\"sleep\" value=\"name\"/>"));
        assert_eq!(label(&t[7]), "2d6 Fire");
        assert_eq!(t[8], Tok::Text(" & ".into()));
        assert!(matches!(&t[9], Tok::Tag { text, xml, .. } if text == "a Magic action" && xml == "<rule name=\"Magic\">a Magic action</rule>"));
        assert!(tokenize("<b>open").is_err());
    }

    #[test]
    fn labels() {
        let l = |x: &str| label(&tokenize(x).unwrap()[0]);
        assert_eq!(l("<savingthrow stat=\"dex\" dc=\"15\"/>"), "DC 15 Dexterity save");
        assert_eq!(l("<skillcheck stat=\"wis\" skill=\"perception\"/>"), "Wisdom (Perception) check");
        assert_eq!(l("<damage num=\"1\" sides=\"8\" stat=\"str\" mod=\"-1\" type=\"slashing\"/>"), "1d8 + STR − 1 Slashing");
        assert_eq!(l("<area name=\"sphere\" radius=\"20\"/>"), "20-ft Sphere");
        assert_eq!(l("<condition>prone</condition>"), "Prone");
    }

    #[test]
    fn relative_paths() {
        assert_eq!(rel_path("E:/D/adventures/x/item.xml", "E:/D/shared/internal/spells/sleep.xml"), "../../shared/internal/spells/sleep.xml");
        assert_eq!(rel_path("E:\\D\\a\\item.xml", "E:\\D\\a\\b.xml"), "b.xml");
        assert_eq!(hint_kind("Spell"), Some("spell"));
        assert!(score("Burning Hands", "hands").is_some() && score("Sleep", "fire").is_none());
    }
}

/// Do two names read the same (letters and digits only, any case)?
pub fn norm_eq(a: &str, b: &str) -> bool {
    crate::names::norm(a) == crate::names::norm(b)
}
