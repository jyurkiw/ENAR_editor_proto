//! Spellcheck: an en_US Hunspell dictionary (SCOWL), loaded in the background and
//! checked with `spellbook`. [`crate::hints`] draws what it finds.

use std::cell::RefCell;

use leptos::logging::log;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::spawn_local;

thread_local! {
    /// Set once the dictionary is loaded, so boxes drawn before then get checked.
    static READY: leptos::prelude::ArcRwSignal<bool> = leptos::prelude::ArcRwSignal::new(false);
    static DICT: RefCell<Option<spellbook::Dictionary>> = const { RefCell::new(None) };
    /// Words ignored this session.
    static IGNORED: RefCell<std::collections::HashSet<String>> = RefCell::new(Default::default());
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

/// Whether `word` is misspelled; false until the dictionary is loaded.
pub fn misspelled(word: &str) -> bool {
    let w = word.replace('\u{2019}', "'");
    if IGNORED.with(|i| i.borrow().contains(&w)) {
        return false;
    }
    DICT.with(|d| d.borrow().as_ref().is_some_and(|dict| !dict.check(&w)))
}

/// Stops marking `word` for the rest of the session.
pub fn ignore(word: &str) {
    IGNORED.with(|i| i.borrow_mut().insert(word.replace('\u{2019}', "'")));
    crate::hints::bump();
}

/// Up to five spellings the dictionary suggests for `word`.
pub fn suggest(word: &str) -> Vec<String> {
    let w = word.replace('\u{2019}', "'");
    DICT.with(|d| {
        let mut out = Vec::new();
        if let Some(dict) = d.borrow().as_ref() {
            dict.suggest(&w, &mut out);
        }
        out.truncate(5);
        out
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

/// Check and suggestion timings for a word (the spike's console probe).
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
