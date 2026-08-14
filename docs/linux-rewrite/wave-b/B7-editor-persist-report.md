# B7-editor-persist — build report

Slice: `docs/linux-rewrite/wave-b/B7-editor-persist.md`
Owned files: `tiller_ui/src/editor.rs`, `tiller_ui/src/file_view.rs`,
`tiller_persistence/src/migrations.rs`, `tiller_terminal/src/lib.rs`,
`tiller/src/session.rs`.

No verdicts are stated here; that is the orchestrator's call against
`INVENTORY-LEDGER.md`, not mine.

## Incident: a sibling commit silently reverted a committed file in this slice

While working F-TERM-PTY-06, `tiller_terminal/src/lib.rs` was found to have
lost the already-committed F-PER-06 fix (`terminate_descendant_process_groups`,
`descendant_pids`, `descendant_process_groups`, `PidGuard`, and the
`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
test) — `git log -- rust/crates/tiller_terminal/src/lib.rs` shows a later
commit `7c8c787` ("feat(chat): recover chip-removal focus, wire file drop,
wire Follow Edited Files", about `chat.rs`) whose diff on this file is a
byte-for-byte revert back to the pre-fix version. This is a shared working
tree, not per-agent isolation, so file ownership is a *convention* other
agents can still violate by committing a stale copy. I re-applied F-PER-06
verbatim, re-verified it, and recommitted (`915b0d0`) rather than editing
history. Verified all five owned files still carry my content after every
subsequent commit in this session; none reverted again.

## Rows

### F-EDIT-02 — real mouse-driven caret/selection (built)

The per-line click handler only ever produced a whole-line selection, and
once `source_selection` was `None` (e.g. after a plain click or an
un-shifted arrow key), `formatting_selection`'s fallback was hard-coded to
`Selection::point(buffer.len())` — end of buffer — instead of the tracked
caret. Both halves are now fixed:

- `EditableLine`, a GPUI `Element` wrapping each source line's `StyledText`
  (modeled on `Chat`'s `TranscriptSelectableText`), replaces the old
  per-line `div` + `render_code_spans`. It handles mouse down/move/up via
  `index_for_position`/`position_for_index`: a click places a collapsed
  caret at the clicked buffer offset, a drag extends it exactly like
  Shift+Arrow (`FileView::begin_mouse_selection` /
  `extend_mouse_selection` / `end_mouse_selection`), and a double-click
  (`event.click_count >= 2`) selects the touched word via new
  `editor::word_range_at`.
- `formatting_selection`'s fallback now mirrors `current_selection`'s
  (collapse to `self.caret`), not end-of-buffer.
- It also paints the exact selected sub-range per line (via
  `position_for_index`), not the old whole-line background — a selection
  spanning several lines now highlights only what is actually selected on
  the first/last line.

### F-CORE-FILE-04 — clickable rendered link labels (built, main.rs wiring foreign)

`resolve_file_link` had no caller. `EditableLine` now detects inline
`[label](target)` Markdown links per line (new `editor::markdown_links_in_line`)
and renders the label with an accent underline; a platform-modifier click
(mirrors the terminal's `opens_terminal_link` convention, P82 SEAMS.md —
a plain click still only places the caret, since this is an editable
surface) resolves the link against the open file's own directory and emits
a new `FileViewEvent::OpenFile(PathBuf)`.

Wiring that event to the shell's `add_file_tab` (the same path
`RightPanelEvent::OpenFile`/`ChatEvent::OpenFile` already use) is a
one-arm `cx.subscribe` in `tiller/src/main.rs`, which I do not own — see
`wantedForeignFiles`.

### F-PERSIST-DB-06 — account-identity schema (partially-built)

Added migration v13: `account_identity(provider TEXT PRIMARY KEY, identity
TEXT NOT NULL, detected_at INTEGER NOT NULL)` — one upserted row per
provider. This is the store `discover_claude_identity`/
`discover_codex_identity` (`tiller_ui::settings`) need to persist their
detected display line into, and the Settings restore path needs to read
from. The save/load functions (`tiller_persistence::db`) and the two
callers (`tiller_usage::account`, `tiller_ui::settings`) are all outside
this slice's ownership — see `wantedForeignFiles`.

Bumping `CURRENT_SCHEMA_VERSION` 12 -> 13 breaks two hardcoded `12`
literals in `tests/persistence_integration.rs` (not owned this wave;
36/37 of that file's tests still pass, only the one with the hardcoded
literal fails) — see `wantedForeignFiles`.

### F-PERSIST-DB-11 — forward-migration data-survival test (built)

Added a migrations.rs unit test that plants a `session_ref` row, a `tab`
row (with `order_idx`), and a `chat_turn` row at schema v6 (before v10's
`tab.agent_id` and v12's `chat_turn.updated_at` exist), migrates forward
to current, and asserts each survives with the exact backfilled values
those migrations document — `agent_id -> NULL`, `updated_at -> 0` — not
just that the row count is unchanged.

Separately, the evidence also notes chat_turn content "does not restore
after boot" even on a fresh v12 database with no migration involved — that
is schema-independent (my test shows the schema layer is sound) and points
at the caller side (`tiller_persistence::db::load_chat_transcript` has a
real, correct implementation; nothing in `tiller_ui::chat`/`tiller/main.rs`
that I could find calls it on tab restore). That is a different bug in
files this slice doesn't own; noted for visibility, not claimed as fixed
here.

### F-PER-06 — killpg every descendant process group (built)

`terminate_process_group` only ever signaled the pgid captured once at
spawn. A job-control-spawned child that detaches into its own process
group (a compound command, a backgrounded job) survived shutdown as an
orphan. Now walks `/proc/<pid>/task/*/children` from the shell pid at kill
time, reads each descendant's live `getpgid`, and kills every distinct
group found. New test uses `setsid` to force a real detached group
deterministically (a plain `sleep &` under a non-interactive `-c` shell
doesn't reliably separate group, which is why the older test alone wasn't
proof).

### F-TERM-PTY-06 — real ExternalPaths (OS-level) file drop (built)

`on_drop::<PathBuf>` only ever caught GPUI's in-app typed drag (a
Files-panel row). Added a second `on_drop::<gpui::ExternalPaths>` handler
on the terminal's drop target — GPUI's actual XDND payload type, absent
from the whole workspace before this — and widened `receive_file_drop`
from one `PathBuf` to `Vec<PathBuf>` so a multi-file OS drop inserts all
of them space-joined. Real XDND is not exercisable by this harness (no
window manager, per ENVIRONMENT.md); the new test drives the same
production handler via an in-app drag carrying an `ExternalPaths` payload
with two paths — GPUI's typed-drop dispatch does not distinguish an
XDND-originated payload from an in-app one of the same type, so this
exercises the real code path, not a simulation of a simulation.

### F-PRJ-17 / F-PRJ-18 — default worktree base + location override (partially-built)

`tiller_git`/`tiller_persistence`/`tiller_project` already fully support
both settings (the `project` table has had `default_worktree_base` and
`worktree_location_override` columns since migration v1); only
`CatalogProjectSettings` was missing the fields, so `write_catalog` never
set them and `restore_catalog` never read them back — any value would be
silently dropped at the next save. Added both fields, threaded through
`write_catalog`/`restore_catalog` the same way `color_hex`/`display_name`
already round-trip, with a new test proving the full write-then-restore
round trip through a real `SessionStore`.

`tiller_ui::sidebar` (the New Worktree popover's UI controls and
`confirm_worktree_prompt`'s two hard-coded `None` call sites) and
`tiller/main.rs` (`update_project_settings`'s struct literal) are both
outside this slice's ownership — see `wantedForeignFiles`. I verified
locally (then reverted, uncommitted) that the correct main.rs fix is not
just adding `None` for the two new fields — that would silently wipe any
base/override the user had set on the very next unrelated icon/color/name
edit — but reading the project's *existing* settings first and carrying
those two fields forward. All 29 `session::` tests pass with that local
fix in place.

## Foreign files this slice cannot touch (wantedForeignFiles)

- `rust/crates/tiller/src/main.rs` (owned by B1-tabbar-zorder):
  1. `update_project_settings` (~line 2977): its `CatalogProjectSettings`
     struct literal needs `default_worktree_base`/`worktree_location_override`
     added — by reading `self.project_catalog.project_settings(&update.id)`
     first and carrying those two fields forward, not hard-coding `None`.
  2. A `cx.subscribe(&file_view, ...)` arm forwarding
     `FileViewEvent::OpenFile(path)` to `workspace.add_file_tab(path, cx)`,
     mirroring the existing `RightPanelEvent::OpenFile`/`ChatEvent::OpenFile`
     arms, to finish F-CORE-FILE-04's wiring.
- `rust/crates/tiller_ui/src/sidebar.rs`: a default-worktree-base control
  and a location-chooser + restore-default-parent control on
  `render_project_settings`; at `confirm_worktree_prompt`
  (sidebar.rs:1333-1373) pass the real values into `create_worktree`'s
  `base: Option<&str>` and `resolve_parent_directory`'s
  `override_dir: Option<&Path>` instead of the hard-coded `None` at
  :1366/:1354 (F-PRJ-17/F-PRJ-18).
- `rust/crates/tiller_persistence/src/db.rs`: `save_account_identity`/
  `load_account_identity` functions against the new v13 `account_identity`
  table (F-PERSIST-DB-06).
- `rust/crates/tiller_usage/src/account.rs`: call the new persistence
  functions on successful `discover_claude_identity`/
  `discover_codex_identity` detection (F-PERSIST-DB-06).
- `rust/crates/tiller_ui/src/settings.rs`: read the persisted identity on
  restore, before/alongside the live shell-out (F-PERSIST-DB-06).
- `rust/crates/tiller_persistence/tests/persistence_integration.rs`: two
  hardcoded `12` literals (lines 274-275, in
  `v9_tab_rows_upgrade_to_current_with_no_recorded_agent_identity`) need
  to become `13` (or read `CURRENT_SCHEMA_VERSION` instead of a literal,
  so this doesn't recur) now that migration v13 exists.
