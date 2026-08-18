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

## F-TAB-24 — Escape does not cancel a tab drag

**Reproduced live first.** Lane `wf-fix2`, `project.add` on this repo, two real tabs (`Chat`,
`TerminalRENAMEDTAB`, the latter left over from the F-TAB-14 pass in this same lane). Composing a
raw held-button drag from `wayland-drive.sh`'s primitives (`down`, `move`, `key`, `up` — the
`drag` helper alone cannot interleave a keystroke mid-gesture), before any code change: `down` on
`TerminalRENAMEDTAB`'s tab, `move` twice to cross into `Chat`'s hitbox (confirmed a real live
reorder by finishing the same gesture with a plain `up`, no Escape — order flipped from `[Chat,
TerminalRENAMEDTAB]` to `[TerminalRENAMEDTAB, Chat]`, screenshot
`reference/linux-progress/wf-fix2-tab24/03-positive-control-no-escape-reorders.png`), then
repeating from a reset baseline with `key Escape` sent *before* the `up` — the order still flipped
identically; Escape had no effect at all. This matches the ledger's finding exactly.

### Root cause

Two independent gaps, both confirmed by reading `rust/crates/tiller/src/main.rs` before touching
it: (1) `preview_tab_reorder` mutates `self.tabs` directly on every `on_drag_move` hover crossing
— there is no separate "commit on drop" step, so by the time any cancel signal could arrive the
live model is already changed with nothing recorded to put back; (2) there was no pre-drag
snapshot anywhere in the drag path and no Escape handling reachable from it at all — `grep -n
"click_count\|Escape"` around the tab-row render code and the drag closures returned nothing.

### Fix

