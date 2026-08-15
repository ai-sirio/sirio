# D-MAIN-8 critic verdicts

Critic pass, independent of the builder. HEAD at verify time: `4560076` (post wave-D integration),
binary at `rust/target/debug/tiller` (271 MB, rebuilt this wave, matches `git log -1`). Instruments
used: `cargo test -p tiller_ui --lib` / `cargo test -p tiller --bin tiller` (per-crate, targeted
tests only), `Scripts/wayland-drive.sh` (labels `d8crit*` for F-WIN-10, `f07crit*` for F-WIN-07),
and source inspection of `rust/crates/tiller/src/main.rs`, `rust/crates/tiller_ui/src/titlebar.rs`,
`rust/crates/tiller/src/command_palette.rs`.

Both rows required real live UI gestures, not just socket calls — `add_project`'s toast is wired
only in the UI-event path (`SidebarEvent::AddProject`), not the control socket's separate
`control_add_project`, and the History routes are keyboard/click entry points with no socket
equivalent. Getting reliable synthetic clicks on freshly-opened dynamic overlays required
discovering and working around a real harness gotcha: `wayland-drive.sh`'s `shot` alternates the
compositor's output resolution every call (1400×900 / 1715×972), and a click issued between two
`shot` calls lands at whatever resolution is *actually* active at that instant — which is not
necessarily the resolution of whichever screenshot a coordinate was eyeballed from. Sidebar-column
elements (top-left, fixed-width) are position-invariant across both sizes and forgiving of this;
a form field or a right-anchored titlebar icon is not. Coordinates below were re-derived per
resolution regime after this was diagnosed; noting it here since a future critic hitting "click
lands one row off" on this lane should suspect this first.

## `F-WIN-10` — PASSED

Live (Wayland lane, label `d8crit...n`): tracked `$HOME` as a project via `ctl project.add`
(control-path, state only), then drove the **real UI gesture** the row's own route names — sidebar
`+` → `Create Project…` → typed a name → clicked `Create project` — which creates a folder nested
inside the already-tracked `$HOME`, hitting `add_project`'s `Ok(false)` "already tracked or nested"
branch. The captured frame
(`/tmp/d8shots13/02-after-submit.png`, not retained — described here) shows **two distinct**
elements simultaneously: the persistent inline red sidebar notice at bottom-left
("already tracked or nested: /home/enzopalmisano/d") *and* a separate bordered card floating
bottom-right with the same message — the `#workspace-toast` overlay. A second capture 5+ seconds
later, forced via a fresh repaint, shows the inline sidebar notice **still present** while the
floating card has **disappeared** — the discriminating half: a static screenshot containing a red
message would not distinguish "toast" from "just the existing inline banner", but the auto-dismiss
timing split from the persistent banner is exactly the mechanism under test, and only a real
timer-driven UI could produce it. This is on top of (not instead of) the builder's own drawn GPUI
test (`drawn_add_project_duplicate_shows_sidebar_notice`, extended with a `wait_for_drawn`/clock-
advance/`debug_bounds`-absence assertion), which I re-ran myself and confirms the same auto-dismiss
timing via the real background executor. Source confirms the toast is wired at both `add_project`
error branches (`Ok(false)` and `Err`), tagged by an incrementing `toast_id` so a stale timer from
a superseded toast cannot clear a newer one. Side effect cleaned up: the drive created a real
`/home/enzopalmisano/d` directory (the `CreateForm`'s `create_project` does a real `mkdir` before
the catalog check runs); removed with `rmdir` immediately after, confirmed gone.

## `F-WIN-07` — PASSED

Live (Wayland lane, labels `f07crit...b/c/e`), three independent routes, all reachable and all
converging on the same real handler:

1. **Command palette.** Focused the Chat tab (ctrl-k is intentionally inert with a terminal
   focused — confirmed by a real app test, `ctrl_k_in_a_focused_terminal_does_not_open_the_palette`
   — so this is required, not incidental), pressed `ctrl-k`, and captured the full palette open
   with `History: Restore Previous Launch  Ctrl+Shift+O` listed among the other commands. Typed
   `history` into the (auto-focused) search box and captured the list narrowing to that exact one
   row, discriminating: a stub or unwired entry would not appear in a live substring-filtered
   command list built from the same catalog every other real command lives in.
2. **Direct chord.** From the same focused state, sent `ctrl+shift+o` via `wtype -M ctrl -M shift
   -k o -m shift -m ctrl` (bypassing the `chord` helper, which only accepts one modifier). The
   sidebar immediately showed a genuine, specific error: `[history] could not restore the previous
   launch: unknown worktree: /home/enzopalmisano/Scrivania/Progetti/tiller-linux`. This is strongly
   discriminating: the builder's report states the *old* behavior silently dropped this same `Err`
   in two of the three routes, so a regression back to that state would show *nothing* here, not an
   error. Seeing the real, worktree-specific message proves the keybinding dispatches through to
   `restore_launch_snapshot` and that its `Err` now reaches `sidebar.set_notice`, live.
3. **Titlebar button.** Cropped and visually confirmed the RefreshCw-style circular-arrow icon
   sitting left of the right-panel toggle in the titlebar (matches the claimed `titlebar-history`
   selector's icon and position). Clicked it directly (at the coordinate re-derived for the active
   1715×972 regime, per the resolution-parity note above) and got the **identical** error notice as
   route 2 — proof this is not a decorative icon but the same `on_history` → `restore_launch_snapshot`
   path.

`cargo test -p tiller_ui --lib` confirms the two new titlebar seam tests pass
(`the_history_seam_invokes_its_wired_handler`, `unwired_history_seam_renders_but_does_not_panic_on_
click`); `cargo test -p tiller --bin tiller` is green at 143/143, matching the integrator's count,
no regressions from this row's change to the two existing chord-array tests.

## Notes

Per-crate suites run clean: `tiller_ui --lib` (2/2 targeted + no regressions reported by
integrator at 304/304), `tiller --bin tiller` 143/143. Did not run `--workspace`, per house rules.
No `docs/linux-rewrite/INVENTORY-LEDGER.md` edits made.
