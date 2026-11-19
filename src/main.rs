//! Enigma Narrative supplement editor: the Leptos frontend.
//!
//! Layout: a toolbar; file tabs across the top (one per open file); vertical tabs on the
//! left switching the side panel between the File-system tree and the File-structure
//! tree; the main area showing the selected element's source.

mod api;
mod apply;
mod block_form;
mod box_form;
mod chapter_form;
mod cover_form;
mod dialogs;
mod reference_form;
mod files;
mod item_form;
mod spell_form;
mod spelling;
mod hints;
mod license_form;
mod supplement_parts;
mod section_el_form;
mod list_form;
mod prose_editor;
mod family_form;
mod statblock_form;
mod template_form;
mod monster_form;
mod state;
mod structure;
mod table_form;
mod viewer;

use leptos::prelude::*;
use leptos::task::spawn_local;

use dialogs::{BusyNotice, Dialogs, ErrorDialog};
use files::{ContextMenu, FilesPanel};
use state::{ctx, AppCtx, Dialog, Side};
use structure::StructurePanel;
use viewer::Viewer;

fn main() {
    // This crate is the web UI: it runs as WebAssembly inside the app window. Built for the
    // desktop (`cargo run` in this folder), its first browser call would panic.
    #[cfg(not(target_arch = "wasm32"))]
    {
        eprintln!("enigma-ui is the editor's web UI and only runs inside the app. Start the editor with:\n\n    cargo tauri dev\n");
        std::process::exit(1);
    }
    #[allow(unreachable_code)]
    console_error_panic_hook::set_once();
    spelling::start();
    leptos::mount::mount_to_body(App);
}

#[component]
fn App() -> impl IntoView {
    let app = AppCtx::new();
    provide_context(app);
    // Reopen the last project.
    spawn_local(async move {
        if let Some(p) = api::last_project().await {
            app.open_project(p);
        }
    });
    // The spellchecker's known words: the project word list, and every library name.
    Effect::new(move |_| {
        if app.project.with(|p| p.is_some()) {
            spawn_local(async move {
                spelling::set_project_words(api::project_words().await.unwrap_or_default());
                let lib = api::library().await.map_err(|e| e.message);
                app.library.set(Some(lib));
            });
        }
    });
    Effect::new(move |_| {
        app.library.with(|l| {
            if let Some(Ok(lib)) = l {
                spelling::set_library_words(spelling::words_of(lib.iter().flat_map(|e| std::iter::once(e.name.as_str()).chain(e.aliases.iter().map(|(_, t)| t.as_str())))));
            }
        })
    });
    view! {
        <div class="app">
            <Toolbar/>
            <FileTabs/>
            <div class="body">
                <SideTabs/>
                <aside class="side">
                    {move || match app.side.get() {
                        Side::Files => view! { <FilesPanel/> }.into_any(),
                        Side::Structure => view! { <StructurePanel/> }.into_any(),
                    }}
                </aside>
                <main class="main"><Viewer/></main>
            </div>
            <ContextMenu/>
            <Dialogs/>
            <BusyNotice/>
            <ErrorDialog/>
        </div>
    }
}

#[component]
fn Toolbar() -> impl IntoView {
    let app = ctx();
    let new_supplement = move |_| {
        if let Some(p) = app.project.get_untracked() {
            app.dialog.set(Some(Dialog::NewSupplement { parent: p.root }));
        }
    };
    let new_monster_file = move |_| {
        if let Some(p) = app.project.get_untracked() {
            app.dialog.set(Some(Dialog::NewMonsterFile { parent: p.root }));
        }
    };
    let new_spell = move |_| {
        if let Some(p) = app.project.get_untracked() {
            app.dialog.set(Some(Dialog::NewSpell { parent: p.root }));
        }
    };
    let new_item = move |_| {
        if let Some(p) = app.project.get_untracked() {
            app.dialog.set(Some(Dialog::NewMagicItem { parent: p.root }));
        }
    };
    view! {
        <header class="toolbar">
            <span class="brand">"Enigma Narrative"</span>
            <span class="project" title=move || app.project.get().map(|p| p.root).unwrap_or_default()>
                {move || app.project.get().map(|p| p.name).unwrap_or_else(|| "no project".into())}
            </span>
            <span class="spacer"></span>
            <button on:click=move |_| app.pick_and_open_project()>"Open project…"</button>
            <button disabled=move || app.project.get().is_none() on:click=new_supplement>"New supplement…"</button>
            <button disabled=move || app.project.get().is_none() on:click=new_monster_file>"New monster file…"</button>
            <button disabled=move || app.project.get().is_none() on:click=new_item>"New magic item…"</button>
            <button disabled=move || app.project.get().is_none() on:click=new_spell>"New spell…"</button>
        </header>
    }
}

#[component]
fn FileTabs() -> impl IntoView {
    let app = ctx();
    view! {
        <nav class="file-tabs">
            <For
                each=move || app.tabs.get()
                key=|t| t.id
                children=move |t| {
                    let id = t.id;
                    let name = t.doc_untracked().name.clone();
                    let folder = t.doc_untracked().path.rsplit(['/', '\\']).nth(1).unwrap_or("").to_string();
                    let path = t.doc_untracked().path.clone();
                    view! {
                        <div
                            class="file-tab"
                            class:active=move || app.active.get() == Some(id)
                            title=path
                            on:click=move |_| app.active.set(Some(id))
                            on:auxclick=move |ev| if ev.button() == 1 { app.close_tab(id) }
                        >
                            <span class="tab-name">{name}</span>
                            <span class="tab-folder">{folder}</span>
                            <button class="close" title="Close" on:click=move |ev| {
                                ev.stop_propagation();
                                app.close_tab(id);
                            }>"×"</button>
                        </div>
                    }
                }
            />
            <Show when=move || app.tabs.with(|t| t.is_empty())>
                <div class="file-tabs-empty">"No open files"</div>
            </Show>
        </nav>
    }
}

#[component]
fn SideTabs() -> impl IntoView {
    let app = ctx();
    let tab = move |side: Side, label: &'static str| {
        view! {
            <button class="side-tab" class:active=move || app.side.get() == side on:click=move |_| app.side.set(side)>
                {label}
            </button>
        }
    };
    view! {
        <nav class="side-tabs">
            {tab(Side::Files, "File system")}
            {tab(Side::Structure, "File structure")}
        </nav>
    }
}
