//! A supplement's blocks after its chapters: `<credits>`, `<license>`, `<treasure>` (its
//! parcels) and `<data>` (global, per-chapter and per-section values). Each opens on its
//! own page from the tree; the supplement's own page offers to add the missing ones.
//!
//! Cards work as on the item page: a draft of one element, applied whole, written only
//! where it changed and only if the file validates.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use shared::statblock::{self as sb, El, StatEdit, ROOT};

use crate::dialogs::ask;
use crate::license_form::p_row;
use crate::state::DocTab;
use crate::statblock_form::{card_bar, edit, keys, kid_text, pick_in, text_in, Card, Loc, Rows};

/// The blocks this module edits, with what the Add buttons call them.
pub const PARTS: &[(&str, &str)] = &[("credits", "Credits"), ("license", "License"), ("treasure", "Treasure"), ("data", "Data values")];

const FLOATS: &[&str] = &["none", "bottom"];

/// A new, empty block of this kind.
pub fn new_part(tag: &str) -> El {
    match tag {
        "license" => El::new("license").with("file", "dmsguild-license.xml"),
        t => El::new(t),
    }
}

/// The root's first child with this tag, as a card.
fn part_card(tab: DocTab, tag: &'static str, norm: Option<fn(&mut El)>) -> (Card, Memo<Option<El>>) {
    let root = Memo::new(move |_| sb::read(&tab.doc().source, ROOT));
    let el = Memo::new(move |_| root.with(|m| m.as_ref().and_then(|m| m.kid(tag).cloned())));
    (Card::new(tab, ROOT, el, None, norm), el)
}

fn frame(c: Card, title: &'static str, hint: &'static str, body: AnyView) -> AnyView {
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="table-card-title">{title} <span class="hint">{hint}</span></div>
            {body}
            {card_bar(c)}
        </div>
    }
    .into_any()
}

/// Spacing and placement shared by credit lines.
fn style_fields(c: Card, l: Loc) -> AnyView {
    view! {
        {text_in(c, l, "padTop", "Space above", "(none) 6pt", "")}
        {text_in(c, l, "padBottom", "Space below", "(none)", "")}
        {pick_in(c, l, "float", FLOATS, Some("(none)"), "Float", "bottom: to the page's foot")}
    }
    .into_any()
}

/// An optional single child with a checkbox: present or not.
fn toggle(c: Card, tag: &'static str, label: &'static str, make: fn() -> El) -> impl IntoView {
    view! {
        <label class="flag">
            <input type="checkbox" prop:checked=move || c.m().kid(tag).is_some() on:change=move |ev| {
                let on = event_target_checked(&ev);
                c.change(|e| if on { if e.kid(tag).is_none() { e.insert_kid(make()); } } else { e.remove_kid(tag) })
            }/>
            {label}
        </label>
    }
}

// ---- credits ------------------------------------------------------------------------

fn role_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "name", "Role", "Writer", "")}{text_in(c, l, "plural", "Plural", "(Writers)", "When several have it")}</div> }.into_any()
}

fn role_style_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "name", "Role", "Writer", "Which role's lines")}{style_fields(c, l)}</div> }.into_any()
}

fn credit_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "name", "Name", "Jeff Yurkiw", "")}{text_in(c, l, "role", "Role", "Writer", "One of the roles")}</div> }.into_any()
}

fn isbn_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {text_in(c, l, "value", "ISBN", "978-1-23456-789-7", "ISBN-10 or -13, with or without hyphens")}
            {pick_in(c, l, "format", &["print", "hardcover", "paperback", "pdf", "epub", "ebook"], Some("(none)"), "Format", "")}
        </div>
    }
    .into_any()
}

fn verbatim_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "", "Line", "Printed as written", "")}{style_fields(c, l)}</div> }.into_any()
}

