# H2-ctxmenu verdicts

Critic pass, independent of the H2-ctxmenu builder. Read `docs/linux-rewrite/tasks/P129-overlay-click-routing.md`
first as instructed: it found and fixed (commit `6bbdbbbf`, landed before H2 started) a real overlay
double-offset bug in the terminal pane's own right-click menu, and separately could not reproduce a
defect in the tab-strip's `TabContextAction` menu (the one F-CHG-18 actually names), attributing wave-G's
0/5 to two harness artifacts. Both conclusions were re-derived independently below rather than trusted.

## `F-TAB-11` — PASSED

Code: `TerminalContextItem::disabled_reason` exists, `items_with_split_availability(width, height,
sole_tab_in_group)` folds in both the pane-too-narrow reason and (after the integration's applied
foreign-file fix, commit `68978b09`) the `SoleTabInGroup` reason, with the latter taking priority.
The render site draws a dimmed row with a second reason line and no `.on_click()` for disabled items.

Tests, per-crate: `cargo test -p tiller_terminal --lib` — 44 passed (the one pre-existing flaky test
named in house rules failed once on the full run, then passed alone — matches the documented flake,
not a regression). `cargo test -p tiller a_solo_tab_is_pushed_as_the_sole_tab_in_its_group` and
`two_tabs_sharing_a_group_are_not_pushed_as_sole` both green, proving the `main.rs` wiring the H2
report itself declined to guess at is real and tested.

Live, single `wayland-drive.sh` invocation (`TILLER_WL_LABEL=h2crit2`): added a scratch repo,
selected it, ran `pane.split direction=right` three times to produce a pane measured at 77pt wide,
right-clicked it. Capture shows the menu anchored exactly at the click point (P129's fix holding),
with Split Left and Split Right dimmed and reading "pane is too narrow to split: 77pt available,
160pt required" as a visible second line, while Split Above/Down and the rest render normally —
this is a live value the app would never reach on its own (a 220px+ pane is the default). Clicking
directly on the dimmed Split Left row's own painted position produced no new pane in the next capture
(still 4 columns, same widths) — the disabled item does not fire, discriminating against "items
render dimmed but still work."

## `F-CHG-18` — PASSED

P129's finding that the terminal pane's own menu had the real anchoring bug, and that this specific
row names the tab-strip menu instead, both check out by reading the git history (`6bbdbbbf` predates
H2) and by independently reading `render_tab_context_menu`'s local (not window-absolute) coordinate
math in `tab_bar.rs`/`main.rs`.

Re-drove live rather than trusting H2's own re-drive. First attempt, no settle between `rightclick`
and the following `click` on "Move to New Pane" (same-invocation, `h2crit6`): the click landed but
did not fire — capture shows the menu still open with the item merely hover-highlighted. This
reproduces the literal "click does nothing" signature and initially looked like a live regression
of F-CHG-18. Retried (`h2crit7`) with one `shot` (which forces a real repaint + ~1.4s settle) inserted
between the `rightclick` and the `click`, same coordinates, same invocation: this time the click fired
correctly — tab count collapsed from 3 to 1 visible plus a real split (Changes list left, fresh
Terminal pane right) appeared in the very next capture. Concluded the first failure is a synthetic-only
race (two IPC pointer commands issued faster than any human can move a mouse, arriving before the
compositor had delivered a frame for the newly-opened menu to hit-test against), not a product defect —
consistent with P129's own documented need to let paint settle between gestures.

Completed the full row end-to-end in one invocation (`h2crit8`): `project.add` → `workspace.select` →
`surface.changes.open` → click the Changes tab → right-click the Terminal tab → (settle) → click
"Move to New Pane" (fires, split created) → `drag 447 139 1060 400 6` (the changed-file row onto the
new terminal pane) → the very next capture shows the terminal's breadcrumb grew a "Dropped diff:
SCRATCH_DRAG_TEST.md" pill. This is the exact marker the row's VERIFY line names, produced by a real
XDND-style in-app drag the harness could not have shown without the feature working.

Not part of either row, flagged for whoever owns worktree/session wiring: every scratch-repo pane I
opened (crit1 through crit8) showed a PTY breadcrumb for the real `tiller-linux` / `linux/gpui-waku`
checkout instead of the scratch repo the sidebar had selected, reproduced on demand across 4
independent app instances. Both P129 and the H2 builder flagged this once as a possible harness/session
artifact; seeing it 100% of the time here suggests it is a real, reproducible defect, not a one-off —
worth its own row, but out of scope for a context-menu critic pass.
