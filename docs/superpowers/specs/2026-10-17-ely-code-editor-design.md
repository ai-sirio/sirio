# The file editor on Ely — design

**Date:** 2026-10-17
**Status:** proposed
**Programme:** the file tab's code surface becomes Ely GPUI Components'
`CodeEditor`, and everything Ely's *Editor* section offers that applies to a
file tab is brought into Sirio, on Sirio's own engines. Five projects (§3),
each with its own plan and pull request. This document fixes the programme
and designs **project 1** in full; projects 2–5 each get their own design
document, written from §3's boundaries.
**Related:** the bezel → Ely migration (SP1 merged as #600,
`2026-10-01-sirio-theme-on-ely-palette-design.md`), the Ely chat
(`2026-09-30-ely-agent-chat-design.md`), change requests on Ely
(`2026-10-03-change-requests-on-ely-design.md`), language coverage
(`2026-09-16-language-coverage-design.md`), Markdown preview
(`2026-09-23-markdown-rich-preview-design.md`).
**Prototype:** `2026-10-17-ely-code-editor-prototype.html` beside this
document — the file tab in five states (at rest, typing, understanding and
fixing, find and the open drawer, agents and Vim). Open it in a browser.

## §0 Intent

The user asked to refactor Sirio's text editor onto what Ely GPUI Components
provides — the `CodeEditor` family (gutter, line numbers, folding, minimap,
ruler, indent guides, bracket matching, rainbow brackets, multi-cursor,
selection highlight, sticky scroll, breadcrumb, inlay hints, code lens,
inline decorations), the completion menu with signature help and hover, the
find and replace widget, the problems and output panels, and the status
items (cursor position, language mode, encoding, line ending, indent
settings, branch indicator, LSP status, notification bell) — and for
prototypes of the result. Asked what "only Ely" means where Ely does less
than Sirio already does, they chose **Ely's look over Sirio's engines**:
tree-sitter colours and the Markdown preview stay. They also asked that
**what Ely has and Sirio lacks** be brought in too, and accepted the list in
§2.

Today's editor (`sirio_ui::file_view`, ~6300 lines, over the pure model
`sirio_ui::editor`) is hand-drawn GPUI: one caret, no undo, keys handled by
hand in `on_key_down`, bezel-syntax colours, no editor settings.

Success, for the programme:

- A file tab in Sirio looks and behaves like the prototype: every component
  in §2 is drawn by Ely, coloured by tree-sitter, fed by `sirio_lsp`,
  `sirio_git` and Sirio's agents.
- Nothing the file tab does today is lost (§5.4).
- Every project ends with an end-to-end script against a real, isolated
  Sirio and PID-matched window captures in light and dark, so each step is
  **seen**, not only read through report keys.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Library | Ely GPUI Components, vendored like the chat. No new bezel widget. |
| Ely vs Sirio engines | **Ely's look over Sirio's engines.** Colour stays tree-sitter (`sirio_syntax` through `Theme::syntax_palette`); the Markdown preview stays (`markdown_preview`, `sirio_diagram`); navigation stays `sirio_lsp`. |
| What Ely has and Sirio lacks | Brought in, except what has no engine in Sirio (§2, "Left out"). |
| Layout | **Everything inside the file tab** (prototype option A): breadcrumb, code with gutter and minimap, find widget floating top-right over the code, a closable **Problems / Output drawer** at the bottom of the tab, and Ely's **status row** under it. The app's global status bar is unchanged. |
| Branch in the status row | Kept, although the global bar shows it, for fidelity to Ely. |
| Problems drawer scope | The open file's problems (my reading, confirmed by the user's "va bene" on the playback). |
| Output drawer content | The log of that file's language server. |
| Worktree-wide search | A **right-panel view** beside Files and References, not a drawer tab. |
| Delivery | Five projects in the order of §3. |
| Defaults and settings | On by default: minimap, sticky scroll, indent guides, rainbow brackets, code lens, inlay hints. Off: visible whitespace, ruler. A new **Settings → Editor** page with a switch for each, plus line numbers (absolute, relative, hidden), cursor shape (line, block, underline), ruler column and tab size. |

## §2 Component inventory

Every component of Ely's *Editor* section, with the project that brings it
and the engine that feeds it.

