# Spellcheck and pill hints: dev plan

## Goal

Make prose fields point out two things while you type:

- **Misspellings**: a red wavy underline, with suggestions on right-click. Words can be
  added to a word list that is kept in the project, not in Windows.
- **Pillable text**: a blue wavy underline under every mention of something the document
  already defines, such as a reference entry's `name`, `displayName` or `shortName`. A
  right-click turns that mention, or every mention, into a pill.

Single source of truth is the point. Once a name is a pill, renaming the thing means
editing one attribute. Today that means a find-and-replace, which is risky.

## Decisions so far

| Question | Decision |
|---|---|
| What gets a blue underline | Only **the document's own reference entries**. Library and SRD things are pilled through Ctrl+K, or by selecting the text and using the context menu. Doing either adds a reference entry, so from then on that name is one of the document's own and gets underlined. |
| Which mentions | **Every mention**, not just the first one in a section. |
| Dismissing a match | **Per session**. Nothing is saved. |
| Word list | Lives **in the project**, in a version-controlled file. |
| Native spellcheck | Turned off in chip boxes (`spellcheck="false"`) so only one checker runs. Plain `<input>`s get no checking, because highlights can't be drawn inside form controls. |
| A matched form | Prints itself. If "Alec" matches the `shortName`, the pill is `value="shortName"`. |
| Variables | **"Turn into variable…"** makes a `<data>` entry. It can replace every mention of the value at once (the global replace below). |
| People, places and things | A separate **"Turn into a person, place or thing…"** item makes a `<noun>` entry. |
| Case | Case is ignored only when the entry's text is all lowercase ("the old mill"). A capitalised name must match exactly, so "Rose" doesn't match "rose". |
| Variable values | A variable is **referred to by its name, never by its value**. Its value is never underlined. Its name (the `id`) must be **letters only**: `toll`, `bridgeToll`, never `fire1`. The global replace on the value happens once, when the variable is made. |
| Global replace scope | The open file only, because reference entries belong to one file. |
| Word list | `enigma-words.txt` **next to `enigma-project.toml`**, so each project has its own list. |

## How it fits the existing code

- `prose_editor::choices` ([prose_editor.rs:229](../src/prose_editor.rs)) already collects
  every candidate the generator offers: each reference entry once per form, data entries,
  library items with their aliases, chapters and figures. This plan pulls the
  "document's own entries" part out so the generator and the matcher share it.
- `Act::Insert` and `Act::AddEntry` are what the generator does when you pick something.
  The context menu runs the same actions, so a pill made either way is the same XML.
- `open_gen` already uses the current selection as its query. "Make a pill…" on a selection
  is just `open_gen()`.
- `statblock::is_prose` decides which elements are prose. The global replace touches only
  those elements.
- Library and SRD names are already indexed by `library.rs`. The spellchecker treats them as
  known words, even though they don't get blue underlines.
- `Change::Batch` already applies several changes in order and validates the result as a
  whole. That's what the global replace needs.

## Phases

### Phase 0: spike (about 2 hours)

These two risks decide whether the approach works, so check them before building anything:

1. **The CSS Custom Highlight API from Rust.** `web-sys` may not bind `Highlight` or
   `CSS.highlights`. If it doesn't, write a small `#[wasm_bindgen] extern` block, or call it
   through `js_sys::Reflect`. Prove it by drawing a `::highlight(pillable)` underline over a
   `Range` in a chip box, in the dev app.
2. **spellbook in wasm.** Build `spellbook` for `wasm32-unknown-unknown` with an en_US
   Hunspell dictionary. Measure the wasm size, the load and parse time, and the time to check
   one word. Also check both licenses: spellbook is MPL-2.0, and the dictionary should come
   from SCOWL/wooorm, whose terms are permissive.

If the Highlight API doesn't work, the fallback is to wrap underlined text in spans that the
serializer ignores. That's much more invasive, so phase 0 has to pass first.

**Results (2026-11-14): passed.** The spike code is in the working tree: `src/spelling.rs`, a
hook in `ChipEditor`, `assets/dict/`, and a `::highlight(spelling)` style.

- **Highlights:** they work in WebView2. Underlines drew under misspelled words without
  touching the box's DOM, and chips and caret were unaffected. Two notes:
  - `web-sys` has `Highlight` only behind `--cfg=web_sys_unstable_apis`. A small
    `wasm_bindgen` extern block avoids that; `CSS.highlights` is read with `Reflect`, since
    it's a property, not a function.
  - **`CSS.highlights.highlightsFromPoint(x, y)` is available** and returns the ranges under
    the pointer. Right-click hit-testing for phase 3 is free.
