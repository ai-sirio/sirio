# wf-fix2 — the six hard failures, all found by driving

Lane: `wf-fix2`. Binary pinned at `/tmp/wf-fix2-tiller` (`TILLER_WL_BIN`), rebuilt after every
fix and re-copied before the next live drive. Every row below was reproduced live in this lane
before any code changed, per instructions — a fix for a failure never seen is not attempted.

Evidence standard: per `EVIDENCE-STANDARD.md`, "read the code" is never a verdict on its own.
Every row below pairs a named regression test (quoted red, then green) with a live re-drive using
a hard discriminator — a `panel.list`/`panel.scrollback` read, a pane count, a PID, never "the
screenshot looks right" alone.

I am the builder. **None of these rows is `PASSED`** — see the per-row `half-proven` line and the
named gap a fresh critic must close.

---

## F-TAB-26 — Close Anyway silently no-ops on a tab's sole pane

**Reproduced live first**, confirming the ledger's finding. Lane `wf-fix2`, fresh boot,
`project.add` on this repo. The default worktree opens with a `Chat` tab and a `Terminal` tab,
the Terminal tab holding exactly one pane (`pane-1`). `ctl notify session=pane-1 status=running`
then right-clicking inside the terminal and choosing "Close Terminal…" produced the expected
"This tab has running work. Close anyway?" banner (see `reference/linux-progress/wf-fix2-tab26/
02-confirm-banner-close-anyway.png` — note the banner already says **"tab"**, not "pane", because
of the fix below; before the fix it read "This **pane** has running work"). Before any code
change, clicking **Close Anyway** dismissed the banner but left the Terminal tab and `pane-1`
both fully present in a follow-up `panel.list` — the confirmation UI worked, the actual close did
nothing, exactly as the ledger row describes.

### Root cause

`request_close_terminal_at` (`rust/crates/tiller/src/main.rs`) always built
`PendingPaneClose { whole_tab: false, .. }` regardless of whether the pane being closed was its
tab's only pane. `confirm_pending_pane_close`'s `whole_tab: false` branch calls
`close_terminal_at`, which deliberately refuses to remove a tab's *last* leaf (the F-TAB-13
empty-state guard: `tab.panes.leaf_ids().len() <= 1` returns early). So "Close Anyway" on a
single-pane tab always hit that guard and did nothing, while the identical control on a
multi-pane tab worked, because there the guard never triggers.

### Fix

`request_close_terminal_at` now computes `whole_tab = tab.panes.leaf_ids().len() <= 1` and uses
it both when building `PendingPaneClose` (so `confirm_pending_pane_close`'s existing, already-
correct `close_tab_by_id` branch is taken) and on the no-confirmation-needed immediate path,
which previously had the exact same silent no-op for an idle/done sole pane.

### Regression test

`rust/crates/tiller/src/main.rs`,
`tests::close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping`: builds the existing
single-tab/single-pane `worktree_urgency_test_workspace` fixture, marks the pane `Running`,
calls `request_close_terminal_at(0, 0, cx)`, asserts the confirm banner is drawn, clicks the real
drawn "Close Anyway" control, and asserts `workspace.tabs.len() == 0`.

**Red, on the unfixed code:**
```
thread 'tests::close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping' panicked at
crates/tiller/src/main.rs:13073:9:
assertion `left == right` failed: Close Anyway on a tab's sole pane must remove the tab, not
silently do nothing
  left: 1
 right: 0
```

**Green, after the fix:**
```
test tests::close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping ... ok
```

Fixing `request_close_terminal_at`'s immediate (no-confirmation) branch also changed the
long-standing `drawn_pane_close_prompt_is_requested_for_exactly_the_urgent_statuses` test's
behaviour: its pre-existing "an idle pane closes without a prompt" step now *actually* closes the
fixture's sole tab (previously it silently didn't, which is exactly this bug, and the test never
asserted the tab was gone). That test is restructured — the Running/NeedsInput/Error loop (which
uses Cancel and needs the tab to persist) now runs first, and the single no-confirmation check at
the end (using `Done`) asserts `tabs.len() == 0`. `ActivityStatus::from_agent_status`
(`tiller_activity/src/activity.rs`) documents `Idle` (no status notified) and `Done` as both
mapping to `requires_close_confirmation() == false`, so one no-confirmation case now covers what
two redundant ones did before, without needing a second sole-pane tab mid-test. Full crate suite:
`cargo test --manifest-path rust/Cargo.toml -p tiller` → **183 passed, 0 failed**.

### Re-driven live after the fix

Single `wayland-drive.sh` invocation (see `WAYLAND-LANE.md` harness traps — every action below is
in ONE invocation so no automatic per-invocation baseline `shot` lands mid-gesture):

```
ctl project.add path=<repo>            → worktreeCount 6
ctl panel.list                          → pane-0 Chat, pane-1 Terminal (2 panes)
ctl notify session=pane-1 status=running
click 700 300 ; rightclick 700 300 ; sleep 1.2 ; shot menu-open        → 12-item terminal menu
click 760 669 (Close Terminal…) ; sleep 1.2 ; shot confirm-banner      → "This tab has running work"
click 816 504 (Close Anyway) ; sleep 1.2 ; shot after-close-anyway
ctl panel.list                          → {"active":"true","agent":"","id":"pane-0","tab":"Chat","title":"Chat"}
```

**Hard discriminator**: the final `panel.list` returns exactly **one** pane (`pane-0`/Chat);
`pane-1` is gone entirely — not a screenshot impression, a control-socket read of the live pane
registry before (2 panes) and after (1 pane). Screenshots:
`reference/linux-progress/wf-fix2-tab26/01-terminal-context-menu.png`,
`02-confirm-banner-close-anyway.png` (banner reads "tab", confirming `whole_tab` routing),
`03-after-close-anyway-tab-removed.png` (only the Chat tab remains in the strip).

### Verdict: half-proven (builder-driven, not critic-passed)

Proven: reproduction of the original bug, root cause, regression test red→green, full crate suite
green, and a live re-drive with a `panel.list` hard discriminator showing the pane count drop.

**Gap for a fresh critic**: this drive only exercised the *sole-pane* path
(`Running` → Close Terminal → Close Anyway). A critic should also re-confirm the **multi-pane**
case still works unchanged (right-click Close Terminal on one of several panes in a tab, Close
Anyway, confirm only that pane's `panel.list` row disappears and the tab itself survives) — the
regression test's sibling assertions cover this in-process, but it was not re-driven live in this
pass, and a critic independent of the builder should be the one to promote it.

---
