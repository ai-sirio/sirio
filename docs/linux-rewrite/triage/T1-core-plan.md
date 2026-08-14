# T1-core build plan — F-CORE, 23 rows

Read-only triage output. No verdicts changed, no code touched. Each section names what a row
actually needs and which files a fix would touch, per `docs/linux-rewrite/triage/T1-core.md`.
Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

---

## `F-CORE-ACT-02` — half-proven

**Needs: exercise.** The sidebar half (Terminal/pane-1 notify -> glyph change) is already
proven. The remaining half is the D-Bus notification path for a Layer-A transition, and it is
blocked by the same broken exercise gesture as ACT-06/07/11 (see their shared cause below) —
**but it does not need to be.** `tillerctl notify --session <pane-id> --status <status>` only
needs a pane that is already *registered* with `AgentActivityModel`; the driver's route to
registration was OSC-title injection via `wtype` (fragile, unquoted, breaks mid-command). A
spawn-owned pane — launch a real agent through Tiller's own "Add agent" action
(`add_agent_tab`, `rust/crates/tiller/src/main.rs:4621`) — registers with
`self.activity` at spawn time with no title injection required at all. Gesture: spawn a real
agent tab, then `tillerctl notify --session pane-N --status needs-input` targeting that pane,
and watch `dbus-monitor` for the `Notify` call this time reachable without the fragile route.

The manifest also records a companion claim ("pane-0's notify call also marked the Chat tab")
that source does not support: `tab_status`'s `TabContent::Chat` branch
(`rust/crates/tiller/src/main.rs:3260-3270`) computes its own status from
`chat.is_streaming()`/`chat.has_completed_turn()` and never reads `self.activity` at all,
unlike the `TabContent::Terminal` branch a few lines below it. This is architectural, not a
bug in this row's clause (ACT-02 is about `AgentActivityModel::notify()`'s pane-level
recording, not about Chat tabs specifically) — flagging so nobody re-opens it as a defect
against this row. If a future row *does* want `tillerctl notify` to reach a Chat tab's icon,
that branch is the file and the line.

- **files**: none (exercise only; the registration-then-notify route uses code that already
  exists and is already exercised elsewhere)
- **size**: S

---

## `F-CORE-ACT-06`, `F-CORE-ACT-07`, `F-CORE-ACT-11` — NOT EXERCISED (all three)

**Needs: exercise**, all three, **shared cause**.

The full ownership machinery these three rows exercise is present and unit-tested:
`title_owned_panes`/`process_owned_panes` sets, `handle_title_change`, `process_identified`,
`process_gone`, and the pane-close clearing of both sets together, all in
`rust/crates/tiller_activity/src/model.rs:63-64,187-344`. `panes::tests::terminal_events_feed_
title_and_settled_content_into_the_one_model` and
`panes::tests::process_refresh_preserves_process_ownership_until_process_gone` (pass 16, green)
already prove this at the unit level. Nothing here needs a code change.

**Shared cause**: all three rows are blocked on one broken exercise gesture, not on missing
code. The evidence (`02-07-title-set-claude.png`) shows the OSC-title injection command —
`wtype`-typed, unquoted, containing a bare `;` — splitting mid-command rather than reaching
the terminal as one title-setting sequence. No title-owned identity ever gets registered, so
none of these three can be driven past that first step. The fix is a properly shell-quoted
title-injection command in whatever drives the gesture (e.g. `printf '\033]0;%s\007'
'<title>'` passed as a single quoted argument rather than typed keystroke-by-keystroke through
`wtype`), not a change to `rust/crates/tiller_activity` or `rust/crates/tiller/src`.

Once title injection lands cleanly, each row's own gesture (already spelled out in
`02-inventory-packages.md`'s VERIFY column) follows directly:
- ACT-06: identify a pane by title, change only the title to unrelated text, confirm
  title-owned state clears; repeat for a process-identified pane and confirm unrelated title
  text does *not* clear it.
- ACT-07: send a hook status, immediately force a contradictory recognized title, confirm the
  hook status holds; repeat after >1.5s and confirm the title can change it then. (The notify
  half of this race is already reachable — see ACT-02.)
- ACT-11: exercise title, process, and spawn detection on three separate panes, alter/kill one
  signal source at a time, close each pane, confirm only the correct state clears. All three
  ownership classes must be exercised together for this row's claim, not just the
  reachable spawn-owned class alone.

- **files**: none (exercise only)
- **size**: S each

---

## `F-CORE-ACT-10` — builder-claimed, unverified

