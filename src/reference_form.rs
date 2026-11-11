//! The `<reference>` editor: every entry as a card, stacked in file order. A card is a
//! one-line summary that opens in place to its form. Each card applies on its own: when
//! focus leaves it, or on its Apply button. Add (a menu of entry kinds), delete and
//! drag-to-reorder apply at once. Every apply is validated before anything is written.

use std::collections::HashMap;

use leptos::ev::{DragEvent, FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::reference::{self, RefEdit, RefEntry, KINDS};
use shared::values::{is_length, parse_length};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::api;
use crate::apply::{apply_from, ref_key};
use crate::state::DocTab;

const CRS: &[&str] = &[
    "0", "1/8", "1/4", "1/2", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
];

/// Attributes each kind's form shows; anything else is listed as "kept as written".
fn known_attrs(tag: &str) -> &'static [&'static str] {
    match tag {
        "monster" => &["id", "name", "displayName", "shortName", "cr", "src", "file", "monster", "summonerRef", "spellLevel"],
        "spell" => &["id", "name", "src", "file"],
        "item" => &["id", "name", "displayName", "src", "file"],
        "noun" => &["id", "name", "displayName", "shortName", "kind"],
        "image" => &["id", "src", "name", "credit", "height", "span", "placement", "align"],
        "data" => &["id", "value"],
        _ => &["id"],
    }
}

/// heightType: a LaTeX length, or (the old form) a bare integer, which is ignored.
fn height_problem(v: &str) -> Option<&'static str> {
    if v.is_empty() || is_length(v) {
        None
    } else if v.chars().all(|c| c.is_ascii_digit()) {
        Some("A bare number has no unit and is ignored (the compiler warns); write 3in or 45%")
    } else {
        Some("A LaTeX length (4in, 10cm) or a % of the text height (45%)")
    }
}

fn dir_of(path: &str) -> &str {
    path.rfind(['\\', '/']).map_or("", |i| &path[..i])
}

/// One card's state: the file's entry (None for a new card) and the draft.
#[derive(Clone, Copy)]
struct Card {
    tab: DocTab,
    key: StoredValue<String>,
    /// The entry in the file, by key, from the current doc.
    base: Memo<Option<RefEntry>>,
}

impl Card {
    fn model(&self) -> RefEntry {
        let key = self.key.get_value();
        self.tab
            .ref_drafts
            .with(|d| d.get(&key).cloned())
            .or_else(|| self.base.get())
            .unwrap_or_default()
    }

    fn update(&self, f: impl FnOnce(&mut RefEntry)) {
        let key = self.key.get_value();
        let mut m = self.tab.ref_drafts.with_untracked(|d| d.get(&key).cloned()).or_else(|| self.base.get_untracked()).unwrap_or_default();
        f(&mut m);
        let base = self.base.get_untracked();
        self.tab.ref_drafts.update(|d| {
            if base.as_ref() == Some(&m) {
                d.remove(&key);
            } else {
                d.insert(key, m);
            }
        });
    }

    fn is_new(&self) -> bool {
        self.key.with_value(|k| k.starts_with("new:"))
    }

    fn dirty(&self) -> bool {
        let key = self.key.get_value();
        self.tab.ref_drafts.with(|d| d.contains_key(&key))
    }

    /// Validate and write this card's draft.
    fn apply(&self) {
        let key = self.key.get_value();
        let Some(entry) = self.tab.ref_drafts.with_untracked(|d| d.get(&key).cloned()) else { return };
        let edit = match self.base.get_untracked() {
            Some(b) => RefEdit::Update { tag: b.tag.clone(), id: b.id(), entry },
            None => RefEdit::Add { entry },
        };
        apply_from(self.tab, Change::Reference { edit }, Some(key));
    }

    fn revert(&self) {
        let key = self.key.get_value();
        if self.is_new() {
            self.tab.ref_new.update(|n| n.retain(|k| *k != key));
        }
        self.tab.ref_drafts.update(|d| {
            d.remove(&key);
        });
        self.tab.ref_failed.update(|f| {
            f.remove(&key);
        });
    }
}

