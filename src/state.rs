//! App-wide state (provided as context) and the actions that change it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::ChapterModel;
use shared::cover::CoverModel;
use shared::block::{Container, Header};
use shared::reference::RefEntry;
use shared::boxes::BoxAt;
use shared::list::ListAt;
use shared::table::{Part, TableAt};
use shared::Change;
use shared::{AppError, FsEntry, OpenProject, OpenedDoc, Problem, ProjectInfo};

use crate::api;

/// The vertical tabs on the left.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Files,
    Structure,
    /// The open stat block as it prints.
    Preview,
}

/// An opened file and its tree's parent links.
pub struct DocData {
    pub doc: OpenedDoc,
    /// Parent of each node; None for the root.
    pub parents: Vec<Option<usize>>,
}

impl DocData {
    fn new(doc: OpenedDoc) -> Self {
        let mut parents = vec![None; doc.nodes.len()];
        for (i, n) in doc.nodes.iter().enumerate() {
            for c in &n.children {
                parents[*c] = Some(i);
            }
        }
        DocData { doc, parents }
    }

    /// Root first, ending with idx.
    pub fn path_to(&self, idx: usize) -> Vec<usize> {
        let mut out = vec![idx];
        let mut cur = idx;
        while let Some(p) = self.parents.get(cur).copied().flatten() {
            out.push(p);
            cur = p;
        }
        out.reverse();
        out
    }

    /// A node's position as child indexes from the root, which survives re-parsing
    /// when edits don't add or remove its earlier siblings.
    pub fn key_of(&self, idx: usize) -> Vec<usize> {
        let path = self.path_to(idx);
        path.windows(2)
            .map(|w| self.doc.nodes[w[0]].children.iter().position(|c| *c == w[1]).unwrap_or(0))
            .collect()
    }

    /// The node at a key, or the deepest ancestor that still exists.
    pub fn idx_of(&self, key: &[usize]) -> usize {
        let mut cur = 0;
        for k in key {
            match self.doc.nodes[cur].children.get(*k) {
                Some(c) => cur = *c,
                None => break,
            }
        }
        cur
    }

    pub fn find_tag(&self, tag: &str) -> Option<usize> {
        self.doc.nodes.iter().position(|n| n.tag == tag)
    }
}

impl std::ops::Deref for DocData {
    type Target = OpenedDoc;
    fn deref(&self) -> &OpenedDoc {
        &self.doc
    }
}

/// How the main area shows an element that has an edit form.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Form,
    Source,
}

