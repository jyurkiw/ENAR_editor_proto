//! The prose editor: a box where you type text and every inline tag shows as a chip
//! with what it prints ("Burning Hands", "2d6 Fire", "DC 15 Dexterity save"). Raw XML
//! never shows here; the Source view has it.
//!
//! - Click a chip to edit its fields (or remove it); Backspace deletes a chip.
//! - Ctrl+B / Ctrl+I bold and italicize the selection.
//! - Ctrl+K or ＋ opens the ref generator: a search over what fits here (this stat
//!   block's actions, the file's entries, chapters and figures, the SRD library and the
//!   project's files) and the other tags, ranked by context (a table cell under a "Spell"
//!   heading puts spells first). Picking a library spell, monster or item that isn't an
//!   entry yet adds its `<reference>` entry too.
//!
//! The box's XML is its value. The page provides a [`ProseEnv`] saying what kind of file
//! it is and how to name its refs.

use leptos::ev::{KeyboardEvent, MouseEvent};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::edit::{escape_attr, escape_text};
use shared::inline::{RefCtx, RefTag};
use shared::prose::{self, LibEntry, Palette, Tok};
use shared::reference::{self, RefEdit, RefEntry};
use shared::statblock::{Field, InlineSpec};
use shared::Change;
use wasm_bindgen::JsCast;

use crate::apply::apply;
use crate::state::{ctx, DocTab};

/// What a page's prose boxes need to know.
#[derive(Clone, Copy)]
pub struct ProseEnv {
    pub tab: DocTab,
    pub palette: Palette,
    /// The file's entries, chapters, figures and what they print.
    pub doc: Memo<RefCtx>,
    /// A stat block's own refs targets: (tag, id, name) of its actions, attacks, saves,
    /// spellcasting, spells and speeds.
    pub own: Memo<Vec<(String, String, String)>>,
    /// What `<shortNounRef/>` prints.
    pub noun: Memo<String>,
    /// The names of the file's own entries, for the blue "make this a pill" underlines.
    pub pills: Memo<Vec<shared::pills::Candidate>>,
}

/// Gives the page's prose boxes their environment.
pub fn provide_env(tab: DocTab, palette: Palette, own: Option<Memo<Vec<(String, String, String)>>>) -> ProseEnv {
    let doc = Memo::new(move |_| {
        let d = tab.doc();
        let index = tab.refs.with(|r| r.as_ref().and_then(|r| r.as_ref().ok().cloned()));
        RefCtx::from_doc(&d.source, &d.nodes, index)
    });
    let own = own.unwrap_or_else(|| Memo::new(|_| Vec::new()));
    let noun = Memo::new(move |_| {
        if palette == Palette::StatBlock {
            shared::family::read(&tab.doc().source).map(|f| f.header.short_noun).unwrap_or_default()
        } else {
            String::new()
        }
    });
    let pills = Memo::new(move |_| shared::pills::candidates(&doc.get(), palette));
    let env = ProseEnv { tab, palette, doc, own, noun, pills };
    provide_context(env);
    env
}

fn env() -> ProseEnv {
    expect_context::<ProseEnv>()
}

/// What a chip shows, and whether its target was found.
fn chip_label(env: &ProseEnv, t: &Tok) -> (String, bool) {
    let Tok::Tag { tag, attrs, text, .. } = t else { return (String::new(), true) };
    if env.palette == Palette::StatBlock {
        let own = |ids: &[&str]| {
            let id = t.attr("ref");
            env.own.with_untracked(|o| o.iter().find(|(tg, i, _)| i == id && ids.contains(&tg.as_str())).map(|(_, _, n)| n.clone()))
        };
        let found = match tag.as_str() {
            "actionRef" => Some(own(&["action", "attack", "save", "spellcasting"])),
            "attackRef" => Some(own(&["attack"])),
            "saveRef" => Some(own(&["save"])),
            "spellRef" => Some(own(&["spell"])),
            "speedRef" => Some(own(&["speed"]).map(|n| format!("{n} Speed"))),
            "formulaRef" => Some(own(&["formula"]).map(|n| {
                let noun = t.attr("noun");
                if noun.is_empty() { format!("\u{2039}{n}\u{203a}") } else { format!("\u{2039}{n}\u{203a} {noun}") }
            })),
            "choiceRef" => Some(own(&["choice"]).map(|n| format!("\u{2039}{n}\u{203a}"))),
            "shortNounRef" => {
                let n = env.noun.get_untracked();
                // The prose writes its own "The"/"the" before it.
                return (if n.is_empty() { "creature".into() } else { n }, true);
            }
            _ => None,
        };
        if let Some(f) = found {
            return match f {
                Some(n) => (n, true),
                None => (t.attr("ref").to_string(), false),
            };
        }
    } else if shared::inline::spec(tag).is_some() {
        let rt = RefTag { tag: tag.clone(), attrs: attrs.clone(), text: text.clone(), start: 0, end: 0 };
        let r = env.doc.with_untracked(|c| c.resolve(&rt));
        return (r.text, r.known && r.problems.is_empty());
    }
    (prose::label(t), true)
}

fn chip_title(env: &ProseEnv, tag: &str) -> String {
    let what = env.palette.spec(tag).map(|s| s.label).unwrap_or(tag);
    format!("{what} — click to edit")
}

/// The box's HTML: text, bold and italic, and chips. Err: the XML isn't well-formed.
fn render_html(env: &ProseEnv, xml: &str) -> Result<String, String> {
    let toks = prose::tokenize(xml)?;
    let mut h = String::new();
    for t in &toks {
        match t {
            Tok::Text(s) => h.push_str(&escape_text(s)),
            Tok::Open(b) => h.push_str(&format!("<{b}>")),
            Tok::Close(b) => h.push_str(&format!("</{b}>")),
            Tok::Tag { tag, xml, .. } => {
                let (label, ok) = chip_label(env, t);
                h.push_str(&format!(
                    "<span class=\"chip-tag chip-{kind}{bad}\" contenteditable=\"false\" data-xml=\"{x}\" title=\"{title}\">{l}</span>",
                    kind = prose::chip_kind(tag),
                    bad = if ok { "" } else { " chip-bad" },
                    x = escape_attr(xml, '"'),
                    title = escape_attr(&chip_title(env, tag), '"'),
                    l = escape_text(if label.is_empty() { tag } else { &label }),
                ));
            }
        }
    }
    Ok(h)
}

