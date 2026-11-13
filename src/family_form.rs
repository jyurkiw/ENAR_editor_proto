//! The family editor for a monster file: the header (name, short noun, subtitle,
//! habitats) and the family's parts (quote boxes, images, descriptions, distinction
//! tables and lairs), which follow the header in any order. The file's page shows the
//! header and the list of parts; each part opens in its own form from the list or the
//! tree. Text boxes and field groups apply one at a time (Apply, Ctrl+Enter or Enter);
//! adding, moving and deleting apply at once. Every apply validates the whole file.

use leptos::ev::{FocusEvent, KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use shared::family::{self, EffectView, Family, FamilyEdit, HeaderModel, ImageModel, PartAt, PartView, PARTS};
use shared::inline;
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::state::DocTab;

fn confirm(msg: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(msg).ok()).unwrap_or(false)
}

fn edit(tab: DocTab, e: FamilyEdit) {
    apply(tab, Change::Family { edit: e });
}

fn focus_stays(ev: &FocusEvent, el: &NodeRef<html::Div>) -> bool {
    ev.related_target()
        .and_then(|t| t.dyn_into::<web_sys::Node>().ok())
        .zip(el.get())
        .is_some_and(|(n, e)| e.contains(Some(&n)))
}

fn json<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default()
}

/// The draft an edit was made from (see [`Draft`]), so apply can drop it or mark it
/// refused.
pub fn draft_key(e: &FamilyEdit) -> Option<String> {
    use FamilyEdit::*;
    Some(match e {
        Header { .. } => "header".into(),
        SetId { at, .. } => format!("id:{}", at.child),
        SetPara { at, n, .. } => format!("p:{}:{n}", at.child),
        Attribution { at, .. } => format!("attr:{}", at.child),
        Image { at, .. } => format!("img:{}", at.child),
        Table { at, .. } => format!("table:{}", at.child),
        Lair { at, .. } => format!("lair:{}", at.child),
        SetEffect { at, n, .. } => format!("eff:{}:{n}", at.child),
        _ => return None,
    })
}

/// An unapplied edit in the tab, by key, kept as (the text it was made from, the new
/// text): when the file changes under it, the draft no longer matches and is dropped.
/// Field groups keep their model as JSON.
#[derive(Clone, Copy)]
struct Draft {
    tab: DocTab,
    key: StoredValue<String>,
    base: Memo<String>,
}

impl Draft {
    fn new(tab: DocTab, key: String, base: Memo<String>) -> Self {
        Draft { tab, key: StoredValue::new(key), base }
    }
    fn get(&self) -> Option<String> {
        let b = self.base.get();
        let k = self.key.get_value();
        self.tab.fam_drafts.with(|d| d.get(&k).filter(|(o, _)| *o == b).map(|(_, t)| t.clone()))
    }
    fn text(&self) -> String {
        self.get().unwrap_or_else(|| self.base.get())
    }
    fn dirty(&self) -> bool {
        self.get().is_some()
    }
    fn failed(&self) -> bool {
        let k = self.key.get_value();
        self.tab.fam_failed.with(|f| f.contains(&k))
    }
    fn set(&self, v: String) {
        let b = self.base.get_untracked();
        let k = self.key.get_value();
        self.tab.fam_drafts.update(|d| {
            if v == b {
                d.remove(&k);
            } else {
                d.insert(k, (b, v));
            }
        });
    }
    fn revert(&self) {
        let k = self.key.get_value();
        self.tab.fam_drafts.update(|d| {
            d.remove(&k);
        });
        self.tab.fam_failed.update(|f| {
            f.remove(&k);
        });
    }
    fn model<T: DeserializeOwned + Default>(&self) -> T {
        serde_json::from_str(&self.text()).unwrap_or_default()
    }
    fn change<T: Serialize + DeserializeOwned + Default>(&self, f: impl FnOnce(&mut T)) {
        let mut m: T = serde_json::from_str(&self.get().unwrap_or_else(|| self.base.get_untracked())).unwrap_or_default();
        f(&mut m);
        self.set(json(&m));
    }
}

