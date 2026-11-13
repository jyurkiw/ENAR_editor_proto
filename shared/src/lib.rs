//! Types passed between the Tauri backend and the Leptos frontend, plus pure helpers
//! (re-indenting a source slice, tokenizing XML for highlighting) that either side can run.

use serde::{Deserialize, Serialize};

pub mod block;
pub mod boxes;
pub mod chapter;
pub mod cover;
pub mod display;
pub mod edit;
pub mod element;
pub mod family;
pub mod inline;
pub mod list;
pub mod monster;
pub mod reference;
pub mod table;
pub mod values;
pub mod xml;

/// The schema monster files use.
pub const MONSTER_SCHEMA: &str = "monster-template-schema.xsd";

/// What kind of file a tab holds: its root element and schema.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocKind {
    /// `<supplement>`: an adventure, campaign, rules supplement...
    #[default]
    Supplement,
    /// `<monsters>`: a family of stat blocks.
    Monsters,
}

impl DocKind {
    pub fn of_root(tag: &str) -> Option<Self> {
        match tag {
            "supplement" => Some(DocKind::Supplement),
            "monsters" => Some(DocKind::Monsters),
            _ => None,
        }
    }
    pub fn schema(self) -> &'static str {
        match self {
            DocKind::Supplement => SUPPLEMENT_SCHEMA,
            DocKind::Monsters => MONSTER_SCHEMA,
        }
    }
    pub fn root(self) -> &'static str {
        match self {
            DocKind::Supplement => "supplement",
            DocKind::Monsters => "monsters",
        }
    }
    /// "supplement", "monster file".
    pub fn noun(self) -> &'static str {
        match self {
            DocKind::Supplement => "supplement",
            DocKind::Monsters => "monster file",
        }
    }
}

/// Arguments for creating a new monster file.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewMonsterFile {
    /// The folder to create it in (the project root or one under it).
    pub parent: String,
    /// The file name, with or without ".xml".
    pub file: String,
    /// The family's <name> and <shortNoun>.
    pub family: String,
    pub short_noun: String,
    /// The first stat block's id and name.
    pub id: String,
    pub name: String,
}

/// The project settings file at the root of a project directory.
pub const PROJECT_FILE: &str = "enigma-project.toml";

/// The schema every openable file must name.
pub const SUPPLEMENT_SCHEMA: &str = "supplement-template-schema.xsd";
/// The schema's old name. The compiler still maps it to [`SUPPLEMENT_SCHEMA`], with a warning.
pub const LEGACY_SUPPLEMENT_SCHEMA: &str = "adventure-template-schema.xsd";

/// `<supplement type="...">` values from supplementTypeType, grouped as the schema documents them.
/// (group, [(type, description)])
pub const SUPPLEMENT_TYPES: &[(&str, &[(&str, &str)])] = &[
    (
        "Adventures",
        &[
            ("adventure", "A single adventure"),
            ("campaign", "A long campaign of linked adventures"),
            ("anthology", "A collection of separate adventures"),
        ],
    ),
    (
        "Places",
        &[
            ("location", "One site: a dungeon, a building, a ruin"),
            ("settlement", "A village, town or city"),
            ("region", "A stretch of land and what's in it"),
            ("setting", "A whole world or setting"),
        ],
    ),
    (
        "Game content",
        &[
            ("bestiary", "Monsters"),
            ("npcs", "Non-player characters"),
            ("spellbook", "Spells"),
            ("treasury", "Magic items and treasure"),
            ("equipment", "Mundane gear"),
            ("characterOptions", "Subclasses, species, backgrounds, feats"),
            ("faction", "An organization and its people"),
            ("rules", "New or variant rules"),
        ],
    ),
    (
        "GM tools",
        &[
            ("encounters", "Encounters and encounter tables"),
            ("traps", "Traps and hazards"),
            ("toolkit", "Tables, generators and GM aids"),
        ],
    ),
];

/// Types whose main file is `adventure.xml`. Every other type writes `supplement.xml`.
pub const ADVENTURE_TYPES: &[&str] = &["adventure", "campaign", "anthology"];

/// The main file name a new supplement of this type gets.
pub fn main_file_for_type(kind: &str) -> &'static str {
    if ADVENTURE_TYPES.contains(&kind) {
        "adventure.xml"
    } else {
        "supplement.xml"
    }
}

pub fn is_supplement_type(kind: &str) -> bool {
    SUPPLEMENT_TYPES.iter().any(|(_, types)| types.iter().any(|(t, _)| *t == kind))
}

/// What `enigma-project.toml` stores. Paths are written relative to the project root
/// when they can be, so a project folder can move with its schemas.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectConfig {
    /// The folder holding supplement-template-schema.xsd.
    pub schemas_dir: String,
    /// The encompile compiler folder (holds pyproject.toml). Validation runs in its uv project.
    pub compiler_dir: String,
}

