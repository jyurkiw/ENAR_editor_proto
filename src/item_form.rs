//! A magic item's page: the header (name, id, status, type, subtypes, rarities,
//! attunement), the description's blocks as cards (paragraphs, abilities, lists, tables,
//! sidebars, stat blocks; abilities and sidebars hold blocks of their own), and the
//! `<reference>` entries, in the supplement's reference editor.
//!
//! Cards work as on the stat block page (see [`crate::statblock_form`]): a draft of one
//! element, applied whole, written only where it changed and only if the file validates.
//! Prose boxes have the Insert menu with the item schema's tags, and the supplement's
//! entry ref tools and live preview.

use leptos::ev::MouseEvent;
use leptos::html;
use leptos::prelude::*;
use shared::inline::RefCtx as Preview;
use shared::item;
use shared::statblock::{self as sb, El, StatEdit, ROOT};

use crate::family_form::confirm;
use crate::reference_form::ReferenceForm;
use crate::state::DocTab;
use crate::statblock_form::{
    card_bar, edit, flag_in, keys, pick_in, scope, scoped, text_in, Card, Loc, Prose, RefCtx, Rows, Section,
};

/// What every prose box on the page needs: the entry list for ref pickers and the
/// preview's context. Given by context so row builders (plain fns) can reach it.
#[derive(Clone, Copy)]
struct Refs {
    refs: RefCtx,
    preview: Memo<Preview>,
}

fn refs() -> Refs {
    expect_context::<Refs>()
}

fn find(m: &El, path: &[usize]) -> Option<El> {
    m.walk().into_iter().find(|e| e.path.as_deref() == Some(path)).cloned()
}

/// A prose box with the item's tags and the entry ref tools: an optional child `tag` of
/// the element at `loc`, or (None) that element's own text.
fn prose(c: Card, loc: Loc, tag: Option<&'static str>, label: &'static str, hint: &'static str) -> AnyView {
    let Refs { refs, preview } = refs();
    match tag {
        Some(t) => view! { <Prose c loc tag=t label hint refs tags=item::INLINE preview/> }.into_any(),
        None => view! { <Prose c loc label hint refs tags=item::INLINE preview/> }.into_any(),
    }
}

// ---- header ------------------------------------------------------------------------

const HEADER: &[&str] = &["name", "type", "subtypes", "rarities", "attunement"];

fn norm_header(e: &mut El) {
    if let Some(a) = e.kid_mut("attunement") {
        if a.kid("attunementRequirements").is_some_and(|r| r.kids.is_empty()) {
            a.remove_kid("attunementRequirements");
        }
    }
}

fn subtype_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "", "Subtype", "Longsword", "")}</div> }.into_any()
}

fn rarity_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">
            {pick_in(c, l, "", item::RARITY_VALUES, None, "Rarity", "")}
            {text_in(c, l, "note", "Note", "(none) +1", "In parentheses after it")}
        </div>
    }
    .into_any()
}

fn requirement_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! { <div class="field-row">{text_in(c, l, "", "By", "Cleric", "\u{201c}a\u{201d}/\u{201c}an\u{201d} is added")}</div> }.into_any()
}

