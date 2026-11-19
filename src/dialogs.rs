//! Modal dialogs: errors, the busy notice, project settings and New supplement.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::chapter::{self, ChapterEdit, KINDS};
use shared::{main_file_for_type, Change, NewMagicItem, NewMonsterFile, NewSpell, NewSupplement, SUPPLEMENT_TYPES};

use crate::api;
use crate::apply::apply;
use crate::state::{ctx, Dialog};

/// Asks OK/Cancel and waits for the answer. The dialog plugin replaces `window.confirm`
/// with an async one (a Promise of the answer), so a plain `confirm_with_message` sees
/// the Promise, reads it as Cancel, and returns before the dialog is even answered.
pub async fn confirm(msg: &str) -> bool {
    use wasm_bindgen::JsCast;
    let Some(w) = web_sys::window() else { return false };
    let Some(f) = js_sys::Reflect::get(&w, &"confirm".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else {
        return false;
    };
    let Ok(v) = f.call1(&w, &msg.into()) else { return false };
    let v = match v.dyn_into::<js_sys::Promise>() {
        Ok(p) => wasm_bindgen_futures::JsFuture::from(p).await.unwrap_or(wasm_bindgen::JsValue::FALSE),
        Err(v) => v,
    };
    v.as_bool().unwrap_or(false)
}

/// Asks OK/Cancel; runs `yes` once the answer is OK, under the caller's reactive owner
/// (after the await there is none, and `apply` needs the app's context from it).
pub fn ask(msg: impl Into<String>, yes: impl FnOnce() + 'static) {
    let msg = msg.into();
    let owner = Owner::current();
    spawn_local(async move {
        if confirm(&msg).await {
            match owner {
                Some(o) => o.with(yes),
                None => yes(),
            }
        }
    });
}

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
        Some(Dialog::NewMonsterFile { parent }) => view! { <NewMonsterFileDialog parent/> }.into_any(),
        Some(Dialog::NewMagicItem { parent }) => view! { <NewMagicItemDialog parent/> }.into_any(),
        Some(Dialog::NewSpell { parent }) => view! { <NewSpellDialog parent/> }.into_any(),
        Some(Dialog::NewChapter { tab }) => view! { <NewChapterDialog tab_id=tab/> }.into_any(),
        Some(Dialog::PillAll { tab, title, cands, all }) => view! { <PillAllDialog tab_id=tab title cands all/> }.into_any(),
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

/// "Pill every mention": each mention of the names in the file's prose, ticked, with
/// its place and the text around it. Unapplied paragraphs are applied first, so the list
/// (and the replace) sees them. The ticked ones become refs in one change, validated
/// with the whole file.
#[component]
fn PillAllDialog(tab_id: u32, title: String, cands: Vec<shared::pills::Candidate>, all: Vec<shared::pills::Candidate>) -> impl IntoView {
    let app = ctx();
    let Some(tab) = app.tabs.with_untracked(|t| t.iter().find(|t| t.id == tab_id).copied()) else {
        app.dialog.set(None);
        return ().into_any();
    };
    // Apply the paragraphs still being edited (those whose drafts still fit the file).
    let d = tab.doc_untracked();
    let flush: Vec<Change> = tab.p_drafts.with_untracked(|drafts| {
        drafts
            .iter()
            .filter(|((at, child), (orig, _))| shared::block::p_text_at(&d.source, &d.nodes, at, *child).as_deref() == Some(orig.as_str()))
            .map(|((at, child), (_, text))| Change::Block { edit: shared::block::BlockEdit::SetP { at: at.clone(), child: *child, text: text.clone() } })
            .collect()
    });
    let flushed = flush.len();
    if !flush.is_empty() {
        apply(tab, Change::Batch { changes: flush });
    }
    let cands = StoredValue::new(cands);
    let all = StoredValue::new(all);
    // Each mention, with the ref for its form on it.
    let found = Memo::new(move |_| {
        let src = tab.doc().source.clone();
        let m = all.with_value(|a| cands.with_value(|c| shared::pills::mentions_of(&src, a, c)));
        m.into_iter()
            .map(|(mut o, k)| {
                o.cands = vec![k];
                o
            })
            .collect::<Vec<_>>()
    });
    // Unticked mentions, by their place in the file (reset when the file changes).
    let off = RwSignal::new(std::collections::HashSet::<usize>::new());
    Effect::new(move |_| {
        found.track();
        off.set(Default::default());
    });
    let count = move || found.with(|f| f.iter().filter(|o| !off.with(|x| x.contains(&o.start))).count());
    let replace = move || {
        let src = tab.doc_untracked().source.clone();
        let edits: Vec<shared::pills::PillEdit> = found.with_untracked(|f| {
            f.iter()
                .filter(|o| !off.with_untracked(|x| x.contains(&o.start)))
                .map(|o| shared::pills::PillEdit {
                    start: o.start,
                    end: o.end,
                    raw: src[o.start..o.end].to_string(),
                    xml: cands.with_value(|c| c[o.cands[0]].ref_xml.clone()),
                })
                .collect()
        });
        if edits.is_empty() {
            return;
        }
        app.dialog.set(None);
        apply(tab, Change::Pills { edits });
    };
    let close = move || app.dialog.set(None);
    view! {
        <div class="modal-backdrop" on:keydown=move |ev: leptos::ev::KeyboardEvent| if ev.key() == "Escape" { close() }>
            <div class="modal pill-all">
                <h2>"Pill every mention: " {title}</h2>
                {(flushed > 0).then(|| view! { <p class="hint">{format!("Applied {flushed} paragraph{} you were editing first.", if flushed == 1 { "" } else { "s" })}</p> })}
                {move || if tab.applying.get() {
                    view! { <p class="hint">"Writing the file…"</p> }.into_any()
                } else if found.with(|f| f.is_empty()) {
                    view! { <p class="hint">"No mentions left in this file's prose."</p> }.into_any()
                } else {
                    view! {
                        <div class="pill-all-tools">
                            <button type="button" class="small" on:click=move |_| off.set(Default::default())>"Select all"</button>
                            <button type="button" class="small" on:click=move |_| off.set(found.with_untracked(|f| f.iter().map(|o| o.start).collect()))>"Select none"</button>
                            <span class="hint">"Headings and text inside other tags are left alone."</span>
                        </div>
                        <div class="pill-all-list">
                            {found.get().into_iter().map(|o| {
                                let start = o.start;
                                view! {
                                    <label class="pill-all-row">
                                        <input type="checkbox" prop:checked=move || !off.with(|x| x.contains(&start))
                                            on:change=move |ev| {
                                                let on = event_target_checked(&ev);
                                                off.update(|x| { if on { x.remove(&start); } else { x.insert(start); } });
                                            }/>
                                        <span class="pill-all-text">
                                            <span class="pill-all-place">{o.place.clone()}</span>
                                            <span>{o.before.clone()} " " <mark>{o.text.clone()}</mark> " " {o.after.clone()}</span>
                                        </span>
                                    </label>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
                <div class="buttons">
                    <button type="button" on:click=move |_| close()>"Cancel"</button>
                    <button type="button" class="primary" disabled=move || count() == 0 || tab.applying.get() on:click=move |_| replace()>
                        {move || format!("Replace {}", count())}
                    </button>
                </div>
            </div>
        </div>
    }
    .into_any()
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

/// "rats" -> "rats" (a stem), for the id of the first stat block.
fn slug_of(s: &str) -> String {
    let s: String = s.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-")
}

#[component]
fn NewMonsterFileDialog(parent: String) -> impl IntoView {
    let app = ctx();
    let file = RwSignal::new(String::new());
    let family = RwSignal::new(String::new());
    let noun = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let id = RwSignal::new(String::new());
    // Later fields follow earlier ones until they're edited.
    let edited = RwSignal::new((false, false, false, false));
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
        let f = file.get();
        let f = f.trim().trim_end_matches(".xml");
        format!("{}\\{}.xml", rel_parent(), if f.is_empty() { "…" } else { f })
    };
    let follow_name = move |v: &str| {
        if !edited.get_untracked().3 {
            id.set(slug_of(v));
        }
    };
    let create = move || {
        let req = NewMonsterFile {
            parent: parent.get_value(),
            file: file.get_untracked(),
            family: family.get_untracked(),
            short_noun: noun.get_untracked(),
            id: id.get_untracked(),
            name: name.get_untracked(),
        };
        spawn_local(async move {
            match api::create_monster_file(&req).await {
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
                <h2>"New monster file"</h2>
                <label class="field">
                    <span class="field-label">"File name"</span>
                    <input type="text" autofocus placeholder="giant-rats" prop:value=move || file.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let e = edited.get_untracked();
                            if !e.0 {
                                let fam = title_from_folder(v.trim_end_matches(".xml"));
                                if !e.2 {
                                    name.set(fam.clone());
                                    follow_name(&fam);
                                }
                                family.set(fam);
                            }
                            file.set(v);
                        }/>
                </label>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Family name"</span>
                        <input type="text" placeholder="Giant Rats" prop:value=move || family.get() on:input=move |ev| {
                            edited.update(|e| e.0 = true);
                            family.set(event_target_value(&ev));
                        }/>
                        <span class="hint">"<name>: the heading over the family"</span>
                    </label>
                    <label class="field">
                        <span class="field-label">"Short noun"</span>
                        <input type="text" placeholder="rat" prop:value=move || noun.get() on:input=move |ev| {
                            edited.update(|e| e.1 = true);
                            noun.set(event_target_value(&ev));
                        }/>
                        <span class="hint">"<shortNoun>: what <shortNounRef/> prints"</span>
                    </label>
                </div>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"First stat block"</span>
                        <input type="text" placeholder="Giant Rat" prop:value=move || name.get() on:input=move |ev| {
                            edited.update(|e| e.2 = true);
                            let v = event_target_value(&ev);
                            follow_name(&v);
                            name.set(v);
                        }/>
                    </label>
                    <label class="field">
                        <span class="field-label">"Its id"</span>
                        <input type="text" placeholder="giant-rat" prop:value=move || id.get() on:input=move |ev| {
                            edited.update(|e| e.3 = true);
                            id.set(event_target_value(&ev));
                        }/>
                    </label>
                </div>
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

#[component]
fn NewMagicItemDialog(parent: String) -> impl IntoView {
    let app = ctx();
    let file = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let id = RwSignal::new(String::new());
    let item_type = RwSignal::new("wondrous-item".to_string());
    let rarity = RwSignal::new("uncommon".to_string());
    // The name and id follow the file name until they're edited.
    let edited = RwSignal::new((false, false));
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
        let f = file.get();
        let f = f.trim().trim_end_matches(".xml");
        format!("{}\\{}.xml", rel_parent(), if f.is_empty() { "…" } else { f })
    };
    let create = move || {
        let req = NewMagicItem {
            parent: parent.get_value(),
            file: file.get_untracked(),
            id: id.get_untracked(),
            name: name.get_untracked(),
            item_type: item_type.get_untracked(),
            rarity: rarity.get_untracked(),
        };
        spawn_local(async move {
            match api::create_magic_item_file(&req).await {
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
                <h2>"New magic item"</h2>
                <label class="field">
                    <span class="field-label">"File name"</span>
                    <input type="text" autofocus placeholder="flame-tongue" prop:value=move || file.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let (n, i) = edited.get_untracked();
                            let stem = v.trim().trim_end_matches(".xml").to_string();
                            if !n {
                                name.set(title_from_folder(&stem));
                            }
                            if !i {
                                id.set(slug_of(&stem));
                            }
                            file.set(v);
                        }/>
                </label>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Item name"</span>
                        <input type="text" placeholder="Flame Tongue" prop:value=move || name.get() on:input=move |ev| {
                            edited.update(|e| e.0 = true);
                            name.set(event_target_value(&ev));
                        }/>
                    </label>
                    <label class="field">
                        <span class="field-label">"Id"</span>
                        <input type="text" placeholder="flame-tongue" prop:value=move || id.get() on:input=move |ev| {
                            edited.update(|e| e.1 = true);
                            id.set(event_target_value(&ev));
                        }/>
                    </label>
                </div>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Type"</span>
                        <select prop:value=move || item_type.get() on:change=move |ev| item_type.set(event_target_value(&ev))>
                            {shared::item::TYPES.iter().map(|(v, l)| view! { <option value=*v>{*l}</option> }).collect_view()}
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">"Rarity"</span>
                        <select prop:value=move || rarity.get() on:change=move |ev| rarity.set(event_target_value(&ev))>
                            {shared::item::RARITIES.iter().map(|(v, l)| view! { <option value=*v>{*l}</option> }).collect_view()}
                        </select>
                    </label>
                </div>
                <p class="hint">"Starts as a draft with no attunement and one empty paragraph."</p>
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

#[component]
fn NewSpellDialog(parent: String) -> impl IntoView {
    let app = ctx();
    let file = RwSignal::new(String::new());
    let name = RwSignal::new(String::new());
    let id = RwSignal::new(String::new());
    let level = RwSignal::new("1".to_string());
    let school = RwSignal::new("Evocation".to_string());
    // The name and id follow the file name until they're edited.
    let edited = RwSignal::new((false, false));
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
        let f = file.get();
        let f = f.trim().trim_end_matches(".xml");
        format!("{}\\{}.xml", rel_parent(), if f.is_empty() { "…" } else { f })
    };
    let create = move || {
        let req = NewSpell {
            parent: parent.get_value(),
            file: file.get_untracked(),
            id: id.get_untracked(),
            name: name.get_untracked(),
            level: level.get_untracked(),
            school: school.get_untracked(),
        };
        spawn_local(async move {
            match api::create_spell_file(&req).await {
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
                <h2>"New spell"</h2>
                <label class="field">
                    <span class="field-label">"File name"</span>
                    <input type="text" autofocus placeholder="kindle-blade" prop:value=move || file.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            let (n, i) = edited.get_untracked();
                            let stem = v.trim().trim_end_matches(".xml").to_string();
                            if !n {
                                name.set(title_from_folder(&stem));
                            }
                            if !i {
                                id.set(slug_of(&stem));
                            }
                            file.set(v);
                        }/>
                </label>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Spell name"</span>
                        <input type="text" placeholder="Kindle Blade" prop:value=move || name.get() on:input=move |ev| {
                            edited.update(|e| e.0 = true);
                            name.set(event_target_value(&ev));
                        }/>
                    </label>
                    <label class="field">
                        <span class="field-label">"Id"</span>
                        <input type="text" placeholder="kindle-blade" prop:value=move || id.get() on:input=move |ev| {
                            edited.update(|e| e.1 = true);
                            id.set(event_target_value(&ev));
                        }/>
                    </label>
                </div>
                <div class="field-row">
                    <label class="field">
                        <span class="field-label">"Level"</span>
                        <select prop:value=move || level.get() on:change=move |ev| level.set(event_target_value(&ev))>
                            {shared::spell::LEVELS.iter().map(|v| view! { <option value=*v>{if *v == "0" { "Cantrip".to_string() } else { format!("Level {v}") }}</option> }).collect_view()}
                        </select>
                    </label>
                    <label class="field">
                        <span class="field-label">"School"</span>
                        <select prop:value=move || school.get() on:change=move |ev| school.set(event_target_value(&ev))>
                            {shared::spell::SCHOOLS.iter().map(|v| view! { <option value=*v>{*v}</option> }).collect_view()}
                        </select>
                    </label>
                </div>
                <p class="hint">"Starts as a draft: an action to cast, 60 feet, V and S, instantaneous, one empty paragraph."</p>
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
