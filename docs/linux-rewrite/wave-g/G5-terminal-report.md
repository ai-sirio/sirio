# Wave G slice G5-terminal — report

## `F-TERM-UI-01` — Set Title — **already-correct, re-verified live**

The prior critic's "stub, no-op" evidence does not reproduce at HEAD. Read `set_terminal_title`
(`rust/crates/tiller/src/main.rs`) and the full delegation chain
(`context_menu.rs::handle_context_action` → `TerminalContextEvent` → `subscribe_terminal` →
`Workspace::set_terminal_title`): it sets `tab.title = "Terminal {terminal_id}"`, calls
`sync_activity`/`schedule_save`/`cx.notify()`. Live-drove it under `wayland-drive.sh`: added the
project, right-clicked a live PTY terminal, clicked "Set Title" in the drawn context menu, and both
the tab-bar label and the sidebar row changed from "Terminal" to "Terminal terminal-1" in the very
next capture (`/tmp/g5out/02-after-click.png` at the time, not committed — reproduce with the
`howToExercise` below). No code change made; nothing to commit for this row.

## `F-TERM-UI-02` — platform-modifier click opens a terminal link — **fixed (test only)**

Root cause confirmed exactly as the critic described: `platform_modifier_click_opens_a_terminal_link`
drives a `TestAppContext` window rooted at `(0, 0)`, so `bounds.origin` is `(0, 0)` and the
origin-subtraction in `on_left_mouse_down` is untested — reverting it left the test green.

Fix: pulled the click→cell arithmetic out of `TerminalView::on_left_mouse_down`
(`tiller_terminal/src/lib.rs`) into a pure `link_router::resolve_click_cell(event_x, event_y,
origin_x, origin_y, cell_width, line_height) -> (row, col)`, and added two unit tests in
`link_router.rs` that call it directly with a **non-zero** origin (`(400, 100)`), which fails without
the subtraction and passes with it. `lib.rs` now calls the shared function instead of inlining the
same math, so production and test exercise the identical code path. `cargo test -p tiller_terminal
link_router --lib` — 4/4 pass, including the two new ones.

Live gesture proof (real modifier-held click on a rendered URL) is still not attempted this pass —
`wayland-drive.sh`'s `modclick` primitive exists but was not exercised here; the row stays
half-proven on the *gesture* half even though the *arithmetic* half now has a real discriminating
test. Commit: `86695b3`.

## `F-TERM-SCR-02` — output-settle / resize debounce — **half-proven, unchanged**

Re-read `rust/crates/tiller_terminal/src/lib.rs` at current HEAD: `OUTPUT_SETTLE_DEBOUNCE` (200ms),
`TERMINAL_RESIZE_DEBOUNCE` (120ms), the `resize_generation` compare-and-swap, and
`pump_terminal_events`'s drain-until-quiet loop are all present and structurally match the prior
critic's description — no code drift found. I did not build a live WINCH-trap-plus-divider-drag
instrument this pass (out of budget for this row); no code change made, no regression found. Leaving
as half-proven rather than re-affirming PASSED on code-reading alone, per the standing instruction
that code-reading isn't live re-verification.

## `F-TERM-PTY-05` — real agent-CLI child process through the launch path — **half-proven, unchanged, environment-gapped**

No files are mapped to this row and none of G5's owned files contain the command-palette/launch
logic the prior critic's evidence describes (`tab-strip button never opened via mouse`, `Ctrl+Shift+P`
path did). The remaining gap is that this host's `codex`/`opencode` have no stored credentials, so a
prompt can never be sent and a streamed reply never observed — that is an environment fact, not a
code defect, and not something a fix inside G5's owned files can close. Not attempted further this
pass; no code change made.

## `F-CHG-18` — drag a changed-file row to a terminal pane ("drag-to-pill") — **half-proven, unchanged**

Reproduced the builder's "Move to New Pane" step live (real two-pane layout, live PTY on the right)
on the first attempt, then dragged the `README.md` changed-file row onto the terminal pane with
`drag`. The drop did not visibly render `tiller_terminal::lib.rs`'s `terminal-diff-drop` pill (the
concrete signal `receive_diff_drop` is supposed to produce — see `lib.rs:839` and the
`.when_some(dropped_path, ...)` render block), and the pane layout itself collapsed back to a flat
tab strip in the same action that should have merely dropped a payload — evidence the drag's mouse-up
did not land on `terminal-drop-target` as intended, or that `Move to New Pane`'s pane state is more
fragile than the single successful prior repro suggested. A second, cleaner attempt (fresh instance,
explicit `Move to New Pane` then re-selecting the Terminal tab) produced **no split at all** the
second time, despite clicking the identical menu-item coordinates — inconsistent, and worth flagging
to whoever owns pane-split code (outside G5): `Move to New Pane`'s outcome is not reliably
reproducible from identical inputs in this lane. The one clue I chased down and ruled out: the "~1"
badge visible in the terminal breadcrumb after the drop is **not** drop-derived context — it renders
identically on a freshly-loaded window with a pending git change and no drag ever performed (it is a
plain modified-file-count indicator), so do not mistake it for drop evidence in a future pass. No
code change made in G5-owned files this pass; the `(PathBuf, String)` drop wiring in
`tiller_terminal/src/lib.rs` (`on_drop::<(PathBuf, String)>` → `receive_diff_drop`) reads correct and
is unit-tested (`a_drawn_change_row_drags_its_diff_payload_to_a_drop_target`,
`tiller_ui/src/changes.rs`), so the defect (if any) is more likely in pane-split/tab-strip code this
slice does not own.

## Foreign files wanted

None identified as a concrete, describable fix this pass — the F-CHG-18 pane-split inconsistency
above is a lead for whichever slice owns the pane/tab-strip split machinery (not enumerated in G5's
file list), but I could not narrow it to a specific call site worth prescribing blind.
