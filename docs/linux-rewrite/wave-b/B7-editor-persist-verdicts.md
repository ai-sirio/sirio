# B7-editor-persist — verdicts

Critic pass, independent of the builder. A previous verifier for this slice died mid-run; its
partial captures under `reference/linux-progress/verify-B7-editor-persist/` were **not** used or
trusted — nothing here cites them. All evidence below is freshly captured this pass, mostly on the
Wayland lane (`Scripts/wayland-drive.sh`, screenshots under
`reference/linux-progress/verify-B7-final/`), plus two real host-level instruments where pixels
don't apply: a live SQLite read against a running instance's own database, and a real `/proc`
process-tree check around an actual `kill`/close.

**Environment note:** the Wayland lane auto-registers "the nearest repository" from the launch cwd
as a project on some (not all) fresh-DB boots (`initial_working_directory()`,
`crates/tiller/src/main.rs`) — this pulled in the real `tiller`/`tiller-linux` git worktrees
(including the Swift-original checkout at `/home/enzopalmisano/Scrivania/Progetti/tiller`) ahead of
my test project on two runs, shifting sidebar row coordinates. One process spawned in a terminal
pane during that shift was immediately killed with the whole instance the moment the mistake was
caught; nothing was written to either worktree (confirmed via `git status --short` before/after,
byte-identical). All evidence actually cited below was recaptured after switching to a
verify-project-only flow that checks the sidebar layout before clicking.

## F-EDIT-02 — mouse-driven caret/selection wired to formatting — **PASSED**

Live-drove the exact defect on record. Opened a real file (`/tmp/verify-b7-project/notes.md`,
`wordtobold example line.`) in Code mode, double-clicked the word `wordtobold` (word-select, not a
drag) and clicked the toolbar's `B`:

- Before: line read `**wordtobold** example line.` after Bold — the word, not end-of-buffer and not
  the whole line (`reference/linux-progress/verify-B7-final/04-after-bold.png`).
- Same file, double-clicked `example` and clicked `I`: line read
  `wordtobold *example* line.` — again exactly the touched word
  (`reference/linux-progress/verify-B7-final/03-italic-applied.png`).

This directly reverses the ledger's own evidence ("Bold left the selected word unchanged; Italic
wrapped the entire line"). Supplementary: `cargo test -p tiller_ui --lib` —
`mouse_click_formats_the_clicked_line_not_end_of_buffer`,
`drag_selection_produces_the_dragged_range_not_a_whole_line`, and
`double_click_selects_the_touched_word_not_the_whole_line` all pass (drag-select itself is not
exercisable live on this lane — no pointer-drag primitive — so the drag half rests on this test,
not a live capture).

Caveat, not a blocker: pressed Ctrl+Z afterward and the `**` markers were **not** removed
(`04-after-undo.png`). `grep -rn "undo\|redo" editor.rs file_view.rs` returns nothing — undo/redo
was never implemented anywhere in the editor, before or after this row. The triage's own Approach
text asks only for mouse-driven caret/selection wiring, not undo; this is a separate, pre-existing
gap outside the row's scope, worth flagging to the maintainer but not this row's defect.

## F-CORE-FILE-04 — clickable rendered file links — **FAILED — defective**

The detection/resolve/emit half is real: opened `notes.md` in Code mode and
`[gotarget](target.md)` renders with an accent underline exactly as the report describes
(`reference/linux-progress/verify-B7-final/03-italic-applied.png` shows it inline). But the sink is
missing. Exhaustive grep across the whole workspace:

```
grep -rn "FileViewEvent" rust/crates/ --include="*.rs" | grep -v file_view.rs
```

