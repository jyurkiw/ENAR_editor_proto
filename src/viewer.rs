//! The main area: the selected element's edit form (for elements that have one) or its
//! source, re-indented and highlighted; and the Problems panel docked at the bottom.

use leptos::prelude::*;
use shared::cover::{self, CoverModel};
use shared::display::{reindent, tokenize};
use shared::xml::parse;
use shared::Validation;

use shared::chapter::{self, ChapterEdit};

use shared::reference::{self as refs, RefEdit};
use shared::Change;

use crate::apply::{apply, ref_key};
use shared::block::{self as blocks, Container};

use shared::table::{self as tables, TableAt};

use crate::block_form::BlockForm;
use crate::box_form::BoxForm;
use crate::monster_form::MonsterForm;
use crate::item_form::ItemPage;
use crate::spell_form::SpellPage;
use crate::license_form::LicensePage;
use crate::supplement_parts::{AddParts, PartPage};
use crate::section_el_form::SectionElPage;
use crate::list_form::ListForm;
use shared::boxes::{self as boxes, BoxAt};
use crate::table_form::TableForm;
use shared::list::{self as lists, ListAt};
use crate::chapter_form::ChapterForm;
use crate::reference_form::ReferenceForm;
use crate::cover_form::{apply_model, CoverForm};
use crate::state::{ctx, Dialog, DocData, DocTab, Mode};

/// The edit form for an element, if it has one.
#[derive(Clone, PartialEq)]
enum FormKind {
    Cover,
    /// A top-level chapter, by its name in the file.
    Chapter(String),
    /// The reference block; `Some(key)` when one of its entries was picked in the tree.
    Reference(Option<String>),
    /// A section or sidebar; `Some(child)` when one of its children was picked.
    Block(Container, Option<usize>),
    /// A table, wherever it is (a chapter, a sidebar, a sidebarLink).
    Table(TableAt),
    /// A list in a section or sidebar, or an item in it.
    List(ListAt),
    /// A speakAloud or dialog box in a section, or its trigger or a paragraph in it.
    Box(BoxAt),
    /// Any element of a monster file, by its tree path and tag (the raw editor).
    Monster(Vec<usize>, String),
    /// Anything in a magic item file: its one page.
    Item,
    /// Anything in a spell file: its one page.
    Spell,
    /// A license file: its one page.
    License,
    /// A supplement's credits, license, treasure or data (by tag), or anything in one.
    Part(String),
    /// A section's image, figure, link, stat block, monster entry, magic item, spell, skill
    /// check table or page end (or anything in one), by its key from the root.
    SectionEl(Vec<usize>),
}

