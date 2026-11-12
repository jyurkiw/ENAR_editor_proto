//! Sending a form's change to the backend, and taking the result back into the tab.
//! Applies run one at a time per tab; a change made while one runs waits its turn and is
//! then planned against the file as that apply left it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::{self, ChapterEdit};
use shared::block::{self, BlockEdit, Container};
use shared::reference::RefEdit;
use shared::table::{self, TableAt, TableEdit};
use shared::{ApplyResult, Change};

use crate::api;
use crate::state::{ctx, DocData, DocTab};

/// The card key of a reference entry.
pub fn ref_key(tag: &str, id: &str) -> String {
    format!("{tag}:{id}")
}

/// What to select once the change is written.
fn selection(change: &Change) -> impl Fn(&DocData) -> Option<usize> + 'static {
    let change = change.clone();
    move |d: &DocData| match &change {
        Change::Cover { model } => model.present.then(|| d.find_tag("cover")).flatten(),
        Change::Chapter { edit } => match edit {
            ChapterEdit::Update { model, .. } => chapter::find_chapter(&d.source, &d.nodes, &model.name),
            ChapterEdit::Add { name, .. } => chapter::find_chapter(&d.source, &d.nodes, name),
            ChapterEdit::Delete { .. } => Some(0),
        },
        Change::Reference { .. } => d.find_tag("reference"),
        Change::Block { edit } => {
            // A renamed section or sidebar is found under its new name.
            let at = match edit {
                BlockEdit::Header { at: Container::Section { chapter, nth, .. }, header } => {
                    Container::Section { chapter: chapter.clone(), name: header.key.clone(), nth: *nth }
                }
                BlockEdit::Header { at: Container::Sidebar { chapter, .. }, header } => {
                    Container::Sidebar { chapter: chapter.clone(), id: header.key.clone() }
                }
                other => other.container().clone(),
            };
            block::find(&d.source, &d.nodes, &at)
        }
        Change::Table { edit } => {
            let at = match edit {
                TableEdit::Info { at, id, .. } => TableAt { id: id.clone(), nth: at.nth },
                other => other.at().clone(),
            };
            table::find(&d.source, &d.nodes, &at)
        }
    }
}

/// Applies a change: the backend validates the edited file and writes it only if it
/// validates. Written: the tab shows the new file and the form's draft is dropped.
/// Refused: the errors go to the Problems panel and the draft stays.
/// `card` is the reference card the change came from, if any.
pub fn apply(tab: DocTab, change: Change) {
    apply_from(tab, change, None)
}

pub fn apply_from(tab: DocTab, change: Change, card: Option<String>) {
    if tab.applying.get_untracked() {
        tab.queue.update_value(|q| q.push((change, card)));
        return;
    }
    let app = ctx();
    let data = tab.doc_untracked();
    tab.applying.set(true);
    spawn_local(async move {
        match api::apply_change(&data.path, &data.source, &change).await {
            Ok(ApplyResult::Saved { doc }) => {
                saved(tab, &change, card.as_deref());
                tab.clear_problems();
                tab.replace_doc(doc, selection(&change));
                if matches!(change, Change::Reference { .. }) {
                    tab.refresh_refs();
                }
            }
            Ok(ApplyResult::Invalid { problems }) => {
                let checked = change.plan(&data.source).unwrap_or_default();
                if let Some(k) = &card {
                    tab.ref_failed.update(|f| {
                        f.insert(k.clone());
                    });
                }
                if let Change::Table { edit: TableEdit::SetCell { at, part, row, col, .. } } = &change {
                    let k = (at.clone(), *part, *row, *col);
                    tab.cell_failed.update(|f| {
                        f.insert(k);
                    });
                }
                if let Change::Table { edit: TableEdit::SetHeader { at, col, .. } } = &change {
                    let k = (at.clone(), *col);
                    tab.th_failed.update(|f| {
                        f.insert(k);
                    });
                }
                if let Change::Block { edit: BlockEdit::SetP { at, child, .. } } = &change {
                    let k = (at.clone(), *child);
                    tab.p_failed.update(|f| {
                        f.insert(k);
                    });
                }
                tab.show_problems(problems, checked);
            }
            Err(e) => app.show_error(e),
        }
        tab.applying.set(false);
        let next = tab.queue.try_update_value(|q| (!q.is_empty()).then(|| q.remove(0))).flatten();
        if let Some((change, card)) = next {
            apply_from(tab, change, card);
        }
    });
}

