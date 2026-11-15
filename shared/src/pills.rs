//! Pill hints: finding mentions, in plain prose text, of the things a document's own
//! `<reference>` entries name, so the editor can offer to turn each into a ref ("pill").
//!
//! Only the document's own entries are candidates (monsters, nouns, spells, items), one
//! per name form a ref can print. Data entries never are: a variable is referred to by
//! its name, never by its value.

use std::collections::HashMap;

use crate::edit::escape_attr;
use crate::inline::RefCtx;
use crate::prose::Palette;

/// One way of naming one entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// "monster", "noun", "spell" or "item".
    pub kind: String,
    pub id: String,
    /// "name", "displayName" or "shortName".
    pub form: String,
    /// The text as printed.
    pub text: String,
    /// The entry's name, for menus ("Alecia Something").
    pub entry_name: String,
    /// The ref that prints it: `<nounRef ref="alecia" value="shortName"/>`.
    pub ref_xml: String,
    /// Matched ignoring case (the text is all lowercase: "the old mill").
    pub any_case: bool,
}

impl Candidate {
    /// The form, as a menu says it.
    pub fn form_label(&self) -> &'static str {
        match self.form.as_str() {
            "displayName" => "display name",
            "shortName" => "short name",
            _ => "name",
        }
    }
}

/// The name forms each kind's ref can print, in preference order.
fn forms(kind: &str) -> &'static [&'static str] {
    match kind {
        "monster" | "noun" => &["name", "displayName", "shortName"],
        "item" => &["name", "displayName"],
        "spell" => &["name"],
        _ => &[],
    }
}

/// The candidates for a document: each entry once per distinct name form it has, from
/// the printed values in `ctx.index` (nothing until the backend has sent them).
pub fn candidates(ctx: &RefCtx, palette: Palette) -> Vec<Candidate> {
    let Some(index) = &ctx.index else { return Vec::new() };
    let mut out = Vec::new();
    for (kind, id) in &ctx.entries {
        let Some(rt) = palette.ref_tag(kind) else { continue };
        let Some(e) = index.entries.get(id) else { continue };
        let entry_name = e.values.get("name").cloned().unwrap_or_else(|| id.clone());
        let mut seen: Vec<&str> = Vec::new();
        for form in forms(kind) {
            let Some(text) = e.values.get(*form).map(|t| t.trim()).filter(|t| !t.is_empty()) else { continue };
            if seen.iter().any(|s| *s == text) {
                continue;
            }
            seen.push(text);
            out.push(Candidate {
                kind: kind.clone(),
                id: id.clone(),
                form: form.to_string(),
                text: text.to_string(),
                entry_name: entry_name.clone(),
                ref_xml: format!("<{rt} ref=\"{}\" value=\"{form}\"/>", escape_attr(id, '"')),
                any_case: !text.chars().any(char::is_uppercase),
            });
        }
    }
    out
}

/// One mention: the byte range in the text, and every candidate it could be (two
/// entries can share a short name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
    pub cands: Vec<usize>,
}

