//! Checking and previewing attribute values: LaTeX lengths, colours, image ids.

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// lengthType: `-?([0-9]+(\.[0-9]*)?|\.[0-9]+)(pt|mm|cm|in|bp|pc|em|ex|%)`
pub fn is_length(s: &str) -> bool {
    parse_length(s).is_some()
}

/// (number, unit) for a lengthType value.
pub fn parse_length(s: &str) -> Option<(f64, &str)> {
    let unit = ["pt", "mm", "cm", "in", "bp", "pc", "em", "ex", "%"].into_iter().find(|u| s.ends_with(u))?;
    let num = &s[..s.len() - unit.len()];
    let digits = num.strip_prefix('-').unwrap_or(num);
    let (int, frac) = digits.split_once('.').map_or((digits, None), |(a, b)| (a, Some(b)));
    let ok = int.chars().all(|c| c.is_ascii_digit())
        && frac.is_none_or(|f| f.chars().all(|c| c.is_ascii_digit()))
        && (!int.is_empty() || frac.is_some_and(|f| !f.is_empty()));
    if !ok {
        return None;
    }
    Some((num.parse().ok()?, unit))
}

/// A length as a fraction of `page_in` inches (the page width or height), for previews.
/// em and ex are taken at a 10pt font.
pub fn length_fraction(s: &str, page_in: f64) -> Option<f64> {
    let (n, unit) = parse_length(s)?;
    let inches = match unit {
        "%" => return Some(n / 100.0),
        "in" => n,
        "cm" => n / 2.54,
        "mm" => n / 25.4,
        "pt" => n / 72.27,
        "bp" => n / 72.0,
        "pc" => n * 12.0 / 72.27,
        "em" => n * 10.0 / 72.27,
        "ex" => n * 4.3 / 72.27,
        _ => return None,
    };
    Some(inches / page_in)
}

/// Colour names a dndbook document knows, with an approximate RGB for the preview swatch.
/// The xcolor base names are exact; the dndbook palette values are close approximations.
pub const COLORS: &[(&str, (u8, u8, u8))] = &[
    ("white", (255, 255, 255)),
    ("black", (0, 0, 0)),
    ("titlered", (88, 24, 13)),
    ("titlegold", (201, 173, 106)),
    ("rulered", (154, 33, 18)),
    ("bgtan", (238, 229, 206)),
    ("pagegold", (201, 173, 106)),
    ("red", (255, 0, 0)),
    ("green", (0, 255, 0)),
    ("blue", (0, 0, 255)),
    ("cyan", (0, 255, 255)),
    ("magenta", (255, 0, 255)),
    ("yellow", (255, 255, 0)),
    ("gray", (128, 128, 128)),
    ("darkgray", (64, 64, 64)),
    ("lightgray", (191, 191, 191)),
    ("brown", (191, 128, 64)),
    ("lime", (191, 255, 0)),
    ("olive", (128, 128, 0)),
    ("orange", (255, 128, 0)),
    ("pink", (255, 191, 191)),
    ("purple", (191, 0, 64)),
    ("teal", (0, 128, 128)),
    ("violet", (128, 0, 128)),
];

fn named(name: &str) -> Option<(f64, f64, f64)> {
    COLORS.iter().find(|(n, _)| *n == name).map(|(_, (r, g, b))| (*r as f64, *g as f64, *b as f64))
}

/// An approximate CSS colour for a cover colour value: `#RRGGBB`, a known name, or an
/// xcolor mix like `red!60!black` or `black!35` (35% black, the rest white).
/// None when it can't be previewed (an unknown name, a malformed value).
pub fn css_color(value: &str) -> Option<String> {
    let v = value.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{hex}"));
    }
    let parts: Vec<&str> = v.split('!').collect();
    let mut cur = named(parts.first()?)?;
    let mut i = 1;
    while i < parts.len() {
        let pct: f64 = parts[i].parse().ok()?;
        if !(0.0..=100.0).contains(&pct) {
            return None;
        }
        let other = match parts.get(i + 1) {
            Some(n) => named(n)?,
            None => (255.0, 255.0, 255.0),
        };
        let p = pct / 100.0;
        cur = (cur.0 * p + other.0 * (1.0 - p), cur.1 * p + other.1 * (1.0 - p), cur.2 * p + other.2 * (1.0 - p));
        i += 2;
    }
    Some(format!("rgb({:.0}, {:.0}, {:.0})", cur.0, cur.1, cur.2))
}

/// An `<image>` the cover can name.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImageChoice {
    pub id: String,
    /// The file, relative to the supplement.
    pub src: Option<String>,
    pub name: Option<String>,
    /// "reference" or "section"
    pub place: String,
}

/// Every `<image id>` with a file: `<reference>` images and section images.
pub fn image_choices(source: &str) -> Vec<ImageChoice> {
    let mut reader = Reader::from_str(source);
    let mut out: Vec<ImageChoice> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(ev @ (Event::Start(_) | Event::Empty(_))) => {
                let (Event::Start(e) | Event::Empty(e)) = &ev else { unreachable!() };
                if e.name().as_ref() == b"image" {
                    let get = |k: &[u8]| {
                        e.attributes()
                            .flatten()
                            .find(|a| a.key.as_ref() == k)
                            .and_then(|a| a.unescape_value().ok())
                            .map(|v| v.into_owned())
                    };
                    if let (Some(id), Some(src)) = (get(b"id"), get(b"src")) {
                        let place = if stack.iter().any(|t| t == "reference") { "reference" } else { "section" };
                        if !out.iter().any(|c| c.id == id) {
                            out.push(ImageChoice { id, src: Some(src), name: get(b"name"), place: place.into() });
                        }
                    }
                }
                if matches!(ev, Event::Start(_)) {
                    stack.push(String::from_utf8_lossy(e.name().as_ref()).into_owned());
                }
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths() {
        for ok in ["0.5in", "-1in", ".5cm", "3.pt", "80%", "12bp"] {
            assert!(is_length(ok), "{ok}");
        }
        for bad in ["", "in", "1", "1 in", "1.2.3in", "-in", "1px", "."] {
            assert!(!is_length(bad), "{bad}");
        }
        assert!((length_fraction("5.5in", 11.0).unwrap() - 0.5).abs() < 1e-9);
        assert_eq!(length_fraction("50%", 11.0), Some(0.5));
    }

    #[test]
    fn colors() {
        assert_eq!(css_color("#1a2B3c").as_deref(), Some("#1a2B3c"));
        assert_eq!(css_color("white").as_deref(), Some("rgb(255, 255, 255)"));
        assert_eq!(css_color("black!40").as_deref(), Some("rgb(153, 153, 153)"));
        assert_eq!(css_color("red!50!blue").as_deref(), Some("rgb(128, 0, 128)"));
        assert_eq!(css_color("mystery"), None);
        assert_eq!(css_color("#12"), None);
    }

    #[test]
    fn images() {
        let src = r#"<s><chapter><section><image id="a" src="a.png"/><image ref="x"/></section></chapter>
            <reference><image id="b" src="b.png" name="Bee"/><image id="a" src="dup.png"/></reference></s>"#;
        let c = image_choices(src);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].id.as_str(), c[0].place.as_str()), ("a", "section"));
        assert_eq!((c[1].name.as_deref(), c[1].place.as_str()), (Some("Bee"), "reference"));
    }
}
