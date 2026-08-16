# Wave H slice H1-gaps — report

## `F-CORE-DOM-07` — implemented

Root cause matched the recorded diagnosis: `AutoNamingThrottle` (`tiller_project/src/domain.rs`)
had zero production callers. Wired it as `TillerWorkspace::auto_naming_throttle: BTreeMap<usize,
AutoNamingThrottle>`, one throttle per tab id, consulted on a chat tab's running->done/needs-input
status transition. When permitted, spawns the settings-selected summarizer agent (falling back to
the tab's own agent) via the user's shell against the tab's `transcript_for_resume()` growth, and
applies the trimmed/truncated stdout as the tab title. `commit_tab_rename` now sets
`title_is_auto_named = false` so a manual rename can never be silently overwritten afterward
(mirrors Swift's `tab.titleIsAutoNamed`). Scoped to chat tabs, whose transcript source was already
wired; the terminal-tab file-based transcript source (Swift's `resolveFileTranscriptSource`) is a
separate, larger port intentionally left out.

Commit: `26a2c5d4`, `rust/crates/tiller/src/main.rs`. Test:
`tests::auto_naming_prompt_matches_the_reference_wording` plus the throttle's own pre-existing
unit tests in `tiller_project`.

**howToExercise**: open a chat tab whose title is still auto-named (default), send a message, let
the agent finish a turn (status goes idle/needs-input) — the tab title should update to a
summarizer-generated name shortly after. Manually rename the tab (double-click / rename menu) and
send another turn: the title must now stay exactly as manually set, never revert.

## `F-CORE-WSP-04` — implemented

Root cause matched the recorded diagnosis: `LayoutCommand`/`classify_layout_command`
(`tiller_project/src/layout.rs`) had 15 refs, all inside that file plus a bare re-export, and zero
under `rust/crates/tiller/src/`. First production call site: `commit_tab_rename` previously dropped
the rename widget's `FocusHandle` on commit and left nothing focused, so typing right after a
rename silently went nowhere until the user clicked back into the terminal. Fixed by constructing a
real `LayoutCommand::Rename` from the committed rename and consulting
`classify_layout_command(&command).focus` — when it answers `FocusIntent::Tab`, focus is restored
to the tab's content (the terminal's own `FocusHandle`) via the same `focus_tab_content` path
Escape now shares.

Commit: `8eb880f5`, `rust/crates/tiller/src/main.rs`. Test: a new `gpui::test` drives the real
right-click -> Rename -> type -> Enter gesture and asserts the terminal's `FocusHandle` is focused
afterward, not the (now-gone) rename field.

**howToExercise**: right-click a tab, choose Rename, type a new name, press Enter — then, without
clicking anywhere, type a character. It should land in the terminal (visible in the pane), not be
silently dropped. Pressing Escape mid-rename instead of Enter restores focus the same way.

## `F-CORE-WSP-08` — implemented, restart-survival bar exercised and passing

Root cause matched the recorded diagnosis: the persistence target `SessionTabState`
(`tiller/src/session.rs:79`, then holding only `root_id`/`pane_events`/`scrollback`) had no field
for a chat draft, caret, or fold, so nothing could be restored regardless of what fed it.

Implemented the `chat_draft` leg (the one the task named as having a hard, drivable pass bar):
`SessionTabState` grows a `chat_draft: String` field. `TillerWorkspace::layout()` captures it live
from `Chat::draft_text()` (new accessor over the existing `Composer`) at save time — the identical
live-read pattern already used for a terminal pane's `capture_scrollback()` — rather than tracking
it incrementally. `restore_tabs` (both call sites: the initial-window path and the
worktree-switch path) pushes a non-empty persisted draft back into the freshly-constructed `Chat`
entity via the existing `Chat::control_compose`, the same method the control socket's
`surface.chat.compose` already used. `surface.chat.compose`'s handler now also calls
`schedule_save(cx)` after composing, since previously nothing scheduled a layout capture until an
unrelated mutation happened to run one first — a draft composed and then immediately quit-from
would not have been captured at all.

**A real, load-bearing bug was found and fixed while proving this end-to-end**, not just unit
tests: driving compose -> restart via `wayland-drive.sh` against the same `TILLER_DB` showed the
draft did NOT survive, even though the unit tests (which never touch a real SQLite file across two
separate `AppDatabase::open` calls under GPUI's live save path) passed. Root cause, confirmed with
a temporary debug trace: `AppDatabase::save_tabs` (`tiller_persistence/src/db.rs`) upserted tabs in
array order without first clearing existing `is_active` flags. `tab_one_active_per_worktree` is a
*partial* unique index, which SQLite cannot make `DEFERRABLE` — it is checked immediately per
statement. When the active tab moves from a later array position to an earlier one (exactly what
happened here: `tab.select` moved active from Terminal, index 1, to Chat, index 0), upserting the
now-active earlier row happens while the still-active-in-the-database later row hasn't been
cleared yet, transiently violating the index and failing the whole transaction with `UNIQUE
constraint failed: tab.worktree_id` — silently, from the debounced background flusher's point of
view (it just logs and drops the pending snapshot). Fixed by clearing every existing row's
`is_active` flag for the worktree in one `UPDATE` before the per-tab upsert loop.

Added `save_tabs_moving_the_active_flag_to_an_earlier_tab_does_not_violate_the_unique_index`
(`tiller_persistence/tests/persistence_integration.rs`) — confirmed to fail with exactly the
production error message before the fix (verified by temporarily reverting `db.rs` and re-running
it), and to pass after. Also added `tests::layout_captures_the_live_unsent_chat_draft` and
`tests::restore_tabs_seeds_the_composer_with_the_persisted_draft` in `tiller/src/main.rs`.

**editor_caret and editor_folds are intentionally not attempted in this pass.** `Editor`
(`tiller_ui/src/editor.rs`) has a `Selection` concept (start/end offsets) but no persisted "caret"
field distinct from that, and — more importantly — **no folding feature exists at all** anywhere
in `tiller_ui`; `editor_folds` in `tiller_project/src/layout.rs` names a UI capability that was
never built, not a wiring gap. Building real code folding to give `editor_folds` something to
persist is a feature in its own right, well outside "wire an existing type to an existing
feature." The task explicitly allows the restart-survival bar to be met by any one of caret / fold
/ chat draft; chat draft is now proven end-to-end live, not just in unit tests.

Commits: `03f5c889` (`rust/crates/tiller/src/main.rs`, `rust/crates/tiller/src/session.rs`,
`rust/crates/tiller_ui/src/chat.rs`, `rust/crates/tiller_persistence/src/db.rs`,
`rust/crates/tiller_persistence/tests/persistence_integration.rs`).

**howToExercise (the drivable restart bar)**: `TILLER_WL_LABEL=x Scripts/wayland-drive.sh
/tmp/x/run1 'ctl project.add path=<repo>; ctl surface.chat.open; ctl tab.select index=1; ctl
surface.chat.compose surfaceId=default-chat text=some_unsent_draft; shot before'` — the composer
shows the text. Then, in a **second, separate** invocation with the **same** `TILLER_WL_LABEL`
(same `TILLER_DB`, fresh process — this is a real restart, not a live session): `Scripts/
wayland-drive.sh /tmp/x/run2 'ctl tab.select index=1; shot after'` — the composer must show the
same text again, unprompted, with no compose call in the second invocation. Verified live: both
screenshots show `draft_that_survives_restart_XYZQ123` in the composer, the second one produced by
a fresh process reading only from the SQLite database written by the first.