#[component]
fn Field(
    card: Card,
    attr: &'static str,
    label: &'static str,
    #[prop(optional)] hint: &'static str,
    #[prop(optional)] placeholder: &'static str,
    #[prop(optional)] required: bool,
    #[prop(optional)] check: Option<fn(&str) -> Option<&'static str>>,
) -> impl IntoView {
    let value = move || card.model().get(attr);
    let problem = move || {
        let v = value();
        if required && v.trim().is_empty() {
            return Some("Required");
        }
        check.and_then(|c| c(&v))
    };
    view! {
        <label class="field">
            <span class="field-label">{label}{required.then_some(" *")}</span>
            <input type="text" placeholder=placeholder class:invalid=move || problem().is_some()
                data-attr=attr
                prop:value=value
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    card.update(|m| m.set(attr, v));
                }/>
            {move || match problem() {
                Some(p) => view! { <span class="hint bad">{p}</span> }.into_any(),
                None => (!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> }).into_any(),
            }}
        </label>
    }
}

#[component]
fn Choice(
    card: Card,
    attr: &'static str,
    label: &'static str,
    /// (value, label); "" is "not set".
    options: Vec<(String, String)>,
    #[prop(optional)] hint: &'static str,
) -> impl IntoView {
    let value = move || card.model().get(attr);
    let opts = StoredValue::new(options);
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <select on:change=move |ev| {
                let v = event_target_value(&ev);
                card.update(|m| m.set(attr, v));
            }>
                {move || {
                    let cur = value();
                    let mut list = opts.get_value();
                    if !cur.is_empty() && !list.iter().any(|(v, _)| *v == cur) {
                        list.push((cur.clone(), format!("{cur} (not a known value)")));
                    }
                    list.into_iter().map(|(v, l)| {
                        let sel = v == cur;
                        view! { <option value=v selected=sel>{l}</option> }
                    }).collect_view()
                }}
            </select>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </label>
    }
}

fn opts(none: &str, values: &[&str]) -> Vec<(String, String)> {
    std::iter::once((String::new(), none.to_string())).chain(values.iter().map(|v| (v.to_string(), v.to_string()))).collect()
}