**Needs: exercise.** The claim is accurate: `rust/crates/tiller/src/panes.rs:22`
(`PROCESS_SIGNAL_INTERVAL = Duration::from_millis(500)`) and `refresh_process_signal`
(`panes.rs:60`) are called from production at `rust/crates/tiller/src/main.rs:2897`, not just
from tests — `process_signal_interval() == 500ms` is itself unit-tested
(`panes.rs:688`). This just has never been independently driven live. Gesture: launch a
native-binary agent (not a node/bun-hosted one — those are invisible to this layer by design,
per ACT-10's own clause), kill its child process directly from outside Tiller (not via the
app's own close-pane path), and confirm the process-owned status clears within roughly one
tick (~500ms-1s), not on the next unrelated activity.

- **files**: none (exercise only)
- **size**: S

---

## `F-CORE-ACT-19` — FAILED — defective

**Needs: build.** Confirmed at `rust/crates/tiller_activity/src/model.rs:448`:

```rust
title: format!("{agent_display_name} — {}", status.human_label()),
```

The clause requires `<agent display name> — <human worktree label>`; the code instead
interpolates the *status* label into the same slot. Body (`worktree_branch` + optional
`project_name`/`comment`) is correct already, several lines above. Fix: `build_payload`
already receives `worktree_branch: &str` as a parameter — use that (or a purpose-built human
worktree label, if one beyond the branch string is wanted) in the title's second half instead
of `status.human_label()`. No caller-side change needed; `worktree_branch` is already passed
in from `rust/crates/tiller/src/main.rs:3775` (`post_activity_notification`).

- **files**: `rust/crates/tiller_activity/src/model.rs` (the only file that needs to change)
- **size**: S

---

## `F-CORE-ACT-20` — half-proven

**Needs: both.** Live D-Bus already proved the `pane_visible` half of the gate in both
directions. What's missing is not just "drive the other two branches" — one of them cannot
currently be driven at all, because it is dead on arrival:

```rust
// rust/crates/tiller/src/main.rs:3750
if !NotificationPolicy::should_notify(transition.old, transition.new, true, visible) {
```

The third argument — `app_active` — is a **hardcoded literal `true`**, not read from any real
window-focus state (grepped `is_active`/`WindowFocus`/`cx.is_window_active` etc. across
`main.rs`: nothing wires focus into this call). `NotificationPolicy::should_notify`
(`rust/crates/tiller_activity/src/notification.rs:19-29`) is otherwise correct pure logic —
`new == Running || old == Some(new)` suppresses, else `!(app_active && pane_visible)` gates —
but with `app_active` permanently `true`, the clause "no notification when the app is active
and the pane is visible" can only ever be tested through the `pane_visible` half; if Tiller is
literally in the background (another app focused) but happens to have that pane's tab
selected internally, a real transition would still be wrongly suppressed. That's a genuine
defect the "half-proven" verdict's own phrase ("both directions of the app_active/pane_visible
gate") slightly overclaims — only `pane_visible` was actually varied, because `app_active`
*can't* be varied yet.

Two separate things to close this row:
1. **Build**: wire real window-activation state into the `app_active` argument at
   `main.rs:3750` (GPUI's window-focus/activation signal — check what's available on the
   `Window`/`App` context already in scope at the call site) instead of the literal `true`.
2. **Exercise** (once wired): the no-agent-running branch (`post_activity_notification`'s own
   early-return when `self.activity.agent_id(...)` is `None`, `main.rs:3739-3741` — separate
   code path from `should_notify`, already present) and the identical-status branch
   (`old == Some(new)` inside `should_notify` itself) — both are cheap to drive live once
   app-active is real, and the identical-status branch is pure logic that could also just be
   unit-tested directly in `notification.rs` (it currently has zero `#[cfg(test)]` coverage
   at all).

- **files**: `rust/crates/tiller/src/main.rs` (wire real `app_active`, call site at
  `main.rs:3750`), `rust/crates/tiller_activity/src/notification.rs` (optional: add unit
  coverage for the no-agent/identical-status branches directly, no logic change needed there)
- **size**: M

---

## `F-CORE-ACT-24` — FAILED — absent

**Needs: build.** Confirmed directly, live-shaped: `AgentAdapter::resume_command` exists on
every adapter (`rust/crates/tiller_agents/src/{claude,codex,omp,opencode,pi}.rs`) but has
**zero callers** anywhere in `rust/crates/tiller/src` — grepped every call site of `.command(`
and `.resume_command(` in `main.rs`; only two hits, both `.command(` (`main.rs:4621` in
`add_agent_tab`, and `main.rs:5084`), fresh-launch always. This matches the row's own live
finding exactly: two real restarts, two fresh random `--session-id`s, `resume_command` never
invoked.

To build this: `add_agent_tab` (or a new restore-specific path) needs to know, at restart
time, which of a worktree's persisted agent tabs have a prior session id it can resume, and
call `adapter.resume_command(...)` instead of `adapter.command(...)` for those. That
knowledge doesn't fully exist yet either — see ACT-25 below, same subsystem, same gap.
`session.ref`/`session_refs` (`main.rs:6883` `load_session_refs`, referenced in
`ADJUDICATION-BACKLOG.md`) already persists *some* session-reference data across restart and
is the most likely existing seam to extend, rather than inventing a new persistence path.

- **files**: `rust/crates/tiller/src/main.rs` (restart/restore path — `add_agent_tab` or its
  restore-time caller — must resolve resumable vs. fresh and call `resume_command`),
  `rust/crates/tiller_agents/src/*.rs` (resume_command already correct per-adapter, no change
  expected unless the call-site needs a different signature)
- **size**: L — this is a real restore-classification feature, not a one-line wiring fix; see
  `sharedCause` below.
- **sharedCause**: `F-CORE-ACT-24` and `F-CORE-ACT-25` are two clauses over one missing
  subsystem — "on restart, know which prior agent sessions/worktrees to resume/prioritize,
  and act on that knowledge." Building the classification once (which content IDs are
  resumable vs. prunable, which worktrees are priority vs. deferred) and threading it through
  both `add_agent_tab`'s command choice (ACT-24) and the worktree mount order at startup
  (ACT-25) is one piece of work, not two.

