# End-to-end tests

Playwright drives the real editor: its window, the Rust backend, the uv validator and the
compiler. The plan, and the Grand Forks supplement the tests build, are in
[docs/e2e-playwright-plan.md](../docs/e2e-playwright-plan.md).

```powershell
cd e2e
uv run pytest               # builds the app with --features e2e, then runs the tests
uv run pytest --no-build    # reuses the last e2e build
```

You need `uv`, the Tauri CLI and trunk (as for `cargo tauri dev`), and an enigma-narrative
checkout. The checkout defaults to `E:\Data\enigma-narrative`; change it with
`--data-repo PATH` or `ENIGMA_DATA_REPO`. You don't need to install Playwright's own
browsers, because it attaches to the app's WebView2.

## What a run does

- **Builds** `cargo tauri build --debug --no-bundle --features e2e` into `target/e2e/`.
  That's a separate target folder, so a running `cargo tauri dev` doesn't get in the way.
- **Launches** the exe with WebView2's debugging port open and attaches over CDP.
- **Gives the run its own folder**, `e2e/out/<timestamp>/` (gitignored), which holds:
  - the app's settings (`ENIGMA_CONFIG_DIR`), so your last project is untouched
  - the WebView2 profile
  - `app.log`
  - `changes.jsonl`, every apply the tests made
- **Writes the test project** in `<data repo>/adventures/e2e/grand-forks/`, emptied at the
  start of each run. `adventures/e2e/` has a `.gitignore` of its own, so git never sees
  it.

## The suites

A run goes in this order:

1. `story/`: steps 01–12 write Grand Forks. Step 12 compares every file with
   `golden/grand-forks/`, then compiles the PDF into `out/<run>/build/`.
2. `edges/`: independent edge cases (refused applies, Revert, Cancel on each delete, bad
   files, the Source view, tabs, Ctrl+K's list). Each module starts from the story's last
   snapshot.
3. `coverage/`: fails when an edit variant (read from `shared/src`), a section block kind,
   or a feature in `features.toml` went unused. It runs only on a whole run.

```powershell
uv run pytest story --update-golden    # rewrite golden/ from this run, then review the diff
uv run pytest edges --no-build         # just the edge cases, on the newest story snapshot
```

The build in step 12 runs the compiler the way the app does, with `uv run` in the app's own
Python environment (`%LOCALAPPDATA%\com.enigmanarrative.editor\venv`).

## Writing tests

The primary suite is `story/`: ordered steps (`test_01_project.py`, `test_02_new_files.py`
and so on) that build Grand Forks one piece at a time. After a step passes, the run saves the
project to `out/<run>/snapshots/NN`.

```powershell
uv run python tools/launch.py --build          # an app that stays open (Ctrl+C closes it)
uv run pytest story -k 05 --attach             # run steps against it, and watch
uv run pytest story --no-build --from-snapshot 04   # steps 05 on, in a fresh app, on step 04's files
uv run python tools/peek.py "document.title"   # evaluate JavaScript in the live app
```

`pages/ui.py` holds the page objects, so tests read as steps (`ui.select('chapter',
name='tollside')`, `ui.field('Title').fill(...)`, `ui.apply()`).

- **Waiting for an apply:** wrap the click that makes the edit in `ui.click_applies(...)`
  (`ui.apply()` does this). It waits for the backend to log the finished apply, then
  checks it saved and that the Problems panel is empty. Don't use fixed waits.
- **Checking results:** check the file on disk with `ui.xml(rel)` and `ui.one(rel, xpath)`,
  not the Source view's text.

## The test seams (`--features e2e`, see `src-tauri/src/e2e.rs`)

| Seam | In the tests |
|---|---|
| Folder picker | `app.queue_pick(path, ...)` answers the next pickers. `None` answers Cancel. |
| Confirm dialogs | `app.answer_confirms(False, ...)` answers the next ones. With no answers queued, they answer OK. `app.confirms_asked()` returns the messages shown. |
| Change log | `app.changes()`: each apply's change and how it ended (`Saved`, `Invalid`, `Error`). |
| Settings folder | `ENIGMA_CONFIG_DIR`. This one works in every build. |

## Placeholder art

`art/make_slop_art.py OUT_DIR` writes the five pictures the supplement uses. Each is
stamped SLOP ART corner to corner.