#[component]
fn HeaderCard(tab: DocTab, it: Memo<Option<El>>) -> impl IntoView {
    let el = Memo::new(move |_| it.with(|m| m.as_ref().map(|m| scoped(m, HEADER))));
    let c = Card::new(tab, ROOT, el, scope(HEADER), Some(norm_header));
    let r = Loc::ROOT;
    let att = r.at("attunement", 0);
    let required = move || c.m().kid("attunement").is_some_and(|a| a.flag("required", false));
    let Refs { refs, .. } = refs();
    let creator = move || c.m().get("creator").to_string();
    view! {
        <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            {prose(c, r, Some("name"), "Name", "<name>: may pull a creator's name with an entry ref")}
            <div class="field-row">
                {text_in(c, r, "id", "Id", "flame-tongue", "Unique; other files name the item by it")}
                {pick_in(c, r, "status", item::STATUSES, Some("(draft)"), "Status", "")}
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
                {pick_in(c, r.at("type", 0), "", item::TYPE_VALUES, None, "Type", "")}
            </div>
            <Rows c loc=r container="subtypes" tag="subtype" title="Subtypes" hint="In parentheses after the type, joined with \u{201c}or\u{201d}."
                make=|_| El::new("subtype") row=subtype_row/>
            <Rows c loc=r container="rarities" tag="rarity" title="Rarities" hint="\u{201c}Uncommon (+1), Rare (+2), or Very Rare (+3)\u{201d}" min=1
                make=|_| El::new("rarity").with_text("uncommon") row=rarity_row/>
            <div class="field-row">
                {flag_in(c, att, "required", false, "Requires attunement")}
            </div>
            {move || required().then(|| view! {
                <Rows c loc=att container="attunementRequirements" tag="attunementRequirement" title="Attunement by"
                    hint="None: just \u{201c}(Requires Attunement)\u{201d}." make=|_| El::new("attunementRequirement") row=requirement_row/>
            })}
            {card_bar(c)}
        </div>
    }
}

// ---- description blocks -------------------------------------------------------------

/// A new block of this kind, ready to edit.
fn new_block(m: &El, tag: &str) -> El {
    match tag {
        "ability" => El::new("ability").with("name", "New Ability").with_kid(El::new("p")),
        "list" => El::new("list").with_kid(El::new("li")),
        "table" => El::new("table")
            .with_kid(El::new("header").with_kid(El::new("th").with_text("d6")).with_kid(El::new("th")))
            .with_kid(El::new("tableBody").with_kid(El::new("tr").with_kid(El::new("td").with_text("1")).with_kid(El::new("td")))),
        "sidebar" => El::new("sidebar").with_kid(El::new("title").with_text("Sidebar")).with_kid(El::new("p")),
        "monsterStatblock" => {
            let first = m.kid("reference").and_then(|r| r.all("monster").first().map(|e| e.get("id").to_string())).unwrap_or_default();
            El::new("monsterStatblock").with("ref", &first)
        }
        _ => El::new("p"),
    }
}

fn is_block(parent: &str, tag: &str) -> bool {
    item::blocks_in(parent).contains(&tag)
}

/// The blocks in the element at `path` (the description, an ability or a sidebar), as
/// cards, with Add buttons.
#[component]
fn BlockList(tab: DocTab, it: Memo<Option<El>>, path: Vec<usize>, parent: &'static str, focus: Memo<Vec<usize>>) -> impl IntoView {
    let host_path = StoredValue::new(path);
    let host = Memo::new(move |_| it.with(|m| m.as_ref().and_then(|m| find(m, &host_path.get_value()))));
    let blocks = Memo::new(move |_| {
        host.with(|h| h.as_ref().map(|h| h.kids.iter().filter(|k| is_block(parent, &k.tag)).filter_map(|k| k.path.clone()).collect::<Vec<_>>()).unwrap_or_default())
    });
    let busy = move || tab.applying.get();
    let no_monsters = move || it.with(|m| m.as_ref().and_then(|m| m.kid("reference")).is_none_or(|r| r.all("monster").is_empty()));
    let add = move |tag: &'static str| {
        let Some(m) = it.get_untracked() else { return };
        let Some(h) = host.get_untracked() else { return };
        let after = h.kids.iter().rev().find(|k| is_block(parent, &k.tag)).and_then(|k| k.path.as_ref()?.last().copied());
        edit(tab, StatEdit::Insert { child: ROOT, parent: host_path.get_value(), parent_tag: parent.into(), el: new_block(&m, tag), after });
    };
    view! {
        <div class="sb-items">
            <For each=move || blocks.get() key=|p| p.clone() let:path>
                <BlockCard tab it path parent count=Signal::derive(move || blocks.with(|b| b.len())) focus/>
            </For>
        </div>
        <div class="add-part">
            {item::blocks_in(parent).iter().map(|t| {
                let t: &'static str = t;
                let statblock = t == "monsterStatblock";
                view! {
                    <button class="small" disabled=move || busy() || (statblock && no_monsters())
                        title=if statblock { "Prints a <reference><monster> entry's stat block" } else { "" }
                        on:click=move |_| add(t)>{format!("+ {}", item::block_label(t))}</button>
                }
            }).collect_view()}
        </div>
    }
}

