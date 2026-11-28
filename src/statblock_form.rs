//! A stat block's page: one long page for a `<monster>`, top to bottom as it prints.
//! The stat block's name and ids, its own entry parts (subtitle, habitats, lairs, quote,
//! image, lore, distinction table), its heading, ability scores and attributes, then its
//! traits and action lists, each trait and action a card that opens.
//!
//! Each card edits a draft copy of its element (a [`shared::statblock::El`]) and applies
//! it whole with Apply, Enter (in a one-line field) or Ctrl+Enter; the backend writes
//! only what changed, and only if the whole file validates. Adding, moving and deleting
//! traits and actions apply at once. Prose boxes take inline XML; their Insert menu
//! writes the tags (damage, conditions, refs to this stat block's actions...).

use std::collections::HashSet;

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use shared::family;
use shared::inline;
use shared::statblock::{self as sb, El, StatEdit};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::dialogs::ask;
use crate::family_form::{bar, json, Draft};
use crate::prose_editor::ChipEditor;
use crate::state::DocTab;
use crate::template_form as tf;

pub(crate) fn edit(tab: DocTab, e: StatEdit) {
    apply(tab, Change::Stat { edit: e });
}

fn key(child: usize, path: &[usize], scope: Option<&[String]>) -> String {
    let p: Vec<String> = path.iter().map(|i| i.to_string()).collect();
    format!("sb:{child}:{}:{}", p.join("."), scope.map(|s| s.join(",")).unwrap_or_default())
}

/// The change a card's draft (its key and its element as JSON) would apply, for the Source
/// view's "after Apply" text. The card's own tidy-up (dropping empty containers) isn't
/// run, which only matters to what's written, not to what's shown.
pub fn draft_change(key: &str, text: &str) -> Option<Change> {
    let rest = key.strip_prefix("sb:")?;
    let mut parts = rest.splitn(3, ':');
    let child = parts.next()?.parse::<usize>().ok()?;
    let _path = parts.next()?;
    let scope = parts.next()?;
    let el: El = serde_json::from_str(text).ok()?;
    let scope = (!scope.is_empty()).then(|| scope.split(',').map(String::from).collect());
    Some(Change::Stat { edit: StatEdit::Update { child, el, scope } })
}

/// The draft an edit was made from, so apply can drop it or mark it refused.
pub fn draft_key(e: &StatEdit) -> Option<String> {
    match e {
        StatEdit::Update { child, el, scope } => Some(key(*child, el.path.as_deref().unwrap_or(&[]), scope.as_deref())),
        _ => None,
    }
}

/// Where an element is in a card's El: steps of (child tag, which of those).
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Loc {
    pub(crate) s: [(&'static str, usize); 6],
    pub(crate) n: usize,
}

impl Loc {
    pub(crate) const ROOT: Loc = Loc { s: [("", 0); 6], n: 0 };
    pub(crate) fn at(mut self, tag: &'static str, i: usize) -> Loc {
        self.s[self.n] = (tag, i);
        self.n += 1;
        self
    }
    pub(crate) fn get_mut(self, e: &mut El) -> Option<&mut El> {
        e.at_mut(&self.s[..self.n])
    }
}

/// A card: a draft of one element, applied whole.
#[derive(Clone, Copy)]
pub(crate) struct Card {
    pub(crate) tab: DocTab,
    child: usize,
    pub(crate) d: Draft,
    scope: StoredValue<Option<Vec<String>>>,
    norm: Option<fn(&mut El)>,
    /// The draft (or the file's element), read once per change: the fields read this,
    /// so they update only when their own card changes.
    model: Memo<El>,
    bad: Memo<Option<String>>,
}

/// Prose that isn't well-formed XML yet, anywhere in `m`.
fn bad_prose(m: &El) -> Option<String> {
    let mut stack = vec![("", m)];
    while let Some((parent, e)) = stack.pop() {
        if sb::is_prose(parent, &e.tag) || e.tag == "action" {
            if let Err(err) = inline::parse(&e.text) {
                return Some(format!("<{}>: {}", e.tag, err.message));
            }
        }
        stack.extend(e.kids.iter().map(|k| (e.tag.as_str(), k)));
    }
    None
}

impl Card {
    pub(crate) fn new(tab: DocTab, child: usize, el: Memo<Option<El>>, scope: Option<Vec<String>>, norm: Option<fn(&mut El)>) -> Self {
        let base = Memo::new(move |_| el.with(|e| e.as_ref().map(json).unwrap_or_default()));
        let path = el.with_untracked(|e| e.as_ref().and_then(|e| e.path.clone()).unwrap_or_default());
        let k = key(child, &path, scope.as_deref());
        let d = Draft::new(tab, k, base);
        let model = Memo::new(move |_| d.model::<El>());
        let bad = Memo::new(move |_| model.with(bad_prose));
        Card { tab, child, d, scope: StoredValue::new(scope), norm, model, bad }
    }
    pub(crate) fn m(&self) -> El {
        self.model.get()
    }
    pub(crate) fn m_untracked(&self) -> El {
        self.model.get_untracked()
    }
    pub(crate) fn change(&self, f: impl FnOnce(&mut El)) {
        self.d.change(f)
    }
    pub(crate) fn at(&self, loc: Loc, f: impl FnOnce(&mut El)) {
        self.change(|e| {
            if let Some(x) = loc.get_mut(e) {
                f(x)
            }
        })
    }
    pub(crate) fn busy(&self) -> Signal<bool> {
        let tab = self.tab;
        Signal::derive(move || tab.applying.get())
    }
    /// Prose that isn't well-formed XML yet, anywhere in the draft.
    fn bad(&self) -> Option<String> {
        self.bad.get()
    }
    pub(crate) fn apply(&self) {
        if !self.d.dirty() || self.bad().is_some() {
            return;
        }
        let mut el = self.m();
        if let Some(n) = self.norm {
            n(&mut el);
        }
        edit(self.tab, StatEdit::Update { child: self.child, el, scope: self.scope.get_value() });
    }
}

pub(crate) fn keys(c: Card) -> impl Fn(KeyboardEvent) {
    move |ev: KeyboardEvent| {
        let target = ev.target();
        let in_input = target.as_ref().and_then(|t| t.dyn_ref::<web_sys::HtmlInputElement>()).is_some_and(|i| i.type_() == "text");
        let in_area = target.as_ref().and_then(|t| t.dyn_ref::<web_sys::HtmlTextAreaElement>()).is_some();
        // A chip editor: Enter applies, like a one-line field.
        let in_input = in_input || target.as_ref().and_then(|t| t.dyn_ref::<web_sys::HtmlElement>()).is_some_and(|e| e.is_content_editable());
        if ev.key() == "Escape" {
            c.d.revert()
        } else if ev.key() == "Enter" && (in_input || (in_area && ev.ctrl_key())) {
            ev.prevent_default();
            c.apply()
        }
    }
}

/// The card's body with its state badges, the prose check and Revert / Apply.
pub(crate) fn card_bar(c: Card) -> impl IntoView {
    view! {
        {move || c.bad().map(|e| view! { <p class="hint bad">"Not well-formed XML yet: " {e}</p> })}
        {bar(c.d, c.busy(), move || c.apply())}
    }
}

// ---- fields --------------------------------------------------------------------------

/// A text box for an attribute (`k`) or, with `k` empty, the element's text.
pub(crate) fn text_in(c: Card, loc: Loc, k: &'static str, label: &'static str, ph: &'static str, hint: &'static str) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input type="text" placeholder=ph
                prop:value=move || c.m().at(&loc.s[..loc.n]).map(|e| if k.is_empty() { e.text.clone() } else { e.get(k).to_string() }).unwrap_or_default()
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    c.at(loc, |e| if k.is_empty() { e.text = v } else { e.set(k, &v) })
                }/>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </label>
    }
}

/// A choice for an attribute; `empty` names the blank choice (None: no blank).
pub(crate) fn pick_in(c: Card, loc: Loc, k: &'static str, opts: &'static [&'static str], empty: Option<&'static str>, label: &'static str, hint: &'static str) -> impl IntoView {
    let cur = move || c.m().at(&loc.s[..loc.n]).map(|e| if k.is_empty() { e.text.clone() } else { e.get(k).to_string() }).unwrap_or_default();
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <select prop:value=cur on:change=move |ev| {
                let v = event_target_value(&ev);
                c.at(loc, |e| if k.is_empty() { e.text = v } else { e.set(k, &v) })
            }>
                {empty.map(|t| view! { <option value="">{t}</option> })}
                {move || {
                    // A value the list doesn't know stays choosable.
                    let v = cur();
                    (!v.is_empty() && !opts.contains(&v.as_str())).then(|| view! { <option value=v.clone()>{v.clone()}</option> })
                }}
                {opts.iter().map(|o| view! { <option value=*o>{*o}</option> }).collect_view()}
            </select>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </label>
    }
}

