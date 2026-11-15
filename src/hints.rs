//! Underlines in prose boxes, drawn with the CSS Custom Highlight API so a box's DOM
//! (and so its XML) is never touched:
//!
//! - `pillable` (blue): a mention of one of the document's own entries, which can be
//!   turned into a ref ([`shared::pills`]).
//! - `spelling` (red): a word the dictionary doesn't know ([`crate::spelling`]).
//!
//! Each box keeps its own ranges ([`BoxMarks`]) and re-marks itself after every redraw
//! and edit, when the dictionary arrives, when the document's entries change, and when
//! a mention is dismissed.

use std::cell::RefCell;
use std::collections::HashSet;

use shared::pills::{self, Candidate};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    /// A CSS custom highlight: a set of ranges styled by `::highlight(name)`.
    #[wasm_bindgen(js_name = Highlight)]
    type JsHighlight;
    #[wasm_bindgen(constructor, js_class = "Highlight")]
    fn new() -> JsHighlight;
    #[wasm_bindgen(method, js_class = "Highlight")]
    fn add(this: &JsHighlight, r: &web_sys::Range);
    #[wasm_bindgen(method, js_class = "Highlight")]
    fn delete(this: &JsHighlight, r: &web_sys::Range) -> bool;

    /// `CSS.highlights`, the page's registry of named highlights.
    type HighlightRegistry;
    #[wasm_bindgen(method)]
    fn set(this: &HighlightRegistry, name: &str, h: &JsHighlight);
}

/// `CSS.highlights` (a property, so it's read rather than called).
fn registry() -> Option<HighlightRegistry> {
    let css = js_sys::Reflect::get(&js_sys::global(), &"CSS".into()).ok()?;
    let reg = js_sys::Reflect::get(&css, &"highlights".into()).ok()?;
    (!reg.is_undefined()).then(|| reg.unchecked_into())
}

const SPELLING: usize = 0;
const PILLABLE: usize = 1;
const NAMES: [&str; 2] = ["spelling", "pillable"];

thread_local! {
    static HIGHLIGHTS: RefCell<Option<[JsHighlight; 2]>> = const { RefCell::new(None) };
    /// Mentions dismissed this session: (file, normalized text).
    static DISMISSED: RefCell<HashSet<(String, String)>> = RefCell::new(HashSet::new());
    /// Bumped on each dismissal, so boxes re-mark.
    static DISMISS_VERSION: leptos::prelude::ArcRwSignal<u32> = leptos::prelude::ArcRwSignal::new(0);
}

/// Runs `f` on the named highlights, registering them the first time.
fn with_highlights(f: impl FnOnce(&[JsHighlight; 2])) {
    HIGHLIGHTS.with(|h| {
        let mut h = h.borrow_mut();
        if h.is_none() {
            let Some(reg) = registry() else { return };
            let made = [JsHighlight::new(), JsHighlight::new()];
            for (name, hl) in NAMES.iter().zip(&made) {
                reg.set(name, hl);
            }
            *h = Some(made);
        }
        if let Some(h) = h.as_ref() {
            f(h);
        }
    });
}

fn norm(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ").replace('\u{2019}', "'").to_lowercase()
}

/// Stops offering `text` as a pill in `file` for the rest of the session.
pub fn dismiss(file: &str, text: &str) {
    DISMISSED.with(|d| d.borrow_mut().insert((file.to_string(), norm(text))));
    DISMISS_VERSION.with(|v| leptos::prelude::Update::update(v, |n| *n += 1));
}

fn is_dismissed(file: &str, text: &str) -> bool {
    DISMISSED.with(|d| d.borrow().contains(&(file.to_string(), norm(text))))
}

/// Tracks dismissals (for a box's re-mark effect).
pub fn track_dismissals() {
    DISMISS_VERSION.with(|v| leptos::prelude::Track::track(v));
}

/// A mention under a blue underline, and what it could be made into.
pub struct PillMark {
    pub range: web_sys::Range,
    pub text: String,
    pub cands: Vec<Candidate>,
}

/// One box's ranges in the page's highlights.
#[derive(Default)]
pub struct BoxMarks {
    spelling: Vec<web_sys::Range>,
    pub pills: Vec<PillMark>,
}