/// One open file: a tab at the top. Every field is a signal, so it's Copy.
#[derive(Clone, Copy)]
pub struct DocTab {
    pub id: u32,
    data: RwSignal<Arc<DocData>>,
    pub expanded: RwSignal<HashSet<usize>>,
    pub selected: RwSignal<usize>,
    pub mode: RwSignal<Mode>,
    /// The cover form's unapplied edits. None when it matches the file.
    pub cover_draft: RwSignal<Option<CoverModel>>,
    /// Chapter forms' unapplied edits, by the chapter's name in the file.
    pub chapter_drafts: RwSignal<HashMap<String, ChapterModel>>,
    /// Reference cards: unapplied edits by card key ("tag:id", or "new:N" for a card
    /// not in the file yet), new cards in the order added, open cards, cards whose last
    /// apply was refused, and the card to scroll to.
    pub ref_drafts: RwSignal<HashMap<String, RefEntry>>,
    pub ref_new: RwSignal<Vec<String>>,
    pub ref_open: RwSignal<HashSet<String>>,
    pub ref_failed: RwSignal<HashSet<String>>,
    pub ref_focus: RwSignal<Option<String>>,
    /// Section and sidebar forms: header drafts; paragraph drafts by (container, child
    /// index) as (the text they were made from, the new text), so a draft whose
    /// paragraph moved or changed is dropped; paragraphs whose last apply was refused;
    /// and the child to scroll to and focus.
    pub header_drafts: RwSignal<HashMap<Container, Header>>,
    pub p_drafts: RwSignal<HashMap<(Container, usize), (String, String)>>,
    pub p_failed: RwSignal<HashSet<(Container, usize)>>,
    pub block_focus: RwSignal<Option<usize>>,
    /// Table forms: id/title drafts, header drafts by (table, column) as (the text they
    /// were made from, the new text), headers whose last apply was refused, and the
    /// header to focus.
    pub table_drafts: RwSignal<HashMap<TableAt, (String, String)>>,
    pub th_drafts: RwSignal<HashMap<(TableAt, usize), (String, String)>>,
    pub th_failed: RwSignal<HashSet<(TableAt, usize)>>,
    pub th_focus: RwSignal<Option<usize>>,
    /// Cell drafts by (table, part, row, column) as (the text they were made from, the
    /// new text), and cells whose last apply was refused.
    pub cell_drafts: RwSignal<HashMap<(TableAt, Part, usize, usize), (String, String)>>,
    pub cell_failed: RwSignal<HashSet<(TableAt, Part, usize, usize)>>,
    /// List forms: item drafts by (list, item) as (the text they were made from, the new
    /// text), items whose last apply was refused, and the item to focus.
    pub li_drafts: RwSignal<HashMap<(ListAt, usize), (String, String)>>,
    pub li_failed: RwSignal<HashSet<(ListAt, usize)>>,
    pub li_focus: RwSignal<Option<usize>>,
    /// Box forms (speakAloud, dialog): drafts by (box, slot), the slot being None for
    /// the trigger and Some(n) for paragraph n, as (the text they were made from, the new
    /// text); slots whose last apply was refused; and the slot to focus.
    pub box_drafts: RwSignal<HashMap<(BoxAt, Option<usize>), (String, String)>>,
    pub box_failed: RwSignal<HashSet<(BoxAt, Option<usize>)>>,
    pub box_focus: RwSignal<Option<Option<usize>>>,
    /// The raw element editor (monster files): drafts by tree path as (the text they
    /// were made from, the new text), and paths whose last apply was refused.
    pub raw_drafts: RwSignal<HashMap<Vec<usize>, (String, String)>>,
    pub raw_failed: RwSignal<HashSet<Vec<usize>>>,
    /// The monster family's forms: drafts by key (see family_form::draft_key) as (the
    /// text they were made from, the new text), and keys whose last apply was refused.
    pub fam_drafts: RwSignal<HashMap<String, (String, String)>>,
    pub fam_failed: RwSignal<HashSet<String>>,
    /// What the file's `<reference>` entries print, for the paragraph preview: None
    /// while loading, Err when the backend couldn't read them.
    pub refs: RwSignal<Option<Result<shared::inline::RefIndex, String>>>,
    /// An apply is running.
    pub applying: RwSignal<bool>,
    /// Changes waiting for the running apply to finish.
    pub queue: StoredValue<Vec<(Change, Option<String>)>>,
    /// The Problems panel: the last refused apply's errors, and the text that was checked.
    pub problems: RwSignal<Vec<Problem>>,
    pub problem_source: RwSignal<String>,
    pub problems_open: RwSignal<bool>,
    /// The file changed on disk since the tab read it: what happened, for the notice.
    pub disk: RwSignal<Option<DiskNote>>,
}

/// What the tab did about its file changing on disk.
#[derive(Clone, Debug, PartialEq)]
pub enum DiskNote {
    /// Read again (nothing was unapplied). `gone` names the element that was selected,
    /// when the new file doesn't have it any more.
    Reloaded { gone: Option<String> },
    /// Not read again: the tab has unapplied edits, which reading it would throw away.
    Changed { edits: usize },
    /// Changed, and the new text can't be opened (not well-formed, or fails its schema).
    Broken(String),
}