/// One block: its line (kind, summary, move, delete) and its form.
#[component]
fn BlockCard(tab: DocTab, it: Memo<Option<El>>, path: Vec<usize>, parent: &'static str, count: Signal<usize>, focus: Memo<Vec<usize>>) -> impl IntoView {
    let p = StoredValue::new(path.clone());
    let el = Memo::new(move |_| it.with(|m| m.as_ref().and_then(|m| find(m, &p.get_value()))));
    let tag = el.with_untracked(|e| e.as_ref().map(|e| e.tag.clone()).unwrap_or_default());
    let me = *path.last().unwrap_or(&0);
    let up = StoredValue::new(path[..path.len().saturating_sub(1)].to_vec());
    // Siblings of the same kind (blocks), by child index.
    let sibs = Memo::new(move |_| {
        it.with(|m| {
            let m = m.as_ref()?;
            let h = find(m, &up.get_value())?;
            Some(h.kids.iter().filter(|k| is_block(parent, &k.tag)).filter_map(|k| k.path.as_ref()?.last().copied()).collect::<Vec<_>>())
        })
        .unwrap_or_default()
    });
    let pos = move || sibs.with(|s| s.iter().position(|x| *x == me));
    let busy = move || tab.applying.get();
    let t2 = tag.clone();
    let mv = move |before: Option<usize>| edit(tab, StatEdit::Move { child: ROOT, path: p.get_value(), tag: t2.clone(), before });
    let mv2 = mv.clone();
    let up_btn = move |ev: MouseEvent| {
        ev.prevent_default();
        if let Some(i) = pos().filter(|i| *i > 0) {
            mv(sibs.with_untracked(|s| s.get(i - 1).copied()));
        }
    };
    let down_btn = move |ev: MouseEvent| {
        ev.prevent_default();
        if let Some(i) = pos() {
            mv2(sibs.with_untracked(|s| s.get(i + 2).copied()));
        }
    };
    let t3 = tag.clone();
    let delete = move |ev: MouseEvent| {
        ev.prevent_default();
        if confirm(&format!("Delete this {}?", item::block_label(&t3).to_lowercase())) {
            edit(tab, StatEdit::Delete { child: ROOT, path: p.get_value(), tag: t3.clone() });
        }
    };
    let summary = move || el.with(|e| e.as_ref().map(summary_of).unwrap_or_default());
    let details = NodeRef::<html::Details>::new();
    let p4 = path.clone();
    Effect::new(move |_| {
        let f = focus.get();
        if !f.is_empty() && f.starts_with(&p4) && f.len() <= p4.len() + 1 {
            if let Some(d) = details.get() {
                d.set_open(true);
                d.scroll_into_view();
            }
        }
    });
    let body = match tag.as_str() {
        "p" => para_form(tab, el),
        "ability" => ability_form(tab, it, el, path.clone(), focus),
        "list" => list_form(tab, el),
        "table" => table_form(tab, el),
        "sidebar" => sidebar_form(tab, it, el, path.clone(), focus),
        "monsterStatblock" => statblock_form(tab, it, el),
        _ => view! { <p class="hint">"Edit this block in the Source view."</p> }.into_any(),
    };
    let only = move || parent == "description" && count.get() <= 1;
    view! {
        <details node_ref=details class="sb-item" open=true>
            <summary class="sb-item-line">
                <span class="part-kind">{item::block_label(&tag)}</span>
                <span class="sb-item-name part-summary">{summary}</span>
                <span class="spacer"></span>
                <button class="icon-btn" title="Move up" disabled=move || pos().is_none_or(|i| i == 0) || busy() on:click=up_btn>"↑"</button>
                <button class="icon-btn" title="Move down" disabled=move || pos().is_none_or(|i| i + 1 >= sibs.with(|s| s.len())) || busy() on:click=down_btn>"↓"</button>
                <button class="icon-btn danger" disabled=move || busy() || only()
                    title=move || if only() { "The description needs at least one block" } else { "Delete" }
                    on:click=delete>"✕"</button>
            </summary>
            <div class="sb-item-body">{body}</div>
        </details>
    }
}

