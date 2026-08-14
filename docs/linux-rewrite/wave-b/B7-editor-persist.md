# Wave B slice B7-editor-persist — 8 rows to build

**editor, file view, persistence, terminal, session**

## Files you own this wave

- `rust/crates/tiller_ui/src/editor.rs`
- `rust/crates/tiller_ui/src/file_view.rs`
- `rust/crates/tiller_persistence/src/migrations.rs`
- `rust/crates/tiller_terminal/src/lib.rs`
- `rust/crates/tiller/src/session.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-EDIT-02` — ledger line 220, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/file_view.rs, rust/crates/tiller_ui/src/editor.rs
- **Approach:** format_bold/format_italic (editor.rs toggle_wrap) are correct pure string ops. The real gap: file_view.rs's on_mouse_down only calls .focus(), with zero pixel-to-buffer-offset caret placement or drag-selection logic anywhere — the only way source_selection becomes non-None is keyboard Shift+Arrow. A mouse click/drag 'selection' silently falls back to Selection::point(buffer.len()), so formatting lands at EOF, invisible to the driver. Needs real mouse-driven caret placement + drag-select (+ ideally double-click word-select) wired into source_selection the same way move_caret already does for keyboard.
- **Evidence on record:** P104 §Group 2: Bold left the selected word unchanged; Italic wrapped the entire line rather than the selected word, and ctrl-z did not undo it.