pub(crate) fn flag_in(c: Card, loc: Loc, k: &'static str, default: bool, label: &'static str) -> impl IntoView {
    view! {
        <label class="flag">
            <input type="checkbox" prop:checked=move || c.m().at(&loc.s[..loc.n]).is_some_and(|e| e.flag(k, default))
                on:change=move |ev| {
                    let v = event_target_checked(&ev);
                    c.at(loc, |e| e.set_flag(k, v, default))
                }/>
            {label}
        </label>
    }
}

/// The text of an optional child `tag` of the element at `loc` (empty removes it,
/// unless `keep`).
pub(crate) fn kid_text(c: Card, loc: Loc, tag: &'static str, keep: bool, label: &'static str, ph: &'static str, hint: &'static str) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input type="text" placeholder=ph prop:value=move || c.m().at(&loc.s[..loc.n]).map(|e| e.opt_text(tag)).unwrap_or_default()
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    c.at(loc, |e| e.set_opt_text(tag, &v, keep))
                }/>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </label>
    }
}

/// What a stat block's refs can point at: (tag, id, name).
pub(crate) type RefCtx = Memo<Vec<(String, String, String)>>;

fn ref_ctx(mon: Memo<Option<El>>) -> RefCtx {
    Memo::new(move |_| {
        mon.with(|m| {
            m.as_ref()
                .map(|m| {
                    m.walk()
                        .into_iter()
                        .filter(|e| ["action", "attack", "save", "spellcasting", "spell", "speed", "formula", "choice"].contains(&e.tag.as_str()) && !e.get("id").is_empty())
                        .map(|e| {
                            let name = if e.get("name").is_empty() { e.get("id") } else { e.get("name") };
                            (e.tag.clone(), e.get("id").to_string(), name.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    })
}

/// Prose: a chip editor (see [`crate::prose_editor`]). `tag` names an optional child of
/// the element at `loc` (empty removes it, unless `keep`); None edits the element's own
/// text. The page provides the editor's environment.
#[component]
pub(crate) fn Prose(
    c: Card,
    loc: Loc,
    #[prop(optional)] tag: Option<&'static str>,
    label: &'static str,
    #[prop(optional)] hint: &'static str,
    #[prop(optional)] ph: &'static str,
    #[prop(optional)] keep: bool,
    refs: RefCtx,
    /// What the box holds, for ranking refs (a table column's heading).
    #[prop(optional, into)]
    context: Signal<String>,
) -> impl IntoView {
    let _ = refs;
    let get = move || {
        c.m().at(&loc.s[..loc.n]).map(|e| match tag {
            Some(t) => e.opt_text(t),
            None => e.text.clone(),
        }).unwrap_or_default()
    };
    let put = move |v: String| {
        c.at(loc, |e| match tag {
            Some(t) => e.set_opt_text(t, &v, keep),
            None => e.text = v,
        })
    };
    view! {
        <div class="field prose-field">
            <span class="field-label">{label}</span>
            <ChipEditor value=Signal::derive(get) set=Callback::new(put) placeholder=ph hint=context/>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </div>
    }
}

/// Rows of `tag` children in the `container` child of the element at `loc` (or, with
/// `container` empty, of the element itself): each row's fields, reorder, delete, add.
#[component]
pub(crate) fn Rows(
    c: Card,
    loc: Loc,
    #[prop(optional)] container: &'static str,
    tag: &'static str,
    title: &'static str,
    #[prop(optional)] hint: &'static str,
    make: fn(&El) -> El,
    row: fn(Card, Loc, usize) -> AnyView,
    #[prop(optional)] min: usize,
) -> impl IntoView {
    let list_loc = move || if container.is_empty() { loc } else { loc.at(container, 0) };
    let count = Memo::new(move |_| c.m().at(&list_loc().s[..list_loc().n]).map(|e| e.all(tag).len()).unwrap_or(0));
    let with_list = move |f: &dyn Fn(&mut Vec<El>)| {
        c.at(loc, |e| {
            let host = if container.is_empty() { e } else { e.ensure(container) };
            // The tag's rows as a run of children, kept where they are among the others.
            let mut rows: Vec<El> = host.kids.iter().filter(|k| k.tag == tag).cloned().collect();
            f(&mut rows);
            let first = host.kids.iter().position(|k| k.tag == tag);
            host.kids.retain(|k| k.tag != tag);
            match first {
                Some(i) => {
                    let i = i.min(host.kids.len());
                    host.kids.splice(i..i, rows);
                }
                None => {
                    for r in rows {
                        host.insert_kid(r);
                    }
                }
            }
        })
    };
    view! {
        <div class="sb-rows">
            <div class="sb-rows-title">{title} {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}</div>
            {move || (0..count.get()).map(|i| {
                let n = count.get_untracked();
                view! {
                    <div class="sb-row">
                        <div class="sb-row-fields">{row(c, list_loc().at(tag, i), i)}</div>
                        <div class="sb-row-tools">
                            <button type="button" class="icon-btn" title="Move up" disabled={i == 0} on:click=move |_| with_list(&|r| if i > 0 && i < r.len() { r.swap(i, i - 1) })>"↑"</button>
                            <button type="button" class="icon-btn" title="Move down" disabled={i + 1 >= n} on:click=move |_| with_list(&|r| if i + 1 < r.len() { r.swap(i, i + 1) })>"↓"</button>
                            <button type="button" class="icon-btn danger" title="Remove" disabled={n <= min} on:click=move |_| with_list(&|r| if i < r.len() { r.remove(i); })>"✕"</button>
                        </div>
                    </div>
                }
            }).collect_view()}
            <button type="button" class="small" on:click=move |_| {
                let m = c.m();
                let new = make(&m);
                with_list(&|r| r.push(new.clone()))
            }>"+ Add"</button>
        </div>
    }
}

// ---- the stat block's own cards -------------------------------------------------------

/// The monster El cut down to the children with these tags.
pub(crate) fn scoped(m: &El, tags: &[&str]) -> El {
    let mut e = m.clone();
    e.kids.retain(|k| tags.contains(&k.tag.as_str()));
    e
}

pub(crate) fn scope(tags: &[&str]) -> Option<Vec<String>> {
    Some(tags.iter().map(|t| t.to_string()).collect())
}

#[component]
pub(crate) fn Section(#[prop(into)] title: String, #[prop(optional, into)] hint: String, focus: Signal<bool>, children: Children) -> impl IntoView {
    let el = NodeRef::<html::Details>::new();
    Effect::new(move |_| {
        if focus.get() {
            if let Some(d) = el.get() {
                d.set_open(true);
                d.scroll_into_view();
            }
        }
    });
    view! {
        <details node_ref=el class="sb-section" open=true>
            <summary class="sb-section-title">{title} {(!hint.is_empty()).then(|| view! { <span class="hint">{hint.clone()}</span> })}</summary>
            {children()}
        </details>
    }
}

const IDENTITY: &[&str] = &["name"];

/// A template's challenge ratings: "none" too (it has no XP of its own).
const CRS_TPL: &[&str] = &[
    "none", "0", "1/8", "1/4", "1/2", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "20",
    "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
];

#[component]
fn IdentityCard(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().map(|m| scoped(m, IDENTITY))));
    let c = Card::new(tab, child, el, scope(IDENTITY), None);
    let r = Loc::ROOT;
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="field-row">
                {kid_text(c, r, "name", true, "Name", "Gnoll Pack Lord", "<name>: the stat block's heading")}
                {text_in(c, r, "id", "Id", "gnoll-pack-lord", "Unique in the file. <reference><monster monster=…> in a supplement names it: renaming it here doesn't rename those.")}
                {text_in(c, r, "src", "Source", "(original)", "src: the source book, e.g. 5.2.1 SRD. Empty for an original monster.")}
            </div>
            <div class="field-row">
                {flag_in(c, r, "template", false, "Template")}
                <span class="hint">"A summoned creature's stat block whose numbers come from the summoner: formulas and choices (see monsters/TEMPLATES.md)."</span>
            </div>
            {card_bar(c)}
        </div>
    }
}

const ENTRY: &[&str] = &["brief", "habitats", "lairRef"];

fn norm_entry(e: &mut El) {
    if e.kid("brief").is_some_and(|b| b.text.trim().is_empty() && b.get("id").is_empty()) {
        e.remove_kid("brief");
    }
    if e.kid("habitats").is_some_and(|h| h.kids.iter().all(|k| k.get("name").trim().is_empty()) && h.get("id").is_empty()) {
        e.remove_kid("habitats");
    }
    if let Some(h) = e.kid_mut("habitats") {
        h.kids.retain(|k| !k.get("name").trim().is_empty());
    }
}

fn name_rows(c: Card, tag: &'static str, ph: &'static str) -> impl IntoView {
    let names = Memo::new(move |_| c.m().kid("habitats").map(|h| h.all(tag).iter().map(|k| k.get("name").to_string()).collect::<Vec<_>>()).unwrap_or_default());
    let input = RwSignal::new(String::new());
    let add = move || {
        let v = input.get_untracked();
        if v.trim().is_empty() {
            return;
        }
        c.change(|e| {
            e.ensure("habitats").insert_kid(El::new(tag).with("name", v.trim()));
        });
        input.set(String::new());
    };
    view! {
        <div class="chips">
            {move || names.get().into_iter().enumerate().map(|(i, n)| view! {
                <span class="chip">{n}
                    <button type="button" class="chip-x" title="Remove" on:click=move |_| c.change(|e| {
                        if let Some(h) = e.kid_mut("habitats") {
                            let mut k = 0;
                            h.kids.retain(|x| {
                                if x.tag != tag { return true; }
                                k += 1;
                                k - 1 != i
                            });
                        }
                    })>"✕"</button>
                </span>
            }).collect_view()}
            <input type="text" class="chip-input" placeholder=ph prop:value=move || input.get()
                on:input=move |ev| input.set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| if ev.key() == "Enter" { ev.prevent_default(); ev.stop_propagation(); add() }/>
            <button type="button" class="small" on:click=move |_| add()>"+ Add"</button>
        </div>
    }
}

/// The stat block's subtitle, habitat line and lairs.
#[component]
fn EntryCard(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().map(|m| scoped(m, ENTRY))));
    let c = Card::new(tab, child, el, scope(ENTRY), Some(norm_entry));
    let lairs = Memo::new(move |_| {
        family::read(&tab.doc().source)
            .map(|f| f.parts.into_iter().filter(|p| p.tag == "lair" && !p.id.is_empty()).map(|p| (p.id, if p.name.is_empty() { p.summary } else { p.name })).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    let named = move || c.m().all("lairRef").iter().map(|l| l.get("ref").to_string()).collect::<Vec<_>>();
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="table-card-title">"Entry" <span class="hint">"Printed only in a full entry (the file on its own, or a supplement's <monsterEntry>), never with a plain stat block."</span></div>
            <div class="field-row">
                <label class="field grow">
                    <span class="field-label">"Subtitle"</span>
                    <input type="text" placeholder="(none)" prop:value=move || c.m().opt_text("brief")
                        on:input=move |ev| { let v = event_target_value(&ev); c.change(|e| e.ensure("brief").text = v) }/>
                    <span class="hint">"<brief>: this monster's own subtitle. Empty (and no id) leaves it out."</span>
                </label>
                <label class="field id-field">
                    <span class="field-label">"Its id"</span>
                    <input type="text" placeholder="(none)" prop:value=move || c.m().kid("brief").map(|b| b.get("id").to_string()).unwrap_or_default()
                        on:input=move |ev| { let v = event_target_value(&ev); c.change(|e| e.ensure("brief").set("id", &v)) }/>
                </label>
            </div>
            <div class="field-row">
                <div class="field grow"><span class="field-label">"Habitats"</span>{name_rows(c, "habitat", "Hill")}</div>
                <div class="field grow"><span class="field-label">"Treasure"</span>{name_rows(c, "treasure", "Armaments")}</div>
                <label class="field id-field">
                    <span class="field-label">"Its id"</span>
                    <input type="text" placeholder="(none)" prop:value=move || c.m().kid("habitats").map(|b| b.get("id").to_string()).unwrap_or_default()
                        on:input=move |ev| { let v = event_target_value(&ev); c.change(|e| e.ensure("habitats").set("id", &v)) }/>
                </label>
            </div>
            <div class="field">
                <span class="field-label">"Lairs"</span>
                {move || if lairs.with(|l| l.is_empty()) {
                    view! { <span class="hint">"The file has no lairs with ids. Add one on the family's page."</span> }.into_any()
                } else {
                    view! {
                        <div class="flags">
                            {lairs.get().into_iter().map(|(id, name)| {
                                let i2 = id.clone();
                                view! {
                                    <label class="flag">
                                        <input type="checkbox" prop:checked={ let i = id.clone(); move || named().contains(&i) }
                                            disabled={ let i = id.clone(); move || !named().contains(&i) && named().len() >= 3 }
                                            on:change=move |ev| {
                                                let on = event_target_checked(&ev);
                                                let i = i2.clone();
                                                c.change(|e| if on { e.insert_kid(El::new("lairRef").with("ref", &i)); } else { e.kids.retain(|k| !(k.tag == "lairRef" && k.get("ref") == i)) })
                                            }/>
                                        {format!("{name} ({id})")}
                                    </label>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
                <span class="hint">"<lairRef>: up to three. A lair a stat block names prints with it, once."</span>
            </div>
            {card_bar(c)}
        </div>
    }
}

fn p_row(c: Card, loc: Loc, _i: usize) -> AnyView {
    let get = Signal::derive(move || c.m().at(&loc.s[..loc.n]).map(|e| e.text.clone()).unwrap_or_default());
    let put = Callback::new(move |v: String| c.at(loc, |e| e.text = v));
    view! { <ChipEditor value=get set=put placeholder="(empty paragraph)"/> }.into_any()
}

/// One table cell: a one-line chip box.
fn td_row(c: Card, l: Loc, _i: usize) -> AnyView {
    let get = Signal::derive(move || c.m().at(&l.s[..l.n]).map(|e| e.text.clone()).unwrap_or_default());
    let put = Callback::new(move |v: String| c.at(l, |e| e.text = v));
    view! { <ChipEditor value=get set=put single=true/> }.into_any()
}

fn new_p(_: &El) -> El {
    El::new("p")
}

/// One of the stat block's own parts (quote box, image, description, distinction table).
#[component]
fn OwnPart(tab: DocTab, child: usize, mon: Memo<Option<El>>, tag: &'static str, refs: RefCtx) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid(tag).cloned())));
    let c = Card::new(tab, child, el, None, None);
    let r = Loc::ROOT;
    let label = family::part_label(tag);
    let delete = move |_: MouseEvent| {
        let Some(p) = el.get_untracked().and_then(|e| e.path) else { return };
        ask(format!("Delete this stat block's {}?", label.to_lowercase()), move || {
            edit(tab, StatEdit::Delete { child, path: p, tag: tag.into() })
        });
    };
    let body = match tag {
        "quoteBox" => view! {
            <Rows c loc=r container="quote" tag="p" title="Quote" min=1 make=new_p row=p_row/>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Attribution"</span>
                    <input type="text" placeholder="(none)" prop:value=move || c.m().kid("attribution").map(|a| a.get("name").to_string()).unwrap_or_default()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            c.change(|e| if v.trim().is_empty() { e.remove_kid("attribution") } else { e.ensure("attribution").set("name", &v) })
                        }/>
                </label>
                {text_in(c, r, "id", "Id", "(none)", "")}
            </div>
        }.into_any(),
        "image" => view! {
            <div class="field-row">
                {text_in(c, r, "src", "File", "art/gnoll.png", "relative to this file")}
                {text_in(c, r, "alt", "Alt text", "(none)", "")}
                {text_in(c, r, "id", "Id", "(none)", "")}
            </div>
            <div class="field-row">
                {text_in(c, r, "width", "Width", "(auto)", "")}
                {text_in(c, r, "height", "Height", "(auto)", "")}
                {text_in(c, r, "name", "Title", "(none)", "for <imageCredits/>")}
                {text_in(c, r, "credit", "Artist", "(none)", "for <imageCredits/>")}
            </div>
            <Prose c loc=r tag="cutline" label="Cutline" hint="The caption. Empty leaves it out." refs/>
        }.into_any(),
        "description" => view! {
            <Rows c loc=r tag="p" title="Paragraphs" min=1 make=new_p row=p_row/>
            <div class="field-row">{text_in(c, r, "id", "Id", "(none)", "")}</div>
        }.into_any(),
        _ => view! {
            <Rows c loc=r container="header" tag="dh" title="Headings" hint="The first is the die (1d6)." min=1
                make=|_| El::new("dh")
                row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "", "Heading", "", "")}</div> }.into_any()/>
            <Rows c loc=r container="body" tag="tr" title="Rows" hint="One cell per heading."
                min=1 make=|m: &El| {
                    let n = m.at(&[("body", 0)]).map(|b| b.all("tr").len()).unwrap_or(0);
                    let cols = m.at(&[("header", 0)]).map(|h| h.all("dh").len()).unwrap_or(2).max(1);
                    let mut tr = El::new("tr").with_kid(El::new("td").with_text(&(n + 1).to_string()));
                    for _ in 1..cols { tr.kids.push(El::new("td")); }
                    tr
                }
                row=|c, l, _| view! { <Rows c loc=l tag="td" title="Cells" min=1 make=|_| El::new("td") row=td_row/> }.into_any()/>
            <div class="field-row">{text_in(c, r, "id", "Id", "(none)", "")}</div>
        }.into_any(),
    };
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="table-card-title">
                {label} <span class="hint">"This stat block's own; prints only in a full entry."</span>
                <span class="spacer"></span>
                <button class="small danger" disabled=move || tab.applying.get() on:click=delete>{format!("Delete {}", label.to_lowercase())}</button>
            </div>
            {body}
            {card_bar(c)}
        </div>
    }
}

fn new_part(tag: &str) -> El {
    match tag {
        "quoteBox" => El::new("quoteBox").with_kid(El::new("quote").with_kid(El::new("p"))),
        "image" => El { attrs: vec![("src".into(), String::new())], ..El::new("image") },
        "description" => El::new("description").with_kid(El::new("p")),
        _ => {
            let mut body = El::new("body");
            for i in 1..=4 {
                body.kids.push(El::new("tr").with_kid(El::new("td").with_text(&i.to_string())).with_kid(El::new("td")));
            }
            El::new("distinctionTable").with_kid(El::new("header").with_kid(El::new("dh").with_text("1d4")).with_kid(El::new("dh"))).with_kid(body)
        }
    }
}

const OWN_PARTS: &[&str] = &["quoteBox", "image", "description", "distinctionTable"];

// ---- heading, abilities, attributes ---------------------------------------------------

fn size_row(c: Card, l: Loc, _i: usize) -> AnyView {
    let swarm = move || c.m().at(&l.s[..l.n]).is_some_and(|s| s.kid("swarm").is_some());
    view! {
        <div class="field-row">
            <label class="flag">
                <input type="checkbox" prop:checked=swarm on:change=move |ev| {
                    let on = event_target_checked(&ev);
                    c.at(l, |s| {
                        // A swarm and a plain size are different elements: start the size afresh.
                        let mut n = El::new("size");
                        if on {
                            n.kids.push(El::new("swarm").with_kid(El::new("swarmSize").with_text("Medium")).with_kid(El::new("memberSize").with_text("Tiny")));
                        } else {
                            n.text = "Medium".into();
                        }
                        *s = n;
                    })
                }/>
                "Swarm"
            </label>
            {move || if swarm() {
                let sw = l.at("swarm", 0);
                view! {
                    {pick_in(c, sw.at("swarmSize", 0), "", sb::SIZES, None, "Swarm size", "")}
                    {text_in(c, sw, "conjunctiveWord", "Joined by", "or", "or / and / to")}
                    <Rows c loc=sw tag="memberSize" title="Of creatures sized" min=1 make=|_| El::new("memberSize").with_text("Tiny")
                        row=|c, l, _| pick_in(c, l, "", sb::SIZES, None, "Size", "").into_any()/>
                }.into_any()
            } else {
                pick_in(c, l, "", sb::SIZES, None, "Size", "").into_any()
            }}
        </div>
    }
    .into_any()
}

fn speed_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {pick_in(c, l, "name", sb::SPEEDS, None, "Kind", "")}
            {text_in(c, l, "value", "Feet", "30", "")}
            {text_in(c, l, "id", "Id", "ground", "for <speedRef>")}
            {text_in(c, l, "note", "Note", "(none)", "")}
            {flag_in(c, l, "hover", false, "Hover")}
            {tf::formula(c, l, "valueFormula", "Feet formula")}
        </div>
    }
    .into_any()
}

fn new_speed(m: &El) -> El {
    let taken: Vec<String> = m.walk().iter().filter(|e| e.tag == "speed").map(|e| e.get("id").to_string()).collect();
    let name = sb::SPEEDS.iter().find(|s| !taken.iter().any(|t| t == *s)).copied().unwrap_or("ground");
    El::new("speed").with("id", &sb::fresh(name, &taken)).with("name", name).with("value", "30")
}

#[component]
fn HeadingCard(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid("heading").cloned())));
    let c = Card::new(tab, child, el, None, None);
    let r = Loc::ROOT;
    let ini = r.at("initiativeBonus", 0);
    let hd = r.at("hitDice", 0);
    let has_hd = move || c.m().kid("hitDice").is_some();
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <Rows c loc=r container="sizes" tag="size" title="Sizes" hint="Joined with \u{201c}or\u{201d}; the first sets the hit die." min=1
                make=|_| El::new("size").with_text("Medium") row=size_row/>
            <div class="field-row">
                {kid_text(c, r, "type", true, "Type", "Humanoid", "\u{201c}Dragon (Metallic)\u{201d}")}
                {kid_text(c, r, "alignment", true, "Alignment", "Neutral", "")}
                {kid_text(c, r, "armorClass", true, "Armor Class", "10", "")}
                {kid_text(c, r, "proficiencyBonus", true, "Proficiency Bonus", "2", "")}
            </div>
            <div class="field-row">
                {tf::kid_formula(c, "armorClass", "AC formula")}
                {tf::kid_formula(c, "proficiencyBonus", "PB formula")}
                {tf::formula(c, ini, "formula", "Initiative formula")}
                {tf::kid_formula(c, "hitDice", "Hit dice formula")}
                {tf::kid_formula(c, "hitPoints", "HP formula")}
            </div>
            <div class="field-row">
                {text_in(c, ini, "mod", "Initiative mod", "(none)", "Added to Dex mod + PB")}
                <div class="field flags">
                    {flag_in(c, ini, "expertise", false, "Expertise (PB twice)")}
                    {flag_in(c, ini, "advantage", false, "Advantage")}
                </div>
                {kid_text(c, r, "hitDice", false, "Hit dice", "(none)", "How many; the die comes from the size. Empty leaves it out.")}
                {move || has_hd().then(|| text_in(c, hd, "flatBonus", "Flat HP bonus", "(Con mod × dice)", ""))}
                {kid_text(c, r, "hitPoints", false, "Hit points", "(from the dice)", "Replaces the dice's average. One of the two is needed.")}
            </div>
            <Rows c loc=r container="speeds" tag="speed" title="Speeds" min=1 make=new_speed row=speed_row/>
            {card_bar(c)}
        </div>
    }
}