/// One line about a block, for its card's title.
fn summary_of(e: &El) -> String {
    let snip = |s: &str| {
        let plain: String = strip_tags(s);
        let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        if plain.chars().count() > 70 { format!("{}…", plain.chars().take(70).collect::<String>()) } else { plain }
    };
    match e.tag.as_str() {
        "p" => snip(&e.text),
        "ability" => e.get("name").to_string(),
        "list" => format!("{} item(s)", e.all("li").len()),
        "table" => e.kid("title").map(|t| snip(&t.text)).filter(|t| !t.is_empty()).unwrap_or_else(|| format!("{} row(s)", e.kid("tableBody").map_or(0, |b| b.all("tr").len()))),
        "sidebar" => e.kid("title").map(|t| snip(&t.text)).unwrap_or_default(),
        "monsterStatblock" => e.get("ref").to_string(),
        _ => String::new(),
    }
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for ch in s.chars() {
        match ch {
            '<' => inside = true,
            '>' => {
                inside = false;
                out.push(' ');
            }
            _ if !inside => out.push(ch),
            _ => {}
        }
    }
    out
}

fn para_form(tab: DocTab, el: Memo<Option<El>>) -> AnyView {
    let c = Card::new(tab, ROOT, el, None, None);
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            {prose(c, Loc::ROOT, None, "Text", "")}
            {card_bar(c)}
        </div>
    }
    .into_any()
}

fn ability_form(tab: DocTab, it: Memo<Option<El>>, el: Memo<Option<El>>, path: Vec<usize>, focus: Memo<Vec<usize>>) -> AnyView {
    // The ability's own attributes; its blocks are cards of their own below.
    let head = Memo::new(move |_| el.with(|e| e.as_ref().map(|e| scoped(e, &[]))));
    let c = Card::new(tab, ROOT, head, scope(&[]), None);
    let r = Loc::ROOT;
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="field-row">
                {text_in(c, r, "name", "Name", "Regaining Charges", "Runs in at the start of its first paragraph")}
                {flag_in(c, r, "indent", false, "Indent its blocks")}
            </div>
            {card_bar(c)}
        </div>
        <div class="sb-nested">
            <BlockList tab it path parent="ability" focus/>
            <p class="hint">"The first block must be a paragraph."</p>
        </div>
    }
    .into_any()
}

fn sidebar_form(tab: DocTab, it: Memo<Option<El>>, el: Memo<Option<El>>, path: Vec<usize>, focus: Memo<Vec<usize>>) -> AnyView {
    const HEAD: &[&str] = &["title", "subtitle"];
    let head = Memo::new(move |_| el.with(|e| e.as_ref().map(|e| scoped(e, HEAD))));
    let c = Card::new(tab, ROOT, head, scope(HEAD), None);
    let r = Loc::ROOT;
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            {prose(c, r, Some("title"), "Title", "Empty leaves it out")}
            {prose(c, r, Some("subtitle"), "Subtitle", "Empty leaves it out")}
            <div class="field-row">
                {text_in(c, r, "id", "Id", "(none)", "")}
                {text_in(c, r, "style", "Style", "(default)", "A renderer style name")}
            </div>
            {card_bar(c)}
        </div>
        <div class="sb-nested">
            <BlockList tab it path parent="sidebar" focus/>
        </div>
    }
    .into_any()
}

fn li_row(c: Card, l: Loc, _i: usize) -> AnyView {
    prose(c, l, None, "Item", "")
}

