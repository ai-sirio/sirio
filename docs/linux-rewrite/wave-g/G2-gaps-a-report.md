# Wave G slice G2-gaps-a — report

## `F-CORE-ACT-25` — implemented

Root cause matched the recorded diagnosis: `BootstrapRestoreOrder::partition` had zero
production callers. Wired it into `ControlState::from_catalog`
(`rust/crates/tiller/src/main.rs`) — the restored worktree list is partitioned with
`open_worktree_ids = [selected_path]` (the only "previously open" id a launch snapshot that
persists a single `working_directory` can recover) and `selected_worktree_id = Some(selected)`.
Only the `priority` set starts `mounted: true`; every `deferred` worktree now starts unmounted,
replacing the old "mount everything unconditionally" default. `bootstrap.rs` needed no change
— it was already correct in isolation.

Added `tests::bootstrap_restore_order_mounts_only_the_selected_worktree_at_first_paint`
(`rust/crates/tiller/src/main.rs`), asserting a 3-worktree catalog leaves only the selected one
mounted after `from_catalog`.

**howToExercise**: `tillerctl list-workspaces` (or the `workspace.list` control verb) right
after launch with a project that has 2+ worktrees — the non-selected worktree rows should read
`mounted=false`; select one via the sidebar or `worktree.set` and its row flips to `mounted=true`.

## `F-CORE-ACT-26` — implemented

Root cause matched the recorded diagnosis: `WorktreeMountPolicy::ids_to_evict` had zero
production callers, and `AppSettings::mounted_worktrees`/`limit_mounted_worktrees` were read
nowhere in `main.rs`. Added `TillerWorkspace::evict_over_capacity_worktrees`
(`rust/crates/tiller/src/main.rs`), called at the end of `select_worktree` right after the
newly selected worktree is mounted. It reads the cap from the live `Settings` snapshot
(`limit_mounted_worktrees` gates whether a cap applies at all; `mounted_worktrees` clamped
2–50 is the cap), builds `open_worktree_ids` from `ControlState.workspaces` where
`mounted == true`, resolves per-worktree `AgentStatus` by mapping `PaneRegistry::list_for` pane
ids through `AgentActivityModel::status_for_panes` (so a worktree with a running or
needs-input agent is never evicted), and calls `ControlState::close_worktree` for every id
`WorktreeMountPolicy::ids_to_evict` returns. `mount.rs` needed no change.

**Known limitation, not a regression**: the `has_unsaved_work` gate is `|_| false` — no
unsaved-work detector exists yet for a worktree as a whole (only per-tab dirty-file tracking in
`ChangesTab`), so eviction never holds a worktree back on that basis. Nothing evicted worktrees
before this change either, so this is a real, incremental improvement, not a step backward; a
follow-up should thread `ChangesTab`'s dirty predicate (already used for `F-CHG` rows) through
this closure.

Added `tests::selecting_past_the_mount_cap_evicts_the_oldest_idle_worktree`
(`rust/crates/tiller/src/main.rs`) — a full `TillerWorkspace::select_worktree` drive with a
3-worktree fixture and cap 2, asserting the oldest idle worktree is evicted while the
just-selected and one other stay mounted.

**howToExercise**: with `Settings` → mounted-worktree limit enabled and set to 2 (or via
`AppSettings.mounted_worktrees`/`limit_mounted_worktrees` directly), open three worktrees in
sequence via the sidebar and check `tillerctl list-workspaces` after the third selection — the
first-opened, idle worktree's row should now read `mounted=false`.

## `F-CORE-DOM-07` — blocked

Root cause matched the recorded diagnosis and is confirmed **not closeable within this row's
file list**: `AutoNamingThrottle` (`tiller_project/src/domain.rs:92`) is a correct, unit-tested
pure throttle, but there is no auto-naming *feature* anywhere in `main.rs` for it to gate — only
the `auto_naming` settings toggle exists (`rust/crates/tiller/src/main.rs:9379`/`9413`). This
is not a missing call site to a finished subsystem (unlike ACT-25/26) — the subsystem itself
(turn-completion detection → throttle check → name generation → tab rename) does not exist yet,
and building it for real, not as a token call, needs pieces outside this row's owned files:

- **Trigger**: a "chat turn completed, transcript grew" event. `Chat` (`tiller_ui/src/chat.rs`)
  already exposes `transcript_text()`/`transcript_for_resume()` that could supply
  `AutoNamingThrottle::should_request`'s `transcript_len`, but nothing currently calls it on a
  cadence — it would need a new poll or event hook from `TillerWorkspace`, most naturally
  reusing the existing 40ms `cx.spawn` pending-action loop already in `TillerWorkspace::new`.
- **Name source**: `AgentAdapter::summarizer_command` (`tiller_agents/src/lib.rs`, already
  built per `T6-term-agent-plan.md`'s shared-cause-#1 finding, with `SummarizerChoice`
  resolving which adapter to use) returns a shell command line; nothing spawns it. A real
  implementation needs an async process spawn (background-executor `Command` run, not the
  synchronous `Command::new` calls `main.rs` already uses for git) whose stdout becomes the
  generated title, wired back through `cx.spawn`/`workspace.update`.
- **Sink**: renaming a tab is trivial once a name exists — `self.tabs[index].title = name` is
  already how `F-TAB` rename flows work (`main.rs:6872` reads the same field) — this part is
  genuinely small.
- **State**: `AutoNamingThrottle` is stateful per throttle instance (`last_request`,
  `last_transcript_len`); it needs to live per-tab (or per-chat), most naturally as a new field
  on `OpenTab` or the `Chat` entity, neither of which is in this row's owned file list
  (`OpenTab` lives in `main.rs`, which *is* owned — but `Chat`/its transcript accessor lives in
  `tiller_ui/src/chat.rs`, which is not).

**Why not a token call site**: the trap this wave's brief explicitly names — "a token call site
that makes a grep pass while changing no behaviour" — is exactly what a minimal fix here would
be. `AutoNamingThrottle::should_request` could be called from some contrived location, but
without a real transcript-length feed and a real name generator behind it, no observable
behavior changes, and a critic would (correctly) fail it as defective on the same live-drive
basis `T6-term-agent-plan.md` already used to close out `F-AGENT-OMP-03`/`F-AGENT-OPENCODE-03`'s
generator half. The right shape is the single `main.rs` integration pass that
`T6-term-agent-plan.md` describes ("one integration pass in `main.rs` (plus wiring
`AutoNamingThrottle`) closes all three at once") — closing `F-CORE-DOM-07` alongside
`F-AGENT-OMP-03`/`F-AGENT-OPENCODE-03`'s caller half in one slice that owns both `main.rs` and
`tiller_ui/src/chat.rs`.

- **size**: L — a new async process-spawn path plus new per-tab throttle state, not a one-line
  wire-up.
