# Wave E slice E-C-1 — report

## `F-CHAT-34` — fixed: `save_tabs` was cascading away persisted chat transcripts

Root cause confirmed exactly as recorded: `save_tabs` (`rust/crates/tiller_persistence/src/db.rs`)
deleted every `tab` row for the worktree and reinserted fresh rows in the same transaction on every
call, and `schedule_save` calls it on almost any ordinary action (28 call sites in `main.rs`).
`chat_turn.tab_id` has `ON DELETE CASCADE` to `tab(id)`, so even reinserting a row with the *same*
id inside the same transaction wiped that tab's `chat_turn` rows before the reinsert landed.

Fix: `save_tabs` now upserts (`INSERT ... ON CONFLICT(id) DO UPDATE`) tab ids that are still present
in the new slice, via a new `upsert_tab` helper, and only deletes ids that were actually dropped
from the worktree — those deletions still cascade deliberately. Added
`db::save_tabs_tests::resaving_the_same_tabs_does_not_wipe_chat_transcripts` (an in-file
`#[cfg(test)]` unit test, since the integration test file isn't in this slice's owned-files list):
saves a chat tab, saves a transcript for it, re-saves the same tabs slice (simulating an ordinary
autosave), and asserts the chat session still lists that tab with its turn intact. All 38
`tiller_persistence` tests pass (one, `concurrent_writers_save_disjoint_records_and_exit`, is
flaky under parallel `cargo test` load — confirmed pre-existing and unrelated, passes in isolation).

Files: `rust/crates/tiller_persistence/src/db.rs`.

**How to exercise:** open a real agent chat tab, let it complete a turn, then run any ordinary
`ctl tab.select` (or any other action that triggers `schedule_save`) on the same worktree. Chat
History should still list that session afterward — previously it silently dropped to "No past
chats" while the live in-memory transcript kept displaying the turn.

## `F-CHG-13` — fixed: `focus_path` raced the Changes tab's own async refresh

Root cause confirmed exactly as recorded: `add_changes_tab` (`rust/crates/tiller/src/main.rs`)
calls `ChangesTab::focus_path` synchronously right after `ChangesTab::new`, but `new` only starts
an async `git_task` (`refresh`) and `entries` starts empty — `focus_path`'s match against `entries`
always no-op'd on the real call site (the unit test that existed only proved correctness because it
pumped `entries` to populate *before* calling `focus_path`).

Fix: `ChangesTab` (`rust/crates/tiller_ui/src/changes.rs`) gained a `pending_focus: Option<PathBuf>`
field. `focus_path` now defers to it when `entries` is still empty and a refresh is in flight,
instead of silently matching against nothing; `apply_snapshot` replays the deferred request once
the first snapshot lands (applied at most once, whether or not the path turns out to be present).
Added `focus_path_called_before_the_first_refresh_lands_still_expands_once_it_does`, which mirrors
`add_changes_tab`'s actual call sequence (`new` then `focus_path` with no pump in between) and
asserts the section ends up expanded once the snapshot arrives. All 26 `changes::tests` pass.

Files: `rust/crates/tiller_ui/src/changes.rs`.

**How to exercise:** with two real edits in a worktree, click a Files-panel Diff affordance for one
of them (`ctl` equivalent: open the Changes tab focused on a path). The Changes tab now opens with
that file's diff already expanded, instead of both files listed but neither expanded.

## `F-CORE-FILE-04` — fixed: Preview-mode markdown links now route through `FileViewEvent::OpenFile`

