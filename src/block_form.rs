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
use shared::inline::RefCtx;
use shared::Change;
use wasm_bindgen::JsCast;

use shared::statblock as sb;

use crate::apply::apply;
use crate::prose_editor::ChipEditor;
use crate::section_el_form;
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

/// The non-paragraph blocks a container can take, for the add menu.
fn other_blocks(at: &Container) -> Vec<(&'static str, &'static str)> {
    match at {
        Container::Section { .. } => {
            let mut v = vec![("list", "List"), ("speakAloud", "Read-aloud box"), ("dialog", "Dialog")];
            v.extend(section_el_form::SECTION_BLOCKS.iter().map(|t| (*t, section_el_form::label(t))));
            v
        }
        Container::Sidebar { .. } => vec![("list", "List"), ("table", "Table")],
    }
}

/// A new block of kind `what` after child `after`.
fn add_block(tab: DocTab, at: Container, after: Option<usize>, what: &str) {
    let edit = match what {
        "list" => BlockEdit::AddList { at, after },
        "speakAloud" | "dialog" => BlockEdit::AddBox { at, after, tag: what.to_string() },
        other => return section_el_form::add(tab, at, after, other),
    };
    apply(tab, Change::Block { edit });
}

/// A select that adds a block and resets: "+ other below ▾".
fn add_menu(tab: DocTab, at: StoredValue<Container>, label: &'static str, after: impl Fn() -> Option<usize> + Copy + Send + Sync + 'static) -> impl IntoView {
    let busy = move || tab.applying.get();
    view! {
        <select class="add-menu" title="Insert a new block" disabled=busy
            on:click=move |ev: MouseEvent| ev.stop_propagation()
            on:change=move |ev| {
                let v = event_target_value(&ev);
                if let Some(s) = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok()) {
                    s.set_value("");
                }
                if !v.is_empty() {
                    add_block(tab, at.get_value(), after(), &v);
                }
            }>
            <option value="" selected=true>{label}</option>
            {move || {
                let t = sb::read(&tab.doc().source, sb::ROOT).map(|r| section_el_form::targets(&r)).unwrap_or_default();
                other_blocks(&at.get_value()).into_iter().map(|(v, l)| {
                    let why = section_el_form::missing(v, &t);
                    let text = match why { Some(w) => format!("{l} ({w})"), None => l.to_string() };
                    view! { <option value=v disabled=why.is_some()>{text}</option> }
                }).collect_view()
            }}
        </select>
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
    refctx: Memo<RefCtx>,
) -> impl IntoView {
    // Chips name their refs from the tab's prose environment now.
    let _ = refctx;
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
        let c = child();
        let del = move || apply(tab, Change::Block { edit: BlockEdit::DeleteP { at: at.get_value(), child: c } });
        if empty { del() } else { crate::dialogs::ask("Delete this paragraph?", del) }
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
            // A row being removed while focused still gets focusout, after its signals are gone.
            on:focusout=move |ev: FocusEvent| if is_p.try_get_untracked().unwrap_or(false) && !focus_stays(&ev, &el) { do_apply() }
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
                        <ChipEditor value=Signal::derive(text) set=Callback::new(move |s: String| set_text(s)) placeholder="(empty paragraph)"
                            id=Signal::derive(move || format!("p-{}", child()))/>
                        <div class="p-bar">
                            {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                            {move || (dirty() && !failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                            <span class="spacer"></span>
                            <button class="small" title="Insert an empty paragraph below" disabled=busy on:click=add_after>"+ ¶ below"</button>
                            {add_menu(tab, at, "+ other below…", move || Some(child()))}
                            <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                            <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
                            <button class="icon-btn danger" title="Delete paragraph" disabled=busy on:click=delete>"✕"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                let b = block.get().unwrap_or(Block { child: 0, tag: String::new(), text: String::new(), source: String::new() });
                let src = b.source.clone();
                // Lists and tables have their own editors: their summary links there.
                let has_editor = matches!(b.tag.as_str(), "table" | "list" | "speakAloud" | "dialog") || section_el_form::SECTION_BLOCKS.contains(&b.tag.as_str());
                let open_editor = move |ev: MouseEvent| {
                    ev.stop_propagation();
                    let d = tab.doc_untracked();
                    if let Some(ci) = block::find(&d.source, &d.nodes, &at.get_value()) {
                        if let Some(k) = d.nodes[ci].children.get(child()) {
                            tab.reveal(*k);
                        }
                    }
                };
                let editor_name = match b.tag.as_str() {
                    "list" => "list editor",
                    "table" => "table editor",
                    "speakAloud" | "dialog" => "box editor",
                    _ => "block's page",
                };
                view! {
                    <div class="block-main readonly">
                        <div class="ro-head" on:click=move |_| open.update(|o| *o = !*o)>
                            <span class=format!("ro-tag tag-{}", b.tag)>{tag_name(&b.tag)}</span>
                            <code class="ro-el">{format!("<{}>", b.tag)}</code>
                            {if has_editor {
                                view! {
                                    <button type="button" class="link ro-summary ro-link" title=format!("Open the {editor_name}")
                                        on:click=open_editor>{b.text.clone()}</button>
                                }.into_any()
                            } else {
                                view! { <span class="ro-summary">{b.text.clone()}</span> }.into_any()
                            }}
                            <span class="spacer"></span>
                            {has_editor.then(|| view! {
                                <button class="small" title=format!("Open the {editor_name}") on:click=open_editor>"Open"</button>
                            })}
                            <button class="small" title="Insert an empty paragraph below" disabled=busy on:click=move |ev: MouseEvent| {
                                ev.stop_propagation();
                                add_after(ev);
                            }>"+ ¶ below"</button>
                            {add_menu(tab, at, "+ other below…", move || Some(child()))}
                            <span class="twist">{move || if open.get() { "▾" } else { "▸" }}</span>
                        </div>
                        <Show when=move || open.get()>
                            <pre class="code ro-source">{highlighted(&src)}</pre>
                            {if has_editor {
                                view! {
                                    <p class="hint">"Edit it in the "
                                        <button type="button" class="link" on:click=open_editor>{editor_name}</button>"."
                                    </p>
                                }.into_any()
                            } else {
                                view! { <p class="hint">"Read-only for now: this element gets its own editor later."</p> }.into_any()
                            }}
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
    // What the paragraphs' ref tags resolve against: this file's entries, chapters and
    // figures, and the printed values the backend read.
    let refctx = Memo::new(move |_| {
        let d = tab.doc();
        let index = tab.refs.with(|r| r.as_ref().and_then(|r| r.as_ref().ok().cloned()));
        RefCtx::from_doc(&d.source, &d.nodes, index)
    });

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
                            <span class="hint">"Type text; refs and tags show as chips. Click a chip to edit it; ＋ or Ctrl+K inserts one; Ctrl+B and Ctrl+I bold and italicize. Ctrl+Enter applies."</span>
                            <span class="spacer"></span>
                            <button class="small" disabled=busy on:click=add_first>"+ ¶ at start"</button>
                            {add_menu(tab, at, "+ other at start…", || None)}
                        </div>
                        <For each=move || 0..blocks.with(|b| b.len()) key=|i| *i let:pos>
                            <BlockRow tab at pos blocks drag refctx/>
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
