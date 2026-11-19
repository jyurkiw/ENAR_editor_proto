//! The list editor: a `<list>` in a section or sidebar. The list's own settings (bullets
//! or numbers, @style) on top, then its items, each `<li>` a text box of its XML. Items
//! apply one at a time (Apply, Ctrl+Enter, or leaving the box); add, delete and
//! drag-to-reorder apply at once. Deleting the last item deletes the list.

use leptos::ev::{DragEvent, FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::block::{self, Container};
use shared::list::{self, ListAt, ListEdit, ListView};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::dialogs::ask;
use crate::prose_editor::ChipEditor;
use shared::inline::RefCtx;
use crate::state::DocTab;

fn focus_stays(ev: &FocusEvent, el: &NodeRef<html::Div>) -> bool {
    ev.related_target()
        .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
        .zip(el.get())
        .is_some_and(|(n, e)| e.contains(Some(&n)))
}


fn edit(tab: DocTab, e: ListEdit) {
    apply(tab, Change::List { edit: e });
}

#[derive(Clone, Copy)]
struct Drag {
    from: RwSignal<Option<usize>>,
    at: RwSignal<Option<(usize, bool)>>,
}

#[component]
fn ListInfo(tab: DocTab, at: StoredValue<ListAt>, view: Memo<Option<ListView>>) -> impl IntoView {
    let busy = move || tab.applying.get();
    let bullet = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.bullet.clone()).unwrap_or_default()));
    let base_style = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.style.clone()).unwrap_or_default()));
    // An unapplied @style edit: (what it was made from, the new text).
    let style_draft = RwSignal::new(None::<(String, String)>);
    let style = move || {
        let b = base_style.get();
        style_draft.get().filter(|(o, _)| *o == b).map(|(_, t)| t).unwrap_or(b)
    };
    let style_dirty = move || style_draft.get().is_some_and(|(o, t)| o == base_style.get() && t != o);
    let el = NodeRef::<html::Div>::new();
    let apply_style = move || {
        if style_dirty() {
            edit(tab, ListEdit::Attrs { at: at.get_value(), bullet: bullet.get_untracked(), style: style() });
            style_draft.set(None);
        }
    };
    let set_bullet = move |b: String| {
        edit(tab, ListEdit::Attrs { at: at.get_value(), bullet: b, style: base_style.get_untracked() });
    };
    let n_items = move || view.with(|v| v.as_ref().map(|v| v.items.len()).unwrap_or(0));
    let delete = move |_: MouseEvent| {
        ask(format!("Delete this list and its {} item(s)?", n_items()), move || edit(tab, ListEdit::Delete { at: at.get_value() }));
    };
    let open_container = move |_: MouseEvent| {
        let d = tab.doc_untracked();
        if let Some(ci) = block::find(&d.source, &d.nodes, &at.get_value().at) {
            tab.reveal(ci);
        }
    };
    let where_ = move || match at.get_value().at {
        Container::Section { name, .. } => format!("in section \"{name}\""),
        Container::Sidebar { id, .. } => format!("in sidebar \"{id}\""),
        Container::Description => "in the product description".to_string(),
    };
    view! {
        <div node_ref=el class="table-card" class:dirty=style_dirty
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { apply_style() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { style_draft.set(None) } else if ev.key() == "Enter" { apply_style() }
            }>
            <div class="table-card-title">
                "List"
                <span class="hint">{where_}</span>
                <button class="link" title="Open the section or sidebar editor" on:click=open_container>"open it"</button>
                <span class="spacer"></span>
                <button class="small danger" disabled=busy on:click=delete>"Delete list"</button>
            </div>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Markers"</span>
                    <select disabled=busy on:change=move |ev| set_bullet(event_target_value(&ev))>
                        <option value="bullet" selected=move || bullet.get() == "bullet">"• Bullets"</option>
                        <option value="number" selected=move || bullet.get() == "number">"1. Numbers (bullet=\"number\")"</option>
                    </select>
                    <span class="hint">"Applies when changed"</span>
                </label>
                <label class="field">
                    <span class="field-label">"Style"</span>
                    <input type="text" prop:value=style on:input=move |ev| {
                        style_draft.set(Some((base_style.get_untracked(), event_target_value(&ev))))
                    }/>
                    <span class="hint">"@style: kept in the file; the compiler doesn't use it. Empty removes it."</span>
                </label>
            </div>
        </div>
    }
}

