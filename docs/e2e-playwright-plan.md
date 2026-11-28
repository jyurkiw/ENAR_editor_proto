# End-to-end tests with Playwright: dev plan

## Goal

Drive the real editor (Tauri window, WebView2, Rust backend, uv validator, compiler) from
Playwright and touch every feature the editor has. The primary suite is one ordered story:
it starts from an empty folder and, using only the UI, writes a short **settlement**
supplement called **Grand Forks**. At the end the compiler builds it into a PDF.

What the suite leaves behind:

- A small but real supplement, kept by every run so a person can read it and see that the
  editor wrote it.
- The XML compared against a checked-in golden copy, so any change to what the editor
  writes shows up as a diff.
- A coverage report showing that every edit the backend accepts, and every UI feature on
  the list, ran at least once.

The supplement doesn't need to be good or complete. It only needs to use every feature
once, in a place that makes sense.

## Grand Forks, the content

A small town on the **Copperway**, a minor trade road, where the **River Sallow** splits
around a wedge of high ground. The town grew as three separate settlements that a toll
bridge and two ferries later tied together:

| Piece | Where | What's there | Features it exercises |
|---|---|---|---|
| **Tollside** | North bank, where the Copperway arrives | The toll bridge, the Toll Warden's house, the coaching inn | Table with spans, tableLink, sidebar plus sidebarLink, a `<data>` variable (`bridgeToll`) and dataRef, a dialog box with a speaker |
| **Millward** | East fork | Water mills, fishermen, the weir | skillcheck, savingthrow, hazard, condition, skillcheckResultTable, monsterEntry (the Forkjaw pike), the `<dc>` pill |
| **The Wedge** | The point between the forks | Market, ferry shrine, the old ford | magicItemBlock (*Ferryman's Lantern*), spellBlock (*Fork the Current*), parcelLink, figure plus figureRef |

Outline (`supplement type="settlement"`):

1. **Cover**: title, subtitle, coverImage (art), DMs Guild logo, Enigma Narrative logo,
   footer, text colour.
2. **Contents** with `<enigmaLogo/>` in the corner.
3. **productDescription**: two paragraphs and a bulleted list.
4. **Chapter 1, Where the River Splits** (intro):
   - A speakAloud box.
   - Nouns for the town, the river and the road, pilled through "Turn into a person,
     place or thing", with "pill every mention" on the river.
   - The town map as a figure, with `kind="map"`.
5. **Chapter 2, Tollside**: sections for the bridge, the inn and the Warden.
6. **Chapter 3, Millward**: sections for the weir and the mills, plus a pike attack
   encounter.
7. **Chapter 4, The Wedge**: sections for the market and the ferry shrine.
8. **Appendix A, Creatures of the Forks**:
   - monsterStatblock for the Toll Warden.
   - A book stat block reference (SRD *Bandit*).
   - A summoned-creature template.
9. **Back matter**:
   - credits (roles, role styles, verbatim credit, ISBN)
   - imageCredits
   - treasure (parcels with coins, gems, art objects, magic items)
   - global data values
   - chapterOrder
   - license

The supplement links to these files, each created through the editor's own New dialogs:

- `forkjaw-pike.xml` (monsters):
  - A family with header, habitats, a quote box with image, description and a
    distinction table.
  - A lair with effects.
  - Two stat blocks, Forkjaw Pike and Forkjaw Matriarch, with traits, attacks and
    legendary actions.
  - A `<dc>` in the grapple text, and a hit whose text starts with a comma.
- `townsfolk.xml` (monsters): the Toll Warden and a Ferry Hand. These are plain stat
  blocks, used for stat block add, move and delete.
- `ferrymans-lantern.xml` (magic item) and `fork-the-current.xml` (spell).

### Placeholder art

Every picture is generated, and every one is stamped **SLOP ART**: big and diagonal,
corner to corner, semi-transparent, so nobody could mistake it for real art. A script,
`e2e/art/make_slop_art.py` (Pillow), draws each one:

1. A flat colour with a simple shape on it (river fork lines for the map, a fish
   outline for the pike).
2. The title in small text.
3. The watermark, tiled twice on the tall images.

The script writes into the project folder before the suite starts. The art is never
committed; it's made on every run.

| File | Size | Used by |
|---|---|---|
| `cover.png` | 1275×1650 | coverImage, through a `<reference>` image |
| `grand-forks-map.png` | 1600×1200 | figure in Chapter 1, `kind="map"` |
| `toll-bridge.png` | 1200×800 | `<image>` in Tollside, plus a figure by ref in The Wedge (checks that an image used twice is credited once) |
| `forkjaw-pike.png` | 900×1200 | the monster family's quote box and image |
| `lantern.png` | 600×600 | an image in The Wedge's market section |

Credits use made-up names such as "Slop Art Studio". One image is deliberately left
uncredited, so the datasheet's "images to check" list isn't empty.

## How the harness works

### Launch and connect

Playwright can't launch a WebView2 app. Instead it attaches to one over the Chrome
DevTools Protocol (CDP), the way the scratch scripts already do. A session-scoped
pytest fixture handles it:

1. Builds once with `cargo tauri build --debug --no-bundle --features e2e`. That gives a
   self-contained exe with the UI embedded, so trunk doesn't have to serve it.
2. Launches the exe with these environment variables:
   - `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<free port>`
   - `WEBVIEW2_USER_DATA_FOLDER=<run>/webview`
   - `ENIGMA_CONFIG_DIR=<run>/config`. This is new; see the seams below. It means a run
     never sees or changes your real last-project setting.
3. Calls `chromium.connect_over_cdp(...)` and uses `contexts[0].pages[0]`.
4. Kills the process at teardown and keeps the exe's stderr in the run folder.

The uv cache venv stays shared with your real installs, so a run doesn't rebuild it.

### Test seams, built only with `--features e2e`

The native dialogs can't be clicked over CDP, so the e2e build swaps in a stand-in for
each one, plus a log:

| Seam | What it does |
|---|---|
| `ENIGMA_CONFIG_DIR` | Overrides `app_config_dir()`. This one can stay in every build; it's harmless. |
| Folder picker | `pick_folder` returns the next path from a queue that the test fills with an `e2e_queue_pick(paths)` command. Without the feature, the command doesn't exist. |
| Confirm | `dialogs::confirm` asks `window.__enigmaE2E.confirm(msg)` instead of the plugin. The test answers yes or no and checks the message text ("Delete the attack Thrown Harpoon?"). |
| Change log | `apply_change` appends `{target, variant, result}` to `<run>/changes.jsonl`. The coverage check reads this file. |

Everything else is real: the file system, the validator and the compiler.

### Selectors

- Prefer role and name locators. Most buttons already have a `title` or a visible label
  ("+ ¶ below", "Delete paragraph", "Apply").
- Repeated rows need an anchor. Add `data-testid` to:
  - file tree rows (`data-path`)
  - structure tree nodes (`data-node`, the element's path)
  - form rows: paragraph, list item, table row, attack, effect, reference entry
  - the Problems panel.

  Locate the row first, then the button inside it.
- Keep page objects in `e2e/pages/`: `FileTree`, `Structure`, `Viewer` (Form/Source
  toggle, Problems), `ProseEditor` (typing, Ctrl+K, context menu, pills), and one object
  per form. Tests read as steps; the selectors live in one place.
- Prose fields are contenteditable, so use `keyboard.type` and `keyboard.press`, never
  `fill`. A Prose helper types text, inserts a pill by Ctrl+K search, and right-clicks a
  word.

### Checks after each step

After each apply, a step checks three things:

1. The form shows no "unapplied" mark.
2. The Problems panel is empty.
3. An XPath query on the Source view's text finds the expected element. For example,
   the `<table>` has 3 `<tr>`s. Check structure, not whole strings, so reformatting
   doesn't break tests.

The golden comparison at the end catches everything else.

## Layout

```
e2e/
  pyproject.toml          playwright, pytest, pillow, lxml (run with uv)
  conftest.py             build, launch, connect, run folder, confirm/pick fixtures
  art/make_slop_art.py
  pages/                  page objects
  story/                  the primary suite, ordered: test_01_project.py ... test_12_build.py
  edges/                  the secondary suite: independent tests, each from a snapshot
  features.toml           the UI feature list for the coverage check
  golden/grand-forks/     the expected XML files
  out/                    run folders (gitignored)
```

I'd use Python rather than Playwright's Node runner. The repo already uses uv and pytest
for the compiler, lxml makes the XPath checks easy, and the art script is Python anyway.

## The primary suite, step by step

The steps run in order and each one builds on the last. After a step passes, the fixture
copies the project folder to `out/<run>/snapshots/NN`. Running
`pytest story -k 07 --from-snapshot 06` re-runs one step without redoing the whole build.

| # | Step | Covers |
|---|---|---|
| 01 | **Project**: Open project… on an empty folder → Configure dialog → schemas and compiler fields (typed, and picked through the queued picker) → `enigma-project.toml` written. File tree: expand, Refresh, a non-xml row is disabled. | open/configure project, folder picker, file tree |
| 02 | **New files** from the file tree's right-click menu: New supplement (settlement, "Grand Forks"), two New monster files, New magic item, New spell. Run the slop-art script. Check that each file opens in a tab. | all four New dialogs, tabs, open_file and validation on open |
| 03 | **Cover and front matter**: Add a cover → title, footer, text colour. Reference form → + Add ▾ image (cover.png) → cover "Choose an image…" picks it → Convert to `<coverImage>`. DMs Guild logo (find_logo), Enigma logo "Just above". Contents with enigmaLogo. Product description: paragraphs and a list. | cover form, reference images, logos, supplement parts |
| 04 | **Chapters**: Add chapter… ×4 plus the appendix (kind). Set titles, columns and credit. Move one chapter up and back. Delete a scratch chapter (confirm: no, then yes). | chapter Add/Update/Delete, chapter kinds, confirm |
| 05 | **Chapter 1 prose**: sections and paragraphs (+ ¶ at start, + ¶ below, drag to reorder, delete). A speakAloud box. A bulleted list, then switched to numbered. "Turn into a person, place or thing" for Grand Forks, Sallow and Copperway. Review… → pill every mention of "Sallow". Ctrl+K for a condition, a skill and a chapterRef. Spelling: right-click "Sallow" → add to the project word list (`enigma-words.txt`). The map figure and a figureRef. | block edits, boxes, lists, the prose editor, pills, nouns, the spell list |
| 06 | **Tollside**: toll table (headers, rows, a header spanning two columns, + Footer row, move a column, delete a row) plus tableLink. Sidebar "Toll Disputes" plus sidebarLink. Dialog box with a speaker and a trigger. "Turn into variable" on the toll price → `bridgeToll`, then a dataRef. The toll-bridge image. | table form (spans, columns), boxes, data values |
| 07 | **Monster file, the family**: in `forkjaw-pike.xml`, the header (name, short noun, subtitle, habitats), + Quote box & image, + Add a description, distinction table, a lair with three effects (add first, add below, move, delete), part ids, then move parts. | every FamilyEdit variant |
| 08 | **Monster file, stat blocks**: Forkjaw Pike and Matriarch. Ability scores, AC, HP, speeds, senses, languages, CR. Traits. Attacks, including a hit that starts with ", and" and one with a `<dc>` pill. Legendary action uses. Spell list. Gear. Reorder actions and delete one. In `townsfolk.xml`: add, move and delete stat blocks. The Preview tab: shows the selection, and updates after an apply. | StatEdit Update/Insert/Delete/Move, MonsterEdit, preview |
| 09 | **Item and spell files**: *Ferryman's Lantern* (rarity, attunement, description, a small table, a reference entry) and *Fork the Current* (casting, classes, materials, At Higher Levels). | item form, spell form |
| 10 | **Millward and The Wedge**: skillcheck, savingthrow, hazard (falling), skillcheckResultTable. monsterEntry for the pike: print only… with `<part ref>`, then "print everything, except" to cover both forms. magicItemBlock, spellBlock, a figure by ref (the image used twice), parcelLink, pageEnd. | section_el_form, every SECTION_BLOCKS kind |
| 11 | **Appendix and back matter**: monsterStatblock for the Toll Warden. Reference entries: a book stat block (SRD Bandit), a template (summoned creatures, with formulas and choices), an itemRef and a spellRef target, then reorder and delete entries. Credits with roles, role styles, a verbatim credit, an ISBN, imageCredits. Treasure parcels (+ Parcel, coins, gems, art objects, magic items, move, delete). Global values. chapterOrder. License: open a copy of `dmsguild-license.xml` and edit a notice paragraph. | reference form, template form, supplement parts, license form |
| 12 | **Build**: compare every XML file with `golden/` (after normalising indentation), then run `python -m encompile <supplement> --pdf`. Check that the PDF exists, the datasheet lists the uncredited image, the cover PNG exists, and there are no warnings except the expected uncredited-art one. Copy `grand_forks.pdf` to the run folder. | the whole round trip |

To update the golden files on purpose, run `pytest story --update-golden`. That rewrites
`golden/` and the diff gets reviewed like any other change.

## The secondary suite, edge cases

These are independent tests, each starting from a story snapshot. They cover behaviour
that would muddy the story:

- **Refused apply**: an edit the schema rejects (a table row with the wrong number of
  cells) shows "refused — see Problems" with a line number. The file on disk stays the
  same, byte for byte.
- **Revert** throws away unapplied edits. The "Unapplied changes" mark comes and goes.
- **Cancel on confirm** keeps the item, for each delete button. This covers 14 forms, so
  it's table-driven.
- **"This X isn't in the file any more"** shows when the file changes underneath an open
  form (the test edits the file on disk, then refreshes).
- **Opening a bad file**: non-supplement XML, or XML that fails the schema, shows the
  error dialog, and no tab opens.
- **Source view**: re-indented and highlighted. Selecting a structure node scrolls to it.
  Expand all and Collapse all.
- **Tabs**: closing a tab with unapplied changes, and switching tabs keeps each tab's
  selection.
- **Preview**: the debounce (three quick applies render once) and the faded stale
  image. Without pdflatex on PATH it fails with a clear message.
- **Ctrl+K**: the list is complete, names are capped at 40, groups keep their order, and
  the caret ends up after the inserted pill.

## Coverage check

The last tests, `coverage/test_coverage.py`, fail if anything was never used:

1. **Backend edits.**
   - It lists every `Change` target and every variant of every `*Edit` enum. A small
     `cargo run -p shared --bin edit-variants` prints them, so the list can't fall
     behind the code.
   - It also lists every `SECTION_BLOCKS` kind.
   - It checks each one against `changes.jsonl` for a `Saved` result.
2. **UI features.** It checks each entry in `features.toml` (dialogs, context-menu
   items, Ctrl+K, spelling, preview, Source view, tabs) against the `@covers("...")`
   marks on the tests that ran.

When someone adds a new edit variant without a test, the suite fails and names it.

## Order of work

1. **Harness**:
   - The `e2e` feature and its four seams.
   - `data-testid` anchors.
   - The conftest that builds, launches and connects.
   - The art script.
   - One smoke test: open a project, see the file tree.
2. **Steps 01–04**. These make the files every later step needs.
3. **Steps 05–06 and the prose editor page object**. This is the hardest part
   (contenteditable, pills, the context menu), so do it early.
4. **Steps 07–09**: monster, item and spell files. The statblock form is the biggest one.
5. **Steps 10–12**, the golden files and the compiler build.
6. **The coverage check**, then fill whatever it reports as missing.
7. **The secondary suite**.
8. **CI**: done, in `.github/workflows/ci.yml` (see Where it stands). A `windows-latest`
   runner has a desktop session, so WebView2 runs. TeX Live and the dndbook class are the
   slow part, and they're cached.

**Where it stands:** 1–7 are done. A full run (`uv run pytest --no-build`) is 106 tests in
about 2 min 15 s: the story (steps 01–12, golden comparison and PDF build), the secondary
suite (`edges/`, 26 tests) and the coverage check (`coverage/`). Every edit variant,
section block kind and listed UI feature is used, and none is pending.

CI (8) is `.github/workflows/ci.yml`. The Rust tests run on every push. The e2e job runs
the whole suite on `windows-latest`, with TeX Live and the dndbook class installed. It
needs the private data repo, so it waits for a repository secret, `ENIGMA_DATA_TOKEN`
(read access to jyurkiw/enigma_narrative). Until there is one, it says so and stops.

Two things differ from the plan above. The coverage check reads the edit variants from
`shared/src` with a regex instead of a `cargo run` binary. And features map to the tests
that use them in `features.toml` itself, rather than `@covers` marks on the tests.

## Findings so far

Things the story steps turned up. They're gaps in the editor, not in the tests, and each
is a GitHub issue.

**Fixed on this branch:** #3, #4, #5, #6, #8, #9, #10, #11, #12, #13, #14, #15 and #17. The
tests that used to work around them now check the fix (step 03's product description,
the map's kind and the blank line; step 04's chapter move; step 05's chips, focus and
pills; step 06's footer, spans and deleted cells; step 07's cutline and typed cells; and
`edges/test_prose.py`). Fixing them also turned up and fixed a regression: drag markers
reset each row's `class:` toggles, which the section form's old rebuild-on-every-apply had
hidden. Later on the branch, #7 and #16 were fixed too. The editor also gained two things
the last pending feature needed:
- **It notices a file changed on disk**, on focus and every two seconds. A tab with
  nothing unapplied reads the file again and says when the selected element is gone. A
  tab with unapplied edits offers to read it again instead.
- **Taking a form down no longer applies it.** A paragraph, list item, box line, table
  cell or reference card applies on leaving only if it's still on the page, so switching
  to the Source view writes nothing.

No feature in `features.toml` is pending now. The compiler issues are open in
jyurkiw/enigma_narrative (#4, #5, and #6, which is new: table spans). The list below is
the original write-up.

- **No form for `<productDescription>`** (#3). Step 03 can't write one, and nothing else in
  the editor can either.
- **No Kind field on reference images** (#4). The compiler's `kind="art|map"` can't be set
  from the reference form, so the map can't be marked as a map yet.
- **Chapters can't be reordered** (#5). `ChapterEdit` has Add, Update and Delete only.
  Step 04 reorders sections inside a chapter instead.
- **A leftover blank line** (#6). Removing the skeleton's cover and adding a new one leaves
  two blank lines after `<supplement>`.
- **New reference cards focus their Id field one frame late** (#7). Fast typing right
  after "+ Add" can land in the wrong field. The test waits for the focus; a person typing
  can't be fast enough to hit this.
- **Picking a tag in Ctrl+K applies the paragraph, then closes the tag's form** (#8). The
  focused search box is removed, which counts as leaving the paragraph, and the apply
  rebuilds the section form. Step 05 applies each paragraph before a chip goes in.
- **+ ¶ at start doesn't focus the new paragraph** (#9). + ¶ below does.
- **A new noun's chip shows its id in red until the box loses focus** (#10).
- **A table without a footer can't get one** (#11). Footer rows show only once there's a
  `<footer>`.
- **Table cells can't be deleted** (#12). A table link's new table is a cell over once it
  has headers, and spans can never leave a row that matches its headers. Step 06 replaces
  the first row and clears its span again.
- **Clicking the family image's Cutline box focuses its ＋ button** (#13). The box sits
  inside a `<label>`. Step 07 focuses it from script.
- **Typing in a distinction table cell comes out backwards** (#14). Each keystroke
  rebuilds the grid. Step 07 puts each cell's text in with one input event.
- **The compiler crashes on a legendary `<actionRef>` without a name**
  (jyurkiw/enigma_narrative#4). The schema makes the name optional, and so does the
  editor. Step 08 names the reused action so the preview renders.
- **The compiler fails with `-o` on another drive** (jyurkiw/enigma_narrative#5). Step 12
  builds into the run folder, which is on the same drive as the data repo.
- **Closing a tab with unapplied edits doesn't ask** (#15). A paragraph being edited is
  applied, because the close takes focus out of it; a stat block card's draft is lost.
  `edges/` pins today's behaviour.
- **The Source view shows unapplied edits only for the cover, chapters and raw XML** (#16).
- **Escape doesn't close the reference "+ Add ▾" menu, and leaves Ctrl+K with no focus**
  (#17).

Not editor problems, but worth knowing:

- The Windows copy of the dnd LaTeX class (`C:/Users/…/texmf`) fails on `DndReadAloud` and
  `DndSidebar` with no options (`I do not know the key '/tcb/\NoValue'`); the WSL copy
  doesn't. Step 12 allows that error and no other.
- The editor reads a file only when it opens it, so a change on disk shows once the tab is
  reopened. That's why "This X isn't in the file any more" is still pending in
  `features.toml`.

## Open questions

| Question | My suggestion |
|---|---|
| Where the run's project folder lives | `E:\Data\enigma-narrative\adventures\e2e\grand-forks\` (gitignored). Inside the data repo, the schemas, compiler and `shared/internal` SRD files are short relative paths. The alternative is `proto_ui/e2e/out/`, with long `../../..` paths across repos. |
| Keep each run, or overwrite one folder | Overwrite `grand-forks/` and keep only the last run's PDF; snapshots go under `e2e/out/`. |
| Commit the golden XML | Yes, in proto_ui. It doubles as an example supplement. |
| Seams only in the `e2e` feature | Yes for the picker, confirm and change log. `ENIGMA_CONFIG_DIR` can be in every build. |
| Python or Node Playwright | Python, for the reasons in Layout above. |
