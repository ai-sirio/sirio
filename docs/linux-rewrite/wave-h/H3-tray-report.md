# H3-tray report

Read `docs/linux-rewrite/tasks/P128-platform-exemptions-overstated.md` first, as instructed. It put
`F-USE-04`, `F-USE-05`, `F-WIN-08` back in scope after they were wrongly marked `N/A — platform`, and
flagged that all three resolve through one new StatusNotifierItem subsystem GPUI ships none of.

Checked what was already vendored before assuming a fresh fetch would work: `zbus 5.19.0` was already
in `Cargo.lock` transitively (confirmed by `grep` before touching anything), and the network was
reachable — `cargo info ksni` and a scratch `cargo add ksni@0.3.6` both resolved and downloaded clean.
Added `ksni = { version = "0.3.6", features = ["blocking"] }` as a direct dependency of `tiller`
(`crates/tiller/Cargo.toml`); "blocking" runs the whole D-Bus service on its own OS thread with a
synchronous `spawn()`/`Handle::update()`, so nothing here needed an async runtime wired through GPUI's
own executor.

## `F-USE-04` / `F-USE-05` / `F-WIN-08` — StatusNotifierItem tray + roster + hide-on-close — **implemented**

New module `crates/tiller/src/tray.rs`:

- `AgentRosterTray` implements `ksni::Tray`: `id`/`title`/`icon_name` are static; `menu()` reads a
  shared `Arc<Mutex<Vec<TrayRosterEntry>>>` (`SharedRoster`) and renders either a disabled
  "No active agents" row or one `StandardItem` per worktree (`"<branch> — <project> (<status>)"`,
  using the already-existing `AgentStatus::human_label()`), then a separator and "Quit Tiller". A
  left-click on the icon itself (`Tray::activate`) and every roster-row click push a `TrayRequest`
  (`ShowWindow` / `SelectWorktree(path)` / `Quit`) into a second shared queue.
- `tray::spawn()` registers the SNI service and returns `None` (logged, not fatal) if no
  `StatusNotifierWatcher` answers — headless CI, a WM with no tray applet — so the app runs exactly as
  it did before this module existed in that case.

Wiring in `main.rs` (`TillerWorkspace::new` gained three new trailing parameters —
`tray_roster`/`tray_requests`/`tray_handle`, all `Option`, `None` in both test-helper call sites):

