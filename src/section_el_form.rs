//! A section's other blocks, each on its own page: images, figures, table and sidebar
//! links, treasure links, stat blocks, monster entries, magic item and spell blocks, skill
//! check tables and page ends. The section's page lists them; clicking one opens it here.
//!
//! The page is a card on the block's element, applied whole and written only where it
//! changed, and only if the file validates. Pointers pick from what the file has: its
//! `<reference>` entries, its chapters' tables and sidebars, its treasure parcels. A table
//! or sidebar link can also make a new table or sidebar at its chapter's end.

use leptos::ev::MouseEvent;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::block::{BlockEdit, Container};
use shared::family::Outline;
pub use shared::section::{missing, targets};
use shared::section::{new_block, new_target, Targets};
use shared::statblock::{self as sb, El, StatEdit, ROOT};
use shared::Change;

use crate::apply::apply;
use crate::dialogs::ask;
use crate::prose_editor::ChipEditor;
use crate::state::DocTab;
use crate::statblock_form::{card_bar, edit, flag_in, keys, pick_in, text_in, Card, Loc, Rows};

pub use shared::block::SECTION_BLOCKS;

/// What a block is called on its page and in the add menu.
pub fn label(tag: &str) -> &'static str {
    match tag {
        "image" => "Image",
        "figure" => "Figure",
        "tableLink" => "Table link",
        "sidebarLink" => "Sidebar link",
        "parcelLink" => "Treasure link",
        "monsterStatblock" => "Stat block",
        "monsterEntry" => "Monster entry",
        "magicItemBlock" => "Magic item",
        "spellBlock" => "Spell",
        "skillcheckResultTable" => "Skill check table",
        "pageEnd" => "Page end",
        _ => "Block",
    }
}

fn blurb(tag: &str) -> &'static str {
    match tag {
        "image" => "A picture: a <reference> image by id, or a file of its own.",
        "figure" => "A numbered figure (\u{201c}Figure 3.02: name\u{201d}) around one image, table, sidebar, stat block, magic item or spell. <figureRef> names it.",
        "tableLink" => "Places one of the chapter's tables here.",
        "sidebarLink" => "Places one of the chapter's sidebars here.",
        "parcelLink" => "Prints a treasure parcel here.",
        "monsterStatblock" => "A <reference> monster's stat block.",
        "monsterEntry" => "A monster file printed whole, as a Monster Manual entry: its name, the family's parts, then each stat block with its own.",
        "magicItemBlock" => "A <reference> item printed in full. The entry needs a file.",
        "spellBlock" => "A <reference> spell printed in full. The entry needs a file.",
        "skillcheckResultTable" => "What a check reveals, by DC. A paragraph's <skillcheck table=\"true\"> leads into it.",
        "pageEnd" => "Ends the page here.",
        _ => "",
    }
}

/// Adds a block to a section (or a table or list to a sidebar), with what it needs first.
pub fn add(tab: DocTab, at: Container, after: Option<usize>, tag: &str) {
    let d = tab.doc_untracked();
    let Some(root) = sb::read(&d.source, ROOT) else { return };
    let chapter = match &at {
        Container::Section { chapter, .. } | Container::Sidebar { chapter, .. } => root.kids.iter().position(|k| k.tag == "chapter" && k.get("name") == chapter),
        // The description takes paragraphs and lists only: no section blocks.
        Container::Description => None,
    };
    let Some(chapter) = chapter else { return };
    let (mut pre, el) = match tag {
        "table" => (vec![], new_target("table", &sb::fresh("new-table", &sb::ids(&root)))),
        _ => match new_block(tag, &root, chapter) {
            Some(x) => x,
            None => return,
        },
    };
    let add = Change::Block { edit: BlockEdit::AddEl { at, after, el } };
    if pre.is_empty() {
        apply(tab, add);
    } else {
        pre.push(add);
        apply(tab, Change::Batch { changes: pre });
    }
}

// ---- fields ---------------------------------------------------------------------------