- **spellbook in wasm:** it builds and runs.

  | Measure | Release build | Debug build |
  |---|---|---|
  | wasm size added | about 330 KB (4.98 MB to 5.31 MB) | |
  | dictionary download | 555 KB, a local asset, about 10 ms | |
  | dictionary parse | 12 ms | 93 to 133 ms |
  | `check` one word | under 0.1 ms | |
  | `suggest` one word | 5 to 19 ms | 40 to 80 ms |
  | re-checking a paragraph on input | | about 2 ms |

  Checking on every keystroke is cheap enough to need no debounce for checking alone.
  Suggestions are only computed on right-click, so their cost doesn't matter.
- **Dictionary:** SCOWL en_US 2020.12.07, from the `dictionary-en@4.0.0` package, under an
  MIT and BSD license kept in `assets/dict/LICENSE-en_US.txt`. `.gitattributes` keeps
  `assets/dict/*` byte-for-byte, so git can't rewrite its line endings. en_CA and en_GB
  versions exist if you'd rather have one of those.
- **Lessons for the later phases:**
  - A box drawn before the dictionary loads is re-checked when it arrives, through a `ready()`
    signal.
  - A box must remove its ranges from the shared highlight when it's unmounted. Without that,
    the first test had piled up about 1,450 dead ranges after 25 sections. `on_cleanup`
    fixes it.
  - Replacing a box's HTML kills its ranges, so it's re-checked right after each redraw.
  - Before phase 5's known words exist, the checker flags every proper noun. A survey of 25
    test-adventure sections flagged 43 distinct words, all names (Hillsfort, Yeenoghu,
    Krugga, Corvin…) or game terms (NPC, DM, handaxe, spellbook, brewhouse). That's the case
    for the known words: entry names, library names and the project word list.
  - Spike-only code to remove later: the `__spellProbe` console hook.

### Phase 1: matching, in `shared::pills` (about half a day, unit tested)

This is pure Rust with no UI.

- `Candidate { kind, id, form, text, ref_xml }`, one per non-empty form of each reference
  entry: monsters, nouns, spells and items. Data entries are not candidates, because a
  variable is referred to by its name and its value is never underlined.
- Case: ignored only when the candidate's text is all lowercase.
- `index(root: &El) -> Vec<Candidate>`, built from the file's `<reference>`.
- `find(text: &str, cands) -> Vec<Match { start, end, cands: Vec<usize> }>`:
  - matches whole words only;
  - prefers the longest match, so "Alecia Something" wins over "Alecia";
  - lets a match span several words;
  - treats `Alec's` as a match on `Alec` followed by `'s`;
  - puts every candidate that shares a text into one match, so the menu can list them all.
- Tests cover overlapping names, possessives, punctuation, Unicode, a name inside a longer
  word (no match), and one text that two entries share.

**Results (2026-11-14): done.** `shared::pills`, with `candidates(ctx, palette)` and
`find(text, cands)`, as planned.

- **Unit tests (8):** cover candidates per form, the longest match winning, whole words only,
  possessives and either apostrophe, the case rule, two entries sharing a name, Unicode and
  byte offsets, and no partial or overlapping matches.
- **Real-repo survey:** an ignored test, `pill_hints_in_real_supplements`, runs over every
  supplement in the data repo. It found 95 mentions in 5 files, and checking a whole
  adventure took about 7 ms in a debug build. Every hit is a real mention of the file's own
  entries, for example:
  - Garrick ×21, Krugga ×13 and Ashvyrrix ×15 in the compiler test;
  - Carl, Maya and Aria in *Merchants on Fyier*.
- **One case to watch:** a monster entry that only points at a stat block takes its `name`
  from the stat block. In *Merchants on Fyier*, `eamon`'s `name` is "Bandit", so a
  capitalised "Bandit" in that file is offered as Eamon's name. The pill would still print
  "Bandit", so it isn't wrong, but it could surprise you. Phase 3's menu shows the entry's
  name next to the form ("Eamon · name") so it's clear what you'd be linking.

### Phase 2: blue underlines in the chip box (about half a day)

- After each edit, debounced to about 150 ms, walk the chip box's text nodes, skipping
  `.chip-tag` spans. Join the text with an offset map, run `pills::find`, and turn each match
  back into a DOM `Range`. A match can't cross a chip.
- Put every range into one `Highlight` registered as `pillable`, and style it with a CSS
  wavy underline in blue.
- Rebuild the index whenever the document's reference entries change, so a new entry lights
  up every one of its mentions straight away.
- Keep session dismissals in the app context as a set of
  `(file, entry id, matched text)`. Dismissed matches aren't added.

