//! The box editor: a `<speakAloud>` (read aloud to the players) or a `<dialog>` (an NPC's
//! lines) in a section. On top, the box's settings (a dialog's speaker); then its
//! `<trigger>` (when to read it) and its paragraphs, each a text box of its XML with the
//! ref preview. Text boxes apply one at a time (Apply, Ctrl+Enter, or leaving the box);
//! add, delete and drag-to-reorder apply at once.

use leptos::ev::{DragEvent, FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::block;
use shared::boxes::{self, BoxAt, BoxEdit, BoxView};
use shared::inline::{self, Piece, RefCtx};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::inline_refs::{RefTools, RefToolsView};
use crate::state::DocTab;

fn focus_stays(ev: &FocusEvent, el: &NodeRef<html::Div>) -> bool {
    ev.related_target()
        .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
        .zip(el.get())
        .is_some_and(|(n, e)| e.contains(Some(&n)))
}

fn confirm(msg: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(msg).ok()).unwrap_or(false)
}

fn edit(tab: DocTab, e: BoxEdit) {
    apply(tab, Change::Box { edit: e });
}

fn box_name(tag: &str) -> &'static str {
    if tag == "dialog" { "dialog" } else { "read-aloud box" }
}

/// The text as printed (refs resolved, markup dropped); None when it isn't well-formed.
fn printed(text: &str, ctx: &RefCtx) -> Option<String> {
    let pieces = inline::parse(text).ok()?;
    Some(
        pieces
            .iter()
            .map(|p| match p {
                Piece::Text { text, .. } => text.clone(),
                Piece::Ref(t) => ctx.resolve(t).text,
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[derive(Clone, Copy)]
struct Drag {
    from: RwSignal<Option<usize>>,
    at: RwSignal<Option<(usize, bool)>>,
}

#[component]
fn BoxInfo(tab: DocTab, at: StoredValue<BoxAt>, view: Memo<Option<BoxView>>) -> impl IntoView {
    let busy = move || tab.applying.get();
    let tag = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.tag.clone()).unwrap_or_default()));
    let base = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.speaker.clone()).unwrap_or_default()));
    let draft = RwSignal::new(None::<(String, String)>);
    let speaker = move || {
        let b = base.get();
        draft.get().filter(|(o, _)| *o == b).map(|(_, t)| t).unwrap_or(b)
    };
    let dirty = move || draft.get().is_some_and(|(o, t)| o == base.get() && t != o);
    let el = NodeRef::<html::Div>::new();
    let do_apply = move || {
        if dirty() {
            edit(tab, BoxEdit::Speaker { at: at.get_value(), speaker: speaker() });
            draft.set(None);
        }
    };
    let n_paras = move || view.with(|v| v.as_ref().map(|v| v.paras.len()).unwrap_or(0));
    let delete = move |_: MouseEvent| {
        if confirm(&format!("Delete this {} and its {} paragraph(s)?", box_name(&tag.get_untracked()), n_paras())) {
            edit(tab, BoxEdit::Delete { at: at.get_value() });
        }
    };
    let open_section = move |_: MouseEvent| {
        let d = tab.doc_untracked();
        if let Some(ci) = block::find(&d.source, &d.nodes, &at.get_value().at) {
            tab.reveal(ci);
        }
    };
    let section = move || match at.get_value().at {
        block::Container::Section { name, .. } => format!("in section \"{name}\""),
        block::Container::Sidebar { id, .. } => format!("in sidebar \"{id}\""),
    };
    view! {
        <div node_ref=el class="table-card" class:dirty=dirty
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { draft.set(None) } else if ev.key() == "Enter" { do_apply() }
            }>
            <div class="table-card-title">
                {move || if tag.get() == "dialog" { "Dialog" } else { "Read-aloud box" }}
                <span class="hint">{section}</span>
                <button class="link" title="Open the section editor" on:click=open_section>"open it"</button>
                <span class="spacer"></span>
                <button class="small danger" disabled=busy on:click=delete>{move || format!("Delete {}", box_name(&tag.get()))}</button>
            </div>
            <p class="hint">{move || if tag.get() == "dialog" {
                "NPC speech, boxed in the dialogue style. With a speaker, each paragraph prints as one line: Speaker: \u{201c}line\u{201d}."
            } else {
                "Text to read or paraphrase to the players, printed in a shaded box."
            }}</p>
            <Show when=move || tag.get() == "dialog">
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Speaker"</span>
                        <input type="text" prop:value=speaker on:input=move |ev| {
                            draft.set(Some((base.get_untracked(), event_target_value(&ev))))
                        }/>
                        <span class="hint">"@speaker: who is talking (printed bold). Empty removes it: the paragraphs print as written."</span>
                    </label>
                </div>
            </Show>
        </div>
    }
}

