//! The `<table>` editor: an info header (id and name), the headers as a vertical list
//! (create, edit, delete, move; each is a column, so rows follow), and the rows as
//! read-only placeholders showing each `<tr>`'s XML. Header text and the info header
//! apply on their own (Apply, Ctrl+Enter, or leaving the box); add, delete and move
//! apply at once. Everything is validated before the file is written.

use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::table::{self, TableAt, TableEdit, TableView};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
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

#[component]
fn InfoCard(tab: DocTab, at: StoredValue<TableAt>, view: Memo<Option<TableView>>, links: Memo<usize>) -> impl IntoView {
    let base = move || view.get().map(|v| (v.id, v.title)).unwrap_or_default();
    let model = move || tab.table_drafts.with(|d| d.get(&at.get_value()).cloned()).unwrap_or_else(base);
    let update = move |f: &dyn Fn(&mut (String, String))| {
        let b = view.get_untracked().map(|v| (v.id, v.title)).unwrap_or_default();
        let a = at.get_value();
        let mut m = tab.table_drafts.with_untracked(|d| d.get(&a).cloned()).unwrap_or_else(|| b.clone());
        f(&mut m);
        tab.table_drafts.update(|d| {
            if m == b {
                d.remove(&a);
            } else {
                d.insert(a, m);
            }
        });
    };
    let dirty = move || tab.table_drafts.with(|d| d.contains_key(&at.get_value()));
    let busy = move || tab.applying.get();
    let do_apply = move || {
        let (id, title) = model();
        if dirty() && !id.trim().is_empty() {
            apply(tab, Change::Table { edit: TableEdit::Info { at: at.get_value(), id, title } });
        }
    };
    let revert = move || {
        let a = at.get_value();
        tab.table_drafts.update(|d| {
            d.remove(&a);
        });
    };
    let el = NodeRef::<html::Div>::new();
    view! {
        <div node_ref=el class="table-card" class:dirty=dirty
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { do_apply() }
            }>
            <div class="table-card-title">"Table"</div>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Id *"</span>
                    <input type="text" class:invalid=move || model().0.trim().is_empty() prop:value=move || model().0
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            update(&|m| m.0 = v.clone());
                        }/>
                    <span class="hint">{move || {
                        let n = links.get();
                        if n == 0 { "@id. No <tableLink> places it.".to_string() } else { format!("@id, placed by {n} <tableLink>") }
                    }}</span>
                </label>
                <label class="field wide-field">
                    <span class="field-label">"Name"</span>
                    <input type="text" placeholder="(no title)" prop:value=move || model().1
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            update(&|m| m.1 = v.clone());
                        }/>
                    <span class="hint">"<title>: the caption above the table. Empty removes it."</span>
                </label>
            </div>
            <Show when=move || { view.get().is_some_and(|v| v.id != model().0) && links.get() > 0 }>
                <p class="hint warn-text">{move || format!("Apply also renames the {} <tableLink>(s) that place it.", links.get())}</p>
            </Show>
            {move || {
                let other = view.get().map(|v| v.other_attrs).unwrap_or_default();
                (!other.is_empty()).then(|| {
                    let s = other.iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect::<Vec<_>>().join(" ");
                    view! { <p class="hint">"Also kept as written: " <code>{s}</code></p> }
                })
            }}
            <div class="ref-actions">
                <span class="hint">"Applies when you leave these fields · Ctrl+Enter applies · Esc reverts"</span>
                <span class="spacer"></span>
                <button on:click=move |_| revert() disabled=move || !dirty() || busy()>"Revert"</button>
                <button class="primary" on:click=move |_| do_apply() disabled=move || !dirty() || busy()>"Apply"</button>
            </div>
        </div>
    }
}

