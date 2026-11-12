//! Inline ref tags in the paragraph editor: the preview line under a paragraph's XML
//! (text as printed, each ref tag a pill showing what it prints), the panel that edits
//! one tag's attributes, and the Insert panel.
//!
//! The textarea stays the source of truth. The panels rewrite the tag's text in the
//! paragraph draft, so the paragraph's Apply writes it like any other edit.

use leptos::ev::KeyboardEvent;
use leptos::html;
use leptos::prelude::*;
use shared::inline::{self, Checker, Diagnostic, Piece, RefCtx, RefTag, Severity, Style, TAGS};

/// UTF-16 index (what a textarea counts) of byte `b` of `s`.
pub fn u16_of(s: &str, b: usize) -> u32 {
    let b = b.min(s.len());
    let b = (0..=b).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
    s[..b].encode_utf16().count() as u32
}

/// Byte of `s` at UTF-16 index `u`.
pub fn byte_of(s: &str, u: u32) -> usize {
    let mut n = 0u32;
    for (i, c) in s.char_indices() {
        if n >= u {
            return i;
        }
        n += c.len_utf16() as u32;
    }
    s.len()
}

/// Focuses the textarea and selects bytes `start..end` of `text` (its value).
pub fn select_range(ta: NodeRef<html::Textarea>, text: &str, start: usize, end: usize) {
    if let Some(t) = ta.get_untracked() {
        let _ = t.focus();
        let _ = t.set_selection_range(u16_of(text, start), u16_of(text, end));
    }
}

/// The textarea's selection as bytes of `text`.
pub fn selection(ta: NodeRef<html::Textarea>, text: &str) -> (usize, usize) {
    let Some(t) = ta.get_untracked() else { return (text.len(), text.len()) };
    let a = t.selection_start().ok().flatten().map(|u| byte_of(text, u)).unwrap_or(text.len());
    let b = t.selection_end().ok().flatten().map(|u| byte_of(text, u)).unwrap_or(a);
    (a.min(b), a.max(b))
}

/// A new tag of a kind, pointing at the first thing it can (the first entry, chapter...).
pub fn default_tag(tag: &str, ctx: &RefCtx, selected_text: &str) -> RefTag {
    let mut t = RefTag::new(tag);
    let Some(spec) = inline::spec(tag) else { return t };
    match tag {
        "chapterRef" => t.set("name", Some(ctx.chapters.first().map(|c| c.name.as_str()).unwrap_or(""))),
        "figureRef" => t.set("ref", Some(ctx.figures.first().map(|f| f.id.as_str()).unwrap_or(""))),
        _ => {
            if let Some(kind) = spec.kind {
                t.set("ref", Some(ctx.ids(kind).first().map(String::as_str).unwrap_or("")));
            }
            if let Some(v) = spec.values.first() {
                t.set("value", Some(v));
            }
        }
    }
    if tag == "coreBookChapterRef" {
        t.set("num", Some("1"));
    }
    if spec.has_text && !selected_text.contains('<') {
        t.text = selected_text.trim().to_string();
    }
    t
}

fn style_class(s: Style) -> &'static str {
    match s {
        Style::Creature => "st-creature",
        Style::Title => "st-title",
        Style::Plain => "st-plain",
    }
}

fn label_of(tag: &str) -> &'static str {
    inline::spec(tag).map(|s| s.label).unwrap_or("Tag")
}

