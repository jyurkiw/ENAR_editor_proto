//! Sending a form's change to the backend, and taking the result back into the tab.
//! Applies run one at a time per tab; a change made while one runs waits its turn and is
//! then planned against the file as that apply left it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::{self, ChapterEdit};
use shared::block::{self, BlockEdit, Container};
use shared::reference::RefEdit;
use shared::boxes::{self, BoxEdit};
use shared::monster::{self, MonsterEdit};
use shared::list::{self, ListEdit};
use shared::table::{self, TableAt, TableEdit};
use shared::{ApplyResult, Change};

use crate::api;
use crate::state::{ctx, DocData, DocTab};
use shared::boxes::BoxAt;

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
            let ci = block::find(&d.source, &d.nodes, &at)?;
            // A new list or box opens in its editor.
            let new_tag = match edit {
                BlockEdit::AddList { after, .. } => Some(("list", after)),
                BlockEdit::AddBox { after, tag, .. } => Some((tag.as_str(), after)),
                _ => None,
            };
            if let Some((tag, after)) = new_tag {
                let pos = match after {
                    Some(a) => a + 1,
                    None => d.nodes[ci].children.iter().position(|k| d.nodes[*k].tag == tag)?,
                };
                return d.nodes[ci].children.get(pos).copied().filter(|k| d.nodes[*k].tag == tag).or(Some(ci));
            }
            Some(ci)
        }
        // The element keeps its place: the selection stays (found again by its path).
        Change::Element { .. } => None,
        Change::Monster { edit } => match edit {
            MonsterEdit::Add { id, .. } | MonsterEdit::Move { id, .. } => monster::find_monster(&d.source, &d.nodes, id),
            MonsterEdit::Delete { .. } => Some(0),
        },
        Change::Box { edit } => {
            boxes::find(&d.source, &d.nodes, edit.at()).or_else(|| block::find(&d.source, &d.nodes, &edit.at().at))
        }
        Change::List { edit } => {
            // Still there: the list; deleted: its section or sidebar.
            list::find(&d.source, &d.nodes, edit.at()).or_else(|| block::find(&d.source, &d.nodes, &edit.at().at))
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
                if let Change::Box { edit: BoxEdit::SetTrigger { at, .. } | BoxEdit::SetP { at, .. } } = &change {
                    let slot = match &change {
                        Change::Box { edit: BoxEdit::SetP { n, .. } } => Some(*n),
                        _ => None,
                    };
                    let k = (at.clone(), slot);
                    tab.box_failed.update(|f| {
                        f.insert(k);
                    });
                }
                if let Change::Element { edit } = &change {
                    let k = edit.path.clone();
                    tab.raw_failed.update(|f| {
                        f.insert(k);
                    });
                }
                if let Change::List { edit: ListEdit::SetItem { at, n, .. } } = &change {
                    let k = (at.clone(), *n);
                    tab.li_failed.update(|f| {
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
            BlockEdit::AddList { .. } | BlockEdit::AddBox { .. } => {}
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
        Change::Element { edit } => {
            let k = edit.path.clone();
            tab.raw_drafts.update(|d| {
                d.remove(&k);
            });
            tab.raw_failed.update(|f| {
                f.remove(&k);
            });
        }
        // Stat blocks added, moved or deleted: tree paths after them shifted.
        Change::Monster { .. } => {
            tab.raw_drafts.update(|d| d.clear());
            tab.raw_failed.update(|f| f.clear());
        }
        Change::Box { edit } => {
            let at = edit.at().clone();
            // Paragraphs from `from` on shifted (or the box is gone).
            let from = match edit {
                BoxEdit::SetTrigger { .. } | BoxEdit::SetP { .. } => {
                    let slot = if let BoxEdit::SetP { n, .. } = edit { Some(*n) } else { None };
                    let k = (at.clone(), slot);
                    tab.box_drafts.update(|d| {
                        d.remove(&k);
                    });
                    tab.box_failed.update(|f| {
                        f.remove(&k);
                    });
                    None
                }
                BoxEdit::Speaker { .. } => None,
                BoxEdit::AddP { after, .. } => Some(after.map_or(0, |a| a + 1)),
                BoxEdit::DeleteP { n, with_box, .. } => Some(if *with_box { 0 } else { *n }),
                BoxEdit::MoveP { n, before, .. } => Some((*n).min(before.unwrap_or(*n))),
                BoxEdit::Delete { .. } => Some(0),
            };
            if let Some(from) = from {
                let gone = matches!(edit, BoxEdit::Delete { .. } | BoxEdit::DeleteP { with_box: true, .. });
                let keep = move |a: &BoxAt, s: &Option<usize>| *a != at || (!gone && s.is_none_or(|i| i < from));
                tab.box_drafts.update(|d| d.retain(|(a, s), _| keep(a, s)));
                tab.box_failed.update(|f| f.retain(|(a, s)| keep(a, s)));
            }
        }
        Change::List { edit } => {
            let at = edit.at().clone();
            // Items from `from` on shifted (or the list is gone): their drafts no longer match.
            let from = match edit {
                ListEdit::SetItem { n, .. } => {
                    let k = (at.clone(), *n);
                    tab.li_drafts.update(|d| {
                        d.remove(&k);
                    });
                    tab.li_failed.update(|f| {
                        f.remove(&k);
                    });
                    None
                }
                ListEdit::Attrs { .. } => None,
                ListEdit::AddItem { after, .. } => Some(after.map_or(0, |a| a + 1)),
                ListEdit::DeleteItem { n, .. } => Some(*n),
                ListEdit::MoveItem { n, before, .. } => Some((*n).min(before.unwrap_or(*n))),
                ListEdit::Delete { .. } => Some(0),
            };
            if let Some(from) = from {
                tab.li_drafts.update(|d| d.retain(|(a, i), _| *a != at || *i < from));
                tab.li_failed.update(|f| f.retain(|(a, i)| *a != at || *i < from));
            }
        }
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