#[component]
fn HeaderRow(tab: DocTab, at: StoredValue<TableAt>, col: usize, view: Memo<Option<TableView>>, cells: Memo<Vec<usize>>) -> impl IntoView {
    let base = move || view.with(|v| v.as_ref().and_then(|v| v.headers.get(col).cloned())).unwrap_or_default();
    let count = move || view.with(|v| v.as_ref().map(|v| v.headers.len()).unwrap_or(0));
    let key = move || (at.get_value(), col);
    let draft = move || {
        let b = base();
        tab.th_drafts.with(|d| d.get(&key()).filter(|(orig, _)| *orig == b).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(base);
    let dirty = move || draft().is_some();
    let failed = move || tab.th_failed.with(|f| f.contains(&key()));
    let busy = move || tab.applying.get();
    let el = NodeRef::<html::Div>::new();
    let set = move |v: String| {
        let b = base();
        let k = key();
        tab.th_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            apply(tab, Change::Table { edit: TableEdit::SetHeader { at: at.get_value(), col, text: t } });
        }
    };
    let revert = move || {
        let k = key();
        tab.th_drafts.update(|d| {
            d.remove(&k);
        });
        tab.th_failed.update(|f| {
            f.remove(&k);
        });
    };
    let insert = move |pos: usize| apply(tab, Change::Table { edit: TableEdit::AddHeader { at: at.get_value(), col: pos, text: String::new() } });
    let delete = move |_: MouseEvent| {
        let name = base();
        let label = if name.trim().is_empty() { format!("column {}", col + 1) } else { format!("\"{name}\"") };
        let msg = if count() == 1 {
            format!("Delete the header {label}? It's the only one, so the <header> goes and the rows keep their cells.")
        } else {
            let n = cells.with(|c| c.get(col).copied().unwrap_or(0));
            format!("Delete the column {label}? Its header and {n} row cell(s) are removed.")
        };
        if confirm(&msg) {
            apply(tab, Change::Table { edit: TableEdit::DeleteHeader { at: at.get_value(), col } });
        }
    };
    let swap = move |c: usize| apply(tab, Change::Table { edit: TableEdit::SwapHeaders { at: at.get_value(), col: c } });

    view! {
        <div node_ref=el class="th-row" class:dirty=dirty class:failed=failed
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <span class="th-num" title="Column">{col + 1}</span>
            <textarea class="th-text" id=format!("th-{col}") rows=1 placeholder="(empty header)"
                prop:value=text on:input=move |ev| set(event_target_value(&ev))></textarea>
            <span class="th-cells" title="Row cells in this column">{move || {
                let n = cells.with(|c| c.get(col).copied().unwrap_or(0));
                format!("{n} cell(s)")
            }}</span>
            <div class="th-buttons">
                {move || failed().then(|| view! { <span class="state-badge bad">"refused"</span> })}
                <button class="small" disabled=move || !dirty() || busy() on:click=move |_| revert()>"Revert"</button>
                <button class="small primary" disabled=move || !dirty() || busy() on:click=move |_| do_apply()>"Apply"</button>
                <button class="icon-btn" title="Move up (with its column)" disabled=move || col == 0 || busy() on:click=move |_| swap(col - 1)>"↑"</button>
                <button class="icon-btn" title="Move down (with its column)" disabled=move || { col + 1 >= count() || busy() } on:click=move |_| swap(col)>"↓"</button>
                <button class="icon-btn" title="Insert a header below (adds a column)" disabled=busy on:click=move |_| insert(col + 1)>"+"</button>
                <button class="icon-btn danger" title="Delete (with its column)" disabled=busy on:click=delete>"✕"</button>
            </div>
        </div>
    }
}

fn rows_view(rows: Vec<(String, usize)>, headers: Option<usize>, label: &'static str) -> impl IntoView {
    rows.into_iter()
        .enumerate()
        .map(|(i, (xml, n))| {
            let mismatch = headers.is_some_and(|h| h != n);
            let note = match headers {
                Some(h) if h != n => format!("{n} cell(s), {h} header(s)"),
                _ => format!("{n} cell(s)"),
            };
            let lines = xml.lines().count().max(1);
            view! {
                <div class="tr-row" class:mismatch=mismatch>
                    <span class="th-num">{format!("{label}{}", i + 1)}</span>
                    <textarea class="tr-text" readonly rows=lines prop:value=xml></textarea>
                    <span class="th-cells" class:bad=mismatch>{note}</span>
                </div>
            }
        })
        .collect_view()
}

#[component]
pub fn TableForm(tab: DocTab, at: TableAt) -> impl IntoView {
    let at = StoredValue::new(at);
    let view = Memo::new(move |_| {
        let d = tab.doc();
        table::find(&d.source, &d.nodes, &at.get_value()).map(|ti| table::read(&d.source, &d.nodes, ti))
    });
    let links = Memo::new(move |_| {
        let d = tab.doc();
        table::links(&d.source, &d.nodes, &at.get_value().id)
    });
    // Row cells in each header's column.
    let cells = Memo::new(move |_| {
        let d = tab.doc();
        let Some(ti) = table::find(&d.source, &d.nodes, &at.get_value()) else { return Vec::new() };
        let n = view.with(|v| v.as_ref().map(|v| v.headers.len()).unwrap_or(0));
        (0..n).map(|c| table::cells_in_column(&d.nodes, ti, c)).collect::<Vec<_>>()
    });
    let busy = move || tab.applying.get();

    // Focus a header just added.
    Effect::new(move |_| {
        let Some(col) = tab.th_focus.get() else { return };
        view.track();
        request_animation_frame(move || {
            if let Some(e) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(&format!("th-{col}"))) {
                if let Ok(h) = e.dyn_into::<web_sys::HtmlElement>() {
                    let _ = h.focus();
                }
            }
        });
        tab.th_focus.set(None);
    });

    let add_last = move |_: MouseEvent| {
        let n = view.get_untracked().map(|v| v.headers.len()).unwrap_or(0);
        apply(tab, Change::Table { edit: TableEdit::AddHeader { at: at.get_value(), col: n, text: String::new() } });
    };
    let add_first = move |_: MouseEvent| {
        apply(tab, Change::Table { edit: TableEdit::AddHeader { at: at.get_value(), col: 0, text: String::new() } });
    };

    // Only presence and "has a header" re-render the frame, so focus survives applies.
    let exists = Memo::new(move |_| view.with(|v| v.is_some()));
    let has_header = Memo::new(move |_| view.with(|v| v.as_ref().is_some_and(|v| v.has_header)));
    let rows = move || {
        view.get().map(|v| {
            let headers = v.has_header.then_some(v.headers.len());
            let n_body = v.body.len();
            let footer = v.footer.clone();
            view! {
                <div class="table-card-title">
                    {format!("Rows ({n_body})")}
                    <span class="hint">"Read-only for now: each <tr>'s cells as XML."</span>
                </div>
                {rows_view(v.body, headers, "")}
                {v.has_footer.then(|| view! {
                    <div class="table-card-subtitle">{format!("Footer rows ({})", footer.len())}</div>
                    {rows_view(footer, headers, "F")}
                })}
            }
        })
    };

    view! {
        {move || if !exists.get() {
            view! { <div class="empty"><p>"This table isn't in the file any more."</p></div> }.into_any()
        } else {
            view! {
                <div class="table-editor">
                    <InfoCard tab at view links/>

                    <div class="table-card">
                        <div class="table-card-title">
                            "Headers"
                            <span class="hint">"Each header is a column: adding, deleting or moving one does the same to that cell in every row."</span>
                        </div>
                        {move || if has_header.get() {
                            view! {
                                <div class="th-tools">
                                    <button class="small" disabled=busy on:click=add_first>"+ Header at the start"</button>
                                </div>
                                <For each=move || 0..view.with(|v| v.as_ref().map(|v| v.headers.len()).unwrap_or(0)) key=|c| *c let:col>
                                    <HeaderRow tab at col view cells/>
                                </For>
                                <div class="th-tools">
                                    <button class="small" disabled=busy on:click=add_last>"+ Header at the end"</button>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <p class="hint">"This table has no <header>. Adding one creates it; the rows aren't changed."</p>
                                <div class="th-tools"><button class="small" disabled=busy on:click=add_last>"+ Add a header"</button></div>
                            }.into_any()
                        }}
                    </div>

                    <div class="table-card">{rows}</div>
                </div>
            }.into_any()
        }}
    }
}
