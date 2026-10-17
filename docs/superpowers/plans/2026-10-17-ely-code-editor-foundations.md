# File editor on Ely — project 1 (foundations) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `FileView`'s hand-drawn code surface with Ely's `CodeEditor`, fed by Sirio's engines, and add everything spec §4 lists: breadcrumb, gutter with git marks and inline diff, minimap, sticky scroll, find/replace, read-only banner, the Ely status row, and Settings → Editor.

**Architecture:** Ely's `editor` module is vendored from the pinned upstream tree `e17e31a6`, with editor-only fixes ported from upstream `main` and a few hosting hooks added as recorded local changes. `FileView` hosts one `CodeEditor` entity per loaded file. `CodeEditor` owns the text, selections and undo. `sirio_ui::editor::Editor` keeps everything about the disk. Every Sirio engine (tree-sitter per-line colour, `sirio_lsp`, `sirio_git`) reaches the editor through `FileView`.

**Tech Stack:** Rust, gpui (`bezel-gpui =0.3.8`), vendored `ely-gpui-component`, `sirio_syntax`, `sirio_lsp`, `sirio_git`, `sirio_persistence`. Bash E2E against a real debug Sirio over the control socket.

**Spec:** `docs/superpowers/specs/2026-10-17-ely-code-editor-design.md` (§4–§9 are project 1). The prototype `docs/superpowers/specs/2026-10-17-ely-code-editor-prototype.html`, scene 1 and scene 4's find widget, is the visual target.

## Global Constraints

- Ely source: the pinned tree `e17e31a6` from `https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components`. Fetch it with `git clone https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components ely && git -C ely archive e17e31a6 src | tar -x`. Never vendor from `main` wholesale. Only the fixes Task 1 names come from `main`.
- Vendored files outside `src/editor/`, and outside the new files Task 1 adds, do not change. The chat and change-request surfaces must not move (spec §5.1).
- Every vendored file and every local change is recorded in `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md`.
- Colour stays per line, exactly as today: `file_view::code_spans(language, line)` → `sirio_syntax::highlight`, mapped through `Theme::syntax_palette()`. Ely's own lexer never colours a file.
- A missing language server is never volunteered (CLAUDE.md, Languages). `LspStatus` is absent for plain text and for a server that is not installed.
- Defaults:
  - On: minimap, sticky scroll, indent guides, rainbow brackets, code lens, inlay hints.
  - Off: visible whitespace, ruler.
  - Ruler column 100, range 20..=400.
  - Tab size 4, range 1..=16.
  - Line numbers absolute.
  - Cursor shape line.
- Setting keys, stored as strings in the existing `setting` table with no migration: `editor.minimap`, `editor.stickyScroll`, `editor.indentGuides`, `editor.rainbowBrackets`, `editor.codeLens`, `editor.inlayHints`, `editor.whitespace`, `editor.ruler`, `editor.rulerColumn`, `editor.lineNumbers` (`absolute|relative|hidden`), `editor.cursorShape` (`line|block|underline`), `editor.tabSize`.
- Keys:
  - Find: `secondary-f`.
  - Replace: `cmd-alt-f` on macOS, `ctrl-h` elsewhere.
  - Escape closes find and returns focus to the code.
  - Every shell binding that works with a file tab focused today keeps working. Where an Ely binding collides, the shell's binding wins and the Ely binding is moved and recorded.
  - The one collision found while planning: Ely's add-cursor `ctrl-alt-up` / `ctrl-alt-down` (non-macOS) collide with `FocusPaneAbove` / `FocusPaneBelow` (`rust/crates/sirio/src/panes.rs:108-109`). The `ctrl-alt-shift-*` keys are taken by the pane splits, so add-cursor moves to `shift-alt-up` / `shift-alt-down` on non-macOS. macOS keeps `cmd-alt-up` / `cmd-alt-down`.
  - Ely's `secondary-c` in the `ElyEditor` context is more specific than `text_selection`'s `CopySelectedText` (`!Terminal`), so the editor copies its own selection. With no selection it must `cx.propagate()`, never swallow the chord (CLAUDE.md, text selection).
- Debug-only control verbs use the `"surface.editor.X" if cfg!(debug_assertions)` guard, are pushed into `capabilities` only under `cfg!(debug_assertions)`, and are added to `respond_to_control_action` and `request_for_control_method` (`rust/crates/sirio/src/main.rs`; see "Control verb recipe" below).
- `sirio_perf` names are content-free `&'static str`. Each new `Render::render` gets a `span`.
- Never run `Scripts/ci.sh` or `Scripts/ci-linux.sh`; ask the user. Iterate with `cargo build -p <crate>` / `cargo nextest run -p <crate>`. Building `sirio` needs Zig 0.15.2 on PATH.
- Never build or run E2E artifacts on `/tmp`: it is a small tmpfs with a quota. Use `CARGO_TARGET_DIR` under the worktree, and have scripts create `RUN_DIR` under `${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}`.
- Tests: E2E first. A `#[test]` only for a pure unit, and its failure list is written in the test module's doc comment before the code. No tautological or change-detector tests (CLAUDE.md).
- Commits: Conventional Commits, lower-case imperative, and **no** Co-Authored-By or session links.

### Control verb recipe (used by Tasks 6–11)

A new debug-only verb `surface.editor.<name>` touches:

1. `ControlAction::Editor<Name> { …, reply: ControlReply }` (enum at `main.rs` ~1074–1334).
2. The capabilities list under `if cfg!(debug_assertions)` (~2546).
3. A dispatch arm `"surface.editor.<name>" if cfg!(debug_assertions) => self.queue_action(request, |reply| …)`, parsing `request.params`.
4. The drain arm in the `for action in pending_control` loop (~5527), calling a `SirioWorkspace::control_editor_<name>` method.
5. `respond_to_control_action` (~40744) and `request_for_control_method` (~40844).
6. A builder in `sirio_control::protocol::request`.
7. A `sirioctl surface editor <name>` arm in `cmd_surface`, with a "(debug builds)" usage line.

Every verb acts on the active tab's `FileView` and answers `"no file tab is active"` otherwise.

## Review Focus

