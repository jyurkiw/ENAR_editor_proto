//! A spell's page: one page, no sub-pages. The header (name, id, level, school,
//! ritual, classes), the casting lines (casting times, range and area, components with
//! materials, duration), the description's paragraphs, lists and tables as cards, At
//! Higher Levels, and the `<reference>` entries (monster and magic item stat blocks the
//! spell links to).
//!
//! Cards work as on the item page: a draft of one element, applied whole, written only
//! where it changed and only if the file validates. Prose boxes are chip editors.

use leptos::prelude::*;
use shared::spell;
use shared::statblock::{self as sb, El, ROOT};

use crate::item_form::{prose, prose_kept, provide_refs, BlockList};
use crate::reference_form::ReferenceForm;
use crate::state::DocTab;
use crate::statblock_form::{card_bar, keys, kid_text, pick_in, scope, scoped, text_in, Card, Loc, Rows, Section};

/// A checkbox for a child holding "true" or "false" (`<ritual>`, `<verbal>`...).
fn kid_bool(c: Card, loc: Loc, tag: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <label class="flag">
            <input type="checkbox" prop:checked=move || c.m().at(&loc.s[..loc.n]).is_some_and(|e| e.opt_text(tag).trim() == "true")
                on:change=move |ev| {
                    let v = event_target_checked(&ev);
                    c.at(loc, |e| e.ensure(tag).text = if v { "true".into() } else { "false".into() })
                }/>
            {label}
        </label>
    }
}

/// A card's frame: keys, dirty and refused marks, and Revert / Apply.
fn frame(c: Card, title: &'static str, hint: &'static str, body: AnyView) -> AnyView {
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            {(!title.is_empty()).then(|| view! { <div class="table-card-title">{title} <span class="hint">{hint}</span></div> })}
            {body}
            {card_bar(c)}
        </div>
    }
    .into_any()
}

/// A card for the root's child `tag`, edited whole.
fn kid_card(tab: DocTab, sp: Memo<Option<El>>, tag: &'static str) -> Card {
    let el = Memo::new(move |_| sp.with(|m| m.as_ref().and_then(|m| m.kid(tag).cloned())));
    Card::new(tab, ROOT, el, None, None)
}

const HEADER: &[&str] = &["name", "level", "school", "ritual", "classes"];

#[component]
fn HeaderCard(tab: DocTab, sp: Memo<Option<El>>, refs: crate::statblock_form::RefCtx) -> impl IntoView {
    let el = Memo::new(move |_| sp.with(|m| m.as_ref().map(|m| scoped(m, HEADER))));
    let c = Card::new(tab, ROOT, el, scope(HEADER), None);
    let r = Loc::ROOT;
    let creator = move || c.m().get("creator").to_string();
    let body = view! {
        {prose(c, r, Some("name"), "Name", "<name>: may pull its creator's name with an entry ref")}
        <div class="field-row">
            {text_in(c, r, "id", "Id", "fireball", "Unique; other files name the spell by it")}
            {text_in(c, r, "src", "Source", "(original)", "e.g. 5.2.1 SRD")}
            <label class="field">
                <span class="field-label">"Creator"</span>
                <select prop:value=creator on:change=move |ev| { let v = event_target_value(&ev); c.change(|e| e.set("creator", &v)) }>
                    <option value="">"(none)"</option>
                    {move || refs.get().into_iter().map(|(t, id, name)| view! { <option value=id.clone()>{format!("{name} ({t} {id})")}</option> }).collect_view()}
                </select>
                <span class="hint">"A <reference> entry"</span>
            </label>
        </div>
        <div class="field-row">
            {pick_in(c, r.at("level", 0), "", spell::LEVELS, None, "Level", "0 is a cantrip")}
            {pick_in(c, r.at("school", 0), "", spell::SCHOOLS, None, "School", "")}
            <div class="field flags">{kid_bool(c, r, "ritual", "Ritual (adds \u{201c}or Ritual\u{201d})")}</div>
        </div>
        <Rows c loc=r container="classes" tag="class" title="Classes" min=1 make=|_| El::new("class").with_text("Wizard")
            row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "", "Class", "Wizard", "")}</div> }.into_any()/>
    }
    .into_any();
    frame(c, "", "", body)
}

