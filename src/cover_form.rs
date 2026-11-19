//! The `<cover>` edit form, with a rough preview of the printed cover.
//!
//! Edits go into the tab's cover draft. Apply sends the draft to the backend, which
//! plans the same surgical edits against the file on disk, validates the result, and
//! writes it only if it validates; otherwise the errors go to the Problems panel.

use leptos::prelude::*;
use leptos::task::spawn_local;
use shared::cover::{self, CoverImage, CoverModel};
use shared::Change;
use shared::values::{css_color, image_choices, is_length, length_fraction, ImageChoice, COLORS};

use crate::api;
use crate::apply::apply;
use crate::state::DocTab;

/// US Letter, which dndbook uses.
const PAGE_W: f64 = 8.5;
const PAGE_H: f64 = 11.0;

/// The draft, with the file's cover as the fallback. Copy, so every field can hold one.
#[derive(Clone, Copy)]
struct Form {
    tab: DocTab,
    base: Memo<CoverModel>,
}

impl Form {
    fn model(&self) -> CoverModel {
        self.tab.cover_draft.get().unwrap_or_else(|| self.base.get())
    }

    fn update(&self, f: impl FnOnce(&mut CoverModel)) {
        let mut m = self.tab.cover_draft.get_untracked().unwrap_or_else(|| self.base.get_untracked());
        f(&mut m);
        let same = m == self.base.get_untracked();
        self.tab.cover_draft.set((!same).then_some(m));
    }
}

/// Applies a cover model to the tab's file. Shared by Apply, Remove cover and Add cover.
pub fn apply_model(tab: DocTab, model: CoverModel) {
    apply(tab, Change::Cover { model });
}

/// A length field's problem, if any.
fn length_problem(v: &str) -> Option<&'static str> {
    (!v.is_empty() && !is_length(v)).then_some("A LaTeX length: 0.5in, 2cm, 36pt, or a % of the page")
}

fn color_problem(v: &str) -> Option<&'static str> {
    let v = v.trim();
    if v.starts_with('#') && css_color(v).is_none() {
        return Some("A hex colour needs six digits, like #1A2B3C");
    }
    None
}

#[component]
fn TextField(
    form: Form,
    label: &'static str,
    #[prop(optional)] hint: &'static str,
    #[prop(optional)] placeholder: &'static str,
    get: fn(&CoverModel) -> String,
    set: fn(&mut CoverModel, String),
    #[prop(optional)] check: Option<fn(&str) -> Option<&'static str>>,
    #[prop(optional)] list: &'static str,
) -> impl IntoView {
    let value = move || get(&form.model());
    let problem = move || check.and_then(|c| c(&value()));
    view! {
        <label class="field">
            <span class="field-label">{label}</span>
            <input
                type="text"
                class:invalid=move || problem().is_some()
                placeholder=placeholder
                list=list
                prop:value=value
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    form.update(|m| set(m, v));
                }
            />
            {move || match problem() {
                Some(p) => view! { <span class="hint bad">{p}</span> }.into_any(),
                None => (!hint.is_empty()).then(|| view! { <span class="hint">{hint}</span> }).into_any(),
            }}
        </label>
    }
}

#[component]
fn ColorField(
    form: Form,
    label: &'static str,
    placeholder: &'static str,
    get: fn(&CoverModel) -> String,
    set: fn(&mut CoverModel, String),
    #[prop(optional)] disabled: Option<fn(&CoverModel) -> bool>,
) -> impl IntoView {
    let value = move || get(&form.model());
    let off = move || disabled.is_some_and(|d| d(&form.model()));
    let swatch = move || {
        let v = value();
        let v = if v.trim().is_empty() { placeholder.to_string() } else { v };
        css_color(&v)
    };
    view! {
        <label class="field" class:disabled=off>
            <span class="field-label">{label}</span>
            <div class="with-swatch">
                <span
                    class="swatch"
                    class:unknown=move || swatch().is_none()
                    title=move || if swatch().is_some() { "Approximate colour" } else { "Can't preview this colour" }
                    style=move || swatch().map(|c| format!("background: {c}")).unwrap_or_default()
                ></span>
                <input
                    type="text"
                    list="cover-colors"
                    placeholder=placeholder
                    disabled=off
                    class:invalid=move || color_problem(&value()).is_some()
                    prop:value=value
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        form.update(|m| set(m, v));
                    }
                />
            </div>
            {move || match color_problem(&value()) {
                Some(p) => view! { <span class="hint bad">{p}</span> }.into_any(),
                None => view! {
                    <span class="hint">"A colour name (white, titlered, titlegold…), a mix like red!60!black, or #1A2B3C"</span>
                }.into_any(),
            }}
        </label>
    }
}