#[component]
fn ItemRow(tab: DocTab, at: StoredValue<ListAt>, n: usize, items: Memo<Vec<String>>, drag: Drag, numbered: Memo<bool>, refctx: Memo<RefCtx>) -> impl IntoView {
    let _ = refctx;
    let base = Memo::new(move |_| items.with(|i| i.get(n).cloned().unwrap_or_default()));
    let count = Memo::new(move |_| items.with(|i| i.len()));
    let key = move || (at.get_value(), n);
    let draft = move || {
        let b = base.get();
        tab.li_drafts.with(|d| d.get(&key()).filter(|(o, _)| *o == b).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(|| base.get());
    let dirty = move || draft().is_some();
    let failed = move || tab.li_failed.with(|f| f.contains(&key()));
    let busy = move || tab.applying.get();
    let el = NodeRef::<html::Div>::new();
    let set = move |v: String| {
        let b = base.get_untracked();
        let k = (at.get_value(), n);
        tab.li_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            edit(tab, ListEdit::SetItem { at: at.get_value(), n, text: t });
        }
    };
    let revert = move || {
        let k = key();
        tab.li_drafts.update(|d| {
            d.remove(&k);
        });
        tab.li_failed.update(|f| {
            f.remove(&k);
        });
    };

    let add_below = move |_: MouseEvent| {
        tab.li_focus.set(Some(n + 1));
        edit(tab, ListEdit::AddItem { at: at.get_value(), after: Some(n) });
    };
    let delete = move |_: MouseEvent| {
        let only = count.get_untracked() == 1;
        let empty = base.get_untracked().trim().is_empty();
        let del = move || edit(tab, ListEdit::DeleteItem { at: at.get_value(), n });
        if only {
            ask("Delete the only item? The <list> goes with it.", del)
        } else if empty {
            del()
        } else {
            ask("Delete this item?", del)
        }
    };

    let on_dragstart = move |ev: DragEvent| {
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &n.to_string());
            dt.set_effect_allowed("move");
        }
        drag.from.set(Some(n));
    };
    let on_dragend = move |_: DragEvent| {
        drag.from.set(None);
        drag.at.set(None);
    };
    let on_dragover = move |ev: DragEvent| {
        if drag.from.get_untracked().is_none() {
            return;
        }
        ev.prevent_default();
        let after = el.get().is_some_and(|e| {
            let r = e.get_bounding_client_rect();
            (ev.client_y() as f64) > r.top() + r.height() / 2.0
        });
        if drag.at.get_untracked() != Some((n, after)) {
            drag.at.set(Some((n, after)));
        }
    };
    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        let (Some(from), Some((target, after))) = (drag.from.get_untracked(), drag.at.get_untracked()) else { return };
        drag.from.set(None);
        drag.at.set(None);
        let before = if after { target + 1 } else { target };
        let before = (before < count.get_untracked()).then_some(before);
        if before == Some(from) || before == Some(from + 1) || (before.is_none() && from + 1 == count.get_untracked()) {
            return;
        }
        edit(tab, ListEdit::MoveItem { at: at.get_value(), n: from, before });
    };
    let marker = move || {
        drag.at.with(|d| match d {
            Some((p, after)) if *p == n => if *after { "drop-after" } else { "drop-before" },
            _ => "",
        })
    };

    view! {
        <div node_ref=el class="body-block para" class:drop-before=move || marker() == "drop-before" class:drop-after=move || marker() == "drop-after"
            class:dirty=dirty class:failed=failed class:dragging=move || drag.from.get() == Some(n)
            on:dragover=on_dragover on:drop=on_drop
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="block-gutter li-gutter">
                <span class="grip" draggable="true" title="Drag to reorder" on:dragstart=on_dragstart on:dragend=on_dragend>"⠿"</span>
                <span class="li-marker">{move || if numbered.get() { format!("{}.", n + 1) } else { "•".into() }}</span>
            </div>
            <div class="block-main">
                <ChipEditor value=Signal::derive(text) set=Callback::new(move |s: String| set(s)) placeholder="(empty item)"
                    id=Signal::derive(move || format!("li-{n}"))/>
                <div class="p-bar">
                    {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                    {move || (dirty() && !failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                    <span class="spacer"></span>
                    <button class="small" title="Insert an empty item below" disabled=busy on:click=add_below>"+ item below"</button>
                    <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                    <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
                    <button class="icon-btn danger" disabled=busy
                        title=move || if count.get() == 1 { "Delete the item (and the list: it's the only one)" } else { "Delete item" }
                        on:click=delete>"✕"</button>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn ListForm(tab: DocTab, at: ListAt) -> impl IntoView {
    let at = StoredValue::new(at);
    let view = Memo::new(move |_| {
        let d = tab.doc();
        list::find(&d.source, &d.nodes, &at.get_value()).map(|i| list::read(&d.source, &d.nodes, i))
    });
    let exists = Memo::new(move |_| view.with(|v| v.is_some()));
    let items = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.items.clone()).unwrap_or_default()));
    let numbered = Memo::new(move |_| view.with(|v| v.as_ref().is_some_and(|v| v.bullet == "number")));
    let drag = Drag { from: RwSignal::new(None), at: RwSignal::new(None) };
    let busy = move || tab.applying.get();
    let refctx = Memo::new(move |_| {
        let d = tab.doc();
        let index = tab.refs.with(|r| r.as_ref().and_then(|r| r.as_ref().ok().cloned()));
        RefCtx::from_doc(&d.source, &d.nodes, index)
    });

    // Focus an item just added.
    Effect::new(move |_| {
        let Some(n) = tab.li_focus.get() else { return };
        items.track();
        request_animation_frame(move || {
            let doc = web_sys::window().and_then(|w| w.document());
            if let Some(e) = doc.and_then(|d| d.get_element_by_id(&format!("li-{n}"))) {
                e.scroll_into_view();
                if let Ok(h) = e.dyn_into::<web_sys::HtmlElement>() {
                    let _ = h.focus();
                }
            }
        });
        tab.li_focus.set(None);
    });

    let add_first = move |_: MouseEvent| {
        tab.li_focus.set(Some(0));
        edit(tab, ListEdit::AddItem { at: at.get_value(), after: None });
    };

    view! {
        {move || if !exists.get() {
            view! { <div class="empty"><p>"This list isn't in the file any more."</p></div> }.into_any()
        } else {
            view! {
                <div class="table-editor list-editor">
                    <ListInfo tab at view/>
                    <div class="table-card">
                        <div class="table-card-title">
                            {move || format!("Items ({})", items.with(|i| i.len()))}
                            <span class="hint">"Type text; refs and tags show as chips. Click a chip to edit it; ＋ or Ctrl+K inserts one; Ctrl+B and Ctrl+I bold and italicize. Ctrl+Enter applies."</span>
                        </div>
                        <div class="th-tools">
                            <button class="small" disabled=busy on:click=add_first>"+ Item at the start"</button>
                        </div>
                        <div class="block-body">
                            <For each=move || 0..items.with(|i| i.len()) key=|n| *n let:n>
                                <ItemRow tab at n items drag numbered refctx/>
                            </For>
                            <div class="body-end"
                                on:dragover=move |ev: DragEvent| if drag.from.get_untracked().is_some() { ev.prevent_default() }
                                on:drop=move |ev: DragEvent| {
                                    ev.prevent_default();
                                    if let Some(from) = drag.from.get_untracked() {
                                        drag.from.set(None);
                                        drag.at.set(None);
                                        if from + 1 != items.with_untracked(|i| i.len()) {
                                            edit(tab, ListEdit::MoveItem { at: at.get_value(), n: from, before: None });
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