| Ely component(s) | Project | Fed by |
|---|---|---|
| `CodeEditor`, `EditorGutter`, `LineNumbers`, `FoldingControl`, `MiniMap`, `Ruler`, `IndentGuide`, `BracketMatcher`, `RainbowBrackets`, `MultiCursor`, `SelectionHighlight`, `StickyScroll` | 1 | Ely's own engine; colours from tree-sitter (§5.2) |
| `WhitespaceRenderer`, `CursorRenderer` | 1 | Ely |
| `Breadcrumb` | 1 | path segments, then `sirio_lsp` document symbols when a server answers |
| `InlineDecoration` (diagnostics) | 1 | `sirio_lsp` `publishDiagnostics` (already wired to `FileView::set_diagnostics`) |
| `InlineDiff`, gutter git marks | 1 | `sirio_git` against `HEAD`; pressing a git mark opens that hunk inline (§5.4) |
| `ReadOnlyBanner` | 1 | change-request snapshot tabs |
| `FindWidget`, `ReplaceWidget` | 1 | Ely's `find_all` over the buffer |
| `CursorPosition`, `LanguageMode`, `Encoding`, `LineEnding`, `IndentSettings`, `BranchIndicator`, `LspStatus`, `NotificationBell` | 1 | §5.6 |
| `ProblemsPanel`, `DiagnosticsList`, `OutputPanel` (the drawer) | 2 | diagnostics; the language server's log |
| `SearchPanel`, `SearchResultItem`, `SearchFilters` (right panel) | 2 | a worktree search engine (project 2 decides: `git grep` or a walk) |
| `CompletionMenu`, `SignatureHelp` | 3 | **new** `textDocument/completion`, `signatureHelp` in `sirio_lsp` |
| `HoverInfo` | 3 | `textDocument/hover` (exists) |
| `CodeActionMenu`, `RenameInput` | 3 | **new** `codeAction`, `rename` (+ applying a `WorkspaceEdit`) |
| `InlayHints`, `CodeLens` | 3 | **new** `inlayHint`, `codeLens` |
| `PeekView` | 3 | `definition` (exists) |
| `SymbolOutline`, `GoToSymbol` | 3 | `documentSymbol` (exists; Go to Symbol exists as `sirio_ui::outline`) |
| `ReferencesPanel` | 3 | `references` (exists; restyles the right panel's References view) |
| `CallHierarchy`, `TypeHierarchy` | 3 | **new** `callHierarchy`, `typeHierarchy` |
| `InlineChat` | 4 | a Sirio chat in the same worktree, answering as an inline diff |
| `GhostText` | 4 | an agent or model chosen in project 4's brainstorm |
| `VimModeIndicator` | 5 | a Vim mode, project 5's own engine |

**Left out** — no engine in Sirio to attach them to: `DebugConsole` and
gutter breakpoints (no debugger), `ExtensionsPanel` / `PluginList` (no
extensions), `SettingsEditor` / `KeybindingsEditor` (Sirio has its own
Settings), `WelcomePage`, `RecentProjects`, `ProjectSwitcher`,
`ThemePreview`, `TaskRunner`, `RunConfiguration` (IDE pages, not the editor).

## §3 The five projects

Each depends on the one before it unless noted.

1. **Foundations** (this document, §4–§9). Ely's editor module vendored
   (§5.1); the code surface of `FileView` replaced by `CodeEditor`
   with tree-sitter colours; every component in §2 marked project 1; the
   Settings → Editor page.
2. **Panels.** The Problems / Output drawer inside the file tab, and
   worktree search as a right-panel view (`PanelView::Search`, Ctrl/Cmd+Shift+F,
   available with no file open).
3. **Language server.** The new `sirio_lsp` requests of §2 and the Ely UI
   for them and for what exists (hover, peek, outline, references).
4. **Agents in the editor.** `InlineChat` bound to Sirio's chats, and
   `GhostText`. Its brainstorm decides which agent or model writes
   suggestions and what leaves the machine.
5. **Vim mode** and its indicator. Independent of 2–4; can move.

## §4 Project 1 — what it delivers

At the end of project 1 a file tab draws, top to bottom:

- the **breadcrumb**;
- the **code**: gutter (git marks, diagnostic icons, line numbers, fold
  chevrons), code with indent guides, rainbow brackets, matched-bracket
  frame, selection highlight, diagnostic underlines with the message at the
  line's end, the optional ruler, sticky scroll, multi-cursor, and the
  **minimap** with its viewport and its error and git marks;
- **InlineDiff**: pressing a git mark in the gutter opens that hunk in
  place — the removed lines drawn above the current ones — and pressing it
  again or Escape closes it (`CodeEditor::set_diff`, one `sirio_git` read per
  open hunk);
- the **find / replace** widget floating top-right over the code;
- the **ReadOnlyBanner** on a change-request snapshot;
- the **status row**.

The drawer is not part of project 1: the status row sits directly under the
code until project 2 inserts the drawer between them. Code lens and inlay
hints are drawn by `CodeEditor` but have no data until project 3; their
switches exist in Settings from project 1 so the page does not change shape
later.

## §5 Project 1 — design

### §5.1 Vendoring

The vendored crate (`rust/vendor/ely-gpui-component`) is at upstream
`e17e31a6` and excludes the `editor` module, which that revision already
contains. Upstream moved: the GitHub repository is gone, and the source now
lives at `https://git.zacharyzhang.com/ZacharyZhang-NY/Ely-GPUI-Components`
with a rewritten history: `e17e31a6` is still in its object store but no
longer an ancestor of `main` (`2e65b05a` on 2026-10-16). Between the two
trees the shared modules the chat and the change-request surfaces use
(`chat`, `agent`, `buttons`, `menus`, `theme`, `navigation/tabs.rs`, `git`,
`overlays/popover.rs`) changed in 45 files (+3752/−1015), including
deliberate restyles.

**My call: vendor the editor from the pinned tree `e17e31a6`, and port the
editor-only fixes from `main` as recorded local changes.** The rest of the
crate does not move, so the chat and the change-request surfaces cannot
change. Concretely:

1. `LOCAL-CHANGES.md` gets the new upstream URL (the current provenance line
   is a dead link) and a note that `e17e31a6` is off `main`, so the vendored
   copy is the record.
2. Add the `editor` module's closure for project 1 from `e17e31a6`, behind
   the crate's module selection (the `sirio-chat` build's selection or a
   sibling feature, the builder's choice): `state`, `buffer`, `cursor`,
   `edit`, `keys`, `moves`, `measure`, `line`, `row`, `view`, `layout`,
   `gutter`, `decor`, `syntax`, `anchors`, `ime`, `find`, `status`, plus
   `navigation::breadcrumb` and whatever of their imports the closure lacks.
   Later projects add the files they draw (`hints`, `hover`, `refactor`,
   `symbols`, `search`, `filters`, `problems`, `console`, `chat`). §2's
   "Left out" files are never vendored.
3. Port from `main`, each as a recorded local change: the editor's panic
   fixes (owner offsets ride the text's edits; stale ranges, rows and globs
   log instead of panicking), the measurement fixes (columns sit where gpui
   lays them out; measure with the current text, the line end is a stop),
   and *Escape from the find query closes the box*. Not ported: the
   documents / writing-view / typewriter work and i18n catalog changes.
4. Every API the editor files use that Sirio's `bezel-gpui` lacks is adapted
   locally and recorded, the way `HoverListenerMode` was.

**Proof that nothing else moved.** `Scripts/Tests/test-ely-chat-ui-e2e.py`,
`test-ely-forge-probe.sh`, `test-forge-ui-e2e.sh`, `test-forge-diff-e2e.sh`,
`test-changes-e2e.sh` and `test-text-selection-e2e.sh` pass with their steps
unchanged. No capture comparison is needed: the files those surfaces draw
from are not touched (the plan checks that with a diff of the vendored tree
outside `src/editor` and the closure's new files).

### §5.2 Colour: tree-sitter into `CodeEditor`

Today `file_view` colours **each line on its own**: `code_spans(language,
line)` calls `sirio_syntax::highlight(line, language.fence_tag())` and maps
each `HighlightKind` through `Theme::syntax_palette`. Upstream's
`CodeEditor::highlighter` hook has exactly that shape — `Fn(&str, &App) ->
Vec<(Range, Highlight)>`, one line's text — so tree-sitter plugs in with no
change to Ely: the hook calls `code_spans` for the tab's `Language`. Colours
are therefore what they are today, line for line, including today's limits
(a string or comment spanning lines is coloured per line). A
whole-document parse would be an improvement, not part of this programme.

Plain text gets no colour, as today. Ely's own lexer is never used for a
file. The three agreement tests named in CLAUDE.md
(`every_language_the_editor_recognises_has_a_grammar` and its siblings) stay
green and keep meaning what they mean.

Rainbow brackets and bracket matching use Ely's own bracket scanner
(`editor::syntax`) over the buffer. Folding uses Ely's indentation folds;
Sirio has no fold engine today, and tree-sitter folds are a possible later
improvement, not part of this programme.

### §5.3 Who owns the text

`CodeEditor` owns the buffer, the selections and undo. `sirio_ui::editor::
Editor` keeps everything about the file on disk: path, load status and its
failure classes, the disk baseline, dirty state, conflict detection,
`MAX_FILE_BYTES`, language detection, the Markdown document model. One
direction of truth:

- `EditorEvent::Changed` → `FileView` reads `CodeEditor::text()` into
  `Editor`, which recomputes dirty and notifies the LSP (`didChange`) as the
  current edit path does.
- Load, reload, *Keep* and conflict resolution → `CodeEditor::reset` (no
  undo across a reload), never `set_text` mid-history.
- Save writes `CodeEditor`'s text with the chosen line ending (§5.6).

Undo/redo come from `CodeEditor`'s history with upstream's byte budget
(64 MiB); with `MAX_FILE_BYTES` at 1 MiB that is at least 64 steps of a
maximal file.

### §5.4 What the file tab keeps

Each of these works after the swap as before; where its drawing changes, it
is redrawn on Ely:

- the load states and their distinct messages (missing, unreadable, binary,
  too large) and the snapshot states (pending, failed with Restart);
- change-request snapshots: read only (`CodeEditor::read_only`) with
  `ReadOnlyBanner`, whose action is the existing *Open local file* where it
  applies;
- the conflict banner (*Reload* / *Keep*) and `check_external` on
  activation;
- the Markdown row: formatting controls and the Code / Preview switch; the
  preview is Sirio's (`markdown_preview`, diagrams, `SelectableMarkdown`);
  Markdown **source** mode is a `CodeEditor`;
- links in Markdown source stay clickable (`markdown_links_in_line`), drawn
  as an inline decoration;
- hover cards and go-to-definition, with today's sequence counter;
- the file context menu (`file_context_menu`), including the missing-server
  entry;
- Go to Symbol (`sirio_ui::outline`);
- the message strip with actions (`set_message`, `offer`) — its messages
  also land in the notification bell (§5.6);
- `reveal_at(line)` and every `FileViewEvent` the shell listens to;
- the text-selection contract of CLAUDE.md: the editor keeps its own
  selection and copy; Ctrl/Cmd+C in the editor copies the editor's
  selection and is never swallowed when there is none.

### §5.5 Keys

Ely's `bind_keys` brings the editor's bindings (Cmd/Ctrl+D next match,
Cmd/Ctrl+Alt+Down add cursor, Alt+click add cursor, Cmd/Ctrl+/ comment, Tab
indent, undo/redo, …) scoped to the editor's key context. Rules:

- Every shell binding that works with a file tab focused today (save, close
  tab, palette, Go to Symbol, …) still works; where an Ely binding collides
  with one, the shell's wins and the Ely binding is moved, recorded in
  `LOCAL-CHANGES.md` and listed in the plan.
- Find is Cmd/Ctrl+F; replace is Cmd+Alt+F on macOS and Ctrl+H elsewhere;
  Escape closes the widget and returns focus to the code.
- A terminal's Ctrl+C stays SIGINT (the editor's bindings are scoped to the
  editor).

### §5.6 The status row

Ely's status items, inside the file tab, under the code:

| Item | Shows | Pressing it |
|---|---|---|
| `BranchIndicator` | the worktree's branch, `*` when dirty, ahead/behind | nothing in project 1 |
| `LspStatus` | the file's server state from `sirio_lsp`'s lifecycle, mapped onto Ely's `LspState`: starting → `Starting`, running → `Ready`, crashed or stopped → `Failed`. **Absent** for plain text and when the language's server is not installed: CLAUDE.md's rule that a missing server is never volunteered holds; the context menu stays the one place that names it | steps through the states' details (Ely's behaviour); progress ("indexing 214 files") needs `$/progress`, which is project 3 |
| `CursorPosition` | `Ln, Col` and "(n selected)" across all cursors | opens a go-to-line input |
| `IndentSettings` | "Spaces: 4" / "Tab: 4" | switches width and spaces/tabs for this tab |
| `Encoding` | "UTF-8" | nothing: Sirio reads only UTF-8 (anything else is `Binary`) |
| `LineEnding` | LF or CRLF, detected at load | switches; the next save writes the chosen ending |
| `LanguageMode` | the detected `Language` | overrides it for this tab (colour and server follow) |
| `NotificationBell` | count of this tab's messages | lists them: the message strip's messages and, from project 3, the server's `window/showMessage` |

### §5.7 Settings → Editor

A new page in Settings, drawn with **Ely components** (switch, select,
number field, vendoring whichever of them the closure lacks). It is the
first Ely page inside a Settings screen whose other pages still use Sirio's
and bezel's controls; that follows the rule set in
`2026-10-03-change-requests-on-ely-design.md` §1 — Ely only for every new
surface — and the rest of Settings moves with the bezel → Ely migration's
Settings sub-project, not here. Its spacing and typography match the
neighbouring pages. Contents:
switches for minimap, sticky scroll, indent guides, rainbow brackets, code
lens, inlay hints, visible whitespace, ruler; a ruler column; line numbers
(absolute / relative / hidden); cursor shape (line / block / underline); tab
size. Defaults per §1. Stored through `sirio_persistence` like the other
settings, applied live to every open file tab.

### §5.8 Performance

`CodeEditor` draws with `uniform_list`; the tree-sitter parse runs off the
render path once per buffer revision. Each new `Render::render` gets a
`sirio_perf` span under the crate's rule (content-free `&'static str`
names). The plan measures typing latency and scroll on a 1 MiB file against
today's editor with `Scripts/perf/`, and records the numbers in
`docs/perf/`. Not a fixed-number gate: if either is worse than today's, the
work stops and the numbers come back to the user before merging.

## §6 Project 1 — proof

End to end, in the shape of the existing scripts:
`Scripts/Tests/test-editor-e2e.sh` (prints `EDITOR E2E OK`), driving a real,
isolated debug Sirio over the control socket, with `--out-dir`,
`--state-only`, `--xvfb` / `--display :N` and `--appearance light|dark` like
its siblings. It needs debug-only `surface.editor.*` verbs (open a file,
type, place cursors, run find/replace, toggle a setting, read the editor's
state), absent from release `capabilities`, following the `surface
change-request act` precedent.

Scenarios: every load state; type, undo, redo, multi-cursor edit and save,
read back from disk; CRLF file switched to LF and saved; tree-sitter colours
on a Rust file and a multi-line string; diagnostics drawn in gutter, code
and minimap; git marks after an edit against `HEAD`, and a hunk opened
inline from its mark and closed; find and replace all;
snapshot read only with the banner; Markdown source ↔ preview; hover and
definition against a stub language server; each Settings → Editor switch
seen in a capture; conflict *Reload* / *Keep*.

Captures are PID-matched window grabs in light and dark, and the run's
artifact (transcript + captures + the files read back) is what a reviewer
re-runs and checks.

Existing `#[test]`s in `file_view.rs` that pin behaviour (load states,
conflict, save, links, hover sequencing) are kept and pointed at the new
surface; those that pin the old hand-drawn surface's internals go with it.
New unit tests only for a unit proven in isolation, failure list first
(CLAUDE.md).

## §7 Out of scope for project 1

The drawer, worktree search (project 2); any new LSP request, including
`$/progress` (project 3); agents in the editor (project 4); Vim (project 5);
tree-sitter folds; encodings other than UTF-8; `bezel-editor` removal (SP7
of the bezel → Ely migration); Ely's `MarkdownDocument` (Sirio keeps its
preview).