#[component]
fn AbilitiesCard(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid("statBlock").cloned())));
    let c = Card::new(tab, child, el, None, None);
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <table class="sb-stats">
                <thead><tr><th></th><th>"Score"</th><th>"Save proficiency"</th><th>"Expertise"</th>
                    {move || tf::is_template().then(|| view! { <th>"Score formula"</th><th>"Explained by"</th> })}</tr></thead>
                <tbody>
                    {(0..6).map(|i| {
                        let l = Loc::ROOT.at("stat", i);
                        view! {
                            <tr>
                                <th>{move || c.m().at(&l.s[..l.n]).map(|s| s.get("name").to_uppercase()).unwrap_or_default()}</th>
                                <td><input type="text" class="num" prop:value=move || c.m().at(&l.s[..l.n]).map(|s| s.get("value").to_string()).unwrap_or_default()
                                    on:input=move |ev| { let v = event_target_value(&ev); c.at(l, |s| s.set("value", &v)) }/></td>
                                <td>{flag_in(c, l, "saveProficiency", false, "")}</td>
                                <td>{flag_in(c, l, "expertise", false, "")}</td>
                                {move || tf::is_template().then(|| view! { <td>{tf::formula(c, l, "valueFormula", "")}</td><td>{tf::trait_ref(c, l)}</td> })}
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
            {card_bar(c)}
        </div>
    }
}

const DEFENSES: &[&str] = &[
    "acid", "bludgeoning", "cold", "fire", "force", "lightning", "necrotic", "piercing", "poison", "psychic", "radiant", "slashing",
    "thunder", "blinded", "charmed", "deafened", "exhaustion", "frightened", "grappled", "incapacitated", "invisible", "paralyzed",
    "petrified", "poisoned", "prone", "restrained", "stunned", "unconscious",
];

fn norm_attributes(e: &mut El) {
    for t in ["skills", "vulnerabilities", "resistances", "immunities", "gear", "senses", "languages"] {
        if e.kid(t).is_some_and(|k| k.kids.is_empty()) {
            e.remove_kid(t);
        }
    }
}

fn skill_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {pick_in(c, l, "name", sb::SKILLS, None, "Skill", "")}
            {pick_in(c, l, "stat", sb::STATS, Some("(its usual)"), "Ability", "")}
            {text_in(c, l, "skillMod", "Mod", "(none)", "")}
            <div class="field flags">{flag_in(c, l, "proficiency", false, "Proficient")}{flag_in(c, l, "expertise", false, "Expertise")}</div>
        </div>
    }
    .into_any()
}

