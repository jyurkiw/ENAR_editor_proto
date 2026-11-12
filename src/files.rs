//! The File-system panel: a lazily loaded folder tree with a right-click menu.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::FsEntry;

use crate::api;
use crate::state::{ctx, Dialog, Menu};

#[component]
pub fn FilesPanel() -> impl IntoView {
    let app = ctx();
    move || match app.project.get() {
        None => view! {
            <div class="empty">
                <p>"No project is open."</p>
                <button on:click=move |_| app.pick_and_open_project()>"Open project…"</button>
            </div>
        }
        .into_any(),
        Some(p) => {
            let root = FsEntry { name: p.name.clone(), path: p.root.clone(), is_dir: true };
            view! { <div class="tree">{fs_node(root, 0)}</div> }.into_any()
        }
    }
}

fn indent(depth: usize) -> String {
    format!("padding-left: {}px", 6 + depth * 14)
}

fn fs_node(entry: FsEntry, depth: usize) -> AnyView {
    let app = ctx();
    let entry = StoredValue::new(entry);
    let is_dir = entry.with_value(|e| e.is_dir);
    let is_xml = entry.with_value(|e| e.is_xml());
    let name = entry.with_value(|e| e.name.clone());
    let path = entry.with_value(|e| e.path.clone());
    let expanded = {
        let path = path.clone();
        Memo::new(move |_| app.open_dirs.with(|d| d.contains(&path)))
    };
    let children: RwSignal<Option<Result<Vec<FsEntry>, String>>> = RwSignal::new(None);

    // Load (or reload) the folder when it's open and whenever the file tree is refreshed.
    if is_dir {
        let path = path.clone();
        Effect::new(move |_| {
            app.fs_version.track();
            if expanded.get() {
                let path = path.clone();
                spawn_local(async move {
                    let r = api::list_dir(&path).await.map_err(|e| e.message);
                    children.set(Some(r));
                });
            }
        });
    }

    let on_click = move |_: MouseEvent| {
        if is_dir {
            let path = entry.with_value(|e| e.path.clone());
            app.open_dirs.update(|d| {
                if !d.remove(&path) {
                    d.insert(path);
                }
            });
        }
    };
    let on_dblclick = move |_: MouseEvent| {
        if is_xml {
            app.open_file(entry.with_value(|e| e.path.clone()));
        }
    };
    let on_context = move |ev: MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        app.menu.set(Some(Menu { x: ev.client_x(), y: ev.client_y(), entry: entry.get_value() }));
    };

    let icon = move || match (is_dir, expanded.get(), is_xml) {
        (true, true, _) => "▾",
        (true, false, _) => "▸",
        (false, _, true) => "◆",
        _ => "·",
    };
    let class = if is_dir {
        "row dir"
    } else if is_xml {
        "row file xml"
    } else {
        "row file other"
    };

    view! {
        <div class="node">
            <div class=class style=indent(depth) title=path on:click=on_click on:dblclick=on_dblclick on:contextmenu=on_context>
                <span class="icon">{icon}</span>
                <span class="name">{name}</span>
            </div>
            <Show when=move || is_dir && expanded.get()>
                {move || match children.get() {
                    None => view! { <div class="row note" style=indent(depth + 1)>"Loading…"</div> }.into_any(),
                    Some(Err(e)) => view! { <div class="row note error" style=indent(depth + 1)>{e}</div> }.into_any(),
                    Some(Ok(list)) if list.is_empty() => {
                        view! { <div class="row note" style=indent(depth + 1)>"(empty)"</div> }.into_any()
                    }
                    Some(Ok(list)) => list.into_iter().map(|c| fs_node(c, depth + 1)).collect_view().into_any(),
                }}
            </Show>
        </div>
    }
    .into_any()
}

/// The right-click menu, drawn over everything at the click point.
#[component]
pub fn ContextMenu() -> impl IntoView {
    let app = ctx();
    let close = move || app.menu.set(None);
    move || {
        app.menu.get().map(|m| {
            let entry = StoredValue::new(m.entry.clone());
            let items = if m.entry.is_dir {
                view! {
                    <button on:click=move |_| {
                        close();
                        app.dialog.set(Some(Dialog::NewSupplement { parent: entry.with_value(|e| e.path.clone()) }));
                    }>"New supplement here…"</button>
                    <button on:click=move |_| {
                        close();
                        app.dialog.set(Some(Dialog::NewMonsterFile { parent: entry.with_value(|e| e.path.clone()) }));
                    }>"New monster file here…"</button>
                    <button on:click=move |_| {
                        close();
                        app.refresh_files();
                    }>"Refresh"</button>
                }
                .into_any()
            } else {
                let xml = m.entry.is_xml();
                view! {
                    <button
                        disabled=!xml
                        title=if xml { "" } else { "Only .xml files can be opened" }
                        on:click=move |_| {
                            close();
                            app.open_file(entry.with_value(|e| e.path.clone()));
                        }
                    >"Open"</button>
                }
                .into_any()
            };
            view! {
                <div class="menu-backdrop" on:click=move |_| close() on:contextmenu=move |ev: MouseEvent| {
                    ev.prevent_default();
                    close();
                }></div>
                <div class="menu" style=format!("left: {}px; top: {}px", m.x, m.y)>
                    <div class="menu-title">{m.entry.name.clone()}</div>
                    {items}
                </div>
            }
        })
    }
}
