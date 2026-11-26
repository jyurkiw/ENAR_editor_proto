//! The `<chapter>` edit form: the chapter's attributes and title, and its contents as
//! summaries (title, or the start of the first paragraph) that can be opened, moved,
//! deleted and added to. Everything waits for Apply, which validates before writing.

use leptos::prelude::*;
use shared::chapter::{self, ChapterEdit, ChapterModel, ChildItem, CHILD_TAGS, KINDS};
use shared::Change;

use crate::apply::apply;
use crate::state::DocTab;

/// The draft for one chapter, with the file's chapter as the fallback.
#[derive(Clone, Copy)]
struct Form {
    tab: DocTab,
    /// The chapter's name in the file (the draft's key).
    name: StoredValue<String>,
    base: Memo<Option<ChapterModel>>,
}

impl Form {
    fn model(&self) -> Option<ChapterModel> {
        let name = self.name.get_value();
        self.tab.chapter_drafts.with(|d| d.get(&name).cloned()).or_else(|| self.base.get())
    }

    fn update(&self, f: impl FnOnce(&mut ChapterModel)) {
        let name = self.name.get_value();
        let Some(base) = self.base.get_untracked() else { return };
        let mut m = self.tab.chapter_drafts.with_untracked(|d| d.get(&name).cloned()).unwrap_or_else(|| base.clone());
        f(&mut m);
        self.tab.chapter_drafts.update(|d| {
            if m == base {
                d.remove(&name);
            } else {
                d.insert(name, m);
            }
        });
    }

    fn dirty(&self) -> bool {
        let name = self.name.get_value();
        self.tab.chapter_drafts.with(|d| d.contains_key(&name))
    }
}

/// creditNamesType: names separated by ";", none empty.
fn credit_problem(v: &str) -> Option<&'static str> {
    (!v.trim().is_empty() && v.split(';').any(|p| p.trim().is_empty()))
        .then_some("Names separated by \";\", with none empty: \"Jeff Yurkiw; Ana Ruiz\"")
}

#[component]
fn Text(
    form: Form,
    label: &'static str,
    #[prop(optional)] hint: &'static str,
    #[prop(optional)] placeholder: &'static str,
    get: fn(&ChapterModel) -> String,
    set: fn(&mut ChapterModel, String),
    #[prop(optional)] check: Option<fn(&str) -> Option<&'static str>>,
) -> impl IntoView {
    let value = move || form.model().map(|m| get(&m)).unwrap_or_default();
    let problem = move || check.and_then(|c| c(&value()));
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input type="text" placeholder=placeholder class:invalid=move || problem().is_some() prop:value=value
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    form.update(|m| set(m, v));
                }/>
            {move || match problem() {
                Some(p) => view! { <span class="hint bad">{p}</span> }.into_any(),
                None => (!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> }).into_any(),
            }}
        </label>
    }
}

fn tag_label(tag: &str) -> &'static str {
    match tag {
        "section" => "Section",
        "sidebar" => "Sidebar",
        "table" => "Table",
        "pageEnd" => "Page end",
        _ => "Element",
    }
}

