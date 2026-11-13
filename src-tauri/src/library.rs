//! The name index: every spell, monster stat block and magic item defined in the shared
//! SRD library (`<schemas>/../shared/internal`) and the open project, and every
//! `<reference>` entry's name, displayName and shortName, each tied to what it names
//! (see [`shared::names`]).
//!
//! It's built from the files in the background when a project opens, and kept current by
//! a file watcher on both folders (so edits made outside the app count too). The ref
//! generator lists its things; opening a file warns about shortNames that clash with
//! another thing's name, and an apply that adds or changes one is refused.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, RwLock};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use shared::names::{self, Def, NameUse};
use shared::prose::LibEntry;
use shared::ProjectInfo;

use crate::project::path_str;

/// One file's definitions and names.
struct FileNames {
    defs: Vec<Def>,
    uses: Vec<NameUse>,
}

#[derive(Default)]
struct Index {
    /// The SRD library folder and the project root, canonical.
    srd: Option<PathBuf>,
    root: Option<PathBuf>,
    files: HashMap<PathBuf, FileNames>,
    ready: bool,
}

fn index() -> &'static RwLock<Index> {
    static I: OnceLock<RwLock<Index>> = OnceLock::new();
    I.get_or_init(|| RwLock::new(Index::default()))
}

fn watcher() -> &'static Mutex<Option<notify::RecommendedWatcher>> {
    static W: OnceLock<Mutex<Option<notify::RecommendedWatcher>>> = OnceLock::new();
    W.get_or_init(|| Mutex::new(None))
}

fn canon(p: &Path) -> PathBuf {
    dunce::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// A file's key in target strings: canonical, with `/`.
fn key(p: &Path) -> String {
    path_str(&canon(p)).replace('\\', "/")
}

fn xml_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if depth < 10 && !name.starts_with('.') && name != "node_modules" && name != "target" {
                xml_files(&p, out, depth + 1);
            }
        } else if wanted(&p) {
            out.push(p);
        }
    }
}

/// An XML file the index reads (not the editor's own temporary files).
fn wanted(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    !name.starts_with('.') && name.to_ascii_lowercase().ends_with(".xml")
}

/// The names in one file's text.
fn scan_text(file: &Path, source: &str) -> FileNames {
    if !(source.contains("<spell") || source.contains("<magicItem") || source.contains("<monsters") || source.contains("<reference")) {
        return FileNames { defs: vec![], uses: vec![] };
    }
    let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let resolve = move |rel: &str| key(&dir.join(rel));
    let (defs, uses) = names::scan(source, &key(file), &resolve);
    FileNames { defs, uses }
}

fn scan_file(file: &Path) -> Option<FileNames> {
    let src = std::fs::read_to_string(file).ok()?;
    let f = scan_text(file, &src);
    (!f.defs.is_empty() || !f.uses.is_empty()).then_some(f)
}

fn srd_dir(project: &ProjectInfo) -> Option<PathBuf> {
    Path::new(&project.schemas_dir).parent().map(|p| canon(&p.join("shared").join("internal")))
}

/// Reads every file: the SRD library's spells, monsters and items, and the project.
fn build(project: &ProjectInfo) -> Index {
    let srd = srd_dir(project);
    let root = canon(Path::new(&project.root));
    let mut files = Vec::new();
    if let Some(d) = &srd {
        for sub in ["spells", "monsters", "magic_items"] {
            xml_files(&d.join(sub), &mut files, 0);
        }
    }
    xml_files(&root, &mut files, 0);
    let mut seen = HashSet::new();
    let mut map = HashMap::new();
    for f in files {
        let c = canon(&f);
        if seen.insert(c.clone()) {
            if let Some(n) = scan_file(&c) {
                map.insert(c, n);
            }
        }
    }
    Index { srd, root: Some(root), files: map, ready: true }
}

