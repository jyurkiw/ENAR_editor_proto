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
    let env = ProseEnv { tab, palette, doc, own, noun };
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
    // This box's misspelling ranges, in the page's "spelling" highlight.
    let marks = StoredValue::new_local(Vec::<web_sys::Range>::new());
    let check = move || {
        if let Some(div) = el.get_untracked() {
            marks.update_value(|m| crate::spelling::mark(&div, m));
        }
    };
    // Check once the dictionary arrives; drop this box's marks when it goes away.
    Effect::new(move |_| {
        if crate::spelling::ready() {
            check();
        }
    });
    on_cleanup(move || {
        marks.try_update_value(|m| crate::spelling::clear(m));
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
    let choose = move |c: Choice| match c.act {
        Act::Insert(x) => insert_xml(x),
        Act::AddEntry(entry, x) => {
            apply(env.tab, Change::Reference { edit: RefEdit::Add { entry } });
            insert_xml(x);
        }
        Act::NewTag(tag) => {
            let sel = query.get_untracked();
            tag_attrs.set(Vec::new());
            tag_text.set(sel);
            panel.set(Panel::Tag { tag: tag.to_string(), attrs: Vec::new(), text: String::new(), chip: None });
            focus_panel();
        }
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
                    on:input=move |_| { emit(); check(); } on:keydown=on_key on:click=on_click></div>
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