impl DocTab {
    /// How many unapplied edits the tab's forms hold (drafts, and reference cards not yet
    /// added).
    pub fn unapplied(&self) -> usize {
        self.cover_draft.with_untracked(|d| usize::from(d.is_some()))
            + self.chapter_drafts.with_untracked(|d| d.len())
            + self.ref_drafts.with_untracked(|d| d.len())
            + self.ref_new.with_untracked(|d| d.len())
            + self.header_drafts.with_untracked(|d| d.len())
            + self.p_drafts.with_untracked(|d| d.len())
            + self.table_drafts.with_untracked(|d| d.len())
            + self.th_drafts.with_untracked(|d| d.len())
            + self.cell_drafts.with_untracked(|d| d.len())
            + self.li_drafts.with_untracked(|d| d.len())
            + self.box_drafts.with_untracked(|d| d.len())
            + self.raw_drafts.with_untracked(|d| d.len())
            + self.fam_drafts.with_untracked(|d| d.len())
    }

    /// Throws away every unapplied edit (before reading the file again).
    pub fn discard_drafts(&self) {
        self.cover_draft.set(None);
        self.chapter_drafts.set(Default::default());
        self.ref_drafts.set(Default::default());
        self.ref_new.set(Default::default());
        self.header_drafts.set(Default::default());
        self.p_drafts.set(Default::default());
        self.table_drafts.set(Default::default());
        self.th_drafts.set(Default::default());
        self.cell_drafts.set(Default::default());
        self.li_drafts.set(Default::default());
        self.box_drafts.set(Default::default());
        self.raw_drafts.set(Default::default());
        self.fam_drafts.set(Default::default());
    }

    /// Reads the file again from disk, keeping the tree's state and the selection where
    /// they can be kept, and says what changed.
    pub fn reload(&self) {
        let tab = *self;
        let path = self.data.with_untracked(|d| d.path.clone());
        spawn_local(async move {
            match api::open_file(&path).await {
                Ok(doc) => {
                    let old = tab.data.get_untracked();
                    let sel = tab.selected.get_untracked();
                    let (key, tag) = (old.key_of(sel), old.nodes[sel].tag.clone());
                    // Its start tag, attributes and all: the same element, not only one
                    // of the same kind in its place.
                    let open_of = |d: &DocData, i: usize| d.source[d.nodes[i].start..].split('>').next().unwrap_or("").to_string();
                    let was = open_of(&old, sel);
                    tab.discard_drafts();
                    tab.replace_doc(doc, |_| None);
                    // The selection survives if the same kind of element is still there.
                    let new = tab.data.get_untracked();
                    let at = new.idx_of(&key);
                    let kept = new.key_of(at) == key && new.nodes[at].tag == tag && open_of(&new, at) == was;
                    tab.disk.set(Some(DiskNote::Reloaded { gone: (!kept).then(|| tag) }));
                    if matches!(new.kind, shared::DocKind::Supplement | shared::DocKind::MagicItem | shared::DocKind::Spell) {
                        tab.refresh_refs();
                    }
                }
                Err(e) => tab.disk.set(Some(DiskNote::Broken(std::iter::once(e.message).chain(e.details).collect::<Vec<_>>().join(" ")))),
            }
        });
    }