/// Drops a box's ranges from the highlights (before re-marking, or when it goes away).
pub fn clear(m: &mut BoxMarks) {
    with_highlights(|h| {
        for r in m.spelling.drain(..) {
            h[SPELLING].delete(&r);
        }
        for p in m.pills.drain(..) {
            h[PILLABLE].delete(&p.range);
        }
    });
    m.spelling.clear();
    m.pills.clear();
}

/// A run of text nodes with no chip or line break between them, joined.
struct Run {
    text: String,
    /// Each node and the byte offset its text starts at in `text`.
    nodes: Vec<(web_sys::Node, usize)>,
}

impl Run {
    /// The node and UTF-16 offset for byte `off`. An end offset prefers the node it ends
    /// in; a start offset, the node it starts in.
    fn locate(&self, off: usize, is_end: bool) -> Option<(&web_sys::Node, u32)> {
        let i = self.nodes.iter().rposition(|(_, s)| if is_end { *s < off } else { *s <= off }).unwrap_or(0);
        let (node, start) = self.nodes.get(i)?;
        let end = self.nodes.get(i + 1).map(|n| n.1).unwrap_or(self.text.len());
        let local = &self.text[*start..end];
        Some((node, local[..(off - start).min(local.len())].encode_utf16().count() as u32))
    }

    fn range(&self, doc: &web_sys::Document, s: usize, e: usize) -> Option<web_sys::Range> {
        let (sn, so) = self.locate(s, false)?;
        let (en, eo) = self.locate(e, true)?;
        let r = doc.create_range().ok()?;
        r.set_start(sn, so).ok()?;
        r.set_end(en, eo).ok()?;
        Some(r)
    }
}

/// The box's text, as runs broken at chips and line breaks.
fn runs(div: &web_sys::HtmlElement) -> Vec<Run> {
    fn walk(n: &web_sys::Node, out: &mut Vec<Run>) {
        let kids = n.child_nodes();
        for i in 0..kids.length() {
            let Some(k) = kids.item(i) else { continue };
            if k.node_type() == web_sys::Node::TEXT_NODE {
                let t = k.text_content().unwrap_or_default();
                let run = out.last_mut().expect("a run is always open");
                run.nodes.push((k, run.text.len()));
                run.text.push_str(&t);
                continue;
            }
            let Some(el) = k.dyn_ref::<web_sys::Element>() else { continue };
            let tag = el.tag_name().to_ascii_lowercase();
            let breaks = el.class_list().contains("chip-tag") || matches!(tag.as_str(), "br" | "div" | "p");
            if breaks {
                out.push(Run { text: String::new(), nodes: Vec::new() });
            }
            if !el.class_list().contains("chip-tag") {
                walk(&k, out);
                if breaks {
                    out.push(Run { text: String::new(), nodes: Vec::new() });
                }
            }
        }
    }
    let mut out = vec![Run { text: String::new(), nodes: Vec::new() }];
    walk(div, &mut out);
    out.retain(|r| !r.text.trim().is_empty());
    out
}

/// Re-marks one box: its old ranges go, and each mention of a candidate (not dismissed
/// in `file`) and each misspelled word outside those mentions gets a range.
pub fn mark(div: &web_sys::HtmlElement, m: &mut BoxMarks, file: &str, cands: &[Candidate]) {
    clear(m);
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    for run in runs(div) {
        let found: Vec<pills::Match> = pills::find(&run.text, cands).into_iter().filter(|x| !is_dismissed(file, &run.text[x.start..x.end])).collect();
        for x in &found {
            if let Some(range) = run.range(&doc, x.start, x.end) {
                m.pills.push(PillMark { range, text: run.text[x.start..x.end].to_string(), cands: x.cands.iter().map(|i| cands[*i].clone()).collect() });
            }
        }
        for (s, e) in crate::spelling::words(&run.text) {
            if found.iter().any(|x| s < x.end && x.start < e) || !crate::spelling::misspelled(&run.text[s..e]) {
                continue;
            }
            if let Some(r) = run.range(&doc, s, e) {
                m.spelling.push(r);
            }
        }
    }
    with_highlights(|h| {
        for r in &m.spelling {
            h[SPELLING].add(r);
        }
        for p in &m.pills {
            h[PILLABLE].add(&p.range);
        }
    });
}
