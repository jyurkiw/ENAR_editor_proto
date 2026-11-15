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
    /// What the entry is called, for menus: its display name, else its name ("Eamon",
    /// whose stat block is named "Bandit").
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
        let entry_name = ["displayName", "name"].iter().find_map(|f| e.values.get(*f).filter(|v| !v.trim().is_empty()).cloned()).unwrap_or_else(|| id.clone());
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

// ------------------------------------------------------------------ the whole file

/// One mention in a file's source, for the "pill every mention" list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Byte range in the source (as written, so `&amp;` and all).
    pub start: usize,
    pub end: usize,
    /// The text, as read ("Krugga's" for `Krugga&apos;s`).
    pub text: String,
    /// Indexes into the candidates.
    pub cands: Vec<usize>,
    /// Up to 40 characters of the surrounding text, read.
    pub before: String,
    pub after: String,
    /// Where it is: the nearest section, sidebar or chapter's label.
    pub place: String,
}

/// Text decoded from XML, with the source offset of each decoded byte (and one past).
fn decode(raw: &str, base: usize) -> (String, Vec<usize>) {
    let mut out = String::new();
    let mut map = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let rest = &raw[i..];
        if rest.starts_with('&') {
            if let Some(semi) = rest.find(';').filter(|s| *s <= 10) {
                let ent = &rest[1..semi];
                let ch = match ent {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32),
                    _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32),
                    _ => None,
                };
                if let Some(c) = ch {
                    for _ in 0..c.len_utf8() {
                        map.push(base + i);
                    }
                    out.push(c);
                    i += semi + 1;
                    continue;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        for _ in 0..c.len_utf8() {
            map.push(base + i);
        }
        out.push(c);
        i += c.len_utf8();
    }
    map.push(base + raw.len());
    (out, map)
}

