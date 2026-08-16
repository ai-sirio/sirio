# H3-tray critic verdicts

Critic pass, independent of the builder (`H3-tray-report.md`, commit `46de0584`). Read
`docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md` and the Swift reference
(`App/AgentRosterView.swift`, `App/TillerApp.swift:115`) first, as instructed. Everything below is a
fresh drive against a fresh `tiller` process (`TILLER_DB=/tmp/h3crit/db.sqlite`,
`TILLER_SOCKET=/tmp/h3crit/ctl.sock`, isolated from the builder's own stray processes, one of which
— pid 3481424 — was still alive and untouched on this machine's real COSMIC session when this pass
started), not a replay of the builder's own transcript.

## `F-USE-04` — PASSED

Live, independent, real `StatusNotifierWatcher` (`com.system76.CosmicStatusNotifierWatcher` /
`org.kde.StatusNotifierWatcher`, this machine's real COSMIC session, not the sandboxed
`wayland-drive.sh` compositor, which has none). Launched a fresh binary; `busctl --user list` showed
it registered as `org.kde.StatusNotifierItem-<mypid>-1`. `busctl call … /MenuBar
com.canonical.dbusmenu GetLayout` with no worktrees registered returned exactly `disabled "No active
agents"` + separator + `"Quit Tiller"`. Then, over the real control socket: `project.add` a scratch
repo, `workspace.select`, `panel.create`, `notify session=<id> status=needs-input` — a fresh
`GetLayout` call now returned `"master — repo1 (needs input)"` as the first item, discriminating
against the empty-roster default. This is a second, independent reproduction of the builder's own
steps 1-3 with a different scratch repo and a different process.

## `F-USE-05` — half-proven

The `select_worktree` half is independently PASSED: with two worktrees registered (repo1, repo2) and
repo2 selected (`workspace.list` confirmed `repo2.selected=true`), a real
`com.canonical.dbusmenu Event "clicked"` sent at repo1's roster item id flipped `workspace.list` back
to `repo1.selected=true` / `repo2.selected=false` — the same discriminating flip the builder reported,
reproduced independently against a fresh process and fresh worktrees. The worst-status-tab-jump half
remains unverified by either pass: the builder said plainly it "was not independently screenshotted",
and this pass's own attempt to exercise it (notifying a real tab's pane id, then reselecting the
worktree via the tray and reading `panel.list`'s `active` flag back) got tangled in this app's
single-window-per-worktree model — an unmounted worktree's real tab entries stop appearing in
`panel.list` at all, so the instrument needed to observe which tab the jump lands on doesn't yet
exist. Both code paths (`worst_status_tab_id`, `select_tab`) are real and already used elsewhere per
static reading, so this is not `FAILED`, but nobody has driven the actual jump behind the tray path.

## `F-WIN-08` — PASSED

Independently reproduced the close-intercept with a fresh drive, not the builder's own session:
`TILLER_WL_LABEL=h3critwin TILLER_WL_KEEP=1 Scripts/wayland-drive.sh` against a fresh nested-sway app
instance, action block `shot before-close; swaymsg -s "$SWAYSOCK" kill; sleep 1; shot after-close`.
`swaymsg kill` sends a real `xdg_toplevel` close request. The script's own post-action `kill -0
$APP_PID` guard did not fire (no "app died" failure), and the two screenshots are byte-identical
(`md5sum` match, `e76fb72...`) — a real close request left the process alive and the rendered frame
unchanged, confirming `on_window_should_close` returning `false` + `minimize_window()` on the path a
real compositor close event takes. The re-show half (`TrayRequest::ShowWindow` /
`activate_window()`) is the same code already exercised by `F-USE-05`'s proven click path above, not
separately re-screenshotted against a visually-minimized COSMIC window in this pass either — same gap
the builder flagged, not closed here.

## Also checked

`cargo test -p tiller tray_roster_lists_only_active_worktrees_sorted_by_urgency` — reran independently,
`1 passed; 0 failed`. Read `crates/tiller/src/tray.rs` and the wiring in `crates/tiller/src/main.rs`
(`tray_roster_snapshot`, the 40ms poll loop's roster diff/nudge, the `TrayRequest` match arms,
`on_window_should_close`) — matches what the report describes; `control_state.workspaces` (not
`project_catalog`) is indeed what `tray_roster_snapshot` reads, and `TrayHandle::nudge()` is called
whenever the snapshot changes.
