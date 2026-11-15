//! Spellcheck for prose boxes (phase 0 spike): an en_US Hunspell dictionary checked with
//! `spellbook`, misspellings drawn with the CSS Custom Highlight API so the box's DOM is
//! never touched.

use std::cell::RefCell;

use leptos::logging::log;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    /// A CSS custom highlight: a set of ranges styled by `::highlight(name)`.
    #[wasm_bindgen(js_name = Highlight)]
    pub type JsHighlight;
    #[wasm_bindgen(constructor, js_class = "Highlight")]
    fn new() -> JsHighlight;
    #[wasm_bindgen(method, js_class = "Highlight")]
    fn add(this: &JsHighlight, r: &web_sys::Range);
    #[wasm_bindgen(method, js_class = "Highlight")]
    fn delete(this: &JsHighlight, r: &web_sys::Range) -> bool;
    #[wasm_bindgen(method, getter, js_class = "Highlight")]
    fn size(this: &JsHighlight) -> u32;

    /// `CSS.highlights`, the page's registry of named highlights.
    type HighlightRegistry;
    #[wasm_bindgen(method)]
    fn set(this: &HighlightRegistry, name: &str, h: &JsHighlight);
}

/// `CSS.highlights` (a property, so it's read rather than called).
fn highlights() -> Option<HighlightRegistry> {
    let css = js_sys::Reflect::get(&js_sys::global(), &"CSS".into()).ok()?;
    let reg = js_sys::Reflect::get(&css, &"highlights".into()).ok()?;
    (!reg.is_undefined()).then(|| reg.unchecked_into())
}

thread_local! {
    /// Set once the dictionary is loaded, so boxes drawn before then get checked.
    static READY: leptos::prelude::ArcRwSignal<bool> = leptos::prelude::ArcRwSignal::new(false);
    static DICT: RefCell<Option<spellbook::Dictionary>> = const { RefCell::new(None) };
    static SPELLING: RefCell<Option<JsHighlight>> = const { RefCell::new(None) };
}

fn now() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

async fn fetch_text(url: &str) -> Result<String, JsValue> {
    let w = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(w.fetch_with_str(url)).await?.dyn_into()?;
    if !resp.ok() {
        return Err(format!("{url}: HTTP {}", resp.status()).into());
    }
    let t = wasm_bindgen_futures::JsFuture::from(resp.text()?).await?;
    Ok(t.as_string().unwrap_or_default())
}

/// Loads the dictionary in the background; boxes are checked once it's ready.
pub fn start() {
    spawn_local(async {
        let t0 = now();
        let (aff, dic) = match (fetch_text("dict/en_US.aff").await, fetch_text("dict/en_US.dic").await) {
            (Ok(a), Ok(d)) => (a, d),
            (a, d) => {
                log!("spelling: dictionary not loaded: {:?} {:?}", a.err(), d.err());
                return;
            }
        };
        let t1 = now();
        match spellbook::Dictionary::new(&aff, &dic) {
            Ok(d) => {
                let t2 = now();
                log!("spelling: fetched in {:.0} ms, parsed in {:.0} ms", t1 - t0, t2 - t1);
                DICT.with(|x| *x.borrow_mut() = Some(d));
                READY.with(|r| leptos::prelude::Set::set(r, true));
                // Spike: a console probe, `__spellProbe("word")`.
                let f = Closure::<dyn Fn(String) -> String>::new(|w: String| spelling_probe(&w));
                let _ = js_sys::Reflect::set(&js_sys::global(), &"__spellProbe".into(), f.as_ref());
                f.forget();
            }
            Err(e) => log!("spelling: dictionary didn't parse: {e}"),
        }
    });
}

/// Whether the dictionary is loaded (tracked).
pub fn ready() -> bool {
    READY.with(|r| leptos::prelude::Get::get(r))
}

/// Drops a box's ranges from the highlight (when it re-checks, or goes away).
pub fn clear(old: &mut Vec<web_sys::Range>) {
    if let Some(h) = registry_highlight() {
        for r in old.drain(..) {
            h.delete(&r);
        }
    }
}

