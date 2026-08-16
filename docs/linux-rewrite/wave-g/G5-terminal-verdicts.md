# Wave G slice G5-terminal — critic verdicts

Independent critic pass, 2026-08-16. Never the builder of this slice. Findings below are my own
live drives and code checks this pass, not a re-statement of the builder's report.

## `F-TERM-UI-01` — Set Title — **PASSED**

Live-drove it myself under `wayland-drive.sh` (label `g5crit02`), single invocation so no app
restart intervened between gesture and capture: `project.add`, right-clicked a live PTY terminal
at (700,300), clicked "Set Title" at (1069,474) in the drawn menu, then `shot` in the *same*
invocation. Before: tab bar and sidebar both read plain "Terminal". After: both read
"Terminal terminal-1" in the very next capture — a value the app never reaches on its own,
confirmed changed in both places `subscribe_terminal`/`set_terminal_title` are supposed to touch
(`tab.title`, read by both the tab-bar and sidebar renderers). This directly contradicts the
ledger's current `FAILED — defective` (from an earlier sweep that read `set_terminal_title` as a
stub) — reading the current source at HEAD shows it is not a stub (`main.rs:3535-3554` sets
`tab.title = format!("Terminal {terminal_id}")`, `sync_activity`, `schedule_save`, `cx.notify()`),
and this live capture confirms the effect renders. My first attempt (separate invocation for the
click, `g5crit01`) produced a false negative: the screenshot froze mid-click with the menu still
open and highlighted, and a later *fresh app instance* naturally showed the old title back — not a
defect, just cross-invocation state never being live in the first place. Discard that run; the
same-invocation repro above is the real result.

## `F-TERM-UI-02` — platform-modifier click opens a terminal link — **half-proven**

**Arithmetic half — now solidly proven.** `cargo test -p tiller_terminal link_router --lib` — 4/4
pass. I independently mutated `resolve_click_cell` (temporarily reverted the origin subtraction on
both axes) and reran: `click_cell_undoes_a_non_zero_pane_origin` failed (`(6, 50)` vs expected
`(0, 0)`), confirming the test genuinely discriminates, not just that it exists. Reverted the
mutation; `git status --porcelain` on `link_router.rs` is clean afterward.

**Gesture half — still not proven, and still inconclusive rather than failing.** I drove 4 separate
live `modclick logo <x> <y>` attempts (confirmed `platform` on this GPUI/Wayland build maps to the
XKB Logo/Super modifier, per `gpui_linux/src/linux/platform.rs::modifiers_from_xkb`, so `logo` is
the correct wtype modifier name) against a real rendered `https://example.com` line, at several
x/row positions spanning the URL text. None produced the discriminating marker (a new Browser tab
via `WorkspaceAction::OpenBrowserLink` → `add_browser_tab`) in the capture taken in the same
invocation. One later capture in the same run showed the app window unexpectedly shrunk/letterboxed
for no attributable reason — collateral compositor oddity, not evidence either way, and not chased
further given the standing "a null result from modclick is inconclusive" rule. Leaving at
half-proven: the test's discriminating power is now independently confirmed (stronger than the
builder's own claim), but the real user gesture remains unproven in either direction.

## `F-CHG-18` — drag a changed-file row to a terminal pane ("drag-to-pill") — **half-proven**

Read the owned code first: `receive_diff_drop` (`tiller_terminal/src/lib.rs:839`), the
`.on_drop::<(PathBuf, String)>` wiring and the `terminal-diff-drop` pill render block, and the
`(PathBuf, String)` `.on_drag` source in `tiller_ui/src/changes.rs` — all read correct and are
unit-tested (`a_drawn_change_row_drags_its_diff_payload_to_a_drop_target`). No defect found in
G5's own drop-receiving code.

Live: created a real uncommitted file (`CRITIC_SCRATCH_DRAG_TEST.md`, deleted after, tree clean —
`git status --porcelain` confirms) so "Local changes (1)" had a real row to drag. `surface.changes.
open` correctly opened a live Changes tab showing it (`Local changes (1) > Untracked (1) >
CRITIC_SCRA...`), so the Changes-side rendering is real and live.

**The prerequisite step is where this pass differs sharply from the builder's report.** Across 5
independent live attempts — 2x "Move to New Pane", 1x "Rename", 1x "Close", 1x an x-offset probe —
right-clicking the Terminal tab and clicking an item in the drawn tab-context-menu (`main.rs`'s
`TabContextAction` menu, not the terminal-pane's own context menu) never closed the menu or
performed the action, 0/5, even though a subsequent click *outside* the menu correctly dismissed it
every time (proving synthetic clicks are reaching the app in general). This is a stronger, more
specific negative signal than the builder's "second attempt showed the split itself was not
reliably reproduced" — I got zero splits, not an intermittent one. For comparison, the identical
rightclick→click→shot technique worked cleanly for `F-TERM-UI-01`'s Set Title in the terminal
pane's *own* context menu, so this isn't a generic tool/timing problem — something specific to the
tab-strip's `TabContextItem` menu is not delivering clicks to its rows, a pattern matching the
already-documented P123 mis-positioned-popover class of bug. Never reached the drag step or the
`terminal-diff-drop` pill this pass. Leaving at half-proven (owned code reads correct and is
tested; live end-to-end proof still owed) but flagging this specific, reproducible tab-menu
click-through failure as a harder blocker than previously recorded, for whoever owns
`main.rs`'s tab-strip context-menu dispatch.

## `F-TERM-SCR-02` — output-settle / resize debounce — **half-proven**

Independently re-read `tiller_terminal/src/lib.rs` at current HEAD myself (not trusting the
report's line numbers): `TERMINAL_RESIZE_DEBOUNCE = 120ms` (:522), `OUTPUT_SETTLE_DEBOUNCE =
200ms` (:635), the `resize_generation` fetch-add/load CAS pair (:418-423), all present and
structurally unchanged — no regression. Given the tight per-row time budget this pass, I did not
build the live WINCH-trap-plus-divider-drag instrument either (the same gap the builder and the
prior critic both left open) — code-reading alone is not a live re-verification per the standing
rule, so this stays half-proven, unchanged, rather than re-affirming PASSED on a third round of
reading the same source.

## `F-TERM-PTY-05` — real agent-CLI child process through the launch path — **half-proven**

Correction to the report: `main.rs` (a G5-owned file) does contain the launch path this row needs
— `open_command_palette`/`render_command_palette` (:8117-8455) and `NewTabAction::Codex` (:2471) —
so this is not actually a "no files map to this row" gap, contrary to the report's claim. Did not
re-run the live command-palette→Codex-child drive this pass (the existing ledger evidence is
same-day, 2026-08-16, and no code in this path changed this wave per the integration report, so a
repeat run would reconfirm rather than add information, and the row's real blocker is
environment-external). Re-confirmed directly: `codex login status` still reports "Not logged in" on
this host, so the credential gap the prior evidence hit is still real, not a stale claim. Leaving at
half-proven, unchanged verdict, environment-gapped as before.