/// Re-reads one file (or drops it when it's gone). Only files the full scan would read
/// count: under the SRD library or the project, and not in a hidden folder (the editor's
/// and the tests' temporary copies).
pub fn update_file(file: &Path) {
    let c = canon(file);
    let hidden = c.components().any(|p| p.as_os_str().to_string_lossy().starts_with('.'));
    let mut i = index().write().unwrap();
    let inside = i.srd.as_ref().is_some_and(|s| c.starts_with(s)) || i.root.as_ref().is_some_and(|r| c.starts_with(r));
    if hidden || !inside || !wanted(&c) {
        return;
    }
    drop(i);
    let n = if c.is_file() { scan_file(&c) } else { None };
    let mut i = index().write().unwrap();
    match n {
        Some(n) => {
            i.files.insert(c, n);
        }
        None => {
            i.files.remove(&c);
        }
    }
}

/// Builds the index for a project in the background and watches its folders.
pub fn start(project: &ProjectInfo) {
    index().write().unwrap().ready = false;
    let p = project.clone();
    std::thread::spawn(move || {
        let built = build(&p);
        let dirs: Vec<PathBuf> = built.srd.iter().cloned().chain(built.root.clone()).collect();
        *index().write().unwrap() = built;
        watch(&dirs);
    });
}

/// Watches the folders; changed XML files are re-read a moment after the last change.
fn watch(dirs: &[PathBuf]) {
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            for p in ev.paths {
                if wanted(&p) {
                    let _ = tx.send(p);
                }
            }
        }
    });
    let Ok(mut w) = w else { return };
    for d in dirs {
        let _ = w.watch(d, RecursiveMode::Recursive);
    }
    // A new project's watcher replaces the old one (whose channel then closes).
    *watcher().lock().unwrap() = Some(w);
    std::thread::spawn(move || {
        while let Ok(first) = rx.recv() {
            let mut batch = HashSet::from([first]);
            while let Ok(p) = rx.recv_timeout(Duration::from_millis(300)) {
                batch.insert(p);
            }
            for p in batch {
                update_file(&p);
            }
        }
    });
}

/// Builds the index now if the background build hasn't finished.
fn ensure(project: &ProjectInfo) {
    let ready = index().read().unwrap().ready;
    if !ready {
        let built = build(project);
        let mut i = index().write().unwrap();
        if !i.ready {
            *i = built;
        }
    }
}

