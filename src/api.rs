//! Calls into the Tauri backend (window.__TAURI__.core.invoke, from withGlobalTauri).

use serde::de::DeserializeOwned;
use serde::Serialize;
use shared::{AppError, ApplyResult, Change, FsEntry, NewMagicItem, NewMonsterFile, NewSupplement, OpenProject, OpenedDoc, ProjectInfo};
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

pub async fn library() -> Result<Vec<shared::prose::LibEntry>, AppError> {
    invoke("library", &NoArgs {}).await
}

pub async fn open_file(path: &str) -> Result<OpenedDoc, AppError> {
    invoke("open_file", &PathArg { path }).await
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

/// The DMs Guild logo file a supplement's cover would use: `src` relative to the
/// supplement, or DMsGuildProductLogoLarge.png in its folder or any folder above.
pub async fn find_logo(doc_path: &str, src: Option<&str>) -> Option<String> {
    #[derive(Serialize)]
    struct A<'a> {
        doc_path: &'a str,
        src: Option<&'a str>,
    }
    invoke("find_logo", &A { doc_path, src }).await.ok().flatten()
}