#[component]
fn Check(
    form: Form,
    label: &'static str,
    #[prop(optional)] hint: &'static str,
    get: fn(&CoverModel) -> bool,
    set: fn(&mut CoverModel, bool),
) -> impl IntoView {
    view! {
        <label class="check">
            <input
                type="checkbox"
                prop:checked=move || get(&form.model())
                on:change=move |ev| {
                    let v = event_target_checked(&ev);
                    form.update(|m| set(m, v));
                }
            />
            <span>
                <span class="field-label">{label}</span>
                {(!hint.is_empty()).then(|| view! { <span class="hint block">{hint}</span> })}
            </span>
        </label>
    }
}

fn img_mut(m: &mut CoverModel) -> &mut CoverImage {
    m.image.get_or_insert_with(CoverImage::default)
}

#[component]
fn CoverArt(form: Form, choices: Memo<Vec<ImageChoice>>) -> impl IntoView {
    let on = move || form.model().image.is_some();
    let current_ref = move || form.model().image.map(|i| i.image_ref).unwrap_or_default();
    let toggle = move |ev| {
        let checked = event_target_checked(&ev);
        let first = choices.get_untracked().first().map(|c| c.id.clone());
        form.update(|m| {
            if checked {
                let r = if !m.legacy_id.is_empty() { m.legacy_id.clone() } else { first.unwrap_or_default() };
                m.image = Some(CoverImage { image_ref: r, ..Default::default() });
            } else {
                m.image = None;
            }
        });
    };
    let options = move || {
        let cur = current_ref();
        let list = choices.get();
        let missing = !cur.is_empty() && !list.iter().any(|c| c.id == cur);
        let mut out: Vec<AnyView> = Vec::new();
        if cur.is_empty() {
            out.push(view! { <option value="" selected>"Choose an image…"</option> }.into_any());
        }
        if missing {
            let label = format!("{cur} (no <image> has this id)");
            out.push(view! { <option value=cur.clone() selected>{label}</option> }.into_any());
        }
        for c in list {
            let label = match (&c.name, &c.src) {
                (Some(n), Some(s)) => format!("{} — {n} ({s})", c.id),
                (None, Some(s)) => format!("{} ({s})", c.id),
                _ => c.id.clone(),
            };
            let sel = c.id == cur;
            out.push(view! { <option value=c.id.clone() selected=sel>{label}</option> }.into_any());
        }
        out
    };
    let size_set = move || {
        let m = form.model();
        m.image.is_some_and(|i| !i.width.is_empty() || !i.height.is_empty())
    };
    view! {
        <fieldset class="group">
            <legend>"Cover art"</legend>
            <label class="check">
                <input type="checkbox" prop:checked=on on:change=toggle/>
                <span><span class="field-label">"Print cover art behind the text"</span>
                    <span class="hint block">"<coverImage>: an <image> from the supplement, centred on the page"</span></span>
            </label>
            <Show when=on>
                <label class="field">
                    <span class="field-label">"Image"</span>
                    <select on:change=move |ev| {
                        let v = event_target_value(&ev);
                        form.update(|m| img_mut(m).image_ref = v);
                    }>{options}</select>
                    {move || choices.with(|c| c.is_empty()).then(|| view! {
                        <span class="hint bad">"This supplement's <reference> has no <image> yet. Cover art must name one there."</span>
                    })}
                </label>
                <div class="field-row">
                    <TextField form label="Margin" placeholder="0" check=length_problem
                        hint="Clear space on every side when fitting"
                        get=|m| m.image.as_ref().map(|i| i.margin.clone()).unwrap_or_default()
                        set=|m, v| img_mut(m).margin = v/>
                    <TextField form label="Width" placeholder="fit" check=length_problem
                        get=|m| m.image.as_ref().map(|i| i.width.clone()).unwrap_or_default()
                        set=|m, v| img_mut(m).width = v/>
                    <TextField form label="Height" placeholder="fit" check=length_problem
                        get=|m| m.image.as_ref().map(|i| i.height.clone()).unwrap_or_default()
                        set=|m, v| img_mut(m).height = v/>
                </div>
                <div class="field-row">
                    <TextField form label="Move right" placeholder="0" check=length_problem
                        hint="xOffset; negative moves left"
                        get=|m| m.image.as_ref().map(|i| i.x_offset.clone()).unwrap_or_default()
                        set=|m, v| img_mut(m).x_offset = v/>
                    <TextField form label="Move up" placeholder="0" check=length_problem
                        hint="yOffset; negative moves down"
                        get=|m| m.image.as_ref().map(|i| i.y_offset.clone()).unwrap_or_default()
                        set=|m, v| img_mut(m).y_offset = v/>
                </div>
                <Show when=move || size_set() && form.model().image.is_some_and(|i| !i.margin.is_empty())>
                    <p class="hint bad">"The margin is ignored when a width or height is set (the compiler warns)."</p>
                </Show>
            </Show>
        </fieldset>
    }
}

