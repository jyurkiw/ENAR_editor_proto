//! The `<table>` editor: an info header (id and name), the headers as a vertical list
//! (create, edit, delete, move; each is a column, so rows follow), and the rows as
//! read-only placeholders showing each `<tr>`'s XML. Header text and the info header
//! apply on their own (Apply, Ctrl+Enter, or leaving the box); add, delete and move
//! apply at once. Everything is validated before the file is written.

use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::table::{self, Part, TableAt, TableEdit, TableView};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::prose_editor::ChipEditor;
use crate::apply::apply;
use crate::dialogs::ask;
use crate::state::DocTab;

fn focus_stays(ev: &FocusEvent, el: &NodeRef<html::Div>) -> bool {
    ev.related_target()
        .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
        .zip(el.get())
        .is_some_and(|(n, e)| e.contains(Some(&n)))
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
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { crate::apply::if_still_shown(el, move || do_apply()) }
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
        ask(msg, move || apply(tab, Change::Table { edit: TableEdit::DeleteHeader { at: at.get_value(), col } }));
    };
    let swap = move |c: usize| apply(tab, Change::Table { edit: TableEdit::SwapHeaders { at: at.get_value(), col: c } });

    view! {
        <div node_ref=el class="th-row" class:dirty=dirty class:failed=failed
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { crate::apply::if_still_shown(el, move || do_apply()) }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <span class="th-num" title="Column">{col + 1}</span>
            <div class="th-text"><ChipEditor value=Signal::derive(text) set=Callback::new(move |s: String| set(s)) placeholder="(empty header)"
                single=true id=Signal::derive(move || format!("th-{col}"))/></div>
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

#[component]
fn CellField(tab: DocTab, at: StoredValue<TableAt>, view: Memo<Option<TableView>>, part: Part, row: usize, col: usize) -> impl IntoView {
    let cell = Memo::new(move |_| {
        view.with(|v| {
            let v = v.as_ref()?;
            let rows = if part == Part::Body { &v.body } else { &v.footer };
            rows.get(row)?.cells.get(col).cloned()
        })
    });
    let n_cells = Memo::new(move |_| {
        view.with(|v| v.as_ref().and_then(|v| (if part == Part::Body { &v.body } else { &v.footer }).get(row).map(|r| r.cells.len())).unwrap_or(0))
    });
    // Named by the column it starts in (cells before it may span several).
    let label = move || {
        view.with(|v| {
            let v = v.as_ref()?;
            let c = start_col(v, part, row, col);
            Some(match v.headers.get(c) {
                Some(h) if h.trim().is_empty() => format!("(column {})", c + 1),
                Some(h) => h.clone(),
                None => format!("(no header) {}", c + 1),
            })
        })
        .unwrap_or_else(|| format!("(no header) {}", col + 1))
    };
    let no_header = move || view.with(|v| v.as_ref().is_some_and(|v| v.has_header && start_col(v, part, row, col) >= v.headers.len()));
    // A cell that can go: one past the headers, or the row's last when its spans make the
    // row wider than the headers.
    let spare = move || {
        view.with(|v| {
            let Some(v) = v.as_ref().filter(|v| v.has_header) else { return false };
            let rows = if part == Part::Body { &v.body } else { &v.footer };
            let Some(r) = rows.get(row) else { return false };
            let width = row_width(v, part, row);
            r.cells.len() > 1 && col < r.cells.len() && (start_col(v, part, row, col) >= v.headers.len() || (col + 1 == r.cells.len() && width > v.headers.len()))
        })
    };
    let delete_cell = move |_: MouseEvent| {
        apply(tab, Change::Table { edit: TableEdit::DeleteCell { at: at.get_value(), part, row, col } });
    };
    let base = move || cell.get().map(|c| c.text).unwrap_or_default();
    let key = move || (at.get_value(), part, row, col);
    let draft = move || {
        let b = base();
        tab.cell_drafts.with(|d| d.get(&key()).filter(|(orig, _)| *orig == b).map(|(_, t)| t.clone()))
    };
    let text = move || draft().unwrap_or_else(base);
    let dirty = move || draft().is_some();
    let failed = move || tab.cell_failed.with(|f| f.contains(&key()));
    let busy = move || tab.applying.get();
    let el = NodeRef::<html::Div>::new();
    let set = move |v: String| {
        let b = base();
        let k = key();
        tab.cell_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    };
    let do_apply = move || {
        if let Some(t) = draft() {
            apply(tab, Change::Table { edit: TableEdit::SetCell { at: at.get_value(), part, row, col, text: t } });
        }
    };
    let revert = move || {
        let k = key();
        tab.cell_drafts.update(|d| {
            d.remove(&k);
        });
        tab.cell_failed.update(|f| {
            f.remove(&k);
        });
    };
    let spans = move || {
        cell.get().map(|c| {
            let mut s = Vec::new();
            if !c.colspan.is_empty() {
                s.push(format!("colspan {}", c.colspan));
            }
            if !c.rowspan.is_empty() {
                s.push(format!("rowspan {}", c.rowspan));
            }
            s.join(" · ")
        })
    };
    // The span boxes: open while editing them, applied together.
    let spans_open = RwSignal::new(false);
    let col_d = RwSignal::new(String::new());
    let row_d = RwSignal::new(String::new());
    let open_spans = move || {
        let c = cell.get_untracked().unwrap_or_default();
        col_d.set(c.colspan.clone());
        row_d.set(c.rowspan.clone());
        spans_open.set(true);
    };
    let apply_spans = move || {
        let norm = |v: String| if v.trim() == "1" { String::new() } else { v.trim().to_string() };
        apply(tab, Change::Table { edit: TableEdit::Spans { at: at.get_value(), part, row, col, colspan: norm(col_d.get_untracked()), rowspan: norm(row_d.get_untracked()) } });
        spans_open.set(false);
    };
    // Whether the row has this cell: a memo, so typing doesn't re-render the field.
    let present = Memo::new(move |_| cell.with(|c| c.is_some()));

    view! {
        <div node_ref=el class="cell" class:dirty=dirty class:failed=failed class:extra=no_header
            on:focusout=move |ev: FocusEvent| if present.get_untracked() && !focus_stays(&ev, &el) { crate::apply::if_still_shown(el, move || do_apply()) }
            on:keydown=move |ev: KeyboardEvent| {
                if !present.get_untracked() { return }
                if ev.key() == "Escape" { revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="cell-label">
                <span class="cell-name" title="This cell's column header">{label}</span>
                {move || spans().filter(|s| !s.is_empty()).map(|s| view! { <span class="span-badge">{s}</span> })}
                {move || (present.get() && !spans_open.get()).then(|| view! {
                    <button class="small" title="How many columns or rows this cell covers" disabled=busy on:click=move |_| open_spans()>"Spans"</button>
                })}
                <span class="spacer"></span>
                {move || (present.get() && spare()).then(|| view! {
                    <button class="icon-btn danger" title="Delete this cell" disabled=busy on:click=delete_cell>"✕"</button>
                })}
                {move || failed().then(|| view! { <span class="state-badge bad">"refused"</span> })}
                {move || (present.get() && dirty()).then(|| view! {
                    <button class="small" disabled=busy on:click=move |_| revert()>"Revert"</button>
                    <button class="small primary" disabled=busy on:click=move |_| do_apply()>"Apply"</button>
                })}
            </div>
            {move || spans_open.get().then(|| view! {
                <div class="span-fields"
                    on:keydown=move |ev: KeyboardEvent| { ev.stop_propagation(); if ev.key() == "Enter" { ev.prevent_default(); apply_spans() } else if ev.key() == "Escape" { spans_open.set(false) } }>
                    <label class="span-field">"Cols"<input type="text" class="num" placeholder="1" prop:value=move || col_d.get() on:input=move |ev| col_d.set(event_target_value(&ev))/></label>
                    <label class="span-field">"Rows"<input type="text" class="num" placeholder="1" prop:value=move || row_d.get() on:input=move |ev| row_d.set(event_target_value(&ev))/></label>
                    <button class="small primary" disabled=busy on:click=move |_| apply_spans()>"Apply spans"</button>
                    <button class="small" on:click=move |_| spans_open.set(false)>"Cancel"</button>
                </div>
            })}
            {move || if present.get() {
                view! {
                    <ChipEditor value=Signal::derive(text) set=Callback::new(move |s: String| set(s)) placeholder="(empty)" single=true/>
                }.into_any()
            } else {
                // The row is short of the headers: only the first missing slot offers a cell.
                let first_missing = col == n_cells.get_untracked();
                view! {
                    <div class="cell-missing">
                        "No cell"
                        {first_missing.then(|| view! {
                            <button class="small" disabled=busy on:click=move |_| {
                                apply(tab, Change::Table { edit: TableEdit::AddCell { at: at.get_value(), part, row } })
                            }>"+ cell"</button>
                        })}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

fn cell_count(v: &TableView, part: Part, row: usize) -> usize {
    (if part == Part::Body { &v.body } else { &v.footer }).get(row).map(|r| r.cells.len()).unwrap_or(0)
}

/// How many columns the row covers, spans counted.
fn row_width(v: &TableView, part: Part, row: usize) -> usize {
    table::row_widths(if part == Part::Body { &v.body } else { &v.footer }).get(row).copied().unwrap_or(0)
}

/// The column cell `col` of the row starts in: the cells before it, with their colspans.
fn start_col(v: &TableView, part: Part, row: usize, col: usize) -> usize {
    let n = |s: &str| s.trim().parse::<usize>().unwrap_or(1).max(1);
    (if part == Part::Body { &v.body } else { &v.footer }).get(row).map(|r| r.cells.iter().take(col).map(|c| n(&c.colspan)).sum()).unwrap_or(col)
}

#[component]
fn RowCard(tab: DocTab, at: StoredValue<TableAt>, view: Memo<Option<TableView>>, part: Part, row: usize, count: Memo<usize>) -> impl IntoView {
    let busy = move || tab.applying.get();
    // A field per cell, and an empty slot per column the row is short of.
    let cols = Memo::new(move |_| {
        view.with(|v| {
            v.as_ref().map(|v| {
                let n = cell_count(v, part, row);
                if v.has_header { n + v.headers.len().saturating_sub(row_width(v, part, row)) } else { n.max(v.headers.len()) }
            }).unwrap_or(0)
        })
    });
    let mismatch = move || view.with(|v| v.as_ref().is_some_and(|v| v.has_header && row_width(v, part, row) != v.headers.len()));
    let label = if part == Part::Body { format!("Row {}", row + 1) } else { format!("Footer row {}", row + 1) };
    let only_body = move || part == Part::Body && count.get() == 1;
    let delete = move |_: MouseEvent| {
        if part == Part::Body && count.get_untracked() == 1 {
            return;
        }
        let msg = if part == Part::Footer && count.get_untracked() == 1 {
            "Delete the only footer row? The <footer> goes with it.".to_string()
        } else if part == Part::Body {
            format!("Delete row {}?", row + 1)
        } else {
            format!("Delete footer row {}?", row + 1)
        };
        ask(msg, move || apply(tab, Change::Table { edit: TableEdit::DeleteRow { at: at.get_value(), part, row } }));
    };
    let swap = move |r: usize| apply(tab, Change::Table { edit: TableEdit::SwapRows { at: at.get_value(), part, row: r } });
    let add_below = move |_: MouseEvent| apply(tab, Change::Table { edit: TableEdit::AddRow { at: at.get_value(), part, after: Some(row) } });
    view! {
        <div class="row-card" class:mismatch=mismatch>
            <div class="row-head">
                <span class="row-name">{label}</span>
                {move || mismatch().then(|| view! { <span class="hint bad">"cell count doesn't match the headers"</span> })}
                <span class="spacer"></span>
                <button class="icon-btn" title="Move up" disabled=move || { row == 0 || busy() } on:click=move |_| swap(row - 1)>"↑"</button>
                <button class="icon-btn" title="Move down" disabled=move || { row + 1 >= count.get() || busy() } on:click=move |_| swap(row)>"↓"</button>
                <button class="small" title="Insert an empty row below" disabled=busy on:click=add_below>"+ row below"</button>
                <button class="icon-btn danger" disabled=move || { busy() || only_body() }
                    title=move || if only_body() { "A table needs at least one body row" } else { "Delete row" }
                    on:click=delete>"✕"</button>
            </div>
            <div class="cells">
                <For each=move || 0..cols.get() key=|c| *c let:col>
                    <CellField tab at view part row col/>
                </For>
            </div>
        </div>
    }
}

#[component]
fn RowsPart(tab: DocTab, at: StoredValue<TableAt>, view: Memo<Option<TableView>>, part: Part) -> impl IntoView {
    let count = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| if part == Part::Body { v.body.len() } else { v.footer.len() }).unwrap_or(0)));
    let busy = move || tab.applying.get();
    let add_first = move |_: MouseEvent| apply(tab, Change::Table { edit: TableEdit::AddRow { at: at.get_value(), part, after: None } });
    view! {
        <div class="th-tools">
            <button class="small" disabled=busy on:click=add_first>
                {if part == Part::Body { "+ Row at the start" } else { "+ Footer row at the start" }}
            </button>
        </div>
        <For each=move || 0..count.get() key=|r| *r let:row>
            <RowCard tab at view part row count/>
        </For>
    }
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
    let has_footer = Memo::new(move |_| view.with(|v| v.as_ref().is_some_and(|v| v.has_footer)));
    let n_body = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.body.len()).unwrap_or(0)));
    let n_footer = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.footer.len()).unwrap_or(0)));
    let rows = move || {
        view! {
            <div class="table-card-title">
                {move || format!("Rows ({})", n_body.get())}
                <span class="hint">"One field per cell, labelled with its column's header. Cells follow their header when headers move."</span>
            </div>
            <RowsPart tab at view part=Part::Body/>
            <Show when=move || has_footer.get() fallback=move || view! {
                <div class="th-tools">
                    <button class="small" disabled=busy on:click=move |_: MouseEvent| apply(tab, Change::Table { edit: TableEdit::AddRow { at: at.get_value(), part: Part::Footer, after: None } })>
                        "+ Add a footer row"
                    </button>
                    <span class="hint">"Creates the <footer>: rows printed under the body, for totals or notes."</span>
                </div>
            }>
                <div class="table-card-subtitle">{move || format!("Footer rows ({})", n_footer.get())}</div>
                <RowsPart tab at view part=Part::Footer/>
            </Show>
        }
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
