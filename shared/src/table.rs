//! The `<table>` editor: its id and title, its headers (`<header><th>`), and its rows.
//!
//! Headers are columns: adding, deleting or moving one adds, deletes or moves that
//! column's `<td>` in every row of the body and footer. A table without a `<header>`
//! gets one without its rows changing, and deleting the last header removes only the
//! `<header>` (emptying every row would leave `<tr>`s the schema refuses). Rows are shown
//! as their XML, read-only, for now.

use serde::{Deserialize, Serialize};

use crate::block::p_text;
use crate::edit::{self, Anchor, Splice};
use crate::xml::parse;
use crate::StructNode;

/// Which table: its id, and which table with that id it is in document order (ids can
/// repeat in sidebars, which the schema's table key doesn't cover).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableAt {
    pub id: String,
    pub nth: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableView {
    pub id: String,
    /// The `<title>` caption: the table's name.
    pub title: String,
    pub has_header: bool,
    pub headers: Vec<String>,
    /// Each body row's contents (its `<td>`s), dedented, and its cell count.
    pub body: Vec<(String, usize)>,
    pub has_footer: bool,
    pub footer: Vec<(String, usize)>,
    /// Attributes besides @id, kept as written.
    pub other_attrs: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op")]
pub enum TableEdit {
    Info { at: TableAt, id: String, title: String },
    SetHeader { at: TableAt, col: usize, text: String },
    /// A new header at position `col` (0 = first; the header count = last).
    AddHeader { at: TableAt, col: usize, text: String },
    DeleteHeader { at: TableAt, col: usize },
    /// Swap header `col` (and its column) with the next one.
    SwapHeaders { at: TableAt, col: usize },
}

impl TableEdit {
    pub fn at(&self) -> &TableAt {
        match self {
            TableEdit::Info { at, .. }
            | TableEdit::SetHeader { at, .. }
            | TableEdit::AddHeader { at, .. }
            | TableEdit::DeleteHeader { at, .. }
            | TableEdit::SwapHeaders { at, .. } => at,
        }
    }
}

fn attr(source: &str, n: &StructNode, name: &str) -> Option<String> {
    edit::start_tag(source, n.start)?.value(name)
}

/// Every `<table>` node, in document order.
fn tables(nodes: &[StructNode]) -> impl Iterator<Item = usize> + '_ {
    (0..nodes.len()).filter(|i| nodes[*i].tag == "table")
}

pub fn find(source: &str, nodes: &[StructNode], at: &TableAt) -> Option<usize> {
    tables(nodes).filter(|i| attr(source, &nodes[*i], "id").as_deref() == Some(at.id.as_str())).nth(at.nth)
}

/// The locator for the table at node `idx`.
pub fn table_at(source: &str, nodes: &[StructNode], idx: usize) -> Option<TableAt> {
    if nodes.get(idx)?.tag != "table" {
        return None;
    }
    let id = attr(source, &nodes[idx], "id").unwrap_or_default();
    let nth = tables(nodes).take_while(|i| *i != idx).filter(|i| attr(source, &nodes[*i], "id").as_deref() == Some(id.as_str())).count();
    Some(TableAt { id, nth })
}

fn kid<'a>(nodes: &'a [StructNode], n: &StructNode, tag: &str) -> Option<&'a StructNode> {
    n.children.iter().map(|c| &nodes[*c]).find(|c| c.tag == tag)
}

fn kids<'a>(nodes: &'a [StructNode], n: &'a StructNode, tag: &'a str) -> impl Iterator<Item = &'a StructNode> + 'a {
    n.children.iter().map(move |c| &nodes[*c]).filter(move |c| c.tag == tag)
}

/// Every `<tr>` of the body and footer.
fn rows<'a>(nodes: &'a [StructNode], t: &StructNode) -> Vec<&'a StructNode> {
    let mut out: Vec<&StructNode> = Vec::new();
    for part in ["tableBody", "footer"] {
        if let Some(p) = kid(nodes, t, part) {
            out.extend(kids(nodes, p, "tr"));
        }
    }
    out
}

