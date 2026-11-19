//! The monster file editor, first pass: the tree on the left navigates the file (the
//! family, lairs, stat blocks and their entries); the main area edits the selected
//! element as raw XML. Apply validates the whole file against the monster schema and
//! writes it only if it validates. The file and each stat block also get a panel to
//! add, reorder and delete stat blocks.

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::prelude::*;
use shared::display::reindent;
use shared::element::{self, ElementEdit};
use shared::family::{self, HEADER, PARTS};
use shared::monster::{self, MonsterEdit};
use shared::Change;

use crate::apply::apply;
use crate::family_form::{FamilyPage, HeaderPage, PartPage};
use crate::statblock_form::StatPage;
use crate::dialogs::ask;
use crate::state::DocTab;


/// A slug from a name: "Giant Rat" -> "giant-rat".
fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

/// The stat blocks in the file: open, reorder, delete, add.
#[component]
fn StatBlocks(tab: DocTab, path: StoredValue<Vec<usize>>) -> impl IntoView {
    let blocks = Memo::new(move |_| {
        let d = tab.doc();
        monster::stat_blocks(&d.source, &d.nodes)
    });
    let current = Memo::new(move |_| {
        let d = tab.doc();
        let idx = element::at_path(&d.nodes, &path.get_value())?;
        d.nodes.get(idx).filter(|n| n.tag == "monster").and_then(|n| shared::edit::start_tag(&d.source, n.start)?.value("id"))
    });
    let busy = move || tab.applying.get();
    let name = RwSignal::new(String::new());
    let id = RwSignal::new(String::new());
    let id_edited = RwSignal::new(false);
    let add = move || {
        let (n, i) = (name.get_untracked(), id.get_untracked());
        if i.trim().is_empty() {
            return;
        }
        apply(tab, Change::Monster { edit: MonsterEdit::Add { after: current.get_untracked(), id: i, name: n } });
        name.set(String::new());
        id.set(String::new());
        id_edited.set(false);
    };
    let open = move |i: String| {
        let d = tab.doc_untracked();
        if let Some(n) = monster::find_monster(&d.source, &d.nodes, &i) {
            tab.reveal(n);
        }
    };
    let taken = move || {
        let i = id.get();
        blocks.with(|b| b.iter().any(|(x, _)| *x == i.trim()))
    };
    view! {
        <div class="table-card">
            <div class="table-card-title">
                {move || format!("Stat blocks ({})", blocks.with(|b| b.len()))}
                <span class="hint">"Each <monster> in this file. Click one to edit it."</span>
            </div>
            <div class="stat-list">
                {move || {
                    let list = blocks.get();
                    let count = list.len();
                    list.into_iter().enumerate().map(|(pos, (bid, bname))| {
                        let here = current.get().as_deref() == Some(bid.as_str());
                        let (b1, b2, b3, b4) = (bid.clone(), bid.clone(), bid.clone(), bid.clone());
                        let prev = (pos > 0).then(|| blocks.with_untracked(|b| b[pos - 1].0.clone()));
                        let after_next = blocks.with_untracked(|b| b.get(pos + 2).map(|x| x.0.clone()));
                        view! {
                            <div class="stat-row" class:current=here>
                                <button type="button" class="link stat-name" on:click=move |_| open(b1.clone())>
                                    {if bname.is_empty() { "(no name)".to_string() } else { bname.clone() }}
                                </button>
                                <code class="stat-id">{bid.clone()}</code>
                                <span class="spacer"></span>
                                <button class="icon-btn" title="Move up" disabled=move || { pos == 0 || busy() }
                                    on:click=move |_| apply(tab, Change::Monster { edit: MonsterEdit::Move { id: b2.clone(), before: prev.clone() } })>"↑"</button>
                                <button class="icon-btn" title="Move down" disabled=move || { pos + 1 >= count || busy() }
                                    on:click=move |_| apply(tab, Change::Monster { edit: MonsterEdit::Move { id: b3.clone(), before: after_next.clone() } })>"↓"</button>
                                <button class="icon-btn danger" disabled=move || { count <= 1 || busy() }
                                    title=if count <= 1 { "A monster file needs at least one stat block" } else { "Delete this stat block" }
                                    on:click=move |_: MouseEvent| {
                                        let id = b4.clone();
                                        ask(format!("Delete the stat block \"{}\"?", b4), move || {
                                            apply(tab, Change::Monster { edit: MonsterEdit::Delete { id } })
                                        });
                                    }>"✕"</button>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
            <form class="field-row stat-add" on:submit=move |ev| {
                ev.prevent_default();
                add();
            }>
                <label class="field">
                    <span class="field-label">"New stat block name"</span>
                    <input type="text" placeholder="Gnoll Pack Lord" prop:value=move || name.get() on:input=move |ev| {
                        let v = event_target_value(&ev);
                        if !id_edited.get_untracked() {
                            id.set(slug(&v));
                        }
                        name.set(v);
                    }/>
                </label>
                <label class="field">
                    <span class="field-label">"Id"</span>
                    <input type="text" placeholder="gnoll-pack-lord" class:invalid=taken prop:value=move || id.get() on:input=move |ev| {
                        id_edited.set(true);
                        id.set(event_target_value(&ev));
                    }/>
                    {move || if taken() {
                        view! { <span class="hint bad">"Already used in this file"</span> }.into_any()
                    } else {
                        view! { <span class="hint">"Unique in the file; <reference><monster monster=…> names it"</span> }.into_any()
                    }}
                </label>
                <div class="stat-add-button">
                    <button type="submit" class="primary" disabled=move || { busy() || id.get().trim().is_empty() || taken() }>
                        {move || if current.get().is_some() { "+ Add after this one" } else { "+ Add at the end" }}
                    </button>
                </div>
            </form>
            <p class="hint">"A new stat block starts as a Medium Humanoid with every stat 10 and CR 0, ready to edit."</p>
        </div>
    }
}

/// The selected element as raw XML, with Apply.
#[component]
fn RawEditor(tab: DocTab, path: StoredValue<Vec<usize>>, tag: StoredValue<String>) -> impl IntoView {
    let base = Memo::new(move |_| {
        let d = tab.doc();
        element::at_path(&d.nodes, &path.get_value()).map(|i| reindent(&d.source, d.nodes[i].start, d.nodes[i].end)).unwrap_or_default()
    });
    let draft = move || {
        let b = base.get();
        tab.raw_drafts.with(|d| d.get(&path.get_value()).filter(|(o, _)| *o == b).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(|| base.get());
    let dirty = move || draft().is_some();
    let failed = move || tab.raw_failed.with(|f| f.contains(&path.get_value()));
    let busy = move || tab.applying.get();
    let set = move |v: String| {
        let b = base.get_untracked();
        let k = path.get_value();
        tab.raw_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            apply(tab, Change::Element { edit: ElementEdit { path: path.get_value(), tag: tag.get_value(), xml: t } });
        }
    };
    let revert = move || {
        let k = path.get_value();
        tab.raw_drafts.update(|d| {
            d.remove(&k);
        });
        tab.raw_failed.update(|f| {
            f.remove(&k);
        });
    };
    let rows = move || text().lines().count().clamp(4, 40);
    view! {
        <div class="table-card raw-card" class:dirty=dirty class:failed=failed>
            <div class="table-card-title">
                {move || format!("<{}>", tag.get_value())}
                <span class="hint">"Placeholder editor: the element's XML. Apply checks the whole file against the monster schema and writes it only if it's valid."</span>
            </div>
            <textarea class="raw-text" spellcheck="false" rows=rows
                prop:value=text on:input=move |ev| set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| {
                    if ev.key() == "Escape" {
                        revert()
                    } else if ev.key() == "Enter" && ev.ctrl_key() {
                        ev.prevent_default();
                        do_apply()
                    } else if ev.key() == "Tab" && !ev.shift_key() {
                        // Tab indents (four spaces) instead of leaving the box.
                        ev.prevent_default();
                        if let Some(t) = ev.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlTextAreaElement>(t).ok()) {
                            let v = t.value();
                            let a = t.selection_start().ok().flatten().unwrap_or(0);
                            let b = t.selection_end().ok().flatten().unwrap_or(a);
                            let units: Vec<u16> = v.encode_utf16().collect();
                            let (a, b) = (a as usize, b as usize);
                            let new = format!("{}    {}", String::from_utf16_lossy(&units[..a]), String::from_utf16_lossy(&units[b..]));
                            t.set_value(&new);
                            let _ = t.set_selection_range(a as u32 + 4, a as u32 + 4);
                            set(new);
                        }
                    }
                }></textarea>
            <div class="p-bar raw-bar">
                {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                {move || (dirty() && !failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                <span class="hint">"Ctrl+Enter applies · Esc reverts · Tab indents"</span>
                <span class="spacer"></span>
                <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
            </div>
        </div>
    }
}

/// The main area for a monster file, at the element with this tree path and tag: the
/// family's page (header, parts, stat blocks), the header, a part's form, or the raw
/// editor (stat blocks, for now). Pages with a form keep the element's raw XML below.
#[component]
pub fn MonsterForm(tab: DocTab, path: Vec<usize>, tag: String) -> impl IntoView {
    // The family's prose (a stat block's page gives its own, with its refs).
    crate::prose_editor::provide_env(tab, shared::prose::Palette::StatBlock, None);
    let fam = Memo::new(move |_| family::read(&tab.doc().source));
    // The part (a root child) this element is in, if any.
    let top = path.first().copied().and_then(|c| {
        let d = tab.doc_untracked();
        let t = d.nodes.first()?.children.get(c).map(|i| d.nodes[*i].tag.clone())?;
        PARTS.contains(&t.as_str()).then_some((c, t))
    });
    let stat = (path.len() == 1 && tag == "monster").then(|| path[0]);
    let page = if let Some(c) = stat {
        Page::Stat(c)
    } else if path.is_empty() {
        Page::Family
    } else if path.len() == 1 && HEADER.contains(&tag.as_str()) {
        Page::Header
    } else if let Some((c, t)) = top {
        Page::Part(c, t)
    } else {
        Page::Raw
    };
    let stat_panel = path.is_empty() || tag == "monster";
    let raw_dirty = tab.raw_drafts.with_untracked(|d| d.contains_key(&path));
    let raw_only = matches!(page, Page::Raw);
    let path = StoredValue::new(path);
    let tag = StoredValue::new(tag);
    let page_view = match page {
        Page::Family => view! { <FamilyPage tab fam/> }.into_any(),
        Page::Header => view! { <HeaderPage tab fam/> }.into_any(),
        Page::Part(child, t) => view! { <PartPage tab fam child tag=t/> }.into_any(),
        Page::Stat(child) => view! { <StatPage tab child/> }.into_any(),
        Page::Raw if tag.get_value() == "monster" => view! {
            <div class="notice info">"A template stat block (template=\"true\"): edit it as XML."</div>
        }.into_any(),
        Page::Raw => ().into_any(),
    };
    view! {
        <div class="table-editor monster-editor">
            {page_view}
            {stat_panel.then(|| view! { <StatBlocks tab path/> })}
            {if raw_only {
                view! { <RawEditor tab path tag/> }.into_any()
            } else {
                view! {
                    <details class="raw-details" open=raw_dirty>
                        <summary>{move || format!("Raw XML of <{}>", tag.get_value())}</summary>
                        <RawEditor tab path tag/>
                    </details>
                }.into_any()
            }}
        </div>
    }
}

enum Page {
    Stat(usize),
    Family,
    Header,
    Part(usize, String),
    Raw,
}
