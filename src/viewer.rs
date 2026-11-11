//! The main area: the selected element's edit form (for elements that have one) or its
//! source, re-indented and highlighted; and the Problems panel docked at the bottom.

use leptos::prelude::*;
use shared::cover::{self, CoverModel};
use shared::display::{reindent, tokenize};
use shared::xml::parse;
use shared::Validation;

use crate::cover_form::{apply_model, CoverForm};
use crate::state::{ctx, DocTab, Mode};

/// Tags with an edit form.
fn has_form(tag: &str) -> bool {
    tag == "cover"
}

fn highlighted(text: &str) -> impl IntoView {
    tokenize(text).into_iter().map(|(kind, s)| view! { <span class=kind.class()>{s.to_string()}</span> }).collect_view()
}

#[component]
pub fn Viewer() -> impl IntoView {
    let app = ctx();
    move || match app.active_tab() {
        None => view! {
            <div class="viewer-empty">
                {move || if app.project.get().is_some() {
                    "Right-click an .xml file in the Files panel and choose Open."
                } else {
                    "Open a project folder to begin."
                }}
            </div>
        }
        .into_any(),
        Some(tab) => view! { <TabView tab/> }.into_any(),
    }
}

#[component]
fn TabView(tab: DocTab) -> impl IntoView {
    let selected = tab.selected;
    let sel_tag = Memo::new(move |_| {
        let d = tab.doc();
        d.nodes.get(selected.get()).map(|n| n.tag.clone()).unwrap_or_default()
    });
    let form_tag = move || has_form(&sel_tag.get());
    let mode = tab.mode;

    let crumbs = move || {
        let idx = selected.get();
        let d = tab.doc();
        d.path_to(idx.min(d.nodes.len() - 1))
            .into_iter()
            .map(|i| {
                let n = &d.nodes[i];
                let current = i == idx;
                let text = if n.label.is_empty() || n.leaf { n.tag.clone() } else { format!("{} ({})", n.tag, n.label) };
                view! { <span class="crumb" class:current=current on:click=move |_| tab.reveal(i)>{text}</span> }
            })
            .collect_view()
    };
    let lines = move || {
        let d = tab.doc();
        let n = d.nodes.get(selected.get())?;
        let last = n.line + d.source[n.start..n.end].matches('\n').count();
        Some(if last == n.line { format!("line {}", n.line) } else { format!("lines {}–{}", n.line, last) })
    };

    // Source: the file's text for the element; for the cover with a draft, the text it
    // would have after Apply.
    let source = move || {
        let d = tab.doc();
        let Some(n) = d.nodes.get(selected.get()) else { return ().into_any() };
        if n.tag == "cover" {
            if let Some(draft) = tab.cover_draft.get() {
                let planned = cover::plan(&d.source, &draft).and_then(|t| parse(&t).map(|p| (t, p.nodes)).map_err(|e| e.message));
                return match planned {
                    Ok((text, nodes)) => match cover::cover_node(&nodes) {
                        Some(ci) => view! {
                            <div class="source-note">"Showing the text after Apply (not yet written)."</div>
                            <pre class="code">{highlighted(&reindent(&text, nodes[ci].start, nodes[ci].end))}</pre>
                        }
                        .into_any(),
                        None => view! { <div class="source-note">"Apply removes the cover."</div> }.into_any(),
                    },
                    Err(e) => view! { <div class="source-note bad">{format!("Can't plan the edit: {e}")}</div> }.into_any(),
                };
            }
        }
        view! { <pre class="code">{highlighted(&reindent(&d.source, n.start, n.end))}</pre> }.into_any()
    };

    // The supplement has no cover: offer one when the root is selected.
    let no_cover = move || {
        let d = tab.doc();
        selected.get() == 0 && cover::cover_node(&d.nodes).is_none()
    };
    let add_cover = move |_| apply_model(tab, CoverModel { present: true, ..Default::default() });

    let status = move || {
        let d = tab.doc();
        let mut notes: Vec<(&'static str, String)> = Vec::new();
        match &d.validation {
            Validation::Valid { schema } => notes.push(("ok", format!("Valid against {schema}"))),
            Validation::NotRun { reason } => notes.push(("warn", format!("Schema name checked, but not XSD-validated: {reason}"))),
        }
        notes.extend(d.warnings.iter().map(|w| ("warn", w.clone())));
        notes.into_iter().map(|(kind, text)| view! { <div class=format!("status {kind}")>{text}</div> }).collect_view()
    };

    view! {
        <div class="viewer">
            <div class="viewer-head">
                <div class="crumbs">{crumbs}</div>
                <Show when=form_tag>
                    <div class="mode-switch">
                        <button class:active=move || mode.get() == Mode::Form on:click=move |_| mode.set(Mode::Form)>"Form"</button>
                        <button class:active=move || mode.get() == Mode::Source on:click=move |_| mode.set(Mode::Source)>"Source"</button>
                    </div>
                </Show>
                <div class="lines">{lines}</div>
            </div>
            <Show when=no_cover>
                <div class="notice">
                    "This supplement has no cover."
                    <button on:click=add_cover disabled=move || tab.applying.get()>"Add a cover"</button>
                </div>
            </Show>
            <div class="viewer-body">
                {move || if form_tag() && mode.get() == Mode::Form {
                    view! { <CoverForm tab/> }.into_any()
                } else {
                    source().into_any()
                }}
            </div>
            <ProblemsPanel tab/>
            <div class="statusbar">{status}</div>
        </div>
    }
}

/// The bottom panel listing the last refused apply's validation errors, each with the
/// line of edited text it points at.
#[component]
fn ProblemsPanel(tab: DocTab) -> impl IntoView {
    let open = tab.problems_open;
    let count = move || tab.problems.with(|p| p.len());
    let rows = move || {
        let checked = tab.problem_source.get();
        let lines: Vec<&str> = checked.lines().collect();
        tab.problems
            .get()
            .into_iter()
            .map(|p| {
                let at = p.line.and_then(|l| lines.get(l.saturating_sub(1))).map(|l| l.trim().to_string());
                view! {
                    <div class="problem-row">
                        <span class="problem-line">{p.line.map(|l| format!("Ln {l}")).unwrap_or_default()}</span>
                        <span class="problem-msg">
                            {p.message}
                            {at.map(|t| view! { <code class="problem-code">{t}</code> })}
                        </span>
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <section class="problems" class:open=move || open.get()>
            <header on:click=move |_| open.update(|o| *o = !*o)>
                <span class="twist">{move || if open.get() { "▾" } else { "▸" }}</span>
                "Problems"
                <span class="badge" class:zero=move || count() == 0>{count}</span>
                <span class="spacer"></span>
                <Show when=move || { count() > 0 }>
                    <span class="problems-note">"The last Apply was refused: the edited file didn't validate, so nothing was written."</span>
                    <button class="small" on:click=move |ev| {
                        ev.stop_propagation();
                        tab.clear_problems();
                    }>"Clear"</button>
                </Show>
            </header>
            <Show when=move || open.get()>
                <div class="problem-list">
                    {move || if count() == 0 {
                        view! { <div class="problem-empty">"No problems."</div> }.into_any()
                    } else {
                        rows().into_any()
                    }}
                </div>
            </Show>
        </section>
    }
}
