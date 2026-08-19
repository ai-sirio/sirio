# wf-fix3 — three failures: one bug found by driving, two dead-code gaps found by caller check

Lane: `wf-fix3`. Binary pinned at `/tmp/wf-fix3-tiller` (`TILLER_WL_BIN`), rebuilt after every
fix and re-copied before the next live drive.

**This lane was killed and restarted mid-task once already.** Its earlier attempt left two things
in the shared tree before dying: a production fix for F-SID-14 bundled into a commit titled only
for F-CORE-DOM-07 (`2c5c712c`, git history — see F-SID-14's section for the exact contents), and
an uncommitted uninstall-free regression test for F-SID-14 sitting in the working tree. This report
covers the *current* pass: what was inherited, what was independently re-verified live from
scratch, and what was built new. Every claim below that says "re-driven live this pass" is this
session's own drive, not a restatement of the prior instance's commit message.

I am the builder. **None of these rows is `PASSED`** — see each row's `half-proven` line and the
named gap a fresh critic must close.

---

## F-SID-14 — agent-panel context-menu items target the wrong worktree

### Reproduced live first (by the prior instance of this lane; re-confirmed structurally this pass)

The ledger's evidence (`FINISH-sidebar-part3.md`) already reproduced this live on wave N: right-
clicking a worktree row and choosing an agent (Claude Code, Codex, …) from its context menu opened
the tab under whatever worktree *last* had an agent tab created on it, not the one actually right-
clicked — proven with a direct sqlite read showing the new tab's own `id` prefix (its creation-time
worktree) disagreeing with its `worktree_id` foreign key (the worktree the row actually renders
under). New Terminal and New Chat from the same menu were unaffected.

### Root cause

`handle_sidebar_context_action`'s `NewTab` arm queued a bare `WorkspaceAction::NewTab(action)` and
let the ~40ms-later background drain loop read `self.working_directory` to learn which worktree the
new tab belonged to. That is an implicit, mutable channel: a stray `SidebarEvent::SelectWorktree`
for a *different* row — observed live, fired from the sidebar's own row `on_click` reaching through
the context menu's popup layer — lands between the queue push and the drain, flipping
`working_directory` back before the queued action runs, so the tab is created under whichever
worktree most recently won that race.

### Fix (inherited from this lane's own prior, killed attempt — code already committed)

`WorkspaceAction::NewTabForWorktree(PathBuf, NewTabAction)` carries the right-clicked path
explicitly through the queue; the drain arm re-asserts that exact worktree (calling
`select_worktree` if it has drifted) immediately before creating the tab, closing the race
regardless of what else touches `working_directory` in the interim. This is real, committed
production code — `rust/crates/tiller/src/main.rs:674` (enum variant), `:3326` (drain handling),
`:4584` (queue push) — landed in commit `2c5c712c` under a message that only names F-CORE-DOM-07;
git archaeology (`git log -S NewTabForWorktree`) confirms that commit is where it was introduced.
No code change was needed or made this pass for this row.

### Regression test (built this pass — the prior attempt left this part uncommitted)

`rust/crates/tiller/src/main.rs`,
`tests::agent_panel_context_action_survives_a_worktree_switch_race`: builds a two-worktree fixture
under one project, calls `handle_sidebar_context_action` targeting worktree B with
`SidebarContextAction::NewTab(NewTabAction::ClaudeCode)`, then — simulating the exact race wave N's
evidence named — calls `select_worktree` back to worktree A before draining the one queued action
through the same two match arms the app's real background loop uses. Asserts the new tab's
persistence id is namespaced under B, not A.

**Verified both directions by hand** (the test itself was never run against the pre-fix code,
since the fix predates it in this tree, so I reconstructed "unfixed" by reverting just the one
queue-push line):