fn form_kind(d: &DocData, idx: usize) -> Option<FormKind> {
    let n = d.nodes.get(idx)?;
    if d.kind == shared::DocKind::MagicItem {
        return Some(FormKind::Item);
    }
    if d.kind == shared::DocKind::Spell {
        return Some(FormKind::Spell);
    }
    if d.kind == shared::DocKind::License {
        return Some(FormKind::License);
    }
    if d.kind == shared::DocKind::Monsters {
        let key = d.key_of(idx);
        // Anything in a stat block: its page (one form for all of it).
        if let Some(&c) = key.first() {
            if let Some(m) = d.nodes[0].children.get(c).map(|i| &d.nodes[*i]) {
                if m.tag == "monster" {
                    return Some(FormKind::Monster(vec![c], "monster".into()));
                }
            }
        }
        return Some(FormKind::Monster(key, n.tag.clone()));
    }
    // A supplement's credits, license, treasure or data, or anything in one: its page.
    if let Some(&c) = d.key_of(idx).first() {
        if let Some(top) = d.nodes[0].children.get(c).map(|i| &d.nodes[*i]) {
            if crate::supplement_parts::PARTS.iter().any(|(t, _)| *t == top.tag) {
                return Some(FormKind::Part(top.tag.clone()));
            }
        }
    }
    // A section's block (other than paragraphs, lists and boxes), or anything in one.
    let mut cur = idx;
    while let Some(p) = d.parents.get(cur).copied().flatten() {
        if d.nodes[p].tag == "section" && crate::section_el_form::SECTION_BLOCKS.contains(&d.nodes[cur].tag.as_str()) {
            return Some(FormKind::SectionEl(d.key_of(cur)));
        }
        cur = p;
    }
    // A table, or anything inside one: the table editor.
    let mut cur = Some(idx);
    while let Some(i) = cur {
        if d.nodes[i].tag == "table" {
            return tables::table_at(&d.source, &d.nodes, i).map(FormKind::Table);
        }
        cur = d.parents.get(i).copied().flatten();
    }
    // A list, or one of its items: the list editor.
    let list = if n.tag == "li" { d.parents.get(idx).copied().flatten() } else { Some(idx) };
    if let Some(at) = list.and_then(|l| lists::list_at(&d.source, &d.nodes, &d.parents, l)) {
        return Some(FormKind::List(at));
    }
    // A box, or its trigger or a paragraph in it: the box editor.
    let in_box = if matches!(n.tag.as_str(), "p" | "trigger") { d.parents.get(idx).copied().flatten() } else { Some(idx) };
    if let Some(at) = in_box.and_then(|b| boxes::box_at(&d.source, &d.nodes, &d.parents, b)) {
        return Some(FormKind::Box(at));
    }
    let top = d.nodes[0].children.contains(&idx);
    match n.tag.as_str() {
        "cover" | "contents" if top => Some(FormKind::Cover),
        "chapter" if top => {
            shared::edit::start_tag(&d.source, n.start)?.value("name").map(FormKind::Chapter)
        }
        "reference" if top => Some(FormKind::Reference(None)),
        "section" | "sidebar" | "productDescription" => blocks::container_of(&d.source, &d.nodes, &d.parents, idx).map(|c| FormKind::Block(c, None)),
        _ => {
            let parent = d.parents.get(idx).copied().flatten()?;
            // A child of a section or sidebar: that container's editor, at the child.
            if let Some(c) = blocks::container_of(&d.source, &d.nodes, &d.parents, parent) {
                let child = d.nodes[parent].children.iter().position(|k| *k == idx);
                return Some(FormKind::Block(c, child));
            }
            // An entry inside <reference>: the reference editor, at that entry's card.
            let in_ref = d.nodes[parent].tag == "reference" && d.nodes[0].children.contains(&parent);
            if !in_ref {
                return None;
            }
            let id = shared::edit::start_tag(&d.source, n.start)?.value("id").unwrap_or_default();
            Some(FormKind::Reference(Some(ref_key(&n.tag, &id))))
        }
    }
}