/// The runs of pillable text in `source[a..b]` (a prose element's contents): text not
/// inside any tag, broken at every tag and comment. Not even inside `<b>` or `<i>`: they
/// hold plain text only, so a ref there isn't valid. (start, end) bytes.
fn runs(source: &str, a: usize, b: usize) -> Vec<(usize, usize)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut run_start = a;
    let mut i = a;
    let open = |stack: &Vec<String>| stack.is_empty();
    while i < b {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if run_start < i && open(&stack) {
            out.push((run_start, i));
        }
        let rest = &source[i..b];
        let end = if rest.starts_with("<!--") {
            rest.find("-->").map(|e| i + e + 3)
        } else if rest.starts_with("<![CDATA[") {
            rest.find("]]>").map(|e| i + e + 3)
        } else if rest.starts_with("<?") {
            rest.find("?>").map(|e| i + e + 2)
        } else {
            // A tag: find its '>' outside quotes.
            let mut q: Option<u8> = None;
            let mut j = i + 1;
            while j < b {
                match (q, bytes[j]) {
                    (None, b'"') | (None, b'\'') => q = Some(bytes[j]),
                    (Some(c), x) if c == x => q = None,
                    (None, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            let tag = &source[i..(j + 1).min(b)];
            let name: String = tag.trim_start_matches(['<', '/']).chars().take_while(|c| !c.is_whitespace() && *c != '/' && *c != '>').collect();
            if tag.starts_with("</") {
                if let Some(k) = stack.iter().rposition(|t| *t == name) {
                    stack.truncate(k);
                }
            } else if !tag.ends_with("/>") {
                stack.push(name);
            }
            Some(j + 1)
        };
        i = end.unwrap_or(b).min(b);
        run_start = i;
    }
    if run_start < b && open(&stack) {
        out.push((run_start, b));
    }
    out
}

/// Source XML as plain text: tags and comments dropped, entities read.
fn plain(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let close = if rest[i..].starts_with("<!--") { rest[i..].find("-->").map(|e| i + e + 3) } else { rest[i..].find('>').map(|e| i + e + 1) };
        rest = &rest[close.unwrap_or(rest.len())..];
    }
    out.push_str(rest);
    decode(&out, 0).0
}

fn clip(s: &str, n: usize, from_end: bool) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= n {
        return s.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let part: String = if from_end { chars[chars.len() - n..].iter().collect() } else { chars[..n].iter().collect() };
    let part = part.split_whitespace().collect::<Vec<_>>().join(" ");
    if from_end { format!("\u{2026}{part}") } else { format!("{part}\u{2026}") }
}

/// Every mention of a candidate in the file's prose, in file order: the text of every
/// prose element (see [`crate::statblock::is_prose`]), outside its inline tags.
pub fn occurrences(source: &str, cands: &[Candidate]) -> Vec<Occurrence> {
    let Ok(parsed) = crate::xml::parse_full(source) else { return Vec::new() };
    let nodes = &parsed.nodes;
    let mut parent = vec![None; nodes.len()];
    for (i, n) in nodes.iter().enumerate() {
        for c in &n.children {
            parent[*c] = Some(i);
        }
    }
    let place = |mut i: usize| -> String {
        while let Some(p) = parent[i] {
            if matches!(nodes[p].tag.as_str(), "section" | "sidebar" | "chapter") {
                return nodes[p].label.clone();
            }
            i = p;
        }
        String::new()
    };
    let mut out = Vec::new();
    let mut covered = 0;
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by_key(|i| nodes[*i].start);
    for i in order {
        let n = &nodes[i];
        let ptag = parent[i].map(|p| nodes[p].tag.as_str()).unwrap_or("");
        // Headings stay as written.
        let heading = n.tag == "subtitle" || n.tag.to_lowercase().ends_with("title");
        if n.start < covered || heading || !crate::statblock::is_prose(ptag, &n.tag) {
            continue;
        }
        let Some((a, b)) = crate::edit::inner_span(source, n) else { continue };
        covered = n.end;
        for (s, e) in runs(source, a, b) {
            let (text, map) = decode(&source[s..e], s);
            for m in find(&text, cands) {
                out.push(Occurrence {
                    start: map[m.start],
                    end: map[m.end],
                    text: text[m.start..m.end].to_string(),
                    cands: m.cands,
                    before: clip(&plain(&source[a..map[m.start]]), 40, true),
                    after: clip(&plain(&source[map[m.end]..b]), 40, false),
                    place: place(i),
                });
            }
        }
    }
    out
}

/// The mentions of the `chosen` names, found among all the file's names so the longest
/// still wins: with "Garrick Moss" among them, picking the short name "Garrick" doesn't
/// take the "Garrick" out of "Garrick Moss". Each comes with its index in `chosen`.
pub fn mentions_of(source: &str, all: &[Candidate], chosen: &[Candidate]) -> Vec<(Occurrence, usize)> {
    let mut every: Vec<Candidate> = all.to_vec();
    for c in chosen {
        if !every.iter().any(|e| same(e, c)) {
            every.push(c.clone());
        }
    }
    occurrences(source, &every)
        .into_iter()
        .filter_map(|o| {
            let k = o.cands.iter().find_map(|i| chosen.iter().position(|c| same(c, &every[*i])))?;
            Some((o, k))
        })
        .collect()
}

fn same(a: &Candidate, b: &Candidate) -> bool {
    a.kind == b.kind && a.id == b.id && a.form == b.form && a.text == b.text
}

/// One mention made a ref: the source bytes it replaces (checked when planned).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PillEdit {
    pub start: usize,
    pub end: usize,
    /// What `source[start..end]` must still be.
    pub raw: String,
    /// The ref that replaces it.
    pub xml: String,
}

/// Replaces each mention with its ref; refused if the file has changed under any of them.
pub fn plan(source: &str, edits: &[PillEdit]) -> Result<String, String> {
    if edits.is_empty() {
        return Err("nothing to replace".into());
    }
    let mut sorted: Vec<&PillEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| e.start);
    for w in sorted.windows(2) {
        if w[0].end > w[1].start {
            return Err("two replacements overlap".into());
        }
    }
    for e in &sorted {
        if source.get(e.start..e.end) != Some(e.raw.as_str()) {
            return Err("the file has changed since the list was made; open it again".into());
        }
    }
    let splices: Vec<crate::edit::Splice> = sorted.iter().map(|e| crate::edit::Splice { start: e.start, end: e.end, text: e.xml.clone() }).collect();
    crate::edit::apply(source, &splices)
}

