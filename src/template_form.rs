//! Template stat blocks (`<monster template="true">`, see monsters/TEMPLATES.md): the
//! `<formulas>` block (named formulas, each a sum of terms, and choices with their
//! options), and the formula and choice fields beside a stat block's numbers. Those
//! fields show only on a template; on any other stat block the compiler refuses them.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use shared::statblock::{El, StatEdit};

use crate::state::DocTab;
use crate::statblock_form::{card_bar, edit, keys, pick_in, text_in, Card, Loc, Rows};

const SUMMONER: &[&str] = &["proficiencyBonus", "spellAttack", "spellSaveDC", "abilityModifier", "level", "classLevel", "hitPointMaximum", "challengeRating"];
const LOOKUP: &[&str] = &["challengeRating", "level", "classLevel", "spellLevel", "proficiencyBonus"];
const STATS: &[&str] = &["str", "dex", "con", "int", "wis", "cha"];

/// What a stat block's template fields need: whether it's a template, its formula ids,
/// its choices (id, options) and its trait ids.
#[derive(Clone, Copy)]
pub(crate) struct Tpl {
    pub on: Memo<bool>,
    pub formulas: Memo<Vec<String>>,
    pub choices: Memo<Vec<(String, Vec<String>)>>,
    pub traits: Memo<Vec<String>>,
}

pub(crate) fn provide_tpl(mon: Memo<Option<El>>) -> Tpl {
    let on = Memo::new(move |_| mon.with(|m| m.as_ref().is_some_and(|m| m.flag("template", false))));
    let formulas = Memo::new(move |_| {
        mon.with(|m| m.as_ref().and_then(|m| m.kid("formulas")).map(|f| f.all("formula").iter().map(|x| x.get("id").to_string()).collect()).unwrap_or_default())
    });
    let choices = Memo::new(move |_| {
        mon.with(|m| {
            m.as_ref()
                .and_then(|m| m.kid("formulas"))
                .map(|f| f.all("choice").iter().map(|c| (c.get("id").to_string(), c.all("option").iter().map(|o| o.get("value").to_string()).collect())).collect())
                .unwrap_or_default()
        })
    });
    let traits = Memo::new(move |_| {
        mon.with(|m| m.as_ref().and_then(|m| m.kid("traits")).map(|t| t.all("trait").iter().map(|x| x.get("id").to_string()).filter(|i| !i.is_empty()).collect()).unwrap_or_default())
    });
    let t = Tpl { on, formulas, choices, traits };
    provide_context(t);
    t
}

fn tpl() -> Option<Tpl> {
    use_context::<Tpl>()
}