/// The state badges and Revert / Apply.
fn bar(d: Draft, busy: Signal<bool>, apply_it: impl Fn() + Copy + Send + Sync + 'static) -> impl IntoView {
    view! {
        <div class="p-bar fam-bar">
            {move || d.failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
            {move || (d.dirty() && !d.failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
            <span class="spacer"></span>
            <button class="small" disabled=move || !d.dirty() || busy.get() on:click=move |_| d.revert()>"Revert"</button>
            <button class="small primary" disabled=move || !d.dirty() || busy.get() on:click=move |_| apply_it()>"Apply"</button>
        </div>
    }
}

/// Selects the root child `child` in the tree (and so opens its form).
fn open_child(tab: DocTab, child: usize) {
    let d = tab.doc_untracked();
    if let Some(i) = d.nodes.first().and_then(|r| r.children.get(child)).copied() {
        tab.reveal(i);
    }
}

/// A list of names (habitats, treasure) as chips, with an input to add one.
#[component]
fn NameList(values: Signal<Vec<String>>, set: Callback<Vec<String>>, placeholder: &'static str) -> impl IntoView {
    let input = RwSignal::new(String::new());
    let add = move || {
        let v = input.get_untracked();
        let v = v.trim();
        if v.is_empty() {
            return;
        }
        let mut l = values.get_untracked();
        l.push(v.to_string());
        set.run(l);
        input.set(String::new());
    };
    view! {
        <div class="chips">
            {move || values.get().into_iter().enumerate().map(|(i, v)| view! {
                <span class="chip">
                    {v}
                    <button type="button" class="chip-x" title="Remove" on:click=move |_| {
                        let mut l = values.get_untracked();
                        if i < l.len() {
                            l.remove(i);
                        }
                        set.run(l);
                    }>"✕"</button>
                </span>
            }).collect_view()}
            <input type="text" class="chip-input" placeholder=placeholder prop:value=move || input.get()
                on:input=move |ev| input.set(event_target_value(&ev))
                on:keydown=move |ev: KeyboardEvent| if ev.key() == "Enter" {
                    ev.prevent_default();
                    ev.stop_propagation();
                    add()
                }/>
            <button type="button" class="small" on:click=move |_| add()>"+ Add"</button>
        </div>
    }
}

/// The header: `<name>`, `<shortNoun>`, `<brief>`, `<habitats>`.
#[component]
fn FamilyHeader(tab: DocTab, fam: Memo<Option<Family>>) -> impl IntoView {
    let base = Memo::new(move |_| fam.with(|f| f.as_ref().map(|f| json(&f.header)).unwrap_or_default()));
    let d = Draft::new(tab, "header".into(), base);
    let m = move || d.model::<HeaderModel>();
    let busy = Signal::derive(move || tab.applying.get());
    let el = NodeRef::<html::Div>::new();
    let do_apply = move || {
        if d.dirty() {
            edit(tab, FamilyEdit::Header { model: m() });
        }
    };
    let habitats = Signal::derive(move || m().habitats);
    let treasure = Signal::derive(move || m().treasure);
    view! {
        <div node_ref=el class="table-card fam-header" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:keydown=move |ev: KeyboardEvent| {
                let in_input = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some();
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" && in_input { do_apply() }
            }>
            <div class="table-card-title">
                "Family"
                <span class="hint">"The file's header: the heading over the family, the short noun, and the subtitle and habitat line under the heading. Enter applies; Esc reverts."</span>
            </div>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Family name"</span>
                    <input type="text" placeholder="Gnolls of Yeenoghu" prop:value=move || m().name
                        on:input=move |ev| d.change(|h: &mut HeaderModel| h.name = event_target_value(&ev))/>
                    <span class="hint">"<name>: the entry's heading"</span>
                </label>
                <label class="field">
                    <span class="field-label">"Short noun"</span>
                    <input type="text" placeholder="gnoll" prop:value=move || m().short_noun
                        on:input=move |ev| d.change(|h: &mut HeaderModel| h.short_noun = event_target_value(&ev))/>
                    <span class="hint">"<shortNoun>: what <shortNounRef/> prints"</span>
                </label>
                <label class="field check-field">
                    <span class="field-label">"Proper noun"</span>
                    <input type="checkbox" prop:checked=move || m().proper
                        on:change=move |ev| d.change(|h: &mut HeaderModel| h.proper = event_target_checked(&ev))/>
                    <span class="hint">"A name (\u{201c}Yeenoghu\u{201d}): printed without \u{201c}the\u{201d}"</span>
                </label>
            </div>
            <div class="field-row">
                <label class="field grow">
                    <span class="field-label">"Subtitle"</span>
                    <input type="text" placeholder="(none) e.g. Hyenas in the shape of men" prop:value=move || m().brief
                        on:input=move |ev| d.change(|h: &mut HeaderModel| h.brief = event_target_value(&ev))/>
                    <span class="hint">"<brief>: italic, under the heading. Empty leaves it out."</span>
                </label>
                <label class="field id-field">
                    <span class="field-label">"Its id"</span>
                    <input type="text" placeholder="(none)" prop:value=move || m().brief_id
                        on:input=move |ev| d.change(|h: &mut HeaderModel| h.brief_id = event_target_value(&ev))/>
                </label>
            </div>
            <div class="field-row">
                <div class="field grow">
                    <span class="field-label">"Habitats"</span>
                    <NameList values=habitats placeholder="Hill"
                        set=Callback::new(move |v: Vec<String>| d.change(|h: &mut HeaderModel| h.habitats = v))/>
                </div>
                <div class="field grow">
                    <span class="field-label">"Treasure"</span>
                    <NameList values=treasure placeholder="Armaments"
                        set=Callback::new(move |v: Vec<String>| d.change(|h: &mut HeaderModel| h.treasure = v))/>
                </div>
                <label class="field id-field">
                    <span class="field-label">"Its id"</span>
                    <input type="text" placeholder="(none)" prop:value=move || m().habitats_id
                        on:input=move |ev| d.change(|h: &mut HeaderModel| h.habitats_id = event_target_value(&ev))/>
                </label>
            </div>
            <p class="hint">"<habitats> prints as \u{201c}Habitat: Hill, Forest; Treasure: Armaments\u{201d}. No habitats or treasure leaves it out."</p>
            {bar(d, busy, do_apply)}
        </div>
    }
}

/// "+ Add" a part (or a quote box and an image together) after root child `after`
/// (None: right after the header).
#[component]
fn AddPart(tab: DocTab, after: Signal<Option<usize>>, #[prop(into)] label: String) -> impl IntoView {
    let kind = RwSignal::new("description".to_string());
    let busy = move || tab.applying.get();
    let add = move |tags: Vec<String>| edit(tab, FamilyEdit::AddParts { after: after.get_untracked(), tags });
    view! {
        <div class="add-part">
            <span class="hint">{label}</span>
            <select prop:value=move || kind.get() on:change=move |ev| kind.set(event_target_value(&ev))>
                {PARTS.iter().map(|t| view! { <option value=*t>{family::part_label(t)}</option> }).collect_view()}
            </select>
            <button class="small" disabled=busy on:click=move |_| add(vec![kind.get_untracked()])>"+ Add"</button>
            <button class="small" disabled=busy title="A quote box with an image after it: they usually go together"
                on:click=move |_| add(vec!["quoteBox".into(), "image".into()])>"+ Quote box & image"</button>
        </div>
    }
}

fn delete_part(tab: DocTab, p: &PartView) {
    let what = family::part_label(&p.tag).to_lowercase();
    let msg = if !p.named_by.is_empty() {
        format!(
            "Delete the lair \u{201c}{}\u{201d}? The stat blocks {} name it with <lairRef>; those go too.",
            p.summary,
            p.named_by.join(", ")
        )
    } else {
        format!("Delete this {what}?\n\n{}", p.summary)
    };
    if confirm(&msg) {
        edit(tab, FamilyEdit::DeletePart { at: p.at() });
    }
}

/// The family's parts in file order: open, reorder, delete, add.
#[component]
fn PartsList(tab: DocTab, fam: Memo<Option<Family>>) -> impl IntoView {
    let parts = Memo::new(move |_| fam.with(|f| f.as_ref().map(|f| f.parts.clone()).unwrap_or_default()));
    let busy = move || tab.applying.get();
    let last = Signal::derive(move || {
        fam.with(|f| f.as_ref().map(|f| f.parts.last().map(|p| p.child).unwrap_or(f.header_end)))
    });
    view! {
        <div class="table-card">
            <div class="table-card-title">
                {move || format!("Family parts ({})", parts.with(|p| p.len()))}
                <span class="hint">"After the header, in any order and any number; an entry prints them in this order, before the stat blocks. Click one to edit it."</span>
            </div>
            <Show when=move || parts.with(|p| p.is_empty())>
                <p class="hint">"No lore or lairs yet."</p>
            </Show>
            <div class="stat-list">
                {move || {
                    let list = parts.get();
                    let count = list.len();
                    list.iter().enumerate().map(|(pos, p)| {
                        let child = p.child;
                        let at = p.at();
                        let (a1, a2) = (at.clone(), at.clone());
                        let prev = (pos > 0).then(|| list[pos - 1].child);
                        let after_next = list.get(pos + 2).map(|x| x.child);
                        let pv = p.clone();
                        let summary = if p.summary.trim().is_empty() { "(empty)".to_string() } else { p.summary.clone() };
                        view! {
                            <div class="stat-row">
                                <span class="part-kind">{family::part_label(&p.tag)}</span>
                                <button type="button" class="link stat-name part-summary" on:click=move |_| open_child(tab, child)>{summary}</button>
                                {(!p.id.is_empty()).then(|| view! { <code class="stat-id">{p.id.clone()}</code> })}
                                <span class="spacer"></span>
                                <button class="icon-btn" title="Move up" disabled=move || { pos == 0 || busy() }
                                    on:click=move |_| edit(tab, FamilyEdit::MovePart { at: a1.clone(), before: prev })>"↑"</button>
                                <button class="icon-btn" title="Move down" disabled=move || { pos + 1 >= count || busy() }
                                    on:click=move |_| edit(tab, FamilyEdit::MovePart { at: a2.clone(), before: after_next })>"↓"</button>
                                <button class="icon-btn danger" title="Delete" disabled=busy on:click=move |_: MouseEvent| delete_part(tab, &pv)>"✕"</button>
                            </div>
                        }
                    }).collect_view()
                }}
            </div>
            <AddPart tab after=last label="New part at the end:"/>
        </div>
    }
}

/// One paragraph of a part: a text box of its XML.
#[component]
fn ParaRow(tab: DocTab, at: StoredValue<PartAt>, n: usize, paras: Memo<Vec<String>>, min_one: bool) -> impl IntoView {
    let base = Memo::new(move |_| paras.with(|p| p.get(n).cloned().unwrap_or_default()));
    let count = Memo::new(move |_| paras.with(|p| p.len()));
    let d = Draft::new(tab, format!("p:{}:{n}", at.get_value().child), base);
    let busy = Signal::derive(move || tab.applying.get());
    let bad = move || inline::parse(&d.text()).err().map(|e| e.message);
    let el = NodeRef::<html::Div>::new();
    let do_apply = move || {
        if d.dirty() && bad().is_none() {
            edit(tab, FamilyEdit::SetPara { at: at.get_value(), n, text: d.text() });
        }
    };
    let delete = move |_: MouseEvent| {
        if base.get_untracked().trim().is_empty() || confirm("Delete this paragraph?") {
            edit(tab, FamilyEdit::DeletePara { at: at.get_value(), n });
        }
    };
    view! {
        <div node_ref=el class="body-block para" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="block-gutter"><span class="para-n">{n + 1}</span></div>
            <div class="block-main">
                <textarea class="p-text" spellcheck="true" rows=2 placeholder="(empty paragraph)"
                    prop:value=move || d.text() on:input=move |ev| d.set(event_target_value(&ev))></textarea>
                {move || bad().map(|e| view! { <p class="hint bad">"Not well-formed XML yet: " {e}</p> })}
                <div class="p-bar">
                    {move || d.failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                    {move || (d.dirty() && !d.failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                    <span class="spacer"></span>
                    <button class="icon-btn" title="Move up" disabled=move || { n == 0 || busy.get() }
                        on:click=move |_| edit(tab, FamilyEdit::MovePara { at: at.get_value(), n, before: Some(n - 1) })>"↑"</button>
                    <button class="icon-btn" title="Move down" disabled=move || { n + 1 >= count.get() || busy.get() }
                        on:click=move |_| {
                            let before = (n + 2 < count.get_untracked()).then_some(n + 2);
                            edit(tab, FamilyEdit::MovePara { at: at.get_value(), n, before })
                        }>"↓"</button>
                    <button class="small" title="Insert an empty paragraph below" disabled=busy
                        on:click=move |_| edit(tab, FamilyEdit::AddPara { at: at.get_value(), after: Some(n) })>"+ ¶ below"</button>
                    <button class="small" disabled=move || !d.dirty() || busy.get() on:click=move |_| d.revert()>"Revert"</button>
                    <button class="small primary" disabled=move || !d.dirty() || busy.get() || bad().is_some() on:click=move |_| do_apply()>"Apply"</button>
                    <button class="icon-btn danger" disabled=move || { busy.get() || (min_one && count.get() <= 1) }
                        title=move || if min_one && count.get() <= 1 { "It needs at least one paragraph: delete the part instead" } else { "Delete paragraph" }
                        on:click=delete>"✕"</button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn Paras(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>, min_one: bool, #[prop(into)] title: String, #[prop(into)] hint: String) -> impl IntoView {
    let paras = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.paras.clone()).unwrap_or_default()));
    let count = Memo::new(move |_| paras.with(|p| p.len()));
    let busy = move || tab.applying.get();
    view! {
        <div class="table-card">
            <div class="table-card-title">
                {move || format!("{title} ({})", count.get())}
                <span class="hint">{hint}</span>
            </div>
            <Show when=move || { count.get() == 0 }>
                <p class="hint">"None yet."</p>
            </Show>
            <div class="block-body">
                <For each=move || 0..count.get() key=|i| *i let:i>
                    <ParaRow tab at n=i paras min_one/>
                </For>
            </div>
            <div class="add-part">
                <button class="small" disabled=busy on:click=move |_| edit(tab, FamilyEdit::AddPara { at: at.get_value(), after: None })>"+ ¶ first"</button>
            </div>
        </div>
    }
}

/// A part's title bar: its kind, its id, move, delete, back to the family.
#[component]
fn PartBar(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>, fam: Memo<Option<Family>>) -> impl IntoView {
    let child = at.get_value().child;
    let tag = at.get_value().tag;
    let is_lair = tag == "lair";
    let base = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.id.clone()).unwrap_or_default()));
    let d = Draft::new(tab, format!("id:{child}"), base);
    let busy = Signal::derive(move || tab.applying.get());
    let el = NodeRef::<html::Div>::new();
    let do_apply = move || {
        if d.dirty() {
            edit(tab, FamilyEdit::SetId { at: at.get_value(), id: d.text() });
        }
    };
    let list = Memo::new(move |_| fam.with(|f| f.as_ref().map(|f| f.parts.iter().map(|p| p.child).collect::<Vec<_>>()).unwrap_or_default()));
    let pos = Memo::new(move |_| list.with(|l| l.iter().position(|c| *c == child)));
    let count = Memo::new(move |_| list.with(|l| l.len()));
    let move_up = move |_| {
        let Some(p) = pos.get_untracked().filter(|p| *p > 0) else { return };
        let before = list.with_untracked(|l| l[p - 1]);
        edit(tab, FamilyEdit::MovePart { at: at.get_value(), before: Some(before) });
    };
    let move_down = move |_| {
        let Some(p) = pos.get_untracked() else { return };
        let before = list.with_untracked(|l| l.get(p + 2).copied());
        edit(tab, FamilyEdit::MovePart { at: at.get_value(), before });
    };
    let after = Signal::derive(move || Some(child));
    view! {
        <div node_ref=el class="table-card part-bar" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" { do_apply() }
            }>
            <div class="table-card-title">
                {family::part_label(&tag)}
                <span class="hint">{move || match (pos.get(), count.get()) {
                    (Some(p), c) => format!("part {} of {c} in the family", p + 1),
                    _ => String::new(),
                }}</span>
                <button class="link" title="Back to the family's header and parts" on:click=move |_| tab.reveal(0)>"the family"</button>
                <span class="spacer"></span>
                <button class="icon-btn" title="Move up" disabled=move || { pos.get().is_none_or(|p| p == 0) || busy.get() } on:click=move_up>"↑"</button>
                <button class="icon-btn" title="Move down" disabled=move || { pos.get().is_none_or(|p| p + 1 >= count.get()) || busy.get() } on:click=move_down>"↓"</button>
                <button class="small danger" disabled=busy on:click=move |_: MouseEvent| {
                    if let Some(p) = view.get_untracked() {
                        delete_part(tab, &p)
                    }
                }>{format!("Delete {}", family::part_label(&tag).to_lowercase())}</button>
            </div>
            <div class="field-row">
                <label class="field id-field wide">
                    <span class="field-label">"Id"</span>
                    <input type="text" placeholder="(none)" prop:value=move || d.text() on:input=move |ev| d.set(event_target_value(&ev))/>
                    <span class="hint">{if is_lair {
                        "Stat blocks name the lair with <lairRef ref>; renaming it renames those too. Unique among the file's part and lair ids."
                    } else {
                        "Optional. A supplement's <monsterEntry> uses it to leave the part out (omitParts) or print it by name (<part ref>). Unique in the file."
                    }}</span>
                </label>
            </div>
            {move || d.failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
            <AddPart tab after label="New part after this one:"/>
        </div>
    }
}

#[component]
fn QuoteForm(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>) -> impl IntoView {
    let base = Memo::new(move |_| view.with(|v| v.as_ref().and_then(|v| v.attribution.clone()).unwrap_or_default()));
    let d = Draft::new(tab, format!("attr:{}", at.get_value().child), base);
    let busy = Signal::derive(move || tab.applying.get());
    let do_apply = move || {
        if d.dirty() {
            edit(tab, FamilyEdit::Attribution { at: at.get_value(), name: d.text() });
        }
    };
    view! {
        <Paras tab at view min_one=true title="Quote" hint="<quote>: the quoted paragraphs, boxed. At least one."/>
        <div class="table-card" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" { do_apply() }
            }>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Attribution"</span>
                    <input type="text" placeholder="(none) e.g. Garrick Moss, fewterer of Hillsfort" prop:value=move || d.text()
                        on:input=move |ev| d.set(event_target_value(&ev))/>
                    <span class="hint">"<attribution name>: who said it, under the quote. Empty removes it."</span>
                </label>
            </div>
            {bar(d, busy, do_apply)}
        </div>
    }
}

#[component]
fn ImageForm(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>) -> impl IntoView {
    let base = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| json(&v.image)).unwrap_or_default()));
    let d = Draft::new(tab, format!("img:{}", at.get_value().child), base);
    let m = move || d.model::<ImageModel>();
    let busy = Signal::derive(move || tab.applying.get());
    let bad = move || inline::parse(&m().cutline).err().map(|e| e.message);
    let do_apply = move || {
        if d.dirty() && bad().is_none() {
            edit(tab, FamilyEdit::Image { at: at.get_value(), model: m() });
        }
    };
    macro_rules! field {
        ($label:expr, $field:ident, $ph:expr, $hint:expr) => {
            view! {
                <label class="field">
                    <span class="field-label">{$label}</span>
                    <input type="text" placeholder=$ph prop:value=move || m().$field
                        on:input=move |ev| d.change(|x: &mut ImageModel| x.$field = event_target_value(&ev))/>
                    <span class="hint">{$hint}</span>
                </label>
            }
        };
    }
    view! {
        <div class="table-card" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:keydown=move |ev: KeyboardEvent| {
                let in_input = ev.target().and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok()).is_some();
                if ev.key() == "Escape" { d.revert() }
                else if (ev.key() == "Enter" && in_input) || (ev.key() == "Enter" && ev.ctrl_key()) { ev.prevent_default(); do_apply() }
            }>
            <div class="table-card-title">
                "Image"
                <span class="hint">"A column-wide figure, with the cutline as its caption."</span>
            </div>
            <div class="field-row">
                {field!("File", src, "art/gnoll.png", "src, relative to this monster file")}
                {field!("Alt text", alt, "(none)", "alt: what the picture shows")}
            </div>
            {move || m().src.trim().is_empty().then(|| view! {
                <p class="hint bad">"No file yet: the compiler stops on an image it can't find."</p>
            })}
            <div class="field-row">
                {field!("Width", width, "(auto)", "width (optional)")}
                {field!("Height", height, "(auto)", "height (optional)")}
                {field!("Title", name, "(none)", "name: the piece, for <imageCredits/>")}
                {field!("Artist", credit, "(none)", "credit: the artist, for <imageCredits/>")}
            </div>
            <label class="field">
                <span class="field-label">"Cutline"</span>
                <textarea class="p-text" rows=2 placeholder="(none) The caption under the image"
                    prop:value=move || m().cutline on:input=move |ev| d.change(|x: &mut ImageModel| x.cutline = event_target_value(&ev))></textarea>
                {move || bad().map(|e| view! { <span class="hint bad">"Not well-formed XML yet: " {e}</span> })}
                <span class="hint">"<cutline>: the caption, in italics. Empty removes it. Ctrl+Enter applies."</span>
            </label>
            {bar(d, busy, do_apply)}
        </div>
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct Grid {
    heads: Vec<String>,
    rows: Vec<Vec<String>>,
}

