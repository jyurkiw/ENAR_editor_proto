//! The File-structure panel: the active file's element tree.
//!
//! Clicking a node selects it (the main area shows its text). Clicking a node that's
//! already selected, or its arrow, opens or closes it.

use leptos::ev::MouseEvent;
use leptos::prelude::*;

use std::sync::Arc;

use crate::state::{ctx, DocData, DocTab};

#[component]
pub fn StructurePanel() -> impl IntoView {
    let app = ctx();
    move || match app.active_tab() {
        None => view! { <div class="empty"><p>"Open a supplement file to see its structure."</p></div> }.into_any(),
        Some(tab) => {
            let tab = StoredValue::new(tab);
            view! {
                <div class="tree-tools">
                    <button title="Close every node" on:click=move |_| tab.with_value(|t| t.expanded.set([0].into()))>
                        "Collapse all"
                    </button>
                    <button title="Open every node" on:click=move |_| tab.with_value(|t| {
                        let all = t.doc_untracked().nodes.iter().enumerate().filter(|(_, n)| !n.children.is_empty()).map(|(i, _)| i).collect();
                        t.expanded.set(all)
                    })>"Expand all"</button>
                </div>
                <div class="tree">{move || {
                    let data = tab.with_value(|t| t.doc());
                    struct_node(tab.get_value(), data, 0, 0)
                }}</div>
            }
            .into_any()
        }
    }
}

fn indent(depth: usize) -> String {
    format!("padding-left: {}px", 6 + depth * 14)
}

fn struct_node(tab: DocTab, data: Arc<DocData>, idx: usize, depth: usize) -> AnyView {
    let node = &data.nodes[idx];
    let has_children = !node.children.is_empty();
    let tag = node.tag.clone();
    let label = node.label.clone();
    let line = node.line;
    let expanded = tab.expanded;
    let selected = tab.selected;
    let is_open = Memo::new(move |_| expanded.with(|e| e.contains(&idx)));
    let toggle = move || {
        expanded.update(|e| {
            if !e.remove(&idx) {
                e.insert(idx);
            }
        })
    };
    let on_click = move |_: MouseEvent| {
        if selected.get_untracked() == idx {
            if has_children {
                toggle();
            }
        } else {
            selected.set(idx);
            if has_children && !is_open.get_untracked() {
                toggle();
            }
        }
    };
    let on_arrow = move |ev: MouseEvent| {
        ev.stop_propagation();
        toggle();
    };
    let arrow = move || match (has_children, is_open.get()) {
        (false, _) => "",
        (true, true) => "▾",
        (true, false) => "▸",
    };
    let tab = StoredValue::new(tab);
    let data = StoredValue::new(data);

    view! {
        <div class="node">
            <div
                class="row element"
                class:selected=move || selected.get() == idx
                style=indent(depth)
                title=format!("line {line}")
                on:click=on_click
            >
                <span class="icon" on:click=on_arrow>{arrow}</span>
                <span class="tag">{tag}</span>
                <span class="label">{label}</span>
            </div>
            <Show when=move || has_children && is_open.get()>
                {move || data.with_value(|d| {
                    d.nodes[idx].children.iter().map(|c| struct_node(tab.get_value(), d.clone(), *c, depth + 1)).collect_view()
                })}
            </Show>
        </div>
    }
    .into_any()
}