#[component]
pub fn CoverForm(tab: DocTab) -> impl IntoView {
    let base = Memo::new(move |_| {
        let d = tab.doc();
        cover::read_nodes(&d.source, &d.nodes)
    });
    let form = Form { tab, base };
    // coverImage @ref must name a <reference> image (the coverImageTarget keyref).
    let choices = Memo::new(move |_| {
        image_choices(&tab.doc().source).into_iter().filter(|c| c.place == "reference").collect::<Vec<_>>()
    });
    let dirty = move || tab.cover_draft.with(|d| d.is_some());
    let busy = move || tab.applying.get();

    let apply = move |_| {
        if let Some(m) = tab.cover_draft.get_untracked() {
            apply_model(tab, m);
        }
    };
    let revert = move |_| tab.cover_draft.set(None);
    let remove = move |_| {
        crate::dialogs::ask("Remove the cover from this supplement? Its title, art and footer lines are deleted from the file.", move || {
            // The contents-page logo isn't part of the cover: it stays.
            let m = base.get_untracked();
            apply_model(tab, CoverModel { contents_logo: m.contents_logo, contents_attrs: m.contents_attrs, ..Default::default() });
        });
    };
    let convert_legacy = move |_| {
        form.update(|m| {
            let id = std::mem::take(&mut m.legacy_id);
            if m.image.is_none() {
                m.image = Some(CoverImage { image_ref: id, ..Default::default() });
            }
        })
    };

    view! {
        <div class="form-layout">
            <div class="form">
                <datalist id="cover-colors">
                    {COLORS.iter().map(|(n, _)| view! { <option value=*n></option> }).collect_view()}
                </datalist>
                <Show when=move || !form.model().legacy_id.is_empty()>
                    <div class="notice">
                        {move || format!("This cover names its art with the deprecated id=\"{}\".", form.model().legacy_id)}
                        <button on:click=convert_legacy>"Convert to <coverImage>"</button>
                    </div>
                </Show>

                <fieldset class="group">
                    <legend>"Title"</legend>
                    <TextField form label="Title" hint="<headerTitle>: large, near the top" placeholder="(none)"
                        get=|m| m.header_title.clone() set=|m, v| m.header_title = v/>
                    <TextField form label="Subtitle" hint="<headerSubtitle>: the line under the title" placeholder="(none)"
                        get=|m| m.header_subtitle.clone() set=|m, v| m.header_subtitle = v/>
                </fieldset>

                <CoverArt form choices/>

                <fieldset class="group">
                    <legend>"Footer"</legend>
                    <TextField form label="Chaser" hint="<footerChaser>: text near the bottom" placeholder="(none)"
                        get=|m| m.footer_chaser.clone() set=|m, v| m.footer_chaser = v/>
                    <TextField form label="Subchaser" hint="<footerSubchaser>: a smaller line under the chaser" placeholder="(none)"
                        get=|m| m.footer_subchaser.clone() set=|m, v| m.footer_subchaser = v/>
                    <TextField form label="Author" placeholder="(none)"
                        get=|m| m.author.clone() set=|m, v| m.author = v/>
                    <Check form label="Print the author line exactly as written"
                        hint="override=\"true\": without it, the compiler adds \"Written by\""
                        get=|m| m.author_override set=|m, v| m.author_override = v/>
                    {move || {
                        let m = form.model();
                        (!m.author.trim().is_empty()).then(|| {
                            let line = if m.author_override { m.author.trim().to_string() } else { format!("Written by {}", m.author.trim()) };
                            let warn = !m.author_override && m.author.trim().to_lowercase().starts_with("written by");
                            view! {
                                <p class="hint">"Prints: " <em>{line}</em></p>
                                {warn.then(|| view! { <p class="hint bad">"It already starts with \"Written by\"; tick the box above or write just the name."</p> })}
                            }
                        })
                    }}
                </fieldset>

                <fieldset class="group">
                    <legend>"Text colour"</legend>
                    <ColorField form label="Colour" placeholder="white"
                        get=|m| m.color.clone() set=|m, v| m.color = v/>
                    <Check form label="Drop shadow" hint="Draws the text again behind itself, nudged down and right"
                        get=|m| m.shadow set=|m, v| m.shadow = v/>
                    <ColorField form label="Shadow colour" placeholder="black"
                        disabled=|m| !m.shadow
                        get=|m| m.shadow_color.clone() set=|m, v| m.shadow_color = v/>
                </fieldset>

                <fieldset class="group">
                    <legend>"Logo"</legend>
                    <Check form label="DMs Guild logo"
                        hint="Lower-left corner. Untick for releases on other platforms."
                        get=|m| m.dmsguild_logo set=|m, v| m.dmsguild_logo = v/>
                    {move || {
                        let m = form.model();
                        (m.dmsguild_logo && !m.logo_attrs.is_empty()).then(|| {
                            let attrs = m.logo_attrs.iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect::<Vec<_>>().join(" ");
                            view! { <p class="hint">"Kept as written: " <code>{attrs}</code></p> }
                        })
                    }}
                    <Check form label="Enigma Narrative logo"
                        hint="The same size as the DMs Guild logo unless its width or height says otherwise."
                        get=|m| m.enigma_logo set=|m, v| m.enigma_logo = v/>
                    {move || form.model().enigma_logo.then(|| view! {
                        <label class="field">
                            <span class="field-label">"Placed"</span>
                            <select on:change=move |ev| { let v = event_target_value(&ev); form.update(move |m| m.enigma_place = v.clone()); }>
                                <option value="opposite" selected=move || form.model().enigma_place != "above">"Opposite the DMs Guild logo (lower right)"</option>
                                <option value="above" selected=move || form.model().enigma_place == "above">"Just above the DMs Guild logo"</option>
                            </select>
                        </label>
                    })}
                    {move || {
                        let m = form.model();
                        let above_alone = m.enigma_logo && m.enigma_place == "above" && !m.dmsguild_logo;
                        above_alone.then(|| view! { <p class="hint bad">"\u{201c}Just above\u{201d} sits on the DMs Guild logo: tick it too, or the compiler stops."</p> })
                    }}
                    {move || {
                        let m = form.model();
                        let extra: Vec<String> = m.enigma_attrs.iter().filter(|(k, _)| k != "place").map(|(k, v)| format!("{k}=\"{v}\"")).collect();
                        (m.enigma_logo && !extra.is_empty()).then(|| view! { <p class="hint">"Kept as written: " <code>{extra.join(" ")}</code></p> })
                    }}
                    <Check form label="Enigma Narrative logo on the contents page"
                        hint="Top-right corner of the contents page, beside the heading; the entries don't move. DMsGuild doesn't allow imprint logos on the cover, so for a DMsGuild release use this instead of the cover logo."
                        get=|m| m.contents_logo set=|m, v| m.contents_logo = v/>
                    {move || {
                        let m = form.model();
                        (m.contents_logo && !m.contents_attrs.is_empty()).then(|| {
                            let attrs = m.contents_attrs.iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect::<Vec<_>>().join(" ");
                            view! { <p class="hint">"Kept as written: " <code>{attrs}</code></p> }
                        })
                    }}
                </fieldset>

                <div class="form-actions">
                    <button class="danger" on:click=remove disabled=busy>"Remove cover"</button>
                    <span class="spacer"></span>
                    {move || dirty().then(|| view! { <span class="dirty-note">"Unapplied changes"</span> })}
                    <button on:click=revert disabled=move || !dirty() || busy()>"Revert"</button>
                    <button class="primary" on:click=apply disabled=move || !dirty() || busy()>
                        {move || if busy() { "Validating…" } else { "Apply" }}
                    </button>
                </div>
            </div>
            <CoverPreview form choices/>
        </div>
    }
}