fn casting_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {kid_text(c, l, "value", true, "Number", "1", "Empty for an action")}
            {pick_in(c, l.at("unit", 0), "", spell::CASTING_UNITS, None, "Unit", "")}
            {text_in(c, l, "note", "Note", "(none)", "In parentheses after it")}
        </div>
        {prose_kept(c, l, "trigger", "Trigger", "Reactions need one; bonus actions may have one. Completes \u{201c}which you take …\u{201d}.")}
    }
    .into_any()
}

#[component]
fn CastingCard(tab: DocTab, sp: Memo<Option<El>>) -> impl IntoView {
    let c = kid_card(tab, sp, "castingTimes");
    let body = view! {
        <Rows c loc=Loc::ROOT tag="castingTime" title="Casting times" hint="Joined with \u{201c}or\u{201d}." min=1
            make=|_| El::new("castingTime").with_kid(El::new("value").with_text("1")).with_kid(El::new("unit").with_text("action")).with_kid(El::new("trigger"))
            row=casting_row/>
    }
    .into_any();
    frame(c, "", "", body)
}

#[component]
fn RangeCard(tab: DocTab, sp: Memo<Option<El>>) -> impl IntoView {
    let c = kid_card(tab, sp, "range");
    let r = Loc::ROOT;
    let area = r.at("area", 0);
    // An empty <area/> means none: only a shaped one counts.
    let has_area = move || c.m().kid("area").is_some_and(|a| !a.get("name").is_empty());
    let body = view! {
        <div class="field-row">
            {pick_in(c, r.at("type", 0), "", spell::RANGE_KINDS, None, "Range", "")}
            {kid_text(c, r, "distance", true, "Distance", "(none)", "For ranged; empty otherwise")}
            {pick_in(c, r.at("distance", 0), "unit", spell::DISTANCE_UNITS, Some("(feet)"), "Unit", "")}
        </div>
        <label class="flag">
            <input type="checkbox" prop:checked=has_area on:change=move |ev| {
                let on = event_target_checked(&ev);
                c.change(|e| if on {
                    e.ensure("area").set("name", "sphere")
                } else if let Some(a) = e.kid_mut("area") {
                    // Back to an empty area (the file's own way of saying none).
                    a.attrs.clear();
                })
            }/>
            "An area after the range: \u{201c}Self (15-foot Cone)\u{201d}"
        </label>
        {move || has_area().then(|| view! {
            <div class="field-row">
                {pick_in(c, area, "name", sb::AREAS, Some("(none)"), "Shape", "")}
                {text_in(c, area, "size", "Size", "", "cone, cube, square…")}
                {text_in(c, area, "radius", "Radius", "", "")}
                {text_in(c, area, "diameter", "Diameter", "", "")}
            </div>
            <div class="field-row">
                {text_in(c, area, "length", "Length", "", "")}
                {text_in(c, area, "width", "Width", "", "")}
                {text_in(c, area, "height", "Height", "", "")}
                {text_in(c, area, "thickness", "Thickness", "", "")}
                {pick_in(c, area, "unit", sb::AREA_UNITS, Some("(feet)"), "Unit", "")}
                {pick_in(c, area, "thicknessUnit", sb::AREA_UNITS, Some("(feet)"), "Thickness unit", "")}
            </div>
        })}
    }
    .into_any();
    frame(c, "", "", body)
}

fn material_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        {prose_kept(c, l, "description", "Material", "\u{201c}a ruby worth 50+ GP\u{201d}: what's printed inside M (…)")}
        <div class="field-row">
            {kid_text(c, l, "cost", true, "Cost", "0", "0 for none")}
            {pick_in(c, l.at("cost", 0), "currency", spell::CURRENCIES, Some("(gp)"), "Currency", "")}
            <div class="field flags">{kid_bool(c, l, "consumed", "Consumed")}</div>
        </div>
    }
    .into_any()
}