fn defense_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{pick_in(c, l, "name", sb::DAMAGE_TYPES, None, "Damage type", "")}{text_in(c, l, "note", "Note", "(none)", "")}</div> }.into_any()
}

#[component]
fn AttributesCard(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid("attributes").cloned())));
    let c = Card::new(tab, child, el, None, Some(norm_attributes));
    let r = Loc::ROOT;
    let lang = r.at("languages", 0);
    let tele = lang.at("telepathy", 0);
    let has_tele = move || c.m().kid("languages").is_some_and(|l| l.kid("telepathy").is_some());
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <Rows c loc=r container="challengeRatings" tag="challengeRating" title="Challenge rating" hint="A second one with a note: \u{201c}11 in lair\u{201d}." min=1
                make=|_| El::new("challengeRating").with("value", "1")
                row=|c, l, _| view! { <div class="field-row">{move || pick_in(c, l, "value", if tf::is_template() { CRS_TPL } else { sb::CRS }, None, "CR", "")}{text_in(c, l, "note", "Note", "(base)", "")}</div> }.into_any()/>
            <Rows c loc=r container="skills" tag="skill" title="Skills"
                make=|_| El::new("skill").with("name", "perception").with("stat", "wis").with("proficiency", "true") row=skill_row/>
            <Rows c loc=r container="senses" tag="sense" title="Senses" hint="Passive Perception is computed."
                make=|_| El::new("sense").with("name", "darkvision").with("range", "60")
                row=|c, l, _| view! { <div class="field-row">{pick_in(c, l, "name", sb::SENSES, None, "Sense", "")}{text_in(c, l, "range", "Feet", "60", "")}{text_in(c, l, "note", "Note", "(none)", "")}{tf::formula(c, l, "rangeFormula", "Feet formula")}</div> }.into_any()/>
            <Rows c loc=r container="vulnerabilities" tag="vulnerability" title="Vulnerabilities" make=|_| El::new("vulnerability").with("name", "fire") row=defense_row/>
            <Rows c loc=r container="resistances" tag="resistance" title="Resistances" make=|_| El::new("resistance").with("name", "fire") row=defense_row/>
            <Rows c loc=r container="immunities" tag="immunity" title="Immunities" hint="Damage types and conditions."
                make=|_| El::new("immunity").with("name", "poison")
                row=|c, l, _| view! { <div class="field-row">{pick_in(c, l, "name", DEFENSES, None, "Damage type or condition", "")}{text_in(c, l, "note", "Note", "(none)", "")}</div> }.into_any()/>
            <Rows c loc=r container="gear" tag="item" title="Gear" make=|_| El::new("item").with("name", "Dagger")
                row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "name", "Item", "Dagger", "")}{text_in(c, l, "quantity", "Quantity", "1", "")}</div> }.into_any()/>
            <Rows c loc=r container="languages" tag="language" title="Languages" make=|_| El::new("language").with("name", "Common")
                row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "name", "Language", "Common", "")}</div> }.into_any()/>
            <div class="field-row">
                <label class="flag">
                    <input type="checkbox" prop:checked=has_tele on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        c.change(|e| {
                            let l = e.ensure("languages");
                            if on { l.ensure("telepathy").set("range", "120") } else { l.remove_kid("telepathy") }
                        })
                    }/>
                    "Telepathy"
                </label>
                {move || has_tele().then(|| view! { {text_in(c, tele, "range", "Feet", "120", "")}{text_in(c, tele, "note", "Note", "(none)", "")} })}
            </div>
            {card_bar(c)}
        </div>
    }
}

