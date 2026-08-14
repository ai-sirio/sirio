# B1-tabbar-zorder — critic verdicts

Slice: `docs/linux-rewrite/wave-b/B1-tabbar-zorder.md`
Builder report: `docs/linux-rewrite/wave-b/B1-tabbar-zorder-report.md` (commit `2c1ee10`)
Critic instrument: `Scripts/linux-drive.sh` on `DISPLAY=:1` (drive lock), `rclick`/`click` — a
right-click primitive was required for 7 of 8 rows and the Wayland lane cannot deliver one.
Fresh `TILLER_DB=/tmp/verify-B1-tabbar.sqlite` / `TILLER_SOCKET=/tmp/verify-B1-tabbar.sock`,
project `tiller` (worktree `linux/gpui-waku`, this same repo) added via `project.add` for a
live, real tab strip. 33 numbered captures + two montages under
`reference/linux-progress/verify-B1-tabbar-zorder/`.

Frames read: 33 (`00-probe.png` … `33-montage.png`, plus a 21-frame pixel-boundary scan
folded into two montages).

## Summary

All 8 rows were driven live with a real right-click/left-click gesture and a real screenshot,
not a socket call and not a test run. 7 of 8 are cleanly **PASSED** — every popover this slice
touched (overflow "All Tabs" list, tab context menu, and the bulk/single dirty-close confirm
dialogs it triggers) now visibly paints over `#centre-surface`, exactly reversing the original
defect ("state toggled, nothing appeared"). The 8th, F-TAB-13, is **half-proven**: the menu's
z-order fix is confirmed live, but tracing `tab_machinery.rs` and every tab-creation call site
in `main.rs` shows `add_group` is `#[cfg(test)]`-only and no production code path ever creates
a second pane group — "Move to Pane" can never be reached by any UI, keyboard, or control-socket
action in the shipped binary, contrary to what the build report's citation of "Split Right"
implies.

## Rows

### F-TAB-02 — overflow chevron / All Tabs list — PASSED
Opened 8 tabs (7 terminals + Chat) at full window width; the strip correctly showed only what
fit plus a chevron (`04-overflow-open.png` is the setup, `03-many-tabs.png`/`04-...` show the
strip). Clicked the chevron: the "All Tabs" panel now visibly renders **over** the terminal
content below it (previously invisible on click despite the state toggling), listing all 8 tabs
with a check mark on the active (last) one — `reference/linux-progress/verify-B1-tabbar-zorder/04-overflow-open.png`.

Then drove the false-positive half precisely: with a fixed set of 4 equal-width terminal tabs,
scanned window width in 5px steps across the crossover (`32-montage.png` at 10px steps 1350‑1650,
`33-montage.png` at 5px steps 1505‑1548). At `w=1515` all 4 tabs render with the "+" button
flush against the 4th tab — zero reserved gap; at `w=1510` it cleanly flips to 3 tabs + chevron.
No width in the scan showed a "chevron already present but a 4th tab would still have fit"
state, which is exactly what the old chevron-width-pre-reservation bug would have produced.
Cause 2 is confirmed fixed by direct pixel measurement, not just the new unit test.

### F-TAB-12 — Move Earlier / Move Later — PASSED
Right-clicked a middle tab (`11a-menu.png`): menu paints over centre-surface, Move Earlier/Move
Later both enabled. Clicked Move Later: tab visibly shifted one slot right in both the tab strip
and the sidebar tree (`11-move-later.png`). Right-clicked it again, clicked Move Earlier: shifted
back (`12-move-earlier.png`). Right-clicked the first tab (Chat): Move Earlier shown disabled
with "already the first tab", Move Later enabled (`13-first-tab-menu.png`).

