//! Calls into the Tauri backend (window.__TAURI__.core.invoke, from withGlobalTauri).

use serde::de::DeserializeOwned;
use serde::Serialize;
use shared::{AppError, ApplyResult, Change, FsEntry, NewMagicItem, NewMonsterFile, NewSpell, NewSupplement, OpenProject, OpenedDoc, ProjectInfo};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    async fn tauri_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;

    /// A URL the webview can load a local file from (Tauri's asset protocol).
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = convertFileSrc)]
    pub fn convert_file_src(path: &str) -> String;
}

async fn invoke<A: Serialize, R: DeserializeOwned>(cmd: &str, args: &A) -> Result<R, AppError> {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args.serialize(&ser).map_err(|e| AppError::new(format!("{cmd}: bad arguments: {e}")))?;
    match tauri_invoke(cmd, args).await {
        Ok(v) => serde_wasm_bindgen::from_value(v).map_err(|e| AppError::new(format!("{cmd}: bad reply: {e}"))),
        Err(e) => Err(serde_wasm_bindgen::from_value::<AppError>(e.clone())
            .unwrap_or_else(|_| AppError::new(e.as_string().unwrap_or_else(|| format!("{cmd} failed: {e:?}"))))),
    }
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
struct PathArg<'a> {
    path: &'a str,
}

/// The clipboard's text (for the prose menu's Paste).
pub async fn clipboard_text() -> Result<String, AppError> {
    invoke("plugin:clipboard-manager|read_text", &NoArgs {}).await
}

pub async fn last_project() -> Option<String> {
    invoke("last_project", &NoArgs {}).await.ok().flatten()
}

pub async fn open_project(path: &str) -> Result<OpenProject, AppError> {
    invoke("open_project", &PathArg { path }).await
}

pub async fn configure_project(root: &str, schemas_dir: &str, compiler_dir: &str) -> Result<ProjectInfo, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        root: &'a str,
        schemas_dir: &'a str,
        compiler_dir: &'a str,
    }
    invoke("configure_project", &A { root, schemas_dir, compiler_dir }).await
}

pub async fn list_dir(path: &str) -> Result<Vec<FsEntry>, AppError> {
    invoke("list_dir", &PathArg { path }).await
}

pub async fn create_supplement(req: &NewSupplement) -> Result<String, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        req: &'a NewSupplement,
    }
    invoke("create_supplement", &A { req }).await
}

pub async fn create_monster_file(req: &NewMonsterFile) -> Result<String, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        req: &'a NewMonsterFile,
    }
    invoke("create_monster_file", &A { req }).await
}

pub async fn create_magic_item_file(req: &NewMagicItem) -> Result<String, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        req: &'a NewMagicItem,
    }
    invoke("create_magic_item_file", &A { req }).await
}

/// The project word list.
pub async fn project_words() -> Result<Vec<String>, AppError> {
    invoke("project_words", &NoArgs {}).await
}

/// Adds a word to the project word list; the new list.
pub async fn add_project_word(word: &str) -> Result<Vec<String>, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        word: &'a str,
    }
    invoke("add_project_word", &A { word }).await
}

pub async fn library() -> Result<Vec<shared::prose::LibEntry>, AppError> {
    invoke("library", &NoArgs {}).await
}

pub async fn create_spell_file(req: &NewSpell) -> Result<String, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        req: &'a NewSpell,
    }
    invoke("create_spell_file", &A { req }).await
}

pub async fn open_file(path: &str) -> Result<OpenedDoc, AppError> {
    invoke("open_file", &PathArg { path }).await
}

/// The file's text as it is on disk, unchecked.
pub async fn read_text(path: &str) -> Result<String, AppError> {
    invoke("read_text", &PathArg { path }).await
}

pub async fn pick_folder(title: &str, start: Option<&str>) -> Option<String> {
    #[derive(Serialize)]
    struct A<'a> {
        title: &'a str,
        start: Option<&'a str>,
    }
    invoke("pick_folder", &A { title, start }).await.ok().flatten()
}

pub async fn apply_change(path: &str, expected: &str, change: &Change) -> Result<ApplyResult, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        path: &'a str,
        expected: &'a str,
        change: &'a Change,
    }
    invoke("apply_change", &A { path, expected, change }).await
}

/// What each `<reference>` entry of the file prints (read with the compiler's loader).
pub async fn ref_index(path: &str) -> Result<shared::inline::RefIndex, AppError> {
    invoke("ref_index", &PathArg { path }).await
}

/// The stat block `monster` (its id) of the monster file at `path`, printed by the
/// compiler and LaTeX: a PNG data URL per page.
pub async fn preview_statblock(path: &str, monster: &str) -> Result<shared::Preview, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        path: &'a str,
        monster: &'a str,
    }
    invoke("preview_statblock", &A { path, monster }).await
}

/// The parts, lairs and stat blocks of the monster file `file` (relative to the
/// supplement at `doc_path`).
pub async fn monster_outline(doc_path: &str, file: &str) -> Result<shared::family::Outline, AppError> {
    #[derive(Serialize)]
    struct A<'a> {
        doc_path: &'a str,
        file: &'a str,
    }
    invoke("monster_outline", &A { doc_path, file }).await
}

/// The DMs Guild logo file a supplement's cover would use: `src` relative to the
/// supplement, or DMsGuildProductLogoLarge.png in its folder or any folder above.
/// A cover logo's file: its src, or `file` (default the DMs Guild logo) found upward.
pub async fn find_logo(doc_path: &str, src: Option<&str>, file: Option<&str>) -> Option<String> {
    #[derive(Serialize)]
    struct A<'a> {
        doc_path: &'a str,
        src: Option<&'a str>,
        file: Option<&'a str>,
    }
    invoke("find_logo", &A { doc_path, src, file }).await.ok().flatten()
}