/// The preview line and its problem list.
#[component]
pub fn RefPreview(
    text: Signal<String>,
    ctx: Memo<RefCtx>,
    /// The ref tag (by its position among the paragraph's refs) the edit panel shows.
    selected: RwSignal<Option<usize>>,
    /// Selects bytes of the text in the textarea.
    pick: Callback<(usize, usize)>,
) -> impl IntoView {
    let parsed = Memo::new(move |_| text.with(|t| inline::parse(t)));
    let diags = Memo::new(move |_| -> Vec<Diagnostic> {
        parsed.with(|p| match p {
            Ok(pieces) => text.with(|t| ctx.with(|c| c.check(t, pieces))),
            Err(_) => Vec::new(),
        })
    });
    let line = move || match parsed.get() {
        Err(e) => {
            let at = e.at;
            view! {
                <span class="pv-broken">
                    "Not well-formed XML: " {e.message}
                    <button type="button" class="link" on:click=move |_| pick.run((at, at))>"show"</button>
                </span>
            }
            .into_any()
        }
        Ok(pieces) => {
            let c = ctx.get();
            let mut n = 0usize;
            pieces
                .into_iter()
                .map(|p| match p {
                    Piece::Text { text, bold, italic, term, .. } => {
                        let class = format!(
                            "pv-text{}{}{}",
                            if bold { " b" } else { "" },
                            if italic { " i" } else { "" },
                            if term.is_some() { " term" } else { "" }
                        );
                        view! { <span class=class title=term.map(|t| format!("<{t}>")).unwrap_or_default()>{text}</span> }.into_any()
                    }
                    Piece::Ref(t) => {
                        let i = n;
                        n += 1;
                        let r = c.resolve(&t);
                        let bad = !r.problems.is_empty();
                        let class = format!(
                            "ref-pill tag-{} {}{}{}",
                            t.tag,
                            style_class(r.style),
                            if bad { " bad" } else { "" },
                            if r.known { "" } else { " unknown" }
                        );
                        let mut tip = t.to_xml();
                        for p in &r.problems {
                            tip.push_str("\n⚠ ");
                            tip.push_str(p);
                        }
                        view! {
                            <button type="button" class=class class:sel=move || selected.get() == Some(i) title=tip
                                on:click=move |_| selected.update(|s| *s = if *s == Some(i) { None } else { Some(i) })>
                                {bad.then(|| view! { <span class="pill-warn">"⚠"</span> })}
                                {r.text}
                            </button>
                        }
                        .into_any()
                    }
                })
                .collect_view()
                .into_any()
        }
    };
    view! {
        <div class="p-preview" title="As printed. Click a tag to edit it.">{line}</div>
        <Show when=move || diags.with(|d| !d.is_empty())>
            <ul class="pv-diags">
                {move || diags.get().into_iter().map(|d| {
                    let (a, b) = (d.start, d.end);
                    let nth = parsed.with_untracked(|p| p.as_ref().ok().and_then(|p| inline::refs(p).position(|t| t.start == a)));
                    view! {
                        <li class:error=d.severity == Severity::Error on:click=move |_| {
                            pick.run((a, b));
                            if let Some(n) = nth { selected.set(Some(n)) }
                        }>
                            <span class="diag-mark">"⚠"</span>{d.message}
                        </li>
                    }
                }).collect_view()}
            </ul>
        </Show>
    }
}

fn select_field(
    label: &'static str,
    opts: impl Fn() -> Vec<(String, String)> + Send + Sync + 'static,
    cur: impl Fn() -> String + Copy + Send + Sync + 'static,
    on: impl Fn(String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <select on:change=move |ev| on(event_target_value(&ev))>
                {move || {
                    let c = cur();
                    let mut o = opts();
                    if !o.iter().any(|(v, _)| *v == c) {
                        let l = if c.is_empty() { "(none)".to_string() } else { format!("{c} — not found") };
                        o.insert(0, (c.clone(), l));
                    }
                    o.into_iter().map(|(v, l)| {
                        let s = v == c;
                        view! { <option value=v selected=s>{l}</option> }
                    }).collect_view()
                }}
            </select>
        </label>
    }
}

fn text_field(
    label: &'static str,
    hint: &'static str,
    cur: impl Fn() -> String + Copy + Send + Sync + 'static,
    on: impl Fn(String) + Copy + Send + Sync + 'static,
) -> impl IntoView {
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input type="text" prop:value=cur on:input=move |ev| on(event_target_value(&ev))/>
            <span class="hint">{hint}</span>
        </label>
    }
}

