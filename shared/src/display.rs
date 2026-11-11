//! Turning a node's source slice into display text: re-indenting it, and splitting it
//! into tokens for syntax highlighting.

/// Spaces a tab counts as.
const TAB: &str = "    ";

/// The source of `start..end`, re-indented so the element's own start tag sits at column 0
/// and everything inside keeps its indentation relative to it. Tabs become four spaces,
/// trailing whitespace is dropped, and comments and text are kept exactly.
pub fn reindent(source: &str, start: usize, end: usize) -> String {
    let start = start.min(source.len());
    let end = end.clamp(start, source.len());
    // The indentation of the start tag: whatever is between the line start and `<`,
    // when it is all whitespace.
    let line_start = source[..start].rfind('\n').map_or(0, |i| i + 1);
    let lead = &source[line_start..start];
    let base = if lead.chars().all(|c| c == ' ' || c == '\t') { width(lead) } else { 0 };

    let slice = &source[start..end];
    let mut out = String::with_capacity(slice.len());
    for (i, line) in slice.split('\n').enumerate() {
        let line = line.trim_end_matches('\r').replace('\t', TAB);
        let line = if i == 0 { line } else { strip_indent(&line, base) };
        if i > 0 {
            out.push('\n');
        }
        out.push_str(line.trim_end());
    }
    out
}

fn width(s: &str) -> usize {
    s.chars().map(|c| if c == '\t' { TAB.len() } else { 1 }).sum()
}

/// `line` with up to `n` leading spaces removed.
fn strip_indent(line: &str, n: usize) -> String {
    let spaces = line.chars().take(n).take_while(|c| *c == ' ').count();
    line[spaces..].to_string()
}

/// Kinds of highlighted text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Text,
    Comment,
    /// `<`, `</`, `>`, `/>`, `=`
    Punct,
    TagName,
    AttrName,
    AttrValue,
    /// `<?xml ...?>`, `<!DOCTYPE ...>`, CDATA
    Special,
}

impl TokenKind {
    pub fn class(self) -> &'static str {
        match self {
            TokenKind::Text => "t-text",
            TokenKind::Comment => "t-comment",
            TokenKind::Punct => "t-punct",
            TokenKind::TagName => "t-tag",
            TokenKind::AttrName => "t-attr",
            TokenKind::AttrValue => "t-value",
            TokenKind::Special => "t-special",
        }
    }
}

/// Splits XML text into highlight tokens. It never fails: text it can't make sense of
/// (an unclosed tag at the end of a slice, say) comes out as plain text.
pub fn tokenize(xml: &str) -> Vec<(TokenKind, &str)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push((TokenKind::Text, rest));
            break;
        };
        if lt > 0 {
            out.push((TokenKind::Text, &rest[..lt]));
            rest = &rest[lt..];
        }
        let special_end = if rest.starts_with("<!--") {
            Some(("-->", TokenKind::Comment))
        } else if rest.starts_with("<![CDATA[") {
            Some(("]]>", TokenKind::Special))
        } else if rest.starts_with("<?") {
            Some(("?>", TokenKind::Special))
        } else if rest.starts_with("<!") {
            Some((">", TokenKind::Special))
        } else {
            None
        };
        if let Some((close, kind)) = special_end {
            let n = rest.find(close).map_or(rest.len(), |i| i + close.len());
            out.push((kind, &rest[..n]));
            rest = &rest[n..];
            continue;
        }
        match tag(rest) {
            Some((tokens, n)) => {
                out.extend(tokens);
                rest = &rest[n..];
            }
            None => {
                out.push((TokenKind::Text, rest));
                break;
            }
        }
    }
    out
}

/// Tokens for one start, end or empty tag at the start of `s`, and its length in bytes.
fn tag(s: &str) -> Option<(Vec<(TokenKind, &str)>, usize)> {
    let b = s.as_bytes();
    let mut toks = Vec::new();
    let mut i = if s.starts_with("</") { 2 } else { 1 };
    toks.push((TokenKind::Punct, &s[..i]));
    let name_end = i + s[i..].find(|c: char| c.is_whitespace() || c == '>' || c == '/').unwrap_or(s.len() - i);
    if name_end == i {
        return None;
    }
    toks.push((TokenKind::TagName, &s[i..name_end]));
    i = name_end;
    loop {
        // Whitespace between attributes.
        let ws = s[i..].len() - s[i..].trim_start().len();
        if ws > 0 {
            toks.push((TokenKind::Text, &s[i..i + ws]));
            i += ws;
        }
        if i >= b.len() {
            return None;
        }
        if s[i..].starts_with("/>") {
            toks.push((TokenKind::Punct, &s[i..i + 2]));
            return Some((toks, i + 2));
        }
        if b[i] == b'>' {
            toks.push((TokenKind::Punct, &s[i..i + 1]));
            return Some((toks, i + 1));
        }
        // An attribute: name = "value"
        let name_end = i + s[i..].find(|c: char| c.is_whitespace() || c == '=' || c == '>' || c == '/')?;
        if name_end == i {
            return None;
        }
        toks.push((TokenKind::AttrName, &s[i..name_end]));
        i = name_end;
        let ws = s[i..].len() - s[i..].trim_start().len();
        if ws > 0 {
            toks.push((TokenKind::Text, &s[i..i + ws]));
            i += ws;
        }
        if i < b.len() && b[i] == b'=' {
            toks.push((TokenKind::Punct, &s[i..i + 1]));
            i += 1;
            let ws = s[i..].len() - s[i..].trim_start().len();
            if ws > 0 {
                toks.push((TokenKind::Text, &s[i..i + ws]));
                i += ws;
            }
            let quote = *b.get(i)?;
            if quote != b'"' && quote != b'\'' {
                return None;
            }
            let close = i + 1 + s[i + 1..].find(quote as char)?;
            toks.push((TokenKind::AttrValue, &s[i..=close]));
            i = close + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reindent_moves_element_to_column_zero() {
        let src = "<a>\n    <b x=\"1\">\n        <c/>\n    </b>\n</a>";
        let start = src.find("<b").unwrap();
        let end = src.find("</b>").unwrap() + 4;
        assert_eq!(reindent(src, start, end), "<b x=\"1\">\n    <c/>\n</b>");
    }

    #[test]
    fn reindent_handles_tabs_and_crlf() {
        let src = "<a>\r\n\t<b>\r\n\t\t<c/>\r\n\t</b>\r\n</a>";
        let start = src.find("<b").unwrap();
        let end = src.find("</b>").unwrap() + 4;
        assert_eq!(reindent(src, start, end), "<b>\n    <c/>\n</b>");
    }

    #[test]
    fn tokenize_round_trips() {
        let src = "<?xml version=\"1.0\"?>\n<!-- hi -->\n<a b=\"c\" d='e'>text <i>x</i></a><e/>";
        let joined: String = tokenize(src).iter().map(|(_, t)| *t).collect();
        assert_eq!(joined, src);
        let kinds: Vec<_> = tokenize("<a b=\"c\"/>").iter().map(|(k, _)| *k).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Punct,
                TokenKind::TagName,
                TokenKind::Text,
                TokenKind::AttrName,
                TokenKind::Punct,
                TokenKind::AttrValue,
                TokenKind::Punct
            ]
        );
    }

    #[test]
    fn tokenize_unclosed_is_text() {
        let joined: String = tokenize("a < b").iter().map(|(_, t)| *t).collect();
        assert_eq!(joined, "a < b");
    }
}
