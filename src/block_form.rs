//! The section and sidebar editor. A section is prose in the chapter's flow; a sidebar
//! is a boxed aside that prints where a `<sidebarLink>` places it. Both get the same
//! frame: a header form, then the body in document order: each `<p>` a text box of its
//! XML, every other child a read-only block. Each paragraph and the header apply on
//! their own (Apply, Ctrl+Enter, or leaving the box); add, delete and drag-to-reorder
//! apply at once. Everything is validated before the file is written.

use leptos::ev::{DragEvent, FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::block::{self, Block, BlockEdit, Container, Header};
use shared::display::tokenize;
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::state::DocTab;

fn highlighted(text: &str) -> impl IntoView {
    tokenize(text).into_iter().map(|(kind, s)| view! { <span class=kind.class()>{s.to_string()}</span> }).collect_view()
}

/// Does `ev`'s new focus stay inside `el`?
fn focus_stays(ev: &FocusEvent, el: &NodeRef<html::Div>) -> bool {
    ev.related_target()
        .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
        .zip(el.get())
        .is_some_and(|(n, e)| e.contains(Some(&n)))
}

fn tag_name(tag: &str) -> &'static str {
    match tag {
        "dialog" => "Dialog",
        "speakAloud" => "Read aloud",
        "list" => "List",
        "image" => "Image",
        "figure" => "Figure",
        "table" => "Table",
        "tableLink" => "Table link",
        "sidebarLink" => "Sidebar link",
        "parcelLink" => "Treasure link",
        "monsterStatblock" => "Stat block",
        "magicItemBlock" => "Magic item",
        "spellBlock" => "Spell",
        "skillcheckResultTable" => "Skill check table",
        "pageEnd" => "Page end",
        _ => "Element",
    }
}

#[component]
fn HeaderForm(tab: DocTab, at: StoredValue<Container>, base: Memo<Option<Header>>, links: Memo<usize>) -> impl IntoView {
    let is_sidebar = matches!(at.get_value(), Container::Sidebar { .. });
    let model = move || {
        let a = at.get_value();
        tab.header_drafts.with(|d| d.get(&a).cloned()).or_else(|| base.get()).unwrap_or_default()
    };
    let update = move |f: &dyn Fn(&mut Header)| {
        let a = at.get_value();
        let b = base.get_untracked().unwrap_or_default();
        let mut m = tab.header_drafts.with_untracked(|d| d.get(&a).cloned()).unwrap_or_else(|| b.clone());
        f(&mut m);
        tab.header_drafts.update(|d| {
            if m == b {
                d.remove(&a);
            } else {
                d.insert(a, m);
            }
        });
    };
    let dirty = move || tab.header_drafts.with(|d| d.contains_key(&at.get_value()));
    let busy = move || tab.applying.get();
    let do_apply = move || {
        if !dirty() || model().key.trim().is_empty() {
            return;
        }
        apply(tab, Change::Block { edit: BlockEdit::Header { at: at.get_value(), header: model() } });
    };
    let revert = move || {
        let a = at.get_value();
        tab.header_drafts.update(|d| {
            d.remove(&a);
        });
    };
    let el = NodeRef::<html::Div>::new();
    let renamed = move || base.get().is_some_and(|b| b.key != model().key);
    let field = move |label: &'static str, hint: &'static str, get: fn(&Header) -> String, set: fn(&mut Header, String)| {
        view! {
            <label class="field">
                <span class="field-label">{label}</span>
                <input type="text" prop:value=move || get(&model()) on:input=move |ev| {
                    let v = event_target_value(&ev);
                    update(&|m| set(m, v.clone()));
                }/>
                <span class="hint">{hint}</span>
            </label>
        }
    };
    view! {
        <div node_ref=el class="block-header" class:dirty=dirty
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { do_apply() }
            }>
            <div class="field-row">
                {field("Title", "<title>: the heading; leave empty for none", |h| h.title.clone(), |h, v| h.title = v)}
                {field("Subtitle", "<subtitle>", |h| h.subtitle.clone(), |h, v| h.subtitle = v)}
            </div>
            <div class="field-row">
                {if is_sidebar {
                    view! {
                        {field("Id", "@id: what <sidebarLink> places", |h| h.key.clone(), |h, v| h.key = v)}
                        {field("Style", "@style", |h| h.extra.clone(), |h, v| h.extra = v)}
                    }.into_any()
                } else {
                    view! {
                        {field("Name", "@name: a slug for this section (nothing points at it)", |h| h.key.clone(), |h, v| h.key = v)}
                        {field("Credit", "@credit: names separated by \";\", printed in the chapter's \"Written by\" line", |h| h.extra.clone(), |h, v| h.extra = v)}
                    }.into_any()
                }}
            </div>
            <Show when=move || is_sidebar && renamed()>
                <p class="hint warn-text">{move || format!("Apply also renames the {} <sidebarLink>(s) that place it.", links.get())}</p>
            </Show>
            <Show when=move || model().key.trim().is_empty()>
                <p class="hint bad">{if is_sidebar { "A sidebar needs an id." } else { "A section needs a name." }}</p>
            </Show>
            <div class="ref-actions">
                <span class="hint">"Applies when you leave these fields · Ctrl+Enter applies · Esc reverts"</span>
                <span class="spacer"></span>
                <button on:click=move |_| revert() disabled=move || !dirty() || busy()>"Revert"</button>
                <button class="primary" on:click=move |_| do_apply() disabled=move || !dirty() || busy()>"Apply"</button>
            </div>
        </div>
    }
}