/// A select of (value, label) for an attribute; a value the list doesn't know stays.
fn opt_pick(c: Card, loc: Loc, k: &'static str, label: &'static str, hint: &'static str, opts: Signal<Vec<(String, String)>>, empty: Option<&'static str>) -> impl IntoView {
    let cur = move || c.m().at(&loc.s[..loc.n]).map(|e| e.get(k).to_string()).unwrap_or_default();
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <select prop:value=cur on:change=move |ev| { let v = event_target_value(&ev); c.at(loc, |e| e.set(k, &v)) }>
                {empty.map(|t| view! { <option value="">{t}</option> })}
                {move || {
                    let mut o = opts.get();
                    let v = cur();
                    if !v.is_empty() && !o.iter().any(|(x, _)| *x == v) {
                        o.insert(0, (v.clone(), format!("{v} (not in the file)")));
                    }
                    o.into_iter().map(|(v, l)| view! { <option value=v.clone()>{l}</option> }).collect_view()
                }}
            </select>
            {(!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> })}
        </label>
    }
}

fn two(v: Vec<(String, String, String)>) -> Vec<(String, String)> {
    v.into_iter().map(|(a, b, _)| (a, b)).collect()
}

/// What a block's fields need: the file's targets, the block's chapter, the tab.
#[derive(Clone, Copy)]
struct Ctx {
    tab: DocTab,
    t: Memo<Targets>,
    chapter: usize,
}

fn image_fields(c: Card, l: Loc, x: Ctx) -> AnyView {
    let own = RwSignal::new(c.m_untracked().at(&l.s[..l.n]).is_some_and(|e| !e.get("src").is_empty()));
    let imgs = Signal::derive(move || two(x.t.get().images));
    view! {
        <div class="field-row">
            <label class="flag"><input type="radio" prop:checked=move || !own.get() on:change=move |_| {
                own.set(false);
                let first = x.t.get_untracked().images.first().map(|i| i.0.clone()).unwrap_or_default();
                c.at(l, |e| { e.set("src", ""); e.set("id", &first); })
            }/>"A <reference> image"</label>
            <label class="flag"><input type="radio" prop:checked=move || own.get() on:change=move |_| {
                own.set(true);
                let id = sb::fresh("image", &x.t.get_untracked().ids);
                c.at(l, |e| { e.set("id", &id); if e.get("src").is_empty() { e.set("src", "image.png") } })
            }/>"Its own file"</label>
        </div>
        {move || if own.get() {
            view! {
                <div class="field-row">
                    {text_in(c, l, "src", "File", "art/gnoll.png", "Relative to the supplement")}
                    {text_in(c, l, "id", "Id", "gnoll-art", "No other section image with a file may use it, nor a <reference> entry")}
                </div>
            }.into_any()
        } else {
            view! {
                <div class="field-row">
                    {opt_pick(c, l, "id", "Image", "Its file, title and artist come from the entry; anything set below wins", imgs, None)}
                </div>
            }.into_any()
        }}
        <div class="field-row">
            {text_in(c, l, "name", "Title", "(none)", "For <imageCredits/>: \u{201c}Sneaking Rogue\u{201d}")}
            {text_in(c, l, "credit", "Artist", "(none)", "For <imageCredits/>")}
        </div>
        <div class="field-row">
            {text_in(c, l, "height", "Height", "(natural)", "4in, 10cm, or 45% of the text height")}
            {pick_in(c, l, "span", &["column", "both"], Some("(column)"), "Width", "both: across both columns")}
            {pick_in(c, l, "placement", &["here", "top", "bottom", "page"], Some("(here)"), "Placement", "Where on the page it floats to")}
            {text_in(c, l, "align", "Align", "(none)", "")}
        </div>
    }
    .into_any()
}