// ---- traits and actions ---------------------------------------------------------------

fn uses_rows(c: Card, l: Loc) -> impl IntoView {
    view! {
        <Rows c loc=l tag="uses" title="Uses" hint="\u{201c}(3/Day, or 4/Day in Lair)\u{201d}: one per variant."
            make=|_| El::new("uses").with("number", "1").with("frequency", "day")
            row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "number", "Number", "1", "")}{text_in(c, l, "frequency", "Per", "day", "")}{text_in(c, l, "note", "Note", "(none)", "")}{tf::formula(c, l, "numberFormula", "Number formula")}</div> }.into_any()/>
    }
}

/// A save's fields (an action's, or a trait's with no id or name).
fn save_fields(c: Card, l: Loc, refs: RefCtx) -> impl IntoView {
    let outcome = move |tag: &'static str, label: &'static str| {
        let has = move || c.m().at(&l.s[..l.n]).is_some_and(|s| s.kid(tag).is_some());
        view! {
            <Prose c loc=l tag label refs/>
            {move || has().then(|| view! { <div class="field-row">{text_in(c, l.at(tag, 0), "pre", "Label", "(default)", "Instead of \u{201c}Failure:\u{201d} and so on")}</div> })}
        }
    };
    view! {
        <div class="field-row">
            {pick_in(c, l, "saveType", sb::STATS, None, "Target saves with", "")}
            {pick_in(c, l, "saveStat", sb::STATS, None, "DC from", "8 + PB + this mod")}
            {text_in(c, l, "saveMod", "DC adjustment", "(none)", "")}
            {move || (c.m().at(&l.s[..l.n]).is_some_and(|s| s.tag == "save")).then(|| tf::formula(c, l, "dcFormula", "DC formula"))}
        </div>
        <Prose c loc=l tag="targets" keep=true label="Targets" hint="\u{201c}each creature in a 15-foot Cone\u{201d}" refs/>
        {outcome("failure", "Failure")}
        {outcome("success", "Success")}
        {outcome("failureOrSuccess", "Failure or Success")}
    }
}

#[component]
fn TraitCard(tab: DocTab, child: usize, mon: Memo<Option<El>>, path: Vec<usize>, fresh: bool, refs: RefCtx, focus: Memo<Vec<usize>>) -> impl IntoView {
    let p2 = path.clone();
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| find(m, &p2))));
    let c = Card::new(tab, child, el, None, None);
    let r = Loc::ROOT;
    let has_save = move || c.m().kid("save").is_some();
    let summary = move || el.with(|e| e.as_ref().map(|e| e.get("name").to_string()).unwrap_or_default());
    view! {
        <ItemShell tab child mon path=path.clone() fresh focus kind="Trait" summary=Signal::derive(summary) dirty=Signal::derive(move || c.d.dirty())>
            <div class="sb-item-body" on:keydown=keys(c)>
                <div class="field-row">
                    {text_in(c, r, "name", "Name", "Keen Smell", "")}
                    {text_in(c, r, "shortName", "Short name", "(none)", "")}
                    {text_in(c, r, "id", "Id", "(none)", "")}
                </div>
                {tf::variant(c, r)}
                {uses_rows(c, r)}
                <Prose c loc=r tag="description" label="Description" refs/>
                <label class="flag">
                    <input type="checkbox" prop:checked=has_save on:change=move |ev| {
                        let on = event_target_checked(&ev);
                        c.change(|e| if on {
                            e.insert_kid(El::new("save").with("saveType", "dex").with("saveStat", "con").with_kid(El::new("targets")));
                        } else {
                            e.remove_kid("save")
                        })
                    }/>
                    "Forces a saving throw"
                </label>
                {move || has_save().then(|| save_fields(c, r.at("save", 0), refs))}
                {card_bar(c)}
            </div>
        </ItemShell>
    }
}

fn find(m: &El, path: &[usize]) -> Option<El> {
    m.walk().into_iter().find(|e| e.path.as_deref() == Some(path)).cloned()
}

