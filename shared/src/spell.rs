//! Spell files: one `<spell>`. Its header (name, level, school, ritual, classes), its
//! casting lines (casting times, range, components, duration), its description
//! (paragraphs, lists, tables), At Higher Levels, and its `<reference>` are edited through
//! [`crate::statblock`] with the file's root as the base ([`crate::statblock::ROOT`]).

use crate::edit::{escape_attr, escape_text};
use crate::statblock::{Field, InlineSpec, AREAS, AREA_UNITS, CONDITIONS, DAMAGE_TYPES, HAZARDS, STATS};

pub const LEVELS: &[&str] = &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
pub const SCHOOLS: &[&str] = &["Abjuration", "Conjuration", "Divination", "Enchantment", "Evocation", "Illusion", "Necromancy", "Transmutation"];
pub const CASTING_UNITS: &[&str] = &["action", "bonus-action", "reaction", "minute", "hour"];
pub const RANGE_KINDS: &[&str] = &["self", "touch", "ranged", "sight", "unlimited", "special"];
pub const DISTANCE_UNITS: &[&str] = &["feet", "miles"];
pub const DURATION_UNITS: &[&str] = &[
    "instantaneous", "round", "minute", "hour", "day", "until-dispelled", "until-dispelled-or-triggered", "until-triggered", "special",
    "up-to-round", "up-to-minute", "up-to-hour", "up-to-day",
];
pub const CURRENCIES: &[&str] = &["cp", "sp", "ep", "gp", "pp"];
/// The reference kinds the spell page's Add menu offers: what a spell links out to.
/// (Spell and data entries already in a file stay editable.)
pub const REF_KINDS: &[&str] = &["monster", "item"];
/// The description's blocks.
pub const BLOCKS: &[&str] = &["p", "list", "table"];

const DICE: &[(&str, Field, bool)] = &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Text, false)];

/// The inline tags a spell's prose takes besides its entry refs (monsterRef, spellRef,
/// itemRef, dataRef), which the ref generator inserts.
pub const INLINE: &[InlineSpec] = &[
    InlineSpec { tag: "damage", label: "Damage", attrs: &[("num", Field::Int, false), ("sides", Field::Int, false), ("mod", Field::Text, false), ("type", Field::Pick(DAMAGE_TYPES), false)], text: None },
    InlineSpec { tag: "roll", label: "Dice roll", attrs: DICE, text: None },
    InlineSpec { tag: "healing", label: "Healing", attrs: DICE, text: None },
    InlineSpec { tag: "temphp", label: "Temporary hit points", attrs: DICE, text: None },
    InlineSpec { tag: "savingthrow", label: "Saving throw", attrs: &[("stat", Field::Pick(STATS), true)], text: None },
    InlineSpec { tag: "skillcheck", label: "Ability check", attrs: &[("stat", Field::Pick(STATS), true), ("skill", Field::Text, false), ("dc", Field::Text, false), ("plural", Field::Bool, false)], text: None },
    InlineSpec { tag: "spellAttack", label: "Spell attack", attrs: &[("type", Field::Pick(&["melee", "ranged"]), true)], text: None },
    InlineSpec { tag: "area", label: "Area of effect", attrs: &[("name", Field::Pick(AREAS), true), ("size", Field::Dec, false), ("radius", Field::Dec, false), ("diameter", Field::Dec, false), ("length", Field::Dec, false), ("width", Field::Dec, false), ("height", Field::Dec, false), ("thickness", Field::Dec, false), ("unit", Field::Pick(AREA_UNITS), false), ("thicknessUnit", Field::Pick(AREA_UNITS), false)], text: None },
    InlineSpec { tag: "spellLevelRef", label: "This spell's level", attrs: &[], text: None },
    InlineSpec { tag: "condition", label: "Condition", attrs: &[], text: Some(Field::Pick(CONDITIONS)) },
    InlineSpec { tag: "hazard", label: "Hazard", attrs: &[("name", Field::Pick(HAZARDS), false)], text: Some(Field::Text) },
    InlineSpec { tag: "h", label: "Run-in heading", attrs: &[], text: Some(Field::Text) },
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

/// A new, schema-valid spell file: a draft cantrip-or-higher with one empty paragraph.
pub fn skeleton(schema_location: &str, id: &str, name: &str, level: &str, school: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <spell xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\" xsi:noNamespaceSchemaLocation=\"{loc}\" id=\"{id}\">\n\
         \x20   <name>{name}</name>\n\
         \x20   <level>{level}</level>\n\
         \x20   <school>{school}</school>\n\
         \x20   <ritual>false</ritual>\n\
         \x20   <castingTimes>\n\
         \x20       <castingTime>\n\
         \x20           <value>1</value>\n\
         \x20           <unit>action</unit>\n\
         \x20           <trigger/>\n\
         \x20       </castingTime>\n\
         \x20   </castingTimes>\n\
         \x20   <range>\n\
         \x20       <type>ranged</type>\n\
         \x20       <distance>60</distance>\n\
         \x20   </range>\n\
         \x20   <components>\n\
         \x20       <verbal>true</verbal>\n\
         \x20       <somatic>true</somatic>\n\
         \x20       <materials/>\n\
         \x20   </components>\n\
         \x20   <duration>\n\
         \x20       <concentration>false</concentration>\n\
         \x20       <value/>\n\
         \x20       <unit>instantaneous</unit>\n\
         \x20   </duration>\n\
         \x20   <description>\n\
         \x20       <p></p>\n\
         \x20   </description>\n\
         \x20   <higherLevels>\n\
         \x20       <cantripUpgrades/>\n\
         \x20       <slotUpgrades/>\n\
         \x20   </higherLevels>\n\
         \x20   <classes>\n\
         \x20       <class>Wizard</class>\n\
         \x20   </classes>\n\
         \x20   <reference/>\n\
         </spell>\n",
        loc = escape_attr(schema_location, '"'),
        id = escape_attr(id, '"'),
        name = escape_text(name),
        level = escape_text(level),
        school = escape_text(school),
    )
}
