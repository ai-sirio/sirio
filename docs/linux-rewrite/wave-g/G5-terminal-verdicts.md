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