/// The things refs can be made to, with the other names entries give them.
pub fn library(project: &ProjectInfo) -> Vec<LibEntry> {
    ensure(project);
    let i = index().read().unwrap();
    let mut aliases: HashMap<&str, Vec<(String, String)>> = HashMap::new();
    for f in i.files.values() {
        for u in f.uses.iter().filter(|u| !u.entry.is_empty() && u.form != "name") {
            let v = aliases.entry(u.target.as_str()).or_default();
            if !v.iter().any(|(form, t)| *form == u.form && names::norm(t) == names::norm(&u.text)) {
                v.push((u.form.clone(), u.text.clone()));
            }
        }
    }
    let mut out = Vec::new();
    for (path, f) in &i.files {
        let srd = i.srd.as_ref().is_some_and(|s| path.starts_with(s));
        for d in &f.defs {
            out.push(LibEntry {
                kind: d.kind.clone(),
                id: d.id.clone(),
                name: d.name.clone(),
                file: path_str(path),
                src: d.src.clone(),
                srd,
                target: d.target.clone(),
                aliases: aliases.get(d.target.as_str()).cloned().unwrap_or_default(),
            });
        }
    }
    out.retain(|e| !e.id.is_empty() && !e.name.is_empty());
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// The shortNames in `source` (the text of `file`) that clash with another thing's name,
/// as (line, message). With `old`, only shortNames new or changed since that text count.
pub fn clashes(project: &ProjectInfo, file: &Path, source: &str, old: Option<&str>) -> Vec<(usize, String)> {
    ensure(project);
    let me = canon(file);
    let here = scan_text(&me, source);
    let shorts: Vec<&NameUse> = match old {
        Some(o) => {
            let before = scan_text(&me, o);
            names::changed_short_names(&before.uses, &here.uses).into_iter().collect()
        }
        None => here.uses.iter().filter(|u| u.form == "shortName").collect(),
    };
    if shorts.is_empty() {
        return vec![];
    }
    let i = index().read().unwrap();
    let others: Vec<&NameUse> = i.files.iter().filter(|(p, _)| **p != me).flat_map(|(_, f)| f.uses.iter()).chain(here.uses.iter()).collect();
    let here_key = key(&me);
    shorts
        .into_iter()
        .filter_map(|s| {
            let with = names::clashes(s, others.iter().copied().filter(|u| !std::ptr::eq(*u, s)));
            (!with.is_empty()).then(|| (s.line, names::message(s, &with, &here_key)))
        })
        .collect()
}

#[cfg(test)]
mod real {
    use super::*;

    fn project() -> ProjectInfo {
        ProjectInfo {
            root: "E:/Data/enigma-narrative/adventures".into(),
            name: "adventures".into(),
            schemas_dir: "E:/Data/enigma-narrative/schemas".into(),
            compiler_dir: "E:/Data/enigma-narrative/compiler".into(),
        }
    }

    #[test]
    #[ignore]
    fn reads_the_srd_library() {
        let lib = library(&project());
        let count = |k: &str| lib.iter().filter(|e| e.kind == k && e.srd).count();
        println!("{} spells, {} monsters, {} items, {} project", count("spell"), count("monster"), count("item"), lib.iter().filter(|e| !e.srd).count());
        assert!(count("spell") > 300 && count("monster") > 300 && count("item") > 250);
        assert!(lib.iter().any(|e| e.name == "Burning Hands" && e.id == "burning-hands"));
        // The apprentice wizard is known as Thistlewood by the spellbook's entry.
        let wiz = lib.iter().find(|e| e.id == "apprentice-wizard" && !e.srd).unwrap();
        assert!(wiz.aliases.iter().any(|(f, t)| f == "shortName" && t == "Thistlewood"), "{:?}", wiz.aliases);
    }

    /// Every shortName clash in the whole data repository, listed.
    #[test]
    #[ignore]
    fn lists_clashes_in_the_repo() {
        let p = ProjectInfo { root: "E:/Data/enigma-narrative".into(), ..project() };
        *index().write().unwrap() = build(&p);
        let files: Vec<PathBuf> = index().read().unwrap().files.iter().filter(|(_, f)| f.uses.iter().any(|u| u.form == "shortName")).map(|(p, _)| p.clone()).collect();
        let mut n = 0;
        for f in &files {
            let src = std::fs::read_to_string(f).unwrap();
            for (line, m) in clashes(&p, f, &src, None) {
                n += 1;
                println!("{}:{line}: {m}", f.display());
            }
        }
        println!("{n} clash(es) in {} files with short names", files.len());
    }

    #[test]
    #[ignore]
    fn flags_a_clashing_short_name() {
        let p = project();
        let file = Path::new("E:/Data/enigma-narrative/adventures/compiler-test/thistlewoods-spellbook.xml");
        let src = std::fs::read_to_string(file).unwrap();
        // As written: no clashes.
        assert!(clashes(&p, file, &src, None).is_empty(), "{:?}", clashes(&p, file, &src, None));
        // A red dragon called "Flametongue" reads as the SRD item Flame Tongue.
        let bad = src.replace("<reference>", "<reference>\n    <monster id=\"red\" name=\"Red Dragon\" cr=\"17\" shortName=\"Flametongue\"/>");
        let c = clashes(&p, file, &bad, Some(&src));
        println!("{c:?}");
        assert_eq!(c.len(), 1);
        assert!(c[0].1.contains("Flame Tongue"));
    }
}
