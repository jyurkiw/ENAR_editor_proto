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
