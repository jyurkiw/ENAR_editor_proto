//! Opening a file: the root and schema-name check (here, in Rust), then XSD validation
//! with lxml through the compiler's uv project.
//!
//! The Python environment lives in the app's cache folder (UV_PROJECT_ENVIRONMENT), so
//! the compiler's own .venv is never touched.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use shared::{
    AppError, ApplyResult, DocKind, OpenedDoc, Problem, ProjectInfo, Validation, LEGACY_SUPPLEMENT_SCHEMA,
    MAGIC_ITEM_SCHEMA, MONSTER_SCHEMA, SUPPLEMENT_SCHEMA,
};

use crate::project::path_str;
use shared::xml as xmldoc;

pub const SCRIPT: &str = include_str!("../resources/validate.py");
pub const REFS_SCRIPT: &str = include_str!("../resources/refs.py");

/// Where the validator script and its Python environment live.
pub struct Validator {
    pub script: PathBuf,
    /// Resolves <reference> entries to what the compiler prints (refs.py).
    pub refs_script: PathBuf,
    pub venv: PathBuf,
}

impl Validator {
    pub fn install(cache_dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        let write = |name: &str, text: &str| -> std::io::Result<PathBuf> {
            let p = cache_dir.join(name);
            if std::fs::read_to_string(&p).ok().as_deref() != Some(text) {
                std::fs::write(&p, text)?;
            }
            Ok(p)
        };
        let script = write("validate.py", SCRIPT)?;
        let refs_script = write("refs.py", REFS_SCRIPT)?;
        Ok(Validator { script, refs_script, venv: cache_dir.join("venv") })
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

fn python(v: &Validator, compiler: &str, script: &Path) -> Command {
    let mut cmd = Command::new(uv_exe());
    cmd.args(["run", "--quiet", "--project", compiler, "python"]).arg(script).env("UV_PROJECT_ENVIRONMENT", &v.venv);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[derive(Deserialize)]
struct PyRefs {
    ok: bool,
    #[serde(default)]
    entries: std::collections::HashMap<String, shared::inline::IndexEntry>,
    #[serde(default)]
    problems: Vec<String>,
}

/// What the compiler prints for each <reference> entry's values, read with the
/// compiler's own loader (refs.py), so the entries' files load as they do in a build.
pub fn ref_index(path: &str, project: &ProjectInfo, v: &Validator) -> Result<shared::inline::RefIndex, AppError> {
    let file = dunce::canonicalize(path).map_err(|e| AppError::new(format!("Can't open {path}: {e}")))?;
    let out = python(v, &project.compiler_dir, &v.refs_script)
        .arg(&file)
        .arg(&project.compiler_dir)
        .output()
        .map_err(|e| AppError::new(format!("couldn't run uv: {e}")))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json = stdout.lines().rev().find(|l| l.trim_start().starts_with('{'));
    match json.and_then(|j| serde_json::from_str::<PyRefs>(j).ok()) {
        Some(r) if r.ok => Ok(shared::inline::RefIndex { entries: r.entries, problems: r.problems }),
        Some(r) => Err(AppError::with_details("Can't read the reference entries.", r.problems)),
        None => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let msg = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            Err(AppError::new(format!("Reading the reference entries failed: {}", msg.trim())))
        }
    }
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

/// The checks that don't need Python: well-formed, a `<supplement>` or `<monsters>`
/// root, and that kind's schema named. Returns the tree, the kind, the XSD to validate
/// with, and warnings.
fn check_text(
    file: &Path,
    name: &str,
    source: &str,
    project: &ProjectInfo,
) -> Result<(xmldoc::Parsed, DocKind, PathBuf, Vec<String>), AppError> {
    let parsed = xmldoc::parse(source).map_err(|e| AppError::new(format!("Can't open {name}. {}", e.message)))?;
    let mut warnings = Vec::new();

    // 1. The root must be <supplement>, <monsters> or <magicItem>.
    if parsed.root_tag == "adventure" {
        return Err(AppError::with_details(
            format!("{name} is an old-style <adventure> file, not a <supplement>."),
            vec!["Convert it with shared/internal/tools/migrate_schemas.py, then open it again.".into()],
        ));
    }
    let Some(kind) = DocKind::of_root(&parsed.root_tag) else {
        let schema = parsed.schema_location.as_deref().map(file_name).unwrap_or("no schema");
        return Err(AppError::with_details(
            format!("{name} isn't a supplement, monster or magic item file."),
            vec![
                format!("Its root element is <{}> and it names {schema}.", parsed.root_tag),
                format!("Only files using {SUPPLEMENT_SCHEMA}, {MONSTER_SCHEMA} or {MAGIC_ITEM_SCHEMA} can be opened."),
            ],
        ));
    };
    let schema_name = kind.schema();

    // 2. It must name that kind's schema.
    let Some(loc) = parsed.schema_location.clone() else {
        return Err(AppError::with_details(
            format!("{name} doesn't name a schema."),
            vec![format!("Its <{}> needs xsi:noNamespaceSchemaLocation pointing at {schema_name}.", kind.root())],
        ));
    };
    let loc_name = file_name(&loc);
    if kind == DocKind::Supplement && loc_name == LEGACY_SUPPLEMENT_SCHEMA {
        warnings.push(format!("It names the old schema {LEGACY_SUPPLEMENT_SCHEMA}; validated with {SUPPLEMENT_SCHEMA}."));
    } else if loc_name != schema_name {
        return Err(AppError::with_details(
            format!("{name} uses the wrong schema."),
            vec![format!("It names {loc}; a {} must use {schema_name}.", kind.noun())],
        ));
    }

    // 3. Find the schema: the named path, else the project's schemas folder (as the compiler does).
    let dir = file.parent().unwrap_or(Path::new("."));
    let named = dir.join(&loc);
    let project_xsd = Path::new(&project.schemas_dir).join(schema_name);
    let xsd = if loc_name == schema_name && named.is_file() {
        if !same_file(&named, &project_xsd) {
            warnings.push(format!(
                "Its schema ({}) isn't the project's ({}).",
                path_str(&dunce::canonicalize(&named).unwrap_or(named.clone())),
                path_str(&project_xsd)
            ));
        }
        named
    } else if project_xsd.is_file() {
        if loc_name == schema_name {
            let fix = pathdiff::diff_paths(&project_xsd, dir).map(|p| path_str(&p).replace('\\', "/"));
            warnings.push(format!(
                "Its schema location \"{loc}\" doesn't exist; validated with the project's schema. Write xsi:noNamespaceSchemaLocation=\"{}\".",
                fix.unwrap_or_default()
            ));
        }
        project_xsd
    } else {
        return Err(AppError::new(format!("Can't find {schema_name} for {name}.")));
    };

    Ok((parsed, kind, xsd, warnings))
}

pub fn open_file(path: &str, project: &ProjectInfo, v: &Validator) -> Result<OpenedDoc, AppError> {
    let ReadFile { file, name, source } = read_file(path)?;
    let (parsed, kind, xsd, mut warnings) = check_text(&file, &name, &source, project)?;
    warnings.extend(clash_warnings(&file, &source, project));

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
            return Err(AppError::with_details(format!("{name} doesn't match the {} schema.", kind.noun()), lines));
        }
        Outcome::NotRun(reason) => Validation::NotRun { reason },
    };

    Ok(OpenedDoc { path: path_str(&file), kind, name, source, nodes: parsed.nodes, validation, warnings })
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
    // A shortName this edit adds or changes mustn't read like another thing's name.
    let clashes = crate::library::clashes(project, &file, &new, Some(&source));
    if !clashes.is_empty() {
        let problems = clashes.into_iter().map(|(line, message)| Problem { line: Some(line), message }).collect();
        return Ok(ApplyResult::Invalid { problems });
    }
    let xsd = match check_text(&file, &name, &new, project) {
        Ok((_, _, xsd, _)) => xsd,
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
            .inspect(|_| crate::library::update_file(&file))
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

/// Warnings for shortNames in the file that read like another thing's name.
fn clash_warnings(file: &Path, source: &str, project: &ProjectInfo) -> Vec<String> {
    crate::library::clashes(project, file, source, None).into_iter().map(|(line, m)| format!("Line {line}: {m}")).collect()
}

/// The doc for text that has just been validated.
fn open_parsed(file: PathBuf, name: String, source: String, project: &ProjectInfo) -> Result<OpenedDoc, AppError> {
    let (parsed, kind, xsd, mut warnings) = check_text(&file, &name, &source, project)?;
    warnings.extend(clash_warnings(&file, &source, project));
    Ok(OpenedDoc {
        path: path_str(&file),
        kind,
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

    /// Every inline ref in the test adventure resolves, with no problems, to the text the
    /// compiler prints.
    #[test]
    #[ignore]
    fn resolves_every_inline_ref() {
        use shared::inline::{self, Checker, RefCtx};
        let (p, v) = setup();
        let path = format!("{REPO}/adventures/compiler-test/adventure.xml");
        let doc = open_file(&path, &p, &v).unwrap();
        let index = ref_index(&path, &p, &v).unwrap();
        assert_eq!(index.entries["hearthward"].values["name"], "Thistlewood's Hearthward");
        assert_eq!(index.entries["krugga"].values["shortName"], "Krugga");
        let ctx = RefCtx::from_doc(&doc.source, &doc.nodes, Some(index));
        let mut seen = 0;
        for n in doc.nodes.iter().filter(|n| matches!(n.tag.as_str(), "p" | "li" | "trigger" | "td")) {
            let Some((a, b)) = shared::edit::inner_span(&doc.source, n) else { continue };
            let text = &doc.source[a..b];
            let pieces = inline::parse(text).unwrap_or_else(|e| panic!("line {}: {e:?}", n.line));
            let d = ctx.check(text, &pieces);
            assert!(d.is_empty(), "line {}: {d:?}", n.line);
            for t in inline::refs(&pieces) {
                let r = ctx.resolve(t);
                assert!(r.known, "line {}: {} unresolved", n.line, t.to_xml());
                seen += 1;
                if seen <= 60 {
                    println!("{:>4}  {:<55} {}", n.line, t.to_xml(), r.text);
                }
            }
        }
        assert!(seen > 30, "only {seen} refs");
    }

    #[test]
    #[ignore]
    fn opens_monster_files_rejects_spells() {
        let (p, v) = setup();
        let doc = open_file(&format!("{REPO}/adventures/compiler-test/apprentice-wizard.xml"), &p, &v).unwrap();
        assert_eq!(doc.kind, DocKind::Monsters);
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        let err = open_file(&format!("{REPO}/adventures/compiler-test/thistlewoods-hearthward.xml"), &p, &v).unwrap_err();
        println!("{}\n  {}", err.message, err.details.join("\n  "));
        assert!(err.message.contains("isn't a supplement, monster or magic item file"));
        // Every shared monster file opens and validates.
        let dir = format!("{REPO}/shared/internal/monsters");
        let mut n = 0;
        for f in std::fs::read_dir(&dir).unwrap().flatten().filter(|f| f.path().extension().is_some_and(|e| e == "xml")).take(25) {
            let doc = open_file(&path_str(&f.path()), &p, &v).unwrap_or_else(|e| panic!("{}: {} {:?}", f.path().display(), e.message, e.details));
            assert!(matches!(doc.validation, Validation::Valid { .. }));
            assert!(!shared::monster::monster_nodes(&doc.nodes).is_empty());
            n += 1;
        }
        assert!(n > 10);
    }

    #[test]
    #[ignore]
    fn monster_round_trip() {
        use shared::element::ElementEdit;
        use shared::monster::{self, MonsterEdit};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/shared/internal/monsters")).unwrap();
        let file = t.path().join("gnoll-warrior.xml");
        let src = std::fs::read_to_string(format!("{REPO}/shared/internal/monsters/gnoll-warrior.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        // The temp folder is one deeper: point the schema location one level further up.
        let src = src.replace("../../../schemas/", "../../../../schemas/");
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, change: shared::Change| match apply_edit(&path_str(&file), expected, &p, &v, |s| change.plan(s)).unwrap() {
            ApplyResult::Saved { doc } => doc,
            other => panic!("{change:?} refused: {other:?}"),
        };
        let doc = open_file(&path_str(&file), &p, &v).unwrap();
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
        let m = monster::monster_nodes(&doc.nodes)[0];
        let path_of = |nodes: &[shared::StructNode], idx: usize| -> Vec<usize> {
            fn find(nodes: &[shared::StructNode], at: usize, want: usize, path: &mut Vec<usize>) -> bool {
                if at == want {
                    return true;
                }
                for (k, c) in nodes[at].children.iter().enumerate() {
                    path.push(k);
                    if find(nodes, *c, want, path) {
                        return true;
                    }
                    path.pop();
                }
                false
            }
            let mut p = Vec::new();
            find(nodes, 0, idx, &mut p);
            p
        };
        // Edit the heading as raw XML.
        let h = doc.nodes[m].children.iter().copied().find(|c| doc.nodes[*c].tag == "heading").unwrap();
        let shown = shared::display::reindent(&doc.source, doc.nodes[h].start, doc.nodes[h].end);
        let edit = ElementEdit { path: path_of(&doc.nodes, h), tag: "heading".into(), xml: shown.replace("<armorClass>15<", "<armorClass>16<") };
        let doc = run(&doc.source, shared::Change::Element { edit });
        assert!(doc.source.contains("            <armorClass>16</armorClass>"), "indentation kept");
        // A schema error is refused.
        let h = doc.nodes[m].children.iter().copied().find(|c| doc.nodes[*c].tag == "heading").unwrap();
        let shown = shared::display::reindent(&doc.source, doc.nodes[h].start, doc.nodes[h].end);
        let bad = ElementEdit { path: path_of(&doc.nodes, h), tag: "heading".into(), xml: shown.replace("<armorClass>16</armorClass>", "") };
        let r = apply_edit(&path_str(&file), &doc.source, &p, &v, |s| shared::element::plan(s, &bad)).unwrap();
        let ApplyResult::Invalid { problems } = r else { panic!("expected a refusal") };
        println!("missing armorClass: {problems:?}");
        // Stat blocks: add (the skeleton validates), move, delete.
        let doc = run(&doc.source, shared::Change::Monster { edit: MonsterEdit::Add { after: None, id: "gnoll-pup".into(), name: "Gnoll Pup".into() } });
        let doc = run(&doc.source, shared::Change::Monster { edit: MonsterEdit::Move { id: "gnoll-pup".into(), before: Some("gnoll-warrior".into()) } });
        let ids: Vec<_> = monster::stat_blocks(&doc.source, &doc.nodes).into_iter().map(|(i, _)| i).collect();
        assert_eq!(ids, ["gnoll-pup", "gnoll-warrior"]);
        let doc = run(&doc.source, shared::Change::Monster { edit: MonsterEdit::Delete { id: "gnoll-pup".into() } });
        assert_eq!(monster::stat_blocks(&doc.source, &doc.nodes).len(), 1);

        // A new monster file validates.
        let req = shared::NewMonsterFile {
            parent: path_str(t.path()),
            file: "rats".into(),
            family: "Rats".into(),
            short_noun: "rat".into(),
            id: "giant-rat".into(),
            name: "Giant Rat".into(),
        };
        let f = crate::fsops::create_monster_file(&req, REPO, &p.schemas_dir).unwrap();
        let doc = open_file(&f, &p, &v).unwrap();
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    }

    /// Every stat block in the data: each card applied unchanged writes nothing.
    #[test]
    #[ignore]
    fn stat_cards_unchanged_are_no_ops() {
        use shared::statblock::{self as sb, El, StatEdit};
        fn xmls(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if p.is_dir() && !name.starts_with('.') && name != "node_modules" && name != "target" {
                    xmls(&p, out);
                } else if name.ends_with(".xml") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        xmls(Path::new(REPO), &mut files);
        let (mut checked, mut failures) = (0, Vec::new());
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            if !src.contains("<monsters") {
                continue;
            }
            let Ok(p) = xmldoc::parse(&src) else { continue };
            for (c, mi) in p.nodes[0].children.iter().enumerate() {
                let n = &p.nodes[*mi];
                if n.tag != "monster" || shared::monster::is_template(&src, n) {
                    continue;
                }
                let m = sb::read(&src, c).unwrap();
                let scoped = |e: &El, tags: &[&str]| {
                    let mut x = e.clone();
                    x.kids.retain(|k| tags.contains(&k.tag.as_str()));
                    (x, Some(tags.iter().map(|t| t.to_string()).collect::<Vec<_>>()))
                };
                let mut cards = vec![scoped(&m, &["name"]), scoped(&m, &["brief", "habitats", "lairRef"])];
                for k in &m.kids {
                    match k.tag.as_str() {
                        "heading" | "statBlock" | "attributes" | "quoteBox" | "image" | "description" | "distinctionTable" => cards.push((k.clone(), None)),
                        "traits" | "actions" | "bonusActions" | "reactions" | "legendaryActions" => {
                            cards.push(scoped(k, &["multiattack"]));
                            cards.push(scoped(k, &["uses"]));
                            cards.extend(k.kids.iter().filter(|i| i.tag == "trait" || sb::ITEMS.contains(&i.tag.as_str())).map(|i| (i.clone(), None)));
                        }
                        _ => {}
                    }
                }
                checked += 1;
                for (el, scope) in cards {
                    let tag = el.tag.clone();
                    match sb::plan(&src, &StatEdit::Update { child: c, el, scope }) {
                        Ok(out) if out == src => {}
                        Ok(out) => {
                            let at = src.bytes().zip(out.bytes()).position(|(a, b)| a != b).unwrap_or(0);
                            failures.push(format!("{}: {} <{tag}> changed near: {:?}", f.display(), m.get("id"), &src[at.saturating_sub(40)..(at + 40).min(src.len())]));
                        }
                        Err(e) => failures.push(format!("{}: {} <{tag}>: {e}", f.display(), m.get("id"))),
                    }
                }
            }
        }
        println!("{checked} stat blocks checked");
        assert!(checked > 300);
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures[..failures.len().min(20)].join("\n"));
    }

    /// Every magic item in the data: each card applied unchanged writes nothing.
    #[test]
    #[ignore]
    fn item_cards_unchanged_are_no_ops() {
        use shared::statblock::{self as sb, El, StatEdit};
        fn xmls(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                let name = p.file_name().unwrap().to_string_lossy().to_string();
                if p.is_dir() && !name.starts_with('.') && name != "node_modules" && name != "target" {
                    xmls(&p, out);
                } else if name.ends_with(".xml") {
                    out.push(p);
                }
            }
        }
        let mut files = Vec::new();
        xmls(Path::new(REPO), &mut files);
        let (mut checked, mut failures) = (0, Vec::new());
        for f in files {
            let src = std::fs::read_to_string(&f).unwrap();
            if !src.contains("<magicItem") {
                continue;
            }
            let Some(m) = sb::read(&src, sb::ROOT) else { continue };
            if m.tag != "magicItem" {
                continue;
            }
            checked += 1;
            let header: Vec<String> = ["name", "type", "subtypes", "rarities", "attunement"].iter().map(|t| t.to_string()).collect();
            let mut h = m.clone();
            h.kids.retain(|k| header.contains(&k.tag));
            let mut cards: Vec<(El, Option<Vec<String>>)> = vec![(h, Some(header))];
            if let Some(d) = m.kid("description") {
                for b in &d.kids {
                    cards.push((b.clone(), None));
                    cards.extend(b.kids.iter().map(|k| (k.clone(), None)));
                }
            }
            for (el, scope) in cards {
                let tag = el.tag.clone();
                match sb::plan(&src, &StatEdit::Update { child: sb::ROOT, el, scope }) {
                    Ok(out) if out == src => {}
                    Ok(out) => {
                        let at = src.bytes().zip(out.bytes()).position(|(a, b)| a != b).unwrap_or(0);
                        failures.push(format!("{}: <{tag}> changed near: {:?}", f.display(), &src[at.saturating_sub(40)..(at + 40).min(src.len())]));
                    }
                    Err(e) => failures.push(format!("{}: <{tag}>: {e}", f.display())),
                }
            }
        }
        println!("{checked} magic items checked");
        assert!(checked > 250);
        assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures[..failures.len().min(20)].join("\n"));
    }

    /// A new magic item, built up through its cards and reference edits, against the
    /// magic item schema.
    #[test]
    #[ignore]
    fn magic_item_round_trip() {
        use shared::reference::{RefEdit, RefEntry};
        use shared::statblock::{self as sb, El, StatEdit, ROOT};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures/compiler-test")).unwrap();
        let req = shared::NewMagicItem {
            parent: path_str(t.path()),
            file: "lantern".into(),
            id: "lantern".into(),
            name: "Lantern of Lisra".into(),
            item_type: "wondrous-item".into(),
            rarity: "rare".into(),
        };
        let path = crate::fsops::create_magic_item_file(&req, REPO, &p.schemas_dir).unwrap();
        let doc = open_file(&path, &p, &v).unwrap();
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        assert_eq!(doc.kind, DocKind::MagicItem);
        let run = |expected: &str, change: shared::Change| match apply_edit(&path, expected, &p, &v, |s| change.plan(s)).unwrap() {
            ApplyResult::Saved { doc } => doc,
            other => panic!("{change:?} refused: {other:?}"),
        };
        let stat = |e: StatEdit| shared::Change::Stat { edit: e };
        let m = |s: &str| sb::read(s, ROOT).unwrap();

        // A creator entry, then the header: the name pulls the creator's name.
        let mut lisra = RefEntry::new("monster");
        lisra.attrs = vec![("id".into(), "lisra".into()), ("name".into(), "Lisra".into()), ("shortName".into(), "Lisra".into()), ("cr".into(), "3".into())];
        let doc = run(&doc.source, shared::Change::Reference { edit: RefEdit::Add { entry: lisra } });
        let header: Vec<String> = ["name", "type", "subtypes", "rarities", "attunement"].iter().map(|s| s.to_string()).collect();
        let mut h = m(&doc.source);
        h.kids.retain(|k| header.contains(&k.tag));
        h.set("creator", "lisra");
        h.set("status", "mechanics-locked");
        h.kid_mut("name").unwrap().text = "<monsterRef ref=\"lisra\" value=\"shortName\"/>'s Lantern".into();
        h.kid_mut("subtypes").unwrap().kids.push(El::new("subtype").with_text("Lantern"));
        h.kid_mut("rarities").unwrap().kids.push(El::new("rarity").with("note", "awakened").with_text("very-rare"));
        let att = h.kid_mut("attunement").unwrap();
        att.set("required", "true");
        att.insert_kid(El::new("attunementRequirements").with_kid(El::new("attunementRequirement").with_text("Wizard")));
        let doc = run(&doc.source, stat(StatEdit::Update { child: ROOT, el: h, scope: Some(header) }));
        assert!(doc.source.contains("<attunementRequirement>Wizard</attunementRequirement>"), "{}", doc.source);

        // The first paragraph, an ability with a table, a list, and a sidebar.
        let d = m(&doc.source).kid("description").unwrap().clone();
        let mut para = d.kids[0].clone();
        para.text = "It sheds light in a <area name=\"sphere\" radius=\"30\"/>; creatures take <damage num=\"2\" sides=\"6\" type=\"radiant\"/>.".into();
        let doc = run(&doc.source, stat(StatEdit::Update { child: ROOT, el: para, scope: None }));
        let dp = d.path.clone().unwrap();
        let ability = El::new("ability").with("name", "Flare").with_kid(El::new("p").with_text("Roll <roll num=\"1\" sides=\"4\"/>."))
            .with_kid(El::new("table").with_kid(El::new("header").with_kid(El::new("th").with_text("d4")).with_kid(El::new("th").with_text("Effect")))
                .with_kid(El::new("tableBody").with_kid(El::new("tr").with_kid(El::new("td").with_text("1")).with_kid(El::new("td").with_text("Nothing")))));
        let doc = run(&doc.source, stat(StatEdit::Insert { child: ROOT, parent: dp.clone(), parent_tag: "description".into(), el: ability, after: Some(0) }));
        let list = El::new("list").with("bullet", "number").with_kid(El::new("li").with_text("One")).with_kid(El::new("li").with_text("Two"));
        let doc = run(&doc.source, stat(StatEdit::Insert { child: ROOT, parent: dp.clone(), parent_tag: "description".into(), el: list, after: Some(1) }));
        let side = El::new("sidebar").with_kid(El::new("title").with_text("Lore")).with_kid(El::new("p").with_text("Old."));
        let doc = run(&doc.source, stat(StatEdit::Insert { child: ROOT, parent: dp.clone(), parent_tag: "description".into(), el: side, after: Some(2) }));
        // A row added to the ability's table; the list moved to the front; the sidebar gone.
        let mut table = m(&doc.source).at(&[("description", 0), ("ability", 0), ("table", 0)]).unwrap().clone();
        table.kid_mut("tableBody").unwrap().kids.push(El::new("tr").with_kid(El::new("td").with_text("2")).with_kid(El::new("td").with_text("A <condition>blinded</condition> foe")));
        let doc = run(&doc.source, stat(StatEdit::Update { child: ROOT, el: table, scope: None }));
        let mut lp = dp.clone();
        lp.push(2);
        let doc = run(&doc.source, stat(StatEdit::Move { child: ROOT, path: lp, tag: "list".into(), before: Some(0) }));
        assert_eq!(m(&doc.source).kid("description").unwrap().kids[0].tag, "list");
        let mut sp = dp.clone();
        sp.push(3);
        let doc = run(&doc.source, stat(StatEdit::Delete { child: ROOT, path: sp, tag: "sidebar".into() }));

        // Renaming the creator's entry renames the creator and the name's ref.
        let mut renamed = RefEntry::new("monster");
        renamed.attrs = vec![("id".into(), "lisra-the-bright".into()), ("name".into(), "Lisra".into()), ("shortName".into(), "Lisra".into()), ("cr".into(), "3".into())];
        let doc = run(&doc.source, shared::Change::Reference { edit: RefEdit::Update { tag: "monster".into(), id: "lisra".into(), entry: renamed } });
        assert!(doc.source.contains("creator=\"lisra-the-bright\"") && doc.source.contains("<monsterRef ref=\"lisra-the-bright\""), "{}", doc.source);

        // A bad rarity is refused, and nothing is written.
        let mut bad = m(&doc.source).at(&[("rarities", 0)]).unwrap().clone();
        bad.kids[0].text = "mythic".into();
        let change = stat(StatEdit::Update { child: ROOT, el: bad, scope: None });
        let r = apply_edit(&path, &doc.source, &p, &v, |s| change.plan(s)).unwrap();
        assert!(matches!(r, ApplyResult::Invalid { .. }), "expected a refusal");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), doc.source);
        println!("{}", doc.source);
    }

    /// A real stat block edited through its cards, against the monster schema.
    #[test]
    #[ignore]
    fn stat_block_round_trip() {
        use shared::statblock::{self as sb, El, StatEdit};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures/compiler-test")).unwrap();
        let file = t.path().join("aboleth.xml");
        let src = std::fs::read_to_string(format!("{REPO}/shared/internal/monsters/aboleth.xml")).unwrap();
        let k = src.find("noNamespaceSchemaLocation=\"").unwrap() + "noNamespaceSchemaLocation=\"".len();
        let e = k + src[k..].find('"').unwrap();
        let src = format!("{}../../../schemas/monster-template-schema.xsd{}", &src[..k], &src[e..]);
        std::fs::write(&file, &src).unwrap();
        let path = path_str(&file);
        let doc = open_file(&path, &p, &v).unwrap();
        assert!(matches!(doc.validation, Validation::Valid { .. }), "{:?}", doc.validation);
        let child = doc.nodes[0].children.iter().position(|c| doc.nodes[*c].tag == "monster").unwrap();
        let run = |expected: &str, edit: StatEdit| {
            let change = shared::Change::Stat { edit };
            match apply_edit(&path, expected, &p, &v, |s| change.plan(s)).unwrap() {
                ApplyResult::Saved { doc } => doc,
                other => panic!("{change:?} refused: {other:?}"),
            }
        };
        let m = |s: &str| sb::read(s, child).unwrap();
        let up = |s: &str, el: El| run(s, StatEdit::Update { child, el, scope: None });

        // Heading: AC, a new climb speed, hit points beside the dice.
        let mut h = m(&doc.source).kid("heading").unwrap().clone();
        h.kid_mut("armorClass").unwrap().text = "18".into();
        h.kid_mut("speeds").unwrap().kids.push(El::new("speed").with("id", "climb").with("name", "climb").with("value", "20"));
        h.set_opt_text("hitPoints", "150", false);
        let doc = up(&doc.source, h);
        assert!(doc.source.contains("<armorClass>18</armorClass>") && doc.source.contains("<hitPoints>150</hitPoints>"));

        // The tentacle: renamed (its refs follow), a rider and a miss.
        let mut a = m(&doc.source).at(&[("actions", 0), ("attack", 0)]).unwrap().clone();
        a.set("id", "tendril");
        a.set("reach", "20");
        a.insert_kid(El::new("rider").with("damage", "1d6").with("damageType", "psychic"));
        a.set_opt_text("miss", "The <shortNounRef/> recoils.", false);
        let doc = up(&doc.source, a);
        assert!(!doc.source.contains("ref=\"tentacle\"") && doc.source.contains("<attackRef ref=\"tendril\"/>"), "{}", doc.source);

        // Attributes: a skill more, no senses.
        let mut at = m(&doc.source).kid("attributes").unwrap().clone();
        at.kid_mut("skills").unwrap().kids.push(El::new("skill").with("name", "arcana").with("stat", "int").with("proficiency", "true"));
        at.remove_kid("senses");
        let doc = up(&doc.source, at);
        assert!(doc.source.contains("name=\"arcana\"") && !doc.source.contains("<senses>"));

        // A trait: uses added; a new trait in, up, and out.
        let mut tr = m(&doc.source).at(&[("traits", 0), ("trait", 0)]).unwrap().clone();
        tr.insert_kid(El::new("uses").with("number", "1").with("frequency", "day"));
        let doc = up(&doc.source, tr);
        let traits = m(&doc.source).kid("traits").unwrap().clone();
        let tp = traits.path.clone().unwrap();
        let new = El::new("trait").with("name", "Slick").with_kid(El::new("description").with_text("It's <condition>prone</condition>-proof."));
        let doc = run(&doc.source, StatEdit::Insert { child, parent: tp.clone(), parent_tag: "traits".into(), el: new, after: None });
        let n = m(&doc.source).kid("traits").unwrap().kids.len();
        let mut last = tp.clone();
        last.push(n - 1);
        let doc = run(&doc.source, StatEdit::Move { child, path: last, tag: "trait".into(), before: Some(0) });
        assert_eq!(m(&doc.source).at(&[("traits", 0), ("trait", 0)]).unwrap().get("name"), "Slick");
        let mut first = tp.clone();
        first.push(0);
        let doc = run(&doc.source, StatEdit::Delete { child, path: first, tag: "trait".into() });

        // New lists: bonus actions, and a reaction with its trigger and response.
        let ba = El::new("bonusActions").with_kid(El::new("action").with("id", "slither").with("name", "Slither").with_text("It moves."));
        let doc = run(&doc.source, StatEdit::Insert { child, parent: vec![], parent_tag: "monster".into(), el: ba, after: None });
        let re = El::new("reactions").with_kid(El::new("action").with("id", "ink").with("name", "Ink").with_text("Ink spreads."));
        let doc = run(&doc.source, StatEdit::Insert { child, parent: vec![], parent_tag: "monster".into(), el: re, after: None });
        let mut ink = m(&doc.source).at(&[("reactions", 0), ("action", 0)]).unwrap().clone();
        ink.set_opt_text("trigger", "It takes damage.", false);
        ink.set_opt_text("response", "It swims away.", false);
        ink.text = "Ink fills a 10-foot <area name=\"cube\"/>.".into();
        let doc = up(&doc.source, ink);
        let back = m(&doc.source).at(&[("reactions", 0), ("action", 0)]).unwrap().clone();
        assert_eq!(back.text, "Ink fills a 10-foot <area name=\"cube\"/>.");
        assert_eq!(back.opt_text("trigger"), "It takes damage.");

        // A schema error (bad dice) is refused, and nothing is written.
        let mut bad = m(&doc.source).at(&[("actions", 0), ("attack", 0)]).unwrap().clone();
        bad.set("damage", "2x6");
        let change = shared::Change::Stat { edit: StatEdit::Update { child, el: bad, scope: None } };
        let r = apply_edit(&path, &doc.source, &p, &v, |s| change.plan(s)).unwrap();
        assert!(matches!(r, ApplyResult::Invalid { .. }), "expected a refusal");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), doc.source);
        println!("{}", doc.source);
    }

