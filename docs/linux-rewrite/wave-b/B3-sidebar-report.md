# B3-sidebar — build report

Slice: `docs/linux-rewrite/wave-b/B3-sidebar.md`
Files owned this wave: `rust/crates/tiller_ui/src/sidebar.rs`,
`rust/crates/tiller_ui/src/project_identity.rs`, `rust/crates/tiller_ui/src/row_reorder.rs`
Branch: `linux/gpui-waku`, worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`

All 5 rows built. No foreign files needed — everything landed inside the three owned files.

## F-SID-17 — worktree/tab drag-reorder silently misplaces the dragged row

Triage's own hand-trace found no defect and flagged this as needing live instrumentation.
Reproduced it instead with a unit test built from the two existing (passing) drag-reorder
tests, but with `is_git: true` — the realistic case, since worktrees only exist on git
projects. Both existing tests (`dragging_worktree_rows_reorders_only_their_project_group`,
`dragging_tab_rows_reorders_only_their_worktree_group`) use `is_git: false`, which happens
to suppress the exact condition that triggers the bug.

**Root cause:** every real git project's row block always ends with a trailing
`RowKind::NewWorktree` ("New Worktree…") affordance row after its last worktree
(`from_projects`, unconditionally for `is_git` projects). `reorder_rows`' generic
worktree/tab insertion path computes `insert_position` relative to the reorder group only,
then looks up the real `self.rows` index via `remaining_indices.get(insert_position)`. When
a drag lands at the *last* position inside its group, `insert_position ==
remaining_indices.len()`, which is out of bounds — the old code's `.unwrap_or(self.rows.len())`
fallback then inserted at the very end of *all* rows, landing the dragged row after the
trailing `NewWorktree` row (or, in a multi-project sidebar, potentially past other projects
entirely) instead of right after its intended new sibling.

**Fix:** replaced the `self.rows.len()` fallback with "insert right after the group's last
remaining sibling" (`remaining_indices.last() + 1`). Verified this preserves the two existing
tests' behavior exactly (both happen to have nothing trailing the group, so `remaining_indices
.last() + 1 == self.rows.len()` coincidentally) while fixing the real-world case.

- Files: `rust/crates/tiller_ui/src/sidebar.rs` (`reorder_rows`)
- Tests: added `dragging_worktree_onto_last_sibling_lands_before_new_worktree_row` — asserts
  the *entire* row order (kinds + titles) after a same-project two-worktree drag with
  `is_git: true`; fails against the old code with branch-0 landing after the `NewWorktree`
  row, passes after the fix.
- **howToExercise:** in a real (git) project with two or more worktrees expanded in the
  sidebar, drag the first worktree row down onto the last worktree row (drop in its bottom
  half). The dragged worktree should land immediately below the row it was dropped on — not
  disappear, not jump past the "New Worktree…" row underneath it.

## F-PRJ-01 — add-project menu draws with no opaque background

Confirmed exactly as triage's superseded-pass-13 note describes: the `+` menu now opens
correctly (three items, Clone opens its form), but the `.when(add_project_menu, ...)` overlay
was inserted into the child list right after the sidebar header — so the Filter field and
every project row, both `.child()`-appended afterward, painted over it, even though the menu's
own background is fully opaque (`card_fill`, alpha 1.0). Pure paint/z-order bug.

**Fix:** moved the `.when(add_project_menu, ...)` block to sit after the other three
end-of-child-list overlays (`context_menu`/`project_settings`/`project_form`), matching their
established pattern. The menu is `.absolute()`-positioned, so only its paint order changes —
layout is untouched.

- Files: `rust/crates/tiller_ui/src/sidebar.rs` (`Render` impl child order)
- Tests: existing `add_project_menu_open_choice_reports_the_chosen_directory` and
  `add_project_menu_open_cancel_is_silent` still pass unchanged (they only assert behavior
  via `debug_bounds`/simulated clicks, not paint order, so they couldn't have caught this).
  No new automated test — GPUI's `TestAppContext` doesn't expose actual pixel/paint-order
  assertions; the existing repo pattern for this class of bug (see F-PRJ-01's own evidence)
  is a live screenshot.
- **howToExercise:** click the `+` next to "Projects" in the sidebar header. The resulting
  menu (Open Project…/Clone Repository…/Create Project…) should now have a fully opaque card
  background — the Filter field and any project row/path text directly underneath it must no
  longer show through.

## F-PRJ-11 — Project Settings has no removal control

Confirmed absent by full enumeration, exactly as triage found: `render_project_settings` had
no removal affordance anywhere (path, repo type, display-name field, optional Initialize Git,
icon picker, Close, id — no trash/remove). The removal logic itself
(`request_remove_project`: native confirm prompt, emits `SidebarEvent::RemoveProject`, files
untouched) was already correct and already used by the row's own context menu — just not
reachable from the settings sheet.

**Fix:** added a "Remove Project" control to `render_project_settings`, styled with
`theme.diff_deletion` (this crate's established destructive-text color, already used for
validation-error text elsewhere) and the `Icon::Close` glyph (the same icon this file already
uses for the worktree-row remove control — there is no dedicated trash icon in this port's
icon set, and adding one means editing `icons.rs`, outside this slice's owned files). Its
`on_click` calls the exact same `request_remove_project` the context menu path uses.

- Files: `rust/crates/tiller_ui/src/sidebar.rs` (`render_project_settings`)
- Tests: added `project_settings_remove_project_confirms_before_emitting` — opens Project
  Settings, clicks the new control, asserts a platform confirm prompt appears and nothing is
  emitted before an answer, then asserts `SidebarEvent::RemoveProject("proj-1")` fires on
  accept. Mirrors the existing context-menu-path test
  (`remove_project_context_item_confirms_before_emitting`), which still passes unchanged.
- **howToExercise:** right-click a project → Project Settings (or click the gear that appears
  on row hover). A "Remove Project" row with a close-glyph icon should now be visible in the
  sheet. Clicking it should bring up the native "Remove project from Tiller?" confirmation —
  accepting should remove the project from the sidebar; files on disk are untouched either way.
- wantedForeignFiles: none — `icons.rs` (outside this slice) would let a dedicated trash glyph
  replace the reused `Icon::Close`, but the control is fully functional without it.

## F-PRJ-12 — Project Settings sheet shows stale repo-type/path after Initialize Git

Confirmed the staleness half of triage's diagnosis by read: `open_project_settings` snapshots
`is_git`/`path` into the `ProjectSettingsCard` once, at open time. `set_projects` — how the
host reports *any* project-list change, including "Initialize Git" completing — rebuilds
`self.rows` but never touched an already-open `project_settings` card, so a sheet left open
across that host round-trip kept showing "Repository: Folder" and the Initialize Git button
even after the project actually became a repo.

The other half triage flagged (display-name propagation to the sidebar row) was already
correct by read — `on_display_name_key` writes `row.title` live — and is now additionally
confirmed by this same fix's new test, which asserts the open card and the underlying rows
stay in sync across a `set_projects` call without needing to close and reopen the sheet.

**Fix:** `set_projects` now looks up the rebuilt row matching the open card's project id (via
`project_ids`) and patches `card.is_git`/`card.path` in place before notifying.

- Files: `rust/crates/tiller_ui/src/sidebar.rs` (`set_projects`)
- Tests: added `set_projects_refreshes_an_open_project_settings_card` — opens Project Settings
  on a folder project (asserts `project-settings-initialize-git` is drawn), calls
  `set_projects` with the same project now `is_git: true` while the sheet stays open, and
  asserts the Initialize Git control disappears and `card.is_git` itself flips, with no
  close/reopen in between.
- **howToExercise:** open Project Settings on a non-git folder project, leave the sheet open,
  and run "Initialize Git repository" from the row's context menu (or however the host wires
  that action) without closing the sheet first. The open sheet should immediately switch from
  "Repository: Folder" + the Initialize Git button to "Repository: Git" with that button gone
  — no need to close and reopen Project Settings to see the update.

## F-PRJ-16 — "Open Emoji Picker" is a dead control

Confirmed triage's diagnosis exactly: `on_open_emoji_picker` was a bare `Rc<dyn Fn()>`
host-callback hook whose only caller anywhere in `rust/` was a unit test — the real mount
site (`sidebar.rs`'s `open_project_settings`) never chained it, and no picker overlay existed
anywhere to open. There genuinely is no Linux desktop-portal equivalent of macOS's system
character palette this crate could shell out to instead (confirmed: no such integration
exists in this codebase or its dependencies).

**Fix, deviating from the suggested approach:** rather than build the overlay and then wire
an external `Fn()` callback to open it from the mount site, I removed the host-callback seam
entirely and made the picker self-contained. The reason is structural, not stylistic: `Fn()`
takes no `Context`/`Window`, so a closure of that shape cannot mutate the picker's own GPUI
entity state (open a grid, focus a search field) no matter where it's wired from — the
callback shape itself was incompatible with "open an overlay". Since project_identity.rs is
this port's own file (not delegating to any OS service either way), the natural fix is for
`ProjectIconPicker` to own the overlay outright: clicking "Open Emoji Picker" now directly
opens a searchable grid (`EMOJI_CHOICES`, a curated ~40-entry set with keywords, the same
"curated subset" precedent `ProjectGlyph::ALL` already sets above it in this file). Typing
filters the grid by keyword or exact emoji match; clicking a swatch commits it immediately
(mirrors `select_glyph` in Icon mode) and closes the grid. No file outside this slice needed
any wiring as a result — `sidebar.rs`'s mount site is unchanged.

The grid is deliberately **not** `.absolute()` — it's appended as a normal child after the
field/button row, so later siblings can't paint over it the way F-PRJ-01 painted the
add-project menu.

- Files: `rust/crates/tiller_ui/src/project_identity.rs` (`EMOJI_CHOICES` const,
  `ProjectIconPicker` struct fields, `open_emoji_grid`/`close_emoji_grid`/
  `on_emoji_grid_query_key`/`pick_emoji_from_grid`, `render_emoji_mode`,
  new `render_emoji_grid`); removed `on_open_emoji_picker` field/builder (dead, no caller
  outside this file's own now-replaced test) and the now-unused `App` import.
- Tests: replaced `open_emoji_picker_button_is_inert_without_a_handler_and_fires_when_set`
  (tested the old dead-seam shape) with
  `open_emoji_picker_opens_a_searchable_grid_that_commits_on_click` — asserts the grid is
  closed until clicked, opens on click even with the field empty (the row's own VERIFY
  clause), narrows correctly on a "rocket" search (🚀 stays, 🐍 is filtered out), and that
  clicking 🚀 both commits `ProjectIconValue::Emoji("🚀")` through the existing `on_change`
  callback and closes the grid.
- **howToExercise:** open Project Settings on any project → the icon picker → Emoji tab.
  With the emoji field left empty, click "Open Emoji Picker" — a bordered card with a search
  field and a grid of emoji swatches should appear below the button (previously: nothing).
  Type e.g. "rocket" into that search field — the grid should narrow to matching swatches.
  Click 🚀 — the project's icon swatch should update to 🚀 immediately and the grid should
  close, without needing to also click "Set Emoji".

## Verification

```
cargo check -p tiller_ui                       → clean (0 errors; 1 pre-existing unrelated
                                                  dead_code warning on
                                                  BrowserSurface::pump_task, not owned by
                                                  this slice)
cargo test -p tiller_ui --lib sidebar::         → 33 passed; 0 failed
cargo test -p tiller_ui --lib project_identity:: → 9 passed; 0 failed
cargo test -p tiller_ui --lib row_reorder::     → 2 passed; 0 failed
cargo test -p tiller_ui --lib                   → 280 passed; 2 failed (chat.rs, file_view.rs
                                                  — both outside this slice's owned files,
                                                  mid-edit by sibling agents in this shared
                                                  worktree during this build pass; every
                                                  failure inside the three owned files is 0)
```

No commits to any file outside `sidebar.rs`/`project_identity.rs`/`row_reorder.rs`. Five
commits, one per row, in order: F-SID-17, F-PRJ-01, F-PRJ-12, F-PRJ-11, F-PRJ-16.