/// A trait's or an action's card: its line (kind, name, move, delete) and its form.
#[component]
fn ItemShell(
    tab: DocTab,
    child: usize,
    mon: Memo<Option<El>>,
    path: Vec<usize>,
    fresh: bool,
    focus: Memo<Vec<usize>>,
    kind: &'static str,
    summary: Signal<String>,
    dirty: Signal<bool>,
    children: Children,
) -> impl IntoView {
    let (up, last) = path.split_at(path.len() - 1);
    let (up, me) = (up.to_vec(), last[0]);
    let p = StoredValue::new(path.clone());
    let up = StoredValue::new(up);
    // The siblings of its kind (child indexes), from the container.
    let sibs = Memo::new(move |_| {
        mon.with(|m| {
            let m = m.as_ref()?;
            let host = find(m, &up.get_value()).or_else(|| up.get_value().is_empty().then(|| m.clone()))?;
            let mine = host.kids.iter().find(|k| k.path.as_deref() == Some(&p.get_value()[..]))?.tag.clone();
            let r = sb::rank(&host.tag, &mine);
            Some((host.tag.clone(), host.kids.iter().filter(|k| sb::rank(&host.tag, &k.tag) == r).filter_map(|k| k.path.as_ref()?.last().copied()).collect::<Vec<_>>(), host.kids.len()))
        })
    });
    let pos = move || sibs.with(|s| s.as_ref().and_then(|(_, l, _)| l.iter().position(|x| *x == me)));
    let count = move || sibs.with(|s| s.as_ref().map_or(0, |(_, l, _)| l.len()));
    let busy = move || tab.applying.get();
    let tag_now = move || mon.with(|m| m.as_ref().and_then(|m| find(m, &p.get_value())).map(|e| e.tag).unwrap_or_default());
    let mv = move |before: Option<usize>| edit(tab, StatEdit::Move { child, path: p.get_value(), tag: tag_now(), before });
    let up_btn = move |ev: MouseEvent| {
        ev.prevent_default();
        if let Some(i) = pos().filter(|i| *i > 0) {
            let b = sibs.with_untracked(|s| s.as_ref().map(|(_, l, _)| l[i - 1]));
            mv(b);
        }
    };
    let down_btn = move |ev: MouseEvent| {
        ev.prevent_default();
        if let Some(i) = pos() {
            let b = sibs.with_untracked(|s| s.as_ref().and_then(|(_, l, _)| l.get(i + 2).copied()));
            mv(b);
        }
    };
    let delete = move |ev: MouseEvent| {
        ev.prevent_default();
        let tag = tag_now();
        let path = p.get_value();
        let id = mon.with_untracked(|m| m.as_ref().and_then(|m| find(m, &path)).map(|e| e.get("id").to_string()).unwrap_or_default());
        let refs = if id.is_empty() { 0 } else { sb::refs_to(&tab.doc_untracked().source, child, &tag, &id) };
        let warn = if refs > 0 { format!("\n\n{refs} ref(s) in this stat block point at \u{201c}{id}\u{201d}: the file won't validate until they go.") } else { String::new() };
        // The last item of a list that needs one: the list goes.
        let only = count() <= 1;
        let host = sibs.with_untracked(|s| s.as_ref().map(|(t, _, n)| (t.clone(), *n)));
        let whole = match &host {
            Some((t, n)) => only && (t == "traits" || t == "reactions" || t == "legendaryActions" || *n == 1),
            None => false,
        };
        let e = if whole {
            let t = host.map(|h| h.0).unwrap_or_default();
            StatEdit::Delete { child, path: up.get_value(), tag: t }
        } else {
            StatEdit::Delete { child, path, tag }
        };
        ask(format!("Delete the {} \u{201c}{}\u{201d}?{warn}", kind.to_lowercase(), summary.get_untracked()), move || edit(tab, e));
    };
    let el = NodeRef::<html::Details>::new();
    let p3 = path.clone();
    Effect::new(move |_| {
        let f = focus.get();
        if !f.is_empty() && f.starts_with(&p3) {
            if let Some(d) = el.get() {
                d.set_open(true);
                d.scroll_into_view();
            }
        }
    });
    view! {
        <details node_ref=el class="sb-item" open=fresh>
            <summary class="sb-item-line">
                <span class="part-kind">{kind}</span>
                <span class="sb-item-name">{move || { let s = summary.get(); if s.is_empty() { "(no name)".to_string() } else { s } }}</span>
                {move || dirty.get().then(|| view! { <span class="state-badge">"unapplied"</span> })}
                <span class="spacer"></span>
                <button class="icon-btn" title="Move up" disabled=move || pos().is_none_or(|i| i == 0) || busy() on:click=up_btn>"↑"</button>
                <button class="icon-btn" title="Move down" disabled=move || pos().is_none_or(|i| i + 1 >= count()) || busy() on:click=down_btn>"↓"</button>
                <button class="icon-btn danger" title="Delete" disabled=busy on:click=delete>"✕"</button>
            </summary>
            {children()}
        </details>
    }
}

fn kind_label(tag: &str) -> &'static str {
    match tag {
        "action" => "Action",
        "attack" => "Attack",
        "save" => "Save",
        "spellcasting" => "Spellcasting",
        "spell" => "Spell",
        "actionRef" => "Uses action",
        "attackRef" => "Uses attack",
        "saveRef" => "Uses save",
        "spellRef" => "Casts spell",
        "speedRef" => "Moves",
        _ => "Item",
    }
}

fn rider_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {pick_in(c, l, "coordinatingConjunction", sb::CONJUNCTIONS, Some("plus"), "Joined by", "")}
            {text_in(c, l, "damage", "Dice", "1d6", "")}
            {text_in(c, l, "damageMod", "Mod", "(none)", "")}
            {pick_in(c, l, "damageType", sb::DAMAGE_TYPES, Some("(none)"), "Type", "")}
            {pick_in(c, l, "saveType", sb::STATS, Some("(no save)"), "Save", "")}
            {text_in(c, l, "saveMod", "Save DC adj.", "(none)", "")}
        </div>
        <div class="field-row">{tf::formula(c, l, "damageFormula", "Damage formula")}{tf::choice(c, l, "damageTypeChoice", "Type choice")}</div>
        <RiderProse c l/>
    }
    .into_any()
}

#[component]
fn RiderProse(c: Card, l: Loc) -> impl IntoView {
    let get = move |t: &'static str| Signal::derive(move || c.m().at(&l.s[..l.n]).map(|e| e.opt_text(t)).unwrap_or_default());
    let put = move |t: &'static str| Callback::new(move |v: String| c.at(l, |e| e.set_opt_text(t, &v, false)));
    view! {
        <div class="field-row">
            <div class="field grow"><span class="field-label">"After its damage"</span>
                <ChipEditor value=get("chase") set=put("chase") placeholder="(none) if the target is Prone" single=true/></div>
            <div class="field grow"><span class="field-label">"Or instead, all of it"</span>
                <ChipEditor value=get("override") set=put("override") placeholder="(none)" single=true/></div>
        </div>
    }
}

fn tier_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">{text_in(c, l, "frequency", "Frequency", "at will", "\u{201c}1/Day Each\u{201d}")}{text_in(c, l, "id", "Id", "(none)", "")}</div>
        <Rows c loc=l tag="spell" title="Spells" min=1 make=new_spell
            row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "name", "Spell", "Mage Hand", "")}{text_in(c, l, "id", "Id", "mage-hand", "for <spellRef>")}{text_in(c, l, "note", "Note", "(none)", "in parentheses")}</div> }.into_any()/>
    }
    .into_any()
}

/// A new spell, its id not yet used in `m` (the spellcasting it goes in).
fn new_spell(m: &El) -> El {
    El::new("spell").with("id", &sb::fresh("new-spell", &sb::ids(m))).with("name", "New Spell")
}

#[component]
fn ActionCard(tab: DocTab, child: usize, mon: Memo<Option<El>>, path: Vec<usize>, list: &'static str, fresh: bool, refs: RefCtx, focus: Memo<Vec<usize>>) -> impl IntoView {
    let p2 = path.clone();
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| find(m, &p2))));
    let tag = el.with_untracked(|e| e.as_ref().map(|e| e.tag.clone()).unwrap_or_default());
    let c = Card::new(tab, child, el, None, None);
    let legendary = list == "legendaryActions";
    let reaction = list == "reactions";
    let summary = move || el.with(|e| e.as_ref().map(|e| {
        let n = e.get("name");
        if n.is_empty() { e.get("ref").to_string() } else { n.to_string() }
    }).unwrap_or_default());
    // Each kind's form is its own function: one function holding them all needs more
    // stack than wasm has in an unoptimized (dev) build.
    let ctx = ItemCtx { c, legendary, reaction, refs };
    let body = match tag.as_str() {
        "action" => action_form(ctx),
        "attack" => attack_form(ctx),
        "save" => save_form(ctx),
        "spellcasting" => spellcasting_form(ctx),
        "spell" => spell_form(ctx),
        t => ref_form(ctx, t),
    };
    view! {
        <ItemShell tab child mon path=path.clone() fresh focus kind=kind_label(&tag) summary=Signal::derive(summary) dirty=Signal::derive(move || c.d.dirty())>
            <div class="sb-item-body" on:keydown=keys(c)>
                {body}
                {card_bar(c)}
            </div>
        </ItemShell>
    }
}