fn list_form(tab: DocTab, el: Memo<Option<El>>) -> AnyView {
    let c = Card::new(tab, ROOT, el, None, None);
    let r = Loc::ROOT;
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="field-row">
                {pick_in(c, r, "bullet", &["bullet", "number"], Some("(bullet)"), "Marker", "")}
                {text_in(c, r, "style", "Style", "(default)", "")}
            </div>
            <Rows c loc=r tag="li" title="Items" min=1 make=|_| El::new("li") row=li_row/>
            {card_bar(c)}
        </div>
    }
    .into_any()
}

fn norm_table(e: &mut El) {
    for t in ["header", "footer"] {
        if e.kid(t).is_some_and(|k| k.kids.is_empty()) {
            e.remove_kid(t);
        }
    }
}

fn new_row(m: &El) -> El {
    let mut tr = El::new("tr");
    for _ in 0..table_width(m) {
        tr.kids.push(El::new("td"));
    }
    tr
}

/// Columns: the header's cells, else the widest row.
fn table_width(m: &El) -> usize {
    let rows = ["tableBody", "footer"].iter().filter_map(|s| m.kid(s)).flat_map(|s| s.all("tr")).map(|r| r.all("td").len()).max().unwrap_or(0);
    m.kid("header").map(|h| h.all("th").len()).unwrap_or(0).max(rows).max(1)
}

/// The rows of one table section ("tableBody" or "footer") in the draft.
fn rows_mut<'a>(e: &'a mut El, section: &str) -> Vec<&'a mut El> {
    match e.kid_mut(section) {
        Some(s) => s.kids.iter_mut().filter(|k| k.tag == "tr").collect(),
        None => Vec::new(),
    }
}

/// One cell: a one-line box of its XML. A row short of cells is padded when typed into.
fn cell_input(c: Card, section: &'static str, row: Option<usize>, col: usize) -> impl IntoView {
    let get = move || {
        let m = c.m();
        let cell = match row {
            None => m.kid("header").and_then(|h| h.all("th").get(col).map(|x| x.text.clone())),
            Some(i) => m.kid(section).and_then(|s| s.all("tr").get(i).and_then(|r| r.all("td").get(col).map(|x| x.text.clone()))),
        };
        cell.unwrap_or_default()
    };
    view! {
        <input type="text" class="cell" prop:value=get on:input=move |ev| {
            let v = event_target_value(&ev);
            c.change(|e| {
                let (host, tag) = match row {
                    None => (e.kid_mut("header"), "th"),
                    Some(i) => (e.kid_mut(section).and_then(|s| s.nth_mut("tr", i)), "td"),
                };
                if let Some(h) = host {
                    while h.all(tag).len() <= col {
                        h.kids.push(El::new(tag));
                    }
                    if let Some(x) = h.nth_mut(tag, col) {
                        x.text = v;
                    }
                }
            })
        }/>
    }
}

/// A section's rows as grid rows, with move and delete.
fn grid_rows(c: Card, section: &'static str, count: Memo<usize>, cols: Memo<usize>, min: usize) -> impl IntoView {
    move || {
        let n = count.get();
        (0..n).map(|i| {
            let swap = move |a: usize, b: usize| c.change(|e| {
                if let Some(s) = e.kid_mut(section) {
                    let idx: Vec<usize> = s.kids.iter().enumerate().filter(|(_, k)| k.tag == "tr").map(|(x, _)| x).collect();
                    if let (Some(&x), Some(&y)) = (idx.get(a), idx.get(b)) {
                        s.kids.swap(x, y);
                    }
                }
            });
            view! {
                <tr>
                    {move || (0..cols.get()).map(|j| view! { <td>{cell_input(c, section, Some(i), j)}</td> }).collect_view()}
                    <td class="row-tools">
                        <button type="button" class="icon-btn" title="Move up" disabled={i == 0} on:click=move |_| swap(i, i - 1)>"↑"</button>
                        <button type="button" class="icon-btn" title="Move down" disabled={i + 1 >= n} on:click=move |_| swap(i, i + 1)>"↓"</button>
                        <button type="button" class="icon-btn danger" title="Delete this row" disabled={n <= min}
                            on:click=move |_| c.change(|e| {
                                if let Some(s) = e.kid_mut(section) {
                                    let mut k = 0;
                                    s.kids.retain(|x| {
                                        if x.tag != "tr" { return true; }
                                        k += 1;
                                        k - 1 != i
                                    });
                                }
                            })>"✕"</button>
                    </td>
                </tr>
            }
        }).collect_view()
    }
}

