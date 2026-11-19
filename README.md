# Enigma Narrative editor (prototype)

A Tauri 2 + Leptos (Rust/WASM) UI for writing Enigma Narrative supplement XML
(`supplement-template-schema.xsd`), compatible with the `encompile` compiler.

## Run

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --version "~0.21" --locked   # 0.22 needs rustc 1.96+
cargo install tauri-cli --version "^2" --locked
cargo tauri dev
```

`cargo tauri dev` is for development only: its executable (`target\debug\enigma-editor.exe`)
loads the UI from trunk's dev server on localhost:1420, so it shows nothing when run on its own.

## Build a standalone app

```powershell
.\build-release.ps1            # the .exe and an installer
.\build-release.ps1 -NoBundle  # just the .exe (faster)
```

This runs `cargo tauri build`, which compiles the UI with `trunk build --release` and embeds
it in a release executable. The script copies the results to `release\`:

- `release\Enigma Narrative Editor.exe`: a single file. Double-click it, pin it, or copy it anywhere.
- `release\Enigma Narrative Editor_<version>_x64-setup.exe`: an NSIS installer with a Start menu
  entry and an uninstaller.

At run time the app needs WebView2 (part of Windows 10/11) and, for validation, `uv`. A
double-clicked app looks for `uv` on PATH, then in `%USERPROFILE%\.local\bin`,
`%USERPROFILE%\.cargo\bin` and `%LOCALAPPDATA%\Programs\uv`. Its settings and validation venv live
in `%APPDATA%\com.enigmanarrative.editor` and `%LOCALAPPDATA%\com.enigmanarrative.editor`, shared
with dev builds.

## How it works

- **Project**: any folder. `enigma-project.toml` at its root records the schemas folder and
  the compiler folder (relative paths). Opening an unconfigured folder asks for both,
  suggesting `schemas/` and `compiler/` found by walking up.
- **New supplement**: makes `<folder>/adventure.xml` (adventure, campaign, anthology) or
  `<folder>/supplement.xml` (every other type), a minimal schema-valid skeleton.
- **Opening a file** (right-click > Open, or double-click): the root must be `<supplement>`
  naming `supplement-template-schema.xsd`, then it's XSD-validated with lxml via
  `uv run --project <compiler>`. The Python env lives in the app cache folder
  (`UV_PROJECT_ENVIRONMENT`), so the compiler's own `.venv` is untouched. Failures aren't opened.
- **Layout**: file tabs on top; vertical tabs on the left for File system / File structure /
  Preview; the main area shows the selected element's source, re-indented and highlighted
  (read-only).
- **Preview** (monster files): the stat block the selection is in (else the file's first), as
  the book prints it, one column wide. The compiler builds it (`python -m encompile.preview`,
  through the same uv environment as the validator), pdflatex sets it, and it's cropped to the
  block. While the tab is showing, every applied change prints it again; the last picture stays
  up, faded, until the new one is ready. Needs pdflatex and the dndbook class.

## Layout

- `shared/` types and pure helpers (re-indent, XML tokenizer) used by both sides
- `src-tauri/` backend commands: project config, listing, creation, structure parse, validation
- `src/` Leptos frontend

Tests: `cargo test -p shared -p enigma-editor`; against the real repo:
`cargo test -p enigma-editor real -- --ignored`. End-to-end tests (Playwright driving the
built app): `cd e2e; uv run pytest`, see [e2e/README.md](e2e/README.md).
