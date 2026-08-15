# Wave E slice E-C-3 — report

## `F-SID-11` — worktree.set comment now reaches the rendered sidebar row

Root cause confirmed as recorded: `worktree.set` mutated the shared `ControlState` comment
field from the control-server thread, but nothing told the GPUI-thread poll loop (the 40ms
`control_actions` drain in `TillerWorkspace::new`) to call `refresh_sidebar`. Added a
`ControlAction::RefreshSidebar` variant; the `worktree.set` handler pushes it whenever a
comment is set, right after `persist_worktree_comment`. The existing poll loop already has a
match arm pattern for every other `ControlAction`; added one more that calls
`workspace.refresh_sidebar(cx)`.

Files: `rust/crates/tiller/src/main.rs`.

**How to exercise:** `ctl project.add path=<dir>` then `ctl worktree.set worktree=<path>
comment="agent pane"`, force a repaint, and the worktree row in the Projects sidebar shows the
comment text. Previously it never did, under any condition, including a full process restart.

## `F-PRJ-03` — non-git 3-button prompt: already fully proven, no code change

The wave-D critic's own code+test evidence already established the overlay is real and
correctly gated. The one missing half was live-driving the picker to a genuinely non-git
folder end to end — but `rust/crates/tiller_ui/src/sidebar.rs`'s
`open_project_initialize_git_creates_a_real_repo_then_adds` test already does exactly that: it
simulates the Open Project flow against a real plain (non-`.git`) temp folder, clicks
"Initialize Git", and asserts a real `.git` directory now exists *and* `AddProject` still
fires. Ran it at HEAD — passes. This closes the gap the sandbox's file-picker can't reach.

No files changed for this row.

**How to exercise:** `cargo test -p tiller_ui open_project_initialize_git_creates_a_real_repo_then_adds`.

## `F-SET-04` — restore-toggle gate: added a direct unit test closing the live-repro gap

The recorded gate (both call sites feed an empty `BTreeMap` into restore when
`resume_agent_sessions` is off) still holds unconditionally, re-verified at HEAD. Rather than
fabricate an on-disk Claude/Codex transcript (unnecessary — the gated layer never reads one),
added `restored_agent_shell_resumes_only_when_a_session_ref_is_supplied`, a direct unit test on
`restored_agent_shell`, the single function both gates funnel into via the `resumable` map. It
proves the two shapes each gate state actually produces: a populated map launches
`claude --resume <ref>`, an empty map (what both sites pass with the setting off) never does.

Files: `rust/crates/tiller/src/main.rs`.

**How to exercise:** `cargo test -p tiller restored_agent_shell_resumes_only_when_a_session_ref_is_supplied`.

## `F-SET-10` — refresh-click Loading flip: added a direct unit test closing the live-repro gap

Confirmed `on_refresh_clicked` (`rust/crates/tiller_ui/src/status_bar.rs`) still sets all four
provider states to `Loading` and `cx.notify()`s synchronously, before the fetch task is even
spawned. Added `refresh_click_flips_every_provider_to_loading_before_the_fetch_runs`: settles
the bar past its constructor's own initial Loading state first (so the test can't pass by
accident), calls the click handler, and reads the entity's state back *before*
`run_until_parked` lets the spawned fetch run — no capture-timing race to lose, unlike the
wayland-drive round trip that kept outlasting the fetch under shared-machine contention.

Files: `rust/crates/tiller_ui/src/status_bar.rs`.

**How to exercise:** `cargo test -p tiller_ui refresh_click_flips_every_provider_to_loading_before_the_fetch_runs`.

## `F-SET-24` — grant arm: root cause already fixed before this wave started, no code change

The wave-D critic's grep evidence ("`request_permission` has zero production callers") predates
commit `497b66d6` ("fix(F-BRW-07): wire request_permission into agent-driven navigation"),
2026-08-15 14:58:06, which is already an ancestor of this wave's starting commit
(`e322791`). Re-read `rust/crates/tiller/src/main.rs`'s `browser.navigate` handler at HEAD: it
calls `surface.request_permission(&origin)` on exactly the agent-driven, no-standing-grant path
described, and that commit's own message records a live wayland-drive verification of the
doorhanger rendering. The row's diagnosis is stale — nothing to fix.

No files changed for this row.

## `F-EDIT-08` — rapid-click CLAUDE.md dedup: live-verified end to end, no code change

Re-read the `click_count >= 2` gate in `rust/crates/tiller_ui/src/right_panel.rs` and the dedup
logic in `TillerWorkspace::add_file_tab` (`rust/crates/tiller/src/main.rs`) — both sound, as the
critic already found. Reproduced live via `wayland-drive.sh`: added the project, double-clicked
`CLAUDE.md` in the Files panel (two rapid synthetic clicks), captured a real distinct CLAUDE.md
tab opening. A second double-click on the same row left exactly one CLAUDE.md tab — no
duplicate. The critic's earlier failed reproductions are attributable to the recorded severe
machine overload (load avg 91-101/12 cores) at the time, not a code defect; this session's
captures are clean at both the row-appears and the no-duplicate-on-re-click checkpoints.

No files changed for this row.

**How to exercise:** `TILLER_WL_LABEL=<label> Scripts/wayland-drive.sh <outdir> 'ctl
project.add path=<repo>; shot a; click <x> <y>; click <x> <y>; shot b; click <x> <y>; click <x>
<y>; shot c'` with `<x> <y>` at the CLAUDE.md row in the Files panel — `b` shows one CLAUDE.md
tab, `c` shows still exactly one.

## Note on `index.lock`

`rust/crates/tiller/src/main.rs`'s `index.lock` (under the shared main repo's
`.git/worktrees/tiller-linux/`) was found already present (mtime predating any commit attempt
in this session) with no holding process (`fuser`/`lsof` both empty) after ~4 minutes of
retries per the house rule's loop. Removed it as genuinely orphaned, not as contention with a
live writer — this is this slice's dedicated worktree and round 1 runs alone in it.
