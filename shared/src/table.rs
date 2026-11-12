//! The `<table>` editor: its id and title, its headers (`<header><th>`), and its rows.
//!
//! Headers are columns: adding, deleting or moving one adds, deletes or moves that
//! column's `<td>` in every row of the body and footer. A table without a `<header>`
//! gets one without its rows changing, and deleting the last header removes only the
//! `<header>` (emptying every row would leave `<tr>`s the schema refuses). Rows are shown
//! as their XML, read-only, for now.

use serde::{Deserialize, Serialize};

use crate::block::{p_text, set_inner};
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

/// The body or the footer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Part {
    Body,
    Footer,
}

impl Part {
    fn tag(self) -> &'static str {
        match self {
            Part::Body => "tableBody",
            Part::Footer => "footer",
        }
    }
}

/// One `<td>`: its contents as XML (dedented), and its spans as written.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CellView {
    pub text: String,
    pub colspan: String,
    pub rowspan: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RowView {
    pub cells: Vec<CellView>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TableView {
    pub id: String,
    /// The `<title>` caption: the table's name.
    pub title: String,
    pub has_header: bool,
    pub headers: Vec<String>,
    pub body: Vec<RowView>,
    pub has_footer: bool,
    pub footer: Vec<RowView>,
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
    /// Replace a cell's contents (raw XML).
    SetCell { at: TableAt, part: Part, row: usize, col: usize, text: String },
    /// Add an empty cell at the end of a row (one short of the headers).
    AddCell { at: TableAt, part: Part, row: usize },
    /// A new row after `after`, or first when None: one empty cell per header.
    AddRow { at: TableAt, part: Part, after: Option<usize> },
    DeleteRow { at: TableAt, part: Part, row: usize },
    /// Swap row `row` with the next one.
    SwapRows { at: TableAt, part: Part, row: usize },
}

impl TableEdit {
    pub fn at(&self) -> &TableAt {
        match self {
            TableEdit::Info { at, .. }
            | TableEdit::SetHeader { at, .. }
            | TableEdit::AddHeader { at, .. }
            | TableEdit::DeleteHeader { at, .. }
            | TableEdit::SwapHeaders { at, .. }
            | TableEdit::SetCell { at, .. }
            | TableEdit::AddCell { at, .. }
            | TableEdit::AddRow { at, .. }
            | TableEdit::DeleteRow { at, .. }
            | TableEdit::SwapRows { at, .. } => at,
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
    let rows_of = |part: &str| -> Vec<RowView> {
        kid(nodes, t, part)
            .map(|p| {
                kids(nodes, p, "tr")
                    .map(|r| RowView {
                        cells: kids(nodes, r, "td")
                            .map(|td| CellView {
                                text: p_text(source, td),
                                colspan: attr(source, td, "colspan").unwrap_or_default(),
                                rowspan: attr(source, td, "rowspan").unwrap_or_default(),
                            })
                            .collect(),
                    })
                    .collect()
            })
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
        TableEdit::SetCell { part, row, col, text, .. } => {
            let td = row_of(&nodes, t, *part, *row)?
                .children
                .iter()
                .map(|c| &nodes[*c])
                .filter(|c| c.tag == "td")
                .nth(*col)
                .ok_or("that cell isn't there any more; reopen the file")?;
            vec![set_inner(source, td, text).ok_or("bad <td>")?]
        }
        TableEdit::AddCell { part, row, .. } => {
            let r = row_of(&nodes, t, *part, *row)?;
            let n = kids(&nodes, r, "td").count();
            vec![insert_at(source, &nodes, r, "td", n, "<td></td>").ok_or("can't place the cell")?]
        }
        TableEdit::AddRow { part, after, .. } => {
            let p = kid(&nodes, t, part.tag()).ok_or("the table has no such part")?;
            let trs: Vec<&StructNode> = kids(&nodes, p, "tr").collect();
            // As many cells as headers; without headers, as many as the widest row.
            let width = if ths.is_empty() {
                all_rows.iter().map(|r| kids(&nodes, r, "td").count()).max().unwrap_or(1).max(1)
            } else {
                ths.len()
            };
            // Lay it out like the first row: cells inline, or one per line.
            let lined = trs.first().is_some_and(|r| source[r.start..r.end].contains('\n'));
            let xml = if lined {
                let unit = edit::indent_unit(source);
                format!("<tr>\n{}\n</tr>", vec![format!("{unit}<td></td>"); width].join("\n"))
            } else {
                format!("<tr>{}</tr>", "<td></td>".repeat(width))
            };
            let anchor = match after {
                Some(i) => Anchor::After(trs.get(*i).ok_or("that row isn't there any more")?),
                None => match trs.first() {
                    Some(f) => Anchor::Before(f),
                    None => Anchor::Into(p),
                },
            };
            vec![edit::insert_child(source, anchor, &xml).ok_or("can't place the row")?]
        }
        TableEdit::DeleteRow { part, row, .. } => {
            let p = kid(&nodes, t, part.tag()).ok_or("the table has no such part")?;
            let r = row_of(&nodes, t, *part, *row)?;
            if kids(&nodes, p, "tr").count() == 1 {
                match part {
                    // The footer's last row: the footer goes too.
                    Part::Footer => vec![edit::remove_element(source, p)],
                    Part::Body => return Err("a table needs at least one body row".into()),
                }
            } else {
                vec![edit::remove_element(source, r)]
            }
        }
        TableEdit::SwapRows { part, row, .. } => {
            let (a, b) = (row_of(&nodes, t, *part, *row)?, row_of(&nodes, t, *part, row + 1)?);
            swap(source, a, b)
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

fn row_of<'a>(nodes: &'a [StructNode], t: &StructNode, part: Part, row: usize) -> Result<&'a StructNode, String> {
    kid(nodes, t, part.tag())
        .and_then(|p| kids(nodes, p, "tr").nth(row))
        .ok_or_else(|| "that row isn't there any more; reopen the file".to_string())
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
        let texts = |r: &RowView| r.cells.iter().map(|c| c.text.clone()).collect::<Vec<_>>();
        assert_eq!(texts(&v.body[0]), ["15+", "Unnoticed <b>quietly</b>."]);
        assert_eq!(texts(&v.body[1]), ["10", "Caught."]);
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
        assert!(v.body.iter().chain(&v.footer).all(|r| r.cells.len() == 3));
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
        assert_eq!(v.body[0].cells.len(), 2);
        assert!(out.contains("<table id=\"bare\">\n            <header>\n                <th>A</th>\n            </header>\n            <tableBody>"), "{out}");
    }

    #[test]
    fn delete_and_swap_columns() {
        let out = plan(SRC, &TableEdit::DeleteHeader { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert_eq!(v.headers, ["Outcome"]);
        assert!(v.body.iter().chain(&v.footer).all(|r| r.cells.len() == 1));
        assert!(!out.contains("15+") && out.contains("Unnoticed"));
        // The last header goes alone.
        let out = plan(&out, &TableEdit::DeleteHeader { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert!(!v.has_header && v.body[0].cells.len() == 1);

        let out = plan(SRC, &TableEdit::SwapHeaders { at: at("t"), col: 0 }).unwrap();
        let v = view(&out, "t");
        assert_eq!(v.headers, ["Outcome", "Result"]);
        assert_eq!(v.body[1].cells[0].text, "Caught.");
        assert_eq!(v.body[1].cells[1].text, "10");
        assert_eq!(v.footer[0].cells[0].text, "DM only.");
        assert!(plan(SRC, &TableEdit::SwapHeaders { at: at("t"), col: 1 }).is_err(), "no header after the last");
    }

    #[test]
    fn cells_and_rows() {
        let out = plan(SRC, &TableEdit::SetCell { at: at("t"), part: Part::Body, row: 1, col: 1, text: "Caught <i>red-handed</i>.".into() }).unwrap();
        assert!(out.contains("<tr><td>10</td><td>Caught <i>red-handed</i>.</td></tr>"));
        // A multi-line row gets a new row laid out the same way, one cell per header.
        let out = plan(SRC, &TableEdit::AddRow { at: at("t"), part: Part::Body, after: None }).unwrap();
        assert!(out.contains("<tableBody>\n                <tr>\n                    <td></td>\n                    <td></td>\n                </tr>\n                <tr>\n                    <td>15+</td>"), "{out}");
        let out = plan(SRC, &TableEdit::AddRow { at: at("t"), part: Part::Footer, after: Some(0) }).unwrap();
        assert_eq!(view(&out, "t").footer.len(), 2);
        assert!(out.contains("<tr><td>Note</td><td>DM only.</td></tr>\n                <tr><td></td><td></td></tr>"), "{out}");
        // Swap and delete.
        let out = plan(SRC, &TableEdit::SwapRows { at: at("t"), part: Part::Body, row: 0 }).unwrap();
        assert_eq!(view(&out, "t").body[0].cells[0].text, "10");
        let out = plan(SRC, &TableEdit::DeleteRow { at: at("t"), part: Part::Body, row: 0 }).unwrap();
        assert_eq!(view(&out, "t").body.len(), 1);
        assert!(plan(&out, &TableEdit::DeleteRow { at: at("t"), part: Part::Body, row: 0 }).is_err(), "the last body row stays");
        let out = plan(SRC, &TableEdit::DeleteRow { at: at("t"), part: Part::Footer, row: 0 }).unwrap();
        assert!(!view(&out, "t").has_footer && !out.contains("<footer>"));
        // A short row gets a cell.
        let src = SRC.replace("<tr><td>10</td><td>Caught.</td></tr>", "<tr><td>10</td></tr>");
        let out = plan(&src, &TableEdit::AddCell { at: at("t"), part: Part::Body, row: 1 }).unwrap();
        assert!(out.contains("<tr><td>10</td><td></td></tr>"));
    }

    #[test]
    fn set_header_escapes() {
        let out = plan(SRC, &TableEdit::SetHeader { at: at("t"), col: 1, text: "Outcome & Notes".into() }).unwrap();
        assert!(out.contains("<th>Outcome &amp; Notes</th>"));
    }
}
