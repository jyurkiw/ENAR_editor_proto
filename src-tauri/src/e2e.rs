//! Test seams for the Playwright suite in `e2e/`. They only do anything in a build with
//! `--features e2e`; in every other build the queue command refuses and the rest is inert.
//!
//! - **Folder picker**: `pick_folder` answers from a queue the test fills with
//!   `e2e_queue_pick` (an empty string answers Cancel) instead of opening the native dialog.
//! - **Confirm**: [`HOOK_JS`] is run in the page after each load. It defines
//!   `window.__enigmaE2E`, which `dialogs::confirm` asks instead of the native dialog.
//! - **Change log**: with `ENIGMA_E2E_LOG` set to a file, every `apply_change` appends a line
//!   of JSON: the change as the frontend sent it, and how it ended.

use shared::{AppError, ApplyResult, Change};

#[cfg(feature = "e2e")]
static PICKS: std::sync::Mutex<std::collections::VecDeque<String>> = std::sync::Mutex::new(std::collections::VecDeque::new());

/// Queues answers for the next folder pickers.
pub fn queue_picks(paths: Vec<String>) -> Result<(), AppError> {
    #[cfg(feature = "e2e")]
    {
        PICKS.lock().unwrap().extend(paths);
        Ok(())
    }
    #[cfg(not(feature = "e2e"))]
    {
        let _ = paths;
        Err(AppError::new("e2e_queue_pick needs a build with --features e2e"))
    }
}

/// The next queued picker answer: `Some(None)` is Cancel, `None` means nothing is queued
/// (so the native dialog opens).
pub fn next_pick() -> Option<Option<String>> {
    #[cfg(feature = "e2e")]
    {
        PICKS.lock().unwrap().pop_front().map(|p| (!p.is_empty()).then_some(p))
    }
    #[cfg(not(feature = "e2e"))]
    {
        None
    }
}

/// Appends the change and its outcome to `ENIGMA_E2E_LOG`.
pub fn log_change(change: &Change, result: &Result<ApplyResult, AppError>) {
    #[cfg(feature = "e2e")]
    {
        use std::io::Write;
        let Some(path) = std::env::var_os("ENIGMA_E2E_LOG") else { return };
        let outcome = match result {
            Ok(ApplyResult::Saved { .. }) => serde_json::json!({ "status": "Saved" }),
            Ok(ApplyResult::Invalid { problems }) => serde_json::json!({ "status": "Invalid", "problems": problems }),
            Err(e) => serde_json::json!({ "status": "Error", "message": e.message }),
        };
        let line = serde_json::json!({ "change": change, "result": outcome });
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
    #[cfg(not(feature = "e2e"))]
    {
        let _ = (change, result);
    }
}

/// Defines `window.__enigmaE2E`: `confirm(msg)` records the message in `asked` and answers
/// with the next of `answers` (OK when there are none left).
#[cfg_attr(not(feature = "e2e"), allow(dead_code))]
pub const HOOK_JS: &str = r#"
if (!window.__enigmaE2E) {
  window.__enigmaE2E = {
    asked: [],
    answers: [],
    confirm(msg) {
      this.asked.push(String(msg));
      return this.answers.length ? !!this.answers.shift() : true;
    },
  };
}
"#;