/// One text box of the box: the trigger (`n` None) or paragraph `n`.
#[component]
fn TextRow(
    tab: DocTab,
    at: StoredValue<BoxAt>,
    n: Option<usize>,
    view: Memo<Option<BoxView>>,
    refctx: Memo<RefCtx>,
    #[prop(optional)] drag: Option<Drag>,
) -> impl IntoView {
    let base = Memo::new(move |_| {
        view.with(|v| {
            v.as_ref().map(|v| match n {
                None => v.trigger.clone(),
                Some(i) => v.paras.get(i).cloned().unwrap_or_default(),
            })
        })
        .unwrap_or_default()
    });
    let count = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.paras.len()).unwrap_or(0)));
    let tag = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.tag.clone()).unwrap_or_default()));
    let speaker = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.speaker.clone()).unwrap_or_default()));
    let key = move || (at.get_value(), n);
    let draft = move || {
        let b = base.get();
        tab.box_drafts.with(|d| d.get(&key()).filter(|(o, _)| *o == b).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(|| base.get());
    let dirty = move || draft().is_some();
    let failed = move || tab.box_failed.with(|f| f.contains(&key()));
    let busy = move || tab.applying.get();
    let el = NodeRef::<html::Div>::new();
    let set = move |v: String| {
        let b = base.get_untracked();
        let k = (at.get_value(), n);
        tab.box_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            let at = at.get_value();
            edit(tab, match n {
                None => BoxEdit::SetTrigger { at, text: t },
                Some(n) => BoxEdit::SetP { at, n, text: t },
            });
        }
    };
    let revert = move || {
        let k = key();
        tab.box_drafts.update(|d| {
            d.remove(&k);
        });
        tab.box_failed.update(|f| {
            f.remove(&k);
        });
    };
    let ta = NodeRef::<html::Textarea>::new();
    let tools = RefTools::new(Signal::derive(text), Callback::new(move |s: String| set(s)), ta, refctx);

    let add_below = move |_: MouseEvent| {
        let after = n;
        tab.box_focus.set(Some(Some(after.map_or(0, |a| a + 1))));
        edit(tab, BoxEdit::AddP { at: at.get_value(), after });
    };
    let delete = move |_: MouseEvent| {
        let Some(i) = n else { return };
        let name = box_name(&tag.get_untracked());
        let empty = base.get_untracked().trim().is_empty();
        let e = if count.get_untracked() == 1 {
            if confirm(&format!("This is the {name}'s last paragraph. Delete the whole {name}?")) {
                Some(true)
            } else if confirm(&format!("Delete just the paragraph, leaving the {name} empty?")) {
                Some(false)
            } else {
                None
            }
        } else {
            (empty || confirm("Delete this paragraph?")).then_some(false)
        };
        if let Some(with_box) = e {
            edit(tab, BoxEdit::DeleteP { at: at.get_value(), n: i, with_box });
        }
    };
    let clear_trigger = move |_: MouseEvent| edit(tab, BoxEdit::SetTrigger { at: at.get_value(), text: String::new() });

    // Drag (paragraphs only).
    let on_dragstart = move |ev: DragEvent| {
        let (Some(d), Some(i)) = (drag, n) else { return };
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &i.to_string());
            dt.set_effect_allowed("move");
        }
        d.from.set(Some(i));
    };
    let on_dragend = move |_: DragEvent| {
        if let Some(d) = drag {
            d.from.set(None);
            d.at.set(None);
        }
    };
    let on_dragover = move |ev: DragEvent| {
        let (Some(d), Some(i)) = (drag, n) else { return };
        if d.from.get_untracked().is_none() {
            return;
        }
        ev.prevent_default();
        let after = el.get().is_some_and(|e| {
            let r = e.get_bounding_client_rect();
            (ev.client_y() as f64) > r.top() + r.height() / 2.0
        });
        if d.at.get_untracked() != Some((i, after)) {
            d.at.set(Some((i, after)));
        }
    };
    let on_drop = move |ev: DragEvent| {
        let Some(d) = drag else { return };
        ev.prevent_default();
        let (Some(from), Some((target, after))) = (d.from.get_untracked(), d.at.get_untracked()) else { return };
        d.from.set(None);
        d.at.set(None);
        let total = count.get_untracked();
        let before = if after { target + 1 } else { target };
        let before = (before < total).then_some(before);
        if before == Some(from) || before == Some(from + 1) || (before.is_none() && from + 1 == total) {
            return;
        }
        edit(tab, BoxEdit::MoveP { at: at.get_value(), n: from, before });
    };
    let marker = move || match (drag, n) {
        (Some(d), Some(i)) => d.at.with(|a| match a {
            Some((p, after)) if *p == i => if *after { "drop-after" } else { "drop-before" },
            _ => "",
        }),
        _ => "",
    };
    let dragging = move || matches!((drag, n), (Some(d), Some(i)) if d.from.get() == Some(i));

    // What the trigger prints before the box; what a dialog line prints as.
    let note = move || {
        let t = text();
        if t.trim().is_empty() {
            return None;
        }
        let shown = refctx.with(|c| printed(&t, c))?;
        match n {
            None => {
                let s = shown.trim_end_matches([',', '.', ';', ':', ' ']).to_string();
                Some(format!("Prints before the box: \u{201c}{s}, read or paraphrase the following:\u{201d}"))
            }
            Some(_) if tag.get() == "dialog" && !speaker.get().trim().is_empty() => {
                Some(format!("Prints as: {}: \u{201c}{shown}\u{201d}", speaker.get()))
            }
            Some(_) => None,
        }
    };
    let id = match n {
        None => "box-trigger".to_string(),
        Some(i) => format!("box-p-{i}"),
    };

    view! {
        <div node_ref=el class=move || format!("body-block para {}", marker())
            class:dirty=dirty class:failed=failed class:dragging=dragging
            on:dragover=on_dragover on:drop=on_drop
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="block-gutter">
                {n.is_some().then(|| view! {
                    <span class="grip" draggable="true" title="Drag to reorder" on:dragstart=on_dragstart on:dragend=on_dragend>"⠿"</span>
                })}
            </div>
            <div class="block-main">
                <textarea node_ref=ta class="p-text" class:trigger-text=n.is_none() id=id spellcheck="true"
                    rows=if n.is_none() { 1 } else { 2 }
                    placeholder=if n.is_none() { "(no trigger) e.g. When the characters enter the hall" } else { "(empty paragraph)" }
                    prop:value=text on:input=move |ev| set(event_target_value(&ev))></textarea>
                <RefToolsView tools tab/>
                {move || note().map(|s| view! { <p class="hint box-note">{s}</p> })}
                <div class="p-bar">
                    {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                    {move || (dirty() && !failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                    <span class="spacer"></span>
                    <button class="small" title="Insert a ref tag at the cursor" on:click=move |_| tools.open_insert()>"+ Ref"</button>
                    <button class="small" title=if n.is_none() { "Insert an empty paragraph first in the box" } else { "Insert an empty paragraph below" }
                        disabled=busy on:click=add_below>{if n.is_none() { "+ ¶ first" } else { "+ ¶ below" }}</button>
                    <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                    <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
                    {match n {
                        None => view! {
                            <button class="icon-btn danger" title="Remove the trigger"
                                disabled=move || busy() || !view.with(|v| v.as_ref().is_some_and(|v| v.has_trigger))
                                on:click=clear_trigger>"✕"</button>
                        }.into_any(),
                        Some(_) => view! {
                            <button class="icon-btn danger" title="Delete paragraph" disabled=busy on:click=delete>"✕"</button>
                        }.into_any(),
                    }}
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn BoxForm(tab: DocTab, at: BoxAt) -> impl IntoView {
    let at = StoredValue::new(at);
    let view = Memo::new(move |_| {
        let d = tab.doc();
        boxes::find(&d.source, &d.nodes, &at.get_value()).map(|i| boxes::read(&d.source, &d.nodes, i))
    });
    let exists = Memo::new(move |_| view.with(|v| v.is_some()));
    let count = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.paras.len()).unwrap_or(0)));
    let is_dialog = Memo::new(move |_| view.with(|v| v.as_ref().is_some_and(|v| v.tag == "dialog")));
    let refctx = Memo::new(move |_| {
        let d = tab.doc();
        let index = tab.refs.with(|r| r.as_ref().and_then(|r| r.as_ref().ok().cloned()));
        RefCtx::from_doc(&d.source, &d.nodes, index)
    });
    let drag = Drag { from: RwSignal::new(None), at: RwSignal::new(None) };

    // Focus a paragraph just added.
    Effect::new(move |_| {
        let Some(slot) = tab.box_focus.get() else { return };
        count.track();
        let id = match slot {
            None => "box-trigger".to_string(),
            Some(i) => format!("box-p-{i}"),
        };
        request_animation_frame(move || {
            let doc = web_sys::window().and_then(|w| w.document());
            if let Some(e) = doc.and_then(|d| d.get_element_by_id(&id)) {
                e.scroll_into_view();
                if let Ok(h) = e.dyn_into::<web_sys::HtmlElement>() {
                    let _ = h.focus();
                }
            }
        });
        tab.box_focus.set(None);
    });

    view! {
        {move || if !exists.get() {
            view! { <div class="empty"><p>"This box isn't in the file any more."</p></div> }.into_any()
        } else {
            view! {
                <div class="table-editor box-editor" class:dialog=is_dialog>
                    <BoxInfo tab at view/>
                    <div class="table-card">
                        <div class="table-card-title">
                            "Trigger"
                            <span class="hint">"<trigger>: when to read the box. Optional; empty removes it. A chapter can't open with one: its first paragraph must come before any trigger."</span>
                        </div>
                        <div class="block-body">
                            <TextRow tab at n=None view refctx/>
                        </div>
                    </div>
                    <div class="table-card box-body">
                        <div class="table-card-title">
                            {move || format!("Paragraphs ({})", count.get())}
                            <span class="hint">"Each paragraph is its XML, with the line under it showing it as printed; click a ref to edit it."</span>
                        </div>
                        <Show when=move || { count.get() == 0 }>
                            <p class="hint">"No paragraphs: the box prints empty. Add one with \u{201c}+ \u{b6} first\u{201d} on the trigger."</p>
                        </Show>
                        <div class="block-body">
                            <For each=move || 0..count.get() key=|i| *i let:i>
                                <TextRow tab at n=Some(i) view refctx drag/>
                            </For>
                            <div class="body-end"
                                on:dragover=move |ev: DragEvent| if drag.from.get_untracked().is_some() { ev.prevent_default() }
                                on:drop=move |ev: DragEvent| {
                                    ev.prevent_default();
                                    if let Some(from) = drag.from.get_untracked() {
                                        drag.from.set(None);
                                        drag.at.set(None);
                                        if from + 1 != count.get_untracked() {
                                            edit(tab, BoxEdit::MoveP { at: at.get_value(), n: from, before: None });
                                        }
                                    }
                                }></div>
                        </div>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
