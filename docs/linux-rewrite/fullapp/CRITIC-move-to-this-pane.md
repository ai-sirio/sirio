# Critic pass — F-TAB-12 ("Move to This Pane" fix)

Fresh critic, no history with this work. Judged commit `2ff664bb` on branch
`f-tab-12-move-to-this-pane-3615046-2490` (worktree `/var/tmp/tt-ftab12-3615046-5470`),
against ledger row `F-TAB-12` (`docs/linux-rewrite/INVENTORY-LEDGER.md:128`) and the Swift
original at `/home/enzopalmisano/Scrivania/Progetti/tiller` (read-only, per instructions).

**Verdict: F-TAB-12 fix is correct — PASSED.** It removes real dead code for the right reason,
is pinned by a test at the decision point, and does not regress any of the 54 tab/move tests or
the 203-test `tiller` suite. But the fix is *incomplete*: the exact same defect survives, word
for word, in a second surface the builder didn't touch — see "Biggest remaining gap" below — and
one previously-PASSED row (F-TAB-13) now cites evidence that no longer exists.

## What I checked, and how

1. **Root-cause claim vs. Swift source.** The commit's rationale is: Swift's actual "This Pane"
   capability (`App/Workspace/SplitContentMenu.swift`'s "Move Existing Tab" → "This Pane" bucket)
   always creates a *new* split for the picked tab, identical in effect to what Rust's
   "Move to New Pane" already does. I traced this myself rather than trusting the commit message:
   - `SplitContentMenuModel.init` (`SplitContentMenu.swift:91-99`) builds `thisPane` as *sibling*
     tabs already in the anchor's group, `otherPanes` as tabs in other groups — a totally
     different feature from a per-tab "move this tab to my own pane" menu item.
   - Both buckets funnel into `WorkspaceCoordinator.requestSplit` (`WorkspaceCoordinator.swift:237-320`),
     which *always* creates a new `PaneGroupID`/`SplitID` (`.splitGroup` if the picked tab is
     already in the anchor group, `.moveTab(... .newSplit(...))` otherwise) — never a no-op, never
     "stays where it is." There is no Swift code path where picking a tab, from either bucket,
     does anything other than give it a brand-new adjacent pane.
   - I independently confirmed Rust's own tab-strip context menu (`tab_context_items()`,
     `main.rs`) is a *different, unified* menu than Swift's `SplitContentMenu` — it merges pieces
     of four separate Swift surfaces (`TabBarView.swift`'s Rename/Close, `SidebarView.swift`'s
     Attach-to-Current-Terminal, `WorkspaceMenuCommands.swift`'s Move Earlier/Later, and
     `SplitContentMenu.swift`'s move-tab family) into one right-click menu on the tab itself. That
     consolidation is a legitimate design choice, not itself a defect, and it is *why* "This Pane"
     never had a coherent meaning in Rust's version: the menu only ever opens on a tab that's
     already in `machinery.active_group()` (`render_open_tabs` at `main.rs:9393-9404` filters the
     visible strip to `tab.group_id == active_group`; the keyboard `OpenTabMenu` handler at
     `main.rs:9908-9910` and the right-click handler `open_tab_menu` at `main.rs:8610`/`8808` both
     agree). "Move this tab to the pane it's already in" was never a reachable, meaningful
     operation through *this specific* menu — the builder's read is correct.
   - **Conclusion: the row was correctly FAILED before the fix, and the fix's removal — not a
     re-enable — is the right call.** Swift's real capability already has a live Rust home under
     "Move to New Pane" (single right-clicked tab, brand-new pane) and "Move to Pane N" (existing
     other group), both independently confirmed working by the prior critic pass and unaffected by
     this diff.

2. **Test coverage at the decision point.** The new test
   `tab_context_menu_never_offers_a_this_pane_move` (`main.rs:12371-12420`) asserts the exact
   thing that was wrong: with a second pre-existing pane group in play, nothing sits between the
   preceding separator and the live "Move to Pane 1" entry. That's the right level — it pins
   `tab_context_items()`'s output, not a rendered pixel. Ran it directly:
   ```
   test tests::tab_context_menu_never_offers_a_this_pane_move ... ok
   ```
   Then the whole tab/move surface (54 tests) and the *entire* `tiller` bin test suite (203
   tests) — both 100% green, in a target dir I built myself
   (`CARGO_TARGET_DIR=/var/tmp/tt-ftab12-target-crit-1594189-3542`, from a fresh
   `git worktree add`). `cargo fmt`/`cargo clippy` on the touched hunks (`main.rs:8900-9000`,
   `main.rs:12370-12420`, `tiller_ui/src/tab_bar.rs:43-52`) are clean; the only fmt/clippy debt in
   either file is pre-existing and nowhere near this diff (confirmed both before *and* after this
   commit via `git show <parent>:<path>`).

3. **Live drive, not just source-reading.** Compiled the fixed commit to a binary
   (`/var/tmp/tt-ftab12-target-crit-1594189-3542/debug/tiller`), launched it under a nested
   headless Wayland compositor I booted myself (`Scripts/wayland-drive.sh`, isolated
   `XDG_DATA_HOME` — see caveat below), added a throwaway git-fixture project, opened 3 terminal
   tabs, and right-clicked one for real:
   - The rendered menu is: Open File, Rename · Close, Close Others, Close Tabs to the Right ·
     Move Earlier (disabled, "already the first tab"), Move Later · Attach to Current Terminal ·
     **Move to New Pane** (enabled) · Resume Chat (disabled, "no retained chat is available").
     **"Move to This Pane" is not there.** Confirmed a second time after creating a 4th tab and
     moving one out via "Move to New Pane" (which itself worked — the tab count in the strip
     visibly dropped 3→2, matching the already-passing "Move to New Pane" claim).
   - Cross-checked with `strings` on the compiled binary: `"Move to This Pane"` does not occur
     anywhere in it; `"Move to New Pane"` does.
   - I did **not** manage to reproduce a genuine second `tab_machinery` pane group (as opposed to
     a terminal-internal recursive split, a different feature) inside my remaining budget to also
     click-confirm "Move to Pane N" live — I didn't chase this further because it's unit-tested
     (`drawn_tab_context_menu_moves_a_tab_to_another_pane_group`, passing, untouched by this diff)
     and was already independently confirmed live by the prior critic pass. Flagging this as
     NOT-RE-EXERCISED-BY-ME rather than silently reusing someone else's evidence.

   **Environment caveat, unrelated to the app's own correctness:** `Scripts/wayland-drive.sh`
   sets `TILLER_DB` (an env var the app never reads — `grep -rn TILLER_DB crates/tiller/src`
   returns nothing) but not `XDG_DATA_HOME`, so an unmodified invocation of that script shares the
   real `$HOME`'s persistence file across every concurrent agent on this machine. My first drive
   attempt surfaced a project list full of other agents' worktree names before I noticed and
   re-launched with an explicit isolated `XDG_DATA_HOME`. I removed the stray project I'd added
   to the shared store via the app's own "Remove Project" before moving to the isolated instance.
   This is a test-harness gap (worth a `Scripts/wayland-drive.sh` fix), not something I'm holding
   against this ledger row.

## Biggest remaining gap

**The exact defect this commit fixed still lives, verbatim, in the Command Palette —
`crates/tiller/src/command_palette.rs:365-378` and its handler
`main.rs:9956-9963`/`main.rs:10287-10289` — and nothing here touched it.**

`TabCommand::MoveTabToCurrentPane` builds a palette entry literally labeled "Move Tab to This
Pane", gated on `context.has_other_pane` (`main.rs:10016`: `self.tab_machinery.groups().len() >
1`) — the *same* "does another pane exist" condition the just-deleted context-menu item used, and
the *same* underlying primitive: `move_selected_tab(MoveTarget::CurrentPane, cx)` →
`TabMachinery::move_tab(tab_id, MoveTarget::CurrentPane)`, which resolves
`target_group = self.active_group` (`tab_machinery.rs:279`). The tab this command operates on
(`self.tab_menu_tab.or_else(|| self.tabs.get(self.active_tab)...)`, `main.rs:6484-6490`) is
*always* already in the active group when invoked from the palette (there is only ever one
globally-focused tab, and it belongs to `active_group()` by construction) — so, precisely as the
commit's own rationale argues for the context-menu case, gating this on "another pane exists" is
incoherent: whether another pane exists has no bearing on what this command does, because it
never touches another pane. Confirmed live: with one pane group, the palette shows

> Move Tab to This Pane            (No other pane is available)

in the exact greyed-out state the commit just removed from the tab-strip menu. Unlike that menu,
this primitive *does* have an observable effect when enabled — `move_tab` still runs, and because
the source and target group are identical it pushes the tab to the end of its own group's list
and re-activates it (see the pre-existing test
`moving_an_existing_tab_to_this_or_another_group_preserves_identity` at
`tab_machinery.rs:367-385`, unchanged by this diff) — i.e. "bring to last position in the current
strip." That's a real, if oddly-named and oddly-gated, capability, and there is **no test anywhere**
(checked `command_palette.rs`'s 4-test module — none touch `MoveTabToCurrentPane`/
`MoveTabToOtherPane` at all) exercising either its enablement logic or what it actually does when
enabled. A builder should either (a) delete "Move Tab to This Pane" and its `TabCommand` variant
the same way this commit deleted the context-menu twin — the cleanest choice if reorder-to-end
isn't wanted as a standalone feature — or (b) keep it, rename it to describe what it does
("Move Tab to End of Pane" or similar), gate it on something that actually predicts a visible
effect (e.g. "more than one tab in the active group, and this tab isn't already last"), and add a
test pinning both the label and the gate at `command_palette.rs`'s level, the same way this
commit's own test pins `tab_context_items()`.

## Collateral: F-TAB-13's cited evidence is now stale

F-TAB-13 (ledger line 129, currently **PASSED**) cites: *"with zero eligible tabs, Move to This
Pane shows explanatory disabled text 'no other tab is available'"* — directly the item this
commit deletes. Its own VERIFY clause (`01-inventory-app.md:73`, same SRC citation as F-TAB-12:
`SplitContentMenu.swift:292`) is: *"Open Move Existing Tab with no other tab or no eligible tab,
and confirm the menu explains that no tabs are available."*

I checked whether that's still reachable post-fix. It is not, in any state: every remaining
`TabContextItem::disabled(...)` call site in `tab_context_items()` (`main.rs`) belongs to "Move
Earlier"/"Move Later" (position-based), "Attach to Current Terminal", or "Resume Chat" — the
"Move to New Pane"/"Move to Pane N" branch (`main.rs:8925-8938`, both arms) is built exclusively
with `TabContextItem::enabled(...)`, unconditionally. Confirmed live with the simplest possible
state (a single tab in a single pane group, right-clicked): the menu shows "Move to New Pane"
cleanly enabled, with no disabled sibling and no "no tabs available" message anywhere near it —
this is, in fact, unambiguously *correct* behavior (moving your only tab into a fresh pane is
always meaningful, unlike Swift's needlessly-disabled degenerate case), but it means F-TAB-13's
specific cited evidence for its "Move Existing Tab" half no longer exists to be re-observed, and
its VERIFY clause can no longer be satisfied via this feature in any app state I could construct
or reason through the source. F-TAB-13 also credits itself partly via "Resume Chat"'s
still-correct, still-gated empty state ("no retained chat is available"), which *is* unaffected by
this diff and genuinely dynamic — so F-TAB-13 may still deserve a PASS on that half alone, but its
Move-tab half needs a fresh drive, not the pre-fix screenshot it currently cites. I am not
re-judging F-TAB-13 myself (out of scope for this pass); flagging it precisely so the orchestrator
can decide whether to re-open it.

## Regressions checked and cleared

- `tiller` crate: 203/203 tests pass on my own build of this exact commit.
- All 54 tab/move-prefixed tests pass, including
  `drawn_tab_context_menu_moves_a_tab_to_another_pane_group` and
  `drawn_tab_context_menu_move_records_the_terminal_in_the_pane_cache` (both exercise "Move to
  Pane N"/"Move to New Pane" end to end at the drawn-workspace level, unaffected by this diff).
- `cargo fmt --check` / `cargo clippy` clean on every line this commit touched; pre-existing debt
  elsewhere in both files is unchanged by it (diffed the parent commit's copies directly).
- Live: right-clicking a real tab after the fix shows the correct, reduced menu; "Move to New
  Pane" still moves a tab (tab count in the strip visibly dropped after clicking it).
- `Scripts/stale-failed-census.py` cites the now-removed `"Move to This Pane"` string as its
  F-TAB-13 "built" needle (`Scripts/stale-failed-census.py:134`). That script is an explicitly
  point-in-time historical audit tool (its own docstring: "INPUT IS THE PIN, NOT THE LIVE LEDGER
  ... a moving input cannot be re-audited") and is not wired into `Scripts/ci.sh` or any other
  gate, so this does not block anything — but its citation control (#4, "every `built` needle must
  be found in its cited file under `--tree`") would now fail loudly if anyone re-ran it against
  the live tree instead of the pinned snapshot it's documented to use. Noting it so it isn't a
  surprise later; not treating it as a regression in its own right.

## Files touched by this judgment

- Read only: `/home/enzopalmisano/Scrivania/Progetti/tiller/App/Workspace/SplitContentMenu.swift`,
  `App/Workspace/WorkspaceCoordinator.swift`, `App/TabBarView.swift`, `App/SidebarView.swift`,
  `App/Workspace/WorkspaceMenuCommands.swift` (Swift original — not modified).
- Read only in the judged worktree: `rust/crates/tiller/src/main.rs`,
  `rust/crates/tiller/src/tab_machinery.rs`, `rust/crates/tiller/src/command_palette.rs`,
  `rust/crates/tiller_ui/src/tab_bar.rs`.
- Built and tested in an isolated target dir under `/var/tmp` (not committed, not shared with any
  other agent).
- This report: `docs/linux-rewrite/fullapp/CRITIC-move-to-this-pane.md`, committed by explicit
  path in the main repository.