/// The box's XML, read back from its DOM.
fn serialize(node: &web_sys::Node) -> String {
    let mut out = String::new();
    let kids = node.child_nodes();
    for i in 0..kids.length() {
        let Some(k) = kids.item(i) else { continue };
        match k.node_type() {
            web_sys::Node::TEXT_NODE => out.push_str(&escape_text(&k.text_content().unwrap_or_default().replace('\u{a0}', " "))),
            web_sys::Node::ELEMENT_NODE => {
                let el: &web_sys::Element = k.unchecked_ref();
                if let Some(x) = el.get_attribute("data-xml") {
                    out.push_str(&x);
                    continue;
                }
                let inner = serialize(&k);
                match el.tag_name().to_ascii_uppercase().as_str() {
                    "B" | "STRONG" if !inner.is_empty() => out.push_str(&format!("<b>{inner}</b>")),
                    "I" | "EM" if !inner.is_empty() => out.push_str(&format!("<i>{inner}</i>")),
                    "BR" => out.push(' '),
                    "DIV" | "P" => {
                        if !out.is_empty() && !out.ends_with(' ') {
                            out.push(' ');
                        }
                        out.push_str(&inner);
                    }
                    _ => out.push_str(&inner),
                }
            }
            _ => {}
        }
    }
    out
}

fn tokens_xml(toks: &[Tok]) -> String {
    toks.iter()
        .map(|t| match t {
            Tok::Text(s) => escape_text(s),
            Tok::Open(b) => format!("<{b}>"),
            Tok::Close(b) => format!("</{b}>"),
            Tok::Tag { xml, .. } => xml.clone(),
        })
        .collect()
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// The selection, if it's inside `el`.
fn range_in(el: &web_sys::HtmlElement) -> Option<web_sys::Range> {
    let sel = web_sys::window()?.get_selection().ok()??;
    if sel.range_count() == 0 {
        return None;
    }
    let r = sel.get_range_at(0).ok()?;
    let node = r.common_ancestor_container().ok()?;
    el.contains(Some(&node)).then_some(r)
}

/// What the panel under the box shows.
#[derive(Clone, PartialEq)]
enum Panel {
    None,
    /// The ref generator, with its search.
    Gen,
    /// A tag's fields: a new tag, or chip number `chip` (among the box's chips).
    Tag { tag: String, attrs: Vec<(String, String)>, text: String, chip: Option<usize> },
    /// The right-click menu at viewport point (x, y).
    Menu { x: i32, y: i32, title: Option<String>, items: Vec<MenuItem> },
    /// Turning the selection into a variable: a `<data>` entry holding it as its value.
    Variable { value: String },
    /// Turning the selection into a person, place or thing: a `<noun>` entry.
    Noun { text: String },
}

/// One row of the right-click menu.
#[derive(Clone, PartialEq)]
enum MenuItem {
    /// Make the mention under the pointer a ref to this entry form.
    Pill(shared::pills::Candidate),
    /// Stop offering this text as a pill for the session.
    Dismiss(String),
    /// Review every mention of this entry form in the file.
    PillAll(shared::pills::Candidate),
    /// Replace the misspelled word with this spelling.
    Suggest(String),
    NoSuggestions,
    /// Stop marking this word for the session.
    Ignore(String),
    /// Add this word to the project word list.
    AddWord(String),
    /// The ref generator, on the selection (as Ctrl+K).
    MakePill,
    Variable,
    Noun,
    Cut,
    Copy,
    Paste,
    Sep,
}

/// Why `name` can't be a new variable's name (letters only, not taken), if it can't.
fn variable_problem(name: &str, taken: &[String]) -> Option<String> {
    if name.is_empty() {
        Some("Give the variable a name.".into())
    } else if !name.chars().all(|c| c.is_ascii_alphabetic()) {
        Some("Letters only: no digits, spaces or punctuation (bridgeToll, not toll2).".into())
    } else if taken.iter().any(|t| t == name) {
        Some(format!("Another entry is already called \u{201c}{name}\u{201d}."))
    } else {
        None
    }
}

/// A letters-only name from a value's words ("Reed Mill toll" -> "reedMillToll").
fn camel(value: &str) -> String {
    let mut out = String::new();
    for w in value.split(|c: char| !c.is_ascii_alphabetic()).filter(|w| !w.is_empty()) {
        let w = w.to_ascii_lowercase();
        if out.is_empty() {
            out.push_str(&w);
        } else {
            let mut cs = w.chars();
            if let Some(f) = cs.next() {
                out.push(f.to_ascii_uppercase());
                out.push_str(cs.as_str());
            }
        }
    }
    out
}

/// Moves a range's ends inward past whitespace (within its end text nodes), so turning
/// " the toll " into a pill keeps the spaces around it.
fn trim_range(r: &web_sys::Range) {
    let ws_utf16 = |s: &str| s.chars().take_while(|c| c.is_whitespace()).map(|c| c.len_utf16() as u32).sum::<u32>();
    if let (Ok(n), Ok(o)) = (r.start_container(), r.start_offset()) {
        if n.node_type() == web_sys::Node::TEXT_NODE {
            let t: Vec<u16> = n.text_content().unwrap_or_default().encode_utf16().collect();
            let rest = String::from_utf16_lossy(&t[(o as usize).min(t.len())..]);
            let k = ws_utf16(&rest);
            if k > 0 {
                let _ = r.set_start(&n, o + k);
            }
        }
    }
    if let (Ok(n), Ok(o)) = (r.end_container(), r.end_offset()) {
        if n.node_type() == web_sys::Node::TEXT_NODE {
            let t: Vec<u16> = n.text_content().unwrap_or_default().encode_utf16().collect();
            let before: String = String::from_utf16_lossy(&t[..(o as usize).min(t.len())]).chars().rev().collect();
            let k = ws_utf16(&before);
            if k > 0 && o >= k {
                let _ = r.set_end(&n, o - k);
            }
        }
    }
}

/// The selection holds text and no chips (so it can become a variable or a noun).
fn plain_selection(div: &web_sys::HtmlElement, r: &web_sys::Range) -> bool {
    if String::from(r.to_string()).trim().is_empty() {
        return false;
    }
    let Ok(chips) = div.query_selector_all("[data-xml]") else { return true };
    !(0..chips.length()).filter_map(|i| chips.item(i)).any(|c| r.intersects_node(&c).unwrap_or(false))
}

fn exec(cmd: &str, value: Option<&str>) {
    if let Some(hd) = document().and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok()) {
        let _ = match value {
            Some(v) => hd.exec_command_with_show_ui_and_value(cmd, false, v),
            None => hd.exec_command(cmd),
        };
    }
}

/// One thing the generator can insert.
#[derive(Clone, PartialEq)]
struct Choice {
    label: String,
    sub: String,
    group: &'static str,
    score: u32,
    act: Act,
}

