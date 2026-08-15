# Wave E slice E-P2 — report

## `F-CORE-ACT-11` — ledger line 347 — action: **already-correct, live-reproduced**

`tab_bar.rs`'s new-tab `+` button and its dropdown were re-read (`tiller_ui/src/tab_bar.rs`
lines ~605-668): `on_click` calls `cx.stop_propagation()` before `toggle_menu`, the anchor bounds
are captured via a `canvas` child, and the menu is rendered inside `deferred(anchored()...)`
which (unlike the Clone/Create popover in P123) positions relative to the window root, not a
narrow parent — so it is not the same overlay-positioning bug class.

`cargo test -p tiller_activity --lib --tests` (10 + 25 = 35 tests) and the tab_bar drawn-menu
tests (`drawn_new_tab_menu_dispatches_every_item_action`, `..._offers_browser_action`,
`..._escape_dispatches_through_the_real_key_path`) all pass unmodified — the wave-D critic's
own re-read of `model.rs` (three disjoint ownership sets) also held.

Re-ran the exact live gesture the critic could not reproduce, in a fresh, less-loaded
`wayland-drive.sh` instance (`TILLER_WL_LABEL=ep2row1c`): `ctl project.add ...` then a real
synthetic `click` at the `+` button's on-screen position. The dropdown opened correctly on the
first attempt, anchored directly under the button, listing all nine expected items (New
Terminal, Changes, New Browser, Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, Split Claude Code,
New Chat) — screenshot
`/tmp/.../scratchpad/wd1/02-menuopen.png` (not committed; ephemeral scratch capture). This
confirms the critic's own suspicion: the prior non-reproduction was session-load flakiness (30
concurrent instances), not a defect. No code change was needed or made.

**howToExercise:** `wayland-drive.sh <outdir> 'ctl project.add path=<abs> \n click <x> <y-of-plus-button> \n shot open'` —
click the `+` at the right edge of the tab bar (next to the last open tab); the dropdown must
render anchored under it with all nine New-Tab-menu items, not clipped to a wrong parent.

## `F-USE-03` — ledger line 266 — action: **already-correct**

Re-read `tiller_usage/src/model.rs`, `tiller_usage/src/claude.rs`, `tiller_agents/src/claude.rs`,
`tiller_ui/src/status_bar.rs`. `cargo test -p tiller_usage --lib` — 40/40 pass, including
`model::tests::success_replaces_the_previous_state` (the TimedOut -> Stale reducer transition)
and the bounded-timeout PTY test in `claude.rs`. `status_bar.rs`'s
`unavailable_reasons_render_distinct_text` test independently proves `Stale` renders identically
to `Loaded` except for dimming (`segment_dimmed`), which is the exact UI behaviour the row is
about. No regression, no code change needed.

Did not attempt a fresh live end-to-end drive of the stale transition: `Claude::TIMEOUT` is a
real 25s bound and the fetch shells out to the actual `claude` CLI, so reproducing it live needs
a real Claude installation plus >25s of wall time inside a single dispatch — the same budget
constraint the wave-D critic hit. This is not new evidence beyond wave D; the live-UI half stays
owed to a slice with more time, not because anything here is suspect.

**howToExercise:** with a real `claude` CLI reachable, open the status bar and stop network/PTY
access to the usage source mid-poll (or wait out `Claude::TIMEOUT` = 25s after a successful
fetch); the Claude segment must keep showing the last good `NN% 5h` text but visibly dim
(`segment_dimmed` halves the text alpha) rather than switching to "Claude timed out".