/// A table's header, body and footer as one grid of cell boxes.
#[component]
fn TableGrid(c: Card) -> impl IntoView {
    let cols = Memo::new(move |_| table_width(&c.m()));
    let has_header = Memo::new(move |_| c.m().kid("header").is_some());
    let body_n = Memo::new(move |_| c.m().kid("tableBody").map_or(0, |b| b.all("tr").len()));
    let foot_n = Memo::new(move |_| c.m().kid("footer").map_or(0, |b| b.all("tr").len()));
    let add_row = move |section: &'static str| c.change(|e| {
        let tr = new_row(e);
        e.ensure(section).kids.push(tr);
    });
    let add_col = move |_| c.change(|e| {
        let w = table_width(e);
        if let Some(h) = e.kid_mut("header") {
            while h.all("th").len() < w + 1 {
                h.kids.push(El::new("th"));
            }
        }
        for s in ["tableBody", "footer"] {
            for r in rows_mut(e, s) {
                while r.all("td").len() < w + 1 {
                    r.kids.push(El::new("td"));
                }
            }
        }
    });
    let del_col = move |j: usize| c.change(|e| {
        let drop = |h: &mut El, tag: &str| {
            let mut k = 0;
            h.kids.retain(|x| {
                if x.tag != tag { return true; }
                k += 1;
                k - 1 != j
            });
        };
        if let Some(h) = e.kid_mut("header") {
            drop(h, "th");
        }
        for s in ["tableBody", "footer"] {
            for r in rows_mut(e, s) {
                drop(r, "td");
            }
        }
    });
    let toggle_header = move |_| c.change(|e| {
        if e.kid("header").is_some() {
            e.remove_kid("header");
        } else {
            let mut h = El::new("header");
            for _ in 0..table_width(e) {
                h.kids.push(El::new("th"));
            }
            e.insert_kid(h);
        }
    });
    view! {
        <table class="dist-grid item-grid">
            {move || has_header.get().then(|| view! {
                <thead>
                    <tr>
                        {move || (0..cols.get()).map(|j| view! {
                            <th>
                                {cell_input(c, "header", None, j)}
                                <button type="button" class="icon-btn danger" title="Delete this column" disabled={cols.get_untracked() <= 1}
                                    on:click=move |_| del_col(j)>"✕"</button>
                            </th>
                        }).collect_view()}
                        <th class="row-tools"></th>
                    </tr>
                </thead>
            })}
            <tbody>{grid_rows(c, "tableBody", body_n, cols, 1)}</tbody>
            {move || (foot_n.get() > 0).then(|| view! {
                <tfoot>{grid_rows(c, "footer", foot_n, cols, 0)}</tfoot>
            })}
        </table>
        <div class="add-part">
            <button type="button" class="small" on:click=move |_| add_row("tableBody")>"+ Row"</button>
            <button type="button" class="small" on:click=add_col>"+ Column"</button>
            <button type="button" class="small" on:click=move |_| add_row("footer")>"+ Footer row"</button>
            <button type="button" class="small" on:click=toggle_header>{move || if has_header.get() { "Remove header row" } else { "+ Header row" }}</button>
            <span class="hint">"Cells are XML (inline tags allowed); rowspan and colspan are kept as written."</span>
        </div>
    }
}

fn table_form(tab: DocTab, el: Memo<Option<El>>) -> AnyView {
    let c = Card::new(tab, ROOT, el, None, Some(norm_table));
    let r = Loc::ROOT;
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            {prose(c, r, Some("title"), "Title", "Bold, just above the table. Empty leaves it out.")}
            <div class="field-row">
                {text_in(c, r, "id", "Id", "(none)", "")}
                {text_in(c, r, "style", "Style", "(default)", "")}
            </div>
            <TableGrid c/>
            {card_bar(c)}
        </div>
    }
    .into_any()
}