### F-TAB-13 — Move to Pane — half-proven
The z-order fix itself is proven live: right-clicking a tab now shows the menu drawn over
centre-surface with the disabled reasoning correctly rendered — "Move to This Pane: no other tab
is available" / "Move to Other Pane: no other pane is available" (`17-move-to-pane-menu.png`,
`21-attach-self-disabled.png`). That specific defect the ledger named ("the required
no-eligible-tab explanation was absent") is fixed.

But the row's headline capability — actually moving a tab to a second pane — is **structurally
unreachable**, not merely undriven. I performed a live "Split Right" from the terminal's own
context menu (`15-split-right.png`, `16-split-clean.png`: a real second terminal view appeared
side by side) and then re-right-clicked a tab: "Move to Other Pane" still read "no other pane is
available" (`17-move-to-pane-menu.png`, taken *after* the split). Tracing the code explains why:
`rust/crates/tiller/src/tab_machinery.rs:150` marks `add_group` `#[cfg(test)]`, and it is never
called anywhere outside that one unit test (`grep -rn "add_group(" rust/crates/` returns only the
definition and its own test at lines 435‑436). Every live tab-creation site in `main.rs` (lines
4188, 4241, 4304, 4380, 4408, 4442) hard-codes `group_id: self.tab_machinery.active_group()` —
new tabs always join the *current* group. "Split Right" splits the terminal's own `PaneNode`
tree (a different mechanism, `TerminalContextCommand::Split`), not a `TabGroup`. So no UI action,
keyboard shortcut, or control-socket method in the shipped binary can ever produce a second
`TabGroup`, and the enabled "Move to Pane `<id>`" branch is dead code in production — only
reachable by a test forging `tab_machinery` directly (`drawn_tab_context_menu_moves_a_tab_to_another_pane_group`,
which does exactly that at `main.rs:9372`). The build report treats "Split Right...already
worked" as covering this row's second half; it does not — it is an unrelated surface.

### F-TAB-14 — Rename — PASSED
Right-click → Rename item visible in the on-top menu; clicking it turned the tab into an inline
editable field pre-filled with "Terminal". Typed a new name and pressed Enter; the tab title
updated to "bcdef" in both the strip and the sidebar (`10-rename5.png`). Note: `xdotool`
delivered capital letters as lowercase and dropped some characters on this harness (three
attempts, `06`–`10`, before landing full round-trip evidence with single slow `key` presses) —
an input-fidelity artifact of this X11 instrument, not an app defect; the mechanism itself
(field opens pre-filled, accepts typed text, commits on Enter, updates both the strip and the
sidebar) is proven. Independently confirmed the builder's own flag: `grep`ing `main.rs` and
`tiller_ui::tab_bar` found no double-click-to-rename handler anywhere — rename is reachable only
through this menu item, matching the build report's note verbatim.

### F-TAB-15 — Close — PASSED
Right-click → Close item visible in the on-top menu; clicking it raised "Close dirty tab?
Discard unsaved work in Terminal?" (`22-close-single.png`) — itself a correctly-layered modal.
Confirmed Close; the sidebar tab count dropped by exactly one (`23-close-confirmed.png`).

### F-TAB-17 — Close Others / Close Tabs to the Right — PASSED
On a 5-tab strip, right-clicked tab 2 and clicked "Close Tabs to the Right": a bulk "Close dirty
tabs? Discard unsaved work in bcdef, Terminal, Ter…" dialog listed all three tabs to the right
(`25-close-right-clicked.png`); confirming left exactly 2 tabs (`26-close-right-done.png`). Then
added 2 more tabs, right-clicked the last of 4, clicked "Close Others": bulk dirty-confirm dialog
(`28-close-others-confirm.png`), confirmed, exactly 1 tab remained (`29-close-others-done.png`).

### F-TAB-21 — tab-strip right-click reachability — PASSED
Right-clicking directly on tabs in the strip reliably opened the (now on-top) menu across roughly
ten separate `rclick` actions in this session — never once dead. Separately right-clicked inside
the terminal body: its own, different context menu opened (Copy/Paste/Split Left/Right/Above/
Down/Clear Terminal/Close Terminal, `14-terminal-body-menu.png`), confirming the two paths are
independent and neither occludes the other.

### F-TAB-25 — Attach to Current Terminal — PASSED
Right-clicked a non-current terminal tab: "Attach to Current Terminal" enabled
(`19-attach-menu.png`). Clicked it: that tab disappeared from the sidebar list (8→7 tabs) and its
terminal pane visibly joined the active tab's content as a third split pane
(`20-attach-done.png`). Right-clicked the current/active terminal tab itself: "Attach to Current
Terminal" shown disabled with reason "select another terminal tab" (`21-attach-self-disabled.png`),
matching the row's exercise text exactly.

## Environment notes for the ledger writer
- Right-click was required for 7/8 rows; drove all of them on `DISPLAY=:1` via
  `Scripts/linux-drive.sh`'s `rclick`, holding the drive lock for the whole session (no
  contention encountered).
- All captures and intermediate crops live under
  `reference/linux-progress/verify-B1-tabbar-zorder/` in this worktree, including the two
  boundary-scan montages (`32-montage.png`, `33-montage.png`) that back the F-TAB-02 pixel
  claim.