    fn new(id: u32, doc: OpenedDoc) -> Self {
        DocTab {
            id,
            data: RwSignal::new(Arc::new(DocData::new(doc))),
            expanded: RwSignal::new(HashSet::from([0])),
            selected: RwSignal::new(0),
            mode: RwSignal::new(Mode::Form),
            cover_draft: RwSignal::new(None),
            chapter_drafts: RwSignal::new(HashMap::new()),
            ref_drafts: RwSignal::new(HashMap::new()),
            ref_new: RwSignal::new(Vec::new()),
            ref_open: RwSignal::new(HashSet::new()),
            ref_failed: RwSignal::new(HashSet::new()),
            ref_focus: RwSignal::new(None),
            header_drafts: RwSignal::new(HashMap::new()),
            p_drafts: RwSignal::new(HashMap::new()),
            p_failed: RwSignal::new(HashSet::new()),
            block_focus: RwSignal::new(None),
            table_drafts: RwSignal::new(HashMap::new()),
            th_drafts: RwSignal::new(HashMap::new()),
            th_failed: RwSignal::new(HashSet::new()),
            th_focus: RwSignal::new(None),
            cell_drafts: RwSignal::new(HashMap::new()),
            cell_failed: RwSignal::new(HashSet::new()),
            li_drafts: RwSignal::new(HashMap::new()),
            li_failed: RwSignal::new(HashSet::new()),
            li_focus: RwSignal::new(None),
            box_drafts: RwSignal::new(HashMap::new()),
            box_failed: RwSignal::new(HashSet::new()),
            box_focus: RwSignal::new(None),
            raw_drafts: RwSignal::new(HashMap::new()),
            raw_failed: RwSignal::new(HashSet::new()),
            fam_drafts: RwSignal::new(HashMap::new()),
            fam_failed: RwSignal::new(HashSet::new()),
            refs: RwSignal::new(None),
            disk: RwSignal::new(None),
            applying: RwSignal::new(false),
            queue: StoredValue::new(Vec::new()),
            problems: RwSignal::new(Vec::new()),
            problem_source: RwSignal::new(String::new()),
            problems_open: RwSignal::new(false),
        }
    }

    /// The file, tracked: views that read it re-render when an apply replaces it.
    pub fn doc(&self) -> Arc<DocData> {
        self.data.get()
    }

    pub fn doc_untracked(&self) -> Arc<DocData> {
        self.data.get_untracked()
    }

    pub fn path_to(&self, idx: usize) -> Vec<usize> {
        self.data.with_untracked(|d| d.path_to(idx))
    }

    /// Selects a node and opens its ancestors so it shows in the tree.
    pub fn reveal(&self, idx: usize) {
        let path = self.path_to(idx);
        self.expanded.update(|e| e.extend(path[..path.len() - 1].iter().copied()));
        self.selected.set(idx);
    }

    /// Swaps in the file as an apply left it, keeping the tree's open nodes and the
    /// selection where they were. `select` picks the element to select instead, if any.
    pub fn replace_doc(&self, doc: OpenedDoc, select: impl Fn(&DocData) -> Option<usize>) {
        let old = self.data.get_untracked();
        let open: Vec<Vec<usize>> = self.expanded.get_untracked().iter().map(|i| old.key_of(*i)).collect();
        let sel = old.key_of(self.selected.get_untracked());
        let new = Arc::new(DocData::new(doc));
        let expanded: HashSet<usize> = open.iter().map(|k| new.idx_of(k)).chain([0]).collect();
        let selected = select(&new).unwrap_or_else(|| new.idx_of(&sel));
        self.data.set(new);
        self.expanded.set(expanded);
        self.reveal(selected);
    }

    /// Reads what the `<reference>` entries print, again (after an entry changes).
    pub fn refresh_refs(&self) {
        let tab = *self;
        let path = self.data.with_untracked(|d| d.path.clone());
        spawn_local(async move {
            let r = api::ref_index(&path).await.map_err(|e| {
                std::iter::once(e.message).chain(e.details).collect::<Vec<_>>().join(" ")
            });
            tab.refs.set(Some(r));
        });
    }

    pub fn show_problems(&self, problems: Vec<Problem>, checked: String) {
        self.problems.set(problems);
        self.problem_source.set(checked);
        self.problems_open.set(true);
    }

    pub fn clear_problems(&self) {
        self.problems.set(Vec::new());
        self.problem_source.set(String::new());
    }
}

#[derive(Clone)]
pub struct Menu {
    pub x: i32,
    pub y: i32,
    pub entry: FsEntry,
}