#[cfg(test)]
mod file_tests {
    use super::*;

    fn cand(text: &str, xml: &str) -> Candidate {
        Candidate { kind: "noun".into(), id: "x".into(), form: "name".into(), text: text.into(), entry_name: text.into(), ref_xml: xml.into(), any_case: !text.chars().any(char::is_uppercase) }
    }

    const SRC: &str = r#"<supplement>
    <chapter name="c"><chapterTitle>Alec Hall</chapterTitle>
        <section name="s"><title>Alec's Home</title>
            <p>Alec met <b>Alec</b> and <nounRef ref="a" value="name"/>; <rule name="x">Alec rule</rule> Alec&apos;s dog.<!-- Alec --></p>
            <p>Smartalec, ALEC, Alec &amp; Co.</p>
        </section>
    </chapter>
    <reference><noun id="a" name="Alec"/></reference>
</supplement>"#;

    #[test]
    fn finds_prose_mentions_only() {
        let c = [cand("Alec", "<nounRef ref=\"a\" value=\"name\"/>")];
        let o = occurrences(SRC, &c);
        let got: Vec<String> = o.iter().map(|x| format!("{}|{}", x.text, &SRC[x.start..x.end])).collect();
        // Not in titles, attributes, <b>, the <rule>'s text, the comment, Smartalec or ALEC.
        assert_eq!(got, ["Alec|Alec", "Alec|Alec", "Alec|Alec"], "{o:#?}");
        // The second is Alec&apos;s: only "Alec" is replaced.
        assert!(SRC[o[1].end..].starts_with("&apos;s dog"));
        // Context reads through tags.
        assert_eq!(o[0].after, "met Alec and ; Alec rule Alec's dog.");
        assert!(!o[0].place.is_empty());
    }

    #[test]
    fn entities_decode() {
        let c = [cand("Alec & Co.", "<nounRef ref=\"co\" value=\"name\"/>")];
        let o = occurrences(SRC, &c);
        assert_eq!(o.len(), 1);
        assert_eq!(&SRC[o[0].start..o[0].end], "Alec &amp; Co.");
    }

    #[test]
    fn longest_still_wins_for_one_name() {
        let src = "<supplement><chapter name=\"c\"><section name=\"s\"><p>Garrick Moss waves. Garrick nods.</p></section></chapter></supplement>";
        let full = Candidate { form: "name".into(), ..cand("Garrick Moss", "<nounRef ref=\"g\" value=\"name\"/>") };
        let short = Candidate { form: "shortName".into(), ..cand("Garrick", "<nounRef ref=\"g\" value=\"shortName\"/>") };
        let m = mentions_of(src, &[full.clone(), short.clone()], std::slice::from_ref(&short));
        assert_eq!(m.len(), 1);
        assert!(src[m[0].0.end..].starts_with(" nods"));
        // With both chosen, each mention gets its own form.
        let m = mentions_of(src, &[], &[full, short]);
        assert_eq!(m.iter().map(|(o, k)| (o.text.as_str(), *k)).collect::<Vec<_>>(), [("Garrick Moss", 0), ("Garrick", 1)]);
    }

    #[test]
    fn plan_replaces_and_refuses_stale() {
        let c = [cand("Alec", "<nounRef ref=\"a\" value=\"name\"/>")];
        let o = occurrences(SRC, &c);
        let edits: Vec<PillEdit> = o.iter().map(|x| PillEdit { start: x.start, end: x.end, raw: SRC[x.start..x.end].into(), xml: c[0].ref_xml.clone() }).collect();
        let out = plan(SRC, &edits).unwrap();
        assert!(out.contains(r#"<p><nounRef ref="a" value="name"/> met <b>Alec</b> and"#), "{out}");
        assert!(out.contains(r#"<nounRef ref="a" value="name"/>&apos;s dog"#));
        assert!(out.contains("<title>Alec's Home</title>"));
        // Run again on the result: the old offsets no longer hold.
        assert!(plan(&out, &edits).is_err());
        assert!(plan(SRC, &[]).is_err());
    }
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