1. **CRLF and mixed line endings.** A CRLF file is not dirty right after load, is saved with CRLF, and becomes dirty when its ending is switched. A mixed file is not dirty until edited, and is saved uniform. Test: Task 3 unit tests plus the Task 6 E2E CRLF scenario.
2. **Multi-byte text.** Accents, emoji and CJK must work through typing, find, diagnostics converted from UTF-16, hover offsets and git marks. No panic, and no edit or selection splitting a character. Test: Task 6 E2E types `è😀漢`, and Task 9 finds `😀` and replaces it.
3. **The disk changes under the editor.** A clean editor adopts the new text silently (`CodeEditor::reset`, cursor kept in range). A dirty one shows the conflict banner and keeps the user's text. Test: Task 6 E2E conflict scenario, both cases.
4. **A read-only snapshot refuses every edit path:** typing, paste, the context menu's Cut and Paste, find's replace, multi-cursor, the Markdown format buttons. Test: Task 10 E2E snapshot scenario.
5. **Edge-shaped files:** empty file, no trailing newline, a 1 MiB file, a single 20 000-character line. They open, scroll, find and save without a panic. Test: Task 6 E2E fixtures, plus Task 12's 1 MiB perf run.

---

### Task 1: Vendor Ely's editor closure

**Files:**
- Create: `rust/vendor/ely-gpui-component/src/editor/{mod,state,buffer,cursor,edit,keys,moves,line,row,view,layout,gutter,decor,syntax,ime,find,status,anchors,measure}.rs` (from `e17e31a6`; `anchors.rs` and `measure.rs` come from `main`, see step 3)
- Create: `rust/vendor/ely-gpui-component/src/navigation/breadcrumb.rs`; `src/shell/{mod,bars}.rs` (only `StatusBarItem` and what it needs)
- Modify: `rust/vendor/ely-gpui-component/src/{lib.rs,compat.rs,navigation/mod.rs}`; `LOCAL-CHANGES.md`
- Modify: `rust/crates/sirio_ui/src/ely.rs` (bind the editor's keys)

**Interfaces:**
- Produces:
  - `ely_gpui_component::editor::{CodeEditor, EditorEvent, LineNumbers, CursorShape, Selection, Diagnostic, DiffHunk, GitMark, ReadOnlyBanner, FindWidget, FindOptions, find_all, CursorPosition, LanguageMode, Encoding, LineEnding, IndentSettings, BranchIndicator, LspStatus, LspState, NotificationBell}`
  - `ely_gpui_component::navigation::{Breadcrumb, Crumb}`
  - `ely_gpui_component::init_editor(cx: &mut App)`, which binds the `"ElyEditor"` key context and is idempotent

- [ ] **Step 1: Write the module's `mod.rs`.** Export only project 1's files. Do not declare `hints, hover, refactor, symbols, search, filters, problems, console, chat, extensions, settings, keybindings, preview, tasks, welcome`. Where a kept file references a type from an undeclared file (for example `decor::GhostText` used by `state.rs`), keep the type in `decor.rs` and drop only the drawing.
- [ ] **Step 2: Add the missing imports.**
  - `charts::paint::tint` becomes `compat::tint` (copied verbatim, recorded the way `code-color` was).
  - `shell::bars::StatusBarItem` comes in with its own imports only.
  - `navigation/breadcrumb.rs` comes in with `forms::{Choice, Listing, listing}` if they are missing.
- [ ] **Step 3: Port from `main`, one commit each, each recorded:**
  - "Panics · P17 Editor: owner offsets ride the text's edits, stale ranges, rows and globs log instead of panicking". This brings `anchors.rs`, and `select` / `edit` log and ignore a bad range instead of `assert!`.
  - "Code editor: columns sit where gpui lays them out" and "Code editor: measure with the current text; the line end is a stop". These bring `measure.rs`.
  - "Editor · FindWidget: Escape from the query closes the box".
  - Find each fix by its subject with `git log --format='%h %s' main -- src/editor`. If a fix depends on code outside the closure, rewrite it against `e17e31a6` and record the rewrite.
- [ ] **Step 4: Add `pub fn init_editor(cx: &mut App)`** in `lib.rs`. It calls `init_chat(cx)` then `editor::bind_keys(cx)`, behind a once-guard (an `OnceFlag` global). Call it from `sirio_ui::ely::init` in place of `init_chat`.
- [ ] **Step 4b: Apply the key rules** in Global Constraints to `editor/keys.rs`: move add-cursor on non-macOS, and make the copy handler propagate when every selection is empty. Record both in `LOCAL-CHANGES.md`.
- [ ] **Step 5: Build.** Run `cd rust && cargo build -p ely-gpui-component && cargo nextest run -p ely-gpui-component`. Expected: the build succeeds and the inline tests of the vendored editor files pass. Adapt any gpui API gap locally and record it. If a gap needs a gpui change, stop and report (spec §8).
- [ ] **Step 6: Prove nothing else moved.**
  - Run `git diff --stat main -- rust/vendor/ely-gpui-component/src | grep -v -E 'src/(editor|shell)/|navigation/breadcrumb.rs|navigation/mod.rs|compat.rs|lib.rs'`. Expected: no lines.
  - Run `Scripts/Tests/test-ely-chat-ui-e2e.py` and `Scripts/Tests/test-ely-forge-probe.sh --xvfb`. Expected: both OK lines.
- [ ] **Step 7: Commit.** `feat(ely): vendor the code editor from the pinned upstream tree`. Record the new upstream URL, and the fact that `e17e31a6` is off upstream `main`, at the top of `LOCAL-CHANGES.md`.

### Task 2: Ely hosting hooks (local changes)

**Files:**
- Modify: `rust/vendor/ely-gpui-component/src/editor/{state,line,view,gutter}.rs`, `LOCAL-CHANGES.md`
- Create: `rust/crates/sirio_ui/examples/ely_editor_probe.rs`, `Scripts/Tests/test-ely-editor-probe.sh`

**Interfaces:**
- Consumes: Task 1's `CodeEditor`.
- Produces, all on `CodeEditor`:
  - `pub fn highlighter(self, f: impl Fn(usize, &str, &App) -> Vec<(Range<usize>, HighlightStyle)> + 'static) -> Self` and `pub fn set_highlighter(&mut self, f: …, cx)`. The first argument is the line index. When a highlighter is set, it replaces `syntax::colors` for every line. Without one, upstream behaviour is unchanged.
  - `pub struct ViewOptions { pub minimap: bool, pub sticky_scroll: bool, pub indent_guides: bool, pub rainbow_brackets: bool, pub whitespace: bool, pub rulers: Vec<usize>, pub tab_size: usize, pub line_numbers: LineNumbers, pub cursor_shape: CursorShape }`, plus `pub fn set_view_options(&mut self, options: ViewOptions, cx)` and `pub fn view_options(&self) -> ViewOptions`.
  - `pub fn without_breakpoints(self) -> Self`. The gutter's breakpoint slot draws nothing and takes no press.
  - `pub fn reset(&mut self, text: impl Into<String>, cx)`: new text with no history, the caret at 0, folds cleared. This is ported from upstream `main`'s `prose.rs`.
  - `EditorEvent` gains `Pointer { offset: Option<usize>, position: Point<Pixels> }` (non-drag mouse move over the code, `None` past the text), `SecondaryClick { offset: Option<usize>, position: Point<Pixels> }`, `PlatformClick { offset: usize }` (platform-modifier press and release in place, no drag), and `GitMarkPressed { line: usize }` (a press on the gutter's git bar, made a 4px hit area).

- [ ] **Step 1: Write the probe.** `ely_editor_probe.rs` follows `ely_forge_probe.rs`. It opens a window with Sirio's theme and a `CodeEditor` over a 60-line Rust fixture with `.highlighter(..)` returning one span coloured `#ff00ff` on line 3. It also sets git marks on lines 5–7, a diagnostic on line 9, `ViewOptions` with minimap and sticky scroll on, and `without_breakpoints()`. It prints one line per `EditorEvent` it receives, then saves a frame. `test-ely-editor-probe.sh` runs it under `--xvfb` like `test-ely-forge-probe.sh` and then checks:
  - the frame has ≥200 colours and a `#ff00ff` pixel;
  - after `xdotool` moves over line 3, the transcript holds `Pointer offset=Some(..)`;
  - after `xdotool` clicks the git bar at line 5, it holds `GitMarkPressed line=5`;
  - a click in the breakpoint slot produces no `toggle`.
  
  It prints `ELY EDITOR PROBE OK`.
- [ ] **Step 2: Run it.** Expected: FAIL. `highlighter` does not exist.
- [ ] **Step 3: Implement the hooks above** in the files listed. `line.rs` calls the highlighter with `(line, row_text)`.
- [ ] **Step 4: Run** `Scripts/Tests/test-ely-editor-probe.sh --xvfb`. Expected: `ELY EDITOR PROBE OK`.
- [ ] **Step 5: Commit.** `feat(ely): let a host colour, configure and listen to the code editor`

### Task 3: `Editor` holds LF text and a line ending

**Files:**
- Modify: `rust/crates/sirio_ui/src/editor.rs`

**Interfaces:**
- Produces:
  - `pub enum LineEnding { Lf, Crlf }` with `pub fn detect(raw: &str) -> LineEnding` (CRLF when more lines end in `\r\n` than in a bare `\n`).
  - `Editor::line_ending(&self) -> LineEnding`
  - `Editor::set_line_ending(&mut self, ending: LineEnding)`
  - `Editor::replace_all(&mut self, text: &str)`
  
  Contract: `Editor::buffer()` is always LF-only. `save` writes the buffer with `line_ending()` applied. `is_dirty()` is true when the buffer differs from the LF-normalized disk text, **or** when `line_ending()` differs from the detected one.

- [ ] **Step 1: Write the failure list, then the tests.** The failure list goes in the module's `line_endings` test doc. Tests, in the `#[cfg(test)]` module:
  - `a_crlf_file_loads_lf_and_is_clean`: write `"a\r\nb\r\n"`, open, then assert `buffer() == "a\nb\n"`, `line_ending() == Crlf` and `!is_dirty()`.
  - `a_crlf_file_saves_crlf`: open, `replace_all("a\nc\n")`, save, then assert the disk is `"a\r\nc\r\n"`.
  - `switching_the_ending_dirties_and_saves_it`: open the CRLF file, `set_line_ending(Lf)`, then assert `is_dirty()`; save, then assert the disk is `"a\nb\n"` and `!is_dirty()`.
  - `a_mixed_file_is_clean_until_edited_and_saves_uniform`: write `"a\r\nb\nc\r\n"`, then assert `!is_dirty()` and `Crlf`; `replace_all(buffer + "d\n")`, save, then assert the disk is `"a\r\nb\r\nc\r\nd\r\n"`.
  - `a_lone_carriage_return_is_kept_as_text`: `"a\rb\n"` loads unchanged.
- [ ] **Step 2: Run** `cd rust && cargo nextest run -p sirio_ui line_endings`. Expected: FAIL (`LineEnding` undefined). Check the count: 5 tests selected.
- [ ] **Step 3: Implement.** Normalize in `open`, `from_buffer`, `from_snapshot`, `reload` and `check_external`'s adopt path. Denormalize in `save`. Compute dirty as the contract says. `replace_all` replaces the whole buffer and calls `sync_document_from_buffer`.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_ui editor::`. Expected: the 5 new tests and every existing `editor.rs` test pass.
- [ ] **Step 5: Commit.** `feat(editor): keep the buffer lf and remember the file's line ending`

### Task 4: Editor preferences, stored and live

**Files:**
- Modify: `rust/crates/sirio_persistence/src/{model.rs,db.rs,lib.rs}`, `rust/crates/sirio_persistence/tests/persistence_integration.rs`
- Create: `rust/crates/sirio_ui/src/editor_prefs.rs`; modify `rust/crates/sirio_ui/src/{lib.rs,settings.rs}` (`SettingsSnapshot` only)
- Modify: `rust/crates/sirio/src/{main.rs,session.rs}` (conversions, startup apply, observer)

**Interfaces:**
- Produces:
  - `sirio_persistence::{EditorLineNumbers { Absolute, Relative, Hidden }, EditorCursorShape { Line, Block, Underline }}`, with `raw()` / `parse()` following the `BaseColor` pattern (`model.rs:470-549`).
  - `AppSettings` fields: `editor_minimap: bool`, `editor_sticky_scroll: bool`, `editor_indent_guides: bool`, `editor_rainbow_brackets: bool`, `editor_code_lens: bool`, `editor_inlay_hints: bool`, `editor_whitespace: bool`, `editor_ruler: bool`, `editor_ruler_column: u32`, `editor_line_numbers: EditorLineNumbers`, `editor_cursor_shape: EditorCursorShape`, `editor_tab_size: u32`. Keys and defaults as in Global Constraints. `settings_ranges::{EDITOR_RULER_COLUMN: 20..=400, EDITOR_TAB_SIZE: 1..=16}`.
  - `sirio_ui::editor_prefs::EditorPrefs` (Clone, PartialEq, `gpui::Global`), with the same twelve fields as `AppSettings` and no `editor_` prefix. Methods:
    - `EditorPrefs::current(cx: &App) -> EditorPrefs` (Default when unset)
    - `EditorPrefs::apply(prefs, cx: &mut App)` (sets the global, then `cx.refresh_windows()` only when the value changed)
    - `EditorPrefs::view_options(&self) -> ely_gpui_component::editor::ViewOptions` (rulers is `vec![ruler_column]` when `ruler`, else empty)
  - `SettingsSnapshot.editor: EditorPrefs`

- [ ] **Step 1: Write the failing test.** Add `editor_settings_round_trip_and_default` in `persistence_integration.rs`, modelled on `markdown_plantuml_server_round_trips_and_defaults_to_empty`.
  - A fresh database reads the defaults.
  - Saving non-default values (minimap off, ruler on at 120, relative, block, tab 2) reads them back.
  - Raw rows `editor.rulerColumn=5` and `editor.cursorShape=bogus` read as `20` and `Line`.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_persistence editor_settings`. Expected: FAIL (no field).
- [ ] **Step 3: Implement storage.** Add the fields, keys, ranges and the load/save blocks in `db.rs`. Update the exhaustive literals: `session.rs:3356`, `main.rs:37458`, `settings_snapshot_from_app_settings`, `app_settings_from_snapshot`.
- [ ] **Step 4: Wire live apply.**
  - In `SirioWorkspace::new`'s `cx.observe(&settings, …)` (`main.rs` ~5985), call `EditorPrefs::apply(snapshot.editor.clone(), cx)`. The guard inside `apply` keeps caret blinks from refreshing windows.
  - Apply the stored prefs at startup beside `DiagramSettings::apply` (~25518).
- [ ] **Step 5: Run** `cargo nextest run -p sirio_persistence && cargo build -p sirio`. Expected: PASS and the build succeeds.
- [ ] **Step 6: Commit.** `feat(settings): store editor preferences and publish them app-wide`

### Task 5: Settings → Editor page

**Files:**
- Create: `rust/crates/sirio_ui/src/settings/editor_page.rs`
- Modify: `rust/crates/sirio_ui/src/settings.rs` (category, setters, dispatch), `rust/crates/sirio/src/main.rs` (`parse_settings_category` if needed), `Scripts/visual-sweep.sh:456-462` (one `record_gap` line)

**Interfaces:**
- Consumes: `SettingsSnapshot.editor` (Task 4).
- Produces:
  - `SettingsCategory::Editor`, title "Editor", socket label `editor`, placed after `Appearance`.
  - `Settings::set_editor_prefs(&mut self, prefs: EditorPrefs, cx)`, which calls `changed()`.
  - Debug selectors `settings-editor-<field>` on each control, where `<field>` is the kebab-case key (`minimap`, `sticky-scroll`, …, `ruler-column`, `tab-size`).

- [ ] **Step 1: Write the failing test.** Write `the_editor_page_changes_every_preference_through_its_controls` in `settings.rs` tests, modelled on the appearance test (~6622).
  - Select `settings-category-Editor`.
  - Click each switch, then assert the snapshot field flipped.
  - Pick `relative` and `block` in the two `ToggleGroup`s.
  - Type `120` in the ruler column field and `2` in the tab size field.
  - Assert `snapshot().editor` equals the expected `EditorPrefs`.
  - Also write `the_editor_category_round_trips_through_its_socket_label`.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_ui the_editor_`. Expected: FAIL.
- [ ] **Step 3: Implement the page with Ely components only.**
  - The eight switches are `Checkbox` with a label.
  - Line numbers and cursor shape are `ToggleGroup`. A single-mode click on the selected item returns an empty set, so ignore it.
  - Ruler column and tab size are `TextInput` with `.filter(|c| c.is_ascii_digit())`. Create the input lazily on first render, because it needs a `Window`. Clamp to the ranges on blur or Enter.
  - The ruler column row is disabled while the ruler is off.
  - Layout: `settings_section("Display", …)` holding the eight switches plus line numbers and cursor shape, then `settings_section("Text", …)` holding ruler column and tab size. Spacing and typography match `render_appearance`.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_ui settings`. Expected: PASS, including the existing settings tests.
- [ ] **Step 5: Commit.** `feat(settings): add an editor page drawn with ely`

### Task 6: `FileView` hosts `CodeEditor`

The core swap: text, disk and Markdown. Pointer features follow in Task 7.

**Files:**
- Create: `rust/crates/sirio_ui/src/file_view/code_surface.rs` (hosting, sync, highlighter, prefs), `rust/crates/sirio_ui/src/file_view/sync.rs` (pure helpers)
- Modify: `rust/crates/sirio_ui/src/file_view.rs`. Delete the old surface: `EditableLine`, `render_source_line`, the caret, selection and goal-column machinery, `on_editor_key`, `move_*`, `page_lines`, mouse selection, and `flip_editor_blink`. Keep `code_spans`, `scrollbar` and `horizontal_scrollbar`, which `ci_log_tab.rs` uses.
- Modify: `rust/crates/sirio/src/main.rs` (verbs), `rust/crates/sirio_control/src/{protocol.rs,bin/sirioctl.rs}`
- Create: `Scripts/Tests/test-editor-e2e.sh`, `Scripts/Tests/editor-fixtures/` (see step 1)

**Interfaces:**
- Consumes: Task 1 and Task 2's `CodeEditor` API, Task 3's `LineEnding` and `replace_all`, Task 4's `EditorPrefs`.
- Produces:
  - `FileView::code_editor(&self) -> Option<&Entity<CodeEditor>>`
  - `sync::minimal_edit(old: &str, new: &str) -> Option<(Range<usize>, String)>` (common prefix and suffix, on char boundaries)
  - Verbs:
    - `surface.editor.open path=`
    - `surface.editor.state` → `path, state (loading|loaded|error), error, text (escaped like surface.file.read), dirty, language, line_ending (lf|crlf), selections ("a..b;c..d"), read_only, conflict (none|changed|deleted), mode (code|preview), prefs (k=v,…)`
    - `surface.editor.type text=`
    - `surface.editor.keys keys=` (gpui keystroke list, e.g. `ctrl-z`, dispatched to the window)
    - `surface.editor.select ranges=`
    - `surface.editor.save`
    - `surface.editor.conflict action=reload|keep`
    - `surface.editor.mode mode=code|preview`
    - `surface.editor.pref key= value=`, where `key` is a setting key from Global Constraints. It goes through the same save path as the Settings page, so the live-apply observer is what the E2E proves.

- [ ] **Step 1: Write the E2E skeleton and its first scenarios.**
  - **Harness.** `test-editor-e2e.sh` follows `test-changes-e2e.sh`. Its flags are `--out-dir`, `--state-only`, `--appearance light|dark`, `--display :N`, and `--xvfb` (starting Xvfb as in `test-host-sessions-e2e.sh` 1783–1800). It uses the private `HOME`/`XDG_*` world of `use_world` (`test-host-sessions-e2e.sh` 200–215), `RUN_DIR` under `${SIRIO_E2E_TMP:-$HOME/.cache/sirio-e2e}`, readiness via `sirioctl surface tabs read`, and PID-matched `capture`.
  - **Fixtures**, in a fresh git repo inside `RUN_DIR`: `main.rs` (60 lines), `crlf.txt` (`a\r\nb\r\n`), `mixed.txt`, `empty.txt`, `nonl.txt` (no trailing newline), `wide.txt` (one 20 000-char line), `notes.md` (headings and a `[link](main.rs)`), `binary.bin`, `big.txt` (1.1 MiB), plus a missing path and an unreadable file (`chmod 000`).
  - **Scenarios:**
    - `load_states`: each fixture's `state` and `error` match today's distinct messages.
    - `edit_save`: open `main.rs`, `select 0..0`, type `è😀漢\n`, `keys ctrl-z`, then `keys ctrl-shift-z` (`cmd-shift-z` on macOS). `select` two ranges and type `X` (multi-cursor). `save`, then read the file back and compare with the expected bytes.
    - `crlf`: open `crlf.txt` and check `dirty=false line_ending=crlf`. Type, save, and check the bytes keep `\r\n`.
    - `conflict`, clean: write `main.rs` externally, wait for `text` to change, and check `dirty=false`.
    - `conflict`, dirty: type, write externally, and check `conflict=changed`. `conflict action=keep` and check the text is kept; then `reload` and check the text equals the disk.
    - `markdown`: open `notes.md`, `mode preview`, capture, then `mode code`, capture.
    - `colour`: open `main.rs`, which holds a multi-line raw string, and capture it. The string lines are coloured line by line, as today (spec §5.2).
    - `prefs`: seed `editor.minimap=false` with `--setting`, open, and check `prefs` has `minimap=false`. Then, for each of the eight switches, both line-number modes other than absolute, both cursor shapes other than line, and the ruler at column 40: `pref`, check `state`'s `prefs`, and capture. This is spec §6's "each Settings → Editor switch seen in a capture".
    - `copy_propagates`: with an empty selection, `keys ctrl-c` in the editor leaves the clipboard unchanged and does not error. With a selection, the clipboard holds it.
    - `pane_keys`: with a file tab focused in a split, `keys ctrl-alt-down` moves focus to the pane below (`surface tabs read`), and `keys shift-alt-down` adds a cursor (`selections` has two entries).
  - It prints `EDITOR E2E OK`.
- [ ] **Step 2: Run** `Scripts/Tests/test-editor-e2e.sh --state-only`. Expected: FAIL (`unknown control method: surface.editor.open`).
- [ ] **Step 3: Write `sync.rs`'s failure list and tests, then `minimal_edit`.** Tests:
  - `identical_texts_need_no_edit` → `None`
  - `an_insert_in_the_middle_is_one_small_edit`
  - `an_edit_never_splits_a_character`: `"aè"` → `"aé"` replaces the whole `è`
  - `a_full_rewrite_replaces_everything`
- [ ] **Step 4: Implement `code_surface.rs`.**
  - **Creation.** On `Loaded`, create `CodeEditor::new(editor.buffer(), …)` with `.language(language.name())`, `.highlighter(|_, line, cx| code_spans(language, line) → syntax_palette styles)`, `.without_breakpoints()` and `.read_only()` for snapshots. Then apply `EditorPrefs::current(cx).view_options()`.
  - **`EditorEvent::Changed`.** Call `editor.replace_all(code_editor.text())`, then `cx.notify()`. The existing `cx.observe` in `main.rs` sends `didChange`.
  - **Editor → surface.**
    - `reload`, `keep` with adoption, and `check_external`'s auto-adopt call `CodeEditor::reset`, then restore the caret clamped to the new length.
    - Markdown format ops run on `Editor`, then push `minimal_edit(old, new)` through `CodeEditor::edit`, so they are one undo step, then select the returned `Selection`.
  - **Focus.** `cx.on_focus(&code_editor.focus_handle(cx), …)` calls `check_external`. Keep the inotify poll.
  - **Prefs.** On render, if `EditorPrefs::current(cx).view_options() != code_editor.view_options()`, call `set_view_options`.
  - **Markdown.** Source mode is the `CodeEditor`. Preview, the toolbar, the manual-preview notice and the large-file lock behave as today.
- [ ] **Step 5: Add the verbs** in the "Produces" list, following the recipe.
- [ ] **Step 6: Rewire the (a\*) tests to the new surface.** Tests that waited on `file-text-scroll` / `file-source-line-N` wait on the `CodeEditor`'s element. Tests that read `caret` or `source_selection` read `CodeEditor::selections()`. Tests that focused `editor_focus` focus the `CodeEditor`.

  The tests are: `keyboard_input_edits_the_buffer_through_the_focused_file_view`, `a_file_message_sits_over_the_file_and_can_be_dismissed`, `an_offer_draws_its_actions_and_the_file_underneath`, `revealing_a_line_scrolls_it_into_view`, `a_reveal_asked_before_the_file_loaded_still_happens`, `copy_and_trim_pastes_flush_left`, `the_clipboard_chords_copy_cut_and_paste_the_source_selection`, `select_all_answers_both_the_control_and_the_platform_chord`, `the_paste_chord_leaves_the_buffer_alone_under_the_markdown_preview`, `reveal_and_send_leave_as_events_carrying_the_located_selection`, `markdown_code_mode_edits_and_saves_the_raw_source`, `markdown_preview_and_code_modes_switch_in_the_drawn_frame`, `a_code_file_has_no_mode_switch_and_always_renders_source`, `a_large_markdown_file_opens_in_code_with_manual_preview_and_unlocks`, `markdown_toolbar_controls_change_the_selected_source`, `a_snapshot_shows_the_revisions_text_and_refuses_every_edit`, `a_reveal_asked_before_a_snapshots_retry_succeeded_still_happens`, and the `main.rs` tests at ~26668, 26772 and 35926.

  Delete the (b) tests listed in the research notes: `selection_spans`, `line_walk`, `columns`, caret movement, `the_editor_caret_blinks_…`, `a_long_file_draws_only_the_visible_lines`, `file_horizontal_bar_tracks_wide_source_…`. Keep the `underline_spans` behaviour for Task 7.

  Keep the three colour tests (`java_reaches_the_code_surface_classified`, `every_kind_bezel_classifies_reaches_the_code_surface`, `a_number_is_not_painted_in_the_string_colour`) as they are: they test `code_spans`, which the highlighter calls.
- [ ] **Step 7: Run the unit tests.** `cargo nextest run -p sirio_ui file_view && cargo nextest run -p sirio file`. Expected: PASS. Then run `git diff main --stat` and check that the deleted (b) tests are the only test-count drop: compare `cargo nextest list -p sirio_ui | wc -l` before and after.
- [ ] **Step 8: Run the E2E.** `Scripts/Tests/test-editor-e2e.sh --xvfb --appearance dark`, then the same with `--appearance light`. Expected: `EDITOR E2E OK`, with captures under `--out-dir`.
- [ ] **Step 9: Commit.** `feat(editor): draw the file tab's code with ely's code editor`

### Task 7: Pointer features, context menu, diagnostics, reveal

**Files:**
- Modify: `rust/crates/sirio_ui/src/file_view/code_surface.rs`, `rust/crates/sirio_ui/src/file_view.rs`
- Create: `Scripts/Tests/fake_lsp.py` (stdio JSON-RPC: `initialize`, `textDocument/hover`, `definition`, `documentSymbol`, then pushes `publishDiagnostics` after `didOpen`; answers driven by a JSON file passed as `argv[1]`)
- Modify: `Scripts/Tests/test-editor-e2e.sh`

**Interfaces:**
- Consumes: Task 2's events and Task 6's surface.
- Produces:
  - `FileView::set_diagnostics` maps each `FileDiagnostic` to `ely::editor::Diagnostic { range, severity, message }`. Severity: Error→Error, Warning→Warning, Information→Info, Hint→Info. A zero-width range widens to one character, or backwards at line end, as `underline_spans` pinned.
  - Debug verbs:
    - `surface.editor.hover offset=`, which acts as if the pointer rested at `offset`
    - `surface.editor.click offset= platform=true|false`
    - `surface.editor.context offset= action=<FileContextAction kebab>`
  - `surface.editor.state` gains `hover`, `message` and `diagnostics` (count).

- [ ] **Step 1: Write the E2E scenarios.**
  - The script writes `$XDG_CONFIG_HOME/sirio/languages.toml`, pointing `.rs` at `python3 fake_lsp.py answers.json`.
  - `diagnostics`: open `main.rs`, wait for `diagnostics=2`, capture (gutter, squiggle, minimap).
  - `hover`: `hover offset=<fn name>`, then wait until `hover` contains the fake answer.
  - `definition`: `click offset=<call> platform=true`, then check that `surface tabs read` shows the target file active at the fake line.
  - `link`: in `notes.md` code mode, `click offset=<label> platform=true` opens `main.rs`.
  - `context`: `context action=copy-path`, then read the clipboard with `xclip -o` on Xvfb, and check `send-selection-to-agent` arrives in the chat composer, as `surface.chat` already reads.
  - `reveal`: open via `sirioctl` at line 50, then check that `selections` starts on line 50.
- [ ] **Step 2: Run them.** Expected: FAIL (`unknown control method: surface.editor.hover`).
- [ ] **Step 3: Implement.**
  - `Pointer` → `hover_moved(offset, position)`. Keep the 300 ms delay and the sequence counter; `None` dismisses.
  - `PlatformClick` → a Markdown link label → `open_markdown_link`; otherwise `request_definition(offset, Some(pointer position))`.
  - `SecondaryClick` → `open_context_menu` at `position`. It moves the caret to `offset` when the click is outside the selection, which fixes "Go to definition uses the caret".
  - `handle_context_action`: Copy, Cut, Paste and Select All act on the `CodeEditor`'s selections. Cut and Paste are refused for a snapshot. `selected_text`, `copy_selection(trim)` and `selection_lines` read the primary selection.
  - `reveal_at(line)` selects `line_start..line_start` once the editor exists.
  - Diagnostics map as in Interfaces. The hover card still prefixes local diagnostics.
- [ ] **Step 4: Run** `cargo nextest run -p sirio_ui file_view && cargo nextest run -p sirio definition hover`, then `Scripts/Tests/test-editor-e2e.sh --xvfb`. Expected: PASS and `EDITOR E2E OK`. Also run `cargo nextest run -p sirio an_edit_after_the_open_reaches_the_server an_opened_document_reaches_the_server_with_its_text`, the LSP sync contract. Expected: PASS.
- [ ] **Step 5: Commit.** `feat(editor): carry hover, definition, links and the context menu onto the ely editor`

### Task 8: Git marks and inline diff

**Files:**
- Create: `rust/crates/sirio_ui/src/file_view/git_marks.rs`
- Modify: `rust/crates/sirio_ui/src/file_view.rs`, `rust/crates/sirio/src/main.rs` (fetch and push), `Scripts/Tests/test-editor-e2e.sh`

**Interfaces:**
- Consumes: `sirio_git::{diff_entry, status, FileDiff, Hunk, DiffLine, DiffOrigin}`, plus Task 2's `GitMarkPressed`.
- Produces:
  - `git_marks::marks(diff: &FileDiff) -> Vec<(usize, GitMark)>`, with 0-based lines in the current file:
    - a hunk with additions and no deletions → `Added` on each added line;
    - a hunk with both → `Modified` on each added line;
    - deletions only → `Deleted` on the line above the gap, or line 0 when the gap is at the top.
  - `git_marks::hunk_at(diff: &FileDiff, line: usize) -> Option<DiffHunk>`, the hunk whose new-side lines contain `line`. Its `removed` lines are the deletions.
  - `FileView::set_git_diff(&mut self, diff: Option<FileDiff>, cx)`.
  - The app calls `sirio_git::diff_entry(repo, entry, 0)` off the GPUI thread on `Loaded`, after a save, and on file-system events for that path, then pushes the result. A file that is not in a repository, or is clean, gets `None` (no marks).
  - Verbs: `surface.editor.git_mark line=`. `state` gains `git_marks ("3:added,5:modified")` and `diff_open (line|none)`.

- [ ] **Step 1: Write the failure list, then the tests,** in `git_marks.rs`:
  - `an_added_block_marks_each_added_line`
  - `a_replaced_block_marks_modified`
  - `a_pure_deletion_marks_the_line_above_the_gap`
  - `a_deletion_at_the_top_marks_line_zero`
  - `hunk_at_a_line_outside_every_hunk_is_none`
  - `hunk_at_returns_the_removed_lines_in_order`
  
  Fixtures come from `sirio_git::parse_diff` on literal diff text.
- [ ] **Step 2: Run** `cargo nextest run -p sirio_ui git_marks`. Expected: FAIL. Check that 6 tests are selected.
- [ ] **Step 3: Implement.**
  - `marks` and `hunk_at`.
  - `set_git_diff` → `CodeEditor::set_git_marks`.
  - `GitMarkPressed { line }` → if the open diff is at that hunk, `set_diff(vec![])`; otherwise `set_diff(vec![hunk_at(..)])`.
  - Escape, with no find open, closes an open diff.
  - The app-side fetch as in Interfaces.
- [ ] **Step 4: Write the E2E scenario.** Modify committed `main.rs` on disk (change line 10, delete line 20, add two lines at 30), open it, and wait for `git_marks`. Then `git_mark line=9` → `diff_open=9`, capture; `git_mark line=9` → `diff_open=none`.
- [ ] **Step 5: Run** the unit tests and `test-editor-e2e.sh --xvfb`. Expected: PASS and `EDITOR E2E OK`.
- [ ] **Step 6: Commit.** `feat(editor): mark changed lines against head and open a hunk in place`

### Task 9: Find and replace

**Files:**
- Create: `rust/crates/sirio_ui/src/file_view/find.rs`
- Modify: `rust/crates/sirio_ui/src/file_view.rs` (`init` binds the keys in context `"FileEditor"`), `Scripts/Tests/test-editor-e2e.sh`

**Interfaces:**
- Consumes: `ely::editor::{FindWidget, FindOptions, find_all}`, `ely::forms::TextInput`, `CodeEditor::{set_backgrounds, select, edit, text}`.
- Produces:
  - `struct FindState { query: Entity<TextInput>, replacement: Entity<TextInput>, options: FindOptions, matches: Vec<Range<usize>>, current: Option<usize>, error: Option<SharedString>, replace_open: bool }`, held as `FileView.find: Option<FindState>`.
  - Actions `file_view::{Find, Replace}`. `Find` opens the widget with the primary selection, when it is single-line, as the query.
  - Behaviour:
    - Matches are recomputed on query, option and text change.
    - All matches get a dim background, and the current one the selection colour.
    - Step moves `current` and selects it.
    - Replace swaps the current match and steps.
    - Replace all is **one** `CodeEditor::edit` with every range, so it is one undo step.
    - A regex error shows in the widget's `error`.
    - A snapshot shows no replace row.
  - Verbs: `surface.editor.find query= case= word= regex= replace=` (opens with the replace row when `replace` is given), `surface.editor.find_step forward=`, `surface.editor.replace all=`, `surface.editor.find_close`. `state` gains `find_matches`, `find_current` and `find_error`.

- [ ] **Step 1: Write the E2E scenarios.**
  - `find`: query `fn` → `find_matches` equals `grep -o` on the fixture; `find_step` twice → `find_current=2`; `case=true` changes the count; `regex=true` with `(` → `find_error` is non-empty.
  - `replace`: `replace=Y all=false` → one changed; `all=true` → none left; `keys ctrl-z` once → all restored.
  - `multibyte`: find `😀` and replace it with `x`.
  - `escape`: `keys escape` closes it and the focus is back in the code (a typed `q` lands in the text).
  - Capture with the widget open, dark and light.
- [ ] **Step 2: Run them.** Expected: FAIL.
- [ ] **Step 3: Implement** as in Interfaces. Bind `secondary-f` → `Find` and the platform replace key → `Replace` in context `"FileEditor"`. Check `bind_keys` in `main.rs` (573, 5364) for collisions, and record any in the plan's notes.
- [ ] **Step 4: Run** `test-editor-e2e.sh --xvfb`. Expected: `EDITOR E2E OK`.
- [ ] **Step 5: Commit.** `feat(editor): find and replace in the file with ely's find widget`

### Task 10: Breadcrumb and read-only banner

**Files:**
- Create: `rust/crates/sirio_ui/src/file_view/breadcrumb.rs`
- Modify: `rust/crates/sirio_ui/src/file_view.rs`, `rust/crates/sirio/src/main.rs` (symbols fetch), `Scripts/Tests/test-editor-e2e.sh`

**Interfaces:**
- Consumes: `sirio_ui::outline::OutlineSymbol { name, kind, line, depth, .. }` (the type Go to Symbol already converts to) and `sirio_lsp::document_symbols`.
- Produces:
  - `breadcrumb::chain(symbols: &[OutlineSymbol], caret_line: usize) -> Vec<&OutlineSymbol>`: walking symbols in order with `line <= caret_line`, keep the last symbol seen at each depth, truncating deeper levels whenever a shallower one appears. The protocol gives start lines only (spec §2's breadcrumb row), so the chain is the enclosing-by-start approximation.
  - `FileView::set_symbols(&mut self, symbols: Vec<OutlineSymbol>, cx)`.
  - The app fetches symbols when the server reports `document_symbols` capability, on `Loaded`, after a save, and 1 s after the last edit (debounced).
  - The breadcrumb shows the path relative to the worktree root (from `shell_facts`) plus the chain. Pressing a symbol crumb reveals its line.
  - On snapshots, `ReadOnlyBanner::new("read-only", origin.label())` replaces `render_snapshot_bar`. Its `.action("Open local file", …)` is present when `local_copy` exists and emits `OpenLocalCopy`.
  - `state` gains `breadcrumb ("src › main.rs › impl Foo › bar")` and `banner`.

- [ ] **Step 1: Write the failure list, then the tests,** in `breadcrumb.rs`:
  - `a_caret_before_every_symbol_has_no_chain`
  - `a_caret_inside_a_method_names_its_impl_and_method`
  - `a_shallower_symbol_closes_the_deeper_ones`
  - `siblings_replace_each_other`
- [ ] **Step 2: Run** `cargo nextest run -p sirio_ui breadcrumb`. Expected: FAIL. Check that 4 are selected.
- [ ] **Step 3: Implement** `chain`, the breadcrumb row, the app-side fetch, and the banner.
- [ ] **Step 4: Write the E2E scenarios.**
  - `breadcrumb`: the fake LSP answers `documentSymbol`; `select` inside `bar` → `breadcrumb` ends with `bar`.
  - `snapshot`: open a change-request snapshot as `test-forge-diff-e2e.sh` does, or a debug verb `surface.editor.open_snapshot sha= path=` backed by `FileView::snapshot`.
    - `banner` is set.
    - `type`, `keys ctrl-v`, `context action=cut` and `replace all=true` each leave `text` unchanged (Review Focus 4).
    - A Markdown snapshot's format buttons are absent.
  - Capture both.
- [ ] **Step 5: Run** the unit tests and `test-editor-e2e.sh --xvfb`. Expected: PASS and `EDITOR E2E OK`.
- [ ] **Step 6: Commit.** `feat(editor): show where the caret is and mark read-only snapshots`

### Task 11: The status row

**Files:**
- Create: `rust/crates/sirio_ui/src/file_view/status_row.rs`
- Modify: `rust/crates/sirio_git/src/{status.rs or remote.rs, lib.rs}` (`ahead_behind`), `rust/crates/sirio/src/{main.rs,lsp.rs}` (LSP state push), `rust/crates/sirio_ui/src/file_view.rs`, `Scripts/Tests/test-editor-e2e.sh`

**Interfaces:**
- Produces:
  - `sirio_git::ahead_behind(repo: &Path) -> Result<Option<(usize, usize)>, GitError>`, via `git rev-list --left-right --count @{upstream}...HEAD`. `None` when there is no upstream.
  - `pub struct StatusFacts { pub branch: Option<String>, pub worktree_dirty: bool, pub ahead: usize, pub behind: usize, pub lsp: Option<(String, LspState)> }` and `FileView::set_status_facts(&mut self, facts: StatusFacts, cx)`.
  - LSP mapping, done in `main.rs`:
    - launch in flight → `Starting` (the new `LspSupervisor::starting: HashSet<key>`, set in `start_language_server_for` before spawning);
    - in `servers` → `Ready`;
    - `Dead::Failed` → `Failed(message)`. Today nothing notices a server that dies after a successful start, so in project 1 `Failed` covers launch failures only. Crash detection is project 3's.
    - `Dead::NotInstalled | Installable | Manual`, or plain text → `None`, and the item is absent.
  - Per-tab overrides on `FileView`:
    - `language_override: Option<Language>`. This is colour only and the server stays the path's: `sirio_lsp::LanguageTable::for_path` picks the server from the path and never sees `Language`. This deviates from spec §5.6's "colour and server follow"; moving the server is project 3's.
    - `indent: (spaces: bool, size: usize)`, defaulting to `(true, prefs.tab_size)`;
    - the line ending through `Editor::set_line_ending`.
  - `notifications: Vec<SharedString>`. Every `set_message` / `offer` appends to it; the bell's count is its length, and pressing it opens a list popover. Opening the list clears the count, not the list.
  - Cursor press opens a go-to-line `TextInput` popover; Enter selects that line's start.
  - Verbs: `surface.editor.status item=language|line_ending|indent|goto|bell value=`. `state` gains `status` (`branch=…;ahead=…;behind=…;lsp=…;cursor=Ln a, Col b (n selected);indent=Spaces: 4;encoding=UTF-8;eol=LF;language=Rust;bell=N`).

- [ ] **Step 1: Write the failure list, then the tests,** in `sirio_git`'s tests with a temp repo and a bare upstream:
  - `no_upstream_is_none`
  - `commits_on_both_sides_count_each`
  - `a_detached_head_is_none`
- [ ] **Step 2: Run** `cargo nextest run -p sirio_git ahead_behind`. Expected: FAIL. Check that 3 are selected.
- [ ] **Step 3: Implement** `ahead_behind`, `StatusFacts`, the push from `main.rs` (on `Loaded`, on save, on worktree switch, on LSP state change), the row with `StatusBarItem`s in the order Branch, LSP, then right-aligned Cursor, Indent, Encoding, Line ending, Language, Bell, and the four interactions.
- [ ] **Step 4: Write the E2E scenarios.**
  - `status`: the fixture repo has an upstream one commit behind and one ahead, so expect `ahead=1;behind=1`.
  - `cursor` after `select` with two ranges reads `(n selected)`.
  - `status item=line_ending value=crlf` then `save` writes CRLF.
  - `status item=indent value=tabs:4`, then `keys tab` inserts `\t`.
  - `status item=language value=plain` drops the colour; capture.
  - `status item=goto value=40` → selection on line 39.
  - Plain text shows no `lsp`, and neither does a `.zig` file with no server (Review: CLAUDE.md rule).
  - The fake LSP → `lsp=fake:ready`.
- [ ] **Step 5: Run** the unit tests and `test-editor-e2e.sh --xvfb`, dark and light. Expected: PASS and `EDITOR E2E OK`.
- [ ] **Step 6: Commit.** `feat(editor): add ely's status row to the file tab`

### Task 12: Performance, documentation, final proof

**Files:**
- Create: `docs/perf/2026-10-ely-code-editor.md`; modify `Scripts/perf/` only if a typing or scroll driver is missing
- Modify: `CLAUDE.md` (a short "File editor on Ely" section and the `test-editor-e2e.sh` / `test-ely-editor-probe.sh` command lines), `rust/vendor/ely-gpui-component/LOCAL-CHANGES.md` (final pass)

- [ ] **Step 1: Measure.**
  - With `SIRIO_PERF_TRACE`, on `big.txt` (1.1 MiB):
    - type 200 characters through `surface.editor.type`, one per call;
    - scroll from top to bottom with `surface.editor.select`, 50 lines per step.
  - Do it on this branch and on `main` (`git worktree add`, with the worktree's own target).
  - Report the median and p95 of the `FileView` render span and of the frame time.
- [ ] **Step 2: Decide.** If either median or p95 is worse than `main`, stop and bring the numbers to the user (spec §5.8). Otherwise record both runs, with their commands and hashes, in `docs/perf/2026-10-ely-code-editor.md`.
- [ ] **Step 3: Update `CLAUDE.md`.** Describe the file editor in four to six sentences, in the house style: `CodeEditor` owns the text and `Editor` owns the disk; LF buffer and line ending; per-line tree-sitter colour through the hook; the vendored pin and its ported fixes. Add the two new scripts to Commands.
- [ ] **Step 4: Run the full proof.**
  - `Scripts/Tests/test-editor-e2e.sh --xvfb --appearance dark --out-dir artifacts/editor-dark`, the same for light, `Scripts/Tests/test-ely-editor-probe.sh --xvfb`, and `Scripts/Tests/test-ely-chat-ui-e2e.py`.
  - Expected: every OK line. Look at the captures against the prototype's scene 1 and scene 4.
- [ ] **Step 5: Commit.** `docs: record the ely editor's performance and how it is built`. Then ask the user to run `Scripts/ci.sh` before the PR. The PR description lists the platform rows seen by hand: macOS and Windows keys and IME are not covered by this run (spec §8).