/// The fields for one kind of entry.
fn fields(card: Card, tag: &str, monster_ids: Memo<Vec<String>>, doc_dir: StoredValue<String>) -> AnyView {
    match tag {
        "monster" => view! {
            <div class="field-row">
                <Field card attr="id" label="Id" required=true hint="What monsterRef, statblocks and inline <monster> point at"/>
                <Field card attr="displayName" label="Display name" hint="The name prose uses"/>
                <Field card attr="shortName" label="Short name" hint="For repeat mentions"/>
            </div>
            <div class="subgroup">
                <div class="subgroup-title">"A book stat block"</div>
                <div class="field-row">
                    <Field card attr="name" label="Name" placeholder="Goblin Warrior"/>
                    <Choice card attr="cr" label="CR" options=opts("—", CRS)/>
                    <Field card attr="src" label="Source" placeholder="5.2.1 SRD"/>
                </div>
            </div>
            <div class="subgroup">
                <div class="subgroup-title">"…or one from a monster file (its name, CR and source are read from it; any set above override)"</div>
                <div class="field-row">
                    <Field card attr="file" label="File" placeholder="../monsters/goblins.xml" hint="Relative to this supplement"/>
                    <Field card attr="monster" label="Stat block id" hint="The <monster> id in that file"/>
                </div>
            </div>
            <div class="subgroup">
                <div class="subgroup-title">"Template (summoned creatures)"</div>
                <div class="field-row">
                    {move || {
                        let me = card.model().id();
                        let list: Vec<(String, String)> = std::iter::once((String::new(), "— none".to_string()))
                            .chain(monster_ids.get().into_iter().filter(|i| *i != me).map(|i| (i.clone(), i)))
                            .collect();
                        view! { <Choice card attr="summonerRef" label="Summoner" options=list hint="Its formulas are computed from this stat block"/> }
                    }}
                    <Choice card attr="spellLevel" label="Spell level" options=opts("—", &["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])/>
                </div>
                <Chooses card/>
            </div>
        }
        .into_any(),
        "spell" => view! {
            <div class="field-row">
                <Field card attr="id" label="Id" required=true hint="What spellRef and spellBlock point at"/>
                <Field card attr="name" label="Name" placeholder="Sleep"/>
                <Field card attr="src" label="Source" placeholder="5.2.1 SRD"/>
            </div>
            <Field card attr="file" label="Spell file" placeholder="../../shared/internal/spells/sleep.xml" hint="Relative to this supplement; its name and source are read from it"/>
        }
        .into_any(),
        "item" => view! {
            <div class="field-row">
                <Field card attr="id" label="Id" required=true hint="What itemRef, magicItemBlock and treasure items point at"/>
                <Field card attr="name" label="Name" placeholder="Bag of Holding"/>
                <Field card attr="displayName" label="Display name" hint="The name prose uses"/>
            </div>
            <div class="field-row">
                <Field card attr="src" label="Source" placeholder="5.2.1 SRD"/>
                <Field card attr="file" label="Item file" hint="Relative to this supplement; its name and source are read from it"/>
            </div>
        }
        .into_any(),
        "noun" => view! {
            <div class="field-row">
                <Field card attr="id" label="Id" required=true hint="What nounRef points at"/>
                <Field card attr="name" label="Name" required=true hint="The full name"/>
                <Choice card attr="kind" label="Kind" options=opts("—", &["person", "place", "thing"]) hint="For tools; doesn't change how it prints"/>
            </div>
            <div class="field-row">
                <Field card attr="displayName" label="Display name" hint="The name prose uses"/>
                <Field card attr="shortName" label="Short name" hint="For repeat mentions"/>
            </div>
        }
        .into_any(),
        "image" => view! {
            <div class="image-fields">
                <div class="image-form">
                    <div class="field-row">
                        <Field card attr="id" label="Id" required=true hint="Cover art and section images name it by id"/>
                        <Field card attr="src" label="File" required=true hint="Relative to this supplement"/>
                    </div>
                    <div class="field-row">
                        <Field card attr="name" label="Title" hint="The artwork's title, printed by <imageCredits/>"/>
                        <Field card attr="credit" label="Artist" hint="Printed by <imageCredits/>"/>
                    </div>
                    <div class="field-row">
                        <Field card attr="height" label="Height" placeholder="natural" check=height_problem hint="4in, 10cm, or 45% of the text height"/>
                        <Choice card attr="span" label="Span" options=vec![(String::new(), "column (default)".into()), ("column".into(), "column".into()), ("both".into(), "both columns".into())]/>
                        <Choice card attr="placement" label="Placement" options=vec![
                            (String::new(), "here (default)".into()), ("here".into(), "here".into()), ("top".into(), "top of a page".into()),
                            ("bottom".into(), "bottom of a page".into()), ("page".into(), "a page of its own".into())]
                            hint="span=column only"/>
                        <Field card attr="align" label="Align"/>
                    </div>
                </div>
                {move || {
                    let src = card.model().get("src");
                    (!src.is_empty()).then(|| {
                        let file = format!("{}\\{}", doc_dir.get_value(), src.replace('/', "\\"));
                        let h = card.model().get("height");
                        let note = parse_length(&h).map(|(n, u)| format!("{n}{u} tall in print")).unwrap_or_default();
                        view! {
                            <figure class="thumb">
                                <img src=api::convert_file_src(&file) alt="(file not found)"/>
                                <figcaption>{note}</figcaption>
                            </figure>
                        }
                    })
                }}
            </div>
        }
        .into_any(),
        "data" => view! {
            <div class="field-row">
                <Field card attr="id" label="Id" required=true hint="What dataRef points at"/>
                <Field card attr="value" label="Value" hint="Printed by <dataRef value=\"value\">"/>
            </div>
            {move || {
                let m = card.model();
                if m.body_markup {
                    view! {
                        <label class="field">
                            <span class="field-label">"Text"</span>
                            <textarea disabled rows=3 prop:value=m.body.clone()></textarea>
                            <span class="hint">"This text holds markup, so edit it in the source for now."</span>
                        </label>
                    }.into_any()
                } else {
                    view! {
                        <label class="field">
                            <span class="field-label">"Text"</span>
                            <textarea rows=3 prop:value=m.body.clone() on:input=move |ev| {
                                let v = event_target_value(&ev);
                                card.update(|m| m.body = v);
                            }></textarea>
                            <span class="hint">"Printed by <dataRef value=\"body\">; plain text renders best"</span>
                        </label>
                    }.into_any()
                }
            }}
        }
        .into_any(),
        _ => view! { <p class="hint">"No form for this entry."</p> }.into_any(),
    }
}

/// A monster template's `<choose choice option>` picks.
#[component]
fn Chooses(card: Card) -> impl IntoView {
    let rows = move || {
        card.model()
            .chooses
            .into_iter()
            .enumerate()
            .map(|(i, (c, o))| {
                view! {
                    <div class="choose-row">
                        <input type="text" placeholder="choice" prop:value=c on:input=move |ev| {
                            let v = event_target_value(&ev);
                            card.update(|m| m.chooses[i].0 = v);
                        }/>
                        <span>"→"</span>
                        <input type="text" placeholder="option" prop:value=o on:input=move |ev| {
                            let v = event_target_value(&ev);
                            card.update(|m| m.chooses[i].1 = v);
                        }/>
                        <button class="icon-btn danger" title="Remove" on:click=move |_| card.update(|m| {
                            m.chooses.remove(i);
                        })>"✕"</button>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <div class="field">
            <span class="field-label">"Choices"</span>
            <span class="hint">"<choose>: the summoner's pick for one of the template's choices. Unpicked choices tell the DM to choose."</span>
            {rows}
            <button class="small" on:click=move |_| card.update(|m| m.chooses.push((String::new(), String::new())))>"+ Add a pick"</button>
        </div>
    }
}

#[component]
fn RefCard(
    tab: DocTab,
    key: String,
    usage: Memo<HashMap<(String, String), usize>>,
    monster_ids: Memo<Vec<String>>,
    drag: RwSignal<Option<String>>,
    drop_at: RwSignal<Option<(String, bool)>>,
    order: Memo<Vec<String>>,
) -> impl IntoView {
    let key = StoredValue::new(key);
    let base = Memo::new(move |_| {
        let k = key.get_value();
        let d = tab.doc();
        reference::read(&d.source, &d.nodes).into_iter().map(|(_, e)| e).find(|e| ref_key(&e.tag, &e.id()) == k)
    });
    let card = Card { tab, key, base };
    let doc_dir = StoredValue::new(dir_of(&tab.doc_untracked().path).to_string());
    let open = Memo::new(move |_| tab.ref_open.with(|o| o.contains(&key.get_value())));
    let failed = move || tab.ref_failed.with(|f| f.contains(&key.get_value()));
    let busy = move || tab.applying.get();
    let tag = Memo::new(move |_| card.model().tag);
    let uses = move || {
        let b = base.get()?;
        Some(usage.with(|u| u.get(&(b.tag.clone(), b.id())).copied().unwrap_or(0)))
    };
    let renaming = move || base.get().is_some_and(|b| card.model().id() != b.id());
    let el = NodeRef::<html::Div>::new();

    let toggle = move |_: MouseEvent| {
        let k = key.get_value();
        tab.ref_open.update(|o| {
            if !o.remove(&k) {
                o.insert(k);
            }
        });
    };
    // Apply when focus leaves the card for somewhere outside it.
    let on_focusout = move |ev: FocusEvent| {
        let inside = ev
            .related_target()
            .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
            .zip(el.get())
            .is_some_and(|(n, card_el)| card_el.contains(Some(&n)));
        if !inside && card.dirty() && card.model().missing().is_empty() {
            card.apply();
        }
    };
    let on_key = move |ev: KeyboardEvent| {
        if ev.key() == "Escape" {
            card.revert();
        } else if ev.key() == "Enter" && ev.ctrl_key() {
            card.apply();
        }
    };
    let delete = move |ev: MouseEvent| {
        ev.stop_propagation();
        if card.is_new() {
            card.revert();
            return;
        }
        let Some(b) = base.get_untracked() else { return };
        let n = uses().unwrap_or(0);
        let msg = if n > 0 {
            format!("Delete <{} id=\"{}\">? {n} element(s) still point at it, so validation will refuse until they're gone.", b.tag, b.id())
        } else {
            format!("Delete <{} id=\"{}\"> from <reference>?", b.tag, b.id())
        };
        if web_sys::window().and_then(|w| w.confirm_with_message(&msg).ok()).unwrap_or(false) {
            apply_from(tab, Change::Reference { edit: RefEdit::Delete { tag: b.tag.clone(), id: b.id() } }, Some(key.get_value()));
        }
    };

    // Dragging: the header is the handle; dropping on a card's top or bottom half puts
    // the dragged entry before or after it.
    let on_dragstart = move |ev: DragEvent| {
        if card.is_new() {
            ev.prevent_default();
            return;
        }
        if let Some(dt) = ev.data_transfer() {
            let _ = dt.set_data("text/plain", &key.get_value());
            dt.set_effect_allowed("move");
        }
        drag.set(Some(key.get_value()));
    };
    let on_dragend = move |_: DragEvent| {
        drag.set(None);
        drop_at.set(None);
    };
    let on_dragover = move |ev: DragEvent| {
        if drag.get_untracked().is_none() || card.is_new() {
            return;
        }
        ev.prevent_default();
        let after = el.get().is_some_and(|e| {
            let r = e.get_bounding_client_rect();
            (ev.client_y() as f64) > r.top() + r.height() / 2.0
        });
        let k = key.get_value();
        if drop_at.get_untracked() != Some((k.clone(), after)) {
            drop_at.set(Some((k, after)));
        }
    };
    let on_drop = move |ev: DragEvent| {
        ev.prevent_default();
        let (Some(from), Some((target, after))) = (drag.get_untracked(), drop_at.get_untracked()) else { return };
        drag.set(None);
        drop_at.set(None);
        if from == target {
            return;
        }
        // "After the target" is "before the entry after it", or the end.
        let list = order.get_untracked();
        let before = if after {
            let i = list.iter().position(|k| *k == target);
            i.and_then(|i| list.get(i + 1)).cloned()
        } else {
            Some(target)
        };
        if before.as_deref() == Some(from.as_str()) {
            return;
        }
        let split = |k: &str| k.split_once(':').map(|(t, i)| (t.to_string(), i.to_string()));
        let Some((tag, id)) = split(&from) else { return };
        apply_from(tab, Change::Reference { edit: RefEdit::Move { tag, id, before: before.and_then(|b| split(&b)) } }, None);
    };

    let marker = move || {
        drop_at.with(|d| match d {
            Some((k, after)) if *k == key.get_value() => if *after { "drop-after" } else { "drop-before" },
            _ => "",
        })
    };

    view! {
        <div node_ref=el id=move || format!("ref-card-{}", key.get_value())
            class=move || format!("ref-card {}", marker())
            class:open=open class:dirty=move || card.dirty() class:failed=failed class:dragging=move || drag.with(|d| d.as_deref() == Some(key.get_value().as_str()))
            on:focusout=on_focusout on:keydown=on_key on:dragover=on_dragover on:drop=on_drop
        >
            <div class="ref-head" draggable=move || if card.is_new() { "false" } else { "true" }
                on:dragstart=on_dragstart on:dragend=on_dragend on:click=toggle>
                <span class="grip" title="Drag to reorder">"⠿"</span>
                <span class=move || format!("ref-kind kind-{}", tag.get())>{move || tag.get()}</span>
                <code class="ref-id">{move || { let id = card.model().id(); if id.is_empty() { "(no id)".to_string() } else { id } }}</code>
                <span class="ref-label">{move || card.model().label()}</span>
                {move || uses().map(|n| view! {
                    <span class="uses" class:unused=n == 0 title="Elements that point at this entry">
                        {if n == 0 { "unused".to_string() } else { format!("used {n}×") }}
                    </span>
                })}
                {move || card.is_new().then(|| view! { <span class="new-badge">"new"</span> })}
                {move || (card.dirty() && !card.is_new()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                {move || failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                <span class="spacer"></span>
                <button class="icon-btn danger" title="Delete" disabled=busy on:click=delete>"✕"</button>
                <span class="twist">{move || if open.get() { "▾" } else { "▸" }}</span>
            </div>
            <Show when=move || open.get()>
                <div class="ref-body">
                    {move || fields(card, &tag.get(), monster_ids, doc_dir)}
                    {move || {
                        let m = card.model();
                        let known = known_attrs(&m.tag);
                        let extra: Vec<String> = m.attrs.iter().filter(|(k, _)| !known.contains(&k.as_str())).map(|(k, v)| format!("{k}=\"{v}\"")).collect();
                        (!extra.is_empty()).then(|| view! { <p class="hint">"Also kept as written: " <code>{extra.join(" ")}</code></p> })
                    }}
                    <Show when=renaming>
                        <p class="hint warn-text">{move || format!("Apply also renames the {} element(s) that point at \"{}\".", uses().unwrap_or(0), base.get().map(|b| b.id()).unwrap_or_default())}</p>
                    </Show>
                    {move || {
                        let missing = card.model().missing();
                        (!missing.is_empty()).then(|| view! { <p class="hint bad">{format!("Needs: {}. It applies once they're filled in.", missing.join(", "))}</p> })
                    }}
                    <div class="ref-actions">
                        <span class="hint">"Applies when you leave the card · Ctrl+Enter applies · Esc reverts"</span>
                        <span class="spacer"></span>
                        <button on:click=move |_| card.revert() disabled=move || !card.dirty() || busy()>
                            {move || if card.is_new() { "Discard" } else { "Revert" }}
                        </button>
                        <button class="primary" on:click=move |_| card.apply() disabled=move || !card.dirty() || busy()>
                            {move || if busy() { "Validating…" } else { "Apply" }}
                        </button>
                    </div>
                </div>
            </Show>
        </div>
    }
}

#[component]
pub fn ReferenceForm(tab: DocTab) -> impl IntoView {
    let usage = Memo::new(move |_| reference::usage(&tab.doc().source));
    let entries = Memo::new(move |_| {
        let d = tab.doc();
        reference::read(&d.source, &d.nodes).into_iter().map(|(_, e)| e).collect::<Vec<_>>()
    });
    let monster_ids = Memo::new(move |_| entries.get().into_iter().filter(|e| e.tag == "monster").map(|e| e.id()).collect::<Vec<_>>());
    let order = Memo::new(move |_| entries.get().iter().map(|e| ref_key(&e.tag, &e.id())).collect::<Vec<_>>());
    let filter = RwSignal::new(String::new());
    let menu = RwSignal::new(false);
    let drag = RwSignal::new(None::<String>);
    let drop_at = RwSignal::new(None::<(String, bool)>);
    let counter = StoredValue::new(0u32);

    // Scroll to (and open) a card chosen in the structure tree.
    Effect::new(move |_| {
        if let Some(k) = tab.ref_focus.get() {
            tab.ref_open.update(|o| {
                o.insert(k.clone());
            });
            filter.set(String::new());
            request_animation_frame(move || {
                if let Some(e) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(&format!("ref-card-{k}"))) {
                    e.scroll_into_view();
                }
            });
        }
    });

    let add = move |tag: &'static str| {
        menu.set(false);
        let n = counter.get_value() + 1;
        counter.set_value(n);
        let key = format!("new:{n}");
        tab.ref_drafts.update(|d| {
            d.insert(key.clone(), RefEntry::new(tag));
        });
        tab.ref_new.update(|v| v.push(key.clone()));
        tab.ref_open.update(|o| {
            o.insert(key.clone());
        });
        filter.set(String::new());
        request_animation_frame(move || {
            let doc = web_sys::window().and_then(|w| w.document());
            if let Some(card) = doc.and_then(|d| d.get_element_by_id(&format!("ref-card-{key}"))) {
                card.scroll_into_view();
                if let Ok(Some(input)) = card.query_selector("input[data-attr=id]") {
                    if let Ok(h) = input.dyn_into::<web_sys::HtmlElement>() {
                        let _ = h.focus();
                    }
                }
            }
        });
    };

    let counts = move || {
        let es = entries.get();
        KINDS.iter().map(|(k, _)| (*k, es.iter().filter(|e| e.tag == *k).count())).collect::<Vec<_>>()
    };
    let shown = move || {
        let f = filter.get();
        let mut keys: Vec<String> = entries.get().iter().filter(|e| f.is_empty() || e.tag == f).map(|e| ref_key(&e.tag, &e.id())).collect();
        keys.extend(tab.ref_new.get());
        keys
    };

    view! {
        <div class="ref-editor">
            <div class="ref-toolbar">
                <div class="add-wrap">
                    <button class="primary" on:click=move |_| menu.update(|m| *m = !*m)>"+ Add ▾"</button>
                    <Show when=move || menu.get()>
                        <div class="menu-backdrop" on:click=move |_| menu.set(false)></div>
                        <div class="menu add-menu">
                            {KINDS.iter().map(|(k, d)| view! {
                                <button on:click=move |_| add(k)>
                                    <span class=format!("ref-kind kind-{k}")>{*k}</span>
                                    <span class="menu-desc">{*d}</span>
                                </button>
                            }).collect_view()}
                        </div>
                    </Show>
                </div>
                <div class="chips">
                    <button class="chip" class:active=move || filter.get().is_empty() on:click=move |_| filter.set(String::new())>
                        {move || format!("All {}", entries.with(|e| e.len()))}
                    </button>
                    {move || counts().into_iter().filter(|(_, n)| *n > 0).map(|(k, n)| view! {
                        <button class=format!("chip kind-{k}") class:active=move || filter.get() == k on:click=move |_| filter.set(k.to_string())>
                            {format!("{k} {n}")}
                        </button>
                    }).collect_view()}
                </div>
                <span class="spacer"></span>
                <button class="small" on:click=move |_| {
                    let all: std::collections::HashSet<String> = shown().into_iter().collect();
                    tab.ref_open.set(all);
                }>"Expand all"</button>
                <button class="small" on:click=move |_| tab.ref_open.set(Default::default())>"Collapse all"</button>
            </div>
            <Show when=move || !filter.get().is_empty()>
                <p class="hint filter-note">"Showing one kind. Dragging still places an entry among all of them."</p>
            </Show>
            <div class="ref-list" on:dragleave=move |ev: DragEvent| {
                if ev.related_target().is_none() {
                    drop_at.set(None);
                }
            }>
                <For each=shown key=|k| k.clone() let:k>
                    <RefCard tab key=k usage monster_ids drag drop_at order/>
                </For>
                <Show when=move || shown().is_empty()>
                    <p class="hint">"No entries yet. Add one with + Add."</p>
                </Show>
            </div>
        </div>
    }
}