returns **nothing** — `FileViewEvent::OpenFile` (`file_view.rs:379`) has zero subscribers anywhere,
including `tiller/src/main.rs` (which does wire the sibling events `RightPanelEvent::OpenFile` at
`:2738` and `ChatEvent::OpenFile` at `:2756` — the exact pattern this row needed to mirror, and
didn't get). A real user click — with or without the platform modifier — cannot open the linked
file today: the event fires into the void. (The modifier-click gesture itself isn't exercisable on
this lane — `WAYLAND-LANE.md` lists modifier chords as unsupported — but that's moot here: the
static trace proves no click of any kind would do anything, so the missing gesture support doesn't
change the verdict.) The builder's own report is candid about this — `main.rs`'s subscribe arm is
listed under `wantedForeignFiles`, not owned by this slice, and never landed this wave.

## F-PERSIST-DB-06 — account-identity persistence — **half-proven** (unchanged)

Schema half is real and newly proven: migration v13 creates `account_identity(provider PRIMARY
KEY, identity, detected_at)`; live-queried a running instance's own database directly
(`python3 -c "import sqlite3; ..."`, no `sqlite3` CLI on this host) and confirmed
`PRAGMA user_version` reads `13` and the table exists.

Account half is still completely disconnected end to end — confirmed live, not just by absence of
callers. Drove `surface.settings.open` → `surface.settings.select section=ai-providers`: the real
`discover_claude_identity` shell-out ran and rendered **"Signed in e.palmisano@reply.it"**
(`reference/linux-progress/verify-B7-final/02-settings-ai-providers.png`). Immediately re-queried
the same database: `SELECT * FROM account_identity` → **zero rows**. A real, successful discovery
that a user would see on screen leaves no trace in the store built to hold it. `grep -rn
"account_identity" tiller_persistence/src/db.rs tiller_usage/src/account.rs
tiller_ui/src/settings.rs` matches nothing outside test names — no save function, no load
function, no caller. This is exactly the state the builder's own report describes (schema only,
callers foreign/unowned) — unchanged from the ledger's half-proven, now with a live negative
control instead of just a static read.

## F-PERSIST-DB-11 — forward-migration data survival — **PASSED**

The triage's own gap — "no existing test plants data at an old schema version... and checks it
survives forward migration" — is now closed by a real test, independently run and confirmed:

```
cargo test -p tiller_persistence --lib -- forward_migration_preserves_pre_existing_session_ref_tab_and_chat_turn_data
test migrations::tests::forward_migration_preserves_pre_existing_session_ref_tab_and_chat_turn_data ... ok
```

Read the test directly (`migrations.rs:359`): it plants `session_ref`, `tab` (with `order_idx`),
and `chat_turn` rows at schema v6 — before v10's `tab.agent_id` and v12's `chat_turn.updated_at`
exist — migrates to `CURRENT_SCHEMA_VERSION` (13), and asserts each survives with the *exact*
documented backfill (`agent_id -> NULL`, `updated_at -> 0`), not just row-count. This is exactly
the row's own ask, for exactly the files it owns (`migrations.rs`).

Not this row's scope, noted for visibility only: the same evidence trail (both the ledger's and the
builder's own report) separately observes that chat_turn content does not restore into the *live
chat UI* on app boot — a `tiller_ui::chat` / `tiller/main.rs` session-restore-path question, not a
migrations.rs schema question, and outside the files this slice owns. The new test proves the raw
DB rows genuinely survive the schema upgrade; it says nothing about, and isn't asked to fix,
whether another crate reads them back on boot.

## F-PER-06 — kill every descendant process group on shutdown — **PASSED**