#[component]
pub fn CreditsPage(tab: DocTab) -> impl IntoView {
    let (c, _) = part_card(tab, "credits", None);
    let r = Loc::ROOT;
    let lic = r.at("license", 0);
    let cr = r.at("copyright", 0);
    let ic = r.at("imageCredits", 0);
    let body = view! {
        <div class="field-row">{text_in(c, r, "title", "Heading", "(Credits)", "")}</div>
        <Rows c loc=r tag="role" title="Roles" hint="The roles credit lines name, in print order." make=|_| El::new("role").with("name", "Writer") row=role_row/>
        <Rows c loc=r tag="roleStyle" title="Role styles" hint="Spacing for one role's lines." make=|_| El::new("roleStyle").with("name", "Writer") row=role_style_row/>
        <Rows c loc=r tag="credit" title="Credits" make=|_| El::new("credit").with("name", "Name").with("role", "Writer") row=credit_row/>
        <Rows c loc=r tag="verbatimCredit" title="Lines printed as written" make=|_| El::new("verbatimCredit").with_text("Special thanks to …") row=verbatim_row/>
        <Rows c loc=r tag="isbn" title="ISBNs" make=|_| El::new("isbn").with("value", "9780000000000") row=isbn_row/>
        <div class="field-row">
            {toggle(c, "imageCredits", "Image credits (each image's title and artist)", || El::new("imageCredits"))}
        </div>
        {move || c.m().kid("imageCredits").is_some().then(|| view! { <div class="field-row">{style_fields(c, ic)}</div> })}
        <div class="field-row">{toggle(c, "license", "A license notice in the credits", || El::new("license").with("file", "dmsguild-license.xml"))}</div>
        {move || c.m().kid("license").is_some().then(|| view! {
            <div class="sb-nested">
                <div class="field-row">
                    {text_in(c, lic, "file", "License file", "dmsguild-license.xml", "In the schemas folder")}
                    {text_in(c, lic, "date", "Date", "2026", "Fills the notice's \u{2039}date\u{203a}")}
                    {text_in(c, lic, "ownedBy", "Owner", "Enigma Narrative Studios", "Fills \u{2039}owner\u{203a}")}
                </div>
                <div class="field-row">{style_fields(c, lic)}</div>
                <Rows c loc=lic tag="p" title="Extra paragraphs" make=|_| El::new("p") row=p_row/>
            </div>
        })}
        <div class="field-row">{toggle(c, "copyright", "A copyright line", || El::new("copyright"))}</div>
        {move || c.m().kid("copyright").is_some().then(|| view! {
            <div class="field-row">
                {text_in(c, cr, "", "Text", "(the default line)", "")}
                {text_in(c, cr, "year", "Year", "2026", "")}
                {text_in(c, cr, "holder", "Holder", "Enigma Narrative Studios", "")}
            </div>
        })}
    }
    .into_any();
    frame(c, "Credits", "The credits page: roles, names, ISBNs, image credits, a license notice and a copyright line.", body)
}

// ---- license ------------------------------------------------------------------------

#[component]
pub fn LicensePartPage(tab: DocTab) -> impl IntoView {
    let (c, _) = part_card(tab, "license", None);
    let r = Loc::ROOT;
    let body = view! {
        <div class="field-row">
            {text_in(c, r, "file", "License file", "dmsguild-license.xml", "Its notice text, from the schemas folder")}
            {text_in(c, r, "date", "Date", "2026", "Fills the notice's \u{2039}date\u{203a}")}
            {text_in(c, r, "ownedBy", "Owner", "Enigma Narrative Studios", "Fills \u{2039}owner\u{203a}")}
        </div>
        <div class="field-row">
            {text_in(c, r, "title", "Heading", "(Legal)", "")}
            {pick_in(c, r, "columns", &["1", "2"], Some("(2)"), "Columns", "1: the notice across the full width")}
        </div>
        <Rows c loc=r tag="p" title="Extra paragraphs" hint="Printed after the file's notice." make=|_| El::new("p") row=p_row/>
    }
    .into_any();
    frame(c, "License", "The legal page.", body)
}

// ---- treasure -----------------------------------------------------------------------

/// A parcel's empty groups go.
fn norm_parcel(e: &mut El) {
    for t in ["coins", "gems", "art", "magicItems", "other"] {
        if e.kid(t).is_some_and(|k| k.kids.is_empty()) {
            e.remove_kid(t);
        }
    }
}

/// One coin's count: empty removes it.
fn coin_field(c: Card, coin: &'static str) -> impl IntoView {
    view! {
        <label class="field small-field">
            <span class="field-label">{coin.to_uppercase()}</span>
            <input type="text" placeholder="0" prop:value=move || c.m().kid("coins").and_then(|k| k.kid(coin)).map(|x| x.get("count").to_string()).unwrap_or_default()
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    c.change(|e| {
                        if v.trim().is_empty() {
                            if let Some(k) = e.kid_mut("coins") {
                                k.remove_kid(coin);
                            }
                        } else {
                            e.ensure("coins").ensure(coin).set("count", v.trim());
                        }
                    })
                }/>
        </label>
    }
}