#[derive(Clone, PartialEq)]
enum Act {
    Insert(String),
    /// Add this `<reference>` entry, then insert.
    AddEntry(RefEntry, String),
    /// Open the fields of a new tag.
    NewTag(&'static str),
}

/// The generator's choices for `q`, best first.
fn choices(env: &ProseEnv, lib: &[LibEntry], q: &str, hint: &str) -> Vec<Choice> {
    let want = prose::hint_kind(hint);
    let boost = |kind: &str| if want == Some(kind) { 30 } else { 0 };
    let mut out: Vec<Choice> = Vec::new();
    let mut push = |label: String, sub: String, group: &'static str, base: u32, kind: &str, act: Act| {
        if let Some(s) = prose::score(&label, q) {
            out.push(Choice { label, sub, group, score: s + base + boost(kind), act });
        }
    };
    let pal = env.palette;
    if pal == Palette::StatBlock {
        for (tag, id, name) in env.own.get_untracked() {
            let rt = match tag.as_str() {
                "attack" => "attackRef",
                "save" => "saveRef",
                "spell" => "spellRef",
                "speed" => "speedRef",
                "formula" => "formulaRef",
                "choice" => "choiceRef",
                _ => "actionRef",
            };
            let label = if tag == "speed" { format!("{name} Speed") } else { name.clone() };
            push(label, format!("this stat block's {tag}"), "This stat block", 20, "", Act::Insert(format!("<{rt} ref=\"{}\"/>", escape_attr(&id, '"'))));
        }
        let n = env.noun.get_untracked();
        push(if n.is_empty() { "creature".into() } else { n }, "the creature's short noun".into(), "This stat block", 15, "", Act::Insert("<shortNounRef/>".into()));
    } else {
        let doc = env.doc.get_untracked();
        for (kind, id) in &doc.entries {
            let Some(rt) = pal.ref_tag(kind) else { continue };
            let values = doc.index.as_ref().and_then(|i| i.entries.get(id)).map(|e| e.values.clone()).unwrap_or_default();
            if kind == "data" {
                let label = values.get("value").cloned().unwrap_or_else(|| id.clone());
                push(label, "data entry".into(), "Entries", 20, kind, Act::Insert(format!("<{rt} ref=\"{}\" value=\"value\"/>", escape_attr(id, '"'))));
                continue;
            }
            // One row per form the entry has (its name, display name, short name), each
            // printing that form.
            let mut shown: Vec<String> = Vec::new();
            for (form, what) in [("name", "name"), ("displayName", "display name"), ("shortName", "short name")] {
                let Some(text) = values.get(form).filter(|t| !t.trim().is_empty()) else { continue };
                if shown.iter().any(|t| prose::norm_eq(t, text)) {
                    continue;
                }
                shown.push(text.clone());
                let xml = format!("<{rt} ref=\"{}\" value=\"{form}\"/>", escape_attr(id, '"'));
                let sub = if form == "name" { format!("{kind} entry") } else { format!("{kind} entry \u{b7} {what}") };
                push(text.clone(), sub, "Entries", 20, kind, Act::Insert(xml));
            }
            if shown.is_empty() {
                push(id.clone(), format!("{kind} entry"), "Entries", 20, kind, Act::Insert(format!("<{rt} ref=\"{}\" value=\"name\"/>", escape_attr(id, '"'))));
            }
        }
        if pal == Palette::Supplement {
            for c in &doc.chapters {
                push(c.title.clone(), "chapter".into(), "Chapters & figures", 10, "", Act::Insert(format!("<chapterRef name=\"{}\"/>", escape_attr(&c.name, '"'))));
            }
            for f in &doc.figures {
                push(f.id.clone(), "figure".into(), "Chapters & figures", 10, "", Act::Insert(format!("<figureRef ref=\"{}\"/>", escape_attr(&f.id, '"'))));
            }
        }
    }
    // The library: spells, stat blocks and items.
    let tab = env.tab;
    let d = tab.doc_untracked();
    let path = d.path.clone();
    let entries = if pal == Palette::StatBlock { Vec::new() } else { reference::read(&d.source, &d.nodes).into_iter().map(|(_, e)| e).collect::<Vec<_>>() };
    let taken: Vec<String> = entries.iter().map(|e| e.id()).collect();
    for l in lib {
        let sub = format!("{} {}", if l.srd { "SRD" } else { "project" }, l.kind);
        if pal == Palette::StatBlock {
            if let Some(term) = pal.term_tag(&l.kind) {
                push(l.name.clone(), sub, "Library", 0, &l.kind, Act::Insert(prose::tag_xml(term, &[], &l.name, true)));
            }
            continue;
        }
        let Some(rt) = pal.ref_tag(&l.kind) else { continue };
        let file = prose::rel_path(&path, &l.file);
        // Already an entry for that file (and stat block)? Use it.
        let existing = entries.iter().find(|e| {
            e.tag == l.kind && e.get("file").replace('\\', "/") == file && (l.kind != "monster" || e.get("monster") == l.id)
        });
        // An entry for it already: its rows are among the entries above.
        if existing.is_some() {
            continue;
        }
        let id = shared::statblock::fresh(&l.id, &taken);
        let new_entry = |alias: Option<(&str, &str)>| {
            let mut entry = RefEntry::new(&l.kind);
            entry.attrs = vec![("id".into(), id.clone()), ("file".into(), file.clone())];
            if l.kind == "monster" {
                entry.attrs.push(("monster".into(), l.id.clone()));
            }
            if let Some((form, text)) = alias {
                entry.attrs.push((form.to_string(), text.to_string()));
            }
            entry
        };
        push(l.name.clone(), sub.clone(), "Library", 0, &l.kind, Act::AddEntry(new_entry(None), format!("<{rt} ref=\"{}\" value=\"name\"/>", escape_attr(&id, '"'))));
        // The names other files' entries give it ("Thistlewood" for the apprentice wizard):
        // picking one adds the entry with that name, and prints it.
        for (form, text) in &l.aliases {
            if prose::norm_eq(text, &l.name) || !(form == "shortName" || form == "displayName") || (l.kind == "spell" && form != "displayName") {
                continue;
            }
            let what = if form == "shortName" { "short name" } else { "display name" };
            push(
                text.clone(),
                format!("{sub} \u{b7} {what} of {}", l.name),
                "Library",
                0,
                &l.kind,
                Act::AddEntry(new_entry(Some((form, text))), format!("<{rt} ref=\"{}\" value=\"{form}\"/>", escape_attr(&id, '"'))),
            );
        }
    }
    // The other tags.
    for s in pal.tags() {
        push(s.label.to_string(), format!("<{}>", s.tag).replace(['<', '>'], ""), "Other tags", 0, "", Act::NewTag(s.tag));
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())));
    out.truncate(40);
    out
}