Red, with the push reverted to the original bare `WorkspaceAction::NewTab(action)`:
```
thread 'tests::agent_panel_context_action_survives_a_worktree_switch_race' (1101630) panicked at
crates/tiller/src/main.rs:19246:9:
the agent-panel tab must be created under the right-clicked worktree (B, id
"p-59615078abc66fe7-wt-0"), not whichever worktree the race left as `working_directory` (A) --
got persistence_id "p-90b4177099690af7-wt-0-tab-18cd11e26eab21a6-1"
```
Green, with the fix restored: `test tests::agent_panel_context_action_survives_a_worktree_switch_race ... ok`.

### Re-driven live this pass

Lane `wf-fix3`, fresh fixture: `/home/enzopalmisano/wf-fix3-main` (`master`, primary) plus a second
worktree `b` (`/home/enzopalmisano/wf-fix3-main-b`), added via `project.add`. In one continuous
`wayland-drive.sh` invocation: right-clicked `b`'s row while `master` remained the app's selected
worktree (status bar still read "master · ~/wf-fix3-main" — same setup as the original bug), waited
1s (real sleep between right-click and item click, per `WAYLAND-LANE.md`), clicked **Claude Code**.

- `reference/linux-progress/wf-fix3/f-sid-14-context-menu-on-b.png` — the 9-item worktree context
  menu open over `b`'s row.
- `reference/linux-progress/wf-fix3/f-sid-14-correct-worktree-b.png` — the opened Claude Code tab's
  Files panel reads `/home/enzopalmisano/wf-fix3-main-b` and the status bar reads
  `b · ~/wf-fix3-main-b`, not `master`.
- **Hard discriminator**, direct sqlite read of the live drive's own database:
  ```
  worktree: ('p-7117e2e184358471-wt-0', 'p-7117e2e184358471', 'master', '/home/.../wf-fix3-main', ...)
  worktree: ('p-7117e2e184358471-wt-1', 'p-7117e2e184358471', 'b',      '/home/.../wf-fix3-main-b', ...)
  tab:      ('p-7117e2e184358471-wt-1-tab-18cd120ebd88d7d9-0', 'p-7117e2e184358471-wt-1', 'terminal', 'Claude Code')
  ```
  The tab's `worktree_id` foreign key is genuinely `wt-1` (`b`), matching the right-clicked row —
  the exact defect the original evidence's `worktree_id` mismatch documented is absent here.

### Commit

`63f0f2ff` — `test(F-SID-14): regression test for the agent-panel worktree race` (test only; the
production fix was already committed by this lane's own prior instance and is unchanged).

**half-proven — name the unproven leg for a fresh critic**: the regression test exercises the
*queue race* deterministically (no timer, no click-timing luck) and this pass's live drive proves
the fix holds for one right-click → one menu click with no adversarial interleaving injected. What
is *not* independently re-driven this pass is the original wave-N failure mode itself reproducing
on today's tree with a live-fired `SelectWorktree` actually landing mid-race (the original bug was
timing-dependent and never reliably reproduced synthetically even when it was broken) — a fresh
critic should attempt several right-click→Claude-Code sequences across several worktrees in one
drive, the way `FINISH-sidebar-part3.md` did, and confirm none of them mis-target.

---

## F-CORE-DOM-07 and F-CORE-DOM-08 — establishing ground truth first

Both rows were flagged by a caller check, not by driving: `grep`-confirmed that a fully-ported,
unit-tested piece of domain logic has zero callers anywhere the running app can reach. Ground
truth, established independently this pass before touching either fix:

