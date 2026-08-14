# Critic verdicts — E01-set (F-SET)

Adjudicated by a critic that neither drove nor built this slice. Driver return:
`docs/linux-rewrite/sweep/E01-set-evidence.md`, captures under
`reference/linux-progress/drive-E01-set/`. HEAD under test: `4073297`. No source touched;
five already-built `#[test]`/`#[gpui::test]` cases re-run from cached artifacts (all green,
no recompilation triggered — 0.4-0.5s each) to reconfirm the "UI half" claims this pass relies
on: `general_surface_settings_flow_into_the_persistence_contract`,
`summarizer_picker_is_gated_on_auto_naming_and_selects`,
`install_skill_click_reaches_the_wired_host_callback`,
`install_skill_renders_muted_and_inert_when_unwired`,
`persisted_settings_round_trip_maps_all_eighteen_fields_explicitly`.

## F-SET-04 (ledger line 292) — verdict: `half-proven` (persistence half now closed; new,
narrower half owed)

Driver's DB-persistence claim checks out and is a real upgrade. Confirmed independently:
`tiller_persistence/src/db.rs:733` reads `session.resumeAgentSessions`;
`main.rs:8247`'s `Settings::on_change` closure calls
`session_store.save_settings(&app_settings_from_snapshot(snapshot))` on every settings
change — a real write path, not a stub. The three captures (`00-current-general.png`,
`01-resume-toggled-off.png`, `02-after-relaunch-general.png`) show the toggle genuinely ON
(orange) at baseline, OFF (gray) immediately after the claimed click, and still OFF after the
claimed relaunch — pixel content matches the prose exactly. This closes the DB half the prior
verdict called absent.

**But the row's own VERIFY clause asks for more than a persisted bit**: "quit and relaunch
after each setting, and **confirm restored agent-session behavior changes accordingly**."
That functional half was not driven here — the driver only ever read
`surface.settings.read` and the SQLite row, never checked whether an actual agent session got
relaunched with `--resume`/`--continue` or fresh. Source read confirms why: `resume_command()`
(the per-adapter method that would build a `claude --resume <ref>`-style command) has **zero
production call sites** anywhere in `tiller/src/main.rs` or `session.rs` — every terminal spawn
site (`main.rs:4621`, `:5084`) calls the plain `adapter.command(...)`, never
`adapter.resume_command(...)`. This is not a gap in this drive's coverage; it is independently
confirmed **already absent, live**, by `F-CORE-ACT-24` (current ledger, line 360): "2 real
restarts each spawn Claude Code with a fresh random --session-id, no --resume/--continue
either time." The setting is a real, working, persisted boolean that currently controls
nothing. `half-proven` is the honest ceiling here — not `PASSED`, because the driver's
"exercised-working"/discriminating claim covers only the half that was never in question
(the row was already going to get its schema link from P58); the half the row's clause is
actually built around remains unproven by live evidence and is known-absent by a sibling row.

## F-SET-05 (ledger line 293) — verdict: `PASSED` (upgraded from half-proven)

Row's literal clause ("toggle automatic renaming, confirm the summarizer picker is
disabled/enabled accordingly, then choose a summarizer") is UI-only; the ledger's own
established framing additionally tracks a DB-persistence half for this cluster of rows. Both
now check out:

- UI half: `summarizer_picker_is_gated_on_auto_naming_and_selects` re-run green (cache hit, no
  recompile). Independently more convincing: the driver did **not** personally drive the
  initial flip this session, but I opened the already-committed screenshots from the earlier,
  uncommitted-write-up drive under the same label myself —
  `sweep-D1-settings/d1c-summarizer-picker-open.png` (picker open, cursor on the dropdown,
  value still "Pi", a real terminal pane visible in the background) and
  `d1d-summarizer-codex.png` (value now "Codex", same layout) — two visually distinct,
  sequential states that only a real click-through-the-dropdown produces. This is first-hand
  confirmation, not a trusted retelling.
- DB half: this session's own kill+relaunch (`02-after-relaunch-general.png`) shows
  `autoNaming` ON and `Summarizer agent: Codex` surviving a restart the driver personally
  triggered, matching the socket/SQLite reads claimed.

Caveat carried forward, not blocking: "rename behavior still has no consumer" remains true and
is independently confirmed by two already-`FAILED — absent` sibling rows
(`F-AGENT-OPENCODE-03`, `F-AGENT-OMP-03`: "no summarizer-command generator exists anywhere in
the port"). That is a real product gap but it sits outside this row's own VERIFY text (which
only asks about the toggle+picker+persistence, not about the summarization actually firing),
exactly the same scope split the ledger already draws for F-SET-06/07 below.

## F-SET-06 (ledger line 294) — verdict: `PASSED` (upgraded from half-proven)