/// A table or sidebar link: pick one, or make a new one at the chapter's end.
fn link_fields(c: Card, l: Loc, x: Ctx, what: &'static str) -> AnyView {
    let opts = Signal::derive(move || if what == "table" { x.t.get().tables } else { x.t.get().sidebars });
    let make = move |_: MouseEvent| {
        let taken = x.t.get_untracked().ids;
        let def = sb::fresh(&format!("new-{what}"), &taken);
        let Some(id) = web_sys::window().and_then(|w| w.prompt_with_message_and_default(&format!("The new {what}'s id:"), &def).ok().flatten()) else { return };
        let id = id.trim().to_string();
        if id.is_empty() {
            return;
        }
        if taken.contains(&id) {
            let _ = web_sys::window().map(|w| w.alert_with_message(&format!("\u{201c}{id}\u{201d} is taken: tables, sidebars, reference monsters and data share their ids.")));
            return;
        }
        // The table first (the link's draft takes its id after): each validates alone.
        c.at(l, |e| e.set("id", &id));
        edit(x.tab, StatEdit::Insert { child: ROOT, parent: vec![x.chapter], parent_tag: "chapter".into(), el: new_target(what, &id), after: None });
    };
    let open = move |_: MouseEvent| {
        let id = c.m().at(&l.s[..l.n]).map(|e| e.get("id").to_string()).unwrap_or_default();
        let d = x.tab.doc_untracked();
        let found = d.nodes.iter().position(|n| n.tag == what && shared::edit::start_tag(&d.source, n.start).and_then(|t| t.value("id")).as_deref() == Some(id.as_str()));
        if let Some(i) = found {
            x.tab.reveal(i);
        }
    };
    view! {
        <div class="field-row">
            {opt_pick(c, l, "id", if what == "table" { "Table" } else { "Sidebar" }, "", opts, None)}
            <div class="field">
                <span class="field-label">" "</span>
                <div class="add-part">
                    <button type="button" class="small" disabled=move || x.tab.applying.get() on:click=make>{format!("New {what}\u{2026}")}</button>
                    <button type="button" class="small" on:click=open>{format!("Open the {what}")}</button>
                </div>
            </div>
        </div>
    }
    .into_any()
}

fn statblock_fields(c: Card, l: Loc, x: Ctx) -> AnyView {
    let mons = Signal::derive(move || two(x.t.get().monsters));
    view! {
        <div class="field-row">
            {opt_pick(c, l, "ref", "Monster", "A <reference> monster", mons, None)}
            {pick_in(c, l, "float", &["left", "right"], Some("(none)"), "Float", "Text flows beside it")}
            {pick_in(c, l, "columns", &["2"], Some("(1)"), "Columns", "2: a wide stat block in two columns")}
            {pick_in(c, l, "after", &["twoColumn", "fullWidth", "compressed"], Some("(as before)"), "After it", "How the text after it continues")}
        </div>
    }
    .into_any()
}

fn ref_fields(c: Card, l: Loc, x: Ctx, kind: &'static str) -> AnyView {
    let opts = Signal::derive(move || two(if kind == "item" { x.t.get().items } else { x.t.get().spells }));
    let lab = if kind == "item" { "Magic item" } else { "Spell" };
    view! { <div class="field-row">{opt_pick(c, l, "ref", lab, "A <reference> entry with a file", opts, None)}</div> }.into_any()
}

/// The fields of a figure's content (and of a block on its own).
fn kind_fields(c: Card, l: Loc, x: Ctx, tag: &str) -> AnyView {
    match tag {
        "image" => image_fields(c, l, x),
        "tableLink" => link_fields(c, l, x, "table"),
        "sidebarLink" => link_fields(c, l, x, "sidebar"),
        "monsterStatblock" => statblock_fields(c, l, x),
        "magicItemBlock" => ref_fields(c, l, x, "item"),
        "spellBlock" => ref_fields(c, l, x, "spell"),
        _ => view! { <p class="hint">"Nothing to set."</p> }.into_any(),
    }
}

const FIGURE_KINDS: &[&str] = &["image", "tableLink", "sidebarLink", "monsterStatblock", "magicItemBlock", "spellBlock"];

