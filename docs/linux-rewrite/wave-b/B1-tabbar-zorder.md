# Wave B slice B1-tabbar-zorder — 8 rows to build

**the tab-bar popover z-order defect — ONE bug behind 8 rows**

## Files you own this wave

- `rust/crates/tiller/src/main.rs`

No other agent will touch these. **You must not edit any file outside this list** —
every other source file belongs to a sibling and an edit there is silently lost or
silently overwrites theirs. If a fix genuinely needs a file you do not own, stop and
report it rather than making the change.

## Rows

### `F-TAB-02` — ledger line 118, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** Wrap render_overflow_menu's return in gpui::deferred(...) (part of the shared tab-bar-popover fix); separately fix the has_overflow measurement that fires before real pixel overflow.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: overflow chevron rendered before real pixel overflow; clicking it twice showed no all-tabs list or selected-tab marker.

### `F-TAB-12` — ledger line 128, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** Item logic (Move Earlier/Later enablement) is already correct; only needs the shared popover-visibility fix (deferred() wrap on render_tab_context_menu), then re-drive.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: tab context menu never appeared after repeated right-clicks, so Move Existing Tab could not be invoked.

### `F-TAB-13` — ledger line 129, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** First conjunct (Split Right in the terminal pane) already PASSED live; second conjunct (tab-menu Move to ... entries) is blocked only by the shared popover-visibility bug, item logic already correct.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: the tab menu never appeared, so the required no-eligible-tab explanation was absent; a terminal Split Right is a different surface.

### `F-TAB-14` — ledger line 130, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** Rename logic (begin_tab_rename, rename field) is already wired; blocked only by the shared popover-visibility bug on the menu's Rename entry.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: tab context menu never appeared, so Rename was unavailable; the separately required double-click path was not reached.

### `F-TAB-15` — ledger line 131, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** Strip close-control (X) half already PASSED; context-menu Close conjunct blocked only by the shared popover-visibility bug.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: a tab context menu never appeared, so context Close Tab could not be used; the close-control trial remains unperformed.

### `F-TAB-17` — ledger line 133, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** Close Others / Close Tabs to the Right item logic already correct; blocked only by the shared popover-visibility bug.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: with four terminal tabs present, no tab menu appeared; Close Others and Close Tabs to Right were unreachable.

### `F-TAB-21` — ledger line 137, currently **FAILED — defective**

- **Triage:** build, size S
- **Files triage expects:** rust/crates/tiller/src/main.rs
- **Approach:** open_tab_menu state toggle is correct; the row's own evidence (working terminal-body right-click vs. dead tab-strip right-click) is the clearest instance of the shared popover-occlusion defect.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** P104 §Group 1: repeated tab-strip right-clicks produced no Tab menu despite a verified working terminal-body right-click; no move action was available.

### `F-TAB-25` — ledger line 141, currently **NOT EXERCISED**

- **Triage:** reclassify, size S
- **Files triage expects:** —
- **Approach:** Code and two drawn tests for Attach to Current Terminal exist in the tree today (main.rs:5823-6030+, tests at :11973/:12012), built same-day by P110/codex12 (commit 7289c84, 17:32) — the ledger's 'FAILED — absent... pass 14' text is byte-identical across every ledger revision since pass 14, including the 22:39 sweep this manifest was drawn from, so it was never re-checked after the build landed. Remaining owed work is only the live right-click exercise, itself blocked on the shared tab-bar popover fix.
- **Shared cause:** tab-bar popover paints behind centre-surface, not on top — see notes.
- **Evidence on record:** NOT EXERCISED (changed from FAILED-absent) 2026-08-14: prior 'still no attach-to-terminal code' is false -- code exists (main.rs:5823-6030) and I ran both cited tests myself (cargo test --package tiller), both pass, one via a real simulated right-click+click asserting the pane attaches. Not PASSED: a simulated GPUI-harness test isn't a live drive, and the live right-click gesture is unreachable on