/// A tag's fields, from its spec.
#[component]
fn TagForm(spec: &'static InlineSpec, attrs: RwSignal<Vec<(String, String)>>, text: RwSignal<String>) -> impl IntoView {
    let env = env();
    let val = move |a: &'static str| attrs.with(|v| v.iter().find(|(x, _)| x == a).map(|(_, y)| y.clone()).unwrap_or_default());
    let set_val = move |a: &'static str, y: String| {
        attrs.update(|v| match v.iter_mut().find(|(x, _)| x == a) {
            Some(p) => p.1 = y,
            None => v.push((a.to_string(), y)),
        })
    };
    let options = move |kinds: &'static [&'static str]| -> Vec<(String, String)> {
        if env.palette == Palette::StatBlock {
            return env.own.get().into_iter().filter(|(t, _, _)| kinds.contains(&t.as_str())).map(|(_, id, n)| (id, n)).collect();
        }
        env.doc.with(|d| {
            let mut o: Vec<(String, String)> = Vec::new();
            for k in kinds {
                match *k {
                    "chapter" => o.extend(d.chapters.iter().map(|c| (c.name.clone(), c.title.clone()))),
                    "figure" => o.extend(d.figures.iter().map(|f| (f.id.clone(), f.id.clone()))),
                    _ => o.extend(d.entries.iter().filter(|(kind, _)| kind == k).map(|(_, id)| {
                        let n = d.index.as_ref().and_then(|i| i.entries.get(id)).and_then(|e| e.values.get("name")).cloned().unwrap_or_else(|| id.clone());
                        (id.clone(), n)
                    })),
                }
            }
            o
        })
    };
    let field = move |a: &'static str, f: Field, req: bool| -> AnyView {
        let label = if req { format!("{a} *") } else { a.to_string() };
        let control = match f {
            Field::Pick(opts) => view! {
                <select prop:value=move || val(a) on:change=move |ev| set_val(a, event_target_value(&ev))>
                    <option value="">"—"</option>
                    {opts.iter().map(|o| view! { <option value=*o>{*o}</option> }).collect_view()}
                </select>
            }.into_any(),
            Field::Ref(kinds) => view! {
                <select prop:value=move || val(a) on:change=move |ev| set_val(a, event_target_value(&ev))>
                    <option value="">"—"</option>
                    {move || options(kinds).into_iter().map(|(id, n)| view! { <option value=id.clone()>{if n == id { n } else { format!("{n} ({id})") }}</option> }).collect_view()}
                </select>
            }.into_any(),
            Field::Bool => view! {
                <input type="checkbox" prop:checked=move || val(a) == "true"
                    on:change=move |ev| set_val(a, if event_target_checked(&ev) { "true".into() } else { String::new() })/>
            }.into_any(),
            _ => view! { <input type="text" prop:value=move || val(a) on:input=move |ev| set_val(a, event_target_value(&ev))/> }.into_any(),
        };
        view! { <label class="field small-field"><span class="field-label">{label}</span>{control}</label> }.into_any()
    };
    view! {
        {spec.text.map(|t| match t {
            Field::Pick(opts) => view! {
                <label class="field small-field"><span class="field-label">"text *"</span>
                    <select prop:value=move || text.get() on:change=move |ev| text.set(event_target_value(&ev))>
                        <option value="">"—"</option>
                        {opts.iter().map(|o| view! { <option value=*o>{*o}</option> }).collect_view()}
                    </select>
                </label>
            }.into_any(),
            _ => view! {
                <label class="field small-field"><span class="field-label">"text *"</span>
                    <input type="text" prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev))/>
                </label>
            }.into_any(),
        })}
        {spec.attrs.iter().map(|(a, f, req)| field(*a, *f, *req)).collect_view()}
    }
}

