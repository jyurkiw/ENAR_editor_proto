//! Modal dialogs: errors, the busy notice, project settings and New supplement.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::{self, ChapterEdit, KINDS};
use shared::{main_file_for_type, Change, NewSupplement, SUPPLEMENT_TYPES};

use crate::api;
use crate::apply::apply;
use crate::state::{ctx, Dialog};

#[component]
pub fn ErrorDialog() -> impl IntoView {
    let app = ctx();
    move || {
        app.error.get().map(|e| {
            view! {
                <div class="modal-backdrop">
                    <div class="modal error-modal" role="alertdialog">
                        <h2>"Can't do that"</h2>
                        <p class="message">{e.message.clone()}</p>
                        {(!e.details.is_empty()).then(|| view! {
                            <ul class="details">
                                {e.details.iter().map(|d| view! { <li>{d.clone()}</li> }).collect_view()}
                            </ul>
                        })}
                        <div class="buttons">
                            <button class="primary" on:click=move |_| app.error.set(None)>"OK"</button>
                        </div>
                    </div>
                </div>
            }
        })
    }
}

#[component]
pub fn BusyNotice() -> impl IntoView {
    let app = ctx();
    move || {
        app.busy.get().map(|msg| {
            view! {
                <div class="modal-backdrop light">
                    <div class="busy"><span class="spinner"></span>{msg}</div>
                </div>
            }
        })
    }
}

#[component]
pub fn Dialogs() -> impl IntoView {
    let app = ctx();
    move || match app.dialog.get() {
        None => ().into_any(),
        Some(Dialog::ConfigureProject { root, schemas, compiler, problem }) => {
            view! { <ConfigureProject root schemas compiler problem/> }.into_any()
        }
        Some(Dialog::NewSupplement { parent }) => view! { <NewSupplementDialog parent/> }.into_any(),
        Some(Dialog::NewChapter { tab }) => view! { <NewChapterDialog tab_id=tab/> }.into_any(),
    }
}

/// A text field with a Browse… button that fills it from a folder picker.
#[component]
fn FolderField(label: &'static str, value: RwSignal<String>, hint: &'static str) -> impl IntoView {
    let browse = move |_| {
        spawn_local(async move {
            let cur = value.get_untracked();
            if let Some(p) = api::pick_folder(label, (!cur.is_empty()).then_some(cur.as_str())).await {
                value.set(p);
            }
        })
    };
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <div class="with-button">
                <input type="text" prop:value=move || value.get() on:input=move |ev| value.set(event_target_value(&ev))/>
                <button type="button" on:click=browse>"Browse…"</button>
            </div>
            <span class="hint">{hint}</span>
        </label>
    }
}

#[component]
fn ConfigureProject(root: String, schemas: String, compiler: String, problem: Option<String>) -> impl IntoView {
    let app = ctx();
    let schemas = RwSignal::new(schemas);
    let compiler = RwSignal::new(compiler);
    let error = RwSignal::new(None::<String>);
    let root = StoredValue::new(root);
    let save = move |_| {
        spawn_local(async move {
            let r = api::configure_project(&root.get_value(), &schemas.get_untracked(), &compiler.get_untracked()).await;
            match r {
                Ok(p) => {
                    app.dialog.set(None);
                    app.set_project(p);
                }
                Err(e) => error.set(Some(e.message)),
            }
        })
    };
    view! {
        <div class="modal-backdrop">
            <div class="modal">
                <h2>"Project settings"</h2>
                <p>"Set up " <code>{root.get_value()}</code> " as a project. The settings are saved in "
                   <code>{shared::PROJECT_FILE}</code> " in that folder."</p>
                {problem.map(|p| view! { <p class="problem">{p}</p> })}
                <FolderField label="Schemas folder" value=schemas
                    hint="The folder holding supplement-template-schema.xsd"/>
                <FolderField label="Compiler folder" value=compiler
                    hint="The encompile compiler (has pyproject.toml). Files are validated with its lxml through uv."/>
                {move || error.get().map(|e| view! { <p class="problem">{e}</p> })}
                <div class="buttons">
                    <button on:click=move |_| app.dialog.set(None)>"Cancel"</button>
                    <button class="primary" on:click=save>"Save"</button>
                </div>
            </div>
        </div>
    }
}

