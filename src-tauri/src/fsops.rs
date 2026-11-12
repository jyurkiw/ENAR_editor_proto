//! Directory listing and new-supplement creation.

use std::path::Path;

use shared::{main_file_for_type, is_supplement_type, AppError, FsEntry, NewMonsterFile, NewSupplement, MONSTER_SCHEMA, SUPPLEMENT_SCHEMA};

use crate::project::path_str;

/// Folders hidden from the file tree: dot-folders, Python caches, build output.
fn hidden(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "__pycache__" | "node_modules" | "target")
}

/// The entries of one folder: folders first, then files, each sorted case-insensitively.
pub fn list_dir(path: &str) -> Result<Vec<FsEntry>, AppError> {
    let rd = std::fs::read_dir(path).map_err(|e| AppError::new(format!("Can't list {path}: {e}")))?;
    let mut out: Vec<FsEntry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if hidden(&name) {
                return None;
            }
            let is_dir = e.file_type().ok()?.is_dir();
            Some(FsEntry { name, path: path_str(&e.path()), is_dir })
        })
        .collect();
    out.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(out)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// What's wrong with a folder name, if anything.
fn bad_folder_name(name: &str) -> Option<&'static str> {
    if name.trim().is_empty() {
        return Some("The folder name is empty");
    }
    if name != name.trim() {
        return Some("The folder name starts or ends with a space");
    }
    if name == "." || name == ".." {
        return Some("The folder name can't be . or ..");
    }
    if name.chars().any(|c| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control()) {
        return Some("The folder name can't contain / \\ : * ? \" < > |");
    }
    if name.ends_with('.') {
        return Some("The folder name can't end with a dot");
    }
    None
}

/// The text of a new, schema-valid supplement: a cover with its title, one chapter with
/// one section, and the chapter order.
pub fn skeleton(kind: &str, title: &str, schema_location: &str) -> String {
    let title = xml_escape(title.trim());
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<supplement type="{kind}" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="{loc}">

    <cover>
        <headerTitle>{title}</headerTitle>
    </cover>

    <chapter name="introduction">
        <chapterTitle>Introduction</chapterTitle>

        <section name="intro-opening">
            <p></p>
        </section>
    </chapter>

    <chapterOrder>
        <ch name="introduction"/>
    </chapterOrder>
</supplement>
"#,
        loc = xml_escape(schema_location)
    )
}

/// Creates `parent/folder/` and its adventure.xml or supplement.xml. Returns the file's path.
/// `parent` must be the project root or a folder inside it.
pub fn create_supplement(req: &NewSupplement, project_root: &str, schemas_dir: &str) -> Result<String, AppError> {
    if let Some(problem) = bad_folder_name(&req.folder) {
        return Err(AppError::new(problem));
    }
    if !is_supplement_type(&req.kind) {
        return Err(AppError::new(format!("\"{}\" isn't a supplement type", req.kind)));
    }
    if req.title.trim().is_empty() {
        return Err(AppError::new("The title is empty"));
    }
    let root = dunce::canonicalize(project_root).map_err(|e| AppError::new(format!("Can't open the project: {e}")))?;
    let parent = dunce::canonicalize(&req.parent).map_err(|e| AppError::new(format!("Can't open {}: {e}", req.parent)))?;
    if !parent.starts_with(&root) {
        return Err(AppError::new(format!("{} isn't inside the project", parent.display())));
    }
    let dir = parent.join(&req.folder);
    if dir.exists() {
        return Err(AppError::new(format!("{} already exists", dir.display())));
    }
    let schema = Path::new(schemas_dir).join(SUPPLEMENT_SCHEMA);
    let loc = pathdiff::diff_paths(&schema, &dir)
        .map(|p| path_str(&p).replace('\\', "/"))
        .unwrap_or_else(|| path_str(&schema).replace('\\', "/"));
    std::fs::create_dir(&dir).map_err(|e| AppError::new(format!("Can't create {}: {e}", dir.display())))?;
    let file = dir.join(main_file_for_type(&req.kind));
    std::fs::write(&file, skeleton(&req.kind, &req.title, &loc))
        .map_err(|e| AppError::new(format!("Can't write {}: {e}", file.display())))?;
    Ok(path_str(&file))
}