pub fn read(source: &str, nodes: &[StructNode], ti: usize) -> TableView {
    let t = &nodes[ti];
    let rows_of = |part: &str| -> Vec<(String, usize)> {
        kid(nodes, t, part)
            .map(|p| kids(nodes, p, "tr").map(|r| (p_text(source, r), kids(nodes, r, "td").count())).collect())
            .unwrap_or_default()
    };
    let header = kid(nodes, t, "header");
    TableView {
        id: attr(source, t, "id").unwrap_or_default(),
        title: kid(nodes, t, "title").map(|n| edit::text_of(source, n)).unwrap_or_default(),
        has_header: header.is_some(),
        headers: header.map(|h| kids(nodes, h, "th").map(|th| edit::text_of(source, th)).collect()).unwrap_or_default(),
        body: rows_of("tableBody"),
        has_footer: kid(nodes, t, "footer").is_some(),
        footer: rows_of("footer"),
        other_attrs: edit::start_tag(source, t.start)
            .map(|s| s.attrs.into_iter().filter(|a| a.name != "id").map(|a| (a.name, a.value)).collect())
            .unwrap_or_default(),
    }
}

/// How many `<tableLink>`s name this id.
pub fn links(source: &str, nodes: &[StructNode], id: &str) -> usize {
    nodes.iter().filter(|n| n.tag == "tableLink" && attr(source, n, "id").as_deref() == Some(id)).count()
}

fn th_xml(text: &str) -> String {
    format!("<th>{}</th>", edit::escape_text(text))
}

/// Inserts `xml` as the `col`th child (of those tagged `tag`) of `parent`.
fn insert_at(source: &str, nodes: &[StructNode], parent: &StructNode, tag: &str, col: usize, xml: &str) -> Option<Splice> {
    let cells: Vec<&StructNode> = kids(nodes, parent, tag).collect();
    let anchor = match (cells.get(col), cells.last()) {
        (Some(c), _) => Anchor::Before(c),
        (None, Some(l)) => Anchor::After(l),
        (None, None) => Anchor::Into(parent),
    };
    edit::insert_child(source, anchor, xml)
}

/// Swaps two elements' text, leaving what's around them as it is.
fn swap(source: &str, a: &StructNode, b: &StructNode) -> Vec<Splice> {
    vec![
        Splice { start: a.start, end: a.end, text: source[b.start..b.end].to_string() },
        Splice { start: b.start, end: b.end, text: source[a.start..a.end].to_string() },
    ]
}