### `F-CORE-FILE-04` — ledger line 389, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_ui/src/file_view.rs, rust/crates/tiller_ui/src/editor.rs, rust/crates/tiller_project/src/file_link.rs
- **Approach:** resolve_file_link (file_link.rs:12) is correct and tested but has zero callers outside its own file. No click/cmd-click handler exists anywhere on a rendered Markdown link span (only Editor::make_link, which writes new links, was found). Mirror the terminal's existing link-click convention (tiller_terminal/src/link_router.rs's opens_terminal_link, platform-modifier gesture per the P82 SEAMS.md ruling): add a click handler on rendered link spans that calls resolve_file_link and opens the resolved target through the existing open-document path (same one F-EDIT-10's right-click Open uses).
- **Evidence on record:** `resolve_file_link` (file_link.rs:12) has zero callers outside its own crate and no click-to-open machinery exists (cmd/ctrl-click|open_link|hovered_link: zero); the link tests are real, nothing wires a click to them (FABLE-04 overturn, re-swept pass 14)

### `F-PERSIST-DB-06` — ledger line 509, currently **half-proven**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_persistence/src/migrations.rs, rust/crates/tiller_persistence/src/db.rs, rust/crates/tiller_usage/src/account.rs, rust/crates/tiller_ui/src/settings.rs
- **Approach:** session_ref half is proven. Account half has no store at all: zero migrations create an account table, zero db.rs functions save/load one. discover_claude_identity/discover_codex_identity only shell out live for a Settings display line, never touching a database. Add an account table via a new migration, wire the discovery functions to persist on successful detection, and have the Settings restore path read from the store.
- **Evidence on record:** Confirmed independently: tiller_usage/src/account.rs exists with one caller (tiller_ui/src/settings.rs:548, discover_claude_identity), read directly and confirmed to be a live 'claude auth status' shell-out for a Settings display line only -- no INSERT/SELECT against any account table anywhere in tiller_persistence/src. Session-ref half unchanged; account half still has no store to look up on rest

### `F-PERSIST-DB-11` — ledger line 514, currently **half-proven**

- **Triage:** exercise, size M
- **Files triage expects:** rust/crates/tiller_persistence/src/migrations.rs, rust/crates/tiller_persistence/tests/persistence_integration.rs
- **Approach:** Schema-creation half is now strongly proven (real file, v1->v12, matches 12 migrations). No existing test plants data at an old schema version in chat_turn/session_ref/tab-ordering columns and checks it survives forward migration through the renames/backfills those specific migrations perform. Needs a new test doing exactly that.
- **Evidence on record:** half-proven (unchanged, owed half narrowed). Schema half reconfirmed stronger (real v1->v12 migrated boot, populated sidebar). Data half: session_ref and tab.agent_id verified surviving migration verbatim; chat_turn content specifically does not restore after boot in either the migrated case or a fresh-v12 no-migration control (same emptiness both ways) -- a reproducible, schema-independent findin

### `F-PER-06` — ledger line 242, currently **FAILED — defective**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller_terminal/src/lib.rs
- **Approach:** terminate_process_group (lib.rs:513-529) does a single killpg on the shell's own pgid captured once at spawn (lib.rs:345). Job-control-spawned children (compound commands, backgrounded jobs) detach into a new process group that this single killpg never reaches. Needs to enumerate the shell's actual descendant process groups at kill time (e.g. walk /proc/<pid>/task/*/children recursively) and killpg each distinct group found, not just the one captured at spawn. Same root as F-TERM-08 (WORK-BREAKDOWN D-2, not in this group).
- **Evidence on record:** compound-command panes orphan process groups on quit (pass 6); simple panes flush (pass 4)

### `F-TERM-PTY-06` — ledger line 531, currently **NOT EXERCISED**

- **Triage:** both, size M
- **Files triage expects:** rust/crates/tiller_terminal/src/lib.rs
- **Approach:** receive_file_drop + on_drop::<PathBuf> is built and drawn-tested but is GPUI's in-app typed-drag payload, not OS-level XDND (grep for ExternalPaths returns nothing in the whole workspace). Exercise the in-app path (drag a Files-panel row onto a terminal, not blocked by the XDND environment limitation); build real gpui::ExternalPaths support for the row's literal OS-drop spec.
- **Evidence on record:** P116's evidence is generic panel write/key PTY text injection, not terminal_file_drop/receive_file_drop. Same report's F-CORE-FILE-03 entry confirms the socket has no file-drop method; ENVIRONMENT.md confirms XDND is unexercisable by this harness. Instrument-blocked status unchanged.

### `F-PRJ-17` — ledger line 110, currently **FAILED — absent**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** Add default_worktree_base to CatalogProjectSettings (session.rs:375-380), thread it through write_catalog (:787-839, currently never sets record.default_worktree_base) and restore_catalog (:841-898, currently never reads it back) so it round-trips the already-migrated DB column; add a default-base control to render_project_settings; at confirm_worktree_prompt (sidebar.rs:1333-1373) pass the real value into create_worktree's base:Option<&str> arg instead of the hard-coded None at :1366.
- **Shared cause:** One seam with F-PRJ-18: tiller_git/tiller_persistence/tiller_project already fully support both settings; only session.rs's CatalogProjectSettings + sidebar.rs's UI/consuming call sites are missing. See group notes for the full three-layer trace.
- **Evidence on record:** Live-driven: New Worktree popover has exactly one control, a branch-name field — no default-worktree-base option (current/pinned/primary/no-primary) exists, before or after typing. shots/34.

### `F-PRJ-18` — ledger line 111, currently **FAILED — absent**

- **Triage:** build, size M
- **Files triage expects:** rust/crates/tiller/src/session.rs, rust/crates/tiller_ui/src/sidebar.rs
- **Approach:** Add worktree_location_override to CatalogProjectSettings and thread it through write_catalog/restore_catalog the same way as F-PRJ-17's base field; add a location-chooser + restore-default-parent control to render_project_settings; at confirm_worktree_prompt pass the real override into resolve_parent_directory's override_dir:Option<&Path> arg instead of the hard-coded None at sidebar.rs:1354.
- **Shared cause:** One seam with F-PRJ-17; same three-layer trace, same two files, best built together.
- **Evidence on record:** Live-driven: same popover as F-PRJ-17, only the branch-name field — no location chooser or default-parent control exists anywhere in this flow. shots/34.

