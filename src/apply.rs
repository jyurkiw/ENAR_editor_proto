//! Sending a form's change to the backend, and taking the result back into the tab.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::{self, ChapterEdit};
use shared::{ApplyResult, Change};

use crate::api;
use crate::state::{ctx, DocData, DocTab};

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
    }
}

/// Applies a change: the backend validates the edited file and writes it only if it
/// validates. Written: the tab shows the new file and the form's draft is dropped.
/// Refused: the errors go to the Problems panel and the draft stays.
pub fn apply(tab: DocTab, change: Change) {
    let app = ctx();
    let data = tab.doc_untracked();
    tab.applying.set(true);
    spawn_local(async move {
        match api::apply_change(&data.path, &data.source, &change).await {
            Ok(ApplyResult::Saved { doc }) => {
                match &change {
                    Change::Cover { .. } => tab.cover_draft.set(None),
                    Change::Chapter { edit: ChapterEdit::Update { name, .. } | ChapterEdit::Delete { name } } => {
                        let name = name.clone();
                        tab.chapter_drafts.update(|d| {
                            d.remove(&name);
                        });
                    }
                    Change::Chapter { .. } => {}
                }
                tab.clear_problems();
                tab.replace_doc(doc, selection(&change));
            }
            Ok(ApplyResult::Invalid { problems }) => {
                let checked = change.plan(&data.source).unwrap_or_default();
                tab.show_problems(problems, checked);
            }
            Err(e) => app.show_error(e),
        }
        tab.applying.set(false);
    });
}