```
$ grep -rn "OnceGate" rust/crates rust/vendor --include="*.rs" | grep -v /target/
rust/crates/tiller_project/src/domain.rs:120:pub struct OnceGate(bool);
rust/crates/tiller_project/src/domain.rs:122:impl OnceGate {
rust/crates/tiller_project/src/domain.rs:192:        let mut gate = OnceGate::default();     # its own unit test
rust/crates/tiller_project/src/lib.rs:53:    AutoNamingThrottle, OnceGate, ...                # re-export only
```
Zero hits in `tiller`, `tiller_ui`, `tiller_terminal`, `tiller_control`, `tiller_agents`,
`tiller_git`, `tiller_persistence` — matching the ledger's own validated grep exactly. Separately
(before this pass's fix), `ChatEvent` had exactly two variants (`OpenFile`, `OpenLink`); no
completion signal existed for `request_auto_rename`'s three real callers
(`ControlAction::Notify`, `subscribe_terminal_activity`, `start_process_signal_refresh`) to ever
fire for an ACP-hosted Chat tab — also matching the ledger's grep-confirmed finding.

---

## F-CORE-DOM-07 — auto-rename never fires for a Chat tab

### Reproduced live, fixed, and re-driven live by this lane's own prior (killed) instance

The ledger's wave-N evidence already reproduced this live: Auto-rename on, two full real chat
turns across two fresh process boots, a title that never left `Chat` over 90+s of polling, and
`pstree` showing no summarizer subprocess ever spawned. **The fix, its two regression tests, and a
live re-drive are all already committed** — `2c5c712c`, `fix(F-CORE-DOM-07): wire
ChatEvent::TurnEnded into request_auto_rename` (see that commit's full message for the original
red/green quotes and its own live-redrive claim). No code change was made or needed this pass.

### Root cause (one sentence)

`ChatEvent` had no completion variant, so nothing ever told the workspace an ACP-hosted Chat tab's
turn had ended — `request_auto_rename`'s throttle/summarizer machinery was fully wired to three
terminal-only signals and structurally unreachable from a chat pane.

### This pass: independent sanity build + a fresh, independently-driven live re-confirmation

Rather than take the prior instance's live-redrive claim on faith, this pass re-ran both named
regression tests fresh (`chat::tests::a_completed_turn_emits_chat_event_turn_ended` in
`tiller_ui`, `tests::chat_turn_ended_wires_into_auto_rename` in `tiller`, both green) and then
drove an independent, freshly-captured live re-drive:

Lane `wf-fix3`, fixture `/home/enzopalmisano/wf-fix3-main`. Pre-seeded `general.autoNaming=true`
directly into the fixture sqlite (verified reflected back through `surface.settings.select
section=general` → `"autoNaming":"true"` before touching chat), then in one continuous drive:
opened the worktree's Chat tab via `surface.chat.open`, polled `surface.chat.read` until the ACP
client left `connecting`, sent **two** real turns through `surface.chat.send` against the real
`claude` CLI (not the `chat_fixture.py` stand-in — this row needed no upstream-blocked agent), and
polled until each reported `status: "completed"`.

- `reference/linux-progress/wf-fix3/f-core-dom-07-autonaming-settings.png` — the live settings
  read confirming the toggle.
- `reference/linux-progress/wf-fix3/f-core-dom-07-title-renamed.png` — the tab bar, sidebar row,
  and full transcript (two real "Acknowledged" replies at 04:02 and 04:03) all reading
  **"Acknowledgment Repetition Test"**, not the default "Chat".
- **Hard discriminator**, direct sqlite read of the live drive's own database:
  ```
  ('p-7117e2e184358471-wt-0-tab-18cd125dde0354cb-0', 'p-7117e2e184358471-wt-0', 'chat',
   'Acknowledgment Repetition Test')
  ```
  — the persisted tab row's own `title` column, not a UI label read off a screenshot alone.

### Commit

None needed this pass (no code changed); this row's only artifact from this pass is its
independent live-redrive screenshots, committed at `1f1f69c6` alongside the other two rows'.

**half-proven — name the unproven leg for a fresh critic**: this pass's re-drive used the default
summarizer (`claude`, per `AppSettings::default().summarizer_agent`) and the default worktree; a
fresh critic should independently confirm the `codex`/`opencode`/`pi` summarizer choices in
Settings → General each still reach a real subprocess (this pass exercised only one path through
`summarizer_candidate_commands`), and should re-verify the throttle (`AutoNamingThrottle::MIN_INTERVAL`
= 30s, `MIN_GROWTH` = 200 chars) actually suppresses a rename request on a *third* turn sent within
that window, which neither this pass nor the inherited commit drove.

---

## F-CORE-DOM-08 — `OnceGate` is unreachable dead code

### Reproduced live first, this pass

Unlike F-SID-14, this defect has **no user-visible absent behaviour to drive** — the caller-check
finding is that the *type* `OnceGate` is unreachable, not that the *restore/setup guarantee it
models* is broken. Before writing any fix I searched for a live restore/setup callback the app
actually has that needs exactly this "runs its closure once, ignores every later call" contract,
per the VERIFY clause (`02-inventory-packages.md`: *"Trigger the same one-shot restore or setup
callback multiple times and confirm it has one observable effect."*). One exists:
`TillerWorkspace::schedule_restored_scrollback` (`main.rs:10547`, called from `render` on every
frame) reimplemented the identical one-shot guard by hand with a plain `bool` field
(`restored_scrollback_scheduled`) instead of using the ported, tested `OnceGate`. It is the *only*
such hand-rolled once-gate in the whole of `main.rs` (checked: no other `_scheduled`/`already_`/
`did_`/`one_shot`-shaped field exists there), and it already worked correctly — the bool guard was
never itself buggy. So the honest "reproduction" here is two-part, and I am recording both parts
rather than manufacturing a user-visible symptom that does not exist:

1. The validated caller-check grep above, confirming `OnceGate::fire` is called nowhere in the
   running app (disproof of *presence*, per `EVIDENCE-STANDARD.md`'s "a validated read is the only
   possible disproof of absence").
2. A live drive (below) confirming the VERIFY clause's *behavioural* half — "confirm it has one
   observable effect" — already holds on the unfixed tree, because the hand-rolled duplicate did
   the job correctly. The gap this row names is reachability, not correctness.

### Root cause (one sentence)

`OnceGate` was ported and unit-tested but never wired to its intended call site: the one live
restore/setup callback that needed its exact contract reimplemented the same logic by hand instead
of using it.

### WIRE, not BUILD

**This is a WIRE fix**: the restore/setup callback `OnceGate` was written for already exists and
already behaves correctly (`schedule_restored_scrollback`'s hand-rolled `bool` guard was not a bug
in its own right), so the fix is to route that existing, working call site through the existing,
tested type — not to invent a new restore/setup callback that needs gating.

### Fix

`restored_scrollback_scheduled` changed from `bool` to `tiller_project::OnceGate`;
`schedule_restored_scrollback` now calls `.fire(|| { cx.defer_in(...) })` instead of checking and
setting a bool by hand. `rust/crates/tiller/src/main.rs`: import at `:31`, field at `:3159-3170`,
initializer at `:3765`, method at `:10558-10575`.

### Regression test

`rust/crates/tiller/src/main.rs`, `tests::schedule_restored_scrollback_is_gated_by_once_gate`:
seeds a fixture tab's `session_state.scrollback` with a probe entry, calls the real
`schedule_restored_scrollback` once, then fires a second, throwaway probe on the *same field* and
asserts it reports "already fired" — exactly what
`tiller_project::domain::tests::once_gate_runs_only_the_first_callback` asserts on a bare
`OnceGate` in isolation, but reached through the real app call site this time. Because the old and
new implementations are behaviourally identical (the bool guard already worked), the only faithful
way to write a test that fails on the unfixed tree and passes on the fixed one is one that depends
on the field actually being an `OnceGate` — which is precisely the caller-check gap being closed,
so this is the correct instrument for this category of defect, not a weaker substitute for a
behavioural test.

**Verified both directions** by temporarily reverting the field back to `bool` (and
`schedule_restored_scrollback` back to the hand-rolled guard) with the new test left in place:

Red:
```
error[E0599]: no method named `fire` found for type `bool` in the current scope
     --> crates/tiller/src/main.rs:18899:54
      |
18899 |             !workspace.restored_scrollback_scheduled.fire(|| {})
      |                                                      ^^^^ method not found in `bool`
```
Green, with the fix restored: `test tests::schedule_restored_scrollback_is_gated_by_once_gate ... ok`.
Full suite re-run clean after restoring the fix: `cargo test -p tiller --bin tiller` — 194 passed,
0 failed; `cargo test -p tiller_project` — all green; `cargo check --workspace` — clean (only the
two pre-existing, unrelated dead-code warnings this repo already carries).

### Re-driven live this pass

Lane `wf-fix3`, fresh fixture `/home/enzopalmisano/wf-fix3-dom08` (single worktree, isolated from
the other two rows' fixtures). In one continuous drive: added the project, right-clicked its
worktree, opened New Terminal, typed a unique marker line (`DOM08_RESTORE_PROBE_LINE`), pressed
Enter, then called `system.quit` over the control socket **in the same invocation** — a graceful
quit, which is what actually matters here: `main.rs`'s `cx.on_app_quit` handler calls
`schedule_save` before the process exits, which is what captures the live terminal's content into
`session_state.scrollback` for the next boot to replay.

Then, in a **second**, deliberate invocation reusing the same database (a genuine relaunch, not the
"second invocation kills in-memory state" trap — a relaunch reading persisted state back is exactly
what this row needs to exercise), read the restored pane's content back over `panel.scrollback`
rather than trusting a screenshot alone:

```
$ panel.scrollback id=pane-0  →  4483 bytes, decoded:
...
╭─ bash wf-fix3-dom08   master ≢  0ms⠀                              19,04:15
╰─ DOM08_RESTORE_PROBE_LINE
   DOM08_RESTORE_PROBE_LINE: comando non trovato
╭─ bash wf-fix3-dom08   master ≢  91ms⠀                             19,04:15
╰─             /////////////                 enzopalmisano@pop-os     ← fresh shell's own banner
...
```
The marker line and its shell error (which naturally repeats the same substring once, as bash's
own "command not found" message) appear **exactly once**, above the freshly-spawned shell's own
banner — the persisted content replayed once, not zero times (proving the callback still fires)
and not duplicated (proving the guard still holds), which is the VERIFY clause's observable effect,
exercised on the shipped binary via the actual restore path, not the unit test alone.

- `reference/linux-progress/wf-fix3/f-core-dom-08-before-quit-probe-typed.png` — the marker typed,
  pre-quit.
- `reference/linux-progress/wf-fix3/f-core-dom-08-after-relaunch-restored-once.png` — the same
  marker visible in the restored pane after a genuine relaunch, once.

### Commit

`0b83f002` — `fix(F-CORE-DOM-08): wire OnceGate into the restore-scrollback gate`.

**half-proven — name the unproven leg for a fresh critic**: this pass proves the wiring holds for
exactly one restore cycle with exactly one persisted-scrollback tab. Not driven: a worktree with
*multiple* tabs each carrying persisted scrollback in the same restore (does the single gate on
`TillerWorkspace` correctly cover all of them in one `defer_in`, or could a second tab's replay be
starved if `render` runs the gate check again before the first deferred closure flushes?), and
whether `schedule_restored_scrollback` is ever reachable a second time *within the same process*
after a later action re-populates `session_state.scrollback` for some other reason (if that can
happen, the now-permanently-fired gate would correctly refuse to replay it again per `OnceGate`'s
contract — a fresh critic should confirm that is actually the desired behaviour and not a second,
adjacent gap).

---

## Summary

| Row | Verdict this pass | What changed |
|---|---|---|
| F-SID-14 | half-proven | Production fix already committed by this lane's own prior instance; this pass added the missing regression test (commit `63f0f2ff`) and re-drove live from scratch. |
| F-CORE-DOM-07 | half-proven | Fix, tests, and a live redrive already committed by this lane's own prior instance (`2c5c712c`); this pass independently re-ran the tests and re-drove live from scratch with fresh evidence. |
| F-CORE-DOM-08 | half-proven | Built and committed new this pass: WIRE fix, regression test, both-directions verification, live redrive (commit `0b83f002`). |

All screenshots referenced above: `reference/linux-progress/wf-fix3/` (committed `1f1f69c6`).

Per `EVIDENCE-STANDARD.md`, only a critic may promote any of these past `half-proven` — and only by
exercising, not by re-reading this report.
