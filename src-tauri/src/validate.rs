//! Opening a file: the root and schema-name check (here, in Rust), then XSD validation
//! with lxml through the compiler's uv project.
//!
//! The Python environment lives in the app's cache folder (UV_PROJECT_ENVIRONMENT), so
//! the compiler's own .venv is never touched.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use shared::{
    AppError, ApplyResult, OpenedDoc, Problem, ProjectInfo, Validation, LEGACY_SUPPLEMENT_SCHEMA,
    SUPPLEMENT_SCHEMA,
};

use crate::project::path_str;
use shared::xml as xmldoc;

pub const SCRIPT: &str = include_str!("../resources/validate.py");

/// Where the validator script and its Python environment live.
pub struct Validator {
    pub script: PathBuf,
    pub venv: PathBuf,
}

impl Validator {
    pub fn install(cache_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        let script = cache_dir.join("validate.py");
        if std::fs::read_to_string(&script).ok().as_deref() != Some(SCRIPT) {
            std::fs::write(&script, SCRIPT)?;
        }
        Ok(Validator { script, venv: cache_dir.join("venv") })
    }
}

#[derive(Deserialize)]
struct PyError {
    line: i64,
    message: String,
}

#[derive(Deserialize)]
struct PyResult {
    ok: bool,
    errors: Vec<PyError>,
}

enum Outcome {
    Valid,
    Invalid(Vec<Problem>),
    NotRun(String),
}

/// The uv executable. A double-clicked app may not have the terminal's PATH, so after
/// PATH this tries uv's standard install folders.
fn uv_exe() -> PathBuf {
    let exe = if cfg!(windows) { "uv.exe" } else { "uv" };
    let on_path = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).map(|d| d.join(exe)).any(|f| f.is_file()))
        .unwrap_or(false);
    if on_path {
        return PathBuf::from("uv");
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("UV_INSTALL_DIR") {
        dirs.push(PathBuf::from(d));
    }
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(local).join("Programs").join("uv"));
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|f| f.is_file()).unwrap_or_else(|| PathBuf::from("uv"))
}

fn run_lxml(v: &Validator, compiler: &str, file: &Path, xsd: &Path) -> Outcome {
    let mut cmd = Command::new(uv_exe());
    cmd.args(["run", "--quiet", "--project", compiler, "python"])
        .arg(&v.script)
        .arg(file)
        .arg(xsd)
        .env("UV_PROJECT_ENVIRONMENT", &v.venv);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = match cmd.output() {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Outcome::NotRun("uv isn't installed or isn't on PATH".into())
        }
        Err(e) => return Outcome::NotRun(format!("couldn't run uv: {e}")),
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json = stdout.lines().rev().find(|l| l.trim_start().starts_with('{'));
    match json.and_then(|j| serde_json::from_str::<PyResult>(j).ok()) {
        Some(r) if r.ok => Outcome::Valid,
        Some(r) => Outcome::Invalid(
            r.errors
                .into_iter()
                .map(|e| Problem { line: (e.line > 0).then_some(e.line as usize), message: e.message })
                .collect(),
        ),
        None => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = stderr.lines().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            Outcome::NotRun(format!("the validator failed: {}", msg.trim()))
        }
    }
}