- **F-USE-04**: `tray_roster_snapshot(&self)` reads `self.control_state.lock().workspaces` — not
  `self.project_catalog` — for the live worktree list (see "what nearly shipped broken" below), joins
  each against `self.panes.list_for(path)` + `self.activity.status_for_panes(&refs)` (the exact same
  two calls `evict_over_capacity_worktrees`'s `status_of` closure already makes), and sorts with
  `AttentionSort::sorted` — the same helper the sidebar uses. The existing 40ms poll loop (already
  draining `WorkspaceAction`/`ControlAction`) now also diffs this snapshot against the last one sent
  and, on change, writes it into `SharedRoster` and calls `TrayHandle::nudge()`.
- **F-USE-05**: `TrayRequest::SelectWorktree(path)` calls the existing `workspace.select_worktree`,
  then `worst_status_tab_id(&self, cx)` (same priority order as `worktree_status`'s aggregate dot) and
  the existing `workspace.select_tab`, then `window.activate_window()`.
- **F-WIN-08**: the window's `on_window_should_close` now returns `false` and calls
  `window.minimize_window()` instead of letting the close proceed — GPUI's `PlatformWindow` trait has
  no hide/show pair on Linux (checked the vendored `gpui`/`gpui_linux` source directly), so minimize is
  the closest primitive that keeps the window (and every pane's PTY) alive. `TrayRequest::ShowWindow`
  (the tray icon's own click, and the tail of a roster-row select) is the re-show half — the natural
  surface here, since Linux has no Dock icon to click.

### What nearly shipped broken, and how it was caught

The first version read `self.project_catalog.projects()` for the roster's worktree list. It built and
the tray registered fine, but live-driving it (`project.add` a scratch repo, `panel.create` a pane,
`notify` it `needs-input`) left the DBusMenu answering "No active agents" forever — `project_catalog`
is this window's own render-facing snapshot and does not pick up a bare `project.add`/`worktree.set`
the way `control_state.workspaces` (what `evict_over_capacity_worktrees` actually reads) does. Switched
to `control_state`, rebuilt, redrove — the roster populated.

That surfaced a second, independent bug: even with the right data source, the DBusMenu client kept
seeing the stale layout. Reading `ksni`'s vendored source (`~/.cargo/registry/src/.../ksni-0.3.6/src/`)
showed `GetLayout` serves a **cached** menu tree built once and only rebuilt (with a revision bump and
a `LayoutUpdated` signal) inside `Handle::update()` — writing straight into `SharedRoster` changes what
`Tray::menu()` would *return* if ksni called it again, but never tells ksni to call it again. Fixed by
keeping the `ksni::blocking::Handle` alive (wrapped as `tray::TrayHandle`, not `mem::forget`-ed as the
first draft did) and calling `TrayHandle::nudge()` — `handle.update(|_| {})` — from the poll loop
whenever the roster snapshot actually changed.

### Live verification (real COSMIC session, not the nested Wayland lane)

`wayland-drive.sh`'s private `sway` compositor has no `StatusNotifierWatcher`, so the tray itself can
only be exercised against a real session bus with a real watcher. This machine has one running
(`com.system76.CosmicStatusNotifierWatcher` / `org.kde.StatusNotifierWatcher`, owned by
`cosmic-applet-status-notifier`) — confirmed with `busctl --user list` before relying on it. Ran the
built binary directly (`TILLER_DB=`/`TILLER_SOCKET=` pointed at scratch paths, isolated from any real
Tiller state) and drove it with `Scripts/control-probe.py` + raw `busctl` D-Bus calls — no screenshot
of the real desktop was taken, since that would capture more than this app's own window.

1. `busctl --user list` showed `org.kde.StatusNotifierItem-<pid>-1` with every SNI property populated
   (`IconName "utilities-terminal"`, `Menu "/MenuBar"`, `Status "Active"`, …) immediately after launch.
2. With no worktrees registered, `busctl call … /MenuBar com.canonical.dbusmenu GetLayout …` returned
   exactly `[No active agents (disabled)] [separator] [Quit Tiller]`.
3. `project.add` + `workspace.select` + `panel.create` + `notify … status=needs-input` over the control
   socket, then `GetLayout` again: the roster row appeared verbatim —
   `"master — h3tray-repo (needs input)"`.
4. Added a second worktree and `workspace.select`-ed it (first worktree's `selected` flipped to
   `false`). Sent a real `com.canonical.dbusmenu.Event "clicked"` at the first roster row's item id:
   `workspace.list` immediately showed the first worktree back at `selected:true` and the second at
   `false` — F-USE-05's `select_worktree` half proven through the real menu-click path, not just code
   reading.
5. Sent `Event "clicked"` at the "Quit Tiller" item: the process exited (`ps -p <pid>` came back
   empty) — F-USE-04's Quit item proven end to end.
6. For F-WIN-08, launched under `wayland-drive.sh` (`TILLER_WL_KEEP=1`) and used `swaymsg -s
   $SWAYSOCK '[con_id=N] kill'` against the mapped toplevel — sway's `kill` sends a real close request
   to the client, not a forced kill. The process stayed alive and a follow-up `shot` in the same
   session showed the window still fully rendered with its terminal content unchanged. This proves the
   close-intercept half (`on_should_close` returning `false`) unconditionally — a genuine close request
   did not destroy the window or its pane. It does **not** prove the visual "hide": sway is a tiling
   compositor and does not honor `xdg_toplevel::set_minimized` (a pure client hint with no required
   compositor behavior) the way COSMIC's own compositor does; the window stayed on-screen rather than
   disappearing. The re-show half (`activate_window()`/`ShowWindow`) is exercised by the same code path
   already proven working in step 4 above (every `TrayRequest` arm calls it), but was not separately
   screenshotted against a real minimized COSMIC window in this pass.

### Verified

- `cargo build -p tiller` — green, no new warnings.
- `cargo test -p tiller` — 156 passed, 0 failed (up from 155; added
  `tray_roster_lists_only_active_worktrees_sorted_by_urgency`, a `#[gpui::test]` that populates
  `control_state`/`panes`/`activity` the same way the live drive above did and asserts the roster
  excludes a pane with no notified status and orders Error ahead of Done regardless of insertion
  order).
- Live D-Bus verification against a real `StatusNotifierWatcher`, steps 1–6 above.

Commit: `46de0584 feat(F-USE-04,F-USE-05,F-WIN-08): add a StatusNotifierItem tray with the agent roster`

**howToExercise**: `cargo test -p tiller tray_roster_lists_only_active_worktrees_sorted_by_urgency`
runs the pure-logic regression directly. Live, on any machine with a real tray host (COSMIC/GNOME/KDE
panel): launch `rust/target/debug/tiller`, confirm a terminal-icon tray item appears in the panel: with
no agents running it opens to "No active agents" + "Quit Tiller"; drive one pane to `needs-input` (e.g.
`tillerctl notify --session <pane-id> --status needs-input`, or the control socket's `notify` method
directly) and the tray menu should grow a "`<branch>` — `<project>` (needs input)" row that, when
clicked, reselects that worktree in the sidebar and switches to its worst-status tab. Closing the main
window (its own close button, not Quit) must leave the process running and the tray icon present;
clicking the tray icon or a roster row must bring the window back with pane content intact.