`TillerWorkspace` gains `tab_drag_snapshot: Option<TabDragSnapshot>` (tab ids in their pre-drag
order, plus which id was active — not a clone of `OpenTab` itself, which owns live PTY/agent
entities). The tab row's `on_drag` closure (previously just `|_, _, _, cx| cx.new(|_|
gpui::Empty)`, i.e. it did nothing but build the invisible drag ghost) now also records that
snapshot at the moment the drag starts. A new `cancel_tab_drag` restores `self.tabs` to the
recorded order, recomputes `active_tab` from the recorded active id, and calls
`cx.stop_active_drag(window)` (the same GPUI primitive `workspace::Pane`'s own drag-cancel uses
upstream — see below). The tab strip's container `div` in `render_open_tabs` gets a new
`on_drop::<RowDrag>` that simply clears the snapshot on a real, completed drop (the reorder is
already committed live by that point; this only stops a *later*, unrelated Escape from reverting
an already-finished drag).

**Wiring Escape to `cancel_tab_drag` took two attempts, and the failure of the first is worth
recording because it is a real GPUI trap, not a Tiller-specific one.** The first attempt added the
check to `handle_root_key_down`, a `capture_key_down` raw-key listener already on the workspace
root (the same one the command palette's own Escape handling uses). This passed a from-scratch
regression test... until the test also simulated an actual held-button drag first — at which point
the raw-key listener stopped firing *at all* while `cx.active_drag` was `Some`, reproducible both
in a headless `#[gpui::test]` and, independently, live (the exact same `wayland-drive.sh`
down/move/Escape/up sequence: order flipped as if Escape had never been sent — screenshots from
that broken attempt were not kept, but the mechanism is the same one the final, working attempt's
positive control above demonstrates). Cross-checking the pinned `gpui` git revision's own
`crates/workspace/src/pane.rs` (`~/.cargo/git/checkouts/zed-a70e2ad075855582/c05e346/`, the exact
source this project's `gpui` dependency resolves to) shows Zed's own drag-cancel-on-Escape uses
`.on_action(cx.listener(|_, _: &menu::Cancel, window, cx| if cx.stop_active_drag(window) {} else {
cx.propagate() }))` — an **action** listener, not a raw key listener. `handle_close_settings_surface`
(already bound to the global `escape` → `CloseSettingsSurface` keybinding, previously wired only
into the `show_settings` render branch) now checks `cancel_tab_drag` first, and now is also wired
into the *main* branch — matching Zed's own pattern, including the `cx.propagate()` on the
"nothing to cancel" path, without which GPUI's default "actions stop propagation unconditionally
during the bubble phase" (`Window::dispatch_action_on_node_inner`) would have silently swallowed
every other Escape press in that branch, breaking the command palette's own Escape handling — this
exact regression was caught by the full suite (`escape_closes_the_palette_and_returns_focus_to_the_terminal`
failed) before being fixed by adding the `cx.propagate()` call. Both the broken-raw-key and the
fixed-action-based versions are documented inline at `handle_close_settings_surface`.

### Regression test

`rust/crates/tiller/src/main.rs`, `tests::escape_mid_tab_drag_restores_the_pre_drag_order`: builds
a 3-tab fixture, drags tab 0 over tab 2 with real `MouseDownEvent`/`MouseMoveEvent`s (no
`MouseUpEvent` yet — the button stays "held", mirroring a real mid-drag cancel rather than a
drop), asserts the live order already changed (so the test cannot pass vacuously), then dispatches
a real `escape` keystroke via `cx.simulate_keystrokes` and asserts the order and `active_tab` are
back to their pre-drag values, `tab_drag_snapshot` is consumed, and a subsequent `MouseUpEvent`
does not re-apply anything.

**Red, on the unfixed code** (verified by temporarily forcing `cancel_tab_drag` to `return false`
immediately, keeping the test unchanged):
```
thread 'tests::escape_mid_tab_drag_restores_the_pre_drag_order' panicked at crates/tiller/src/main.rs:14099:9:
assertion `left == right` failed: Escape mid-drag must restore the pre-drag tab order
  left: [1, 2, 0]
 right: [0, 1, 2]
```
**Green, after the fix:**
```
test tests::escape_mid_tab_drag_restores_the_pre_drag_order ... ok
```
Full crate suite: `cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller` →
**185 passed, 0 failed** — including
`tests::escape_closes_the_palette_and_returns_focus_to_the_terminal` and
`tests::escape_closes_the_settings_surface`, both of which the first (raw-key) attempt at this fix
had put at risk and the second (action-based) attempt does not.

### Re-driven live after the fix

Single `wayland-drive.sh` invocation, reusing the two-tab state from the reproduction above (`Chat`
first, `TerminalRENAMEDTAB` second — confirmed by screenshot, not assumed, since a prior corrupted
mid-drag `shot` in this same lane had briefly left the order swapped from an earlier attempt;
`WAYLAND-LANE.md`'s own warning that `shot` forces a real resize and must never land between a
`down` and its `up` is honoured throughout — every screenshot below is taken only after the
matching `up`):

```
down 537 51 ; move 500 51 ; move 355 51 ; key Escape ; up 355 51 ; sleep 0.3 ; shot after-escape
down 537 51 ; move 500 51 ; move 355 51 ;              up 355 51 ; sleep 0.3 ; shot no-escape-control
```

**Hard discriminator**: the drawn tab-strip order itself (this project's own `panel.list` reads
pane *existence*, not tab strip *position* — confirmed by reading its handler, `"panel.list"` in
`main.rs`, which lists `self.panes.list_for(...)`, a registry independent of `self.tabs`' order —
so a screenshot of the strip is the correct, not merely convenient, instrument for a
position/order claim here). With Escape mid-drag: order stays `[Chat, TerminalRENAMEDTAB]`,
identical to the pre-drag baseline
(`reference/linux-progress/wf-fix2-tab24/02-after-escape-cancel-order-unchanged.png` next to
`01-baseline-chat-first.png`). The immediately following positive control — the *exact* same
gesture with no Escape — flips the order to `[TerminalRENAMEDTAB, Chat]`
(`03-positive-control-no-escape-reorders.png`), proving the coordinates and gesture genuinely
drive a real reorder and that Escape, not some coordinate mistake, is what suppressed it.
Re-confirmed once more against the final, `cargo fmt`-formatted binary (`04-refmt-build-baseline.png`,
`05-refmt-build-escape-still-cancels.png` — order unchanged after Escape, same as above).

### Verdict: half-proven (builder-driven, not critic-passed)

Proven: reproduction of the original absence, root cause, a regression test red→green (with the
red run captured against the *fixed* test file and *disabled* fix, per instructions), full crate
suite green (185/185, including both pre-existing Escape tests this fix could have broken and
initially did break in its first form), and a live re-drive with a screenshot-based hard
discriminator plus an in-sequence positive control ruling out a coordinate/gesture mistake.

**Gap for a fresh critic**: (1) this drive only exercised the two-tab case; re-confirm with 3+
tabs and dragging from/to interior positions, matching the unit test's own 3-tab fixture. (2) The
row's clause is specifically about a *tab* drag; sidebar drags (`ReorderScope::Projects` /
`Worktrees`) use a structurally different preview/commit split (`Sidebar::preview_reorder`/
`confirm_reorder`, which *also* mutates its `rows` vector live on hover with no Escape handling of
its own) — untouched by this pass and not covered by the ledger row, but a critic auditing "drag
cancel" more broadly should know it is not fixed there. (3) Nobody yet re-confirmed that a
completed **drop** (not a cancel) still correctly clears `tab_drag_snapshot` live end-to-end
(covered by the unit test and reasoned through inline, but not independently re-driven this pass).