/// The tab's unapplied edits as the changes their forms would apply: paragraphs and
/// section headers, table info, headers and cells, list items, box text, reference cards,
/// and the stat block, item, spell, section block, license and family cards.
fn text_drafts(tab: DocTab) -> Vec<Change> {
    use shared::block::BlockEdit;
    use shared::boxes::BoxEdit;
    use shared::list::ListEdit;
    use shared::table::TableEdit;
    let mut out = Vec::new();
    tab.header_drafts.with(|d| out.extend(d.iter().map(|(at, h)| Change::Block { edit: BlockEdit::Header { at: at.clone(), header: h.clone() } })));
    tab.p_drafts.with(|d| out.extend(d.iter().map(|((at, child), (_, t))| Change::Block { edit: BlockEdit::SetP { at: at.clone(), child: *child, text: t.clone() } })));
    tab.table_drafts.with(|d| out.extend(d.iter().map(|(at, (id, title))| Change::Table { edit: TableEdit::Info { at: at.clone(), id: id.clone(), title: title.clone() } })));
    tab.th_drafts.with(|d| out.extend(d.iter().map(|((at, col), (_, t))| Change::Table { edit: TableEdit::SetHeader { at: at.clone(), col: *col, text: t.clone() } })));
    tab.cell_drafts.with(|d| {
        out.extend(d.iter().map(|((at, part, row, col), (_, t))| Change::Table { edit: TableEdit::SetCell { at: at.clone(), part: *part, row: *row, col: *col, text: t.clone() } }))
    });
    tab.li_drafts.with(|d| out.extend(d.iter().map(|((at, n), (_, t))| Change::List { edit: ListEdit::SetItem { at: at.clone(), n: *n, text: t.clone() } })));
    tab.box_drafts.with(|d| {
        out.extend(d.iter().map(|((at, n), (_, t))| Change::Box {
            edit: match n {
                Some(n) => BoxEdit::SetP { at: at.clone(), n: *n, text: t.clone() },
                None => BoxEdit::SetTrigger { at: at.clone(), text: t.clone() },
            },
        }))
    });
    // Stat block, item, spell, section block and license cards ("sb:" keys), and the
    // family form's drafts. One that no longer fits the file fails to plan, and is skipped.
    let d = tab.doc_untracked();
    let tag_of = |i: usize| d.nodes.first().and_then(|r| r.children.get(i)).map(|c| d.nodes[*c].tag.clone());
    tab.fam_drafts.with(|f| {
        for (key, (_, text)) in f {
            let c = if key.starts_with("sb:") { crate::statblock_form::draft_change(key, text) } else { crate::family_form::draft_change(key, text, tag_of) };
            out.extend(c);
        }
    });
    tab.ref_drafts.with(|d| {
        out.extend(d.iter().map(|(key, entry)| {
            let edit = match key.split_once(':') {
                Some((tag, id)) if tag != "new" => RefEdit::Update { tag: tag.to_string(), id: id.to_string(), entry: entry.clone() },
                _ => RefEdit::Add { entry: entry.clone() },
            };
            Change::Reference { edit }
        }))
    });
    out
}

/// The selected element as the tab's drafts would leave it, if any of them apply.
fn drafts_view(tab: DocTab, d: &DocData, selected: usize) -> Option<AnyView> {
    let changes = text_drafts(tab);
    let mut text = d.source.clone();
    let mut planned = 0;
    for c in &changes {
        if let Ok(t) = c.plan(&text) {
            text = t;
            planned += 1;
        }
    }
    if planned == 0 {
        return None;
    }
    let key = d.key_of(selected);
    Some(planned_view(
        Ok(text),
        move |_, nodes| {
            let mut cur = 0;
            for k in &key {
                cur = *nodes.get(cur)?.children.get(*k)?;
            }
            Some(cur)
        },
        "Apply moves this element.",
    ))
}

/// The re-indented text of the element `find` locates in `text`, highlighted.
fn planned_view(planned: Result<String, String>, find: impl Fn(&str, &[shared::StructNode]) -> Option<usize>, gone: &'static str) -> AnyView {
    let planned = planned.and_then(|t| parse(&t).map(|p| (t, p.nodes)).map_err(|e| e.message));
    match planned {
        Ok((text, nodes)) => match find(&text, &nodes) {
            Some(i) => view! {
                <div class="source-note">"Showing the text after Apply (not yet written)."</div>
                <pre class="code">{highlighted(&reindent(&text, nodes[i].start, nodes[i].end))}</pre>
            }
            .into_any(),
            None => view! { <div class="source-note">{gone}</div> }.into_any(),
        },
        Err(e) => view! { <div class="source-note bad">{format!("Can't plan the edit: {e}")}</div> }.into_any(),
    }
}

fn highlighted(text: &str) -> impl IntoView {
    tokenize(text).into_iter().map(|(kind, s)| view! { <span class=kind.class()>{s.to_string()}</span> }).collect_view()
}

#[component]
pub fn Viewer() -> impl IntoView {
    let app = ctx();
    move || match app.active_tab() {
        None => view! {
            <div class="viewer-empty">
                {move || if app.project.get().is_some() {
                    "Right-click an .xml file in the Files panel and choose Open."
                } else {
                    "Open a project folder to begin."
                }}
            </div>
        }
        .into_any(),
        Some(tab) => view! { <TabView tab/> }.into_any(),
    }
}

