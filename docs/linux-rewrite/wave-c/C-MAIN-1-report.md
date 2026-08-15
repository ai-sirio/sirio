# C-MAIN-1 report

Slice owner: link 1 of 4, `C-MAIN` chain. Files owned: `rust/crates/tiller/src/main.rs` only.

All commits below touch only `rust/crates/tiller/src/main.rs`. `cargo build -p tiller` is green
after every commit. `cargo test -p tiller` could not be run at any point in this session: the
worktree carries an unrelated, pre-existing, uncommitted break in `rust/crates/tiller_ui/src/sidebar.rs`
(`render_worktree_prompt_field` call sites missing two args) that predates this session — confirmed
by `git stash`-ing only my `main.rs` diff and re-running the same failing test build. Not my file,
not touched.

## F-CORE-ACT-24 / F-AGENT-SESSION-01 (combined implementation)

`restore_tabs`/`restore_tabs_in_workspace` (the launch-time and multi-worktree-open restore paths)
used to reconstruct an agent-owned "terminal" tab as a blank shell — no agent CLI was ever invoked
on restart, contradicting P120's live observation that Claude Code respawns with a **fresh** random
`--session-id` each restart, never `--resume`.

Added:
- `resumable_session_refs(restored, saved_refs, worktree_path)` — builds `AgentSessionRef`s keyed by
  the tab's stable `"pane-N"` string (the same string tillerctl hooks report `notify` calls under,
  because a tab's numeric pane id is itself persisted via `SessionTabState::root_id` and reused
  verbatim across restarts), computes `live_content_ids` from the restored layout, and calls
  `AgentSessionRestorePlan::plan` (tiller_activity). Each resumable candidate is then gated through
  `tiller_agents::AgentSessionValidator::is_likely_valid` (Claude: on-disk `.jsonl` transcript file
  exists; Codex: matching rollout file exists) before being trusted.
- `restored_agent_shell(agent_id, pane_key, worktree_path, resumable)` — for an agent-owned terminal
  tab, calls `adapter.prepare` then either `adapter.resume_command` (validated resumable ref) or
  `adapter.command` (fresh), building the same `TerminalShell::WithArguments` `add_agent_tab` uses.
  A tab with no agent identity still gets the old plain `TerminalView::new`.

Both `restore_tabs` and `restore_tabs_in_workspace` now take a `&BTreeMap<String, String>` of saved
session refs (the same map `session_store.load_session_refs()` already produced) and use it to build
the resumable set before their per-tab loop.