pub fn plan(source: &str, e: &TableEdit) -> Result<String, String> {
    let nodes = parse(source).map_err(|e| e.message)?.nodes;
    let ti = find(source, &nodes, e.at()).ok_or_else(|| format!("can't find table \"{}\" any more", e.at().id))?;
    let t = &nodes[ti];
    let header = kid(&nodes, t, "header");
    let ths: Vec<&StructNode> = header.map(|h| kids(&nodes, h, "th").collect()).unwrap_or_default();
    let all_rows = rows(&nodes, t);
    let th = |col: usize| ths.get(col).copied().ok_or_else(|| "that header isn't there any more; reopen the file".to_string());

    let splices: Vec<Splice> = match e {
        TableEdit::Info { at, id, title } => {
            if id.trim().is_empty() {
                return Err("a table needs an id".into());
            }
            let mut s = Vec::new();
            s.extend(edit::set_attr(source, t, "id", Some(id)));
            let cur = kid(&nodes, t, "title");
            match (cur, title.trim().is_empty()) {
                (Some(n), false) => s.extend(edit::set_text(source, n, title)),
                (Some(n), true) => s.push(edit::remove_element(source, n)),
                (None, false) => {
                    let xml = format!("<title>{}</title>", edit::escape_text(title));
                    let anchor = match t.children.first() {
                        Some(f) => Anchor::Before(&nodes[*f]),
                        None => Anchor::Into(t),
                    };
                    s.push(edit::insert_child(source, anchor, &xml).ok_or("can't place the title")?);
                }
                (None, true) => {}
            }
            let text = edit::apply(source, &s)?;
            if *id != at.id {
                // Rename the <tableLink>s that pointed at the old id.
                let nodes = parse(&text).map_err(|e| e.message)?.nodes;
                let links: Vec<Splice> = nodes
                    .iter()
                    .filter(|n| n.tag == "tableLink" && attr(&text, n, "id").as_deref() == Some(at.id.as_str()))
                    .filter_map(|n| edit::set_attr(&text, n, "id", Some(id)))
                    .collect();
                return edit::apply(&text, &links);
            }
            return Ok(text);
        }
        TableEdit::SetHeader { col, text, .. } => edit::set_text(source, th(*col)?, text).into_iter().collect(),
        TableEdit::AddHeader { col, text, .. } => match header {
            None => {
                // The first header: a new <header> after the title (or first), rows unchanged.
                let xml = format!("<header>\n{}{}\n</header>", edit::indent_unit(source), th_xml(text));
                let anchor = match kid(&nodes, t, "title") {
                    Some(title) => Anchor::After(title),
                    None => match t.children.first() {
                        Some(f) => Anchor::Before(&nodes[*f]),
                        None => Anchor::Into(t),
                    },
                };
                vec![edit::insert_child(source, anchor, &xml).ok_or("can't place the header")?]
            }
            Some(h) => {
                let col = (*col).min(ths.len());
                let mut s = vec![insert_at(source, &nodes, h, "th", col, &th_xml(text)).ok_or("can't place the header")?];
                for r in &all_rows {
                    s.extend(insert_at(source, &nodes, r, "td", col, "<td></td>"));
                }
                s
            }
        },
        TableEdit::DeleteHeader { col, .. } => {
            let cell = th(*col)?;
            if ths.len() == 1 {
                vec![edit::remove_element(source, header.unwrap())]
            } else {
                let mut s = vec![edit::remove_element(source, cell)];
                for r in &all_rows {
                    if let Some(td) = kids(&nodes, r, "td").nth(*col) {
                        s.push(edit::remove_element(source, td));
                    }
                }
                s
            }
        }
        TableEdit::SwapHeaders { col, .. } => {
            let (a, b) = (th(*col)?, th(col + 1)?);
            let mut s = swap(source, a, b);
            for r in &all_rows {
                let tds: Vec<&StructNode> = kids(&nodes, r, "td").collect();
                if let (Some(x), Some(y)) = (tds.get(*col), tds.get(col + 1)) {
                    s.extend(swap(source, x, y));
                }
            }
            s
        }
    };
    edit::apply(source, &splices)
}