#[derive(Clone)]
pub enum Dialog {
    /// The project folder has no usable enigma-project.toml.
    ConfigureProject {
        root: String,
        schemas: String,
        compiler: String,
        problem: Option<String>,
    },
    /// Create a supplement folder inside `parent`.
    NewSupplement { parent: String },
    NewMonsterFile { parent: String },
    NewMagicItem { parent: String },
    NewSpell { parent: String },
    /// Add a chapter to the file in this tab.
    NewChapter { tab: u32 },
    /// Every mention of the `cands` names in the tab's file, to make refs of (ticked
    /// ones); `all` is every name in the file, so the longest match still wins.
    PillAll { tab: u32, title: String, cands: Vec<shared::pills::Candidate>, all: Vec<shared::pills::Candidate> },
}

#[derive(Clone, Copy)]
pub struct AppCtx {
    pub project: RwSignal<Option<ProjectInfo>>,
    pub tabs: RwSignal<Vec<DocTab>>,
    pub active: RwSignal<Option<u32>>,
    pub side: RwSignal<Side>,
    pub error: RwSignal<Option<AppError>>,
    /// A message shown while something slow (validation) runs.
    pub busy: RwSignal<Option<String>>,
    /// Bumped to make the file tree re-read its open folders.
    pub fs_version: RwSignal<u32>,
    /// Folders open in the file tree, by path, so a refresh keeps them open.
    pub open_dirs: RwSignal<HashSet<String>>,
    pub menu: RwSignal<Option<Menu>>,
    pub dialog: RwSignal<Option<Dialog>>,
    /// The spells, stat blocks and items refs can be made to (read once, when the ref
    /// generator first needs it).
    pub library: RwSignal<Option<Result<Vec<shared::prose::LibEntry>, String>>>,
    next_id: StoredValue<u32>,
}

pub fn ctx() -> AppCtx {
    expect_context::<AppCtx>()
}

impl AppCtx {
    pub fn new() -> Self {
        AppCtx {
            project: RwSignal::new(None),
            tabs: RwSignal::new(Vec::new()),
            active: RwSignal::new(None),
            side: RwSignal::new(Side::Files),
            error: RwSignal::new(None),
            busy: RwSignal::new(None),
            fs_version: RwSignal::new(0),
            open_dirs: RwSignal::new(HashSet::new()),
            menu: RwSignal::new(None),
            dialog: RwSignal::new(None),
            library: RwSignal::new(None),
            next_id: StoredValue::new(1),
        }
    }

    pub fn active_tab(&self) -> Option<DocTab> {
        let id = self.active.get()?;
        self.tabs.with(|t| t.iter().find(|t| t.id == id).cloned())
    }

    pub fn show_error(&self, e: AppError) {
        self.busy.set(None);
        self.error.set(Some(e));
    }

    /// Opens a project folder: straight in if it's configured, else the settings dialog.
    pub fn open_project(self, path: String) {
        spawn_local(async move {
            match api::open_project(&path).await {
                Ok(OpenProject::Ready { project }) => self.set_project(project),
                Ok(OpenProject::NeedsConfig { root, suggested_schemas, suggested_compiler, problem }) => {
                    self.dialog.set(Some(Dialog::ConfigureProject {
                        root,
                        schemas: suggested_schemas.unwrap_or_default(),
                        compiler: suggested_compiler.unwrap_or_default(),
                        problem,
                    }))
                }
                Err(e) => self.show_error(e),
            }
        });
    }

    pub fn set_project(&self, p: ProjectInfo) {
        let changed = self.project.with_untracked(|cur| cur.as_ref().map(|c| &c.root) != Some(&p.root));
        if changed {
            self.tabs.set(Vec::new());
            self.active.set(None);
            self.side.set(Side::Files);
            self.open_dirs.set(HashSet::from([p.root.clone()]));
        }
        self.project.set(Some(p));
        self.fs_version.update(|v| *v += 1);
    }

    pub fn pick_and_open_project(self) {
        spawn_local(async move {
            let start = self.project.get_untracked().map(|p| p.root);
            if let Some(path) = api::pick_folder("Open project folder", start.as_deref()).await {
                self.open_project(path);
            }
        });
    }