fn file_name(loc: &str) -> &str {
    loc.rsplit(['/', '\\']).next().unwrap_or(loc)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (dunce::canonicalize(a), dunce::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// A file read from disk: its canonical path, name and text (BOM removed).
struct ReadFile {
    file: PathBuf,
    name: String,
    source: String,
}

fn read_file(path: &str) -> Result<ReadFile, AppError> {
    let file = dunce::canonicalize(path).map_err(|e| AppError::new(format!("Can't open {path}: {e}")))?;
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let bytes = std::fs::read(&file).map_err(|e| AppError::new(format!("Can't read {name}: {e}")))?;
    let source = String::from_utf8(bytes).map_err(|_| AppError::new(format!("{name} isn't UTF-8 text")))?;
    let source = source.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(source);
    Ok(ReadFile { file, name, source })
}

/// The checks that don't need Python: well-formed, <supplement> root, the supplement
/// schema named. Returns the tree, the XSD to validate with, and warnings.
fn check_text(
    file: &Path,
    name: &str,
    source: &str,
    project: &ProjectInfo,
) -> Result<(xmldoc::Parsed, PathBuf, Vec<String>), AppError> {
    let parsed = xmldoc::parse(source).map_err(|e| AppError::new(format!("Can't open {name}. {}", e.message)))?;
    let mut warnings = Vec::new();

    // 1. The root must be <supplement>.
    if parsed.root_tag == "adventure" {
        return Err(AppError::with_details(
            format!("{name} is an old-style <adventure> file, not a <supplement>."),
            vec!["Convert it with shared/internal/tools/migrate_schemas.py, then open it again.".into()],
        ));
    }
    if parsed.root_tag != "supplement" {
        let schema = parsed.schema_location.as_deref().map(file_name).unwrap_or("no schema");
        return Err(AppError::with_details(
            format!("{name} isn't a supplement file."),
            vec![
                format!("Its root element is <{}> and it names {schema}.", parsed.root_tag),
                format!("Only files using {SUPPLEMENT_SCHEMA} can be opened."),
            ],
        ));
    }

    // 2. It must name the supplement schema.
    let Some(loc) = parsed.schema_location.clone() else {
        return Err(AppError::with_details(
            format!("{name} doesn't name a schema."),
            vec![format!("Its <supplement> needs xsi:noNamespaceSchemaLocation pointing at {SUPPLEMENT_SCHEMA}.")],
        ));
    };
    let loc_name = file_name(&loc);
    if loc_name == LEGACY_SUPPLEMENT_SCHEMA {
        warnings.push(format!("It names the old schema {LEGACY_SUPPLEMENT_SCHEMA}; validated with {SUPPLEMENT_SCHEMA}."));
    } else if loc_name != SUPPLEMENT_SCHEMA {
        return Err(AppError::with_details(
            format!("{name} uses the wrong schema."),
            vec![format!("It names {loc}; a supplement must use {SUPPLEMENT_SCHEMA}.")],
        ));
    }

    // 3. Find the schema: the named path, else the project's schemas folder (as the compiler does).
    let dir = file.parent().unwrap_or(Path::new("."));
    let named = dir.join(&loc);
    let project_xsd = Path::new(&project.schemas_dir).join(SUPPLEMENT_SCHEMA);
    let xsd = if loc_name == SUPPLEMENT_SCHEMA && named.is_file() {
        if !same_file(&named, &project_xsd) {
            warnings.push(format!(
                "Its schema ({}) isn't the project's ({}).",
                path_str(&dunce::canonicalize(&named).unwrap_or(named.clone())),
                path_str(&project_xsd)
            ));
        }
        named
    } else if project_xsd.is_file() {
        if loc_name == SUPPLEMENT_SCHEMA {
            let fix = pathdiff::diff_paths(&project_xsd, dir).map(|p| path_str(&p).replace('\\', "/"));
            warnings.push(format!(
                "Its schema location \"{loc}\" doesn't exist; validated with the project's schema. Write xsi:noNamespaceSchemaLocation=\"{}\".",
                fix.unwrap_or_default()
            ));
        }
        project_xsd
    } else {
        return Err(AppError::new(format!("Can't find {SUPPLEMENT_SCHEMA} for {name}.")));
    };

    Ok((parsed, xsd, warnings))
}

pub fn open_file(path: &str, project: &ProjectInfo, v: &Validator) -> Result<OpenedDoc, AppError> {
    let ReadFile { file, name, source } = read_file(path)?;
    let (parsed, xsd, warnings) = check_text(&file, &name, &source, project)?;

    // 4. Validate against the XSD.
    let validation = match run_lxml(v, &project.compiler_dir, &file, &xsd) {
        Outcome::Valid => Validation::Valid { schema: path_str(&dunce::canonicalize(&xsd).unwrap_or(xsd)) },
        Outcome::Invalid(errors) => {
            let lines = errors
                .into_iter()
                .map(|e| match e.line {
                    Some(l) => format!("line {l}: {}", e.message),
                    None => e.message,
                })
                .collect();
            return Err(AppError::with_details(format!("{name} doesn't match the supplement schema."), lines));
        }
        Outcome::NotRun(reason) => Validation::NotRun { reason },
    };

    Ok(OpenedDoc { path: path_str(&file), name, source, nodes: parsed.nodes, validation, warnings })
}


/// Applies an edit: `edit` turns the file's text into the new text. The file must still
/// be what the editor opened (`expected`). The new text is validated as a temporary file
/// beside the original (so relative schema paths still work), and only replaces the
/// original when it validates.
pub fn apply_edit(
    path: &str,
    expected: &str,
    project: &ProjectInfo,
    v: &Validator,
    edit: impl FnOnce(&str) -> Result<String, String>,
) -> Result<ApplyResult, AppError> {
    let ReadFile { file, name, source } = read_file(path)?;
    if source != expected {
        return Err(AppError::with_details(
            format!("{name} has changed on disk since it was opened."),
            vec!["Close and reopen it to edit the current version. Nothing was written.".into()],
        ));
    }
    let new = edit(&source).map_err(|e| AppError::new(format!("Can't make that change: {e}")))?;
    if new == source {
        return Ok(ApplyResult::Saved { doc: open_parsed(file, name, source, project)? });
    }
    let xsd = match check_text(&file, &name, &new, project) {
        Ok((_, xsd, _)) => xsd,
        Err(e) => {
            // "Can't open x.xml. The file isn't well-formed XML (line 63): ..." reads
            // oddly for an edit; keep the reason, and lift the line out for the panel.
            let msg = e.message.strip_prefix(&format!("Can't open {name}. ")).unwrap_or(&e.message).to_string();
            let line = msg
                .split_once("(line ")
                .and_then(|(_, rest)| rest.split_once(')'))
                .and_then(|(n, _)| n.parse().ok());
            let mut problems = vec![Problem { line, message: msg.replace("The file isn't", "The edited file isn't") }];
            problems.extend(e.details.into_iter().map(|d| Problem { line: None, message: d }));
            return Ok(ApplyResult::Invalid { problems });
        }
    };
    let dir = file.parent().unwrap_or(Path::new("."));
    let tmp = dir.join(format!(".{name}.enigma-apply.xml"));
    std::fs::write(&tmp, &new).map_err(|e| AppError::new(format!("Can't write {}: {e}", tmp.display())))?;
    let result = match run_lxml(v, &project.compiler_dir, &tmp, &xsd) {
        Outcome::Valid => std::fs::rename(&tmp, &file)
            .map_err(|e| AppError::new(format!("Can't replace {name}: {e}")))
            .and_then(|_| open_parsed(file.clone(), name.clone(), new, project))
            .map(|doc| ApplyResult::Saved { doc }),
        Outcome::Invalid(problems) => Ok(ApplyResult::Invalid { problems }),
        Outcome::NotRun(reason) => Err(AppError::with_details(
            format!("Couldn't validate the change, so {name} wasn't written."),
            vec![reason],
        )),
    };
    let _ = std::fs::remove_file(&tmp);
    result
}

/// The doc for text that has just been validated.
fn open_parsed(file: PathBuf, name: String, source: String, project: &ProjectInfo) -> Result<OpenedDoc, AppError> {
    let (parsed, xsd, warnings) = check_text(&file, &name, &source, project)?;
    Ok(OpenedDoc {
        path: path_str(&file),
        name,
        source,
        nodes: parsed.nodes,
        validation: Validation::Valid { schema: path_str(&dunce::canonicalize(&xsd).unwrap_or(xsd)) },
        warnings,
    })
}

/// Against the real repository at E:/Data/enigma-narrative (run with --ignored).
#[cfg(test)]
mod real {
    use super::*;

    const REPO: &str = "E:/Data/enigma-narrative";

    fn setup() -> (ProjectInfo, Validator) {
        let project = ProjectInfo {
            root: format!("{REPO}/adventures"),
            name: "adventures".into(),
            schemas_dir: format!("{REPO}/schemas"),
            compiler_dir: format!("{REPO}/compiler"),
        };
        let cache = std::env::temp_dir().join("enigma-editor-test-cache");
        (project, Validator::install(&cache).unwrap())
    }

    #[test]
    #[ignore]
    fn opens_compiler_test_adventure() {
        let (p, v) = setup();
        let doc = open_file(&format!("{REPO}/adventures/compiler-test/adventure.xml"), &p, &v).unwrap();
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
        let top: Vec<_> = doc.nodes[0].children.iter().map(|c| doc.nodes[*c].tag.clone()).collect();
        println!("{} nodes; top level: {top:?}", doc.nodes.len());
        for c in &doc.nodes[0].children {
            println!("  {} [{}]", doc.nodes[*c].tag, doc.nodes[*c].label);
        }
    }

    #[test]
    #[ignore]
    fn rejects_monster_file() {
        let (p, v) = setup();
        let err = open_file(&format!("{REPO}/adventures/compiler-test/apprentice-wizard.xml"), &p, &v).unwrap_err();
        println!("{}\n  {}", err.message, err.details.join("\n  "));
        assert!(err.message.contains("isn't a supplement"));
    }

    #[test]
    #[ignore]
    fn new_skeleton_validates() {
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let req = shared::NewSupplement {
            parent: path_str(t.path()),
            folder: "skeleton-test".into(),
            kind: "bestiary".into(),
            title: "Beasts & Bones".into(),
        };
        let file = crate::fsops::create_supplement(&req, &path_str(t.path()), &p.schemas_dir).unwrap();
        let doc = open_file(&file, &p, &v).unwrap();
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    }

    #[test]
    #[ignore]
    fn apply_cover_round_trip() {
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml"))
            .unwrap()
            .replace("../../schemas/", "../../schemas/"); // same depth below adventures/
        std::fs::write(&file, &src).unwrap();
        // An unchanged model writes nothing.
        let m = shared::cover::read(&src).unwrap();
        assert_eq!(shared::cover::plan(&src, &m).unwrap(), src);
        // A valid edit is written.
        let mut m2 = m.clone();
        m2.header_title = "Hillsfort Over Fang".into();
        m2.footer_subchaser.clear();
        let r = apply_edit(&path_str(&file), &src, &p, &v, |s| shared::cover::plan(s, &m2)).unwrap();
        let ApplyResult::Saved { doc } = r else { panic!("expected Saved") };
        let on_disk = std::fs::read_to_string(&file).unwrap();
        assert_eq!(doc.source, on_disk);
        assert!(on_disk.contains("<headerTitle>Hillsfort Over Fang</headerTitle>"));
        assert!(!on_disk.contains("<footerSubchaser>"));
        // An invalid edit (a bad length) is refused and nothing changes.
        let mut m3 = m2.clone();
        m3.image.as_mut().unwrap().margin = "half an inch".into();
        let r = apply_edit(&path_str(&file), &on_disk, &p, &v, |s| shared::cover::plan(s, &m3)).unwrap();
        let ApplyResult::Invalid { problems } = r else { panic!("expected Invalid") };
        println!("{problems:?}");
        assert!(problems.iter().any(|p| p.message.contains("margin")));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), on_disk);
        assert!(!t.path().join(".adventure.xml.enigma-apply.xml").exists());
        // A stale `expected` is refused.
        assert!(apply_edit(&path_str(&file), &src, &p, &v, |s| Ok(s.to_string())).is_err());
    }

    #[test]
    #[ignore]
    fn apply_chapter_round_trip() {
        use shared::chapter::{self, ChapterEdit, ChildItem};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, edit: ChapterEdit| {
            apply_edit(&path_str(&file), expected, &p, &v, |s| chapter::plan(s, &edit)).unwrap()
        };
        let model = |s: &str, name: &str| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            chapter::read(s, &nodes, chapter::find_chapter(s, &nodes, name).unwrap())
        };

        // Rename (with its references), reorder, and add one of each child kind.
        let refs = chapter::name_refs(&src, "hillsfort").len();
        println!("hillsfort has {refs} references");
        let mut m = model(&src, "hillsfort");
        m.name = "hillsfort-town".into();
        m.children.swap(1, 2);
        for (tag, key) in [("section", "new-sec"), ("sidebar", "new-side"), ("table", "new-table"), ("pageEnd", "")] {
            m.children.push(ChildItem { tag: tag.into(), key: key.into(), new_title: "New".into(), ..Default::default() });
        }
        let r = run(&src, ChapterEdit::Update { name: "hillsfort".into(), model: m });
        let ApplyResult::Saved { doc } = r else { panic!("rename/reorder/add refused: {r:?}") };
        assert_eq!(chapter::name_refs(&doc.source, "hillsfort-town").len(), refs);

        // Add a chapter, then delete it.
        let r = run(&doc.source, ChapterEdit::Add { name: "app-maps".into(), title: "Maps".into(), kind: "appendix".into() });
        let ApplyResult::Saved { doc } = r else { panic!("add refused: {r:?}") };
        let r = run(&doc.source, ChapterEdit::Delete { name: "app-maps".into() });
        let ApplyResult::Saved { doc } = r else { panic!("delete refused: {r:?}") };

        // Deleting a chapter something still points at is refused, and nothing changes.
        let before = std::fs::read_to_string(&file).unwrap();
        assert_eq!(before, doc.source);
        let r = run(&before, ChapterEdit::Delete { name: "app-monsters".into() });
        let ApplyResult::Invalid { problems } = r else { panic!("expected refusal") };
        println!("{problems:#?}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    }

    #[test]
    #[ignore]
    fn apply_reference_round_trip() {
        use shared::reference::{self, RefEdit, RefEntry};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, edit: RefEdit| {
            apply_edit(&path_str(&file), expected, &p, &v, |s| reference::plan(s, &edit)).unwrap()
        };
        let entries = |s: &str| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            reference::read(s, &nodes).into_iter().map(|(_, e)| e).collect::<Vec<_>>()
        };
        let usage = reference::usage(&src);
        // The most-used entry of each kind gets renamed.
        let mut cur = src.clone();
        for kind in ["monster", "spell", "item", "image", "data"] {
            let e = entries(&cur)
                .into_iter()
                .filter(|e| e.tag == kind)
                .max_by_key(|e| usage.get(&(kind.to_string(), e.id())).copied().unwrap_or(0))
                .unwrap();
            let old = e.id();
            let uses = reference::usage(&cur).get(&(kind.to_string(), old.clone())).copied().unwrap_or(0);
            let mut n = e.clone();
            n.set("id", format!("{old}-renamed"));
            let r = run(&cur, RefEdit::Update { tag: kind.into(), id: old.clone(), entry: n });
            let ApplyResult::Saved { doc } = r else { panic!("rename of {kind} {old} refused: {r:?}") };
            assert_eq!(reference::usage(&doc.source).get(&(kind.to_string(), format!("{old}-renamed"))).copied().unwrap_or(0), uses);
            println!("renamed {kind} {old} ({uses} uses)");
            cur = doc.source;
        }
        // Move the first entry to the end.
        let first = entries(&cur)[0].clone();
        let r = run(&cur, RefEdit::Move { tag: first.tag.clone(), id: first.id(), before: None });
        let ApplyResult::Saved { doc } = r else { panic!("move refused: {r:?}") };
        assert_eq!(entries(&doc.source).last().unwrap().id(), first.id());
        cur = doc.source;
        // Add one of each kind.
        for (tag, attrs) in [
            ("monster", vec![("id", "new-mon"), ("name", "Wolf"), ("cr", "1/4"), ("src", "SRD")]),
            ("spell", vec![("id", "new-spell"), ("name", "Light"), ("src", "SRD")]),
            ("item", vec![("id", "new-item"), ("name", "Rope")]),
            ("noun", vec![("id", "new-noun"), ("name", "Old Mill"), ("kind", "place")]),
            ("image", vec![("id", "new-img"), ("src", "x.png"), ("height", "3in"), ("span", "both")]),
            ("data", vec![("id", "new-data"), ("value", "7")]),
        ] {
            let mut e = RefEntry::new(tag);
            for (k, v) in attrs {
                e.set(k, v.into());
            }
            let r = run(&cur, RefEdit::Add { entry: e });
            let ApplyResult::Saved { doc } = r else { panic!("add {tag} refused: {r:?}") };
            cur = doc.source;
        }
        // A used entry can't be deleted; an unused one can.
        let r = run(&cur, RefEdit::Delete { tag: "spell".into(), id: entries(&cur).iter().find(|e| e.tag == "spell" && e.id().ends_with("-renamed")).unwrap().id() });
        assert!(matches!(r, ApplyResult::Invalid { .. }), "deleting a used spell should be refused");
        let r = run(&cur, RefEdit::Delete { tag: "noun".into(), id: "new-noun".into() });
        assert!(matches!(r, ApplyResult::Saved { .. }));
    }

    #[test]
    #[ignore]
    fn apply_block_round_trip() {
        use shared::block::{self, BlockEdit, Container};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, edit: BlockEdit| {
            apply_edit(&path_str(&file), expected, &p, &v, |s| block::plan(s, &edit)).unwrap()
        };
        let saved = |r: ApplyResult, what: &str| match r {
            ApplyResult::Saved { doc } => doc.source,
            other => panic!("{what} refused: {other:?}"),
        };
        let read = |s: &str, at: &Container| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            block::read(s, &nodes, block::find(s, &nodes, at).unwrap())
        };
        let sec = Container::Section { chapter: "introduction".into(), name: "how-to-use".into(), nth: 0 };
        let (_, blocks) = read(&src, &sec);
        let first = blocks.iter().find(|b| b.tag == "p").unwrap().clone();
        assert!(first.text.contains("<chapterRef"), "inline markup is kept as XML: {}", first.text);

        // Edit a paragraph, keeping its inline tags.
        let cur = saved(run(&src, BlockEdit::SetP { at: sec.clone(), child: first.child, text: first.text.replace("This adventure", "This short adventure") }), "edit");
        assert!(cur.contains("This short adventure"));
        // A paragraph that isn't well-formed is refused and nothing changes.
        let r = run(&cur, BlockEdit::SetP { at: sec.clone(), child: first.child, text: "Broken <b>bold".into() });
        let ApplyResult::Invalid { problems } = r else { panic!("expected refusal") };
        println!("broken paragraph: {problems:?}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), cur);
        // Add, move, delete.
        let cur = saved(run(&cur, BlockEdit::AddP { at: sec.clone(), after: Some(first.child) }), "add");
        let (_, b) = read(&cur, &sec);
        let new = b.iter().find(|b| b.tag == "p" && b.text.is_empty()).unwrap().child;
        let cur = saved(run(&cur, BlockEdit::SetP { at: sec.clone(), child: new, text: "A new paragraph & more.".replace('&', "&amp;") }), "fill");
        let cur = saved(run(&cur, BlockEdit::Move { at: sec.clone(), child: new, before: Some(first.child) }), "move");
        let (_, b) = read(&cur, &sec);
        assert_eq!(b[0].text, "A new paragraph &amp; more.");
        let cur = saved(run(&cur, BlockEdit::DeleteP { at: sec.clone(), child: b[0].child }), "delete");
        // Header: a subtitle added to a titled section.
        let (h, _) = read(&cur, &sec);
        let cur = saved(run(&cur, BlockEdit::Header { at: sec.clone(), header: block::Header { subtitle: "For the DM".into(), ..h } }), "header");
        // Rename a sidebar that a sidebarLink points at.
        let nodes = xmldoc::parse(&cur).unwrap().nodes;
        let link = nodes.iter().find(|n| n.tag == "sidebarLink").expect("a sidebarLink");
        let id = shared::edit::start_tag(&cur, link.start).unwrap().value("id").unwrap();
        let parents = {
            let mut ps = vec![None; nodes.len()];
            for (i, n) in nodes.iter().enumerate() {
                for c in &n.children {
                    ps[*c] = Some(i);
                }
            }
            ps
        };
        let side = (0..nodes.len())
            .filter_map(|i| block::container_of(&cur, &nodes, &parents, i))
            .find(|c| matches!(c, Container::Sidebar { id: i, .. } if *i == id))
            .unwrap();
        let (h, _) = read(&cur, &side);
        let cur = saved(run(&cur, BlockEdit::Header { at: side, header: block::Header { key: format!("{id}-x"), ..h } }), "sidebar rename");
        assert_eq!(block::sidebar_links(&xmldoc::parse(&cur).unwrap().nodes, &cur, &format!("{id}-x")), 1);
    }

    #[test]
    #[ignore]
    fn apply_table_round_trip() {
        use shared::table::{self, TableAt, TableEdit};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, edit: TableEdit| match apply_edit(&path_str(&file), expected, &p, &v, |s| table::plan(s, &edit)).unwrap() {
            ApplyResult::Saved { doc } => doc.source,
            other => panic!("{edit:?} refused: {other:?}"),
        };
        let view = |s: &str, id: &str| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            table::read(s, &nodes, table::find(s, &nodes, &TableAt { id: id.into(), nth: 0 }).unwrap())
        };
        let at = |id: &str| TableAt { id: id.into(), nth: 0 };
        let nodes = xmldoc::parse(&src).unwrap().nodes;
        let links = table::links(&src, &nodes, "tbl-lifting-ring");
        println!("tbl-lifting-ring: {links} tableLink(s); {:?}", view(&src, "tbl-lifting-ring").headers);

        let cur = run(&src, TableEdit::Info { at: at("tbl-lifting-ring"), id: "tbl-ring".into(), title: "Lifting the Ring".into() });
        assert_eq!(table::links(&cur, &xmldoc::parse(&cur).unwrap().nodes, "tbl-ring"), links);
        let cur = run(&cur, TableEdit::AddHeader { at: at("tbl-ring"), col: 1, text: "DC".into() });
        let cur = run(&cur, TableEdit::SetHeader { at: at("tbl-ring"), col: 1, text: "DC & Notes".into() });
        let cur = run(&cur, TableEdit::SwapHeaders { at: at("tbl-ring"), col: 0 });
        let ring = view(&cur, "tbl-ring");
        assert_eq!(ring.headers, ["DC & Notes", "Result", "Outcome"]);
        assert!(ring.body.iter().all(|r| r.cells.len() == 3));
        let cur = run(&cur, TableEdit::DeleteHeader { at: at("tbl-ring"), col: 0 });
        assert_eq!(view(&cur, "tbl-ring").headers, ["Result", "Outcome"]);
        // Every table in the file reads, and the footer-bearing one keeps its footer rows aligned.
        for id in ["tbl-combatants", "tbl-gnoll-pouch", "tbl-parley", "tbl-xp", "tbl-treasure"] {
            let v = view(&cur, id);
            println!("{id}: {} headers, {} rows, footer {}", v.headers.len(), v.body.len(), v.footer.len());
        }
        let cur = run(&cur, TableEdit::AddHeader { at: at("tbl-xp"), col: 0, text: "#".into() });
        let xp = view(&cur, "tbl-xp");
        assert!(xp.body.iter().chain(&xp.footer).all(|r| r.cells.len() == xp.headers.len()), "{xp:?}");

        // Cells and rows.
        use shared::table::Part;
        let cur = run(&cur, TableEdit::SetCell { at: at("tbl-ring"), part: Part::Body, row: 0, col: 1, text: "Unnoticed, <i>for now</i>.".into() });
        assert_eq!(view(&cur, "tbl-ring").body[0].cells[1].text, "Unnoticed, <i>for now</i>.");
        let cur = run(&cur, TableEdit::AddRow { at: at("tbl-ring"), part: Part::Body, after: Some(0) });
        let cur = run(&cur, TableEdit::SetCell { at: at("tbl-ring"), part: Part::Body, row: 1, col: 0, text: "12".into() });
        let cur = run(&cur, TableEdit::SwapRows { at: at("tbl-ring"), part: Part::Body, row: 0 });
        let tv = view(&cur, "tbl-ring");
        assert_eq!(tv.body[0].cells[0].text, "12");
        assert_eq!(tv.body.len(), 4);
        let cur = run(&cur, TableEdit::DeleteRow { at: at("tbl-ring"), part: Part::Body, row: 0 });
        assert_eq!(view(&cur, "tbl-ring").body.len(), 3);
        // A cell that isn't well-formed is refused.
        let r = apply_edit(&path_str(&file), &cur, &p, &v, |s| table::plan(s, &TableEdit::SetCell { at: at("tbl-ring"), part: Part::Body, row: 0, col: 0, text: "<b>open".into() })).unwrap();
        assert!(matches!(r, ApplyResult::Invalid { .. }));
    }
}