/// Drag state shared by the body's blocks: the child being dragged, and where it would drop.
#[derive(Clone, Copy)]
struct Drag {
    from: RwSignal<Option<usize>>,
    at: RwSignal<Option<(usize, bool)>>,
}

#[component]
fn BlockRow(
    tab: DocTab,
    at: StoredValue<Container>,
    /// Position in the body list.
    pos: usize,
    blocks: Memo<Vec<Block>>,
    drag: Drag,
) -> impl IntoView {
    let block = Memo::new(move |_| blocks.with(|b| b.get(pos).cloned()));
    let child = move || block.get().map(|b| b.child).unwrap_or(0);
    // A memo, so a row re-renders only when it changes kind (not on every edit).
    let is_p = Memo::new(move |_| block.with(|b| b.as_ref().is_some_and(|b| b.tag == "p")));
    let key = move || (at.get_value(), child());
    // A draft counts only while the paragraph still has the text it was made from.
    let draft = move || {
        let base = block.get().map(|b| b.text).unwrap_or_default();
        tab.p_drafts.with(|d| d.get(&key()).filter(|(orig, _)| *orig == base).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(|| block.get().map(|b| b.text).unwrap_or_default());
    let dirty = move || draft().is_some();
    let failed = move || tab.p_failed.with(|f| f.contains(&key()));
    let busy = move || tab.applying.get();
    let open = RwSignal::new(false);
    let el = NodeRef::<html::Div>::new();

    let set_text = move |v: String| {
        let base = block.get_untracked().map(|b| b.text).unwrap_or_default();
        let k = (at.get_value(), block.get_untracked().map(|b| b.child).unwrap_or(0));
        tab.p_drafts.update(|d| {
            if v == base {
                d.remove(&k);
            } else {
                d.insert(k, (base, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            apply(tab, Change::Block { edit: BlockEdit::SetP { at: at.get_value(), child: child(), text: t } });
        }
    };
    let revert = move || {
        let k = key();
        tab.p_drafts.update(|d| {
            d.remove(&k);
        });
        tab.p_failed.update(|f| {
            f.remove(&k);
        });
    };
    let add_after = move |_: MouseEvent| {
        apply(tab, Change::Block { edit: BlockEdit::AddP { at: at.get_value(), after: Some(child()) } });
    };
    let delete = move |_: MouseEvent| {
        let empty = block.get_untracked().is_some_and(|b| b.text.trim().is_empty());
        let ok = empty
            || web_sys::window().and_then(|w| w.confirm_with_message("Delete this paragraph?").ok()).unwrap_or(false);
        if ok {
            apply(tab, Change::Block { edit: BlockEdit::DeleteP { at: at.get_value(), child: child() } });
        }
    };

    // Drag by the grip; drop on a block's top or bottom half.
    let on_dragstart = move |ev: DragEvent| {
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &child().to_string());
            dt.set_effect_allowed("move");
        }
        drag.from.set(Some(child()));
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
        if drag.at.get_untracked() != Some((pos, after)) {
            drag.at.set(Some((pos, after)));
        }
    };
    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        let (Some(from), Some((target, after))) = (drag.from.get_untracked(), drag.at.get_untracked()) else { return };
        drag.from.set(None);
        drag.at.set(None);
        let list = blocks.get_untracked();
        // "After the last block" is the end of the body.
        let before = if after { list.get(target + 1).map(|b| b.child) } else { list.get(target).map(|b| b.child) };
        if before == Some(from) {
            return;
        }
        apply(tab, Change::Block { edit: BlockEdit::Move { at: at.get_value(), child: from, before } });
    };
    let marker = move || {
        drag.at.with(|d| match d {
            Some((p, after)) if *p == pos => if *after { "drop-after" } else { "drop-before" },
            _ => "",
        })
    };

    view! {
        <div node_ref=el class=move || format!("body-block {}", marker())
            class:para=is_p class:dirty=dirty class:failed=failed
            class:dragging=move || drag.from.get() == Some(child())
            on:dragover=on_dragover on:drop=on_drop
            on:focusout=move |ev: FocusEvent| if is_p.get_untracked() && !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if !is_p.get_untracked() { return }
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="block-gutter">
                <span class="grip" draggable="true" title="Drag to reorder" on:dragstart=on_dragstart on:dragend=on_dragend>"⠿"</span>
            </div>
            {move || if is_p.get() {
                view! {
                    <div class="block-main">
                        <textarea class="p-text" id=move || format!("p-{}", child()) spellcheck="true"
                            prop:value=text on:input=move |ev| set_text(event_target_value(&ev))></textarea>
                        <div class="p-bar">
                            {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                            {move || (dirty() && !failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                            <span class="spacer"></span>
                            <button class="small" title="Insert an empty paragraph below" disabled=busy on:click=add_after>"+ ¶ below"</button>
                            <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                            <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
                            <button class="icon-btn danger" title="Delete paragraph" disabled=busy on:click=delete>"✕"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                let b = block.get().unwrap_or(Block { child: 0, tag: String::new(), text: String::new(), source: String::new() });
                let src = b.source.clone();
                view! {
                    <div class="block-main readonly">
                        <div class="ro-head" on:click=move |_| open.update(|o| *o = !*o)>
                            <span class=format!("ro-tag tag-{}", b.tag)>{tag_name(&b.tag)}</span>
                            <code class="ro-el">{format!("<{}>", b.tag)}</code>
                            <span class="ro-summary">{b.text.clone()}</span>
                            <span class="spacer"></span>
                            <button class="small" title="Insert an empty paragraph below" disabled=busy on:click=move |ev: MouseEvent| {
                                ev.stop_propagation();
                                add_after(ev);
                            }>"+ ¶ below"</button>
                            <span class="twist">{move || if open.get() { "▾" } else { "▸" }}</span>
                        </div>
                        <Show when=move || open.get()>
                            <pre class="code ro-source">{highlighted(&src)}</pre>
                            <p class="hint">"Read-only for now: this element gets its own editor later."</p>
                        </Show>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

#[component]
pub fn BlockForm(tab: DocTab, at: Container) -> impl IntoView {
    let is_sidebar = matches!(at, Container::Sidebar { .. });
    let at = StoredValue::new(at);
    let data = Memo::new(move |_| {
        let d = tab.doc();
        block::find(&d.source, &d.nodes, &at.get_value()).map(|ci| block::read(&d.source, &d.nodes, ci))
    });
    let header = Memo::new(move |_| data.get().map(|(h, _)| h));
    let blocks = Memo::new(move |_| data.get().map(|(_, b)| b).unwrap_or_default());
    let links = Memo::new(move |_| {
        let d = tab.doc();
        match at.get_value() {
            Container::Sidebar { id, .. } => block::sidebar_links(&d.nodes, &d.source, &id),
            _ => 0,
        }
    });
    let drag = Drag { from: RwSignal::new(None), at: RwSignal::new(None) };
    let busy = move || tab.applying.get();

    // Scroll to and focus a paragraph picked in the tree, or one just added.
    Effect::new(move |_| {
        let Some(child) = tab.block_focus.get() else { return };
        blocks.track();
        request_animation_frame(move || {
            let doc = web_sys::window().and_then(|w| w.document());
            if let Some(e) = doc.and_then(|d| d.get_element_by_id(&format!("p-{child}"))) {
                e.scroll_into_view();
                if let Ok(h) = e.dyn_into::<web_sys::HtmlElement>() {
                    let _ = h.focus();
                }
            }
        });
        tab.block_focus.set(None);
    });

    let add_first = move |_: MouseEvent| {
        // The new paragraph becomes the first body child: right after the title/subtitle.
        let first = blocks.get_untracked().first().map(|b| b.child);
        let n_header = header.get_untracked().map(|h| (!h.title.is_empty()) as usize + (!h.subtitle.is_empty()) as usize).unwrap_or(0);
        tab.block_focus.set(Some(first.unwrap_or(n_header)));
        apply(tab, Change::Block { edit: BlockEdit::AddP { at: at.get_value(), after: None } });
    };

    view! {
        {move || match data.get() {
            None => view! { <div class="empty"><p>"This section isn't in the file any more."</p></div> }.into_any(),
            Some(_) => view! {
                <div class="block-editor" class:sidebar=is_sidebar>
                    <div class="block-kind">
                        <span class="block-kind-name">{if is_sidebar { "Sidebar" } else { "Section" }}</span>
                        <span class="hint">{if is_sidebar {
                            "A boxed aside, printed where a <sidebarLink> places it. Holds paragraphs, tables and lists."
                        } else {
                            "Prose in the chapter's flow; its title prints as a heading."
                        }}</span>
                        {move || is_sidebar.then(|| {
                            let n = links.get();
                            view! { <span class="uses" class:unused=n == 0>{if n == 0 { "not placed by any <sidebarLink>".to_string() } else { format!("placed by {n} <sidebarLink>") }}</span> }
                        })}
                    </div>
                    <HeaderForm tab at base=header links/>
                    <div class="block-body">
                        <div class="body-tools">
                            <span class="hint">"Each paragraph is its XML: inline tags like <b> and <monsterRef/> are kept. Write & as &amp;."</span>
                            <span class="spacer"></span>
                            <button class="small" disabled=busy on:click=add_first>"+ ¶ at start"</button>
                        </div>
                        <For each=move || 0..blocks.with(|b| b.len()) key=|i| *i let:pos>
                            <BlockRow tab at pos blocks drag/>
                        </For>
                        <Show when=move || blocks.with(|b| b.is_empty())>
                            <p class="hint">"No paragraphs yet."</p>
                        </Show>
                        <div class="body-end"
                            on:dragover=move |ev: DragEvent| if drag.from.get_untracked().is_some() { ev.prevent_default() }
                            on:drop=move |ev: DragEvent| {
                                ev.prevent_default();
                                if let Some(from) = drag.from.get_untracked() {
                                    drag.from.set(None);
                                    drag.at.set(None);
                                    apply(tab, Change::Block { edit: BlockEdit::Move { at: at.get_value(), child: from, before: None } });
                                }
                            }></div>
                    </div>
                </div>
            }.into_any(),
        }}
    }
}