fn worth_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "", "What", "a 50 gp moonstone", "")}{text_in(c, l, "cp", "Worth (cp)", "5000", "")}</div> }.into_any()
}

fn magic_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {text_in(c, l, "ref", "Item entry", "(none) flame-tongue", "A <reference> item id")}
            {text_in(c, l, "bookReference", "Book", "(none)", "Or a book's item")}
            {text_in(c, l, "rarity", "Rarity", "(none)", "")}
            {text_in(c, l, "cp", "Worth (cp)", "", "")}
        </div>
        <div class="field-row">
            {kid_text(c, l, "name", false, "Name", "(from the entry)", "")}
            {kid_text(c, l, "description", false, "Description", "(none)", "")}
        </div>
    }
    .into_any()
}

fn other_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {kid_text(c, l, "name", true, "Name", "Silk rope", "")}
            {kid_text(c, l, "description", true, "Description", "50 feet, knotted", "")}
            {text_in(c, l, "cp", "Worth (cp)", "", "")}
        </div>
    }
    .into_any()
}

#[component]
fn ParcelCard(tab: DocTab, root: Memo<Option<El>>, path: Vec<usize>) -> impl IntoView {
    let p = StoredValue::new(path.clone());
    let el = Memo::new(move |_| root.with(|m| m.as_ref().and_then(|m| m.walk().into_iter().find(|e| e.path.as_deref() == Some(&p.get_value()[..])).cloned())));
    let c = Card::new(tab, ROOT, el, None, Some(norm_parcel));
    let r = Loc::ROOT;
    let busy = move || tab.applying.get();
    let delete = move |_: MouseEvent| {
        ask(format!("Delete parcel {}?", el.get_untracked().map(|e| e.get("number").to_string()).unwrap_or_default()), move || {
            edit(tab, StatEdit::Delete { child: ROOT, path: p.get_value(), tag: "parcel".into() })
        });
    };
    let me = *path.last().unwrap_or(&0);
    let up = path[..path.len() - 1].to_vec();
    let move_up = move |_: MouseEvent| {
        let host = root.with_untracked(|m| m.as_ref().and_then(|m| m.walk().into_iter().find(|e| e.path.as_deref() == Some(&up[..])).cloned()));
        let sibs: Vec<usize> = host.map(|h| h.all("parcel").iter().filter_map(|k| k.path.as_ref()?.last().copied()).collect()).unwrap_or_default();
        if let Some(i) = sibs.iter().position(|x| *x == me).filter(|i| *i > 0) {
            edit(tab, StatEdit::Move { child: ROOT, path: p.get_value(), tag: "parcel".into(), before: Some(sibs[i - 1]) });
        }
    };
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="table-card-title">
                {move || format!("Parcel {}", c.m().get("number"))}
                <span class="spacer"></span>
                <button class="icon-btn" title="Move up" disabled=busy on:click=move_up>"↑"</button>
                <button class="icon-btn danger" title="Delete this parcel" disabled=busy on:click=delete>"✕"</button>
            </div>
            <div class="field-row">
                {text_in(c, r, "number", "Number", "1", "What <parcelLink number> names")}
            </div>
            <div class="field-row coin-row">
                {coin_field(c, "pp")}{coin_field(c, "ep")}{coin_field(c, "gp")}{coin_field(c, "sp")}{coin_field(c, "cp")}
            </div>
            <Rows c loc=r container="gems" tag="gem" title="Gems" make=|_| El::new("gem").with_text("a gem") row=worth_row/>
            <Rows c loc=r container="art" tag="piece" title="Art objects" make=|_| El::new("piece").with_text("an art object") row=worth_row/>
            <Rows c loc=r container="magicItems" tag="item" title="Magic items" make=|_| El::new("item") row=magic_row/>
            <Rows c loc=r container="other" tag="item" title="Other things"
                make=|_| El::new("item").with_kid(El::new("name").with_text("Item")).with_kid(El::new("description").with_text("")) row=other_row/>
            <div class="field-row">{kid_text(c, r, "description", false, "Notes", "(none)", "Where it is, how it's hidden")}</div>
            {card_bar(c)}
        </div>
    }
}