**Results (2026-11-14): done.** `src/hints.rs` holds the shared text walk and both
highlights. `src/spelling.rs` is now just the dictionary.

- **Which text is checked:** a box's text is split into runs. A run breaks at chips and at
  line breaks (`br`, `div`, `p`), but runs through bold and italics. So "Krugga
  <b>Redhide</b>" is one mention, while a name can never cross a chip.
- **Per box:** each box keeps its own `BoxMarks`: its misspelling ranges, plus a `PillMark`
  for each mention (its range, its text and its candidates, for phase 3's menu). A box
  re-marks itself when:
  - it's redrawn or edited;
  - the dictionary loads;
  - the file's candidates change (`ProseEnv::pills`, a memo, so an edit that doesn't change
    any names doesn't trigger it);
  - something is dismissed.

  It clears its marks when it's unmounted.
- **Misspellings vs mentions:** a word inside a mention is never marked misspelled, so "Corvin
  Thistlewood" is blue only, not red.
- **Dismissals:** `hints::dismiss(file, text)` keeps them for the session, keyed by the
  normalized text in that file. It's not called yet; phase 3's "Not a pill" item will call it.
- **Checked in the dev app** on "A Day of Mourning" in the compiler test:
  - the two existing mentions of "Garrick" get blue underlines, but the "Garrick Moss" chip
    doesn't;
  - typing "Then Krugga and Corvin Thistlewood met Garrick." underlines all three names;
  - re-marking on each keystroke takes about 2 ms in a debug build;
  - a name split by bold is matched;
  - after Revert, only the original mentions remain.

### Phase 3: the context menu (about a day)

Right-clicking in a chip box opens our own menu instead of WebView2's. What it lists depends
on where you click:

| Where | Items |
|---|---|
| On a blue underline | **Make pill: Alecia Something (short name)**, one row per candidate · **Pill every mention in this file (N)…** · **Not a pill (this session)** |
| With text selected | **Make a pill…** (the generator, with the selection as its query, the same as Ctrl+K) · **Turn into variable…** · **Turn into a person, place or thing…** |
| On a red underline | Up to five suggestions · **Add to project words** · **Ignore (this session)** |
| Always | Cut · Copy · Paste |

- **Turn into variable…** asks for the variable's **name**, which becomes its `id`. The
  name must be letters only (`bridgeToll`, not `toll2`), and the dialog won't accept
  anything else, or a name another entry already uses. The selected text becomes its
  `value`. It adds `<data id="bridgeToll" value="5 gp"/>`, replaces the selection with
  `<dataRef ref="bridgeToll" value="value"/>`, and then offers the global replace for every
  other "5 gp" in the file. After that, the value is only ever reached by name.
- The Ctrl+K generator lists a data entry under its **name** (the id), with its value as the
  subtitle. Today it lists it under its value.
- **Turn into a person, place or thing…** does the same with a `<noun>` entry: `name`,
  optionally `shortName`, and `kind`. A named NPC that has no stat block belongs here.
- After any action that adds an entry, including the existing Ctrl+K "AddEntry" path, the
  menu says "N more mentions in this file" and offers the global replace.
- **Risk: Paste.** Once WebView2's own menu is replaced, the page has to paste for itself.
  `document.execCommand('paste')` is blocked, and `navigator.clipboard.readText` may prompt
  for permission. Plan on `tauri-plugin-clipboard-manager` for reading the clipboard. Cut and
  Copy can use `execCommand`.
- Reuse the menu and backdrop styles from `files.rs::ContextMenu`.

**Results (2026-11-14): done**, except that offering the global replace after an entry is
added moves to phase 4 along with the replace itself.

- **The menu:** right-clicking a prose box opens our own menu. It uses the existing `.menu`
  styles and is kept on screen. It finds what was clicked by testing the box's own marks
  against the point. It shows:
  - **On a blue underline:** "Make pill: {entry} · {form}" for each candidate, with the
    entry's display name, so "Eamon", not "Bandit". Then "Not a pill (this session)".
  - **On a red underline:** up to five suggestions, or "No suggestions", then
    "Ignore (this session)". "Add to project words" comes in phase 5.
  - **Always:** "Make a pill…" (the Ctrl+K generator) and Cut, Copy and Paste.
  - **With a chip-free selection:** "Turn into variable…" and "Turn into a person, place or
    thing…", each only in files whose prose has that ref (variables wherever `dataRef`
    exists, nouns in supplements).
- **Turn into variable…:**
  - suggests a camelCase name from the value ("old kennel" becomes `oldKennel`);
  - refuses anything but letters, or a name already in use;
  - adds `<data id value>` and puts a `dataRef` in place of the selection.
