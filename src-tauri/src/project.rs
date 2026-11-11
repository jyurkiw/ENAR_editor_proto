//! Project folders and their enigma-project.toml.
//!
//! A project is any folder. Its enigma-project.toml says where the schemas and the
//! compiler are; those paths are written relative to the project root when possible.

use std::path::{Path, PathBuf};

use shared::{AppError, OpenProject, ProjectConfig, ProjectInfo, PROJECT_FILE, SUPPLEMENT_SCHEMA};

pub fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `p` resolved against `base` when it is relative.
fn resolve(base: &Path, p: &str) -> PathBuf {
    let p = Path::new(p);
    let joined = if p.is_absolute() { p.to_path_buf() } else { base.join(p) };
    dunce::canonicalize(&joined).unwrap_or(joined)
}

/// What's wrong with these folders as a schemas folder and a compiler folder, if anything.
fn check_dirs(schemas: &Path, compiler: &Path) -> Option<String> {
    if !schemas.join(SUPPLEMENT_SCHEMA).is_file() {
        return Some(format!("{} has no {SUPPLEMENT_SCHEMA}", schemas.display()));
    }
    if !compiler.join("pyproject.toml").is_file() {
        return Some(format!("{} has no pyproject.toml (it should be the encompile compiler folder)", compiler.display()));
    }
    None
}

fn info(root: &Path, schemas: PathBuf, compiler: PathBuf) -> ProjectInfo {
    ProjectInfo {
        root: path_str(root),
        name: root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path_str(root)),
        schemas_dir: path_str(&schemas),
        compiler_dir: path_str(&compiler),
    }
}

/// Folders above (and including) root that look like a schemas folder and a compiler,
/// found the way the compiler finds schemas/.
fn suggestions(root: &Path) -> (Option<String>, Option<String>) {
    for dir in root.ancestors() {
        let schemas = dir.join("schemas");
        if schemas.join(SUPPLEMENT_SCHEMA).is_file() {
            let compiler = dir.join("compiler");
            let compiler = compiler.join("pyproject.toml").is_file().then(|| path_str(&compiler));
            return (Some(path_str(&schemas)), compiler);
        }
    }
    (None, None)
}

pub fn open(root: &str) -> Result<OpenProject, AppError> {
    let root = dunce::canonicalize(root).map_err(|e| AppError::new(format!("Can't open {root}: {e}")))?;
    if !root.is_dir() {
        return Err(AppError::new(format!("{} isn't a folder", root.display())));
    }
    let needs = |problem: Option<String>| {
        let (suggested_schemas, suggested_compiler) = suggestions(&root);
        Ok(OpenProject::NeedsConfig { root: path_str(&root), suggested_schemas, suggested_compiler, problem })
    };
    let file = root.join(PROJECT_FILE);
    if !file.is_file() {
        return needs(None);
    }
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) => return needs(Some(format!("Can't read {PROJECT_FILE}: {e}"))),
    };
    let config: ProjectConfig = match toml::from_str(&text) {
        Ok(c) => c,
        Err(e) => return needs(Some(format!("{PROJECT_FILE} isn't valid: {}", e.message()))),
    };
    let schemas = resolve(&root, &config.schemas_dir);
    let compiler = resolve(&root, &config.compiler_dir);
    if let Some(problem) = check_dirs(&schemas, &compiler) {
        return needs(Some(problem));
    }
    Ok(OpenProject::Ready { project: info(&root, schemas, compiler) })
}

/// Writes enigma-project.toml for root and returns the opened project.
pub fn configure(root: &str, schemas_dir: &str, compiler_dir: &str) -> Result<ProjectInfo, AppError> {
    let root = dunce::canonicalize(root).map_err(|e| AppError::new(format!("Can't open {root}: {e}")))?;
    let schemas = resolve(&root, schemas_dir);
    let compiler = resolve(&root, compiler_dir);
    if let Some(problem) = check_dirs(&schemas, &compiler) {
        return Err(AppError::new(problem));
    }
    let rel = |p: &Path| {
        pathdiff::diff_paths(p, &root).map(|r| path_str(&r).replace('\\', "/")).unwrap_or_else(|| path_str(p))
    };
    let config = ProjectConfig { schemas_dir: rel(&schemas), compiler_dir: rel(&compiler) };
    let text = format!(
        "# Enigma Narrative project settings, written by the editor.\n\
         # Paths are relative to this file's folder.\n\n{}",
        toml::to_string(&config).map_err(|e| AppError::new(e.to_string()))?
    );
    std::fs::write(root.join(PROJECT_FILE), text)
        .map_err(|e| AppError::new(format!("Can't write {PROJECT_FILE}: {e}")))?;
    Ok(info(&root, schemas, compiler))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("schemas")).unwrap();
        std::fs::write(t.path().join("schemas").join(SUPPLEMENT_SCHEMA), "<xs:schema/>").unwrap();
        std::fs::create_dir_all(t.path().join("compiler")).unwrap();
        std::fs::write(t.path().join("compiler/pyproject.toml"), "").unwrap();
        std::fs::create_dir_all(t.path().join("adventures")).unwrap();
        t
    }

    #[test]
    fn unconfigured_project_suggests_paths() {
        let t = layout();
        let root = t.path().join("adventures");
        match open(&path_str(&root)).unwrap() {
            OpenProject::NeedsConfig { suggested_schemas, suggested_compiler, problem, .. } => {
                assert!(suggested_schemas.unwrap().ends_with("schemas"));
                assert!(suggested_compiler.unwrap().ends_with("compiler"));
                assert!(problem.is_none());
            }
            other => panic!("expected NeedsConfig, got {other:?}"),
        }
    }

    #[test]
    fn configure_writes_relative_paths_and_reopens() {
        let t = layout();
        let root = t.path().join("adventures");
        let schemas = path_str(&t.path().join("schemas"));
        let compiler = path_str(&t.path().join("compiler"));
        configure(&path_str(&root), &schemas, &compiler).unwrap();
        let text = std::fs::read_to_string(root.join(PROJECT_FILE)).unwrap();
        assert!(text.contains("schemas_dir = \"../schemas\""), "{text}");
        assert!(matches!(open(&path_str(&root)).unwrap(), OpenProject::Ready { .. }));
    }

    #[test]
    fn configure_rejects_bad_schemas_dir() {
        let t = layout();
        let root = path_str(&t.path().join("adventures"));
        let err = configure(&root, &root, &path_str(&t.path().join("compiler"))).unwrap_err();
        assert!(err.message.contains(SUPPLEMENT_SCHEMA));
    }
}