---

## F-CHG-02 — Files panel shows an unrelated real file tree on a genuinely empty catalog

**Reproduced live first**, fresh boot, no `project.add` at all (label `wf-fix2repro`). `panel.list`/
`ctl project.list`/`ctl workspace.list`/`ctl workspace.current` confirmed a genuinely empty catalog
(`projects:[]`, `workspaces:[]`, `workspace.current` errors `no current workspace`). The centre pane
correctly showed "No worktree selected. Add a project, then select a worktree." — but the Files
panel on the right simultaneously rendered this repo's own real tree (`.agents`, `.claude`, `App`,
`rust`, `Scripts`, …) rooted at `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, exactly as
the ledger row describes.
`reference/linux-progress/wf-fix2-chg02/01-empty-catalog-boot-BEFORE-FIX.png`.

### Root cause

`RightPanel` tracks its own `worktree_selected: bool`, separate from `TillerWorkspace::has_current_worktree()`
(which the centre pane already checks fresh on every render and got right). `RightPanel::new`
always starts `worktree_selected: true`, and the only two places that ever changed it were
`select_worktree` (replaces the whole entity on an explicit switch) and `close_workspace`'s
`clear_worktree` call. Neither runs at boot. `right_panel` is constructed at startup bound to
`working_directory`, which on an empty catalog is `initial_working_directory()`'s fallback — it
walks up from the process's cwd looking for a git ancestor and finds this real repo (a real,
existing directory; just not a selected worktree) — so the panel showed it as if it were current.

A second, adjacent instance of the *same* class of bug was found live-driving the fix itself (not
in the ledger row, but the identical failure shape): calling `project.add` on exactly the directory
`working_directory` already pointed at makes `sync_control_state` silently match it to the new
project's worktree and flips `has_current_worktree()` to `true` — the centre pane picks this up for
free since it re-checks every render, but `right_panel`'s cached bool has no mutation point on that
path either, so with only the boot-time fix it would swap from "wrong tree at boot" to "correctly
placeholder at boot, then permanently stuck on the placeholder forever after — even once a worktree
genuinely is current." A fix that only covered the literal ledger reproduction would have traded a
`FAILED — defective` for a different, plausibly worse one (first-run user adds the project their own
dev checkout already sits in — not a contrived case), so both are fixed together.

### Fix

`RightPanel` gains `bind_worktree` (`rust/crates/tiller_ui/src/right_panel.rs`), the mirror of the
existing `clear_worktree`: sets `worktree_selected = true`, rebinds `repo_root`, and resets the walk
state — idempotent (a no-op) when neither the selection state nor the bound path actually changed.
`TillerWorkspace::sync_activity` (`rust/crates/tiller/src/main.rs`) — already the app's one
continuous reconciliation point, called unconditionally on every `render()` and from ~44 other
mutation call sites — now reads `has_current_worktree()` on every pass and calls `bind_worktree` or
`clear_worktree` accordingly, instead of only ever calling `set_activity`. This is deliberately not
a one-off check at construction: because `render()` calls `sync_activity` every frame, *any* future
path that changes what "current" means self-heals the Files panel within one repaint, not just the
two call sites this pass happened to find.

### Regression tests

Two, `rust/crates/tiller/src/main.rs`:

- `tests::drawn_empty_catalog_boot_shows_the_right_panels_no_worktree_placeholder` — a new
  `empty_catalog_test_workspace` fixture (empty `ProjectCatalog`, `working_directory` a real,
  existing scratch directory — mirroring the real fallback exactly) boots a `TillerWorkspace` and
  asserts `right-panel-no-worktree` is drawn and `right-panel-files` is not.
- `tests::drawn_adding_a_project_matching_the_fallback_directory_binds_the_right_panel` — from that
  same empty boot, calls `control_add_project` on `working_directory` itself (a plain folder
  synthesizes a worktree at its own root, per `session.rs`'s `catalog_project`), asserts
  `has_current_worktree()` is now true, and asserts the panel switched to `right-panel-files` instead
  of staying on the placeholder.

**Red, on the code with only `set_activity` (both tests, unfixed):**
```
thread 'tests::drawn_empty_catalog_boot_shows_the_right_panels_no_worktree_placeholder' panicked at
crates/tiller/src/main.rs:17862:9:
an empty-catalog boot must draw the right panel's own no-worktree placeholder