    /// The family's header and parts, edited against the real monster schema.
    #[test]
    #[ignore]
    fn family_round_trip() {
        use shared::family::{self, FamilyEdit, ImageModel, PartAt};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures/compiler-test")).unwrap();
        let file = t.path().join("kennel-abomination.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/kennel-abomination.xml")).unwrap();
        // The temp folder is one deeper: point the schema location one level further up.
        std::fs::write(&file, src.replace("\"../../schemas/", "\"../../../schemas/")).unwrap();
        let path = path_str(&file);
        let run = |expected: &str, edit: FamilyEdit| {
            let change = shared::Change::Family { edit };
            match apply_edit(&path, expected, &p, &v, |s| change.plan(s)).unwrap() {
                ApplyResult::Saved { doc } => doc,
                other => panic!("{change:?} refused: {other:?}"),
            }
        };
        let fam = |s: &str| family::read(s).unwrap();
        let part = |s: &str, tag: &str, nth: usize| -> PartAt { fam(s).parts.into_iter().filter(|x| x.tag == tag).nth(nth).unwrap().at() };
        let doc = open_file(&path, &p, &v).unwrap();
        assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);

        // Header: drop the subtitle and a habitat, add treasure; then put the subtitle back.
        let mut h = fam(&doc.source).header;
        let brief = h.brief.clone();
        h.brief.clear();
        h.brief_id.clear();
        h.habitats.retain(|x| x != "Hill");
        h.treasure.push("Relics".into());
        let doc = run(&doc.source, FamilyEdit::Header { model: h.clone() });
        assert!(!doc.source.contains("<brief id=\"subtitle\""));
        h.brief = brief;
        h.brief_id = "subtitle".into();
        let doc = run(&doc.source, FamilyEdit::Header { model: h });
        assert_eq!(fam(&doc.source).header.treasure, vec!["None", "Relics"]);

        // A quote box and an image right after the header: they validate as added.
        let doc = run(&doc.source, FamilyEdit::AddParts { after: None, tags: vec!["quoteBox".into(), "image".into()] });
        let q = part(&doc.source, "quoteBox", 0);
        let doc = run(&doc.source, FamilyEdit::SetPara { at: q.clone(), n: 0, text: "A second <i>quote</i>.".into() });
        let doc = run(&doc.source, FamilyEdit::Attribution { at: q, name: "A Witness".into() });
        let img = part(&doc.source, "image", 0);
        let model = ImageModel { src: "ossivore.png".into(), name: "Second".into(), credit: "Someone".into(), cutline: "The <shortNounRef/> again.".into(), ..Default::default() };
        let doc = run(&doc.source, FamilyEdit::Image { at: img, model });

        // A new lair, its effects and closing line; moved to the front of the parts.
        let last = fam(&doc.source).parts.last().unwrap().child;
        let doc = run(&doc.source, FamilyEdit::AddParts { after: Some(last), tags: vec!["lair".into(), "description".into(), "distinctionTable".into()] });
        let l = part(&doc.source, "lair", 2);
        let doc = run(&doc.source, FamilyEdit::Lair { at: l.clone(), name: "The Third Den".into(), end_verb: "dies".into() });
        let doc = run(&doc.source, FamilyEdit::AddEffect { at: l.clone(), after: Some(0) });
        let doc = run(&doc.source, FamilyEdit::SetEffect { at: l.clone(), n: 1, id: "den-howl".into(), name: "Den Howl".into(), text: "The <shortNounRef/> howls.".into() });
        let doc = run(&doc.source, FamilyEdit::AddPara { at: l.clone(), after: None });
        let doc = run(&doc.source, FamilyEdit::SetPara { at: l.clone(), n: 0, text: "A den.".into() });
        let first = fam(&doc.source).parts[0].child;
        let doc = run(&doc.source, FamilyEdit::MovePart { at: l, before: Some(first) });
        assert_eq!(fam(&doc.source).parts[0].name, "The Third Den");

        // The new table, reshaped; the new description, filled in.
        let dt = part(&doc.source, "distinctionTable", 1);
        let doc = run(&doc.source, FamilyEdit::Table { at: dt, heads: vec!["1d2".into(), "Quirk".into()], rows: vec![vec!["1".into(), "One".into()], vec!["2".into(), "<b>Two</b>".into()]] });
        let d = part(&doc.source, "description", 1);
        let doc = run(&doc.source, FamilyEdit::SetPara { at: d.clone(), n: 0, text: "More lore.".into() });
        let doc = run(&doc.source, FamilyEdit::SetId { at: d, id: "more-lore".into() });

        // Renaming a lair renames its lairRefs; deleting one drops them.
        let kennel = fam(&doc.source).parts.into_iter().find(|x| x.id == "kennel").unwrap().at();
        let doc = run(&doc.source, FamilyEdit::SetId { at: kennel, id: "old-kennel".into() });
        assert!(doc.source.contains("<lairRef ref=\"old-kennel\"/>"));
        let shrine = fam(&doc.source).parts.into_iter().find(|x| x.id == "shrine").unwrap().at();
        let doc = run(&doc.source, FamilyEdit::DeletePart { at: shrine });
        assert!(!doc.source.contains("shrine"));
        // A duplicate id is refused before validation.
        let d = part(&doc.source, "description", 0);
        assert!(family::plan(&doc.source, &FamilyEdit::SetId { at: d, id: "old-kennel".into() }).is_err());
        // A schema error (a width that isn't a number) is refused, and nothing is written.
        let img = part(&doc.source, "image", 1);
        let bad = FamilyEdit::Image { at: img, model: ImageModel { src: "x.png".into(), width: "wide".into(), ..Default::default() } };
        let r = apply_edit(&path, &doc.source, &p, &v, |s| family::plan(s, &bad)).unwrap();
        let ApplyResult::Invalid { problems } = r else { panic!("expected a refusal") };
        println!("bad width: {problems:?}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), doc.source);
        println!("{}", doc.source);
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
    fn apply_list_round_trip() {
        use shared::block::{self, BlockEdit, Container};
        use shared::list::{self, ListAt, ListEdit};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, change: shared::Change| match apply_edit(&path_str(&file), expected, &p, &v, |s| change.plan(s)).unwrap() {
            ApplyResult::Saved { doc } => doc.source,
            other => panic!("{change:?} refused: {other:?}"),
        };
        let l = |e: ListEdit| shared::Change::List { edit: e };
        let sec = Container::Section { chapter: "hillsfort".into(), name: "town-at-a-glance".into(), nth: 0 };
        let nodes = xmldoc::parse(&src).unwrap().nodes;
        let ci = block::find(&src, &nodes, &sec).unwrap();
        let child = nodes[ci].children.iter().position(|c| nodes[*c].tag == "list").unwrap();
        let at = ListAt { at: sec.clone(), child };
        let view = |s: &str| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            list::find(s, &nodes, &at).map(|i| list::read(s, &nodes, i))
        };
        let v0 = view(&src).unwrap();
        assert!(v0.items[0].starts_with("<b>The Trade Road.</b>"), "{:?}", v0.items[0]);

        let cur = run(&src, l(ListEdit::SetItem { at: at.clone(), n: 0, text: v0.items[0].replace("east to west", "west to east") }));
        assert!(cur.contains("west to east"));
        let cur = run(&cur, l(ListEdit::Attrs { at: at.clone(), bullet: "number".into(), style: String::new() }));
        assert_eq!(view(&cur).unwrap().bullet, "number");
        let cur = run(&cur, l(ListEdit::AddItem { at: at.clone(), after: Some(0) }));
        let cur = run(&cur, l(ListEdit::SetItem { at: at.clone(), n: 1, text: "A new item.".into() }));
        let cur = run(&cur, l(ListEdit::MoveItem { at: at.clone(), n: 1, before: None }));
        assert_eq!(view(&cur).unwrap().items.last().unwrap(), "A new item.");
        // A bad item is refused.
        let r = apply_edit(&path_str(&file), &cur, &p, &v, |s| list::plan(s, &ListEdit::SetItem { at: at.clone(), n: 0, text: "<b>open".into() })).unwrap();
        assert!(matches!(r, ApplyResult::Invalid { .. }));
        // Deleting every item deletes the list.
        let mut cur = cur;
        while let Some(v) = view(&cur) {
            cur = run(&cur, l(ListEdit::DeleteItem { at: at.clone(), n: v.items.len() - 1 }));
        }
        assert!(view(&cur).is_none());
        // And a new list can go back in.
        let cur = run(&cur, shared::Change::Block { edit: BlockEdit::AddList { at: sec.clone(), after: Some(child - 1) } });
        assert_eq!(view(&cur).unwrap().items, [""]);
    }

    #[test]
    #[ignore]
    fn apply_box_round_trip() {
        use shared::block::{self, BlockEdit};
        use shared::boxes::{self, BoxEdit};
        let (p, v) = setup();
        let t = tempfile::tempdir_in(format!("{REPO}/adventures")).unwrap();
        let file = t.path().join("adventure.xml");
        let src = std::fs::read_to_string(format!("{REPO}/adventures/compiler-test/adventure.xml")).unwrap();
        std::fs::write(&file, &src).unwrap();
        let run = |expected: &str, change: shared::Change| match apply_edit(&path_str(&file), expected, &p, &v, |s| change.plan(s)).unwrap() {
            ApplyResult::Saved { doc } => doc.source,
            other => panic!("{change:?} refused: {other:?}"),
        };
        let b = |e: BoxEdit| shared::Change::Box { edit: e };
        // The first dialog in the file.
        let nodes = xmldoc::parse(&src).unwrap().nodes;
        let mut parents = vec![None; nodes.len()];
        for (i, n) in nodes.iter().enumerate() {
            for c in &n.children {
                parents[*c] = Some(i);
            }
        }
        let di = nodes.iter().position(|n| n.tag == "dialog").unwrap();
        let at = boxes::box_at(&src, &nodes, &parents, di).unwrap();
        let view = |s: &str| {
            let nodes = xmldoc::parse(s).unwrap().nodes;
            boxes::find(s, &nodes, &at).map(|i| boxes::read(s, &nodes, i))
        };
        let v0 = view(&src).unwrap();
        assert_eq!(v0.tag, "dialog");
        assert!(!v0.speaker.is_empty() && v0.paras.len() >= 2);

        let cur = run(&src, b(BoxEdit::Speaker { at: at.clone(), speaker: "Garrick Moss".into() }));
        let cur = run(&cur, b(BoxEdit::SetTrigger { at: at.clone(), text: "When the characters ask about the <b>dogs</b>".into() }));
        assert!(view(&cur).unwrap().has_trigger);
        let cur = run(&cur, b(BoxEdit::AddP { at: at.clone(), after: None }));
        let cur = run(&cur, b(BoxEdit::SetP { at: at.clone(), n: 0, text: "First, a <monsterRef ref=\"krugga\" value=\"shortName\"/>.".into() }));
        let cur = run(&cur, b(BoxEdit::MoveP { at: at.clone(), n: 0, before: None }));
        assert!(view(&cur).unwrap().paras.last().unwrap().starts_with("First"));
        let cur = run(&cur, b(BoxEdit::SetTrigger { at: at.clone(), text: String::new() }));
        assert!(!view(&cur).unwrap().has_trigger);
        // Delete paragraphs down to one, then leave the box empty: still valid.
        let mut cur = cur;
        while view(&cur).unwrap().paras.len() > 0 {
            cur = run(&cur, b(BoxEdit::DeleteP { at: at.clone(), n: 0, with_box: false }));
        }
        // A new read-aloud box before it, then delete the dialog.
        let sec = at.at.clone();
        let cur = run(&cur, shared::Change::Block { edit: BlockEdit::AddBox { at: sec.clone(), after: Some(at.child - 1), tag: "speakAloud".into() } });
        let moved = boxes::BoxAt { at: sec.clone(), child: at.child + 1 };
        let cur = run(&cur, b(BoxEdit::Delete { at: moved }));
        let nodes = xmldoc::parse(&cur).unwrap().nodes;
        let ci = block::find(&cur, &nodes, &sec).unwrap();
        assert_eq!(nodes[nodes[ci].children[at.child]].tag, "speakAloud");
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