/// "my-new_adventure" -> "My New Adventure"
fn title_from_folder(folder: &str) -> String {
    folder
        .split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
fn NewSupplementDialog(parent: String) -> impl IntoView {
    let app = ctx();
    let folder = RwSignal::new(String::new());
    let kind = RwSignal::new("adventure".to_string());
    let title = RwSignal::new(String::new());
    // The title follows the folder name until it's edited.
    let title_edited = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let parent = StoredValue::new(parent);
    let rel_parent = move || {
        let p = parent.get_value();
        match app.project.get() {
            Some(pr) if p.len() >= pr.root.len() && p[..pr.root.len()].eq_ignore_ascii_case(&pr.root) => {
                format!("{}{}", pr.name, &p[pr.root.len()..])
            }
            _ => p,
        }
    };
    let preview = move || {
        let f = folder.get();
        let f = if f.trim().is_empty() { "…".to_string() } else { f };
        format!("{}\\{}\\{}", rel_parent(), f, main_file_for_type(&kind.get()))
    };
    let create = move || {
        let req = NewSupplement {
            parent: parent.get_value(),
            folder: folder.get_untracked(),
            kind: kind.get_untracked(),
            title: title.get_untracked(),
        };
        spawn_local(async move {
            match api::create_supplement(&req).await {
                Ok(path) => {
                    app.dialog.set(None);
                    app.open_dirs.update(|d| {
                        d.insert(req.parent.clone());
                    });
                    app.refresh_files();
                    app.open_file(path);
                }
                Err(e) => error.set(Some(e.message)),
            }
        })
    };
    view! {
        <div class="modal-backdrop">
            <form class="modal" on:submit=move |ev| {
                ev.prevent_default();
                create();
            }>
                <h2>"New supplement"</h2>
                <label class="field">
                    <span class="field-label">"Folder name"</span>
                    <input type="text" autofocus placeholder="my-adventure" prop:value=move || folder.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            if !title_edited.get_untracked() {
                                title.set(title_from_folder(&v));
                            }
                            folder.set(v);
                        }/>
                </label>
                <label class="field">
                    <span class="field-label">"Type"</span>
                    <select on:change=move |ev| kind.set(event_target_value(&ev)) prop:value=move || kind.get()>
                        {SUPPLEMENT_TYPES.iter().map(|(group, types)| view! {
                            <optgroup label=*group>
                                {types.iter().map(|(t, d)| view! {
                                    <option value=*t selected=*t == "adventure">{format!("{t} — {d}")}</option>
                                }).collect_view()}
                            </optgroup>
                        }).collect_view()}
                    </select>
                </label>
                <label class="field">
                    <span class="field-label">"Title"</span>
                    <input type="text" prop:value=move || title.get() on:input=move |ev| {
                        title_edited.set(true);
                        title.set(event_target_value(&ev));
                    }/>
                    <span class="hint">"Printed on the cover (<headerTitle>)."</span>
                </label>
                <p class="preview">"Creates " <code>{preview}</code></p>
                {move || error.get().map(|e| view! { <p class="problem">{e}</p> })}
                <div class="buttons">
                    <button type="button" on:click=move |_| app.dialog.set(None)>"Cancel"</button>
                    <button type="submit" class="primary">"Create"</button>
                </div>
            </form>
        </div>
    }
}

/// "The Gnashing Gate" -> "gnashing-gate"
fn slug(title: &str) -> String {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    let skip = ["the", "a", "an"];
    let words: Vec<&String> = match words.first() {
        Some(f) if words.len() > 1 && skip.contains(&f.as_str()) => words.iter().skip(1).collect(),
        _ => words.iter().collect(),
    };
    words.into_iter().map(String::as_str).collect::<Vec<_>>().join("-")
}

#[component]
fn NewChapterDialog(tab_id: u32) -> impl IntoView {
    let app = ctx();
    let title = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let name_edited = RwSignal::new(false);
    let kind = RwSignal::new("chapter".to_string());
    let tab = move || app.tabs.with_untracked(|t| t.iter().find(|t| t.id == tab_id).copied());
    let problem = move || {
        let n = name.get();
        if n.trim().is_empty() {
            return Some("The chapter needs a name".to_string());
        }
        let t = tab()?;
        let d = t.doc_untracked();
        chapter::find_chapter(&d.source, &d.nodes, n.trim()).map(|_| format!("There's already a chapter named \"{}\"", n.trim()))
    };
    let create = move || {
        if problem().is_some() {
            return;
        }
        if let Some(t) = tab() {
            app.dialog.set(None);
            apply(t, Change::Chapter {
                edit: ChapterEdit::Add {
                    name: name.get_untracked().trim().to_string(),
                    title: title.get_untracked().trim().to_string(),
                    kind: kind.get_untracked(),
                },
            });
        }
    };
    view! {
        <div class="modal-backdrop">
            <form class="modal" on:submit=move |ev| {
                ev.prevent_default();
                create();
            }>
                <h2>"Add chapter"</h2>
                <p class="hint">"Added after the last chapter, and at the end of <chapterOrder>."</p>
                <label class="field">
                    <span class="field-label">"Title"</span>
                    <input type="text" autofocus placeholder="The Gnashing Gate" prop:value=move || title.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            if !name_edited.get_untracked() {
                                name.set(slug(&v));
                            }
                            title.set(v);
                        }/>
                </label>
                <label class="field">
                    <span class="field-label">"Name"</span>
                    <input type="text" placeholder="gnashing-gate" prop:value=move || name.get()
                        on:input=move |ev| {
                            name_edited.set(true);
                            name.set(event_target_value(&ev));
                        }/>
                    <span class="hint">"@name: the slug chapterRef and chapterOrder use"</span>
                </label>
                <label class="field">
                    <span class="field-label">"Kind"</span>
                    <select on:change=move |ev| kind.set(event_target_value(&ev))>
                        {KINDS.iter().map(|(k, d)| view! { <option value=*k selected=*k == "chapter">{format!("{k} — {d}")}</option> }).collect_view()}
                    </select>
                </label>
                {move || problem().filter(|_| !name.get().is_empty()).map(|p| view! { <p class="problem">{p}</p> })}
                <div class="buttons">
                    <button type="button" on:click=move |_| app.dialog.set(None)>"Cancel"</button>
                    <button type="submit" class="primary" disabled=move || problem().is_some()>"Add chapter"</button>
                </div>
            </form>
        </div>
    }
}