fn registry_highlight() -> Option<JsHighlight> {
    SPELLING.with(|s| {
        let mut s = s.borrow_mut();
        if s.is_none() {
            let h = JsHighlight::new();
            highlights()?.set("spelling", &h);
            *s = Some(h);
        }
        s.as_ref().map(|h| h.unchecked_ref::<JsValue>().clone().unchecked_into())
    })
}

/// The words of `text` worth checking: (byte start, byte end). Letters and inner
/// apostrophes; anything with a digit (dice, sizes) is skipped.
pub fn words(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let is_word = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let mut push = |s: usize, e: usize| {
        let w = text[s..e].trim_matches(|c| c == '\'' || c == '\u{2019}');
        if w.is_empty() || w.chars().any(|c| c.is_ascii_digit()) {
            return;
        }
        let off = s + text[s..e].find(w).unwrap_or(0);
        out.push((off, off + w.len()));
    };
    for (i, c) in text.char_indices() {
        match (is_word(c), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                push(s, i);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        push(s, text.len());
    }
    out
}

/// Byte offset in `s` to the UTF-16 offset a DOM `Range` wants.
fn utf16_at(s: &str, byte: usize) -> u32 {
    s[..byte].encode_utf16().count() as u32
}

/// Re-checks one prose box: drops its old ranges from the highlight and adds a range for
/// each misspelled word in its text (chips skipped). `old` holds the box's ranges.
pub fn mark(div: &web_sys::HtmlElement, old: &mut Vec<web_sys::Range>) {
    clear(old);
    let Some(h) = registry_highlight() else { return };
    let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
    let t0 = now();
    let mut checked = 0;
    DICT.with(|d| {
        let d = d.borrow();
        let Some(dict) = d.as_ref() else { return };
        // Text nodes outside chips, in order.
        let mut stack: Vec<web_sys::Node> = vec![div.clone().into()];
        while let Some(n) = stack.pop() {
            if n.node_type() == web_sys::Node::TEXT_NODE {
                let text = n.text_content().unwrap_or_default();
                for (s, e) in words(&text) {
                    checked += 1;
                    let w = text[s..e].replace('\u{2019}', "'");
                    if dict.check(&w) {
                        continue;
                    }
                    let Ok(r) = doc.create_range() else { continue };
                    if r.set_start(&n, utf16_at(&text, s)).is_ok() && r.set_end(&n, utf16_at(&text, e)).is_ok() {
                        h.add(&r);
                        old.push(r);
                    }
                }
                continue;
            }
            if let Some(el) = n.dyn_ref::<web_sys::Element>() {
                if el.class_list().contains("chip-tag") {
                    continue;
                }
            }
            let kids = n.child_nodes();
            for i in (0..kids.length()).rev() {
                if let Some(k) = kids.item(i) {
                    stack.push(k);
                }
            }
        }
    });
    let ms = now() - t0;
    if ms > 2.0 {
        log!("spelling: {checked} words checked in {ms:.1} ms");
    }
}

/// Suggestions for a word (for the spike's timing check, from the console).
pub fn spelling_probe(word: &str) -> String {
    DICT.with(|d| {
        let d = d.borrow();
        let Some(dict) = d.as_ref() else { return "not loaded".to_string() };
        let t0 = now();
        let ok = dict.check(word);
        let t1 = now();
        let mut sug = Vec::new();
        dict.suggest(word, &mut sug);
        let t2 = now();
        format!("{word}: ok={ok} check={:.2}ms suggest={:.1}ms {:?}", t1 - t0, t2 - t1, sug)
    })
}

#[cfg(test)]
mod tests {
    use super::words;
    #[test]
    fn splits_words() {
        let t = "The goblin's 2d6 axe, 'quoted' — naïve";
        let got: Vec<&str> = words(t).into_iter().map(|(s, e)| &t[s..e]).collect();
        assert_eq!(got, ["The", "goblin's", "axe", "quoted", "naïve"]);
    }
}
