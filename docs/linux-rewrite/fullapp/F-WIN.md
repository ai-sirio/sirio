# F-WIN — Window and application shell (12 rows)

Fresh, independent critic pass. Live-drove against the warm binary (`/dev/shm/tt/debug/tiller`)
via `Scripts/wayland-drive.sh`, label `finwin1` (main pass) plus short calibration/verification
invocations under labels `finwinportal*`/`zzqcritwin*`. Fixtures: `/dev/shm/finwin-fixture` (a
throwaway git repo, worktree used for the terminal/tab rows) and
`/home/enzopalmisano/finwin-dialogfixture`, `/home/enzopalmisano/finwin-projectfixture` (created
for the portal-dependent rows but never reached — see "Not reached" below). Screenshots referenced
below live under `/dev/shm/sweep-0-F-WIN/` (not committed — the deliverable is this report; the
frames were inspected directly, not assumed).

I did **not** simply replay the existing ledger verdicts. Row by row below, where I reproduced a
prior PASSED I say so with my own evidence; where I could not reproduce the prior evidence within
this session I say that loudly instead of forwarding it.

## Verdict table

| row id | verdict | evidence |
|---|---|---|
| F-WIN-01 | PASSED | Live: clicked the status-bar gear (21,953) → Settings surface replaced the workspace (`13-12-settings.png`, landed on Appearance); clicked Back (44,52) → workspace returned with both terminal tabs untouched (`14-13-back-to-workspace.png`). |
| F-WIN-02 | PASSED | Live: `chord ctrl t` on a worktree with one existing "Terminal" tab produced a genuine second "Terminal" tab, selected, with a live streaming pfetch PTY (`08-07-new-terminal-ctrlt.png`). |
| F-WIN-03 | NOT EXERCISED | Attempted 13 consecutive live invocations requiring the harness's virtual keyboard; all 13 failed at `start_virtual_keyboard` under severe host memory pressure (swap 15/31 GiB used, `/dev/shm` 91% full) — see "Harness blocker" below. Not driven this pass. |
| F-WIN-04 | PASSED | Live: `chord ctrl+shift s` hid the Projects sidebar (`09-08-sidebar-hidden.png`), a second `chord ctrl+shift s` restored it exactly (`10-09-sidebar-restored.png`). |
| F-WIN-05 | PASSED | Live: `chord ctrl+shift i` hid the Files right panel (`11-10-rightpanel-hidden.png`), a second chord restored it (`12-11-rightpanel-restored.png`). |
| F-WIN-06 | FAILED — defective | Live, reproduced twice cleanly: `+` → **New Browser** does create a real "Browser" tab with sidebar entry and address-bar chrome (`02-34-recheck2-after-newbrowser.png`) — but the embedded page can never render. A permanent red banner reads "Direct XCB build failed: the window handle kind is not supported; XCB→Xlib adapter failed: GPUI returned unsupported handle: Wayland(WaylandWindowHandle{...})". Traced to source — see "Defects" below. |
| F-WIN-07 | PASSED | Live: closed the launch-snapshot "Terminal" tab (real "Close dirty tab?" confirmation dialog, clicked Close — `05-04-close-dirty-confirm.png`, `06-05-terminal-closed.png`), then `chord ctrl+shift o` restored exactly that tab (`07-06-restored.png`, fresh PTY, same worktree). |
| F-WIN-08 | half-proven | Live: `swaymsg kill` (a real compositor `xdg_toplevel.close` request) against the app window. `pgrep -P $APP_PID` returned the identical 3 child PIDs before and after; `kill -0 $APP_PID` succeeded after; the window kept rendering (`25-24-after-close-attempt.png`). The "process/pane survive a close" half is proven live. The "reopen it from the Dock" half is **UNREACHABLE** in this lane — this headless sway config runs no tray/StatusNotifierWatcher host, and source (`App`'s `tray::TrayRequest::ShowWindow → window.activate_window()`) confirms reopen is tray-driven only; there is nothing to click. |
| F-WIN-09 | half-proven | Live: confirmed my harness's double-click genuinely reaches GPUI (positive control: double-clicking a tab label triggered its rename-field, `raw-after-tab-dblclick.png`). A double-click on an empty titlebar area at (700,18), captured via two independent raw `grim` frames (bypassing `shot()`'s own resize dance, which the script's own header warns can desync anchored UI), produced **no observable change** — window stayed mapped, unminimized, fully rendered, app alive. Live `gsettings get org.gnome.desktop.wm.preferences action-double-click-titlebar` on this host returns `'minimize'`, and source (`titlebar.rs`) is genuinely wired to call `window.minimize_window()` on that value. I could not determine whether the null result is an app no-op or `sway` (a tiling WM with no taskbar) simply not honoring `xdg_toplevel` minimize — a real, documented gap in many wlroots compositors, not something Tiller controls. Recorded half-proven rather than a defect, per the standard of proof: I cannot tell app bug from compositor limitation here. |
| F-WIN-10 | NOT EXERCISED | Same harness blocker as F-WIN-03 (needs both the keyboard and the portal-backed folder picker for the real `+ → Open Project…` UI path that calls `add_project`/`show_toast` — the `ctl project.add` socket path is a **different** function, `control_add_project`, that never shows a toast, confirmed by reading `rust/crates/tiller/src/main.rs`). Not driven this pass. |
| F-WIN-11 | PASSED | Live: drove all 6 `update.event` states plus reset via the control socket, with unique injected marker text so the toast content is provably live, not cached: `available version=FINWIN-9.9.9` → "Tiller FINWIN-9.9.9 is available" + Download (`18-17-update-available.png`); `check-started` → "Checking for updates…" (`17-16-update-checking.png`); `download-progress percent=42` (`19-18-update-downloading.png`); `install-started` → "Installing update…" (`20-19-update-installing.png`); `finished` → "Tiller is up to date" (`21-20-update-finished.png`); `failed message=FINWIN_FAIL_9182` → "Update failed: FINWIN_FAIL_9182" + Retry (`22-21-update-failed.png`); `reset` → toast gone (`23-22-update-reset.png`). |
| F-WIN-12 | N/A — platform | Independently confirmed (not just trusted from the ledger): `grep -rn "onboarding\|FirstRun\|PermissionsOnboarding" rust/crates/` finds nothing but one unrelated `auto_naming_requires_first_run_or_both_throttles` unit test whose name happens to contain the substring "first_run" — there is no permissions-onboarding sheet or code path anywhere in the Linux crate tree. Matches the ledger's own note that this is a macOS-only TCC sheet with no Linux counterpart. |

## Where I disagree with the recorded ledger

- **F-WIN-06 was PASSED in the ledger** ("wave H live drive: tab-strip + menu → New Browser, real
  tab+sidebar entry+chrome, live"). That claim is literally true as far as it goes — I reproduced
  the tab/sidebar/chrome creation myself, twice. But the ledger evidence stops at "a tab appeared"
  and never actually looked at what the tab *shows*. It shows a permanent, unrecoverable error
  banner instead of a web page, on every single load, by construction (see Defects below). A user
  clicking "New Browser" gets a broken feature, not a working one. I am overriding PASSED to
  **FAILED — defective**.

## Defects

**F-WIN-06 — the embedded WebView can never render on a native-Wayland Tiller session.**
`rust/crates/tiller_ui/src/browser.rs`'s own top-of-file doc comment describes the design: "P72
browser-composition surface (WebKitGTK via `wry` in a native X11 child window, composited alongside
GPUI's own X11 surface)". `BrowserSpike::new` (lines ~90-120) asks GPUI for the window's raw handle
and only handles `RawWindowHandle::Xcb` (direct) or wraps it as `RawWindowHandle::Xlib` (adapter);
every other handle kind — including `RawWindowHandle::Wayland`, which is exactly what GPUI hands
back when Tiller runs as a native Wayland client (its normal/only mode under this whole harness,
and the mode `Scripts/wayland-drive.sh`'s own comments describe as how the app runs) — falls into
`raw => Err(format!("GPUI returned unsupported handle: {raw:?}"))`. There is no Wayland embedding
path in this module at all, gated only by `#[cfg(target_os = "linux")]`, not by windowing system.
Reproduction: `+` (1289,50) → **New Browser** (1043,144 in a clean/unperturbed menu render — see
Harness note below) → the tab opens showing exactly this error, unconditionally. Screenshot
`/dev/shm/sweep-0-F-WIN/02-34-recheck2-after-newbrowser.png`.

## Harness notes (recorded so the next critic doesn't re-spend the same effort)

1. **`TILLER_WL_PORTAL=1`'s built-in bootstrap is broken.** It starts `xdg-desktop-portal` and its
   GTK backend *before* `sway` even launches (so there is no `WAYLAND_DISPLAY` for GTK to attach
   to), and the script never calls `dbus-update-activation-environment` after `sway` comes up to
   tell the private bus about the compositor's display. Every backend activation in
   `$LABEL-dbus.log` shows `Gtk-WARNING: cannot open display:` and `Activated service
   'org.freedesktop.impl.portal.desktop.gtk' failed: Process ... exited with status 1`. The
   `wayland-drive.sh` recipe in this task's own brief and the flag's doc comment both imply this
   "just works" with `TILLER_WL_PORTAL=1` — it does not, on this host, as shipped. A working
   pattern (confirmed against the very recent `wf-dom3` pass, `docs/linux-rewrite/FINISH-domain-part2.md`)
   is to skip the flag and manually run `dbus-update-activation-environment WAYLAND_DISPLAY="$WD"`
   plus start `/usr/libexec/xdg-desktop-portal` yourself, from *inside* the action block, after
   `$WD` is known.
2. **A `shot` call between opening an anchored dropdown menu and clicking one of its items can
   desync the click target**, beyond what the script's own header already warns about (the
   2-frame `deferred` link race). I calibrated the tab-strip "+" menu's "New Browser" row position
   from a screenshot taken via `shot` right after opening the menu, then reused that coordinate in
   a later invocation with more tabs open — it missed (closed the menu, created nothing) twice in
   a row. Only after skipping `shot` between menu-open and item-click (plain `sleep` instead, per
   the header's own recommended-but-easy-to-misread guidance) did the click land reliably, twice.
   Recorded as a lane trap, not an app defect: comparing `identify -trim` on the "bad" frames
   showed the app's own window shrunk to exactly the script's internal `W2xH2` (1400×900) constant
   with `sway`'s grey background visible around it — i.e. the anchored popup (and the window
   itself) can still be mid-resize when a `shot` immediately follows opening it.
3. **The harness's virtual-keyboard startup (`start_virtual_keyboard`, `wtype`-backed) failed 13
   consecutive times** across roughly 15 minutes of this session, always at the same point
   (`wtype ... -s 14400000 ...` exits before the hard-coded 0.1s `kill -0` check), always for
   scripts needing `chord`/`type`/`key`. This happened *after* my main pass had already used
   `chord` successfully more than a dozen times without issue, so it is a real regression in host
   conditions mid-session, not a property of my scripts: `uptime` showed load average 9.5–26 on a
   12-core box throughout, and `free -h` showed **15 of 31 GiB of swap in use** and `/dev/shm` at
   91% full the whole time — consistent with this task's own briefed warning about a runaway log
   process (`rsyslogd`/`systemd-journald` both showed 70–80% sustained CPU in `ps aux
   --sort=-%cpu`). I manually reproduced `sway` + `wtype` working correctly in isolation outside
   the wrapper (given more settle time than the wrapper's fixed 0.1s check allows), which points at
   a startup-timing race that only bites under this specific memory/CPU pressure, not a
   `wtype`/`sway`/app defect. I am recording this as a harness/environment limitation, not a
   defect, per the standard of proof — but it is the reason F-WIN-03 and F-WIN-10 are NOT
   EXERCISED below rather than driven.

## Not reached this pass

- **F-WIN-03** (open/save a file via `⌘O`/`⌘S` → Linux `ctrl+o`/`ctrl+s`) and **F-WIN-10** (toast
  on a duplicate `+ → Open Project…`) both need the portal-backed native folder/file picker, which
  needs the virtual keyboard (for `chord ctrl o` and for typing/`Return` inside the GTK dialog).
  Blocked by the harness condition in note 3 above across every attempt (13 tries, labels
  `finwinportal`, `finwinportal2/3/5/6/7/8/A/B/C`, `zzqcritwin1-4`). Fixtures were prepared and are
  still on disk for whoever picks this up next: `/home/enzopalmisano/finwin-dialogfixture/open-me.txt`
  (contains `FINWIN_ORIGINAL_LINE`, for F-WIN-03) and `/home/enzopalmisano/finwin-projectfixture/`
  (an empty non-git folder, for F-WIN-10 — add it once cleanly via the UI, then add it a second
  time to force the duplicate-project toast).
  A very recent (2026-08-18, one day before this pass, same host) predecessor pass —
  `docs/linux-rewrite/FINISH-domain-part2.md`, lane `wf-dom3` — reported PASSED for both rows with
  detailed, specific, reproducible evidence (a hard disk-content discriminator for F-WIN-03: edited
  a file through the picker, saved, then `cat`'d the real path outside the app and got the edited
  content back; a toast-appears-then-auto-dismisses-while-a-different-persistent-banner-stays
  distinction for F-WIN-10). I read that report and cross-checked the source facts it cites
  (`control_add_project` vs `add_project`, `cx.prompt_for_paths`, `TOAST_DURATION = 4s`) against
  the current tree — they all check out. I am **not** relabeling that as my own PASSED, since I did
  not drive it myself this session (the standard of proof for this pass is "I drove it and observed
  it," not "someone recently did and the code hasn't obviously changed since"). Flagging both rows
  NOT EXERCISED by me, with this context, rather than either re-asserting PASSED on secondhand
  evidence or silently downgrading a very recently and carefully re-verified pair of rows.
- **F-WIN-09**'s "reopen it" half and the underlying "did minimize actually do anything" question
  are genuinely unresolved — see the half-proven row above.

## Keyboard-alive control (this pass)

Before relying on any `chord`/`type` action, I ran the scout's own recipe: clicked the terminal
composer, `type "echo FINWIN_KEYBOARD_ALIVE_7731"`, `key Return`, then read the pane back over the
raw control socket (`panel.read`, base64-decoded) independent of the screenshot. Output:
`KEYBOARD-CHECK: MARKER_FOUND`. Screenshot `04-03-typed-marker.png` shows the same literal text and
its echoed output. This is what makes the later `chord` calls (F-WIN-02/04/05/07) trustworthy
evidence rather than an unverified assumption.