---

## `F-CORE-ACT-25` — NOT EXERCISED

**Needs: reclassify → build**, on stronger evidence than the manifest currently records.

The manifest's own reasoning ("single post-restart snapshot can't distinguish absent
priority/deferred split from one resolving faster than sampled") is honest about the *live*
capture being ambiguous — but it doesn't need to stay ambiguous, because the code answer is
unambiguous: `BootstrapRestoreOrder::partition` (`rust/crates/tiller_activity/src/
bootstrap.rs:16`) — the function this row's clause names — has **exactly one caller in the
entire workspace**, and it is its own test
(`rust/crates/tiller_activity/tests/activity_domain_integration.rs:179`). Nothing in
`rust/crates/tiller/src` calls it. There is no priority/deferred split for the live snapshot
to have resolved "faster than sampled" — the function that would produce that split is never
invoked at startup at all. This is the same shape DEAD-MODULES.md already established for
`ids_to_evict` (ACT-26, below) and for half a dozen other mechanically-unreachable
`tiller_activity` symbols: `NOT EXERCISED` reads as "code looks right, proof owed," but the
correct read here is `FAILED — absent` — the reachability check is stronger evidence than the
inconclusive live snapshot, and it settles the direction the snapshot couldn't.

Not changing the verdict per this pass's brief — flagging it. The build, when it happens, is
the same subsystem as ACT-24 (see sharedCause there): whatever startup path decides restore
order needs to call `BootstrapRestoreOrder::partition` with the real open/selected worktree
ids, then mount `priority` before `deferred`.

- **files**: `rust/crates/tiller/src/main.rs` (startup/restore path — wherever worktrees get
  remounted on launch needs to call `partition` and honor its ordering),
  `rust/crates/tiller_activity/src/bootstrap.rs` (no change expected; already correct and
  tested in isolation)
- **size**: L (same subsystem as ACT-24 — do not schedule as a separate small item)

---

## `F-CORE-ACT-26` — FAILED — absent

**Needs: build.** Confirmed: `WorktreeMountPolicy::ids_to_evict`
(`rust/crates/tiller_activity/src/mount.rs:9`) has exactly one caller in the workspace, its
own test (`activity_domain_integration.rs:211`). Nothing in `main.rs` calls it, and — this is
the sharper finding — the *setting* that would drive it is equally disconnected:
`AppSettings::mount_cap` (`rust/crates/tiller_project/src/settings.rs:12`, clamped 1-64,
default 8) has **zero references anywhere in `rust/crates/tiller/src/main.rs`**. It's
persisted, clamped, and completely inert — nothing reads it to decide when to evict, and
`ids_to_evict` — the function that would act on that decision — is never called. There is no
partial eviction path to find; both the policy and the trigger are absent together.

- **files**: `rust/crates/tiller/src/main.rs` (needs an eviction call site — likely wherever a
  worktree is opened/mounted, check the current mount_cap against open worktree count and call
  `ids_to_evict`), `rust/crates/tiller_activity/src/mount.rs` (no change expected; correct and
  tested in isolation), `rust/crates/tiller_project/src/settings.rs` (no change expected;
  `mount_cap` field already exists and clamps correctly)
- **size**: M
- **sharedCause**: shares its root with the mount-cap half of `F-CORE-SET-01` below — one
  missing "read `mount_cap`, call `ids_to_evict` when it's exceeded" call site closes both.