## §8 Risks the plan verifies first, with the fallback decided

- **gpui gap.** If the editor files need a gpui change rather than local
  shims on `bezel-gpui`, stop and bring it back to the user before writing
  editor code.
- **Ported fixes that do not apply.** A fix from `main` that depends on
  code outside the editor closure is rewritten against `e17e31a6` or left
  out; either way `LOCAL-CHANGES.md` says which.
- **Performance.** §5.8: worse than today stops the work.
- **Unverified platforms.** macOS and Windows key handling and IME are
  checked by hand on the release gate, as for the chat; the PR says which
  rows were seen.

## §9 Calls made by me, not by the user

- Vendoring the editor from the pinned `e17e31a6` with editor-only fixes
  ported from `main`, rather than re-vendoring the crate (§5.1) — so the
  chat and change-request surfaces cannot change.
- Colour stays per line exactly as today, through Ely's existing hook
  (§5.2).
- Ely's indentation folds, not tree-sitter folds (§5.2).
- `CodeEditor` owns the text; `Editor` owns the disk (§5.3).
- InlineDiff in project 1 opens a hunk from its gutter mark (§4).
- Shell bindings win key collisions (§5.5).
- Status-row behaviour of each item (§5.6): Encoding display-only, LSP
  progress deferred to project 3, LspStatus absent when no server is
  installed (CLAUDE.md's rule).
- The message strip's messages feed the notification bell (§5.6).
- Settings → Editor drawn with Ely components (§5.7).
- Performance is compared with today, not gated on a number (§5.8).
- The E2E's name, verbs and scenarios (§6).