#[component]
fn TableForm(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>) -> impl IntoView {
    let base = Memo::new(move |_| {
        view.with(|v| v.as_ref().map(|v| json(&Grid { heads: v.heads.clone(), rows: v.rows.clone() })).unwrap_or_default())
    });
    let d = Draft::new(tab, format!("table:{}", at.get_value().child), base);
    let g = move || d.model::<Grid>();
    let busy = Signal::derive(move || tab.applying.get());
    let bad = move || {
        g().rows.iter().flatten().find_map(|c| inline::parse(c).err()).map(|e| e.message)
    };
    let do_apply = move || {
        if d.dirty() && bad().is_none() {
            let Grid { heads, rows } = g();
            edit(tab, FamilyEdit::Table { at: at.get_value(), heads, rows });
        }
    };
    let cols = move || g().heads.len();
    let add_row = move |_| {
        d.change(|x: &mut Grid| {
            let n = x.heads.len().max(1);
            // Number the roll column when the first column counts 1, 2, 3...
            let numbered = !x.rows.is_empty() && x.rows.iter().enumerate().all(|(i, r)| r.first().is_some_and(|c| c.trim() == (i + 1).to_string()));
            let mut row = vec![String::new(); n];
            if numbered {
                row[0] = (x.rows.len() + 1).to_string();
            }
            x.rows.push(row);
        })
    };
    let add_col = move |_| {
        d.change(|x: &mut Grid| {
            x.heads.push(String::new());
            for r in &mut x.rows {
                r.push(String::new());
            }
        })
    };
    view! {
        <div class="table-card" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" { ev.prevent_default(); do_apply() }
            }>
            <div class="table-card-title">
                "Distinction table"
                <span class="hint">"A roll table: headings are plain text, cells are XML (inline tags allowed). Enter applies; Esc reverts."</span>
            </div>
            <table class="dist-grid">
                <thead>
                    <tr>
                        {move || (0..cols()).map(|c| view! {
                            <th>
                                <input type="text" placeholder=if c == 0 { "1d6" } else { "Heading" } prop:value=move || g().heads.get(c).cloned().unwrap_or_default()
                                    on:input=move |ev| d.change(|x: &mut Grid| if let Some(h) = x.heads.get_mut(c) { *h = event_target_value(&ev) })/>
                                <button class="icon-btn danger" title="Delete this column" disabled=move || cols() <= 1
                                    on:click=move |_| d.change(|x: &mut Grid| {
                                        if x.heads.len() > 1 && c < x.heads.len() {
                                            x.heads.remove(c);
                                            for r in &mut x.rows {
                                                if c < r.len() {
                                                    r.remove(c);
                                                }
                                            }
                                        }
                                    })>"✕"</button>
                            </th>
                        }).collect_view()}
                        <th class="row-tools"></th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let rows = g().rows.len();
                        (0..rows).map(|r| view! {
                            <tr>
                                {(0..cols()).map(|c| view! {
                                    <td>
                                        <input type="text" prop:value=move || g().rows.get(r).and_then(|x| x.get(c)).cloned().unwrap_or_default()
                                            on:input=move |ev| d.change(|x: &mut Grid| if let Some(cell) = x.rows.get_mut(r).and_then(|x| x.get_mut(c)) { *cell = event_target_value(&ev) })/>
                                    </td>
                                }).collect_view()}
                                <td class="row-tools">
                                    <button class="icon-btn" title="Move up" disabled=r == 0
                                        on:click=move |_| d.change(|x: &mut Grid| if r > 0 { x.rows.swap(r, r - 1) })>"↑"</button>
                                    <button class="icon-btn" title="Move down" disabled=r + 1 >= rows
                                        on:click=move |_| d.change(|x: &mut Grid| if r + 1 < x.rows.len() { x.rows.swap(r, r + 1) })>"↓"</button>
                                    <button class="icon-btn danger" title="Delete this row" disabled=rows <= 1
                                        on:click=move |_| d.change(|x: &mut Grid| if x.rows.len() > 1 { x.rows.remove(r); })>"✕"</button>
                                </td>
                            </tr>
                        }).collect_view()
                    }}
                </tbody>
            </table>
            {move || bad().map(|e| view! { <p class="hint bad">"A cell isn't well-formed XML yet: " {e}</p> })}
            <div class="add-part">
                <button class="small" on:click=add_row>"+ Row"</button>
                <button class="small" on:click=add_col>"+ Column"</button>
                <span class="hint">"Rows and columns change in the draft; Apply writes the table."</span>
            </div>
            {bar(d, busy, do_apply)}
        </div>
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
struct LairFields {
    name: String,
    end_verb: String,
}