- **Turn into a person, place or thing…:** has Name (the selection to start with), Short name
  and Kind fields.
  - The id is a fresh slug of the name.
  - The pill prints the short name if the selection is the short name, and the name otherwise.
  - The form warns when the selection would print differently ("The selection will print as
    'The Old Kennel'").
  - Spaces around the selection stay outside the pill.
- **Paste** reads the clipboard through `tauri-plugin-clipboard-manager` (permission
  `clipboard-manager:allow-read-text`) and pastes **plain text**. The WebView's own clipboard
  API needs a permission prompt. Ctrl+V still does a native paste, which keeps any copied
  chips. **Cut and Copy** use `execCommand`.
- **Fixed in passing:** a paragraph row removed while it had focus panicked when its
  `focusout` fired after its signals were gone (`block_form.rs`). It now checks with
  `try_get_untracked`.
- **Checked in the dev app** on the compiler test, which was restored from git afterwards:
  - **Make pill:** on "Garrick" it inserted `<nounRef ref="garrick" value="shortName"/>`.
  - **Yeenoghu:** offered "Enough", and Ignore removed its red underline.
  - **Not a pill:** on "Garrick", it removed the underline from every "Garrick" in the file.
  - **Variable:** "old kennel" suggested `oldKennel`, refused `kennel2` (letters only), and
    with `kennel` it added `<data id="kennel" value="old kennel"/>` plus a `dataRef`.
  - **Noun:** " kennel " (with its spaces), named "The Old Kennel" with short name "kennel"
    and kind place, added the noun and printed `value="shortName"` between the kept spaces.
    Once the index refreshed, the chip read "kennel" and the file's other "kennel"s turned
    blue.
  - **Paste:** inserted the clipboard's text.

### Phase 4: the global replace (about a day)

This is the part that has to be safe.

- In `shared::pills`, `occurrences(source, cands) -> Vec<Occurrence { path, start, end, text, context }>`
  walks every prose element in the **whole file**, not just the open field. It skips
  attributes, existing inline tags and non-prose elements.
- A **preview dialog** lists every occurrence with about 40 characters of context on each
  side, each with a checkbox, all ticked to start with. It has "Select all" and "Select none",
  and its button says how many will be replaced ("Replace 12").
- The ticked occurrences become one `Change::Batch` of prose edits. It is planned and
  validated as a whole, the same as every other change, so either all of it lands or none of
  it does. It's also one undo step.
- Text that isn't in a pillable context (a `<title>`, a table header) stays out of the list,
  because `is_prose` excludes it.
- Tests:
  - shared unit tests for possessives, mixed inline markup around a match, and a match next
    to an existing pill;
  - an ignored real-repo test that adds a noun to a copy of the test adventure, pills every
    mention, and checks that the file is valid and that the count matches.

### Phase 5: spellcheck (about a day)

- Ship the dictionary (`en_US.aff` and `.dic`) as Trunk assets and fetch it once at startup.
  Don't use `include_str`, so the wasm stays small. Build the `spellbook::Dictionary` once and
  keep it in the app context.
- **Project word list**: `enigma-words.txt` next to `enigma-project.toml` (the
  project's root, `ProjectInfo::root`), one word per
  line, with `#` comments. It is read when the project opens and appended to through a new
  Tauri command, `add_project_word`, which keeps the file sorted and free of duplicates.
  Because it's a plain text file, it shows up clearly in git diffs.
- Known words come from:
  - the dictionary;
  - the project word list;
  - every word in the document's reference entries' forms;
  - every library and SRD name.

  The last two mean monster and NPC names never get a red underline, without anyone having
  to maintain them.
- The same text walk as phase 2 adds a second `Highlight`, `spelling`, with a red wavy
  underline. A word inside a blue match is never marked red.
- The checker skips numbers, dice notation (`2d6+3`), words in all caps, and anything
  inside chips.

### Phase 6: verification (about half a day)

- `cargo test --workspace`, the ignored real-repo tests, and
  `cargo check --target wasm32-unknown-unknown`.
- The dev app, driven over CDP:
  1. Type a noun's short name and see a blue underline.
  2. Make it a pill from the right-click menu.
  3. Run "Pill every mention" and check the preview count.
  4. Apply, then rename the noun and see every mention follow.
  5. Misspell a word, pick a suggestion, add a word to the list, and check that
     `enigma-words.txt` changed.
- `.\build-release.ps1 -NoBundle`.

**Total: about 4 to 5 days.** Phases 1 to 4 (pills) can ship before phase 5 (spelling) if
you want the single-source-of-truth part first.