fn is_apos(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

fn same_char(a: char, b: char, any_case: bool) -> bool {
    a == b || (is_apos(a) && is_apos(b)) || (any_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Where `cand` ends if it matches `text` from byte `pos`: whitespace in the name
/// matches any run of whitespace, the two apostrophes match each other, and the match
/// must end at a word boundary.
fn match_at(text: &str, pos: usize, cand: &Candidate) -> Option<usize> {
    let mut t = text[pos..].char_indices().peekable();
    let mut c = cand.text.chars().peekable();
    while let Some(cc) = c.next() {
        if cc.is_whitespace() {
            while c.peek().is_some_and(|x| x.is_whitespace()) {
                c.next();
            }
            let mut any = false;
            while t.peek().is_some_and(|(_, x)| x.is_whitespace()) {
                t.next();
                any = true;
            }
            if !any {
                return None;
            }
            continue;
        }
        let (_, tc) = t.next()?;
        if !same_char(cc, tc, cand.any_case) {
            return None;
        }
    }
    let end = t.peek().map(|(i, _)| pos + i).unwrap_or(text.len());
    // A word boundary after: "Alec" isn't in "Alecia".
    let last = cand.text.chars().last()?;
    if last.is_alphanumeric() && text[end..].chars().next().is_some_and(|x| x.is_alphanumeric()) {
        return None;
    }
    Some(end)
}

/// The key a candidate is filed under: its first character, lowercased.
fn key(c: char) -> char {
    if is_apos(c) {
        '\''
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Every mention of a candidate in `text`, left to right. At each place the longest
/// match wins ("Alecia Something" over "Alecia"), matches don't overlap, and candidates
/// matching the same text are grouped.
pub fn find(text: &str, cands: &[Candidate]) -> Vec<Match> {
    let mut by_first: HashMap<char, Vec<usize>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        if let Some(f) = c.text.chars().next() {
            by_first.entry(key(f)).or_default().push(i);
        }
    }
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    let mut skip_to = 0;
    for (pos, ch) in text.char_indices() {
        let starts_word = !prev.is_some_and(|p| p.is_alphanumeric()) || !ch.is_alphanumeric();
        prev = Some(ch);
        if pos < skip_to || !starts_word || ch.is_whitespace() {
            continue;
        }
        let Some(list) = by_first.get(&key(ch)) else { continue };
        let mut best: Option<(usize, Vec<usize>)> = None;
        for &i in list {
            let Some(end) = match_at(text, pos, &cands[i]) else { continue };
            match &mut best {
                Some((e, ids)) if *e == end => ids.push(i),
                Some((e, _)) if *e > end => {}
                _ => best = Some((end, vec![i])),
            }
        }
        if let Some((end, ids)) = best {
            out.push(Match { start: pos, end, cands: ids });
            skip_to = end;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::{IndexEntry, RefIndex};

    fn ctx(entries: &[(&str, &str, &[(&str, &str)])]) -> RefCtx {
        let mut index = RefIndex::default();
        for (kind, id, values) in entries {
            index.entries.insert(
                id.to_string(),
                IndexEntry { kind: kind.to_string(), values: values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), errors: HashMap::new() },
            );
        }
        RefCtx { entries: entries.iter().map(|(k, i, _)| (k.to_string(), i.to_string())).collect(), index: Some(index), ..Default::default() }
    }

    fn found(text: &str, cands: &[Candidate]) -> Vec<String> {
        find(text, cands)
            .iter()
            .map(|m| format!("{}={}", &text[m.start..m.end], m.cands.iter().map(|i| format!("{}.{}", cands[*i].id, cands[*i].form)).collect::<Vec<_>>().join("|")))
            .collect()
    }

    fn alecia() -> Vec<Candidate> {
        candidates(
            &ctx(&[
                ("noun", "alecia", &[("name", "Alecia Something"), ("shortName", "Alecia")]),
                ("noun", "alec", &[("name", "Alec Hart"), ("shortName", "Alec")]),
            ]),
            Palette::Supplement,
        )
    }

    #[test]
    fn candidates_per_form() {
        let c = candidates(
            &ctx(&[
                ("noun", "mill", &[("name", "Reed Mill"), ("displayName", "Reed Mill"), ("shortName", "the mill")]),
                ("spell", "sleep", &[("name", "Sleep"), ("src", "SRD")]),
                ("item", "rope", &[("name", "Rope of Climbing"), ("displayName", "the rope")]),
                ("data", "toll", &[("value", "5 gp")]),
                ("monster", "gob", &[("name", "Goblin"), ("shortName", "goblin")]),
            ]),
            Palette::Supplement,
        );
        let got: Vec<String> = c.iter().map(|c| format!("{}:{}:{}", c.id, c.form, c.text)).collect();
        assert_eq!(
            got,
            ["mill:name:Reed Mill", "mill:shortName:the mill", "sleep:name:Sleep", "rope:name:Rope of Climbing", "rope:displayName:the rope", "gob:name:Goblin", "gob:shortName:goblin"]
        );
        assert_eq!(c[1].ref_xml, r#"<nounRef ref="mill" value="shortName"/>"#);
        assert!(c[1].any_case && !c[0].any_case);
        // No nouns outside a supplement; nothing before the backend sends values.
        assert!(candidates(&ctx(&[("noun", "mill", &[("name", "Reed Mill")])]), Palette::MagicItem).is_empty());
        assert!(candidates(&RefCtx { entries: vec![("noun".into(), "x".into())], ..Default::default() }, Palette::Supplement).is_empty());
    }

    #[test]
    fn longest_match_wins() {
        let c = alecia();
        assert_eq!(found("Alecia Something waves. Alecia smiles.", &c), ["Alecia Something=alecia.name", "Alecia=alecia.shortName"]);
        // Whitespace in the text may differ from the name's.
        assert_eq!(found("Alecia\n   Something", &c), ["Alecia\n   Something=alecia.name"]);
    }

    #[test]
    fn whole_words_only() {
        let c = alecia();
        // Alec isn't in Alecia (Alecia is, as itself); nor in Smartalec or Alecs.
        assert_eq!(found("Alecia met Alec, not Smartalec or Alecs.", &c), ["Alecia=alecia.shortName", "Alec=alec.shortName"]);
    }

    #[test]
    fn possessives_and_punctuation() {
        let c = alecia();
        assert_eq!(found("Alec's dog; (Alec) \"Alec!\" Alec\u{2019}s hat", &c), ["Alec=alec.shortName", "Alec=alec.shortName", "Alec=alec.shortName", "Alec=alec.shortName"]);
        // A name with an apostrophe matches either apostrophe.
        let c = candidates(&ctx(&[("noun", "den", &[("name", "Krugga's Den")])]), Palette::Supplement);
        assert_eq!(found("to Krugga\u{2019}s Den.", &c), ["Krugga\u{2019}s Den=den.name"]);
    }

    #[test]
    fn case_rules() {
        let c = candidates(&ctx(&[("noun", "rose", &[("name", "Rose")]), ("noun", "mill", &[("name", "the old mill")])]), Palette::Supplement);
        // A capitalised name matches exactly; an all-lowercase one in any case.
        assert_eq!(found("a rose for Rose. The Old Mill, the old mill", &c), ["Rose=rose.name", "The Old Mill=mill.name", "the old mill=mill.name"]);
    }

    #[test]
    fn shared_names_group() {
        let c = candidates(&ctx(&[("noun", "a", &[("name", "Tom Reed"), ("shortName", "Tom")]), ("monster", "b", &[("name", "Tom")])]), Palette::Supplement);
        assert_eq!(found("Tom and Tom Reed", &c), ["Tom=a.shortName|b.name", "Tom Reed=a.name"]);
    }

    #[test]
    fn unicode() {
        let c = candidates(&ctx(&[("noun", "zoe", &[("name", "Zoë Ålund")]), ("noun", "e", &[("name", "élan vital")])]), Palette::Supplement);
        assert_eq!(found("Naïve Zoë Ålund; Élan Vital.", &c), ["Zoë Ålund=zoe.name", "Élan Vital=e.name"]);
        // Offsets are bytes into the text.
        let m = &find("— Zoë Ålund", &c)[0];
        assert_eq!(m.start, "— ".len());
    }

    #[test]
    fn no_partial_or_overlap() {
        let c = alecia();
        assert!(found("Aleci Somethin", &c).is_empty());
        assert!(found("", &c).is_empty());
        // A match ends where the next begins.
        assert_eq!(found("Alec Alecia", &c), ["Alec=alec.shortName", "Alecia=alecia.shortName"]);
    }
}