/// A select of ids for an attribute of the element at `loc`, shown only on a template.
fn id_pick(c: Card, loc: Loc, attr: &'static str, label: &'static str, hint: &'static str, ids: fn(Tpl) -> Vec<String>) -> AnyView {
    let Some(t) = tpl() else { return ().into_any() };
    let cur = move || c.m().at(&loc.s[..loc.n]).map(|e| e.get(attr).to_string()).unwrap_or_default();
    view! {
        <Show when=move || t.on.get()>
            <label class="field tpl-field">
                <span class="field-label">{label}</span>
                <select prop:value=cur on:change=move |ev| { let v = event_target_value(&ev); c.at(loc, |e| e.set(attr, &v)) }>
                    <option value="">"(none)"</option>
                    {move || {
                        let mut list = ids(t);
                        let v = cur();
                        if !v.is_empty() && !list.contains(&v) {
                            list.insert(0, v);
                        }
                        list.into_iter().map(|i| view! { <option value=i.clone()>{i.clone()}</option> }).collect_view()
                    }}
                </select>
                {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
            </label>
        </Show>
    }
    .into_any()
}

/// A formula for an attribute of the element at `loc` (template only).
pub(crate) fn formula(c: Card, loc: Loc, attr: &'static str, label: &'static str) -> AnyView {
    id_pick(c, loc, attr, label, "Template: a formula from <formulas>", |t| t.formulas.get())
}

/// The formula of a heading number (`<armorClass formula>`, `<hitPoints formula>`...):
/// choosing one adds the element if it's missing; clearing it drops an element left empty.
pub(crate) fn kid_formula(c: Card, tag: &'static str, label: &'static str) -> AnyView {
    let Some(t) = tpl() else { return ().into_any() };
    let cur = move || c.m().kid(tag).map(|e| e.get("formula").to_string()).unwrap_or_default();
    view! {
        <Show when=move || t.on.get()>
            <label class="field tpl-field">
                <span class="field-label">{label}</span>
                <select prop:value=cur on:change=move |ev| {
                    let v = event_target_value(&ev);
                    c.change(|e| {
                        if !v.is_empty() {
                            e.ensure(tag).set("formula", &v);
                        } else if let Some(k) = e.kids.iter_mut().find(|k| k.tag == tag) {
                            k.set("formula", "");
                            if k.text.trim().is_empty() && k.attrs.is_empty() && !["armorClass", "proficiencyBonus"].contains(&tag) {
                                e.remove_kid(tag);
                            }
                        }
                    })
                }>
                    <option value="">"(none)"</option>
                    {move || {
                        let mut list = t.formulas.get();
                        let v = cur();
                        if !v.is_empty() && !list.contains(&v) {
                            list.insert(0, v);
                        }
                        list.into_iter().map(|i| view! { <option value=i.clone()>{i.clone()}</option> }).collect_view()
                    }}
                </select>
            </label>
        </Show>
    }
    .into_any()
}

/// A choice for an attribute of the element at `loc` (template only).
pub(crate) fn choice(c: Card, loc: Loc, attr: &'static str, label: &'static str) -> AnyView {
    id_pick(c, loc, attr, label, "Template: a choice from <formulas>", |t| t.choices.get().into_iter().map(|(i, _)| i).collect())
}

/// The trait that explains a scaling score (template only).
pub(crate) fn trait_ref(c: Card, loc: Loc) -> AnyView {
    id_pick(c, loc, "traitRef", "Explained by", "", |t| t.traits.get())
}

/// A variant's choice and option: printed only for that option (template only).
pub(crate) fn variant(c: Card, loc: Loc) -> AnyView {
    let Some(t) = tpl() else { return ().into_any() };
    let options = move || {
        let ch = c.m().at(&loc.s[..loc.n]).map(|e| e.get("choice").to_string()).unwrap_or_default();
        t.choices.get().into_iter().find(|(i, _)| *i == ch).map(|(_, o)| o).unwrap_or_default()
    };
    let cur = move || c.m().at(&loc.s[..loc.n]).map(|e| e.get("option").to_string()).unwrap_or_default();
    view! {
        <Show when=move || t.on.get()>
            <div class="field-row">
                {choice(c, loc, "choice", "Variant of")}
                <label class="field tpl-field">
                    <span class="field-label">"Only for"</span>
                    <select prop:value=cur on:change=move |ev| { let v = event_target_value(&ev); c.at(loc, |e| e.set("option", &v)) }>
                        <option value="">"(every version)"</option>
                        {move || options().into_iter().map(|o| view! { <option value=o.clone()>{o.clone()}</option> }).collect_view()}
                    </select>
                    <span class="hint">"\u{201c}Eye Ray (Beholderkin Only)\u{201d}"</span>
                </label>
            </div>
        </Show>
    }
    .into_any()
}

/// Whether this stat block is a template (for lists that differ, like CR "none").
pub(crate) fn is_template() -> bool {
    tpl().is_some_and(|t| t.on.get())
}

// ---- the formulas block ---------------------------------------------------------------

const TERMS: &[(&str, &str)] = &[("constant", "Number"), ("summoner", "Summoner's value"), ("spellLevel", "Spell level"), ("lookup", "Lookup")];

fn scale_fields(c: Card, l: Loc) -> AnyView {
    view! {
        {text_in(c, l, "times", "Times", "", "")}
        {text_in(c, l, "above", "Above", "", "count only above this")}
        {text_in(c, l, "divide", "Divide by", "", "")}
        {pick_in(c, l, "round", &["down", "up"], Some("(down)"), "Round", "")}
    }
    .into_any()
}

fn step_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "from", "From", "0", "this value and up")}{text_in(c, l, "value", "Value", "1", "")}</div> }.into_any()
}

/// One term of a formula, by kind.
fn term_fields(c: Card, l: Loc, tag: &str) -> AnyView {
    match tag {
        "constant" => view! { <div class="field-row">{text_in(c, l, "value", "Value", "5", "")}</div> }.into_any(),
        "summoner" => view! {
            <div class="field-row">
                {pick_in(c, l, "value", SUMMONER, None, "Value", "")}
                {pick_in(c, l, "stat", STATS, Some("(none)"), "Ability", "for abilityModifier")}
                {text_in(c, l, "class", "Class", "(none)", "for classLevel")}
            </div>
            <div class="field-row">{scale_fields(c, l)}</div>
        }
        .into_any(),
        "spellLevel" => view! {
            <div class="field-row">{scale_fields(c, l)}{pick_in(c, l, "article", &["the", "this"], Some("(the)"), "Words", "\u{201c}this spell's level\u{201d}")}</div>
        }
        .into_any(),
        _ => view! {
            <div class="field-row">{pick_in(c, l, "source", LOOKUP, None, "By", "")}{text_in(c, l, "class", "Class", "(none)", "for classLevel")}</div>
            <Rows c loc=l tag="step" title="Steps" min=1 make=|_| El::new("step").with("from", "0").with("value", "1") row=step_row/>
        }
        .into_any(),
    }
}

fn new_term(tag: &str) -> El {
    match tag {
        "constant" => El::new("constant").with("value", "0"),
        "summoner" => El::new("summoner").with("value", "proficiencyBonus"),
        "lookup" => El::new("lookup").with("source", "challengeRating").with_kid(El::new("step").with("from", "0").with("value", "1")),
        t => El::new(t),
    }
}