`chat.retentionCount` clamp range is `5..=500` (`tiller_persistence/src/model.rs:443`) — zero
is genuinely unreachable via the stepper, so the row's "set zero if available" clause is
vacuously satisfied (it is not available). Toggle+stepper UI is not in dispute (drawn test
green); DB half closed the same way as F-SET-04/05: `02-after-relaunch-general.png` shows `17`
surviving this session's own relaunch, and it is independently reinforced by
`sweep-D1-settings/d1e-relaunch-general.png` — a *different*, earlier relaunch (from the
abandoned same-label session) that already showed the same `17` surviving. Two independent
relaunches, two different sessions, same result. "Trimming still has no consumer" caveat
carries forward as a note (out of this row's literal VERIFY scope, same split as F-SET-05).

## F-SET-07 (ledger line 295) — verdict: `PASSED` (upgraded from half-proven)

Row's literal clause explicitly asks to "open enough worktrees to reach [the limit], and
observe the resulting mount/unmount behavior" — that functional half is not what this drive
covers, and it is exactly the piece the *prior* verdict for this same row already carved out
by name: "eviction stays absent (**ACT-26 is its own row**)." `F-CORE-ACT-26` (current ledger,
line 362) is `FAILED — absent`, live: "`ids_to_evict` has 0 callers outside its own test...
4 worktrees survive 2 full restarts with zero evictions." Since the prior critic already
established that split for this specific row (unlike F-SET-04, where no such split was ever
recorded), I'm holding it: F-SET-07's own remaining scope is toggle+stepper+persistence, and
that is now closed live — `02-after-relaunch-general.png` shows `limitMountedWorktrees:true`,
`12` surviving this session's relaunch, reinforced the same way as F-SET-06 by
`sweep-D1-settings/d1e-relaunch-general.png` showing the same `12` surviving an earlier,
independent relaunch.

## F-SET-09 (ledger line 297) — verdict: `half-proven` (upgraded from NOT EXERCISED; a new,
narrower half owed)

Real progress: the row was blocked outright before (900s lock timeout, no click ever landed).
This drive got a live click in and it reached a real, wired consumer — confirmed independently
from source, not just the driver's `panel.list` delta: `settings.rs`'s
`install_skill_handler` closure calls `handler(agent_skill_install_command())`; `main.rs:8235`
wires that to push `WorkspaceAction::InstallSkill`; `main.rs:2341` handles it by calling
`workspace.add_terminal_tab_with_shell("Install Skill", skill_install_shell(command), ...)`.
Both `install_skill_click_reaches_the_wired_host_callback` and
`install_skill_renders_muted_and_inert_when_unwired` re-run green. This closes the "is there
even a live route" question P101 left open.

**Row's clause asks for two more things the drive did not get**: "observe success or error
feedback, and confirm the control changes accordingly." Neither happened, and source review
says neither *can* happen with the current design: `install_skill_handler` only forwards the
command with no return channel, and grep for any install-status/result field
(`skill.*status`, `install.*result`, etc.) across `settings.rs`/`main.rs`/`skill.rs` turns up
nothing — the button is permanently stateless. The captured screenshot
(`03-install-skill-clicked.png`, confirmed 1350×1250 via `file`, matching the claimed resize)
shows the "Install Skill" button unchanged right after the click, no inline state — consistent
with an independently-viewed frame from the earlier abandoned session
(`sweep-D1-settings/d1h-after-install-skill-click.png`) showing the identical static button.
The driver's own report is candid about the gap (`panel.read` on the new pane returned
"unknown pane," so even they couldn't read the spawned terminal's output to check
success/error). This is a structurally absent capability, not merely untested — `half-proven`
is the correct ceiling, naming the owed half as "in-place feedback and control-state change,"
which the current terminal-delegation design has no path to ever produce without further
wiring.

## Notes

- All four DB-half upgrades (04/05/06/07) rest on the same generic pipeline
  (`Settings::on_change` → `save_settings` → `set_setting` upsert), proven end-to-end live for
  F-SET-04's own field this session. Applying that proof to the sibling fields (05/06/07) is
  reasonable because the pipeline is field-agnostic — but it is corroboration, not a
  substitute for the field-specific live evidence, which is present for all four via the
  captures cited above (both this session's and the independently-viewed earlier session's).
- The one disagreement worth flagging loudly: the driver's structured return marks all five
  rows `"claim": "exercised-working"` / `"discriminating": true` uniformly. For F-SET-04 and
  F-SET-09 that overclaims — both rows have an explicit, textual second requirement (agent-
  resume behavior; install feedback/control-state) that was not exercised and that source
  review shows the app currently cannot produce. F-SET-05/06/07 do not have that same
  overclaim risk because their literal VERIFY text is narrower and the ledger already
  established (in each row's own prior verdict text) that the deeper functional behavior is a
  separately-tracked, already-`FAILED — absent` sibling row.