/// What an action card's form needs.
#[derive(Clone, Copy)]
struct ItemCtx {
    c: Card,
    legendary: bool,
    reaction: bool,
    refs: RefCtx,
}

/// Uses, a legendary cost, and a reaction's trigger.
fn item_head(x: ItemCtx) -> AnyView {
    let (c, r, refs) = (x.c, Loc::ROOT, x.refs);
    // Which optional parts show is decided once, as the card opens.
    let cost = x.legendary || c.model.with_untracked(|m| m.kid("cost").is_some());
    let trigger = x.reaction || c.model.with_untracked(|m| m.kid("trigger").is_some());
    view! {
        {uses_rows(c, r)}
        <div class="field-row">
            {cost.then(|| kid_text(c, r, "cost", false, "Cost", "1", "Legendary uses it spends"))}
        </div>
        {trigger.then(|| view! { <Prose c loc=r tag="trigger" label="Trigger" refs/> })}
    }
    .into_any()
}

fn item_response(x: ItemCtx) -> AnyView {
    let (c, refs) = (x.c, x.refs);
    let show = x.reaction || c.model.with_untracked(|m| m.kid("response").is_some());
    show.then(|| view! { <Prose c loc=Loc::ROOT tag="response" label="Response" refs/> }).into_any()
}

fn item_ids(c: Card, recharge: bool) -> AnyView {
    let r = Loc::ROOT;
    view! {
        <div class="field-row">
            {text_in(c, r, "name", "Name", "", "")}
            {text_in(c, r, "id", "Id", "", "Unique in the stat block; refs follow a rename")}
            {recharge.then(|| text_in(c, r, "recharge", "Recharge", "(none) 5-6", ""))}
        </div>
        {recharge.then(|| tf::variant(c, r))}
    }
    .into_any()
}

fn action_form(x: ItemCtx) -> AnyView {
    let (c, refs) = (x.c, x.refs);
    view! {
        {item_ids(c, true)}
        {item_head(x)}
        <Prose c loc=Loc::ROOT label="Text" refs/>
        {item_response(x)}
    }
    .into_any()
}

fn attack_numbers(c: Card) -> AnyView {
    let r = Loc::ROOT;
    view! {
        <div class="field-row">
            {pick_in(c, r, "type", sb::ATTACK_KINDS, None, "Kind", "")}
            {pick_in(c, r, "stat", sb::STATS, Some("(Str melee, Dex ranged)"), "Ability", "")}
            {text_in(c, r, "attackMod", "Attack bonus", "(PB + mod)", "A number overrides")}
            {text_in(c, r, "reach", "Reach", "5", "feet")}
            {text_in(c, r, "range", "Range", "(none) 30/120", "")}
            {tf::formula(c, r, "attackFormula", "Attack formula")}
        </div>
    }
    .into_any()
}

fn attack_damage(c: Card) -> AnyView {
    let r = Loc::ROOT;
    view! {
        <div class="field-row">
            {text_in(c, r, "damage", "Damage dice", "1d8", "")}
            {text_in(c, r, "damageMod", "Damage mod", "(ability mod)", "")}
            {pick_in(c, r, "damageType", sb::DAMAGE_TYPES, Some("(none)"), "Damage type", "")}
            {text_in(c, r, "note", "Note", "(none)", "after the bonus: \u{201c}with Advantage if…\u{201d}")}
        </div>
        <div class="field-row">{tf::formula(c, r, "damageFormula", "Damage formula")}{tf::choice(c, r, "damageTypeChoice", "Damage type choice")}</div>
    }
    .into_any()
}

fn attack_riders(c: Card) -> AnyView {
    view! {
        <Rows c loc=Loc::ROOT tag="rider" title="Riders" hint="Extra damage on a hit: \u{201c}plus 4 (1d8) Lightning damage\u{201d}."
            make=|_| El::new("rider").with("damage", "1d6").with("damageType", "fire") row=rider_row/>
    }
    .into_any()
}

fn attack_outcome(x: ItemCtx) -> AnyView {
    let (c, refs, r) = (x.c, x.refs, Loc::ROOT);
    view! {
        <Prose c loc=r tag="hit" label="Hit" hint="After the damage and riders" refs/>
        <Prose c loc=r tag="miss" label="Miss" refs/>
    }
    .into_any()
}

fn attack_form(x: ItemCtx) -> AnyView {
    let c = x.c;
    view! {
        {item_ids(c, true)}
        {attack_numbers(c)}
        {attack_damage(c)}
        {item_head(x)}
        {attack_riders(c)}
        {attack_outcome(x)}
        {item_response(x)}
    }
    .into_any()
}

fn save_form(x: ItemCtx) -> AnyView {
    view! {
        {item_ids(x.c, true)}
        {item_head(x)}
        {save_fields(x.c, Loc::ROOT, x.refs)}
        {item_response(x)}
    }
    .into_any()
}

fn spellcasting_fields(c: Card) -> AnyView {
    let r = Loc::ROOT;
    view! {
        <div class="field-row">
            {text_in(c, r, "name", "Name", "Spellcasting", "")}
            {text_in(c, r, "id", "Id", "(none)", "")}
            {pick_in(c, r, "spellcastingAbility", sb::STATS, None, "Ability", "")}
            {text_in(c, r, "dcMod", "DC adjustment", "(none)", "")}
            {tf::formula(c, r, "dcFormula", "DC formula")}
        </div>
        {tf::variant(c, r)}
        <div class="field flags">
            {flag_in(c, r, "verbalComponents", true, "Verbal")}
            {flag_in(c, r, "somaticComponents", true, "Somatic")}
            {flag_in(c, r, "materialComponents", true, "Material")}
        </div>
    }
    .into_any()
}

fn spellcasting_form(x: ItemCtx) -> AnyView {
    let c = x.c;
    view! {
        {spellcasting_fields(c)}
        {item_head(x)}
        <Rows c loc=Loc::ROOT tag="spellTier" title="Spell lists" min=1
            make=|m: &El| El::new("spellTier").with("frequency", "1/Day Each").with_kid(new_spell(m)) row=tier_row/>
    }
    .into_any()
}

fn spell_form(x: ItemCtx) -> AnyView {
    let (c, r) = (x.c, Loc::ROOT);
    view! {
        {item_ids(c, false)}
        <div class="field-row">{text_in(c, r, "note", "Note", "(none)", "")}{text_in(c, r, "cost", "Cost", "1", "")}</div>
    }
    .into_any()
}

fn ref_form(x: ItemCtx, t: &str) -> AnyView {
    let (c, refs, r) = (x.c, x.refs, Loc::ROOT);
    let targets: &'static [&'static str] = match t {
        "actionRef" => &["action", "attack", "save", "spellcasting"],
        "attackRef" => &["attack"],
        "saveRef" => &["save"],
        "spellRef" => &["spell"],
        _ => &["speed"],
    };
    view! {
        <div class="field-row">
            <label class="field">
                <span class="field-label">"Uses"</span>
                <select prop:value=move || c.m().get("ref").to_string() on:change=move |ev| { let v = event_target_value(&ev); c.change(|e| e.set("ref", &v)) }>
                    {move || refs.get().into_iter().filter(|(t, _, _)| targets.contains(&t.as_str())).map(|(t, id, name)| view! { <option value=id.clone()>{format!("{name} ({t} {id})")}</option> }).collect_view()}
                </select>
            </label>
            {text_in(c, r, "name", "Own name", "(its name)", "")}
            {text_in(c, r, "id", "Own id", "(none)", "")}
            {text_in(c, r, "cost", "Cost", "1", "")}
        </div>
    }
    .into_any()
}

/// A new item for an action list, with ids not yet used in the stat block.
fn new_item(m: &El, tag: &str) -> El {
    let taken = sb::ids(m);
    let named = |name: &str| sb::fresh(&sb::slug(name), &taken);
    match tag {
        "attack" => El::new("attack").with("id", &named("New Attack")).with("name", "New Attack").with("type", "melee").with("reach", "5").with("damage", "1d6").with("damageType", "slashing"),
        "save" => El::new("save").with("id", &named("New Save")).with("name", "New Save").with("saveType", "dex").with("saveStat", "con").with_kid(El::new("targets")),
        "spellcasting" => El::new("spellcasting").with("id", &named("Spellcasting")).with("spellcastingAbility", "cha")
            .with_kid(El::new("spellTier").with("frequency", "at will").with_kid(El::new("spell").with("id", &named("New Spell")).with("name", "New Spell"))),
        "spell" => El::new("spell").with("id", &named("New Spell")).with("name", "New Spell"),
        "actionRef" => {
            let first = m.walk().into_iter().find(|e| ["action", "attack", "save"].contains(&e.tag.as_str()) && !e.get("id").is_empty()).map(|e| e.get("id").to_string()).unwrap_or_default();
            El::new("actionRef").with("ref", &first)
        }
        _ => El::new("action").with("id", &named("New Action")).with("name", "New Action"),
    }
}

