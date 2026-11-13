//! Magic item files: one `<magicItem>`. Its header (name, type, subtypes, rarities,
//! attunement), its description (paragraphs, abilities, lists, tables, sidebars, stat
//! blocks) and its `<reference>` are edited through [`crate::statblock`] with the file's
//! root as the base ([`crate::statblock::ROOT`]) and through [`crate::reference`].

use crate::edit::{escape_attr, escape_text};

/// itemTypeType, as written and as printed.
pub const TYPES: &[(&str, &str)] = &[
    ("armor", "Armor"),
    ("potion", "Potion"),
    ("ring", "Ring"),
    ("rod", "Rod"),
    ("scroll", "Scroll"),
    ("staff", "Staff"),
    ("wand", "Wand"),
    ("weapon", "Weapon"),
    ("wondrous-item", "Wondrous Item"),
];

/// rarityType, as written and as printed.
pub const RARITIES: &[(&str, &str)] = &[
    ("common", "Common"),
    ("uncommon", "Uncommon"),
    ("rare", "Rare"),
    ("very-rare", "Very Rare"),
    ("legendary", "Legendary"),
    ("artifact", "Artifact"),
    ("varies", "Rarity Varies"),
];

/// statusType: how far along the item is.
pub const STATUSES: &[&str] = &["draft", "mechanics-locked", "prose-complete", "final"];

/// The reference entry kinds an item file has.
pub const REF_KINDS: &[&str] = &["monster", "spell", "item", "data"];

/// The blocks a description holds, and what the Add buttons call them.
pub const BLOCKS: &[(&str, &str)] = &[
    ("p", "Paragraph"),
    ("ability", "Ability"),
    ("list", "List"),
    ("table", "Table"),
    ("sidebar", "Sidebar"),
    ("monsterStatblock", "Stat block"),
];

/// The blocks an ability or a sidebar holds after its first paragraph.
pub fn blocks_in(parent: &str) -> &'static [&'static str] {
    match parent {
        "ability" => &["p", "list", "table", "monsterStatblock"],
        "sidebar" => &["p", "list", "table"],
        _ => &["p", "ability", "list", "table", "sidebar", "monsterStatblock"],
    }
}

pub fn block_label(tag: &str) -> &'static str {
    BLOCKS.iter().find(|(t, _)| *t == tag).map(|(_, l)| *l).unwrap_or("Block")
}

/// A new, schema-valid item file: a draft with one empty paragraph.
pub fn skeleton(schema_location: &str, id: &str, name: &str, item_type: &str, rarity: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <magicItem xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"{loc}\" id=\"{id}\" status=\"draft\">\n\
         \x20   <name>{name}</name>\n\
         \x20   <type>{item_type}</type>\n\
         \x20   <subtypes/>\n\
         \x20   <rarities>\n\
         \x20       <rarity>{rarity}</rarity>\n\
         \x20   </rarities>\n\
         \x20   <attunement required=\"false\"/>\n\
         \x20   <description>\n\
         \x20       <p></p>\n\
         \x20   </description>\n\
         \x20   <reference/>\n\
         </magicItem>\n",
        loc = escape_attr(schema_location, '"'),
        id = escape_attr(id, '"'),
        name = escape_text(name),
        item_type = escape_text(item_type),
        rarity = escape_text(rarity),
    )
}

use crate::statblock::{Field, InlineSpec, AREAS, AREA_UNITS, CONDITIONS, DAMAGE_TYPES, HAZARDS, STATS};

const DICE: &[(&str, Field, bool)] = &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Text, false)];

/// The inline tags an item's prose takes besides its entry refs (monsterRef, spellRef,
/// itemRef, dataRef), which the ref tools insert. Shared with the spell schema.
pub const INLINE: &[InlineSpec] = &[
    InlineSpec { tag: "damage", label: "Damage", attrs: &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Text, false), ("type", Field::Pick(DAMAGE_TYPES), false)], text: None },
    InlineSpec { tag: "roll", label: "Dice roll", attrs: DICE, text: None },
    InlineSpec { tag: "healing", label: "Healing", attrs: DICE, text: None },
    InlineSpec { tag: "temphp", label: "Temporary hit points", attrs: DICE, text: None },
    InlineSpec { tag: "condition", label: "Condition", attrs: &[], text: Some(Field::Pick(CONDITIONS)) },
    InlineSpec { tag: "savingthrow", label: "Saving throw", attrs: &[("stat", Field::Pick(STATS), true), ("dc", Field::Text, false)], text: None },
    InlineSpec { tag: "skillcheck", label: "Ability check", attrs: &[("stat", Field::Pick(STATS), true), ("skill", Field::Text, false), ("dc", Field::Text, false), ("plural", Field::Bool, false)], text: None },
    InlineSpec { tag: "spellAttack", label: "Spell attack", attrs: &[("type", Field::Pick(&["melee", "ranged"]), true)], text: None },
    InlineSpec { tag: "area", label: "Area of effect", attrs: &[("name", Field::Pick(AREAS), true), ("size", Field::Dec, false), ("radius", Field::Dec, false), ("diameter", Field::Dec, false), ("length", Field::Dec, false), ("width", Field::Dec, false), ("height", Field::Dec, false), ("thickness", Field::Dec, false), ("unit", Field::Pick(AREA_UNITS), false), ("thicknessUnit", Field::Pick(AREA_UNITS), false)], text: None },
    InlineSpec { tag: "hazard", label: "Hazard", attrs: &[("name", Field::Pick(HAZARDS), false)], text: Some(Field::Text) },
    InlineSpec { tag: "rule", label: "Rules term", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "spellName", label: "Spell name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "itemName", label: "Item name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "creatureName", label: "Creature name (not an entry)", attrs: &[("name", Field::Text, false)], text: Some(Field::Text) },
    InlineSpec { tag: "b", label: "Bold", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "i", label: "Italic", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookRef", label: "Core book", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookChapterRef", label: "Core book chapter", attrs: &[("num", Field::Int, true)], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookSectionRef", label: "Core book section", attrs: &[], text: Some(Field::Text) },
    InlineSpec { tag: "coreBookTableRef", label: "Core book table", attrs: &[], text: Some(Field::Text) },
];

/// The type values alone, for a choice.
pub const TYPE_VALUES: &[&str] = &["armor", "potion", "ring", "rod", "scroll", "staff", "wand", "weapon", "wondrous-item"];
/// The rarity values alone, for a choice.
pub const RARITY_VALUES: &[&str] = &["common", "uncommon", "rare", "very-rare", "legendary", "artifact", "varies"];