Root cause confirmed exactly as recorded: File Preview (the file view's *default* Markdown mode)
renders through `Chat::render_markdown_document` → `render_inline`
(`rust/crates/tiller_ui/src/chat.rs`), whose `on_click` called `cx.open_url()` unconditionally. Only
the non-default Code-mode + platform-modifier-click path (`file_view.rs`'s `EditableLine` element,
via `open_markdown_link`) routed through `FileViewEvent::OpenFile`. The existing regression test for
this row only ever called `open_markdown_link` directly — it never drove an actual rendered click in
Preview, so it couldn't have caught the gap.

Fix: threaded a new `Option<LinkClickOverride>` parameter (`LinkClickOverride =
Rc<dyn Fn(&str, &mut Window, &mut App)>`) through the whole markdown render chain in `chat.rs`
(`render_markdown` → `render_markdown_block` → `render_markdown_list` /
`render_markdown_table`/`render_row` → `render_inline`). Chat transcripts keep passing `None`
(unchanged `cx.open_url` default — right for assistant-authored prose). `file_view.rs`'s Preview
render now calls the new `Chat::render_markdown_document_with_link_override`, supplying a closure
that resolves the clicked target against the open file's directory via `resolve_file_link` and
emits `FileViewEvent::OpenFile` on a match, falling back to `cx.open_url` only when it doesn't
resolve locally.

Added `clicking_a_rendered_link_in_preview_mode_emits_open_file` in `file_view.rs`: mounts a real
`FileView` on a one-line-link Markdown file, drives an actual `cx.simulate_click` at the rendered
link's computed screen position (accounting for the `mx_auto`-centered, padded content column), and
asserts `FileViewEvent::OpenFile` fires. Verified this test fails against the pre-fix `on_click`
(reverted it locally, reran, confirmed the failure, restored the fix) and passes against the fix.
All 308 `tiller_ui` lib tests pass; `cargo build -p tiller` is green.

Files: `rust/crates/tiller_ui/src/chat.rs`, `rust/crates/tiller_ui/src/file_view.rs`.

**How to exercise:** open a Markdown file with a relative link (e.g. `note.md` containing
`[setup](setup.md)`) in the default Preview mode and click the rendered link. A new tab for the
resolved local file should open — previously nothing visibly happened (no new tab, no browser
launch either) because the click silently fell into `cx.open_url` against a relative, non-URL
target.

## `F-CORE-ACT-25` / `F-CORE-ACT-26` — blocked: no concurrently-mounted-worktrees concept exists to order or cap

Re-read `select_worktree` (`rust/crates/tiller/src/main.rs:4063`) fresh at HEAD: it still tears down
the *previous* worktree's panes synchronously on every switch
(`self.panes.set_external(&old_path, Vec::new())`) and there is no `open_worktree_ids`-style field
anywhere on the app model — only a single `working_directory: PathBuf`. Grepped the whole tree:
`BootstrapRestoreOrder` (`tiller_activity/src/bootstrap.rs`) and `WorktreeMountPolicy`
(`tiller_activity/src/mount.rs`) both still have zero callers outside their own module and tests;
`mounted_worktrees`/`limit_mounted_worktrees` remain persisted-only settings with no eviction
consumer. Exactly one worktree is ever mounted at a time, so there is no set to order at bootstrap
and no cap to enforce — both types are pure policy waiting on a concept (multiple worktrees'
terminal/pane state kept alive concurrently) that the current single-`working_directory` app model
does not have.

Building it for real means: (1) replacing the single `working_directory` with a small mounted-set
model (e.g. `Vec<PathBuf>` of currently-live worktrees plus the selected one), (2) changing
`select_worktree` to *add* to that set instead of tearing down the outgoing worktree's panes —
which means every per-worktree piece of UI state that currently assumes "there is exactly one"
(`self.right_panel`, `self.status_bar`, `self.sidebar`'s per-worktree tab/status caches, the changes
tab, the terminal panes themselves) needs to become keyed by worktree id and kept alive in the
background rather than rebuilt fresh on every switch, (3) calling `BootstrapRestoreOrder::partition`
at launch to decide which worktrees mount eagerly (selected + previously-open) vs. lazily, and
(4) calling `WorktreeMountPolicy::ids_to_evict` whenever the mounted set would exceed
`mounted_worktrees`/`limit_mounted_worktrees`, unmounting the returned ids the same way
`select_worktree` currently always does. That is a genuinely large, cross-cutting change to the
app's core pane-ownership model, not a wiring gap — the two policy types are correct and covered by
their own unit tests already; what's missing is the concept they were built to serve. A narrow call
site (e.g. calling `BootstrapRestoreOrder::partition` from one new, otherwise-inert helper) would
make `grep` show a caller while changing zero observable behavior. No production code changed for
either row.

Files touched: none (confirmed the recorded diagnosis still holds).

**How to exercise:** N/A until the multi-mount model above lands — there is no lane gesture or
control-socket method that can reach either type today (switching worktrees always tears the old
one down, so nothing ever accumulates a set to order or evict). Post-implementation, opening
several worktrees in one session and checking that background ones keep their agent processes alive
(rather than being killed on switch) would exercise `BootstrapRestoreOrder` at next launch and
`WorktreeMountPolicy` once the mounted count exceeds the configured cap.

## `F-CORE-DOM-07` — blocked: no transcript-driven auto-naming feature exists to throttle

Re-grepped at HEAD: `AutoNamingThrottle` (`tiller_project/src/domain.rs`) still has zero callers
outside its own module and unit test; `main.rs` has an `auto_naming` boolean setting and a
`summarizer_agent` choice persisted and round-tripped through settings, but no code path anywhere
that actually generates a tab name from a chat transcript and renames the tab — the feature the
throttle is meant to gate does not exist yet, only its on/off switch and its target-agent picker.

Building it means implementing the auto-naming feature itself first: watching a chat tab's
transcript growth, invoking the configured summarizer agent (an ACP call, following the same
pattern `tiller_acp` already uses for the main agent connection) to produce a short name, and
renaming the tab — with `AutoNamingThrottle::should_request`/`record_request` gating when that
request fires. That is a new feature (an LLM round-trip wired into tab lifecycle), not a throttle
placement — adding a call site to `AutoNamingThrottle` from an unused helper with no actual naming
request behind it would satisfy `grep` while being unreachable from any real gesture, exactly the
outcome the brief warns against. No production code changed for this row.

Files touched: none (confirmed the recorded diagnosis still holds).

**How to exercise:** N/A until transcript-driven auto-naming is implemented — there is no lane
gesture that can reach `AutoNamingThrottle` today since nothing calls it. Post-implementation,
generating enough transcript growth in a chat tab with `auto_naming` enabled, faster than
`MIN_INTERVAL`/`MIN_GROWTH` apart, should show at most one rename request fire per throttle window.