/// One lair effect: name, id and its text.
#[component]
fn EffectRow(tab: DocTab, at: StoredValue<PartAt>, n: usize, effects: Memo<Vec<EffectView>>) -> impl IntoView {
    let base = Memo::new(move |_| effects.with(|e| e.get(n).map(json).unwrap_or_default()));
    let count = Memo::new(move |_| effects.with(|e| e.len()));
    let d = Draft::new(tab, format!("eff:{}:{n}", at.get_value().child), base);
    let m = move || d.model::<EffectView>();
    let busy = Signal::derive(move || tab.applying.get());
    let bad = move || inline::parse(&m().text).err().map(|e| e.message);
    let el = NodeRef::<html::Div>::new();
    let do_apply = move || {
        if d.dirty() && bad().is_none() {
            let e = m();
            edit(tab, FamilyEdit::SetEffect { at: at.get_value(), n, id: e.id, name: e.name, text: e.text });
        }
    };
    view! {
        <div node_ref=el class="body-block para effect-row" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:focusout=move |ev: FocusEvent| if !focus_stays(&ev, &el) { do_apply() }
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" && ev.ctrl_key() { ev.prevent_default(); do_apply() }
            }>
            <div class="block-gutter"><span class="para-n">{n + 1}</span></div>
            <div class="block-main">
                <div class="field-row">
                    <label class="field grow">
                        <span class="field-label">"Effect name"</span>
                        <input type="text" placeholder="Home Ground" prop:value=move || m().name
                            on:input=move |ev| d.change(|x: &mut EffectView| x.name = event_target_value(&ev))/>
                    </label>
                    <label class="field id-field">
                        <span class="field-label">"Id"</span>
                        <input type="text" placeholder="home-ground" prop:value=move || m().id
                            on:input=move |ev| d.change(|x: &mut EffectView| x.id = event_target_value(&ev))/>
                    </label>
                </div>
                <textarea class="p-text" spellcheck="true" rows=2 placeholder="(no text) What the effect does"
                    prop:value=move || m().text on:input=move |ev| d.change(|x: &mut EffectView| x.text = event_target_value(&ev))></textarea>
                {move || bad().map(|e| view! { <p class="hint bad">"Not well-formed XML yet: " {e}</p> })}
                <div class="p-bar">
                    {move || d.failed().then(|| view! { <span class="state-badge bad">"refused — see Problems"</span> })}
                    {move || (d.dirty() && !d.failed()).then(|| view! { <span class="state-badge">"unapplied"</span> })}
                    <span class="spacer"></span>
                    <button class="icon-btn" title="Move up" disabled=move || { n == 0 || busy.get() }
                        on:click=move |_| edit(tab, FamilyEdit::MoveEffect { at: at.get_value(), n, before: Some(n - 1) })>"↑"</button>
                    <button class="icon-btn" title="Move down" disabled=move || { n + 1 >= count.get() || busy.get() }
                        on:click=move |_| {
                            let before = (n + 2 < count.get_untracked()).then_some(n + 2);
                            edit(tab, FamilyEdit::MoveEffect { at: at.get_value(), n, before })
                        }>"↓"</button>
                    <button class="small" title="Insert a new effect below" disabled=busy
                        on:click=move |_| edit(tab, FamilyEdit::AddEffect { at: at.get_value(), after: Some(n) })>"+ effect below"</button>
                    <button class="small" disabled=move || !d.dirty() || busy.get() on:click=move |_| d.revert()>"Revert"</button>
                    <button class="small primary" disabled=move || !d.dirty() || busy.get() || bad().is_some() on:click=move |_| do_apply()>"Apply"</button>
                    <button class="icon-btn danger" disabled=move || { busy.get() || count.get() <= 1 }
                        title=move || if count.get() <= 1 { "A lair needs at least one effect: delete the lair instead" } else { "Delete this effect" }
                        on:click=move |_: MouseEvent| {
                            if confirm(&format!("Delete the effect \u{201c}{}\u{201d}?", m().name)) {
                                edit(tab, FamilyEdit::DeleteEffect { at: at.get_value(), n });
                            }
                        }>"✕"</button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn LairForm(tab: DocTab, at: StoredValue<PartAt>, view: Memo<Option<PartView>>, fam: Memo<Option<Family>>) -> impl IntoView {
    let base = Memo::new(move |_| {
        view.with(|v| v.as_ref().map(|v| json(&LairFields { name: v.name.clone(), end_verb: v.end_verb.clone() })).unwrap_or_default())
    });
    let d = Draft::new(tab, format!("lair:{}", at.get_value().child), base);
    let m = move || d.model::<LairFields>();
    let busy = Signal::derive(move || tab.applying.get());
    let do_apply = move || {
        if d.dirty() {
            let f = m();
            edit(tab, FamilyEdit::Lair { at: at.get_value(), name: f.name, end_verb: f.end_verb });
        }
    };
    let noun = Memo::new(move |_| fam.with(|f| f.as_ref().map(|f| f.header.short_noun.clone()).unwrap_or_default()));
    let family_name = Memo::new(move |_| fam.with(|f| f.as_ref().map(|f| f.header.name.clone()).unwrap_or_default()));
    let named_by = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.named_by.clone()).unwrap_or_default()));
    let effects = Memo::new(move |_| view.with(|v| v.as_ref().map(|v| v.effects.clone()).unwrap_or_default()));
    let n_effects = Memo::new(move |_| effects.with(|e| e.len()));
    let has_description = Memo::new(move |_| view.with(|v| v.as_ref().is_some_and(|v| v.has_description)));
    view! {
        <div class="table-card" class:dirty=move || d.dirty() class:failed=move || d.failed()
            on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" { d.revert() } else if ev.key() == "Enter" { do_apply() }
            }>
            <div class="table-card-title">
                "Lair"
                <span class="hint">{move || {
                    let by = named_by.get();
                    if by.is_empty() {
                        "No stat block names it with <lairRef>: it belongs to the whole family and prints with the family's parts.".to_string()
                    } else {
                        format!("Named by {} with <lairRef>: it prints with the first of them.", by.join(", "))
                    }
                }}</span>
            </div>
            <div class="field-row">
                <label class="field grow">
                    <span class="field-label">"Heading"</span>
                    <input type="text" placeholder=move || format!("(none: \u{201c}{} Lair\u{201d})", family_name.get()) prop:value=move || m().name
                        on:input=move |ev| d.change(|x: &mut LairFields| x.name = event_target_value(&ev))/>
                    <span class="hint">"@name: the lair's heading. Empty prints the family's name and \u{201c}Lair\u{201d}."</span>
                </label>
                <label class="field">
                    <span class="field-label">"Effects end when it…"</span>
                    <input type="text" placeholder="(no closing line) dies" prop:value=move || m().end_verb
                        on:input=move |ev| d.change(|x: &mut LairFields| x.end_verb = event_target_value(&ev))/>
                    <span class="hint">{move || {
                        let v = m().end_verb;
                        if v.trim().is_empty() {
                            "<effectEndVerb>: empty prints no closing line.".to_string()
                        } else {
                            format!("Prints \u{201c}If the {} {}, these effects end.\u{201d}", noun.get(), v.trim())
                        }
                    }}</span>
                </label>
            </div>
            {bar(d, busy, do_apply)}
        </div>
        {move || if has_description.get() {
            view! { <Paras tab at view min_one=false title="Description" hint="The lair's <description>: deleting its last paragraph removes it."/> }.into_any()
        } else {
            view! {
                <div class="table-card">
                    <div class="table-card-title">"Description" <span class="hint">"Optional: paragraphs before the effects."</span></div>
                    <div class="add-part">
                        <button class="small" disabled=move || busy.get() on:click=move |_| edit(tab, FamilyEdit::AddPara { at: at.get_value(), after: None })>"+ Add a description"</button>
                    </div>
                </div>
            }.into_any()
        }}
        <div class="table-card">
            <div class="table-card-title">
                {move || format!("Effects ({})", n_effects.get())}
                <span class="hint">"Each prints as a bold-italic name and its text. At least one; ids are unique across the file's lairs."</span>
            </div>
            <div class="block-body">
                <For each=move || 0..n_effects.get() key=|i| *i let:i>
                    <EffectRow tab at n=i effects/>
                </For>
            </div>
            <div class="add-part">
                <button class="small" disabled=move || busy.get() on:click=move |_| edit(tab, FamilyEdit::AddEffect { at: at.get_value(), after: None })>"+ effect first"</button>
            </div>
        </div>
    }
}