#[component]
pub fn ChapterForm(tab: DocTab, name: String) -> impl IntoView {
    let name = StoredValue::new(name);
    let base = Memo::new(move |_| {
        let d = tab.doc();
        let n = name.get_value();
        chapter::find_chapter(&d.source, &d.nodes, &n).map(|ci| chapter::read(&d.source, &d.nodes, ci))
    });
    let form = Form { tab, name, base };
    let busy = move || tab.applying.get();
    let refs = Memo::new(move |_| chapter::name_refs(&tab.doc().source, &name.get_value()).len());
    let renamed = move || form.model().is_some_and(|m| m.name != name.get_value());

    let apply_update = move |_| {
        if let Some(m) = form.model() {
            apply(tab, Change::Chapter { edit: ChapterEdit::Update { name: name.get_value(), model: m } });
        }
    };
    let revert = move |_| {
        let n = name.get_value();
        tab.chapter_drafts.update(|d| {
            d.remove(&n);
        });
    };
    let delete = move |_| {
        let n = name.get_value();
        let others = refs.get_untracked().saturating_sub(1); // its own <ch> goes with it
        let msg = if others > 0 {
            format!("Delete chapter \"{n}\"? {others} other reference(s) still point at it, so validation will refuse until they're removed.")
        } else {
            format!("Delete chapter \"{n}\" and everything in it? Its <ch> entry is removed from <chapterOrder>.")
        };
        crate::dialogs::ask(msg, move || apply(tab, Change::Chapter { edit: ChapterEdit::Delete { name: n } }));
    };

    // The file's chapters in order, and this one's place among them.
    let order = move || {
        let d = tab.doc();
        d.nodes.first().map(|r| r.children.iter().filter(|c| d.nodes[**c].tag == "chapter")
            .filter_map(|c| shared::edit::start_tag(&d.source, d.nodes[*c].start).and_then(|t| t.value("name"))).collect::<Vec<String>>())
            .unwrap_or_default()
    };
    let pos = move || order().iter().position(|n| *n == name.get_value());
    // Up: before the one above. Down: before the one after the next, or last.
    let move_by = move |step: i32| {
        let list = order();
        let Some(p) = list.iter().position(|n| *n == name.get_value()) else { return };
        let before = if step < 0 {
            if p == 0 { return }
            Some(list[p - 1].clone())
        } else {
            if p + 1 >= list.len() { return }
            list.get(p + 2).cloned()
        };
        apply(tab, Change::Chapter { edit: ChapterEdit::Move { name: name.get_value(), before } });
    };

    // Opening a child: the file's child at that position (only for children already in the file).
    let open_child = move |orig: usize| {
        let d = tab.doc_untracked();
        if let Some(ci) = chapter::find_chapter(&d.source, &d.nodes, &name.get_value()) {
            let kids: Vec<usize> = d.nodes[ci].children.iter().copied().filter(|c| d.nodes[*c].tag != "chapterTitle").collect();
            if let Some(k) = kids.get(orig) {
                tab.reveal(*k);
            }
        }
    };

    let rows = move || {
        let Some(m) = form.model() else { return Vec::new() };
        let n = m.children.len();
        m.children
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let orig = c.orig;
                let is_new = orig.is_none();
                let summary = if is_new {
                    if c.tag == "pageEnd" {
                        "Page end".to_string()
                    } else if !c.new_title.trim().is_empty() {
                        c.new_title.clone()
                    } else if c.tag == "table" {
                        format!("Table {} (no title)", c.key)
                    } else {
                        "(new, empty)".to_string()
                    }
                } else {
                    c.summary.clone()
                };
                let untitled = if is_new { c.new_title.trim().is_empty() && c.tag != "pageEnd" } else { c.untitled };
                let flagged = c.tag == "table" && untitled;
                let key = c.key.clone();
                let tag = c.tag.clone();
                let mv = move |delta: isize| {
                    form.update(|m| {
                        let j = i as isize + delta;
                        if j >= 0 && (j as usize) < m.children.len() {
                            m.children.swap(i, j as usize);
                        }
                    })
                };
                view! {
                    <div class="child-row" class:new=is_new class:clickable=!is_new
                        title=if is_new { "Not in the file until Apply" } else { "Open this element" }
                        on:click=move |_| if let Some(o) = orig { open_child(o) }>
                        <span class=format!("child-tag tag-{tag}")>{tag_label(&tag)}</span>
                        <span class="child-summary" class:untitled=untitled class:flagged=flagged>{summary}</span>
                        {(!key.is_empty()).then(|| view! { <code class="child-key">{key}</code> })}
                        {is_new.then(|| view! { <span class="new-badge">"new"</span> })}
                        <span class="child-buttons" on:click=|ev| ev.stop_propagation()>
                            <button class="icon-btn" title="Move up" disabled=move || i == 0 || busy() on:click=move |_| mv(-1)>"↑"</button>
                            <button class="icon-btn" title="Move down" disabled=move || i + 1 == n || busy() on:click=move |_| mv(1)>"↓"</button>
                            <button class="icon-btn danger" title="Delete (on Apply)" disabled=busy on:click=move |_| {
                                form.update(|m| {
                                    m.children.remove(i);
                                })
                            }>"✕"</button>
                        </span>
                    </div>
                }
            })
            .collect::<Vec<_>>()
    };

    // Adding a child.
    let add_tag = RwSignal::new("section".to_string());
    let add_key = RwSignal::new(String::new());
    let add_title = RwSignal::new(String::new());
    let key_problem = move || {
        let tag = add_tag.get();
        let key = add_key.get();
        if tag == "pageEnd" {
            return None;
        }
        if key.trim().is_empty() {
            return Some(if tag == "section" { "A section needs a name" } else { "Needs an id" });
        }
        let taken = form.model().is_some_and(|m| m.children.iter().any(|c| c.tag == tag && c.key == key));
        taken.then_some("Already used in this chapter")
    };
    let add = move |_| {
        if key_problem().is_some() {
            return;
        }
        let tag = add_tag.get_untracked();
        let key = if tag == "pageEnd" { String::new() } else { add_key.get_untracked().trim().to_string() };
        let title = if tag == "pageEnd" { String::new() } else { add_title.get_untracked().trim().to_string() };
        form.update(|m| m.children.push(ChildItem { tag, key, new_title: title, ..Default::default() }));
        add_key.set(String::new());
        add_title.set(String::new());
    };

    view! {
        {move || match form.model() {
            None => view! { <div class="empty"><p>"This chapter isn't in the file any more."</p></div> }.into_any(),
            Some(_) => view! {
                <div class="form-layout">
                    <div class="form wide">
                        <fieldset class="group">
                            <legend>"Chapter"</legend>
                            {move || {
                                let m = form.model().unwrap_or_default();
                                if m.title_markup {
                                    view! {
                                        <label class="field">
                                            <span class="field-label">"Title"</span>
                                            <input type="text" disabled prop:value=m.title.clone()/>
                                            <span class="hint">"This <chapterTitle> holds markup, so edit it in the source for now."</span>
                                        </label>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Text form label="Title" hint="<chapterTitle>" get=|m| m.title.clone() set=|m, v| m.title = v/>
                                    }.into_any()
                                }
                            }}
                            <div class="field-row">
                                <Text form label="Name" hint="@name: the slug chapterRef and chapterOrder use"
                                    get=|m| m.name.clone() set=|m, v| m.name = v/>
                                <label class="field">
                                    <span class="field-label">"Kind"</span>
                                    <select on:change=move |ev| {
                                        let v = event_target_value(&ev);
                                        // The default stays unwritten unless the file writes it.
                                        let base_kind = form.base.get_untracked().map(|b| b.kind).unwrap_or_default();
                                        form.update(|m| m.kind = if v == "chapter" && base_kind != "chapter" { String::new() } else { v });
                                    }>
                                        {KINDS.iter().map(|(k, d)| {
                                            let k = *k;
                                            view! {
                                                <option value=k selected=move || {
                                                    let cur = form.model().map(|m| m.kind).unwrap_or_default();
                                                    cur == k || (cur.is_empty() && k == "chapter")
                                                }>{format!("{k} — {d}")}</option>
                                            }
                                        }).collect_view()}
                                    </select>
                                </label>
                                <label class="field narrow">
                                    <span class="field-label">"Columns"</span>
                                    <select on:change=move |ev| {
                                        let v = event_target_value(&ev);
                                        let base_cols = form.base.get_untracked().map(|b| b.columns).unwrap_or_default();
                                        form.update(|m| m.columns = if v == "2" && base_cols != "2" { String::new() } else { v });
                                    }>
                                        <option value="2" selected=move || form.model().is_some_and(|m| m.columns != "1")>"2"</option>
                                        <option value="1" selected=move || form.model().is_some_and(|m| m.columns == "1")>"1 (full width)"</option>
                                    </select>
                                </label>
                            </div>
                            <Show when=renamed>
                                <p class="hint warn-text">{move || {
                                    let n = refs.get();
                                    format!("Apply also renames the {n} reference(s) to \"{}\": its <ch> in chapterOrder, any <chapterRef> and <data><chapter>.", name.get_value())
                                }}</p>
                            </Show>
                            <Text form label="Header art" placeholder="(none)" hint="<chapterTitle imgSrc>: an image file, relative to the supplement"
                                get=|m| m.title_img.clone() set=|m, v| m.title_img = v/>
                            <Text form label="Credit" placeholder="(none)" check=credit_problem
                                hint="Prints \"Written by …\" under the heading, and credits each name as an Author"
                                get=|m| m.credit.clone() set=|m, v| m.credit = v/>
                        </fieldset>

                        <fieldset class="group">
                            <legend>"Contents"</legend>
                            <div class="child-list">
                                {rows}
                                <Show when=move || form.model().is_some_and(|m| m.children.is_empty())>
                                    <p class="hint">"No sections yet."</p>
                                </Show>
                            </div>
                            <div class="add-row">
                                <select on:change=move |ev| add_tag.set(event_target_value(&ev))>
                                    {CHILD_TAGS.iter().map(|t| view! {
                                        <option value=*t selected=move || add_tag.get() == *t>{tag_label(t)}</option>
                                    }).collect_view()}
                                </select>
                                <Show when=move || add_tag.get() != "pageEnd">
                                    <input type="text" class:invalid=move || !add_key.get().is_empty() && key_problem().is_some()
                                        placeholder=move || if add_tag.get() == "section" { "name (slug)" } else { "id" }
                                        prop:value=move || add_key.get() on:input=move |ev| add_key.set(event_target_value(&ev))/>
                                    <input type="text" placeholder=move || if add_tag.get() == "table" { "title (optional, recommended)" } else { "title (optional)" }
                                        prop:value=move || add_title.get() on:input=move |ev| add_title.set(event_target_value(&ev))/>
                                </Show>
                                <button on:click=add disabled=move || key_problem().is_some()>"Add"</button>
                            </div>
                            {move || {
                                let p = key_problem();
                                (p.is_some() && !add_key.get().is_empty()).then(|| view! { <p class="hint bad">{p}</p> })
                            }}
                            <p class="hint">"New elements are added at the end; move them into place. Click a summary to open it."</p>
                        </fieldset>

                        <div class="form-actions">
                            <button class="danger" on:click=delete disabled=busy>"Delete chapter"</button>
                            <button title="Move the chapter up (its <ch> follows)" disabled=move || busy() || pos().is_none_or(|p| p == 0)
                                on:click=move |_| move_by(-1)>"↑ Move up"</button>
                            <button title="Move the chapter down (its <ch> follows)" disabled=move || busy() || pos().is_none_or(|p| p + 1 >= order().len())
                                on:click=move |_| move_by(1)>"↓ Move down"</button>
                            <span class="spacer"></span>
                            {move || form.dirty().then(|| view! { <span class="dirty-note">"Unapplied changes"</span> })}
                            <button on:click=revert disabled=move || !form.dirty() || busy()>"Revert"</button>
                            <button class="primary" on:click=apply_update disabled=move || !form.dirty() || busy()>
                                {move || if busy() { "Validating…" } else { "Apply" }}
                            </button>
                        </div>
                    </div>
                </div>
            }.into_any(),
        }}
    }
}
