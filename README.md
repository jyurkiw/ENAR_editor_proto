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
- **Layout**: file tabs on top; vertical tabs on the left for File system / File structure;
  the main area shows the selected element's source, re-indented and highlighted (read-only).

## Layout

- `shared/` types and pure helpers (re-indent, XML tokenizer) used by both sides
- `src-tauri/` backend commands: project config, listing, creation, structure parse, validation
- `src/` Leptos frontend

Tests: `cargo test -p shared -p enigma-editor`; against the real repo:
`cargo test -p enigma-editor real -- --ignored`.