/// A box of prose. `value` is its XML; `set` gets the XML back as you edit. `hint`
/// says what the box holds (a table cell's column heading), for ranking refs.
#[component]
pub fn ChipEditor(
    value: Signal<String>,
    set: Callback<String>,
    #[prop(optional)] placeholder: &'static str,
    #[prop(optional, into)] hint: Signal<String>,
    /// One line, like a text field (table cells).
    #[prop(optional)]
    single: bool,
    /// The editable box's element id (pages focus new paragraphs by it).
    #[prop(optional, into)]
    id: Signal<String>,
) -> impl IntoView {
    let env = env();
    let el = NodeRef::<html::Div>::new();
    let last = StoredValue::new(None::<String>);
    let bad = RwSignal::new(None::<String>);
    let panel = RwSignal::new(Panel::None);
    let saved = StoredValue::new_local(None::<web_sys::Range>);
    let query = RwSignal::new(String::new());
    let picked = RwSignal::new(0usize);
    let tag_attrs = RwSignal::new(Vec::<(String, String)>::new());
    // The panel's first control gets focus when it opens (autofocus doesn't fire for an
    // element added after the page loaded).
    let panel_el = NodeRef::<html::Div>::new();
    let focus_panel = move || {
        request_animation_frame(move || {
            if let Some(p) = panel_el.get_untracked() {
                if let Ok(Some(e)) = p.query_selector("input, select") {
                    if let Ok(h) = e.dyn_into::<web_sys::HtmlElement>() {
                        let _ = h.focus();
                    }
                }
            }
        })
    };
    let tag_text = RwSignal::new(String::new());
    // After an entry is made from the selection: offer to pill its other mentions.
    let offer = RwSignal::new(None::<(String, Vec<shared::pills::Candidate>)>);
    let offer_count = Memo::new(move |_| {
        offer.with(|o| o.as_ref().map(|(_, c)| env.pills.with(|all| shared::pills::mentions_of(&env.tab.doc().source, all, c).len())).unwrap_or(0))
    });
    let cand = |kind: &str, id: &str, form: &str, text: &str, name: &str, xml: String| shared::pills::Candidate {
        kind: kind.into(),
        id: id.into(),
        form: form.into(),
        text: text.trim().into(),
        entry_name: name.into(),
        ref_xml: xml,
        any_case: !text.chars().any(char::is_uppercase),
    };
    // This box's underlines (pillable mentions, misspellings) in the page's highlights.
    let marks = StoredValue::new_local(crate::hints::BoxMarks::default());
    let check = move || {
        if let Some(div) = el.get_untracked() {
            let file = env.tab.doc_untracked().path.clone();
            env.pills.with_untracked(|c| marks.update_value(|m| crate::hints::mark(&div, m, &file, c)));
        }
    };
    // Re-mark when the dictionary arrives, the file's entries change or a mention is
    // dismissed; drop this box's marks when it goes away.
    Effect::new(move |_| {
        crate::spelling::ready();
        env.pills.track();
        crate::hints::track_dismissals();
        check();
    });
    on_cleanup(move || {
        marks.try_update_value(|m| crate::hints::clear(m));
    });

    // Draw the box when its value changes from outside (or the names it shows do).
    Effect::new(move |_| {
        let v = value.get();
        env.doc.track();
        env.own.track();
        let Some(div) = el.get() else { return };
        let node: &web_sys::Node = &div;
        let focused = document().and_then(|d| d.active_element()).is_some_and(|a| a.is_same_node(Some(node)));
        if last.get_value().as_deref() == Some(v.as_str()) && focused {
            return;
        }
        match render_html(&env, &v) {
            Ok(h) => {
                bad.set(None);
                div.set_inner_html(&h);
                check();
            }
            Err(e) => bad.set(Some(e)),
        }
        last.set_value(Some(v));
    });
    let emit = move || {
        let Some(div) = el.get_untracked() else { return };
        let xml = serialize(&div);
        last.set_value(Some(xml.clone()));
        set.run(xml);
        check();
    };
    // Insert a chip where the caret was (replacing the selection), then emit.
    let insert_xml = move |xml: String| {
        let Some(div) = el.get_untracked() else { return };
        let Ok(toks) = prose::tokenize(&xml) else { return };
        let Some(t) = toks.into_iter().next() else { return };
        let Some(d) = document() else { return };
        let Ok(span) = d.create_element("span") else { return };
        let (label, ok) = chip_label(&env, &t);
        let Tok::Tag { tag, .. } = &t else { return };
        let _ = span.set_attribute("class", &format!("chip-tag chip-{}{}", prose::chip_kind(tag), if ok { "" } else { " chip-bad" }));
        let _ = span.set_attribute("contenteditable", "false");
        let _ = span.set_attribute("data-xml", &xml);
        let _ = span.set_attribute("title", &chip_title(&env, tag));
        span.set_text_content(Some(&label));
        let range = saved.get_value().filter(|r| r.common_ancestor_container().ok().is_some_and(|n| div.contains(Some(&n))));
        match range {
            Some(r) => {
                let _ = r.delete_contents();
                let _ = r.insert_node(&span);
            }
            None => {
                let _ = div.append_child(&span);
            }
        }
        saved.set_value(None);
        panel.set(Panel::None);
        emit();
        let _ = div.focus();
    };
    let open_gen = move || {
        let Some(div) = el.get_untracked() else { return };
        let r = range_in(&div);
        query.set(r.as_ref().map(|r| String::from(r.to_string())).unwrap_or_default());
        saved.set_value(r);
        picked.set(0);
        panel.set(Panel::Gen);
        focus_panel();
        let app = ctx();
        // The index is kept current by the backend: read it each time (it's quick).
        spawn_local(async move {
            let r = crate::api::library().await.map_err(|e| e.message);
            app.library.set(Some(r));
        });
    };
    let list = Memo::new(move |_| {
        if panel.with(|p| *p != Panel::Gen) {
            return Vec::new();
        }
        let lib = ctx().library.with(|l| l.as_ref().and_then(|l| l.as_ref().ok().cloned()).unwrap_or_default());
        choices(&env, &lib, &query.get(), &hint.get())
    });
    let choose = move |c: Choice| match c.act.clone() {
        Act::Insert(x) => insert_xml(x),
        Act::AddEntry(entry, x) => {
            let kind = entry.tag.clone();
            let id = entry.attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone()).unwrap_or_default();
            let form = prose::tokenize(&x).ok().and_then(|t| t.into_iter().next()).map(|t| t.attr("value").to_string()).unwrap_or_else(|| "name".into());
            apply(env.tab, Change::Reference { edit: RefEdit::Add { entry } });
            insert_xml(x.clone());
            // Offer to pill the file's other mentions of what was picked.
            offer.set(Some((format!("\u{201c}{}\u{201d}", c.label), vec![cand(&kind, &id, &form, &c.label, &c.label, x)])));
        }
        Act::NewTag(tag) => {
            let sel = query.get_untracked();
            tag_attrs.set(Vec::new());
            tag_text.set(sel);
            panel.set(Panel::Tag { tag: tag.to_string(), attrs: Vec::new(), text: String::new(), chip: None });
            focus_panel();
        }
    };
    // Right-click: our menu, for the mention or misspelling under the pointer, then the
    // selection, then the clipboard.
    let sel_range = StoredValue::new_local(None::<web_sys::Range>);
    let var_name = RwSignal::new(String::new());
    let noun_name = RwSignal::new(String::new());
    let noun_short = RwSignal::new(String::new());
    let noun_kind = RwSignal::new(String::from("person"));
    let form_err = RwSignal::new(None::<String>);
    let on_menu = move |ev: MouseEvent| {
        ev.prevent_default();
        let Some(div) = el.get_untracked() else { return };
        let (x, y) = (ev.client_x(), ev.client_y());
        let hit = marks.with_value(|m| match crate::hints::hit(m, x as f64, y as f64) {
            crate::hints::Hit::Pill(i) => Some((m.pills[i].range.clone(), m.pills[i].text.clone(), Some(m.pills[i].cands.clone()))),
            crate::hints::Hit::Word(r, w) => Some((r, w, None)),
            crate::hints::Hit::None => None,
        });
        let sel = range_in(&div).filter(|r| !r.collapsed());
        sel_range.set_value(sel.clone());
        saved.set_value(None);
        let mut items = Vec::new();
        let mut title = None;
        match hit {
            Some((r, text, Some(cands))) => {
                saved.set_value(Some(r));
                title = Some(format!("\u{201c}{text}\u{201d}"));
                items.extend(cands.iter().cloned().map(MenuItem::Pill));
                items.extend(cands.into_iter().map(MenuItem::PillAll));
                items.push(MenuItem::Dismiss(text));
                items.push(MenuItem::Sep);
            }
            Some((r, word, None)) => {
                saved.set_value(Some(r));
                title = Some(format!("\u{201c}{word}\u{201d}"));
                let sug = crate::spelling::suggest(&word);
                if sug.is_empty() {
                    items.push(MenuItem::NoSuggestions);
                }
                items.extend(sug.into_iter().map(MenuItem::Suggest));
                items.push(MenuItem::AddWord(word.clone()));
                items.push(MenuItem::Ignore(word));
                items.push(MenuItem::Sep);
            }
            None => {}
        }
        items.push(MenuItem::MakePill);
        if sel.as_ref().is_some_and(|r| plain_selection(&div, r)) {
            if env.palette.ref_tag("data").is_some() {
                items.push(MenuItem::Variable);
            }
            if env.palette.ref_tag("noun").is_some() {
                items.push(MenuItem::Noun);
            }
        }
        items.extend([MenuItem::Sep, MenuItem::Cut, MenuItem::Copy, MenuItem::Paste]);
        // Keep the menu on screen.
        let (w, h) = web_sys::window()
            .map(|w| (w.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1e4), w.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1e4)))
            .unwrap_or((1e4, 1e4));
        let (x, y) = (x.min(w as i32 - 320).max(0), y.min(h as i32 - 40 - 30 * items.len() as i32).max(0));
        panel.set(Panel::Menu { x, y, title, items });
    };
    let run_item = move |it: MenuItem| {
        panel.set(Panel::None);
        let Some(div) = el.get_untracked() else { return };
        let selected = move || {
            sel_range.get_value().map(|r| {
                trim_range(&r);
                r
            })
        };
        match it {
            // `saved` holds the mention.
            MenuItem::Pill(c) => insert_xml(c.ref_xml),
            MenuItem::Dismiss(t) => crate::hints::dismiss(&env.tab.doc_untracked().path, &t),
            MenuItem::PillAll(c) => {
                let title = format!("\u{201c}{}\u{201d} \u{2192} {} \u{b7} {}", c.text, c.entry_name, c.form_label());
                ctx().dialog.set(Some(crate::state::Dialog::PillAll { tab: env.tab.id, title, cands: vec![c], all: env.pills.get_untracked() }));
            }
            MenuItem::Suggest(w) => {
                if let (Some(r), Some(d)) = (saved.get_value(), document()) {
                    let _ = r.delete_contents();
                    let _ = r.insert_node(&d.create_text_node(&w));
                    saved.set_value(None);
                    emit();
                }
            }
            MenuItem::Ignore(w) => crate::spelling::ignore(&w),
            MenuItem::AddWord(w) => {
                spawn_local(async move {
                    match crate::api::add_project_word(&w.replace('\u{2019}', "'")).await {
                        Ok(list) => crate::spelling::set_project_words(list),
                        Err(e) => ctx().show_error(e),
                    }
                });
            }
            MenuItem::MakePill => open_gen(),
            MenuItem::Variable => {
                let Some(r) = selected() else { return };
                let v = String::from(r.to_string());
                saved.set_value(Some(r));
                var_name.set(camel(&v));
                form_err.set(None);
                panel.set(Panel::Variable { value: v });
                focus_panel();
            }
            MenuItem::Noun => {
                let Some(r) = selected() else { return };
                let t = String::from(r.to_string());
                saved.set_value(Some(r));
                noun_name.set(t.clone());
                noun_short.set(String::new());
                form_err.set(None);
                panel.set(Panel::Noun { text: t });
                focus_panel();
            }
            MenuItem::Cut | MenuItem::Copy => {
                let _ = div.focus();
                let cut = it == MenuItem::Cut;
                exec(if cut { "cut" } else { "copy" }, None);
                if cut {
                    emit();
                }
            }
            MenuItem::Paste => {
                let _ = div.focus();
                spawn_local(async move {
                    match crate::api::clipboard_text().await {
                        // insertText fires input, which emits.
                        Ok(t) => exec("insertText", Some(&t)),
                        Err(e) => leptos::logging::log!("paste: {}", e.message),
                    }
                });
            }
            MenuItem::NoSuggestions | MenuItem::Sep => {}
        }
    };
    let taken_ids = move || env.doc.with_untracked(|d| d.entries.iter().map(|(_, i)| i.clone()).collect::<Vec<String>>());
    let save_variable = move || {
        let Panel::Variable { value } = panel.get_untracked() else { return };
        let name = var_name.get_untracked().trim().to_string();
        if let Some(e) = variable_problem(&name, &taken_ids()) {
            form_err.set(Some(e));
            return;
        }
        let mut entry = RefEntry::new("data");
        entry.attrs = vec![("id".into(), name.clone()), ("value".into(), value.trim().to_string())];
        apply(env.tab, Change::Reference { edit: RefEdit::Add { entry } });
        let xml = format!("<dataRef ref=\"{}\" value=\"value\"/>", escape_attr(&name, '"'));
        insert_xml(xml.clone());
        offer.set(Some((format!("\u{201c}{}\u{201d}", value.trim()), vec![cand("data", &name, "value", &value, &name, xml)])));
    };
    let save_noun = move || {
        let Panel::Noun { text } = panel.get_untracked() else { return };
        let name = noun_name.get_untracked().trim().to_string();
        let short = noun_short.get_untracked().trim().to_string();
        if name.is_empty() {
            form_err.set(Some("Give it a name.".into()));
            return;
        }
        let id = shared::statblock::fresh(&shared::statblock::slug(&name), &taken_ids());
        let mut entry = RefEntry::new("noun");
        entry.attrs = vec![("id".into(), id.clone()), ("name".into(), name.clone())];
        if !short.is_empty() && short != name {
            entry.attrs.push(("shortName".into(), short.clone()));
        }
        let kind = noun_kind.get_untracked();
        if !kind.is_empty() {
            entry.attrs.push(("kind".into(), kind));
        }
        let form = if !short.is_empty() && prose::norm_eq(&text, &short) { "shortName" } else { "name" };
        apply(env.tab, Change::Reference { edit: RefEdit::Add { entry } });
        insert_xml(format!("<nounRef ref=\"{}\" value=\"{form}\"/>", escape_attr(&id, '"')));
        let mut cs = vec![cand("noun", &id, "name", &name, &name, format!("<nounRef ref=\"{}\" value=\"name\"/>", escape_attr(&id, '"')))];
        if !short.is_empty() && short != name {
            cs.push(cand("noun", &id, "shortName", &short, &name, format!("<nounRef ref=\"{}\" value=\"shortName\"/>", escape_attr(&id, '"'))));
        }
        offer.set(Some((format!("\u{201c}{name}\u{201d}"), cs)));
    };

    // Click a chip: its fields.
    let on_click = move |ev: MouseEvent| {
        let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) else { return };
        let Some(chip) = t.closest("[data-xml]").ok().flatten() else { return };
        let Some(div) = el.get_untracked() else { return };
        let Ok(all) = div.query_selector_all("[data-xml]") else { return };
        let n = (0..all.length()).find(|i| all.item(*i).is_some_and(|x| x.is_same_node(Some(&chip))));
        let Some(n) = n else { return };
        let xml = chip.get_attribute("data-xml").unwrap_or_default();
        let Ok(toks) = prose::tokenize(&xml) else { return };
        if let Some(Tok::Tag { tag, attrs, text, .. }) = toks.into_iter().next() {
            tag_attrs.set(attrs.clone());
            tag_text.set(text.clone());
            panel.set(Panel::Tag { tag, attrs, text, chip: Some(n as usize) });
            focus_panel();
        }
    };
    // Save the tag form: a new chip at the caret, or the clicked chip changed.
    let save_tag = move || {
        let Panel::Tag { tag, chip, .. } = panel.get_untracked() else { return };
        let has_text = env.palette.spec(&tag).is_some_and(|s| s.text.is_some());
        let attrs = tag_attrs.get_untracked();
        let order: Vec<(String, String)> = match env.palette.spec(&tag) {
            Some(s) => s.attrs.iter().map(|(a, _, _)| (a.to_string(), attrs.iter().find(|(x, _)| x == a).map(|(_, v)| v.clone()).unwrap_or_default()))
                .chain(attrs.iter().filter(|(k, _)| !s.attrs.iter().any(|(a, _, _)| a == k)).cloned())
                .collect(),
            None => attrs,
        };
        let xml = prose::tag_xml(&tag, &order, &tag_text.get_untracked(), has_text);
        match chip {
            None => insert_xml(xml),
            Some(n) => {
                let Ok(mut toks) = prose::tokenize(&value.get_untracked()) else { return };
                if let Some(t) = toks.iter_mut().filter(|t| matches!(t, Tok::Tag { .. })).nth(n) {
                    if let Ok(mut new) = prose::tokenize(&xml) {
                        if let Some(nt) = new.pop() {
                            *t = nt;
                        }
                    }
                }
                panel.set(Panel::None);
                last.set_value(None);
                set.run(tokens_xml(&toks));
            }
        }
    };
    let remove_tag = move || {
        let Panel::Tag { chip: Some(n), .. } = panel.get_untracked() else { return };
        let Ok(toks) = prose::tokenize(&value.get_untracked()) else { return };
        let mut k = 0;
        let kept: Vec<Tok> = toks.into_iter().filter(|t| {
            if !matches!(t, Tok::Tag { .. }) { return true; }
            k += 1;
            k - 1 != n
        }).collect();
        panel.set(Panel::None);
        last.set_value(None);
        set.run(tokens_xml(&kept));
    };
    let on_key = move |ev: KeyboardEvent| {
        let k = ev.key();
        if k == "Enter" {
            // No line breaks in prose: Enter applies (the card's own key handler).
            ev.prevent_default();
        } else if ev.ctrl_key() && (k == "k" || k == "K") {
            ev.prevent_default();
            ev.stop_propagation();
            open_gen();
        } else if ev.ctrl_key() && (k == "b" || k == "i") {
            ev.prevent_default();
            if let Some(hd) = document().and_then(|d| d.dyn_into::<web_sys::HtmlDocument>().ok()) {
                let _ = hd.exec_command(if k == "b" { "bold" } else { "italic" });
            }
            emit();
        } else if k == "Escape" && panel.with_untracked(|p| *p != Panel::None) {
            ev.stop_propagation();
            panel.set(Panel::None);
        }
    };
    let gen_key = move |ev: KeyboardEvent| {
        let n = list.with_untracked(|l| l.len());
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                picked.update(|p| *p = (*p + 1).min(n.saturating_sub(1)));
            }
            "ArrowUp" => {
                ev.prevent_default();
                picked.update(|p| *p = p.saturating_sub(1));
            }
            "Enter" => {
                ev.prevent_default();
                ev.stop_propagation();
                if let Some(c) = list.with_untracked(|l| l.get(picked.get_untracked()).cloned()) {
                    choose(c);
                }
            }
            "Escape" => {
                ev.stop_propagation();
                panel.set(Panel::None);
            }
            _ => {}
        }
    };
    let raw = RwSignal::new(String::new());
    view! {
        <div class="chip-editor" class:single=single>
            <div class="chip-row">
                <div node_ref=el id=move || { let i = id.get(); (!i.is_empty()).then_some(i) } class="chip-box" class:single=single contenteditable="true" spellcheck="false" data-placeholder=placeholder
                    class:hidden=move || bad.with(|b| b.is_some())
                    on:input=move |_| emit() on:keydown=on_key on:click=on_click on:contextmenu=on_menu></div>
                <button type="button" class="icon-btn chip-add" title="Insert a ref or tag (Ctrl+K)" on:mousedown=move |ev: MouseEvent| {
                    // Keep the caret where it was.
                    ev.prevent_default();
                    open_gen();
                }>"＋"</button>
            </div>
            {move || bad.get().map(|e| {
                raw.set(value.get_untracked());
                view! {
                    <div class="chip-fallback">
                        <p class="hint bad">"This text isn't well-formed XML (" {e} "), so it's shown as written. Fix it here:"</p>
                        <textarea class="p-text" rows=2 prop:value=move || raw.get() on:input=move |ev| {
                            let v = event_target_value(&ev);
                            raw.set(v.clone());
                            last.set_value(None);
                            set.run(v);
                        }></textarea>
                    </div>
                }
            })}
            {move || offer.get().filter(|_| offer_count.get() > 0 && panel.with(|p| *p == Panel::None)).map(|(title, cs)| {
                let cs = StoredValue::new(cs);
                let t2 = title.clone();
                view! {
                    <div class="pill-offer">
                        <span>"Pill the other mentions of " {title} " in this file too?"</span>
                        <button type="button" class="small primary" on:mousedown=move |ev: MouseEvent| {
                            ev.prevent_default();
                            offer.set(None);
                            ctx().dialog.set(Some(crate::state::Dialog::PillAll { tab: env.tab.id, title: t2.clone(), cands: cs.get_value(), all: env.pills.get_untracked() }));
                        }>"Review\u{2026}"</button>
                        <button type="button" class="small" on:mousedown=move |ev: MouseEvent| { ev.prevent_default(); offer.set(None); }>"No thanks"</button>
                    </div>
                }
            })}
            {move || match panel.get() {
                Panel::None => ().into_any(),
                Panel::Gen => view! {
                    <div node_ref=panel_el class="gen-panel" on:keydown=gen_key>
                        <input type="text" class="gen-search" placeholder="Search refs and tags…"
                            prop:value=move || query.get() on:input=move |ev| { query.set(event_target_value(&ev)); picked.set(0); }/>
                        {move || matches!(ctx().library.get(), None).then(|| view! { <span class="hint">"Reading the library…"</span> })}
                        <div class="gen-list">
                            {move || {
                                let l = list.get();
                                let mut group = "";
                                l.into_iter().enumerate().map(|(i, c)| {
                                    let head = (c.group != group).then(|| { group = c.group; view! { <div class="gen-group">{c.group}</div> } });
                                    let c2 = c.clone();
                                    view! {
                                        {head}
                                        <button type="button" class="gen-item" class:picked=move || picked.get() == i
                                            on:mousedown=move |ev: MouseEvent| { ev.prevent_default(); choose(c2.clone()); }>
                                            <span class="gen-label">{c.label.clone()}</span>
                                            <span class="gen-sub">{c.sub.clone()}</span>
                                        </button>
                                    }
                                }).collect_view()
                            }}
                        </div>
                    </div>
                }.into_any(),
                Panel::Menu { x, y, title, items } => view! {
                    <div class="menu-backdrop" on:mousedown=move |ev: MouseEvent| { ev.prevent_default(); panel.set(Panel::None); }
                        on:contextmenu=move |ev: MouseEvent| { ev.prevent_default(); panel.set(Panel::None); }></div>
                    <div class="menu prose-menu" style=format!("left: {x}px; top: {y}px")>
                        {title.map(|t| view! { <div class="menu-title">{t}</div> })}
                        {items.into_iter().map(|it| {
                            let (label, sub, disabled): (String, Option<String>, bool) = match &it {
                                MenuItem::Pill(c) => (format!("Make pill: {} \u{b7} {}", c.entry_name, c.form_label()), Some(c.kind.clone()), false),
                                MenuItem::Dismiss(_) => ("Not a pill (this session)".into(), None, false),
                                MenuItem::PillAll(c) => (format!("Pill every \u{201c}{}\u{201d} in this file\u{2026}", c.text), None, false),
                                MenuItem::Suggest(w) => (w.clone(), None, false),
                                MenuItem::NoSuggestions => ("No suggestions".into(), None, true),
                                MenuItem::Ignore(_) => ("Ignore (this session)".into(), None, false),
                                MenuItem::AddWord(_) => ("Add to project words".into(), Some("enigma-words.txt".into()), false),
                                MenuItem::MakePill => ("Make a pill\u{2026}".into(), Some("Ctrl+K".into()), false),
                                MenuItem::Variable => ("Turn into variable\u{2026}".into(), None, false),
                                MenuItem::Noun => ("Turn into a person, place or thing\u{2026}".into(), None, false),
                                MenuItem::Cut => ("Cut".into(), None, sel_range.with_value(|r| r.is_none())),
                                MenuItem::Copy => ("Copy".into(), None, sel_range.with_value(|r| r.is_none())),
                                MenuItem::Paste => ("Paste".into(), Some("as plain text".into()), false),
                                MenuItem::Sep => return view! { <div class="menu-sep"></div> }.into_any(),
                            };
                            let bold = matches!(it, MenuItem::Suggest(_) | MenuItem::Pill(_));
                            view! {
                                <button type="button" disabled=disabled class:menu-strong=bold
                                    on:mousedown=move |ev: MouseEvent| { ev.prevent_default(); run_item(it.clone()); }>
                                    <span>{label}</span>
                                    {sub.map(|s| view! { <span class="menu-desc">{s}</span> })}
                                </button>
                            }.into_any()
                        }).collect_view()}
                    </div>
                }.into_any(),
                Panel::Variable { value } => view! {
                    <div node_ref=panel_el class="insert-panel" on:keydown=move |ev: KeyboardEvent| {
                        if ev.key() == "Enter" { ev.prevent_default(); ev.stop_propagation(); save_variable(); }
                        if ev.key() == "Escape" { ev.stop_propagation(); panel.set(Panel::None); }
                    }>
                        <span class="insert-title">"Turn into variable"</span>
                        <label class="pill-field">"Name"
                            <input type="text" placeholder="bridgeToll" prop:value=move || var_name.get()
                                on:input=move |ev| { var_name.set(event_target_value(&ev)); form_err.set(None); }/>
                        </label>
                        <span class="hint">"Value: \u{201c}" {value.trim().to_string()} "\u{201d}. The text will refer to it by name; letters only."</span>
                        {move || form_err.get().map(|e| view! { <span class="hint bad">{e}</span> })}
                        <button type="button" class="small primary" on:click=move |_| save_variable()>"Make variable"</button>
                        <button type="button" class="small" on:click=move |_| panel.set(Panel::None)>"Cancel"</button>
                    </div>
                }.into_any(),
                Panel::Noun { text } => {
                    let text2 = text.clone();
                    view! {
                        <div node_ref=panel_el class="insert-panel" on:keydown=move |ev: KeyboardEvent| {
                            if ev.key() == "Enter" { ev.prevent_default(); ev.stop_propagation(); save_noun(); }
                            if ev.key() == "Escape" { ev.stop_propagation(); panel.set(Panel::None); }
                        }>
                            <span class="insert-title">"Turn into a person, place or thing"</span>
                            <label class="pill-field">"Name"
                                <input type="text" prop:value=move || noun_name.get() on:input=move |ev| { noun_name.set(event_target_value(&ev)); form_err.set(None); }/>
                            </label>
                            <label class="pill-field">"Short name"
                                <input type="text" placeholder="optional" prop:value=move || noun_short.get() on:input=move |ev| noun_short.set(event_target_value(&ev))/>
                            </label>
                            <label class="pill-field">"Kind"
                                <select on:change=move |ev| noun_kind.set(event_target_value(&ev))>
                                    {[("person", "Person"), ("place", "Place"), ("thing", "Thing"), ("", "\u{2014}")].into_iter().map(|(v, l)| view! {
                                        <option value=v selected=move || noun_kind.get() == v>{l}</option>
                                    }).collect_view()}
                                </select>
                            </label>
                            {move || {
                                let (n, s) = (noun_name.get(), noun_short.get());
                                let same = (!s.trim().is_empty() && prose::norm_eq(&text2, &s)) || prose::norm_eq(&text2, &n);
                                (!same && !n.trim().is_empty()).then(|| view! { <span class="hint">"The selection will print as \u{201c}" {n.trim().to_string()} "\u{201d}."</span> })
                            }}
                            {move || form_err.get().map(|e| view! { <span class="hint bad">{e}</span> })}
                            <button type="button" class="small primary" on:click=move |_| save_noun()>"Make it"</button>
                            <button type="button" class="small" on:click=move |_| panel.set(Panel::None)>"Cancel"</button>
                        </div>
                    }.into_any()
                }
                Panel::Tag { tag, chip, .. } => {
                    let spec = env.palette.spec(&tag);
                    view! {
                        <div node_ref=panel_el class="insert-panel" on:keydown=move |ev: KeyboardEvent| {
                            if ev.key() == "Enter" { ev.prevent_default(); ev.stop_propagation(); save_tag(); }
                            if ev.key() == "Escape" { ev.stop_propagation(); panel.set(Panel::None); }
                        }>
                            <span class="insert-title">{spec.map(|s| s.label).unwrap_or("Tag")}</span>
                            {match spec {
                                Some(s) => view! { <TagForm spec=s attrs=tag_attrs text=tag_text/> }.into_any(),
                                None => view! { <span class="hint">"This tag has no form; edit it in the Source view."</span> }.into_any(),
                            }}
                            {spec.is_some().then(|| view! {
                                <button type="button" class="small primary" on:click=move |_| save_tag()>{if chip.is_some() { "Save" } else { "Insert" }}</button>
                            })}
                            {chip.is_some().then(|| view! { <button type="button" class="small danger" on:click=move |_| remove_tag()>"Remove"</button> })}
                            <button type="button" class="small" on:click=move |_| panel.set(Panel::None)>"Cancel"</button>
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}