/// One list (traits, actions, bonus actions, reactions, legendary actions).
#[component]
fn ListSection(tab: DocTab, child: usize, mon: Memo<Option<El>>, list: &'static str, refs: RefCtx, focus: Memo<Vec<usize>>) -> impl IntoView {
    let host = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid(list).cloned())));
    let items = Memo::new(move |_| {
        host.with(|h| h.as_ref().map(|h| h.kids.iter().filter(|k| if list == "traits" { k.tag == "trait" } else { sb::ITEMS.contains(&k.tag.as_str()) }).filter_map(|k| k.path.clone()).collect::<Vec<_>>()).unwrap_or_default())
    });
    let initial: StoredValue<HashSet<Vec<usize>>> = StoredValue::new(items.get_untracked().into_iter().collect());
    let busy = move || tab.applying.get();
    let add = move |tag: &'static str| {
        let Some(m) = mon.get_untracked() else { return };
        let el = if list == "traits" { El::new("trait").with("name", "New Trait").with_kid(El::new("description")) } else { new_item(&m, tag) };
        match m.kid(list) {
            Some(h) => {
                let after = h.kids.iter().rposition(|k| sb::rank(list, &k.tag) == sb::rank(list, &el.tag)).and_then(|i| h.kids[i].path.as_ref()?.last().copied());
                edit(tab, StatEdit::Insert { child, parent: h.path.clone().unwrap_or_default(), parent_tag: list.into(), el, after });
            }
            None => {
                let mut host = El::new(list);
                if list == "legendaryActions" {
                    host.kids.push(El::new("uses").with_kid(El::new("number").with("value", "3")));
                }
                host.kids.push(el);
                edit(tab, StatEdit::Insert { child, parent: vec![], parent_tag: "monster".into(), el: host, after: None });
            }
        }
    };
    let kinds: &'static [&'static str] = match list {
        "traits" => &["trait"],
        "legendaryActions" => &["action", "attack", "save", "spell", "actionRef"],
        _ => &["action", "attack", "save", "spellcasting"],
    };
    view! {
        {(list == "actions" || list == "bonusActions").then(|| view! { <Multiattack tab child mon list refs/> })}
        {(list == "legendaryActions").then(|| view! { <LegendaryUses tab child mon/> })}
        <div class="sb-items">
            <For each=move || items.get() key=|p| p.clone() let:path>
                {
                    let fresh = !initial.with_value(|i| i.contains(&path));
                    if list == "traits" {
                        view! { <TraitCard tab child mon path fresh refs focus/> }.into_any()
                    } else {
                        view! { <ActionCard tab child mon path list fresh refs focus/> }.into_any()
                    }
                }
            </For>
        </div>
        <div class="add-part">
            {kinds.iter().map(|k| { let k: &'static str = k; view! {
                <button class="small" disabled=busy on:click=move |_| add(k)>{format!("+ {}", if k == "trait" { "Trait" } else if k == "actionRef" { "Reuse an action" } else { kind_label(k) })}</button>
            }}).collect_view()}
        </div>
    }
}

#[component]
fn Multiattack(tab: DocTab, child: usize, mon: Memo<Option<El>>, list: &'static str, refs: RefCtx) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid(list)).map(|h| scoped(h, &["multiattack"]))));
    let c = Card::new(tab, child, el, scope(&["multiattack"]), None);
    view! {
        <Show when=move || el.with(|e| e.is_some())>
            <div class="table-card sb-multi" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
                <Prose c loc=Loc::ROOT tag="multiattack" label="Multiattack" ph="(none)" hint="Empty leaves it out." refs/>
                {card_bar(c)}
            </div>
        </Show>
    }
}

#[component]
fn LegendaryUses(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid("legendaryActions")).map(|h| scoped(h, &["uses"]))));
    let c = Card::new(tab, child, el, scope(&["uses"]), None);
    view! {
        <Show when=move || el.with(|e| e.is_some())>
            <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
                <Rows c loc=Loc::ROOT container="uses" tag="number" title="Legendary Action Uses" hint="\u{201c}3 (4 in Lair)\u{201d}: one per variant." min=1
                    make=|_| El::new("number").with("value", "3")
                    row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "value", "Uses", "3", "")}{text_in(c, l, "note", "Note", "(none) in lair", "")}</div> }.into_any()/>
                {card_bar(c)}
            </div>
        </Show>
    }
}

/// The stat block at the root's child `child`, all of it on one page.
#[component]
pub fn StatPage(tab: DocTab, child: usize) -> impl IntoView {
    let mon = Memo::new(move |_| sb::read(&tab.doc().source, child));
    let refs = ref_ctx(mon);
    let tpl = tf::provide_tpl(mon);
    crate::prose_editor::provide_env(tab, shared::prose::Palette::StatBlock, Some(refs));
    // The tree's selection inside this stat block, as a path from it.
    let focus = Memo::new(move |_| {
        let sel = tab.selected.get();
        let key = tab.doc().key_of(sel);
        if key.first() == Some(&child) { key[1..].to_vec() } else { Vec::new() }
    });
    let at = move |tag: &'static str| Signal::derive(move || {
        let f = focus.get();
        mon.with(|m| m.as_ref().and_then(|m| m.kids.iter().find(|k| k.tag == tag)).and_then(|k| k.path.clone())).is_some_and(|p| !f.is_empty() && f.starts_with(&p))
    });
    let present = move |tag: &'static str| mon.with(|m| m.as_ref().is_some_and(|m| m.kid(tag).is_some()));
    let add_part = move |tag: &'static str| edit(tab, StatEdit::Insert { child, parent: vec![], parent_tag: "monster".into(), el: new_part(tag), after: None });
    let lists: [(&'static str, &'static str, &'static str); 5] = [
        ("traits", "Traits", ""),
        ("actions", "Actions", ""),
        ("bonusActions", "Bonus Actions", ""),
        ("reactions", "Reactions", "Each needs a trigger and a response."),
        ("legendaryActions", "Legendary Actions", ""),
    ];
    view! {
        {move || if mon.with(|m| m.is_none()) {
            view! { <div class="empty"><p>"This stat block can't be read."</p></div> }.into_any()
        } else {
            view! {
                <div class="sb-page">
                    <Section title="Stat block" focus=Signal::derive(|| false)>
                        <IdentityCard tab child mon/>
                    </Section>
                    <Section title="Entry parts" hint="Its own subtitle, habitats, lairs and lore" focus=at("brief")>
                        <EntryCard tab child mon/>
                        {OWN_PARTS.iter().map(|t| { let t: &'static str = t; view! {
                            <Show when=move || present(t)>
                                <OwnPart tab child mon tag=t refs/>
                            </Show>
                        }}).collect_view()}
                        <div class="add-part">
                            {OWN_PARTS.iter().map(|t| { let t: &'static str = t; view! {
                                <Show when=move || !present(t)>
                                    <button class="small" disabled=move || tab.applying.get() on:click=move |_| add_part(t)>{format!("+ {}", family::part_label(t))}</button>
                                </Show>
                            }}).collect_view()}
                        </div>
                    </Section>
                    <Show when=move || tpl.on.get() || present("formulas")>
                        <Section title="Formulas" hint="A template's named numbers and the summoner's choices" focus=at("formulas")>
                            <tf::FormulasSection tab child mon/>
                        </Section>
                    </Show>
                    <Section title="Heading" hint="Size, type, AC, initiative, HP, speeds" focus=at("heading")>
                        <HeadingCard tab child mon/>
                    </Section>
                    <Section title="Ability scores" focus=at("statBlock")>
                        <AbilitiesCard tab child mon/>
                    </Section>
                    <Section title="Attributes" hint="Skills, defenses, gear, senses, languages, CR" focus=at("attributes")>
                        <AttributesCard tab child mon/>
                    </Section>
                    {lists.into_iter().map(|(list, title, hint)| view! {
                        <Section title=title hint=hint focus=at(list)>
                            <ListSection tab child mon list refs focus/>
                        </Section>
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
}