/// A part's form, by its root child index and tag.
#[component]
fn PartForm(tab: DocTab, fam: Memo<Option<Family>>, child: usize, tag: String) -> impl IntoView {
    let at = StoredValue::new(PartAt { child, tag: tag.clone() });
    let t2 = tag.clone();
    let view = Memo::new(move |_| fam.with(|f| f.as_ref().and_then(|f| f.parts.iter().find(|p| p.child == child && p.tag == t2).cloned())));
    let exists = Memo::new(move |_| view.with(|v| v.is_some()));
    let tag = StoredValue::new(tag);
    view! {
        {move || if !exists.get() {
            view! { <div class="empty"><p>"This part isn't in the file any more."</p></div> }.into_any()
        } else {
            let body = match tag.get_value().as_str() {
                "quoteBox" => view! { <QuoteForm tab at view/> }.into_any(),
                "image" => view! { <ImageForm tab at view/> }.into_any(),
                "description" => view! {
                    <Paras tab at view min_one=true title="Paragraphs" hint="The family's lore. At least one; each is its XML (inline tags like <shortNounRef/> and <i>)."/>
                }.into_any(),
                "distinctionTable" => view! { <TableForm tab at view/> }.into_any(),
                _ => view! { <LairForm tab at view fam/> }.into_any(),
            };
            view! {
                <PartBar tab at view fam/>
                {body}
            }.into_any()
        }}
    }
}

/// The file's own page: the header and the parts list.
#[component]
pub fn FamilyPage(tab: DocTab, fam: Memo<Option<Family>>) -> impl IntoView {
    view! {
        {move || if fam.with(|f| f.is_none()) {
            view! { <div class="empty"><p>"The family can't be read from this file."</p></div> }.into_any()
        } else {
            view! {
                <FamilyHeader tab fam/>
                <PartsList tab fam/>
            }.into_any()
        }}
    }
}

/// The header alone (a header element selected in the tree).
#[component]
pub fn HeaderPage(tab: DocTab, fam: Memo<Option<Family>>) -> impl IntoView {
    view! { <FamilyHeader tab fam/> }
}

#[component]
pub fn PartPage(tab: DocTab, fam: Memo<Option<Family>>, child: usize, tag: String) -> impl IntoView {
    view! { <PartForm tab fam child tag/> }
}