/// The fields for one tag's attributes. Every change calls `set` with the new tag.
#[component]
pub fn RefFields(model: Signal<RefTag>, set: Callback<RefTag>, ctx: Memo<RefCtx>) -> impl IntoView {
    let tag = Memo::new(move |_| model.with(|t| t.tag.clone()));
    let attr = move |k: &'static str| move || model.with(|t| t.get(k).unwrap_or_default().to_string());
    let put = move |k: &'static str| {
        move |v: String| {
            let mut t = model.get_untracked();
            t.set(k, Some(&v));
            set.run(t);
        }
    };
    // An empty value drops the attribute.
    let put_opt = move |k: &'static str| {
        move |v: String| {
            let mut t = model.get_untracked();
            t.set(k, (!v.is_empty()).then_some(v.as_str()));
            set.run(t);
        }
    };
    let entry_opts = move |kind: &'static str, none: Option<&'static str>| {
        move || {
            ctx.with(|c| {
                none.map(|n| (String::new(), n.to_string()))
                    .into_iter()
                    .chain(c.ids(kind).into_iter().map(|id| {
                        let l = match c.entry_label(&id) {
                            Some(n) if n != id => format!("{id} — {n}"),
                            _ => id.clone(),
                        };
                        (id, l)
                    }))
                    .collect()
            })
        }
    };
    let emph = move || {
        select_field(
            "Style",
            || vec![
                (String::new(), "House style".into()),
                ("true".into(), "Always styled (emphasized=\"true\")".into()),
                ("false".into(), "Never styled (emphasized=\"false\")".into()),
            ],
            attr("emphasized"),
            put_opt("emphasized"),
        )
    };
    let no_ref = Memo::new(move |_| model.with(|t| t.get("ref").unwrap_or_default().is_empty()));

    let fields = move || {
        let t = tag.get();
        let Some(spec) = inline::spec(&t) else { return view! { <p class="hint">"Not a ref tag."</p> }.into_any() };
        match spec.tag {
            "chapterRef" => view! {
                {select_field("Chapter", move || ctx.with(|c| c.chapters.iter().map(|ch| {
                    let l = match &ch.printed { Ok(p) => format!("{} — {p}", ch.name), Err(_) => format!("{} — {} (not numbered)", ch.name, ch.title) };
                    (ch.name.clone(), l)
                }).collect()), attr("name"), put("name"))}
                {emph()}
            }.into_any(),
            "figureRef" => view! {
                {select_field("Figure", move || ctx.with(|c| c.figures.iter().map(|f| {
                    (f.id.clone(), format!("{} — {}", f.id, f.printed.clone().unwrap_or_else(|_| f.name.clone())))
                }).collect()), attr("ref"), put("ref"))}
                {emph()}
            }.into_any(),
            "monster" => view! {
                {select_field("Entry", entry_opts("monster", Some("(no entry: describe it here)")), attr("ref"), put_opt("ref"))}
                {text_field("Display", "@display: the lead name, in place of the entry's", attr("display"), put_opt("display"))}
                <Show when=move || no_ref.get()>
                    {text_field("Name", "@name: the stat block's name", attr("name"), put_opt("name"))}
                    {text_field("CR", "@cr", attr("cr"), put_opt("cr"))}
                    {text_field("Source", "@src", attr("src"), put_opt("src"))}
                </Show>
                {emph()}
            }.into_any(),
            _ if spec.has_text => {
                let text = move || model.with(|t| t.text.clone());
                let set_text = move |v: String| {
                    let mut t = model.get_untracked();
                    t.text = v;
                    set.run(t);
                };
                view! {
                    {text_field("Text", "the name as the book prints it", text, set_text)}
                    {(spec.tag == "coreBookChapterRef").then(|| text_field("Chapter no.", "@num", attr("num"), put("num")))}
                }.into_any()
            }
            _ => {
                let kind = spec.kind.unwrap_or_default();
                let values = spec.values;
                view! {
                    {select_field("Entry", entry_opts(kind, None), attr("ref"), put("ref"))}
                    {select_field("Value", move || {
                        let id = model.with(|t| t.get("ref").unwrap_or_default().to_string());
                        ctx.with(|c| values.iter().map(|v| {
                            let l = match c.value(&id, v) {
                                Some(Ok(s)) => format!("{v} — {s}"),
                                Some(Err(_)) => format!("{v} — (can't print)"),
                                None => v.to_string(),
                            };
                            (v.to_string(), l)
                        }).collect())
                    }, attr("value"), put("value"))}
                    {emph()}
                }.into_any()
            }
        }
    };
    let printed = move || {
        let t = model.get();
        let r = ctx.with(|c| c.resolve(&t));
        view! {
            <div class="ref-prints">
                <span class="hint">"Prints: "</span>
                <span class=format!("ref-pill {}{}", style_class(r.style), if r.known { "" } else { " unknown" })>{r.text}</span>
                {r.problems.into_iter().map(|p| view! { <span class="hint bad block">"⚠ "{p}</span> }).collect_view()}
            </div>
        }
    };
    view! {
        <div class="ref-fields">{fields}</div>
        {printed}
    }
}