#[component]
fn ComponentsCard(tab: DocTab, sp: Memo<Option<El>>) -> impl IntoView {
    let c = kid_card(tab, sp, "components");
    let r = Loc::ROOT;
    let body = view! {
        <div class="field flags">
            {kid_bool(c, r, "verbal", "Verbal")}
            {kid_bool(c, r, "somatic", "Somatic")}
        </div>
        <Rows c loc=r container="materials" tag="material" title="Materials" hint="None: no Material component. Several print inside one M (…), joined with \u{201c}and\u{201d}."
            make=|_| El::new("material").with_kid(El::new("description")).with_kid(El::new("cost").with_text("0")).with_kid(El::new("consumed").with_text("false"))
            row=material_row/>
    }
    .into_any();
    frame(c, "", "", body)
}

#[component]
fn DurationCard(tab: DocTab, sp: Memo<Option<El>>) -> impl IntoView {
    let c = kid_card(tab, sp, "duration");
    let r = Loc::ROOT;
    let body = view! {
        <div class="field-row">
            <div class="field flags">{kid_bool(c, r, "concentration", "Concentration")}</div>
            {kid_text(c, r, "value", true, "Number", "(none)", "Empty for Instantaneous and the like")}
            {pick_in(c, r.at("unit", 0), "", spell::DURATION_UNITS, None, "Unit", "up-to-: \u{201c}Up to 1 minute\u{201d}")}
        </div>
    }
    .into_any();
    frame(c, "", "", body)
}

#[component]
fn HigherCard(tab: DocTab, sp: Memo<Option<El>>) -> impl IntoView {
    let c = kid_card(tab, sp, "higherLevels");
    let r = Loc::ROOT;
    let body = view! {
        {prose_kept(c, r, "cantripUpgrades", "Cantrip upgrade", "For a cantrip: what changes at higher character levels. Empty for none.")}
        {prose_kept(c, r, "slotUpgrades", "Using a higher-level spell slot", "Empty for none.")}
    }
    .into_any();
    frame(c, "", "", body)
}

/// The spell file's page.
#[component]
pub fn SpellPage(tab: DocTab) -> impl IntoView {
    let sp = Memo::new(move |_| sb::read(&tab.doc().source, ROOT).filter(|m| m.tag == "spell"));
    let refs = provide_refs(sp);
    crate::prose_editor::provide_env(tab, shared::prose::Palette::Spell, None);
    let focus = Memo::new(move |_| {
        let sel = tab.selected.get();
        tab.doc().key_of(sel)
    });
    let at = move |tags: &'static [&'static str]| {
        Signal::derive(move || {
            let f = focus.get();
            !f.is_empty()
                && sp.with(|m| m.as_ref().is_some_and(|m| m.kids.iter().filter(|k| tags.contains(&k.tag.as_str())).any(|k| k.path.as_ref().is_some_and(|p| f.starts_with(p)))))
        })
    };
    let desc_path = Memo::new(move |_| sp.with(|m| m.as_ref().and_then(|m| m.kid("description")).and_then(|d| d.path.clone())));
    view! {
        {move || if sp.with(|m| m.is_none()) {
            view! { <div class="empty"><p>"This spell can't be read."</p></div> }.into_any()
        } else {
            view! {
                <div class="sb-page">
                    <Section title="Spell" hint="Name, level, school, classes" focus=at(HEADER)>
                        <HeaderCard tab sp refs/>
                    </Section>
                    <Section title="Casting" hint="Casting time, range, components, duration" focus=at(&["castingTimes", "range", "components", "duration"])>
                        <CastingCard tab sp/>
                        <RangeCard tab sp/>
                        <ComponentsCard tab sp/>
                        <DurationCard tab sp/>
                    </Section>
                    <Section title="Description" hint="Paragraphs, lists and tables, in print order" focus=at(&["description"])>
                        {move || desc_path.get().map(|path| view! { <BlockList tab it=sp path parent="description" focus kinds=spell::BLOCKS/> })}
                    </Section>
                    <Section title="At Higher Levels" focus=at(&["higherLevels"])>
                        <HigherCard tab sp/>
                    </Section>
                    <Section title="Reference" hint="The monster and magic item stat blocks the spell links to" focus=at(&["reference"])>
                        <ReferenceForm tab/>
                    </Section>
                </div>
            }.into_any()
        }}
    }
}