/// The number of cells each column-aware change would touch, for confirms.
pub fn cells_in_column(nodes: &[StructNode], ti: usize, col: usize) -> usize {
    rows(nodes, &nodes[ti]).iter().filter(|r| kids(nodes, r, "td").nth(col).is_some()).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"<supplement>
    <chapter name="c">
        <chapterTitle>C</chapterTitle>
        <section name="s"><tableLink id="t"/></section>
        <table id="t" span="both">
            <title>Ring</title>
            <header>
                <th>Result</th>
                <th>Outcome</th>
            </header>
            <tableBody>
                <tr>
                    <td>15+</td>
                    <td>Unnoticed <b>quietly</b>.</td>
                </tr>
                <tr><td>10</td><td>Caught.</td></tr>
            </tableBody>
            <footer>
                <tr><td>Note</td><td>DM only.</td></tr>
            </footer>
        </table>
        <table id="bare">
            <tableBody><tr><td>a</td><td>b</td></tr></tableBody>
        </table>
    </chapter>
    <chapterOrder><ch name="c"/></chapterOrder>
</supplement>
"#;

    fn at(id: &str) -> TableAt {
        TableAt { id: id.into(), nth: 0 }
    }

    fn view(src: &str, id: &str) -> TableView {
        let nodes = parse(src).unwrap().nodes;
        read(src, &nodes, find(src, &nodes, &at(id)).unwrap())
    }

    #[test]
    fn reads_a_table() {
        let v = view(SRC, "t");
        assert_eq!(v.title, "Ring");
        assert_eq!(v.headers, ["Result", "Outcome"]);
        assert_eq!(v.body[0], ("<td>15+</td>\n<td>Unnoticed <b>quietly</b>.</td>".to_string(), 2));
        assert_eq!(v.body[1], ("<td>10</td><td>Caught.</td>".to_string(), 2));
        assert_eq!(v.footer.len(), 1);
        assert_eq!(v.other_attrs, vec![("span".to_string(), "both".to_string())]);
        assert!(!view(SRC, "bare").has_header);
    }

    #[test]
    fn info_renames_links() {
        let out = plan(SRC, &TableEdit::Info { at: at("t"), id: "ring".into(), title: "Lifting the Ring".into() }).unwrap();
        assert!(out.contains(r#"<table id="ring" span="both">"#));
        assert!(out.contains("<title>Lifting the Ring</title>"));
        assert!(out.contains(r#"<tableLink id="ring"/>"#));
        let out = plan(SRC, &TableEdit::Info { at: at("bare"), id: "bare".into(), title: "Now titled".into() }).unwrap();
        assert!(out.contains("<table id=\"bare\">\n            <title>Now titled</title>\n            <tableBody>"), "{out}");
    }

    #[test]
    fn add_header_adds_a_column() {
        let out = plan(SRC, &TableEdit::AddHeader { at: at("t"), col: 1, text: "DC".into() }).unwrap();
        let v = view(&out, "t");
        assert_eq!(v.headers, ["Result", "DC", "Outcome"]);
        assert!(v.body.iter().chain(&v.footer).all(|(_, n)| *n == 3));
        assert!(out.contains("<td>15+</td>\n                    <td></td>\n                    <td>Unnoticed"), "{out}");
        assert!(out.contains("<tr><td>10</td><td></td><td>Caught.</td></tr>"), "{out}");
        // At the end.
        let out = plan(SRC, &TableEdit::AddHeader { at: at("t"), col: 2, text: "Notes".into() }).unwrap();
        assert_eq!(view(&out, "t").headers, ["Result", "Outcome", "Notes"]);
        assert!(out.contains("<tr><td>10</td><td>Caught.</td><td></td></tr>"));
    }

    #[test]
    fn first_header_leaves_rows() {
        let out = plan(SRC, &TableEdit::AddHeader { at: at("bare"), col: 0, text: "A".into() }).unwrap();
        let v = view(&out, "bare");
        assert_eq!(v.headers, ["A"]);
        assert_eq!(v.body[0].1, 2);
        assert!(out.contains("<table id=\"bare\">\n            <header>\n                <th>A</th>\n            </header>\n            <tableBody>"), "{out}");
    }

    #[test]
    fn delete_and_swap_columns() {
        let out = plan(SRC, &TableEdit::DeleteHeader { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert_eq!(v.headers, ["Outcome"]);
        assert!(v.body.iter().chain(&v.footer).all(|(_, n)| *n == 1));
        assert!(!out.contains("15+") && out.contains("Unnoticed"));
        // The last header goes alone.
        let out = plan(&out, &TableEdit::DeleteHeader { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert!(!v.has_header && v.body[0].1 == 1);

        let out = plan(SRC, &TableEdit::SwapHeaders { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert_eq!(v.headers, ["Outcome", "Result"]);
        assert_eq!(v.body[1].0, "<td>Caught.</td><td>10</td>");
        assert_eq!(v.footer[0].0, "<tr><td>DM only.</td><td>Note</td></tr>".trim_start_matches("<tr>").trim_end_matches("</tr>"));
        assert!(plan(SRC, &TableEdit::SwapHeaders { at: at("t"), col: 1 }).is_err(), "no header after the last");
    }

    #[test]
    fn set_header_escapes() {
        let out = plan(SRC, &TableEdit::SetHeader { at: at("t"), col: 1, text: "Outcome & Notes".into() }).unwrap();
        assert!(out.contains("<th>Outcome &amp; Notes</th>"));
    }
}