    /// Opens (or switches to) an XML file. The backend checks its schema first;
    /// a file that fails isn't opened, and the error is shown.
    pub fn open_file(self, path: String) {
        let existing = self.tabs.with_untracked(|t| t.iter().find(|t| same_path(&t.doc_untracked().path, &path)).map(|t| t.id));
        if let Some(id) = existing {
            self.active.set(Some(id));
            return;
        }
        let name = path.rsplit(['/', '\\']).next().unwrap_or(&path).to_string();
        self.busy.set(Some(format!("Checking {name} against the supplement schema…")));
        spawn_local(async move {
            match api::open_file(&path).await {
                Ok(doc) => {
                    self.busy.set(None);
                    // It may have been opened by another path spelling meanwhile.
                    let dup = self.tabs.with_untracked(|t| t.iter().find(|t| same_path(&t.doc_untracked().path, &doc.path)).map(|t| t.id));
                    let id = dup.unwrap_or_else(|| {
                        let id = self.next_id.get_value();
                        self.next_id.set_value(id + 1);
                        // Files with a <reference>: read what its entries print, for the preview.
                        let is_supplement = matches!(doc.kind, shared::DocKind::Supplement | shared::DocKind::MagicItem | shared::DocKind::Spell);
                        let tab = DocTab::new(id, doc);
                        if is_supplement {
                            tab.refresh_refs();
                        }
                        self.tabs.update(|t| t.push(tab));
                        id
                    });
                    self.active.set(Some(id));
                }
                Err(e) => self.show_error(e),
            }
        });
    }

    /// Checks each open file against the disk; a changed one is read again, or, when its
    /// tab has unapplied edits, flagged instead. Runs on focus and every few seconds.
    pub fn check_disk(&self) {
        let tabs = self.tabs.get_untracked();
        for tab in tabs {
            if tab.applying.get_untracked() {
                continue;
            }
            let (path, source) = tab.data.with_untracked(|d| (d.path.clone(), d.source.clone()));
            spawn_local(async move {
                let Ok(text) = api::read_text(&path).await else { return };
                // An apply may have finished meanwhile: compare with the tab as it is now.
                let now = tab.data.try_with_untracked(|d| d.source.clone());
                if now.as_deref() != Some(source.as_str()) || text == source || tab.applying.get_untracked() {
                    return;
                }
                let edits = tab.unapplied();
                if edits == 0 {
                    tab.reload();
                } else if !matches!(tab.disk.get_untracked(), Some(DiskNote::Changed { .. })) {
                    tab.disk.set(Some(DiskNote::Changed { edits }));
                }
            });
        }
    }

    /// Closes a tab, asking first when it has unapplied edits (which closing throws away).
    pub fn close_tab_asking(&self, id: u32) {
        let app: AppCtx = *self;
        let Some(tab) = self.tabs.with_untracked(|t| t.iter().find(|t| t.id == id).copied()) else { return };
        let n = tab.unapplied();
        if n == 0 {
            app.close_tab(id);
            return;
        }
        let name = tab.doc_untracked().name.clone();
        let what = if n == 1 { "1 unapplied edit".to_string() } else { format!("{n} unapplied edits") };
        crate::dialogs::ask(format!("Close {name}? Its {what} will be lost."), move || app.close_tab(id));
    }

    pub fn close_tab(&self, id: u32) {
        let mut next = None;
        self.tabs.update(|t| {
            if let Some(i) = t.iter().position(|t| t.id == id) {
                t.remove(i);
                next = t.get(i).or_else(|| t.last()).map(|t| t.id);
            }
        });
        if self.active.get_untracked() == Some(id) {
            self.active.set(next);
        }
    }

    pub fn refresh_files(&self) {
        self.fs_version.update(|v| *v += 1);
    }
}

/// Windows paths compare case-insensitively, and either slash.
fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}
