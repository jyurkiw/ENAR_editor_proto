//! The Preview side panel: the open stat block as the book prints it. The compiler builds
//! it (encompile.preview) and LaTeX sets it, one column wide; the panel shows the picture.
//!
//! Which stat block: the one the selection is in, else the file's first. It is printed
//! again whenever the file changes (every applied edit replaces the file's text), but only
//! while the panel is showing: switching to another side tab drops the panel, and with it
//! the watch. The last picture is kept, so switching back shows it at once.

use std::cell::RefCell;

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::{DocKind, Preview};

use crate::api;
use crate::state::ctx;

/// What to print: the file, the stat block's id and name, and the file's text (so any edit
/// is a new target).
#[derive(Clone, PartialEq)]
struct Target {
    path: String,
    id: String,
    name: String,
    source: String,
}

thread_local! {
    /// The last picture printed, and what it was printed from.
    static LAST: RefCell<Option<(Target, Result<Preview, String>)>> = const { RefCell::new(None) };
}

/// Waits `ms` milliseconds.
async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |done, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&done, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

#[component]
pub fn PreviewPanel() -> impl IntoView {
    let app = ctx();
    // Ok(target), or Err(why there's nothing to print).
    let target = Memo::new(move |_| -> Result<Target, &'static str> {
        let tab = app.active_tab().ok_or("Open a monster file to preview its stat blocks.")?;
        let d = tab.doc();
        if d.kind != DocKind::Monsters {
            return Err("The preview shows a monster file's stat blocks.");
        }
        let root = d.nodes.first().ok_or("The file is empty.")?;
        let is_monster = |c: usize| root.children.get(c).is_some_and(|i| d.nodes[*i].tag == "monster");
        let child = d
            .key_of(tab.selected.get())
            .first()
            .copied()
            .filter(|c| is_monster(*c))
            .or_else(|| (0..root.children.len()).find(|c| is_monster(*c)))
            .ok_or("The file has no stat blocks.")?;
        let m = shared::statblock::read(&d.source, child).ok_or("This stat block can't be read.")?;
        Ok(Target { path: d.path.clone(), id: m.get("id").to_string(), name: m.opt_text("name"), source: d.source.clone() })
    });
    let shown = RwSignal::new(LAST.with(|l| l.borrow().clone()));
    let busy = RwSignal::new(false);
    // Each change gets a number; only the newest one's picture is shown.
    let generation = StoredValue::new(0u32);
    Effect::new(move |_| {
        let Ok(t) = target.get() else { return };
        if shown.with_untracked(|s| s.as_ref().is_some_and(|(st, _)| *st == t)) {
            return;
        }
        generation.update_value(|g| *g += 1);
        let mine = generation.get_value();
        spawn_local(async move {
            sleep(250).await; // a burst of applies prints once
            if generation.get_value() != mine {
                return;
            }
            busy.set(true);
            let r = api::preview_statblock(&t.path, &t.id).await.map_err(|e| {
                std::iter::once(e.message).chain(e.details).collect::<Vec<_>>().join("\n")
            });
            if generation.get_value() != mine {
                return;
            }
            busy.set(false);
            LAST.with(|l| *l.borrow_mut() = Some((t.clone(), r.clone())));
            shown.set(Some((t, r)));
        });
    });
    view! {
        <div class="preview-panel">
            {move || match target.get() {
                Err(why) => view! { <div class="empty">{why}</div> }.into_any(),
                Ok(t) => view! {
                    <div class="preview-head">
                        <span class="preview-name">{t.name}</span>
                        <Show when=move || busy.get()><span class="preview-busy">"Rendering…"</span></Show>
                    </div>
                }.into_any(),
            }}
            {move || {
                // The last picture of this file's stat block stays up while the next renders.
                let Ok(t) = target.get() else { return ().into_any() };
                shown.with(|s| match s {
                    Some((st, r)) if st.path == t.path && st.id == t.id => match r {
                        Ok(p) => view! {
                            <div class="preview-pages" class:stale=move || busy.get()>
                                {p.images.iter().map(|src| view! { <img class="preview-img" src=src.clone() alt="The stat block as it prints"/> }).collect_view()}
                            </div>
                            {(!p.warnings.is_empty()).then(|| view! {
                                <ul class="preview-warnings">{p.warnings.iter().map(|w| view! { <li>{w.clone()}</li> }).collect_view()}</ul>
                            })}
                        }.into_any(),
                        Err(e) => view! { <pre class="preview-error">{e.clone()}</pre> }.into_any(),
                    },
                    _ => view! { <div class="empty">"Rendering the stat block…"</div> }.into_any(),
                })
            }}
        </div>
    }
}
