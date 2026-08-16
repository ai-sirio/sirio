# H2-ctxmenu report

## `F-TAB-11` — context-menu items must render disabled with a reason — **implemented**

Verified the recorded diagnosis still held at HEAD: `TerminalContextItem`
(`tiller_terminal/src/context_menu.rs:30`) had only `label`/`action`/`route`, `items()` took no
parameters, and the render site (`tiller_terminal/src/lib.rs`, then ~1471) iterated with no
disabled branch — every item was always clickable. `split_disabled_reason`
(`tiller/src/panes.rs:218`) was still `#[allow(dead_code)]`, `pub(crate)` to the `tiller` crate, and
referenced only by its own three `#[cfg(test)]` tests.

That function could not simply be imported and wired: `tiller` depends on `tiller_terminal`, not the
reverse, so `tiller_terminal`'s own render path cannot reach anything in `tiller::panes`. On top of
that, `split_disabled_reason`'s own minimums (`MIN_SPLIT_PANE_WIDTH = 240`, `MIN_SPLIT_PANE_HEIGHT =
160`) do not match `main.rs`'s real, live, enforced split minimum (`MIN_SPLIT_PANE_SIZE = 160.0`,
applied uniformly via `.min_w()/.min_h()` on every split child) — the "logic" was never
cross-checked against the layout that actually runs.

Implemented, self-contained inside `tiller_terminal` (no foreign-file edit needed):

- `TerminalContextItem` gained `pub disabled_reason: Option<String>`.
- `context_menu::items_with_split_availability(pane_width, pane_height)` computes a per-direction
  reason from the pane's own live size, mirroring `main.rs`'s real `MIN_SPLIT_PANE_SIZE`/
  `SPLIT_DIVIDER_SIZE` (160/6) rather than `panes.rs`'s untested 240/160 pair. Three new unit tests
  cover narrow, short, and comfortably-large panes.
- The render site (`lib.rs`) now reads `terminal.last_bounds` (the same live-prepainted size
  `on_left_mouse_down` already uses for hit-testing) and calls
  `items_with_split_availability` instead of `items()` when a real terminal is running. A disabled
  item renders dimmed (`theme.meta`), `cursor_not_allowed()`, no `.on_click()` attached, with its
  reason drawn inline as a second line under the label.
- New drawn-render test `a_too_narrow_pane_disables_split_left_with_a_visible_reason` opens a real
  220x900 test window (not a faked bounds value), right-clicks the pane, asserts the disabled
  reason text is drawn, that clicking Split Left at its own drawn position fires nothing, and that
  an unrelated enabled item (Copy Pane ID) in the same still-open menu still works.

**Not wired**: `SplitDisabledReason::SoleTabInGroup`, the other half of `panes.rs`'s function. It
needs the workspace's own tab-group membership count (`tab_machinery.group_tabs(...).len()`), which
is only reachable from `main.rs`'s `render_pane_tree`/`render_group_surfaces` — both `&self` methods
with no `Context<Self>`/`cx` parameter, so there is no way to push that count into the terminal
entity mid-render without threading `cx` through both call chains (a real, larger refactor). Left
unimplemented rather than guessed at.

**Foreign-file change wanted** (not applied — I do not own committing to `main.rs` speculatively;
this is a precise, actionable change, not a hypothesis): to close the `SoleTabInGroup` half, add a
`cx: &mut Context<Self>` parameter to `TillerWorkspace::render_pane_tree` and
`render_group_surfaces` (`tiller/src/main.rs`, called from `Render::render`, which already has
`cx`), and in the `PaneNode::Leaf` / `TabContent::Terminal { view }` arm call
`view.update(cx, |t, cx| { t.set_sole_tab_in_group(...); cx.notify(); })` using
`self.tab_machinery.group_tabs(self.tabs[tab_index].group_id).len() == 1`. `TerminalView` would need
a matching `pub fn set_sole_tab_in_group(&mut self, sole: bool)` setter and a third
`disabled_reason` source folded into `items_with_split_availability` (or a sibling function) in
`context_menu.rs`.

Verified: `cargo build -p tiller_terminal` and `cargo build -p tiller` both green.
`cargo test -p tiller_terminal --lib` — 43 passed, 0 failed (up from 42; the fix added one new
context_menu unit test group of 3 plus the drawn-render test, all listed above).

Commit: `671cd378 fix(F-TAB-11): render terminal context-menu split items disabled with a reason`

**howToExercise**: `cargo test -p tiller_terminal --lib a_too_narrow_pane_disables_split_left_with_a_visible_reason`
runs the drawn-render regression directly. Live: open a pane narrower than ~326pt (e.g. after two
nested horizontal splits in a normal-width window), right-click it — Split Left/Right show dimmed
with a "pane is too narrow to split: Npt available, 160pt required" second line and do not respond
to a click, while Split Above/Down and every other item still work normally.

---

## `F-CHG-18` — drag a changed-file row to a terminal pane ("drag-to-pill") — **already-correct (verified live, no code change needed)**

Read `docs/linux-rewrite/tasks/P129-overlay-click-routing.md` first, as instructed. Its conclusion:
the terminal's *own* right-click context menu had a real, proven anchoring bug (Finding A, the same
class as P123) — but that bug lives at a *different* call site than the one F-CHG-18 actually names.
F-CHG-18's own recorded text (`docs/linux-rewrite/wave-g/G5-terminal-verdicts.md`) is explicit that
the blocker was the **tab-strip's** `TabContextAction` menu (`tiller_ui/src/tab_bar.rs` +
`tiller/src/main.rs`, reached by right-clicking a *tab*), not the terminal pane's own menu. P129
could not reproduce a defect in that specific menu and attributed the wave-G "0/5" result to two
harness artifacts (separate-invocation app restarts; one drive that landed on an unrelated
project/branch despite the sidebar's claim).

`git log` at HEAD shows the terminal pane's own menu fix (commit `6bbdbbbf`, P129's Finding A) had
already landed before this row started — so the one *proven* code defect P129 found is already
fixed. That leaves nothing left to fix in code for F-CHG-18's own named menu, per the task's
instruction to scope work to what's actually broken.

Rather than take P129's live evidence for the tab-strip menu on faith, I re-drove it myself, fresh,
in one `wayland-drive.sh` invocation, at current HEAD (binary rebuilt after the `F-TAB-11` commit
above), controlling for both harness artifacts P129 named:

1. `project.add` + `workspace.select` a real scratch git repo with one untracked file, `surface.
   changes.open` — all in the same invocation as everything below.
2. Right-clicked the **Changes** tab (632,51): the menu painted anchored exactly at the click point
   (screenshot `/tmp/h2chg18-probe5-shots/02-menu.png`), refuting any anchoring-offset defect at
   this site too.
3. Selected the **Terminal** tab, then right-clicked it and clicked **Move to New Pane**
   (499,51 → 510,432): the click fired and visibly mutated app state in the very same capture — the
   tab strip collapsed from 3 tabs to 1, and a real split appeared (Changes list on the left, a
   fresh terminal pane on the right) — direct, first-person counter-evidence to "no click on any
   item did anything, 0/5" (screenshot `/tmp/h2chg18-probe9-shots/02-after-move2.png`).
4. With both the Changes list and a terminal pane now visible together, dragged the
   `SCRATCH_DRAG_TEST.md` row onto the terminal pane (`drag 447 139 1060 400 6`) in the same
   invocation: the terminal's breadcrumb grew a **"Dropped diff: SCRATCH_DRAG_TEST.md"** pill in the
   very next capture (screenshot `/tmp/h2chg18-full-shots/03-after-drag.png`) — the exact
   `terminal-diff-drop` pill G5's report says it "never reached." This closes the loop the wave-G
   pass left open end to end: menu opens correctly, an item click fires, and the drag-to-pill
   mechanism it was gating works.

No source change was made for this row — there was nothing left to fix. The unit-tested
drop-handling code G5 already verified, plus this fresh live drive, together fully prove F-CHG-18's
described behavior at HEAD.

One incidental observation, **not part of F-CHG-18** and not investigated further here: the terminal
pane created by "Move to New Pane" in step 3 showed a PTY breadcrumb for the real `tiller-linux`
checkout (`tiller-linux` / `linux/gpui-waku`) rather than the scratch repo the sidebar had selected
— the same "wrong project/branch" symptom P129 flagged as a harness artifact it hit once. Seeing it
again, on demand, from a clean state suggests it may be a real, reproducible session/worktree-wiring
issue rather than a one-off. Flagging for whoever owns that area; out of scope for a context-menu
row.

**howToExercise**: `TILLER_WL_LABEL=<label> Scripts/wayland-drive.sh <outdir> 'ctl project.add
path=<scratch-repo-with-an-untracked-file> ctl workspace.select workspace=<id> ctl surface.changes.
open click 632 51 rightclick 499 51 click 510 432 shot before drag 447 139 1060 400 6 shot after'
— the last capture's terminal breadcrumb must show a "Dropped diff: <filename>" pill.