thread 'tests::drawn_adding_a_project_matching_the_fallback_directory_binds_the_right_panel' panicked at
crates/tiller/src/main.rs:17891:9:
sanity: boot must start on the placeholder before the project is added
```
(The second test's *sanity* assertion is what goes red first on fully-unfixed code — proving the
whole chain, not just the narrower "explicit extra call in `control_add_project`" variant which was
tried and found redundant with the `render()`-driven reconciliation, and reverted rather than kept
as dead weight.)

**Green, after the fix:**
```
test tests::drawn_empty_catalog_boot_shows_the_right_panels_no_worktree_placeholder ... ok
test tests::drawn_adding_a_project_matching_the_fallback_directory_binds_the_right_panel ... ok
```
Full suites: `cargo test --manifest-path rust/Cargo.toml -p tiller --bin tiller` → **187 passed, 0
failed** (one pre-existing, unrelated flake seen once under this box's concurrent load —
`session::tests::the_debounce_collapses_a_burst_into_one_write`, a timing-sensitive debounce test
that passed clean in isolation and touches neither file this row changed);
`cargo test --manifest-path rust/Cargo.toml -p tiller_ui --lib` → **355 passed, 1 failed** — the
failure is `titlebar::tests::double_click_action_from_system_resolves_without_panicking`, which
reads this COSMIC desktop's real `action-double-click` setting (`Minimize`) against a hardcoded
`ToggleMaximize` expectation; pre-existing, environment-dependent, reproduces in isolation, and
`git status` confirms `titlebar.rs` carries none of this pass's edits.
`right_panel::tests::*` alone: **21 passed, 0 failed**.

### Re-driven live after the fix

Fresh label `wf-fix2final`, rebuilt+re-pinned `/tmp/wf-fix2-tiller`:

```
ctl project.list                         → []
ctl workspace.list                       → []
shot 01-boot-empty                       → Files panel: "No worktree selected" placeholder
ctl project.add path=<repo>              → added:true, worktreeCount:6
shot 02-after-project-add                → Files panel: header now reads the repo path, "Loading files…"
ctl workspace.current                    → real workspace, path=<repo>, branch=linux/gpui-waku
shot 03-after-settle                     → Files panel: full real tree, loaded
```

**Hard discriminator**: the Files panel's own drawn header/content across three frames of the *same*
running instance — placeholder text, then a path + "Loading files…", then the real tree — tracking
`workspace.current`'s control-socket answer at each step, not a single static screenshot.
`reference/linux-progress/wf-fix2-chg02/02-after-project-add-loading.png` and
`03-after-settle-full-tree.png`.

### Verdict: half-proven (builder-driven, not critic-passed)

Proven: reproduction of the ledger's exact original bug, root cause, a second related bug found by
live-driving the fix itself (not merely read), two regression tests each red→green on the real
unfixed code (not a doctored revert), full crate suites green modulo two pre-existing
environment/load-related failures unrelated to either changed file, and a three-frame live re-drive
covering both boot-empty and add-project-that-matches-the-fallback-directory.

**Gap for a fresh critic**: (1) the live re-drive's "add project" case happens to have this
project's own dev checkout as the fallback directory, which is what makes the match possible to
demonstrate at all on this box — a critic should also drive the more common real-world shape (add a
project at some *other* path while the fallback stays unrelated) and confirm the panel correctly
stays on the placeholder throughout, not just that it correctly switches when they do match. (2) The
`bind_worktree`/`clear_worktree` reconciliation now lives inside `sync_activity`, which is called
from ~44 other sites for unrelated reasons (agent status, chat state, …); this pass reasoned through
why each is a no-op when nothing about worktree selection changed and confirmed no visible regression
in the full test suites, but did not individually re-drive each of those 44 call sites live. (3) The
`titlebar.rs` double-click-action test failure is real and pre-existing on this desktop but is not
this row's to fix; flagged here so it isn't miscounted against this change by whoever reads the test
totals next.

---
