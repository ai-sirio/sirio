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

## F-TAB-14 — double-click-to-rename (built) + context-menu "silent fail" (harness artifact, not an app bug)

The row bundles two distinct claims. They were investigated, and treated, separately.

### Part 1: double-click-to-rename was genuinely unwired — reproduced, then built

**Reproduced live first**: double-clicking a tab (two real, separate `click` gestures at the same
coordinates) only ever re-selected it; no rename field ever appeared. Reading `tab_bar.rs` and the
tab-render code in `main.rs` confirmed the ledger's finding directly — `grep -n click_count` across
both files returns matches only in `titlebar.rs`, `file_view.rs`, and `right_panel.rs`; the tab
row's own click handler was a single-branch `.on_click(move |_, _, cx| ... select_tab(id, cx))`
with no click-count branch anywhere.

**Root cause**: no code path checked `click_count` on the tab row at all — not a race, not a
guard bug, an absent feature exactly as the row says.

**Fix**: `rust/crates/tiller/src/main.rs`'s tab render function now branches inside the same
`on_click` handler on `ClickEvent::Mouse(mouse) => mouse.up.click_count` (imported `ClickEvent`
from `gpui`) — `click_count >= 2` calls the existing `begin_tab_rename`, otherwise `select_tab`,
matching the click-count convention `right_panel.rs`'s file rows and `titlebar.rs`'s drag area
already use elsewhere in this codebase. Deliberately kept as a single `on_click` listener (not an
added sibling `on_mouse_down`) because GPUI hitboxes nest — the close button's own hitbox sits
inside the tab row's hitbox, so a second `on_mouse_down` on the parent would also fire (on the
Down phase, before the close button's own `on_click`+`stop_propagation` ever runs at Up) every
time the close "×" is clicked. Routing through the one existing `on_click` avoids that risk
entirely: a click resolved on the close button's own hitbox never reaches the tab's click
listener, exactly as it doesn't today.

**Regression test**: `rust/crates/tiller/src/main.rs`,
`tests::double_click_on_a_tab_opens_its_rename_field` — builds a 2-tab fixture, single-clicks tab 1
and asserts only `select_tab` happened (`active_tab == 1`, no `tab-rename-field`), then dispatches
a real `MouseDownEvent`/`MouseUpEvent` pair with `click_count: 2` at tab 0 (the same pattern
`titlebar.rs`'s double-click tests use) and asserts `tab-rename-field` is drawn.

**Red, on the unfixed code** (verified by reverting just the `on_click` change and re-running):
```
thread 'tests::double_click_on_a_tab_opens_its_rename_field' panicked at
crates/tiller/src/main.rs:13551:9:
a real double-click (click_count == 2) must open the tab's rename field
```
**Green, after the fix:**
```
test tests::double_click_on_a_tab_opens_its_rename_field ... ok
```
Full crate suite after restoring the fix: `cargo test --manifest-path rust/Cargo.toml -p tiller` →
**184 passed, 0 failed** (183 + this one new test; no existing test drives the close button
directly, so the nesting risk above was reasoned through, not test-caught, and is exactly what the
`ClickEvent`-based design was chosen to make unnecessary to test).

**Re-driven live**, one `wayland-drive.sh` invocation: `project.add`, then two real `click`s at
the same coordinates on the Terminal tab (`click 505 51` twice — GPUI's own
`DOUBLE_CLICK_INTERVAL` is 400ms and `DOUBLE_CLICK_DISTANCE` 5px,
`rust/vendor/gpui_linux/src/linux/platform.rs`, so two same-position synthetic clicks issued
back-to-back genuinely register as `click_count == 2`, not a simulated event). The rename field
opened (`reference/linux-progress/wf-fix2-tab14/01-doubleclick-opens-rename-field.png`), typed
`RENAMEDTAB`, `Return` committed it — both the tab strip and the sidebar now read
`TerminalRENAMEDTAB` (`02-doubleclick-rename-committed.png`).

**Hard discriminator, two of them**: (1) `panel.list` over the control socket after commit returns
`{"tab":"TerminalRENAMEDTAB","title":"TerminalRENAMEDTAB", ...}` for `pane-1` — a live read of the
real `OpenTab.title`, not a screenshot impression; (2) `panel.scrollback id=pane-1 max_bytes=8000`
decoded to 7100 real bytes of PTY buffer, and `"RENAMEDTAB" in raw` is **`False`** — the typed
characters never reached the shell, which is the exact inverse of the original bug's leaked-text
symptom (full raw scrollback saved at
`reference/linux-progress/wf-fix2-tab14/06-scrollback-no-leak.json`).

### Part 2: context-menu "Rename click silently fails while the menu stays open" — NOT reproduced as an app bug; traced to a documented harness race

Per instructions, this was attempted repeatedly (not "fixed" on a single miss) before drawing a
conclusion, because the row calls it intermittent.

**Three careful attempts, each with a real 1.2s sleep between opening the tab's own context menu
(right-click) and clicking its "Rename" row** (`Scripts/wayland-drive.sh`'s own header comment
warns exactly about this gap: deferred-menu content links into the real dispatch tree "only after
~2 real frames past the click that opened it," and a synthetic click arriving before that linking
finishes silently does nothing while the menu stays open — the identical symptom this row
describes): **3 for 3, the rename field opened cleanly every time**, screenshot at
`reference/linux-progress/wf-fix2-tab14/04-contextmenu-rename-WITH-settle-success.png`. No leaked
text in any trial (each was cancelled with Escape and reset, and the terminal's own prompt line
stayed empty throughout).

**Then the exact same gesture, deliberately with no sleep between the right-click and the "Rename"
click** (`rightclick 505 51` immediately followed by `click 483 133`, same coordinates as every
successful trial): the click landed and did **nothing** — the tab context menu is still fully open
in the resulting frame, cursor sitting on "Rename", no rename field, exactly the ledger's
description of "Close-menu-click can silently fail while the menu stays open." Screenshot:
`reference/linux-progress/wf-fix2-tab14/05-contextmenu-rename-NO-settle-silent-fail.png`.

**Conclusion**: this reproduces the *symptom* the ledger describes, but the controlled A/B (same
coordinates, same tab, only the settle time differs) traces it conclusively to the harness race
`wayland-drive.sh` already documents in its own header — synthetic input delivered faster than a
human can physically move a pointer, arriving inside the ~2-frame window before the deferred
tab-context-menu's dispatch tree finishes linking. This is very likely what the original wave M
drive hit (no sleep recorded between its right-click and its Rename click). **No source change was
made for this half of the row** — per instructions, a fix for a failure not actually reproduced as
an app defect is worse than no fix, and this one traces cleanly to the instrument, not the app. The
existing `committing_a_tab_rename_returns_focus_to_the_terminal` unit test already exercises this
exact right-click → Rename-click path in-process and passes, consistent with there being no app-side
race to fix.

### Verdict: half-proven (builder-driven, not critic-passed)

Proven: double-click-to-rename built, tested red→green, full suite green, live re-drive with a
`panel.list` + raw-scrollback hard discriminator. Also established: the context-menu "silent fail"
claim reproduces only as a documented harness timing artifact, not as an app defect, across 3
positive controls (with settle) and 1 confirmatory negative control (without settle).

**Gap for a fresh critic**: (1) re-drive the double-click fix independently to confirm it holds;
(2) if a critic can still reproduce the context-menu silent-fail *with* a real, generous sleep
between opening the menu and clicking Rename — not back-to-back synthetic clicks — that would
overturn this pass's harness-artifact conclusion and point back at a real app race, and should be
investigated fresh rather than assumed settled by this report.

---