#[component]
pub fn TreasurePage(tab: DocTab) -> impl IntoView {
    let root = Memo::new(move |_| sb::read(&tab.doc().source, ROOT));
    let host = Memo::new(move |_| root.with(|m| m.as_ref().and_then(|m| m.kid("treasure").cloned())));
    let parcels = Memo::new(move |_| host.with(|h| h.as_ref().map(|h| h.all("parcel").iter().filter_map(|p| p.path.clone()).collect::<Vec<_>>()).unwrap_or_default()));
    let busy = move || tab.applying.get();
    let add = move |_: MouseEvent| {
        let Some(h) = host.get_untracked() else { return };
        let n = h.all("parcel").iter().filter_map(|p| p.get("number").parse::<u32>().ok()).max().unwrap_or(0) + 1;
        let after = h.kids.iter().rev().find(|k| k.tag == "parcel").and_then(|k| k.path.as_ref()?.last().copied());
        edit(tab, StatEdit::Insert { child: ROOT, parent: h.path.clone().unwrap_or_default(), parent_tag: "treasure".into(), el: El::new("parcel").with("number", &n.to_string()), after });
    };
    view! {
        <div class="sb-page">
            <div class="table-card">
                <div class="table-card-title">"Treasure" <span class="hint">"The supplement's parcels; a section's <parcelLink number> prints one."</span></div>
                <div class="add-part"><button class="small" disabled=busy on:click=add>"+ Parcel"</button></div>
            </div>
            <For each=move || parcels.get() key=|p| p.clone() let:path>
                <ParcelCard tab root path/>
            </For>
        </div>
    }
}

// ---- data ---------------------------------------------------------------------------

fn norm_data(e: &mut El) {
    if e.kid("global").is_some_and(|g| g.kids.is_empty()) {
        e.remove_kid("global");
    }
}

fn value_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "name", "Name", "dc", "What <dataRef> names")}{text_in(c, l, "value", "Value", "13", "")}</div> }.into_any()
}

fn section_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">{text_in(c, l, "name", "Section", "intro-opening", "A section's name in that chapter")}</div>
        <Rows c loc=l tag="value" title="Values" make=|_| El::new("value").with("name", "name").with("value", "") row=value_row/>
    }
    .into_any()
}

fn chapter_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">{text_in(c, l, "name", "Chapter", "introduction", "A chapter's name")}</div>
        <Rows c loc=l tag="section" title="Sections" make=|_| El::new("section").with("name", "section") row=section_row/>
    }
    .into_any()
}

#[component]
pub fn DataPage(tab: DocTab) -> impl IntoView {
    let (c, _) = part_card(tab, "data", Some(norm_data));
    let r = Loc::ROOT;
    let body = view! {
        <Rows c loc=r container="global" tag="value" title="Global values" hint="Anywhere in the supplement." make=|_| El::new("value").with("name", "name").with("value", "") row=value_row/>
        <Rows c loc=r tag="chapter" title="Per chapter and section" hint="Values that only hold in one section." make=|_| El::new("chapter").with("name", "chapter") row=chapter_row/>
    }
    .into_any();
    frame(c, "Data values", "Named values the supplement's data refs can print.", body)
}

/// The page for a block, by its tag.
#[component]
pub fn PartPage(tab: DocTab, tag: String) -> impl IntoView {
    let page = match tag.as_str() {
        "credits" => view! { <CreditsPage tab/> }.into_any(),
        "license" => view! { <LicensePartPage tab/> }.into_any(),
        "treasure" => view! { <TreasurePage tab/> }.into_any(),
        _ => view! { <DataPage tab/> }.into_any(),
    };
    view! { <div class="sb-page">{page}</div> }
}

/// "Add credits", "Add a license"... for the blocks the supplement lacks.
#[component]
pub fn AddParts(tab: DocTab) -> impl IntoView {
    let missing = Memo::new(move |_| {
        let d = tab.doc();
        let root = d.nodes.first();
        // The product description is offered too, but edited in the section editor.
        PARTS.iter().chain(&[("productDescription", "Product description")])
            .filter(|(t, _)| root.is_some_and(|r| !r.children.iter().any(|c| d.nodes[*c].tag == *t))).copied().collect::<Vec<_>>()
    });
    let busy = move || tab.applying.get();
    view! {
        <Show when=move || missing.with(|m| !m.is_empty())>
            <div class="notice info">
                "Add:"
                {move || missing.get().into_iter().map(|(t, label)| view! {
                    <button disabled=busy on:click=move |_| edit(tab, StatEdit::Insert { child: ROOT, parent: vec![], parent_tag: "supplement".into(), el: new_part(t), after: None })>{label}</button>
                }).collect_view()}
            </div>
        </Show>
    }
}