**howToExercise:** `TILLER_WL_LABEL=<x> Scripts/wayland-drive.sh <dir> 'ctl project.add path=<repo>;
ctl <open a Claude Code tab, send a message so a session.ref/notify agentSession lands>'`, then kill
and relaunch the same instance (same `$XDG_DATA_HOME`/db) and inspect the new process's argv (`ps` on
the pane's PTY child, or watch the `tillerctl notify --session <session>` calls) for `claude --resume
<same-ref>` instead of a fresh session. A restart with no saved ref, or a saved ref whose
`~/.claude/projects/<slug>/<ref>.jsonl` is missing, must fall back to a bare fresh `claude`/`codex`
invocation — confirms the prunable/invalid branch.

## F-AGENT-SAFE-02

`ClaudeHookMigrator::migrate_file` (tiller_agents, already implemented, previously had zero
production callers) is now invoked once at app startup, before any window opens, over every known
worktree's `.claude/settings.local.json` (iterating `project_catalog.projects()` /
`.worktrees`), repairing a stale leading `tillerctl` path in place. This covers the case
`prepare()` cannot: a worktree with no pane reopened this run still gets its hook config fixed.

**howToExercise:** hand-write a `.claude/settings.local.json` under a known worktree with a
`command` whose leading quoted path is some old/wrong `tillerctl` location (same shape `prepare()`
writes), launch Tiller pointed at that worktree without opening a Claude Code tab, then diff the
file: only the leading path changed, pane ids/args/other keys are byte-identical.

## F-AGENT-SESSION-02

Added a new control-socket method, `session.transcript` (added to `system.capabilities`'
advertised list too), that reads a native agent's own on-disk transcript for a session reference
this socket already recorded via `session.ref`/`notify`'s `agentSession` param —
`tiller_agents::ClaudeTranscriptSource`/`CodexTranscriptSource` (already implemented, previously
had zero non-test callers) do the actual file read. Params: `session` (pane id), `agent`
(`claude`/`codex`), `worktree`. The agent identity is an explicit request param rather than
`panel.list`'s `agent` field, which the wayland lane's own trap notes reads `""` even after a
`notify` — not a reliable read of live state.

**howToExercise:** `ctl session.ref session=pane-N ref=<a real session id>` (or let a real `notify
... --agentSession <id>` call populate it), then `ctl session.transcript session=pane-N agent=claude
worktree=<path>` — expect `{"text": "..."}` pulled straight from
`~/.claude/projects/<slug>/<id>.jsonl`. An unknown pane, wrong agent, or missing transcript file
must return a typed failure, not a silent empty success.

## F-CORE-ACT-19

`post_desktop_notification`'s title was `"<agent display name> — <status human label>"` (live
D-Bus proof: `"Claude Code — finished"`); the contract (`AgentActivityModel.swift:261`) requires
`"<agent display name> — <human worktree label>"`. `AgentActivityModel::build_payload`
(`tiller_activity/src/model.rs:448`) still emits the status-suffixed title — that file is not owned
by this slice — so the payload's `title` is overridden immediately after `build_payload` returns,
using the same `activity_label` (`"<project>/<branch>"`) the status bar and window title already
use as the human worktree label. Body format (branch + project) was already correct and is
untouched.

**howToExercise:** trigger any attention transition (agent goes idle/needs-input/error) on a
worktree with a project name, while the pane is not visible/app not focused (`NotificationPolicy`
gate), and read the emitted D-Bus `Notify` title — must read `"<Agent> — <project>/<branch>"`, never
`"<Agent> — <status>"`.

## F-CORE-ACT-20 — already-correct, no change

Read `tiller_activity::NotificationPolicy::should_notify` and
`AgentActivityModel::build_payload`: `should_notify` already suppresses when `old == Some(new)`
(identical-status branch) unconditionally, before the app-active/pane-visible gate; `build_payload`
already returns `None` when `!self.pane_agents.contains_key(pane_id)` (no-agent-running branch).
Both untested branches described in the ledger are implemented correctly in code already reachable
from `main.rs`'s existing `sync_activity` caller — there was no owned-file defect to fix. This row
still needs a critic to actually drive both branches live (no agent ever spawned on a pane, and two
consecutive identical statuses) to move off half-proven; that is an exercise gap, not a code gap.

## F-CHG-02 — partial

Live-drove the exact P104 gesture (`workspace.close` then `surface.changes.open`) via the wayland
lane and found the socket layer's no-current-workspace guard (`has_current_worktree()` in
`control_open_changes`) already correctly rejects the implicit case — the currently-recorded ledger
evidence for that half looks stale. But driving the *explicit* form,
`surface.changes.open worktree=<the just-closed, still-live worktree>`, uncovered a real, separate
bug: it was **also** wrongly rejected with `"no current workspace"`, because `select_worktree`'s
full re-select path was skipped whenever the requested path already equalled the live shell's
`working_directory` (true right after a same-worktree `close`), so `ControlState::current` never got
re-marked. Fixed: when the path already matches but `has_current_worktree()` is false, just
re-mark `ControlState`'s selection instead of skipping entirely (avoids the full teardown, which
would wrongly evict the worktree's own live panes via `panes.set_external`).

Live-verified before/after with the wayland lane (`TILLER_WL_LABEL=cmain1chg02d`):
before, `surface.changes.open worktree=<id>` after close → `{"ok":false,"error":"no current
workspace"}`; after the fix, the same call → `{"ok":true, ...}`.

**Not fixed, needs an owner of `tiller_ui/src/changes.rs` / `tiller_ui/src/right_panel.rs`:** the
row's actual clause (`01-inventory-app.md:135`) wants a visible **"no worktree selected"** empty
state rendered in the right panel/Changes surface itself when there genuinely is no current
worktree (e.g. after closing the only open worktree) — that is a rendering concern in
`ChangesTab`/`RightPanel`, which this slice does not own. See `wantedForeignFiles`.

**howToExercise (fixed half):** `ctl workspace.close workspace=<id>` on the currently-open worktree,
then `ctl surface.changes.open worktree=<same id>` — must return `ok:true` with that worktree's
report, not `"no current workspace"`.

## Not attempted / blocked

- **F-AGENT-OMP-03** — the `main.rs`-side gap cited in the row's own evidence ("main.rs has no
  caller for any adapter") is explicitly F-SET-05's, not this row's. The row's actual remaining
  blocker is a live upstream `oh-my-pi --print --no-tools` `SyntaxError` in the oh-my-pi CLI itself
  — nothing in any file this chain can edit changes that. Re-ran the exact generated command myself;
  same upstream failure. No code fix available.
- **F-CORE-ACT-10** — evidence on record explicitly says the prior positive/negative crops were
  confounds (tab-active-highlight, an accidental misrouted chat conversation), leaving "no valid
  discriminating evidence either way." `panes.rs` (the file most likely to hold real Layer-D wiring)
  is not in this slice's owned-files list even though triage named it, so no code change was made;
  this needs a clean re-drive, which is an exercise task, not something addressed by editing
  `main.rs`.
- **F-BRW-04, F-BRW-05, F-BRW-09, F-CHAT-34, F-CHAT-35, F-CHG-13** — see `wantedForeignFiles` below;
  each row's actual defect lives entirely in a file this slice does not own
  (`tiller_ui/src/browser.rs`, `tiller_ui/src/chat.rs`, `tiller_ui/src/changes.rs`), confirmed by
  re-reading the current file (not the possibly-stale evidence quote) before writing each note.