#[component]
fn TabView(tab: DocTab) -> impl IntoView {
    // Prose boxes in this tab: chips named from its file (a stat block's page gives its own).
    let palette = match tab.doc_untracked().kind {
        shared::DocKind::Supplement => shared::prose::Palette::Supplement,
        shared::DocKind::Monsters => shared::prose::Palette::StatBlock,
        shared::DocKind::MagicItem => shared::prose::Palette::MagicItem,
        shared::DocKind::Spell => shared::prose::Palette::Spell,
        shared::DocKind::License => shared::prose::Palette::License,
    };
    crate::prose_editor::provide_env(tab, palette, None);
    let selected = tab.selected;
    let kind = Memo::new(move |_| form_kind(&tab.doc(), selected.get()));
    // Which form is mounted: the reference editor stays mounted whichever of its
    // entries is picked (and across applies), so focus and scroll survive.
    let mounted = Memo::new(move |_| {
        kind.get().map(|k| match k {
            FormKind::Reference(_) => FormKind::Reference(None),
            FormKind::Block(c, _) => FormKind::Block(c, None),
            other => other,
        })
    });
    // Picking an entry in the tree opens its card.
    Effect::new(move |_| {
        match kind.get() {
            Some(FormKind::Reference(Some(k))) => tab.ref_focus.set(Some(k)),
            Some(FormKind::Block(_, Some(child))) => tab.block_focus.set(Some(child)),
            _ => {}
        }
    });
    let form_tag = move || kind.with(|k| k.is_some());
    let mode = tab.mode;

    let crumbs = move || {
        let idx = selected.get();
        let d = tab.doc();
        d.path_to(idx.min(d.nodes.len() - 1))
            .into_iter()
            .map(|i| {
                let n = &d.nodes[i];
                let current = i == idx;
                let text = if n.label.is_empty() || n.leaf { n.tag.clone() } else { format!("{} ({})", n.tag, n.label) };
                let tag = n.tag.clone();
                view! { <span class="crumb" class:current=current data-tag=tag on:click=move |_| tab.reveal(i)>{text}</span> }
            })
            .collect_view()
    };
    let lines = move || {
        let d = tab.doc();
        let n = d.nodes.get(selected.get())?;
        let last = n.line + d.source[n.start..n.end].matches('\n').count();
        Some(if last == n.line { format!("line {}", n.line) } else { format!("lines {}–{}", n.line, last) })
    };

    // Source: the file's text for the element; for the cover with a draft, the text it
    // would have after Apply.
    let source = move || {
        let d = tab.doc();
        let Some(n) = d.nodes.get(selected.get()) else { return ().into_any() };
        match form_kind(&d, selected.get()) {
            Some(FormKind::Cover) => {
                if let Some(draft) = tab.cover_draft.get() {
                    return planned_view(
                        cover::plan(&d.source, &draft),
                        |_, nodes| cover::cover_node(nodes),
                        "Apply removes the cover.",
                    );
                }
            }
            Some(FormKind::Chapter(name)) => {
                if let Some(draft) = tab.chapter_drafts.with(|m| m.get(&name).cloned()) {
                    let new_name = draft.name.clone();
                    return planned_view(
                        chapter::plan(&d.source, &ChapterEdit::Update { name, model: draft }),
                        move |t, nodes| chapter::find_chapter(t, nodes, &new_name),
                        "Apply removes the chapter.",
                    );
                }
            }
            // Every other form: the text with the tab's drafts applied (paragraphs, headers,
            // tables, lists, boxes, reference cards, stat block, item and family cards).
            Some(FormKind::Reference(_)) | Some(FormKind::Block(..)) | Some(FormKind::Table(_)) | Some(FormKind::List(_)) | Some(FormKind::Box(_))
            | Some(FormKind::Item) | Some(FormKind::Spell) | Some(FormKind::License) | Some(FormKind::Part(_)) | Some(FormKind::SectionEl(_)) => {
                if let Some(v) = drafts_view(tab, &d, selected.get()) {
                    return v;
                }
            }
            None => {}
            Some(FormKind::Monster(path, tag)) => {
                if let Some((_, xml)) = tab.raw_drafts.with(|m| m.get(&path).cloned()) {
                    let p2 = path.clone();
                    return planned_view(
                        shared::element::plan(&d.source, &shared::element::ElementEdit { path, tag, xml }),
                        move |_, nodes| shared::element::at_path(nodes, &p2),
                        "Apply moves this element.",
                    );
                }
                if let Some(v) = drafts_view(tab, &d, selected.get()) {
                    return v;
                }
            }
        }
        view! { <pre class="code">{highlighted(&reindent(&d.source, n.start, n.end))}</pre> }.into_any()
    };

    // The supplement has no cover: offer one when the root is selected.
    let is_supplement = move || tab.doc().kind == shared::DocKind::Supplement;
    let no_cover = move || {
        let d = tab.doc();
        is_supplement() && selected.get() == 0 && cover::cover_node(&d.nodes).is_none()
    };
    let add_cover = move |_| apply_model(tab, CoverModel { present: true, ..Default::default() });
    let app = ctx();
    let add_chapter = move |_| app.dialog.set(Some(Dialog::NewChapter { tab: tab.id }));

    let status = move || {
        let d = tab.doc();
        let mut notes: Vec<(&'static str, String)> = Vec::new();
        match &d.validation {
            Validation::Valid { schema } => notes.push(("ok", format!("Valid against {schema}"))),
            Validation::NotRun { reason } => notes.push(("warn", format!("Schema name checked, but not XSD-validated: {reason}"))),
        }
        notes.extend(d.warnings.iter().map(|w| ("warn", w.clone())));
        notes.into_iter().map(|(kind, text)| view! { <div class=format!("status {kind}")>{text}</div> }).collect_view()
    };

    view! {
        <div class="viewer">
            <div class="viewer-head">
                <div class="crumbs">{crumbs}</div>
                <Show when=form_tag>
                    <div class="mode-switch">
                        <button class:active=move || mode.get() == Mode::Form on:click=move |_| mode.set(Mode::Form)>"Form"</button>
                        <button class:active=move || mode.get() == Mode::Source on:click=move |_| mode.set(Mode::Source)>"Source"</button>
                    </div>
                </Show>
                <div class="lines">{lines}</div>
            </div>
            <Show when=no_cover>
                <div class="notice">
                    "This supplement has no cover."
                    <button on:click=add_cover disabled=move || tab.applying.get()>"Add a cover"</button>
                </div>
            </Show>
            <Show when=move || is_supplement() && selected.get() == 0 && !refs::has_block(&tab.doc().nodes)>
                <div class="notice">
                    "This supplement has no <reference> block for monsters, spells, items, images, nouns and data."
                    <button disabled=move || tab.applying.get()
                        on:click=move |_| apply(tab, Change::Reference { edit: RefEdit::CreateBlock })>"Create <reference>"</button>
                </div>
            </Show>
            {move || tab.disk.get().map(|note| {
                let close = move |_| tab.disk.set(None);
                match note {
                    crate::state::DiskNote::Reloaded { gone } => view! {
                        <div class="notice info disk-note">
                            {match gone {
                                Some(tag) => format!("The file changed on disk and was read again. The <{tag}> you had open isn't in the file any more."),
                                None => "The file changed on disk and was read again.".to_string(),
                            }}
                            <button class="small" on:click=close>"OK"</button>
                        </div>
                    }.into_any(),
                    crate::state::DiskNote::Changed { edits } => view! {
                        <div class="notice warn disk-note">
                            {format!("The file changed on disk. Reading it again throws away {} unapplied edit{}; until then, Apply is refused.", edits, if edits == 1 { "" } else { "s" })}
                            <button class="small primary" on:click=move |_| tab.reload()>"Read it again"</button>
                            <button class="small" on:click=close>"Not now"</button>
                        </div>
                    }.into_any(),
                    crate::state::DiskNote::Broken(why) => view! {
                        <div class="notice warn disk-note">
                            {format!("The file changed on disk and can't be opened as it is now: {why}")}
                            <button class="small" on:click=close>"OK"</button>
                        </div>
                    }.into_any(),
                }
            })}
            <Show when=move || is_supplement() && selected.get() == 0>
                <AddParts tab/>
            </Show>
            <Show when=move || is_supplement() && selected.get() == 0>
                <div class="notice info">
                    {move || format!("{} chapter(s).", chapter::chapter_nodes(&tab.doc().nodes).len())}
                    <button on:click=add_chapter disabled=move || tab.applying.get()>"Add chapter…"</button>
                </div>
            </Show>
            <div class="viewer-body">
                {move || match (mounted.get(), mode.get()) {
                    (Some(FormKind::Cover), Mode::Form) => view! { <CoverForm tab/> }.into_any(),
                    (Some(FormKind::Chapter(name)), Mode::Form) => view! { <ChapterForm tab name/> }.into_any(),
                    (Some(FormKind::Reference(_)), Mode::Form) => view! { <ReferenceForm tab/> }.into_any(),
                    (Some(FormKind::Block(at, _)), Mode::Form) => view! { <BlockForm tab at/> }.into_any(),
                    (Some(FormKind::Table(at)), Mode::Form) => view! { <TableForm tab at/> }.into_any(),
                    (Some(FormKind::List(at)), Mode::Form) => view! { <ListForm tab at/> }.into_any(),
                    (Some(FormKind::Box(at)), Mode::Form) => view! { <BoxForm tab at/> }.into_any(),
                    (Some(FormKind::Monster(path, tag)), Mode::Form) => view! { <MonsterForm tab path tag/> }.into_any(),
                    (Some(FormKind::Item), Mode::Form) => view! { <ItemPage tab/> }.into_any(),
                    (Some(FormKind::Spell), Mode::Form) => view! { <SpellPage tab/> }.into_any(),
                    (Some(FormKind::License), Mode::Form) => view! { <LicensePage tab/> }.into_any(),
                    (Some(FormKind::Part(tag)), Mode::Form) => view! { <PartPage tab tag/> }.into_any(),
                    (Some(FormKind::SectionEl(key)), Mode::Form) => view! { <SectionElPage tab key/> }.into_any(),
                    _ => source().into_any(),
                }}
            </div>
            <ProblemsPanel tab/>
            <div class="statusbar">{status}</div>
        </div>
    }
}

/// The bottom panel listing the last refused apply's validation errors, each with the
/// line of edited text it points at.
#[component]
fn ProblemsPanel(tab: DocTab) -> impl IntoView {
    let open = tab.problems_open;
    let count = move || tab.problems.with(|p| p.len());
    let rows = move || {
        let checked = tab.problem_source.get();
        let lines: Vec<&str> = checked.lines().collect();
        tab.problems
            .get()
            .into_iter()
            .map(|p| {
                let at = p.line.and_then(|l| lines.get(l.saturating_sub(1))).map(|l| l.trim().to_string());
                view! {
                    <div class="problem-row">
                        <span class="problem-line">{p.line.map(|l| format!("Ln {l}")).unwrap_or_default()}</span>
                        <span class="problem-msg">
                            {p.message}
                            {at.map(|t| view! { <code class="problem-code">{t}</code> })}
                        </span>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <section class="problems" class:open=move || open.get()>
            <header on:click=move |_| open.update(|o| *o = !*o)>
                <span class="twist">{move || if open.get() { "▾" } else { "▸" }}</span>
                "Problems"
                <span class="badge" class:zero=move || count() == 0>{count}</span>
                <span class="spacer"></span>
                <Show when=move || { count() > 0 }>
                    <span class="problems-note">"The last Apply was refused: the edited file didn't validate, so nothing was written."</span>
                    <button class="small" on:click=move |ev| {
                        ev.stop_propagation();
                        tab.clear_problems();
                    }>"Clear"</button>
                </Show>
            </header>
            <Show when=move || open.get()>
                <div class="problem-list">
                    {move || if count() == 0 {
                        view! { <div class="problem-empty">"No problems."</div> }.into_any()
                    } else {
                        rows().into_any()
                    }}
                </div>
            </Show>
        </section>
    }
}
