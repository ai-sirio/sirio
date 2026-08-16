# I3-tray-jump report

Row: `F-USE-05`'s unverified half -- clicking a roster row must activate that worktree's
worst-status tab. Read `App/AgentRosterView.swift` (`select(_:)` -> `worstStatusTab(in:)` ->
`activateTab`) and the wave-H report/verdicts first, as instructed.

## What was already true at HEAD

`worst_status_tab_id` (`crates/tiller/src/main.rs`) already existed and the tray's
`TrayRequest::SelectWorktree` arm already called it after `select_worktree`, exactly as wave-H's
static reading said. Nobody had driven it, because the wave-H critic's own attempt -- notify a real
tab's pane id, click the tray, read `panel.list`'s `active` flag -- got blocked by
`panel.list` dropping the unmounted worktree's entries.

## Root cause of the block, found by driving it rather than re-reading the code

Built and live-drove two worktrees under `wayland-drive.sh` (`docs/linux-rewrite/wave-i/`'s own
scratch repos, `/tmp/i3jump-repo1` and `/tmp/i3jump-repo2`, deleted after). `TillerWorkspace` opens
exactly one `cx.open_window` for the whole app (grep confirms a single call site) and holds one flat
`tabs: Vec<OpenTab>` with no worktree/project field on `OpenTab` at all -- this matches the ledger's
own `F-CHG-19` note ("single-open-worktree model"). `select_worktree` (`main.rs:4463-4544`) updates
`working_directory`, the right panel, breadcrumb, sidebar highlight and status bar, but **never
touches `self.tabs`**. Live proof: with repo1's own Chat+Terminal tabs on screen, `workspace.select
workspace=/tmp/i3jump-repo2` flipped every metadata surface (sidebar dot, Files panel, footer
breadcrumb) to repo2 while the **center pane kept showing repo1's live terminal**, and the sidebar's
own nested-tab rows re-labeled that same tab as belonging to repo2 (`sync_activity` ->
`sync_control_panes` re-tags `self.tabs`, unchanged, under the new `working_directory` on every
worktree switch). That re-tagging is exactly the mechanism behind "an unmounted worktree's tab
entries drop from `panel.list`": the old path's entries are explicitly cleared
(`self.panes.set_external(&old_path, Vec::new())`) and never replaced, because nothing reloads
`self.tabs` for the new path.

This is a real, deeper, pre-existing model constraint, not something safe to fix inside this row: a
correct fix means per-worktree tab hydration (reusing `restore_tabs_in_workspace`, today only wired
from `restore_launch_snapshot`'s one-time bootstrap) plus save-before-switch persistence ordering,
touching a function `F-SID-14`, `F-CORE-ACT-26`, `F-CHG-19`, `F-TERM-11` and others already depend on
for their current PASSED verdicts. Out of scope for I3-tray-jump; documented here as the exact
constraint per the task's option 3.

## What was built: a small, honest door (option 1)

Factored the tray handler's own three-step sequence (`select_worktree` -> `worst_status_tab_id` ->
`select_tab`) into `TillerWorkspace::select_worktree_and_jump`, called by **both** the real
`TrayRequest::SelectWorktree` arm and a new control method, `tray.jump` (`workspace` param, added to
`system.capabilities`). This is not a parallel test-only stand-in -- it is the literal production
code path, so driving it over the socket proves the tray arm itself. `control_select_worktree_and_jump`
reports `jumped` (`true`/`false`), and on a jump, `tabId` + `tabTitle`.

## Live verification (option 2, driven end-to-end)

Single `wayland-drive.sh` invocation (same-invocation, per the standing trap): `project.add` two
scratch repos, select repo1, create a Terminal tab (pane-1) via a real click, `notify
session=pane-1 status=needs-input`, `workspace.select workspace=/tmp/i3jump-repo2` (repo2 now
"selected" everywhere -- screenshot confirms), then `ctl tray.jump workspace=/tmp/i3jump-repo1`:

```
{"branch":"master","id":"...-repo1-wt-0","jumped":"true","path":"/tmp/i3jump-repo1",
 "project":"i3jump-repo1","tabId":"1","tabTitle":"Terminal"}
```

Follow-up `panel.list` confirmed `pane-1` (`Terminal`) flipped `active:true`, `pane-0` (`Chat`)
`active:false` -- the exact `panel.list`-active-flag proof wave-H's critic wanted, now reachable
because `tray.jump` is the same production call the D-Bus click makes. (An earlier attempt across
**separate** `wayland-drive.sh` invocations picked the wrong tab -- that was `AgentActivityModel`'s
own needs-input status having decayed across the real wall-clock gap between invocations, not a
defect in the jump logic; the same-invocation run above is decisive.)

A live D-Bus roster click (real `StatusNotifierWatcher`, matching wave-H3's own method) was not
re-run here since `tray.jump` already exercises the identical code that arm calls; wave-H already
independently proved the `select_worktree` half of that click path live.

## Regression test landed in the repo

`tests::tray_jump_lands_on_the_target_worktrees_worst_status_tab` (`crates/tiller/src/main.rs`) --
two tabs, tab 1 pushed to `NeedsInput` while a *different* worktree is current, asserts
`select_worktree_and_jump` returns `Some((1, "Terminal 1"))` and both `working_directory` and
`active_tab` land correctly. `cargo test -p tiller`: 162 passed, 0 failed (up from 161; no
regressions).

## Verified

- `cargo build -p tiller` -- green, no new warnings.
- `cargo test -p tiller` -- 162 passed, 0 failed.
- Live `wayland-drive.sh`, single invocation, screenshots + `panel.list` as above.

Commit: `5e02ac53 feat(I3-tray-jump): expose the tray roster's worst-status-tab jump over the control socket`

**howToExercise**: `cargo test -p tiller tray_jump_lands_on_the_target_worktrees_worst_status_tab`
for the pure-logic regression. Live: `TILLER_WL_LABEL=x Scripts/wayland-drive.sh /tmp/x '
ctl project.add path=<repoA>; ctl project.add path=<repoB>; ctl workspace.select workspace=<repoA>;
click <New Terminal button coords>; ctl notify session=pane-1 status=needs-input;
ctl workspace.select workspace=<repoB>; ctl tray.jump workspace=<repoA>; ctl panel.list'` (one
invocation) -- `tray.jump`'s reply should show `jumped:true, tabId:1, tabTitle:"Terminal"`, and the
follow-up `panel.list` should show `pane-1` `active:true`. This is the literal call the tray's own
`TrayRequest::SelectWorktree` arm makes, so it proves that arm, not a stand-in for it. The deeper
single-tab-list model constraint (a genuinely different worktree's own tabs are not hydrated by
`select_worktree`) remains open and is not something this row's fix touches -- see "Root cause" above
for the design sketch a real fix would need.
