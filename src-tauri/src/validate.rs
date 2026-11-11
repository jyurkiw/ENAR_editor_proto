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

fn run_lxml(v: &Validator, compiler: &str, file: &Path, xsd: &Path) -> Outcome {
    let mut cmd = Command::new("uv");
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
            let mut problems = vec![Problem { line: None, message: e.message }];
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
}