fn figure_fields(c: Card, x: Ctx) -> AnyView {
    let r = Loc::ROOT;
    let kind = Memo::new(move |_| c.m().kids.first().map(|k| k.tag.clone()).unwrap_or_default());
    let set_kind = move |ev| {
        let v = event_target_value(&ev);
        let root = x.tab.doc_untracked();
        let Some(root) = sb::read(&root.source, ROOT) else { return };
        let t = targets(&root);
        let inner = match v.as_str() {
            // A link needs a target: the first, or none yet (pick or make one below).
            "tableLink" => El::new("tableLink").with("id", &t.tables.first().map(|x| x.0.clone()).unwrap_or_default()),
            "sidebarLink" => El::new("sidebarLink").with("id", &t.sidebars.first().map(|x| x.0.clone()).unwrap_or_default()),
            other => match new_block(other, &root, x.chapter) {
                Some((_, el)) => el,
                None => El::new(other).with("ref", ""),
            },
        };
        c.change(|e| e.kids = vec![inner]);
    };
    view! {
        <div class="field-row">
            {text_in(c, r, "id", "Id", "fig-ritual-map", "What <figureRef> names")}
            {text_in(c, r, "name", "Caption", "(none)", "\u{201c}Figure 3.02: caption\u{201d}")}
            <label class="field">
                <span class="field-label">"Holds"</span>
                <select prop:value=move || kind.get() on:change=set_kind>
                    {FIGURE_KINDS.iter().map(|k| view! { <option value=*k>{label(k)}</option> }).collect_view()}
                </select>
            </label>
        </div>
        <div class="sb-rows">
            {move || { let k = kind.get(); let tag: &'static str = FIGURE_KINDS.iter().find(|t| **t == k).copied().unwrap_or("image"); kind_fields(c, r.at(tag, 0), x, tag) }}
        </div>
    }
    .into_any()
}

fn result_row(c: Card, l: Loc, _i: usize) -> AnyView {
    view! {
        <div class="field-row">{text_in(c, l, "dc", "DC", "10", "This result and lower ones")}</div>
        <Rows c loc=l tag="rd" title="Cells" hint="One per column after DC." min=1 make=|_| El::new("rd")
            row=|c, l, _| {
                let get = Signal::derive(move || c.m().at(&l.s[..l.n]).map(|e| e.text.clone()).unwrap_or_default());
                let put = Callback::new(move |v: String| c.at(l, |e| e.text = v));
                view! { <ChipEditor value=get set=put single=true placeholder="(empty)"/> }.into_any()
            }/>
    }
    .into_any()
}

fn skill_table_fields(c: Card) -> AnyView {
    let r = Loc::ROOT;
    let has_headers = move || c.m().kid("headers").is_some();
    view! {
        <div class="field-row">
            {pick_in(c, r, "stat", sb::STATS, Some("(none)"), "Ability", "")}
            {pick_in(c, r, "skill", sb::SKILLS, Some("(none)"), "Skill", "")}
            {text_in(c, r, "dc", "DC", "(none)", "")}
        </div>
        <label class="flag">
            <input type="checkbox" prop:checked=has_headers on:change=move |ev| {
                let on = event_target_checked(&ev);
                c.change(|e| if on {
                    let n = e.all("result").iter().map(|r| r.all("rd").len()).max().unwrap_or(1).max(1);
                    let mut h = El::new("headers");
                    for _ in 0..n { h.kids.push(El::new("rh")); }
                    e.kids.insert(0, h);
                } else {
                    e.remove_kid("headers");
                })
            }/>
            "Column headings"
        </label>
        {move || has_headers().then(|| view! {
            <Rows c loc=r container="headers" tag="rh" title="Headings" hint="The first column is always DC." min=1 make=|_| El::new("rh")
                row=|c, l, _| view! { <div class="field-row">{text_in(c, l, "", "Heading", "Lore", "")}</div> }.into_any()/>
        })}
        <Rows c loc=r tag="result" title="Results" min=1
            make=|m: &El| {
                let n = m.all("result").last().map(|r| r.all("rd").len()).unwrap_or(1).max(1);
                let dc = m.all("result").last().and_then(|r| r.get("dc").parse::<i32>().ok()).map(|d| d + 5).unwrap_or(10);
                let mut res = El::new("result").with("dc", &dc.to_string());
                for _ in 0..n { res.kids.push(El::new("rd")); }
                res
            }
            row=result_row/>
    }
    .into_any()
}

// ---- monster entries ------------------------------------------------------------------

const OMIT: &[(&str, &str)] = &[
    ("brief", "Subtitles"),
    ("habitats", "Habitat lines"),
    ("quoteBox", "Quotes"),
    ("image", "Images"),
    ("description", "Lore"),
    ("distinctionTable", "Distinction tables"),
    ("lair", "Lairs"),
];

/// A space-separated list of ids in an attribute: chosen ones in order, and a menu to add.
fn id_list(c: Card, k: &'static str, title: &'static str, hint: &'static str, opts: Signal<Vec<(String, String)>>) -> impl IntoView {
    let chosen = Memo::new(move |_| c.m().get(k).split_whitespace().map(String::from).collect::<Vec<_>>());
    let put = move |v: Vec<String>| c.change(|e| e.set(k, &v.join(" ")));
    let name = move |id: &str| opts.with(|o| o.iter().find(|(i, _)| i == id).map(|(_, l)| l.clone())).unwrap_or_else(|| format!("{id} (not in the file)"));
    view! {
        <div class="sb-rows">
            <div class="sb-rows-title">{title} <span class="hint">{hint}</span></div>
            {move || {
                let ids = chosen.get();
                let n = ids.len();
                ids.into_iter().enumerate().map(|(i, id)| view! {
                    <div class="sb-row">
                        <div class="sb-row-fields"><span>{name(&id)}</span></div>
                        <div class="sb-row-tools">
                            <button type="button" class="icon-btn" title="Move up" disabled={i == 0} on:click=move |_| { let mut v = chosen.get_untracked(); v.swap(i, i - 1); put(v) }>"↑"</button>
                            <button type="button" class="icon-btn" title="Move down" disabled={i + 1 >= n} on:click=move |_| { let mut v = chosen.get_untracked(); v.swap(i, i + 1); put(v) }>"↓"</button>
                            <button type="button" class="icon-btn danger" title="Remove" on:click=move |_| { let mut v = chosen.get_untracked(); v.remove(i); put(v) }>"✕"</button>
                        </div>
                    </div>
                }).collect_view()
            }}
            <select class="add-menu" on:change=move |ev| {
                let v = event_target_value(&ev);
                if let Some(s) = ev.target().and_then(|t| wasm_bindgen::JsCast::dyn_into::<web_sys::HtmlSelectElement>(t).ok()) { s.set_value(""); }
                if !v.is_empty() { let mut l = chosen.get_untracked(); l.push(v); put(l) }
            }>
                <option value="" selected=true>"+ Add\u{2026}"</option>
                {move || { let have = chosen.get(); opts.get().into_iter().filter(|(i, _)| !have.contains(i)).map(|(i, l)| view! { <option value=i>{l}</option> }).collect_view() }}
            </select>
        </div>
    }
}

/// An `<option>` list for a select of (id, label), keeping an unknown current value.
fn options(list: Vec<(String, String)>, cur: String) -> impl IntoView {
    let mut list = list;
    if !cur.is_empty() && !list.iter().any(|(i, _)| *i == cur) {
        list.insert(0, (cur.clone(), format!("{cur} (not in the file)")));
    }
    list.into_iter().map(|(i, l)| view! { <option value=i>{l}</option> }).collect_view()
}

/// The whitelist: `<part>`s and `<statblock>`s in print order.
#[component]
fn EntryList(c: Card, outline: Memo<Option<Outline>>) -> impl IntoView {
    let family = move || outline.with(|o| o.as_ref().map(|o| o.parts.iter().chain(&o.lairs).cloned().collect::<Vec<_>>()).unwrap_or_default());
    let mons = move || outline.with(|o| o.as_ref().map(|o| o.monsters.iter().map(|m| (m.id.clone(), if m.name.is_empty() { m.id.clone() } else { m.name.clone() })).collect::<Vec<_>>()).unwrap_or_default());
    let own = move |id: &str| {
        outline.with(|o| {
            o.as_ref()
                .map(|o| o.monsters.iter().filter(|m| m.id == id).flat_map(|m| m.parts.clone()).chain(o.lairs.clone()).collect::<Vec<_>>())
                .unwrap_or_default()
        })
    };
    let kids = Memo::new(move |_| c.m().kids.iter().map(|k| k.tag.clone()).collect::<Vec<_>>());
    let edit_kids = move |f: &dyn Fn(&mut Vec<El>)| c.change(|e| f(&mut e.kids));
    view! {
        <div class="sb-rows">
            <div class="sb-rows-title">"Prints, in this order" <span class="hint">"The family's name heads it. A stat block's parts print before it."</span></div>
            {move || {
                let ks = kids.get();
                let n = ks.len();
                ks.into_iter().enumerate().map(|(i, tag)| {
                    let same = (0..i).filter(|j| kids.with_untracked(|k| k[*j] == tag)).count();
                    let is_part = tag == "part";
                    let l = Loc::ROOT.at(if is_part { "part" } else { "statblock" }, same);
                    let cur = move || c.m().at(&l.s[..l.n]).map(|e| e.get("ref").to_string()).unwrap_or_default();
                    let body = if is_part {
                        view! {
                            <label class="field"><span class="field-label">"Part"</span>
                                <select prop:value=cur on:change=move |ev| { let v = event_target_value(&ev); c.at(l, |e| e.set("ref", &v)) }>
                                    {move || options(family(), cur())}
                                </select>
                            </label>
                        }.into_any()
                    } else {
                        view! {
                            <label class="field"><span class="field-label">"Stat block"</span>
                                <select prop:value=cur on:change=move |ev| { let v = event_target_value(&ev); c.at(l, |e| e.set("ref", &v)) }>
                                    {move || options(mons(), cur())}
                                </select>
                            </label>
                            <div class="sb-rows">
                                <div class="sb-rows-title">"Its parts first" <span class="hint">"Its own, or any of the file's lairs."</span></div>
                                {move || {
                                    let np = c.m().at(&l.s[..l.n]).map(|e| e.all("part").len()).unwrap_or(0);
                                    (0..np).map(|j| {
                                        let pl = l.at("part", j);
                                        let pcur = move || c.m().at(&pl.s[..pl.n]).map(|e| e.get("ref").to_string()).unwrap_or_default();
                                        view! {
                                            <div class="sb-row">
                                                <div class="sb-row-fields">
                                                    <select prop:value=pcur on:change=move |ev| { let v = event_target_value(&ev); c.at(pl, |e| e.set("ref", &v)) }>
                                                        {move || options(own(&cur()), pcur())}
                                                    </select>
                                                </div>
                                                <div class="sb-row-tools">
                                                    <button type="button" class="icon-btn danger" title="Remove" on:click=move |_| c.at(l, |e| { let mut k = 0; e.kids.retain(|x| { if x.tag != "part" { return true; } k += 1; k - 1 != j }); })>"✕"</button>
                                                </div>
                                            </div>
                                        }
                                    }).collect_view()
                                }}
                                <button type="button" class="small" on:click=move |_| {
                                    let first = own(&cur()).first().map(|p| p.0.clone()).unwrap_or_default();
                                    c.at(l, |e| e.kids.push(El::new("part").with("ref", &first)))
                                }>"+ Part"</button>
                            </div>
                        }.into_any()
                    };
                    view! {
                        <div class="sb-row">
                            <div class="sb-row-fields">{body}</div>
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
                <button type="button" class="small" on:click=move |_| { let f = family().first().map(|p| p.0.clone()).unwrap_or_default(); edit_kids(&|k| k.push(El::new("part").with("ref", &f))) }>"+ Family part"</button>
                <button type="button" class="small" on:click=move |_| { let f = mons().first().map(|p| p.0.clone()).unwrap_or_default(); edit_kids(&|k| k.push(El::new("statblock").with("ref", &f))) }>"+ Stat block"</button>
            </div>
        </div>
    }
}

fn entry_fields(c: Card, x: Ctx) -> AnyView {
    let r = Loc::ROOT;
    let mons = Signal::derive(move || two(x.t.get().monsters));
    let file = Memo::new(move |_| {
        let id = c.m().get("ref").to_string();
        x.t.with(|t| t.monsters.iter().find(|m| m.0 == id).map(|m| m.2.clone()).unwrap_or_default())
    });
    let got: RwSignal<Option<Result<Outline, String>>> = RwSignal::new(None);
    Effect::new(move |_| {
        let f = file.get();
        got.set(None);
        if f.is_empty() {
            got.set(Some(Err("This <reference> monster names no file.".into())));
            return;
        }
        let path = x.tab.doc_untracked().path.clone();
        spawn_local(async move {
            let r = crate::api::monster_outline(&path, &f).await.map_err(|e| e.message);
            // Only if the entry still names that file.
            if file.get_untracked() == f {
                got.set(Some(r));
            }
        });
    });
    let outline = Memo::new(move |_| got.with(|g| g.as_ref().and_then(|r| r.as_ref().ok().cloned())));
    let listing = Memo::new(move |_| c.m().kids.iter().any(|k| k.tag == "part" || k.tag == "statblock"));
    let mon_opts = Signal::derive(move || outline.with(|o| o.as_ref().map(|o| o.monsters.iter().map(|m| (m.id.clone(), if m.name.is_empty() { m.id.clone() } else { format!("{} ({})", m.name, m.id) })).collect()).unwrap_or_default()));
    let part_opts = Signal::derive(move || {
        outline.with(|o| {
            o.as_ref()
                .map(|o| {
                    let mut v: Vec<(String, String)> = o.parts.iter().chain(&o.lairs).cloned().collect();
                    for m in &o.monsters {
                        v.extend(m.parts.iter().map(|(i, l)| (i.clone(), format!("{l} \u{2014} {}", if m.name.is_empty() { &m.id } else { &m.name }))));
                    }
                    v
                })
                .unwrap_or_default()
        })
    });
    let set_mode = move |list: bool| {
        let o = outline.get_untracked();
        c.change(|e| {
            e.kids.clear();
            if list {
                for k in ["monsters", "omit", "omitParts"] {
                    e.set(k, "");
                }
                if let Some(o) = &o {
                    e.kids.extend(o.parts.iter().map(|(i, _)| El::new("part").with("ref", i)));
                    e.kids.extend(o.monsters.iter().map(|m| El::new("statblock").with("ref", &m.id)));
                }
                if e.kids.is_empty() {
                    e.kids.push(El::new("statblock").with("ref", ""));
                }
            }
        })
    };
    let omitted = move |k: &str| c.m().get("omit").split_whitespace().any(|x| x == k);
    view! {
        <div class="field-row">
            {opt_pick(c, r, "ref", "Monster file", "A <reference> monster with a file", mons, None)}
        </div>
        {move || match got.get() {
            None => view! { <p class="hint">"Reading the monster file\u{2026}"</p> }.into_any(),
            Some(Err(e)) => view! { <p class="hint bad">{e}</p> }.into_any(),
            Some(Ok(o)) => view! { <p class="hint">{format!("{}: {} family part(s), {} lair(s), {} stat block(s) with ids.", if o.name.is_empty() { "The file" } else { &o.name }, o.parts.len(), o.lairs.len(), o.monsters.len())}</p> }.into_any(),
        }}
        <div class="field-row">
            <label class="flag"><input type="radio" prop:checked=move || !listing.get() on:change=move |_| set_mode(false)/>"Print everything, except\u{2026}"</label>
            <label class="flag"><input type="radio" prop:checked=move || listing.get() on:change=move |_| set_mode(true)/>"Print only\u{2026}"</label>
        </div>
        {move || if listing.get() {
            view! { <EntryList c outline/> }.into_any()
        } else {
            view! {
                {id_list(c, "monsters", "Stat blocks", "Which print, in this order. None listed: all of them, in file order.", mon_opts)}
                <div class="sb-rows">
                    <div class="sb-rows-title">"Leave out these kinds" <span class="hint">"The family's and the stat blocks' alike."</span></div>
                    <div class="flags">
                        {OMIT.iter().map(|(k, lab)| {
                            let k: &'static str = k;
                            view! {
                                <label class="flag">
                                    <input type="checkbox" prop:checked=move || omitted(k) on:change=move |ev| {
                                        let on = event_target_checked(&ev);
                                        c.change(|e| {
                                            let mut v: Vec<String> = e.get("omit").split_whitespace().filter(|x| *x != k).map(String::from).collect();
                                            if on { v.push(k.to_string()); }
                                            // In the schema's order.
                                            v.sort_by_key(|x| OMIT.iter().position(|(o, _)| o == x));
                                            e.set("omit", &v.join(" "));
                                        })
                                    }/>
                                    {*lab}
                                </label>
                            }
                        }).collect_view()}
                    </div>
                </div>
                {id_list(c, "omitParts", "Leave out these parts", "By id: the family's, a stat block's own, or a lair.", part_opts)}
            }.into_any()
        }}
    }
    .into_any()
}

// ---- the page -------------------------------------------------------------------------

/// The page of the section block at `key` (child indexes from the root).
#[component]
pub fn SectionElPage(tab: DocTab, key: Vec<usize>) -> impl IntoView {
    let root = Memo::new(move |_| sb::read(&tab.doc().source, ROOT));
    let k2 = key.clone();
    let el = Memo::new(move |_| root.with(|r| r.as_ref().and_then(|r| r.walk().into_iter().find(|e| e.path.as_deref() == Some(&k2[..])).cloned())));
    let tag = el.with_untracked(|e| e.as_ref().map(|e| e.tag.clone()).unwrap_or_default());
    let c = Card::new(tab, ROOT, el, None, Some(norm));
    let t = Memo::new(move |_| root.with(|r| r.as_ref().map(targets).unwrap_or_default()));
    let x = Ctx { tab, t, chapter: key.first().copied().unwrap_or(0) };
    let section = StoredValue::new(key[..key.len().saturating_sub(1)].to_vec());
    let to_section = move |_: MouseEvent| {
        let d = tab.doc_untracked();
        let i = d.idx_of(&section.get_value());
        tab.reveal(i);
    };
    let k3 = StoredValue::new(key.clone());
    let t2 = tag.clone();
    let delete = move |_: MouseEvent| {
        let tag = t2.clone();
        ask(format!("Delete this {}?", label(&t2).to_lowercase()), move || {
            edit(tab, StatEdit::Delete { child: ROOT, path: k3.get_value(), tag })
        });
    };
    let body = match tag.as_str() {
        "figure" => figure_fields(c, x),
        "monsterEntry" => entry_fields(c, x),
        "skillcheckResultTable" => skill_table_fields(c),
        "parcelLink" => {
            let opts = Signal::derive(move || t.get().parcels);
            view! { <div class="field-row">{opt_pick(c, Loc::ROOT, "number", "Parcel", "From the supplement's <treasure>", opts, None)}</div> }.into_any()
        }
        "pageEnd" => view! {
            <div class="field-row">{flag_in(c, Loc::ROOT, "balance", true, "Balance the columns")}</div>
            <p class="hint">"Off: the left column fills before the right one starts."</p>
        }
        .into_any(),
        t => kind_fields(c, Loc::ROOT, x, t),
    };
    let title = label(&tag);
    let hint = blurb(&tag);
    view! {
        {move || if el.with(|e| e.is_none()) {
            view! { <div class="empty"><p>"This block isn't in the file any more."</p></div> }.into_any()
        } else {
            ().into_any()
        }}
        <div class="sb-page" class:hidden=move || el.with(|e| e.is_none())>
            <div class="table-card" class:dirty=move || c.d.dirty() class:failed=move || c.d.failed() on:keydown=keys(c)>
                <div class="table-card-title">
                    {title} <span class="hint">{hint}</span>
                    <span class="spacer"></span>
                    <button class="small" on:click=to_section>"Back to the section"</button>
                    <button class="small danger" disabled=move || tab.applying.get() on:click=delete>"Delete"</button>
                </div>
                {body}
                {card_bar(c)}
            </div>
        </div>
    }
}

/// Before writing: no empty optional attributes (the card's fields remove them already).
fn norm(e: &mut El) {
    if e.tag == "monsterEntry" {
        for k in ["monsters", "omit", "omitParts"] {
            if e.get(k).trim().is_empty() {
                e.set(k, "");
            }
        }
    }
}
