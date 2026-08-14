# Wave A slice W10-chg — 5 rows needing no source change

Triage says each of these needs only exercising, or that its verdict looks
wrong. **Change no code. Do not edit the ledger.**

## `F-CHG-01` — ledger line 192, currently **FAILED — absent**

- **Triage says:** reclassify
- **Approach:** Row's literal 'toggle within one right panel' spec is superseded by a documented, still-current architecture pivot (P21-mount-the-changes-surface.md, P62-the-changes-panel.md 'Leave these' list, 04-ux-patterns-waku-does-not-cover.md:35): Changes is deliberately a first-class tab, not a right-panel section. The underlying capability (see files + changes for the same worktree without losing context) is present via two peer surfaces. Recommend re-wording/retiring the row against the tab model rather than building a redundant toggle.
- **Evidence on record:** Promoted from half-proven: live toggle (close+reopen) confirmed pure Files show/hide both directions — no alternate Changes view exists anywhere in the toggle. shots/215,216.

## `F-CHG-03` — ledger line 194, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** right_panel.rs:698-736 already draws a real Loading/Retry state with named tests at :1425 and :1451 (built per P62's brief). Live evidence only drove the near-instant happy path and never attempted Retry. Re-drive Retry (chmod 000 the repo) and capture the loading flash at a shorter interval before concluding a defect.
- **Evidence on record:** P104 §Group 3: Refresh produced neither loading state nor visible transition in immediate and delayed captures; inaccessible/Retry trial remains unperformed.

## `F-CHG-05` — ledger line 196, currently **FAILED — defective**

- **Triage says:** reclassify
- **Approach:** on_file_key (right_panel.rs:585) correctly implements Down/Up=select, Space=toggle-dir-only, Enter=open-file-or-toggle-dir, exactly per spec, and STALE-FAILED-CENSUS.md independently flags this row 'already built'. Live evidence tested Space on a file (correctly a no-op) and never tried Enter on a file (the actual open gesture). Re-drive with Enter-on-a-file included.
- **Evidence on record:** P104 §Group 3: arrow/Space moved at most a selection cursor and never opened/expanded/changed the active file as required.

## `F-CHG-18` — ledger line 209, currently **NOT EXERCISED**

- **Triage says:** exercise
- **Approach:** Drag source (changes.rs:993, on_drag with a real (PathBuf,String) payload) and drop target (tiller_terminal/src/lib.rs:1448, production on_drop wired to receive_diff_drop) are both real, non-test code (QUEUE.md already resolved a stale two-reference contradiction on this exact point). Block is a Wayland-lane synthetic-drag capability gap. Prove via the same real-mouse-event recipe the in-process test harness already uses (right_panel.rs:1940), or via the DISPLAY=:1 X11 route this manifest names as blocked for this slice.
- **Evidence on record:** NOT EXERCISED (unchanged): independently re-confirmed via wayland-drive.sh and WAYLAND-LANE.md that no drag primitive exists on this lane and the only route (DISPLAY=:1) is barred to this slice -- environmental block, not platform-impossible.

## `F-CHG-22` — ledger line 213, currently **half-proven**

- **Triage says:** exercise
- **Approach:** Data-tier mapping (main.rs:1991 AgentStatus::NeedsInput -> ActivityStatus::NeedsInput) and per-status glyph/color rendering (right_panel.rs:1142-1156, all 5 statuses) are both correct and complete. Only owed: navigate to and expand the actual Activity section (not the Changes tab) with running/done/needs-input agents present, and capture it.
- **Evidence on record:** half-proven (unchanged): panel.state now proves running/done/error differ at the data level (exitStatus success vs code:1, live Whisking spinner in scrollback) -- new and real, but all three new captures show the unrelated Changes tab, not the Activity section (visible only collapsed) or terminal pane. Zero visual corroboration for these three glyphs; owed half is unchanged: navigate to and capture the Activity secti