/// The panel for the selected ref tag of a paragraph.
#[component]
pub fn RefEditPanel(
    text: Signal<String>,
    ctx: Memo<RefCtx>,
    selected: RwSignal<Option<usize>>,
    /// Sets the paragraph draft.
    set_text: Callback<String>,
    pick: Callback<(usize, usize)>,
) -> impl IntoView {
    let current = Memo::new(move |_| {
        let n = selected.get()?;
        text.with(|t| inline::parse(t).ok().and_then(|p| inline::refs(&p).nth(n).cloned()))
    });
    let rewrite = move |with: String| {
        if let Some(old) = current.get_untracked() {
            let t = text.get_untracked();
            set_text.run(inline::replace(&t, old.start, old.end, &with));
        }
    };
    let model = Signal::derive(move || current.get().unwrap_or_else(|| RefTag::new("monsterRef")));
    let set = Callback::new(move |t: RefTag| rewrite(t.to_xml()));
    let unlink = move |_| {
        if let Some(old) = current.get_untracked() {
            let shown = ctx.with_untracked(|c| c.resolve(&old)).text;
            rewrite(shared::edit::escape_text(&shown));
            selected.set(None);
        }
    };
    let remove = move |_| {
        rewrite(String::new());
        selected.set(None);
    };
    view! {
        <Show when=move || current.with(|c| c.is_some())>
            <div class="ref-panel" on:keydown=move |ev: KeyboardEvent| if ev.key() == "Escape" {
                ev.stop_propagation();
                selected.set(None);
            }>
                <div class="ref-panel-head">
                    <span class="ref-panel-title">{move || current.with(|c| c.as_ref().map(|c| label_of(&c.tag)).unwrap_or(""))}</span>
                    <code class="ref-panel-xml">{move || current.with(|c| c.as_ref().map(|c| c.to_xml()).unwrap_or_default())}</code>
                    <span class="spacer"></span>
                    <button type="button" class="small" title="Select the tag in the text"
                        on:click=move |_| if let Some(c) = current.get_untracked() { pick.run((c.start, c.end)) }>"Select in text"</button>
                    <button type="button" class="small" title="Replace the tag with the words it prints" on:click=unlink>"Unlink"</button>
                    <button type="button" class="small danger" title="Delete the tag" on:click=remove>"Remove"</button>
                    <button type="button" class="icon-btn" title="Close (Esc)" on:click=move |_| selected.set(None)>"✕"</button>
                </div>
                <RefFields model set ctx/>
            </div>
        </Show>
    }
}

/// The Insert panel: pick a kind and its attributes, then insert at the caret.
#[component]
pub fn RefInsertPanel(
    /// The tag being built; None when the panel is closed.
    draft: RwSignal<Option<RefTag>>,
    /// The text selected when the panel opened, for tags that take text.
    selected_text: StoredValue<String>,
    ctx: Memo<RefCtx>,
    insert: Callback<String>,
) -> impl IntoView {
    let model = Signal::derive(move || draft.get().unwrap_or_else(|| RefTag::new("monsterRef")));
    let set = Callback::new(move |t: RefTag| draft.set(Some(t)));
    let kind = move || draft.with(|d| d.as_ref().map(|d| d.tag.clone()).unwrap_or_default());
    view! {
        <Show when=move || draft.with(|d| d.is_some())>
            <div class="ref-panel insert" on:keydown=move |ev: KeyboardEvent| {
                if ev.key() == "Escape" {
                    ev.stop_propagation();
                    draft.set(None);
                } else if ev.key() == "Enter" && !ev.ctrl_key() {
                    ev.prevent_default();
                    ev.stop_propagation();
                    if let Some(t) = draft.get_untracked() { insert.run(t.to_xml()) }
                }
            }>
                <div class="ref-panel-head">
                    <span class="ref-panel-title">"Insert a ref"</span>
                    <span class="spacer"></span>
                    <button type="button" class="small primary" on:click=move |_| if let Some(t) = draft.get_untracked() { insert.run(t.to_xml()) }>"Insert"</button>
                    <button type="button" class="icon-btn" title="Close (Esc)" on:click=move |_| draft.set(None)>"✕"</button>
                </div>
                <div class="ref-fields">
                    {select_field("Kind", || TAGS.iter().map(|s| (s.tag.to_string(), format!("{} <{}>", s.label, s.tag))).collect(), kind, move |t: String| {
                        let sel = selected_text.get_value();
                        draft.set(Some(ctx.with_untracked(|c| default_tag(&t, c, &sel))));
                    })}
                </div>
                <RefFields model set ctx/>
            </div>
        </Show>
    }
}