fn statblock_form(tab: DocTab, it: Memo<Option<El>>, el: Memo<Option<El>>) -> AnyView {
    let c = Card::new(tab, ROOT, el, None, None);
    let r = Loc::ROOT;
    let monsters = Memo::new(move |_| {
        it.with(|m| m.as_ref().and_then(|m| m.kid("reference")).map(|r| r.all("monster").iter().map(|e| e.get("id").to_string()).collect::<Vec<_>>()).unwrap_or_default())
    });
    view! {
        <div class="sb-card-body" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
            <div class="field-row">
                <label class="field">
                    <span class="field-label">"Monster"</span>
                    <select prop:value=move || c.m().get("ref").to_string() on:change=move |ev| { let v = event_target_value(&ev); c.change(|e| e.set("ref", &v)) }>
                        {move || monsters.get().into_iter().map(|id| view! { <option value=id.clone()>{id.clone()}</option> }).collect_view()}
                    </select>
                    <span class="hint">"A <reference><monster> entry with a file and a stat block"</span>
                </label>
                {pick_in(c, r, "float", &["none", "left", "right"], Some("(none)"), "Float", "left/right: a short stat block beside the text")}
            </div>
            {card_bar(c)}
        </div>
    }
    .into_any()
}

/// The magic item file's page.
#[component]
pub fn ItemPage(tab: DocTab) -> impl IntoView {
    let it = Memo::new(move |_| sb::read(&tab.doc().source, ROOT).filter(|m| m.tag == "magicItem"));
    let entries = Memo::new(move |_| {
        it.with(|m| {
            m.as_ref()
                .and_then(|m| m.kid("reference"))
                .map(|r| {
                    r.kids
                        .iter()
                        .filter(|e| !e.get("id").is_empty())
                        .map(|e| {
                            let name = if e.get("name").is_empty() { e.get("id") } else { e.get("name") };
                            (e.tag.clone(), e.get("id").to_string(), name.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        })
    });
    let preview = Memo::new(move |_| {
        let d = tab.doc();
        let index = tab.refs.with(|r| r.as_ref().and_then(|r| r.as_ref().ok().cloned()));
        Preview::from_doc(&d.source, &d.nodes, index)
    });
    provide_context(Refs { refs: entries, preview });
    // The tree's selection, as a path from the root.
    let focus = Memo::new(move |_| {
        let sel = tab.selected.get();
        tab.doc().key_of(sel)
    });
    let at = move |tag: &'static str| {
        Signal::derive(move || {
            let f = focus.get();
            it.with(|m| m.as_ref().and_then(|m| m.kid(tag)).and_then(|k| k.path.clone())).is_some_and(|p| !f.is_empty() && f.starts_with(&p))
        })
    };
    let header_focus = Signal::derive(move || {
        let f = focus.get();
        it.with(|m| m.as_ref().is_some_and(|m| m.kids.iter().filter(|k| HEADER.contains(&k.tag.as_str())).any(|k| k.path.as_ref().is_some_and(|p| f.starts_with(p)))))
    });
    let desc_path = Memo::new(move |_| it.with(|m| m.as_ref().and_then(|m| m.kid("description")).and_then(|d| d.path.clone())));
    view! {
        {move || if it.with(|m| m.is_none()) {
            view! { <div class="empty"><p>"This magic item can't be read."</p></div> }.into_any()
        } else {
            view! {
                <div class="sb-page">
                    <Section title="Item" hint="Name, type, rarity and attunement" focus=header_focus>
                        <HeaderCard tab it/>
                    </Section>
                    <Section title="Description" hint="Paragraphs, abilities, lists, tables, sidebars and stat blocks, in print order" focus=at("description")>
                        {move || desc_path.get().map(|path| view! { <BlockList tab it path parent="description" focus/> })}
                    </Section>
                    <Section title="Reference" hint="Monsters, spells, items and data the item names or pulls in" focus=at("reference")>
                        <ReferenceForm tab/>
                    </Section>
                </div>
            }.into_any()
        }}
    }
}