/// An open project, with its config paths resolved to absolute paths.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ProjectInfo {
    pub root: String,
    pub name: String,
    pub schemas_dir: String,
    pub compiler_dir: String,
}

/// Result of opening a project folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum OpenProject {
    Ready { project: ProjectInfo },
    /// The folder has no enigma-project.toml (or it is unreadable). The suggestions are
    /// found by walking up from the folder, the way the compiler finds schemas/.
    NeedsConfig {
        root: String,
        suggested_schemas: Option<String>,
        suggested_compiler: Option<String>,
        problem: Option<String>,
    },
}

/// One entry in a directory listing.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
}

impl FsEntry {
    pub fn is_xml(&self) -> bool {
        !self.is_dir && self.name.to_ascii_lowercase().ends_with(".xml")
    }
}

/// An error a command reports to the user: a message, and optional detail lines
/// (validation errors, for example).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppError {
    pub message: String,
    pub details: Vec<String>,
}

impl AppError {
    pub fn new(message: impl Into<String>) -> Self {
        AppError { message: message.into(), details: Vec::new() }
    }
    pub fn with_details(message: impl Into<String>, details: Vec<String>) -> Self {
        AppError { message: message.into(), details }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// One node of the file-structure tree: an element, with its byte span in the source.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StructNode {
    pub tag: String,
    /// A short description: the chapter/section title, an id or name, or a text snippet.
    pub label: String,
    /// Byte offset of the start tag's `<`.
    pub start: usize,
    /// Byte offset just past the end tag's `>` (or the empty tag's `/>`).
    pub end: usize,
    /// 1-based line of the start tag.
    pub line: usize,
    /// Indexes into [`OpenedDoc::nodes`].
    pub children: Vec<usize>,
    /// True for elements the tree doesn't open: mixed-content blocks like `<p>`,
    /// whose inline tags aren't shown as nodes.
    pub leaf: bool,
}

/// How the schema check went.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status")]
pub enum Validation {
    /// Validated against this XSD with lxml.
    Valid { schema: String },
    /// The root and schema name were right, but the XSD validation couldn't run.
    NotRun { reason: String },
}

/// An opened, checked supplement file.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct OpenedDoc {
    pub path: String,
    /// Supplement or monster file.
    #[serde(default)]
    pub kind: DocKind,
    pub name: String,
    pub source: String,
    /// The tree, root first (index 0 is `<supplement>`).
    pub nodes: Vec<StructNode>,
    pub validation: Validation,
    pub warnings: Vec<String>,
}

/// Arguments for creating a new supplement.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewSupplement {
    /// The folder to create the supplement folder in (the project root or one under it).
    pub parent: String,
    /// The new folder's name.
    pub folder: String,
    /// A supplementTypeType value.
    pub kind: String,
    /// The cover title.
    pub title: String,
}

/// One problem in the Problems panel: a validation error, or a message about an apply.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Problem {
    /// The line in the text that was checked (the edited text, for a refused apply).
    pub line: Option<usize>,
    pub message: String,
}

/// The result of applying an edit.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ApplyResult {
    /// Validated and written; the file as it now is.
    Saved { doc: OpenedDoc },
    /// The edited file didn't validate, so nothing was written.
    Invalid { problems: Vec<Problem> },
}

/// An edit the form asks the backend to apply. The backend plans it again against the
/// file on disk (the frontend plans it too, for the live Source view).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "target")]
pub enum Change {
    Cover { model: cover::CoverModel },
    Chapter { edit: chapter::ChapterEdit },
    Reference { edit: reference::RefEdit },
    Block { edit: block::BlockEdit },
    Table { edit: table::TableEdit },
    List { edit: list::ListEdit },
    Box { edit: boxes::BoxEdit },
    /// Replace one element with new XML (any file).
    Element { edit: element::ElementEdit },
    /// Add, delete or move a stat block (monster files).
    Monster { edit: monster::MonsterEdit },
    /// The family's header and parts (monster files).
    Family { edit: family::FamilyEdit },
}

impl Change {
    pub fn plan(&self, source: &str) -> Result<String, String> {
        match self {
            Change::Cover { model } => cover::plan(source, model),
            Change::Chapter { edit } => chapter::plan(source, edit),
            Change::Reference { edit } => reference::plan(source, edit),
            Change::Block { edit } => block::plan(source, edit),
            Change::Table { edit } => table::plan(source, edit),
            Change::List { edit } => list::plan(source, edit),
            Change::Box { edit } => boxes::plan(source, edit),
            Change::Element { edit } => element::plan(source, edit),
            Change::Monster { edit } => monster::plan(source, edit),
            Change::Family { edit } => family::plan(source, edit),
        }
    }
}