/// A formula's terms: a sum, in order, of any kinds.
#[component]
fn Terms(c: Card, loc: Loc) -> impl IntoView {
    let kinds = Memo::new(move |_| c.m().at(&loc.s[..loc.n]).map(|f| f.kids.iter().map(|k| k.tag.clone()).collect::<Vec<_>>()).unwrap_or_default());
    let edit_kids = move |f: &dyn Fn(&mut Vec<El>)| c.at(loc, |e| f(&mut e.kids));
    view! {
        <div class="sb-rows">
            <div class="sb-rows-title">"Terms" <span class="hint">"Added together."</span></div>
            {move || {
                let ks = kinds.get();
                let n = ks.len();
                ks.into_iter().enumerate().map(|(i, tag)| {
                    let label = TERMS.iter().find(|(t, _)| *t == tag).map(|(_, l)| *l).unwrap_or("Term");
                    let l = loc.at(TERMS.iter().find(|(t, _)| *t == tag).map(|(t, _)| *t).unwrap_or("constant"), (0..i).filter(|j| kinds.with_untracked(|k| k.get(*j) == Some(&tag))).count());
                    view! {
                        <div class="sb-row">
                            <div class="sb-row-fields"><span class="part-kind">{label}</span>{term_fields(c, l, &tag)}</div>
                            <div class="sb-row-tools">
                                <button type="button" class="icon-btn" title="Move up" disabled={i == 0} on:click=move |_| edit_kids(&|k| if i > 0 && i < k.len() { k.swap(i, i - 1) })>"↑"</button>
                                <button type="button" class="icon-btn" title="Move down" disabled={i + 1 >= n} on:click=move |_| edit_kids(&|k| if i + 1 < k.len() { k.swap(i, i + 1) })>"↓"</button>
                                <button type="button" class="icon-btn danger" title="Remove" disabled={n <= 1} on:click=move |_| edit_kids(&|k| if i < k.len() { k.remove(i); })>"✕"</button>
                            </div>
                        </div>
                    }
                }).collect_view()
            }}
            <div class="add-part">
                {TERMS.iter().map(|(t, label)| {
                    let t: &'static str = t;
                    view! { <button type="button" class="small" on:click=move |_| edit_kids(&|k| k.push(new_term(t)))>{format!("+ {label}")}</button> }
                }).collect_view()}
            </div>
        </div>
    }
}

fn formula_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {text_in(c, l, "id", "Id", "ac", "What fields and <formulaRef> name")}
            {text_in(c, l, "min", "At least", "(none)", "")}
            {text_in(c, l, "max", "At most", "(none)", "")}
        </div>
        <Terms c loc=l/>
    }
    .into_any()
}

fn option_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "value", "Option", "fire", "damage types lowercase")}</div> }.into_any()
}

fn choice_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">{text_in(c, l, "id", "Id", "form", "What fields and <choiceRef> name")}</div>
        <Rows c loc=l tag="option" title="Options" min=2 make=|_| El::new("option").with("value", "option") row=option_row/>
    }
    .into_any()
}

/// The template's `<formulas>`: a button to add them, or their card.
#[component]
pub(crate) fn FormulasSection(tab: DocTab, child: usize, mon: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| mon.with(|m| m.as_ref().and_then(|m| m.kid("formulas").cloned())));
    let busy = move || tab.applying.get();
    let add = move |_: MouseEvent| {
        let f = El::new("formulas").with_kid(El::new("formula").with("id", "formula").with_kid(El::new("constant").with("value", "0")));
        edit(tab, StatEdit::Insert { child, parent: vec![], parent_tag: "monster".into(), el: f, after: None });
    };
    view! {
        {move || if el.with(|e| e.is_none()) {
            view! {
                <div class="add-part">
                    <button class="small" disabled=busy on:click=add>"+ Formulas"</button>
                    <span class="hint">"The named numbers a template's fields point at."</span>
                </div>
            }.into_any()
        } else {
            view! { <FormulasCard tab child el/> }.into_any()
        }}
    }
}

#[component]
fn FormulasCard(tab: DocTab, child: usize, el: Memo<Option<El>>) -> impl IntoView {
    let c = Card::new(tab, child, el, None, None);
    let r = Loc::ROOT;
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <Rows c loc=r tag="formula" title="Formulas" hint="Each a sum of terms; resolved from the summoner's stat block, or printed as words."
                make=|_| El::new("formula").with("id", "formula").with_kid(El::new("constant").with("value", "0")) row=formula_row/>
            <Rows c loc=r tag="choice" title="Choices" hint="What the summoner picks: a damage type, a form."
                make=|_| El::new("choice").with("id", "choice").with_kid(El::new("option").with("value", "a")).with_kid(El::new("option").with("value", "b")) row=choice_row/>
            {card_bar(c)}
        </div>
    }
}