/// Drops the draft a written change came from.
fn saved(tab: DocTab, change: &Change, card: Option<&str>) {
    match change {
        Change::Cover { .. } => tab.cover_draft.set(None),
        Change::Chapter { edit: ChapterEdit::Update { name, .. } | ChapterEdit::Delete { name } } => {
            let name = name.clone();
            tab.chapter_drafts.update(|d| {
                d.remove(&name);
            });
        }
        Change::Chapter { .. } => {}
        Change::Block { edit } => match edit {
            BlockEdit::Header { at, .. } => {
                let at = at.clone();
                tab.header_drafts.update(|d| {
                    d.remove(&at);
                });
            }
            BlockEdit::SetP { at, child, .. } => {
                let k = (at.clone(), *child);
                tab.p_drafts.update(|d| {
                    d.remove(&k);
                });
                tab.p_failed.update(|f| {
                    f.remove(&k);
                });
            }
            BlockEdit::AddP { after, .. } => {
                // Focus the new paragraph: right after `after`, or first in the body.
                if let Some(a) = after {
                    tab.block_focus.set(Some(a + 1));
                }
            }
            BlockEdit::DeleteP { at, child } | BlockEdit::Move { at, child, .. } => {
                // Paragraph indexes after it shift; drop drafts and marks there.
                let (at, child) = (at.clone(), *child);
                tab.p_drafts.update(|d| d.retain(|(a, c), _| *a != at || *c < child));
                tab.p_failed.update(|f| f.retain(|(a, c)| *a != at || *c < child));
            }
        },
        Change::Table { edit } => match edit {
            TableEdit::SetCell { at, part, row, col, .. } => {
                let k = (at.clone(), *part, *row, *col);
                tab.cell_drafts.update(|d| {
                    d.remove(&k);
                });
                tab.cell_failed.update(|f| {
                    f.remove(&k);
                });
            }
            TableEdit::AddCell { .. } => {}
            TableEdit::AddRow { at, part, after } => {
                // Rows from here on shifted.
                let (at, part, from) = (at.clone(), *part, after.map_or(0, |a| a + 1));
                tab.cell_drafts.update(|d| d.retain(|(a, p, r, _), _| *a != at || *p != part || *r < from));
                tab.cell_failed.update(|f| f.retain(|(a, p, r, _)| *a != at || *p != part || *r < from));
            }
            TableEdit::DeleteRow { at, part, row } | TableEdit::SwapRows { at, part, row } => {
                let (at, part, from) = (at.clone(), *part, *row);
                tab.cell_drafts.update(|d| d.retain(|(a, p, r, _), _| *a != at || *p != part || *r < from));
                tab.cell_failed.update(|f| f.retain(|(a, p, r, _)| *a != at || *p != part || *r < from));
            }
            TableEdit::Info { at, .. } => {
                let at = at.clone();
                tab.table_drafts.update(|d| {
                    d.remove(&at);
                });
            }
            TableEdit::SetHeader { at, col, .. } => {
                let k = (at.clone(), *col);
                tab.th_drafts.update(|d| {
                    d.remove(&k);
                });
                tab.th_failed.update(|f| {
                    f.remove(&k);
                });
            }
            TableEdit::AddHeader { at, col, .. } | TableEdit::DeleteHeader { at, col } | TableEdit::SwapHeaders { at, col } => {
                // Columns from here on shifted: their drafts no longer match.
                let (at, col) = (at.clone(), *col);
                tab.th_drafts.update(|d| d.retain(|(a, c), _| *a != at || *c < col));
                tab.th_failed.update(|f| f.retain(|(a, c)| *a != at || *c < col));
                // So did the cells in those columns.
                tab.cell_drafts.update(|d| d.retain(|(a, _, _, c), _| *a != at || *c < col));
                tab.cell_failed.update(|f| f.retain(|(a, _, _, c)| *a != at || *c < col));
                if matches!(edit, TableEdit::AddHeader { .. }) {
                    tab.th_focus.set(Some(col));
                }
            }
        },
        Change::Reference { edit } => {
            if let Some(card) = card {
                tab.ref_drafts.update(|d| {
                    d.remove(card);
                });
                tab.ref_failed.update(|f| {
                    f.remove(card);
                });
            }
            match edit {
                // A renamed or new card keeps its place under its new key, still open.
                RefEdit::Update { tag, entry, .. } | RefEdit::Add { entry: entry @ shared::reference::RefEntry { tag, .. } } => {
                    let new_key = ref_key(tag, &entry.id());
                    if let Some(card) = card {
                        tab.ref_new.update(|n| n.retain(|k| k != card));
                        tab.ref_open.update(|o| {
                            if o.remove(card) {
                                o.insert(new_key);
                            }
                        });
                    }
                }
                RefEdit::Delete { tag, id } => {
                    let k = ref_key(tag, id);
                    tab.ref_open.update(|o| {
                        o.remove(&k);
                    });
                }
                RefEdit::Move { .. } | RefEdit::CreateBlock => {}
            }
        }
    }
}