Confirmed first that HEAD actually carries the fix and the sibling revert described in the
builder's report did not resurface: `git log -1` is `d9704e8`, sitting after `915b0d0`
("descendant-group shutdown + real ExternalPaths file drop"), and
`terminate_descendant_process_groups` / `descendant_pids` / `descendant_process_groups` /
`PidGuard` and the `shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
test are all present in `tiller_terminal/src/lib.rs` today (grepped directly, not assumed).

Then drove a real process tree, not a test: opened a real terminal pane, typed
`sleep 271828 &` at an interactive shell prompt (real job control, not `setsid` under `-c`), and
read the resulting process back from the host:

```
PID    PPID    PGID  COMMAND
81691  80506   81691 sleep        # own process group, distinct from the shell's pgid (80506)
```

Closed the tab (`×`), confirmed the app's own "Close dirty tab?" dialog
(`reference/linux-progress/verify-B7-final/04-dialog-shown-final.png`), clicked `Close`
(`05-after-close-final.png` shows the tab gone, back to the empty "No Terminals" state), and
re-checked the host: `ps -p 81691` returned **nothing** — the detached job-control child is fully
terminated. This is the literal scenario the ledger's evidence complained about
("compound-command panes orphan process groups on quit"), reproduced live and shown fixed.
(A same-named unit test flaked once under `--lib`-wide concurrent execution in one run — three
repeats of the test alone were clean; the live process-tree result above doesn't depend on that
test either way.)

## F-TERM-PTY-06 — real OS-level (`ExternalPaths`) file drop — **half-proven**

Build half is real: before this wave, `grep -r ExternalPaths` returned nothing in the whole
workspace (confirmed against the ledger's own prior evidence); today
`on_drop::<gpui::ExternalPaths>` exists on the terminal's drop target (`lib.rs:1548`) and
`receive_file_drop` takes `Vec<PathBuf>`. Ran the specific test:

```
cargo test -p tiller_terminal --lib -- external_paths
test view_tests::a_drawn_terminal_accepts_a_real_external_paths_drop_with_several_files ... ok
```

— it drives the production `on_drop::<ExternalPaths>` handler with GPUI's actual XDND payload
type (not a bespoke stand-in), which is a materially stronger claim than the pre-wave state.

Live-exercise half remains unreached, and not for lack of trying: `WAYLAND-LANE.md` documents no
pointer-drag primitive on this lane (only `click`/`move`), and `ENVIRONMENT.md` documents real XDND
as unexercisable on every lane in this harness (no window manager). Both the in-app drag
(Files-panel row onto a terminal) and the OS-level drop the row's own `howToExercise` names are
therefore instrument-blocked here, same as every prior pass. Verdict moves from `NOT EXERCISED` to
`half-proven` on the strength of the new, correctly-typed, real test — not to `PASSED`, since no
live instrument available to me can drive the actual gesture.

## F-PRJ-17 — default worktree base — **FAILED — absent**

Session-layer plumbing is real: `CatalogProjectSettings::default_worktree_base` round-trips through
`write_catalog`/`restore_catalog` (`session.rs`, verified by reading the code and by
`default_worktree_base_and_location_override_round_trip_through_the_catalog_store` passing), and
the integrator's mechanical fix to `main.rs`'s `update_project_settings` (reading the project's
existing settings and carrying the two new fields forward, not hard-coding `None`) is present at
`main.rs:2984-2990` — confirmed by reading it directly, not just trusting `INTEGRATION.md`.

But live-driven, the user-facing feature does not exist. Opened the New Worktree popover
(`+` under a project) and typed into it:
`reference/linux-progress/verify-B7-final/04-new-worktree-typed.png` shows exactly one control — a
branch-name text field reading `AAAtestbranchAAA`, "Enter to create · Esc to cancel" — no
default-base option anywhere, before or after typing. Reading `sidebar.rs` confirms why:
`confirm_worktree_prompt` still calls `resolve_parent_directory(&repo_root, None)` (`:1386`) and
`create_worktree(&repo_root_for_task, &branch_for_task, &path_for_task, None)` (`:1398`) — both
hard-coded `None`, unchanged. This reproduces the ledger's exact recorded evidence
(shots/34: "exactly one control, a branch-name field") byte-for-byte, because the file that would
carry the fix (`tiller_ui/src/sidebar.rs`) was never in this slice's ownership and nothing else
touched it this wave.

## F-PRJ-18 — worktree location override — **FAILED — absent**

Same popover, same capture (`04-new-worktree-typed.png`), same finding: no location chooser or
restore-default-parent control exists anywhere in this flow. `sidebar.rs`'s
`render_project_settings` has no `worktree_location_override` reference at all (grepped directly),
and `confirm_worktree_prompt`'s `resolve_parent_directory` call still passes the hard-coded `None`
noted above. Same root cause as F-PRJ-17 — the session-layer half is done, the UI half was never
this slice's file to touch and nobody else touched it.

## Summary

| Row | Verdict |
|---|---|
| F-EDIT-02 | PASSED |
| F-CORE-FILE-04 | FAILED — defective |
| F-PERSIST-DB-06 | half-proven |
| F-PERSIST-DB-11 | PASSED |
| F-PER-06 | PASSED |
| F-TERM-PTY-06 | half-proven |
| F-PRJ-17 | FAILED — absent |
| F-PRJ-18 | FAILED — absent |