/// inches -> a CSS length in the page's container-width units.
fn inch(v: f64) -> String {
    format!("{:.3}cqw", v * 100.0 / PAGE_W)
}

fn pt(v: f64) -> String {
    inch(v / 72.27)
}

fn dir_of(path: &str) -> &str {
    path.rfind(['\\', '/']).map_or("", |i| &path[..i])
}

/// The Enigma Narrative logo's file, which the compiler looks for upward.
const ENIGMA_LOGO: &str = "Enigma_Narrative-2-01_bordered.png";

#[component]
fn CoverPreview(form: Form, choices: Memo<Vec<ImageChoice>>) -> impl IntoView {
    let tab = form.tab;
    let logo_src: RwSignal<Option<String>> = RwSignal::new(None);
    let enigma_src: RwSignal<Option<String>> = RwSignal::new(None);
    // Find the logo files, as the compiler would, whenever a logo or its src changes.
    Effect::new(move |_| {
        let m = form.model();
        let src = m.logo_attrs.iter().find(|(k, _)| k == "src").map(|(_, v)| v.clone());
        let path = tab.doc().path.clone();
        if m.dmsguild_logo {
            spawn_local(async move { logo_src.set(api::find_logo(&path, src.as_deref(), None).await) });
        }
    });
    Effect::new(move |_| {
        let m = form.model();
        let src = m.enigma_attrs.iter().find(|(k, _)| k == "src").map(|(_, v)| v.clone());
        let path = tab.doc().path.clone();
        if m.enigma_logo {
            spawn_local(async move { enigma_src.set(api::find_logo(&path, src.as_deref(), Some(ENIGMA_LOGO)).await) });
        }
    });

    let art = move || {
        let m = form.model();
        let img = m.image?;
        let choice = choices.get().into_iter().find(|c| c.id == img.image_ref)?;
        let doc_path = tab.doc().path.clone();
        let file = format!("{}\\{}", dir_of(&doc_path), choice.src?.replace('/', "\\"));
        let url = api::convert_file_src(&file);
        let len = |s: &str, page: f64| length_fraction(s, page).map(|f| f * page);
        let (h, w) = (len(&img.height, PAGE_H), len(&img.width, PAGE_W));
        let mut style = String::new();
        match (w, h) {
            (None, None) => {
                let m = len(&img.margin, PAGE_W).unwrap_or(0.0);
                style += &format!("width:{};height:{};object-fit:contain;", inch(PAGE_W - 2.0 * m), inch(PAGE_H - 2.0 * m));
            }
            (Some(w), None) => style += &format!("width:{};height:auto;", inch(w)),
            (None, Some(h)) => style += &format!("height:{};width:auto;", inch(h)),
            (Some(w), Some(h)) => style += &format!("width:{};height:{};object-fit:contain;", inch(w), inch(h)),
        }
        let dx = len(&img.x_offset, PAGE_W).unwrap_or(0.0);
        let dy = len(&img.y_offset, PAGE_H).unwrap_or(0.0);
        style += &format!("transform:translate(calc(-50% + {}), calc(-50% - {}));", inch(dx), inch(dy));
        Some(view! { <img class="art" src=url style=style/> })
    };

    let text_style = move |size_pt: f64, shadow_pt: f64| {
        let m = form.model();
        let color = css_color(if m.color.trim().is_empty() { "white" } else { &m.color }).unwrap_or_else(|| "white".into());
        let mut s = format!("color:{color};font-size:{};", pt(size_pt));
        if m.shadow {
            let sc = css_color(if m.shadow_color.trim().is_empty() { "black" } else { &m.shadow_color })
                .unwrap_or_else(|| "black".into());
            s += &format!("text-shadow:{o} {o} 0 {sc};", o = pt(shadow_pt));
        }
        s
    };
    let line = move |text: String, top: Option<f64>, bottom: Option<f64>, size: f64, shadow: f64| {
        (!text.trim().is_empty()).then(|| {
            let pos = match (top, bottom) {
                (Some(t), _) => format!("top:{};", inch(t)),
                (_, Some(b)) => format!("bottom:{};", inch(b)),
                _ => String::new(),
            };
            view! { <div class="cover-line" style=format!("{pos}{}", text_style(size, shadow))>{text}</div> }
        })
    };

    // A logo's size, from its own width/height or else `fallback` (the DMs Guild logo's).
    fn size_css(attrs: &[(String, String)], fallback: Option<&[(String, String)]>) -> String {
        let len = |a: &[(String, String)], k: &str, page: f64| a.iter().find(|(n, _)| n == k).and_then(|(_, v)| length_fraction(v, page)).map(|f| f * page);
        let mut wh = (len(attrs, "width", PAGE_W), len(attrs, "height", PAGE_H));
        if wh == (None, None) {
            if let Some(f) = fallback {
                wh = (len(f, "width", PAGE_W), len(f, "height", PAGE_H));
            }
        }
        match wh {
            (None, None) => format!("width:{};", inch(1.0)),
            (Some(w), None) => format!("width:{};", inch(w)),
            (None, Some(h)) => format!("height:{};", inch(h)),
            (Some(w), Some(h)) => format!("max-width:{};max-height:{};", inch(w), inch(h)),
        }
    }
    let img = move |file: Option<String>, style: String, what: &'static str| match file {
        Some(file) => view! { <img class="logo" src=api::convert_file_src(&file) style=style/> }.into_any(),
        None => view! {
            <div class="logo missing" style=format!("{style}height:{};", inch(0.6)) title=format!("No {what} file found in this folder or any folder above: the compiler stops")>
                {format!("{what} (file not found)")}
            </div>
        }
        .into_any(),
    };
    let logo = move || {
        let m = form.model();
        let get = |a: &[(String, String)], k: &str| a.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let len = |a: &[(String, String)], k: &str, page: f64| get(a, k).and_then(|v| length_fraction(&v, page)).map(|f| f * page);
        let margin = len(&m.logo_attrs, "margin", PAGE_W).unwrap_or(0.5);
        let x = len(&m.logo_attrs, "xMargin", PAGE_W).unwrap_or(margin);
        let y = len(&m.logo_attrs, "yMargin", PAGE_H).unwrap_or(margin);
        let guild_size = size_css(&m.logo_attrs, None);
        let enigma_size = size_css(&m.enigma_attrs, Some(&m.logo_attrs));
        let above = m.enigma_logo && m.enigma_place == "above";
        let mut out = Vec::new();
        if m.dmsguild_logo && above {
            // A stack in the lower-left: the Enigma logo, the gap, the DMs Guild logo.
            let gap = len(&m.enigma_attrs, "gap", PAGE_H).unwrap_or(0.15);
            out.push(view! {
                <div class="logo-stack" style=format!("left:{};bottom:{};gap:{};", inch(x), inch(y), inch(gap))>
                    {img(enigma_src.get(), enigma_size.clone(), "Enigma Narrative logo")}
                    {img(logo_src.get(), guild_size.clone(), "DMs Guild logo")}
                </div>
            }.into_any());
        } else {
            if m.dmsguild_logo {
                out.push(img(logo_src.get(), format!("left:{};bottom:{};{guild_size}", inch(x), inch(y)), "DMs Guild logo"));
            }
            if m.enigma_logo && !above {
                let em = len(&m.enigma_attrs, "margin", PAGE_W).unwrap_or(margin);
                let ex = len(&m.enigma_attrs, "xMargin", PAGE_W).unwrap_or(em);
                let ey = len(&m.enigma_attrs, "yMargin", PAGE_H).or(len(&m.logo_attrs, "yMargin", PAGE_H)).unwrap_or(em);
                out.push(img(enigma_src.get(), format!("right:{};bottom:{};{enigma_size}", inch(ex), inch(ey)), "Enigma Narrative logo"));
            }
        }
        out.collect_view()
    };

    view! {
        <div class="preview-col">
            <div class="cover-page">
                {art}
                {move || line(form.model().header_title, Some(1.25), None, 40.0, 2.0)}
                {move || line(form.model().header_subtitle, Some(2.15), None, 17.28, 1.0)}
                {move || line(form.model().footer_chaser, None, Some(1.15), 12.0, 1.0)}
                {move || line(form.model().footer_subchaser, None, Some(0.85), 10.0, 1.0)}
                {move || {
                    let m = form.model();
                    let text = if m.author.trim().is_empty() {
                        String::new()
                    } else if m.author_override {
                        m.author.trim().to_string()
                    } else {
                        format!("Written by {}", m.author.trim())
                    };
                    line(text, None, Some(0.45), 10.0, 1.0)
                }}
                {logo}
            </div>
            <p class="hint">"A rough preview at US Letter size, not the compiled layout."</p>
        </div>
    }
}