/// Creates a new monster file in `req.parent`: the family and one stat block.
pub fn create_monster_file(req: &NewMonsterFile, project_root: &str, schemas_dir: &str) -> Result<String, AppError> {
    let stem = req.file.trim().trim_end_matches(".xml");
    if let Some(why) = bad_folder_name(stem) {
        return Err(AppError::new(format!("The file name {why}")));
    }
    for (what, v) in [("family name", &req.family), ("short noun", &req.short_noun), ("stat block id", &req.id), ("stat block name", &req.name)] {
        if v.trim().is_empty() {
            return Err(AppError::new(format!("The {what} is empty")));
        }
    }
    let root = dunce::canonicalize(project_root).map_err(|e| AppError::new(format!("Can't open the project: {e}")))?;
    let parent = dunce::canonicalize(&req.parent).map_err(|e| AppError::new(format!("Can't open {}: {e}", req.parent)))?;
    if !parent.starts_with(&root) {
        return Err(AppError::new(format!("{} isn't inside the project", parent.display())));
    }
    let file = parent.join(format!("{stem}.xml"));
    if file.exists() {
        return Err(AppError::new(format!("{} already exists", file.display())));
    }
    let schema = Path::new(schemas_dir).join(MONSTER_SCHEMA);
    let loc = pathdiff::diff_paths(&schema, &parent)
        .map(|p| path_str(&p).replace('\\', "/"))
        .unwrap_or_else(|| path_str(&schema).replace('\\', "/"));
    let text = shared::monster::skeleton(&loc, req.family.trim(), req.short_noun.trim(), req.id.trim(), req.name.trim());
    std::fs::write(&file, text).map_err(|e| AppError::new(format!("Can't write {}: {e}", file.display())))?;
    Ok(path_str(&file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_a_monster_file() {
        let t = tempfile::tempdir().unwrap();
        let schemas = t.path().join("schemas");
        std::fs::create_dir(&schemas).unwrap();
        let root = t.path().join("p");
        std::fs::create_dir_all(root.join("m")).unwrap();
        let req = NewMonsterFile {
            parent: path_str(&root.join("m")),
            file: "rats.xml".into(),
            family: "Rats".into(),
            short_noun: "rat".into(),
            id: "giant-rat".into(),
            name: "Giant Rat".into(),
        };
        let f = create_monster_file(&req, &path_str(&root), &path_str(&schemas)).unwrap();
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(text.contains(r#"xsi:noNamespaceSchemaLocation="../../schemas/monster-template-schema.xsd""#), "{text}");
        assert!(create_monster_file(&req, &path_str(&root), &path_str(&schemas)).is_err(), "exists");
    }

    #[test]
    fn creates_adventure_xml_with_relative_schema() {
        let t = tempfile::tempdir().unwrap();
        let schemas = t.path().join("schemas");
        std::fs::create_dir(&schemas).unwrap();
        let root = t.path().join("adventures");
        std::fs::create_dir(&root).unwrap();
        let req = NewSupplement {
            parent: path_str(&root),
            folder: "my-adventure".into(),
            kind: "campaign".into(),
            title: "Rats & Ruin".into(),
        };
        let file = create_supplement(&req, &path_str(&root), &path_str(&schemas)).unwrap();
        assert!(file.ends_with("adventure.xml"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains(r#"xsi:noNamespaceSchemaLocation="../../schemas/supplement-template-schema.xsd""#));
        assert!(text.contains("<headerTitle>Rats &amp; Ruin</headerTitle>"));
        assert!(text.contains(r#"type="campaign""#));
        // A second create of the same folder fails.
        assert!(create_supplement(&req, &path_str(&root), &path_str(&schemas)).is_err());
    }

    #[test]
    fn rules_type_writes_supplement_xml() {
        let t = tempfile::tempdir().unwrap();
        let req = NewSupplement { parent: path_str(t.path()), folder: "r".into(), kind: "rules".into(), title: "R".into() };
        let file = create_supplement(&req, &path_str(t.path()), &path_str(t.path())).unwrap();
        assert!(file.ends_with("supplement.xml"));
    }

    #[test]
    fn rejects_bad_names_and_outside_parents() {
        let t = tempfile::tempdir().unwrap();
        let root = t.path().join("p");
        std::fs::create_dir(&root).unwrap();
        let mut req = NewSupplement { parent: path_str(&root), folder: "a/b".into(), kind: "adventure".into(), title: "T".into() };
        assert!(create_supplement(&req, &path_str(&root), "").is_err());
        req.folder = "ok".into();
        req.parent = path_str(t.path());
        assert!(create_supplement(&req, &path_str(&root), "").unwrap_err().message.contains("isn't inside"));
    }

    #[test]
    fn lists_folders_first_and_hides_dot_folders() {
        let t = tempfile::tempdir().unwrap();
        std::fs::write(t.path().join("b.xml"), "").unwrap();
        std::fs::create_dir(t.path().join("z")).unwrap();
        std::fs::create_dir(t.path().join(".git")).unwrap();
        let names: Vec<_> = list_dir(&path_str(t.path())).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, vec!["z", "b.xml"]);
    }
}
