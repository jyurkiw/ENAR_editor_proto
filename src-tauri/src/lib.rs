//! Tauri commands for the Enigma Narrative supplement editor prototype.
//!
//! The open project lives here, in [`AppState`]; the frontend asks for it with
//! `current_project` and changes it with `open_project` / `configure_project`.

mod fsops;
mod library;
mod project;
mod validate;

use std::path::PathBuf;
use std::sync::Mutex;

use shared::{AppError, ApplyResult, Change, FsEntry, NewMagicItem, NewMonsterFile, NewSpell, NewSupplement, OpenProject, OpenedDoc, ProjectInfo};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use validate::Validator;

struct AppState {
    project: Mutex<Option<ProjectInfo>>,
    validator: Validator,
    /// Holds the last project's path, so the app reopens it.
    last_project_file: PathBuf,
}

impl AppState {
    fn project(&self) -> Result<ProjectInfo, AppError> {
        self.project.lock().unwrap().clone().ok_or_else(|| AppError::new("No project is open"))
    }

    fn set_project(&self, p: &ProjectInfo) {
        *self.project.lock().unwrap() = Some(p.clone());
        // The name index: built in the background, kept current by a file watcher.
        library::start(p);
        let _ = std::fs::write(&self.last_project_file, &p.root);
    }
}

/// Runs blocking work (file I/O, the validator) off the async runtime's worker threads.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, AppError> + Send + 'static,
) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::new(format!("internal error: {e}")))?
}

/// The project opened last time, if its folder still exists.
#[tauri::command(rename_all = "snake_case")]
fn last_project(state: State<'_, AppState>) -> Option<String> {
    let p = std::fs::read_to_string(&state.last_project_file).ok()?;
    let p = p.trim().to_string();
    std::path::Path::new(&p).is_dir().then_some(p)
}

#[tauri::command(rename_all = "snake_case")]
fn current_project(state: State<'_, AppState>) -> Option<ProjectInfo> {
    state.project.lock().unwrap().clone()
}

#[tauri::command(rename_all = "snake_case")]
fn open_project(path: String, state: State<'_, AppState>) -> Result<OpenProject, AppError> {
    let r = project::open(&path)?;
    if let OpenProject::Ready { project } = &r {
        state.set_project(project);
    }
    Ok(r)
}

#[tauri::command(rename_all = "snake_case")]
fn configure_project(
    root: String,
    schemas_dir: String,
    compiler_dir: String,
    state: State<'_, AppState>,
) -> Result<ProjectInfo, AppError> {
    let p = project::configure(&root, &schemas_dir, &compiler_dir)?;
    state.set_project(&p);
    Ok(p)
}

#[tauri::command(rename_all = "snake_case")]
async fn list_dir(path: String) -> Result<Vec<FsEntry>, AppError> {
    blocking(move || fsops::list_dir(&path)).await
}

#[tauri::command(rename_all = "snake_case")]
fn create_supplement(req: NewSupplement, state: State<'_, AppState>) -> Result<String, AppError> {
    let p = state.project()?;
    fsops::create_supplement(&req, &p.root, &p.schemas_dir)
}

#[tauri::command(rename_all = "snake_case")]
fn create_monster_file(req: NewMonsterFile, state: State<'_, AppState>) -> Result<String, AppError> {
    let p = state.project()?;
    fsops::create_monster_file(&req, &p.root, &p.schemas_dir)
}

#[tauri::command(rename_all = "snake_case")]
fn create_magic_item_file(req: NewMagicItem, state: State<'_, AppState>) -> Result<String, AppError> {
    let p = state.project()?;
    fsops::create_magic_item_file(&req, &p.root, &p.schemas_dir)
}

#[tauri::command(rename_all = "snake_case")]
fn create_spell_file(req: NewSpell, state: State<'_, AppState>) -> Result<String, AppError> {
    let p = state.project()?;
    fsops::create_spell_file(&req, &p.root, &p.schemas_dir)
}

#[tauri::command(rename_all = "snake_case")]
async fn open_file(path: String, app: AppHandle) -> Result<OpenedDoc, AppError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let p = state.project()?;
        validate::open_file(&path, &p, &state.validator)
    })
    .await
}

/// Applies a form's change to the file. `expected` is the text the editor has open.
#[tauri::command(rename_all = "snake_case")]
async fn apply_change(path: String, expected: String, change: Change, app: AppHandle) -> Result<ApplyResult, AppError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let p = state.project()?;
        validate::apply_edit(&path, &expected, &p, &state.validator, |src| change.plan(src))
    })
    .await
}

/// The spells, monster stat blocks and magic items refs can be made to.
#[tauri::command(rename_all = "snake_case")]
async fn library(app: AppHandle) -> Result<Vec<shared::prose::LibEntry>, AppError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let p = state.project()?;
        Ok(library::library(&p))
    })
    .await
}

/// What each <reference> entry of the file prints, for the paragraph preview.
#[tauri::command(rename_all = "snake_case")]
async fn ref_index(path: String, app: AppHandle) -> Result<shared::inline::RefIndex, AppError> {
    blocking(move || {
        let state = app.state::<AppState>();
        let p = state.project()?;
        validate::ref_index(&path, &p, &state.validator)
    })
    .await
}

/// The DMs Guild logo a cover would print, as the compiler finds it.
#[tauri::command(rename_all = "snake_case")]
fn find_logo(doc_path: String, src: Option<String>) -> Option<String> {
    let dir = std::path::Path::new(&doc_path).parent()?;
    if let Some(src) = src.filter(|s| !s.trim().is_empty()) {
        let p = dir.join(src);
        return p.is_file().then(|| project::path_str(&p));
    }
    dir.ancestors()
        .map(|d| d.join("DMsGuildProductLogoLarge.png"))
        .find(|p| p.is_file())
        .map(|p| project::path_str(&p))
}

/// The monster file a supplement's `<reference><monster file>` names, as a
/// `<monsterEntry>` sees it: its parts, lairs and stat blocks with ids.
#[tauri::command(rename_all = "snake_case")]
fn monster_outline(doc_path: String, file: String) -> Result<shared::family::Outline, AppError> {
    let p = std::path::Path::new(&doc_path).parent().map(|d| d.join(&file)).ok_or_else(|| AppError::new("The supplement has no folder."))?;
    let text = std::fs::read_to_string(&p).map_err(|e| AppError::new(format!("Can't read {file}: {e}")))?;
    shared::family::outline(&text).ok_or_else(|| AppError::new(format!("{file} isn't a monster file.")))
}

/// A native folder picker. None when cancelled.
#[tauri::command(rename_all = "snake_case")]
async fn pick_folder(title: String, start: Option<String>, app: AppHandle) -> Option<String> {
    let mut dialog = app.dialog().file().set_title(title);
    if let Some(s) = start.filter(|s| std::path::Path::new(s).is_dir()) {
        dialog = dialog.set_directory(s);
    }
    dialog.blocking_pick_folder().and_then(|p| p.into_path().ok()).map(|p| project::path_str(&p))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache = app.path().app_cache_dir()?;
            let config = app.path().app_config_dir()?;
            std::fs::create_dir_all(&config)?;
            app.manage(AppState {
                project: Mutex::new(None),
                validator: Validator::install(&cache)?,
                last_project_file: config.join("last-project.txt"),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            last_project,
            current_project,
            open_project,
            configure_project,
            list_dir,
            create_supplement,
            create_monster_file,
            create_magic_item_file,
            create_spell_file,
            open_file,
            apply_change,
            ref_index,
            library,
            find_logo,
            monster_outline,
            pick_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Enigma editor");
}
