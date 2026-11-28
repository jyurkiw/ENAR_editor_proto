//! The File-structure panel: the active file's element tree, down to the elements that
//! have a form of their own. A supplement shows its top-level parts and its chapters'
//! sections, sidebars and tables; a monster file, its stat blocks and family parts; an
//! item, spell or license file, only itself. What those forms edit (paragraphs, lists,
//! cells, a stat block's fields) isn't listed.
//!
//! Clicking a node selects it (the main area shows its form). Clicking a node that's
//! already selected, or its arrow, opens or closes it. When the selection is inside a
//! node (a list opened from its section), that node is marked as holding it.

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
                        let d = t.doc_untracked();
                        let all = (0..d.nodes.len()).filter(|i| !shown_children(&d, *i).is_empty()).collect();
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

/// The children the tree lists under node `idx`.
pub fn shown_children(d: &DocData, idx: usize) -> Vec<usize> {
    let Some(root) = d.nodes.first() else { return Vec::new() };
    let node = &d.nodes[idx];
    let kids = |keep: &dyn Fn(&str) -> bool| node.children.iter().copied().filter(|c| keep(&d.nodes[*c].tag)).collect();
    if idx == 0 {
        return match root.tag.as_str() {
            "supplement" => kids(&|_| true),
            // The family's header is edited in the family form.
            "monsters" => kids(&|t| !shared::family::HEADER.contains(&t)),
            _ => Vec::new(),
        };
    }
    let top = root.children.contains(&idx);
    if root.tag == "supplement" && top && node.tag == "chapter" {
        // The chapter's title is in the chapter form.
        return kids(&|t| t != "chapterTitle");
    }
    Vec::new()
}

/// The listed node that is, or holds, node `idx`.
fn shown_owner(d: &DocData, idx: usize) -> usize {
    let path = d.path_to(idx.min(d.nodes.len().saturating_sub(1)));
    let mut owner = 0;
    for w in path.windows(2) {
        if !shown_children(d, w[0]).contains(&w[1]) {
            break;
        }
        owner = w[1];
    }
    owner
}

fn indent(depth: usize) -> String {
    format!("padding-left: {}px", 6 + depth * 14)
}

fn struct_node(tab: DocTab, data: Arc<DocData>, idx: usize, depth: usize) -> AnyView {
    let node = &data.nodes[idx];
    let shown = shown_children(&data, idx);
    let has_children = !shown.is_empty();
    let tag = node.tag.clone();
    let label = node.label.clone();
    let (data_tag, data_label) = (tag.clone(), label.clone());
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
    let holds = {
        let data = data.clone();
        move || {
            let sel = selected.get();
            sel != idx && shown_owner(&data, sel) == idx
        }
    };
    let tab = StoredValue::new(tab);
    let data = StoredValue::new(data);
    let shown = StoredValue::new(shown);

    view! {
        <div class="node">
            <div
                class="row element"
                data-tag=data_tag
                data-label=data_label
                class:selected=move || selected.get() == idx
                class:holds-selected=holds
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
                    shown.with_value(|kids| kids.iter().map(|c| struct_node(tab.get_value(), d.clone(), *c, depth + 1)).collect_view())
                })}
            </Show>
        </div>
    }
    .into_any()
}
