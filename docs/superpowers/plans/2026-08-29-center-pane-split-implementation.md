# Implementation plan — center pane split (#318 map)

Spec: `docs/superpowers/specs/2026-08-29-center-pane-split-design.md`

Standing constraint: a worktree keeps ONE tab list (`self.tabs`); the pane is only
where a tab is drawn. Membership is derived from `TabKind`, never stored.

## Steps (each compiles before the next)

1. **`PaneRole` in `sirio_project`** — new enum next to `TabKind`;
   `TabKind::pane_role()`; `appears_in_sidebar()` redefined via `pane_role()`.
   Tests: routing + sidebar predicate.
2. **`CenterSplit` replaces `TabGroup`/`TabMachinery`** (+ the removal cascade,
   which must land in the same compiling unit):
   - `tab_machinery.rs` → `center_split.rs`; struct `{ primary_active,
     secondary_active, focused }` — no Vec, no Result, total constructor.
   - `OpenTab.group_id` deleted (~50 construction sites).
   - `activate_group` → `set_focused_pane(role)`; `rebuild_tab_machinery` →
     recompute role actives from `self.tabs`.
   - Removals: `move_selected_tab_to_new_pane`, `move_selected_tab`,
     `move_selected_tab_with_machinery`, `MoveTabToOtherPane` action + handler +
     palette entry + `has_other_pane` gate, "Move to Pane {id}" context items,
     `TabMachinery::add_group/move_tab/move_candidates/MoveTarget/MoveCandidates`.
   - Terminal pane cache key: role-keyed (`primary-pane-{id}` / `secondary-pane-{id}`).
   - `tab.cycle` / `tab.select` re-pointed at the focused role's tabs.
   - Tests: rewrite tab_machinery tests for CenterSplit; delete/rework the five
     two-group tests; rethink `drawn_detached_pane_group_offers_the_real_empty_prompt`.
3. **Layout: one strip per pane** — `center_column` becomes Primary stack
   (strip+surface) | divider | Secondary stack (strip+surface). Focus underline
   only on the focused pane's active tab. One `+` (the `TabBar` entity) inside
   the Primary strip; `×` at the end of the Secondary strip closes its tabs and
   triggers auto-close. Empty prompt Primary-only.
4. **`resolve_center_split`** in `panel_layout.rs`; `MIN_CENTER_WIDTH` becomes
   `min_center_width(secondary_open)`; `resolve_panel_widths` takes it as a
   parameter. Center divider drag payload writes a ratio; saving reuses
   `schedule_panel_width_save`.
5. **Persistence** — `appearance.centerSplitRatio` (default 500, 100..=900) in
   `AppSettings` + settings plumbing; schema v16
   `worktree.secondary_pane_open`; restore filter at materialisation (editor
   tabs restore only if the file exists); pane starts closed when no Secondary
   tab survives (flag honoured, not overridden).
   Done. Two notes for step 6: the in-memory `secondary_pane_open` field and
   the `secondary_pane_visible()` gate landed here rather than with the
   toggle, because restore is where "flag honoured" has to be true; step 6
   only adds the chord that flips it to `false`. And `open_secondary_pane()`
   is called from the four Secondary inserts, so a tab pushed by hand (two
   drawing tests do this) has to call it too or its pane is not drawn.
6. **`ctrl-shift-b`** through the five sibling points.
7. **Enter promotes the selected Changes row** in the right sidebar (Files
   already has `return_opens_the_selected_file_row`).
   Done, and larger than this line reads: `ChangesTab` had no keyboard
   selection to promote *from* — no selected row, no focus handle, no key
   handler. The selection is keyed by `(section, path)`, not by path, because
   a partially-staged file appears in two sections at once. Return promotes
   only where `embedded_in_panel` draws "Open diff"; inside the Changes tab
   it toggles the row, since the tab is the destination `OpenDiff` reveals.
8. `Scripts/ci.sh` → `CI OK`, then code review, then commit.
   Run on macOS 2026-08-31: `sirio` 312 passed / 3 failed, and the three are
   the pre-existing macOS red recorded on #292
   (`real_pty_layer_a_debounce_suppresses_first_title_and_accepts_second`,
   `drawn_changes_open_diff_action_reveals_the_existing_diff_tab`,
   `opening_changes_with_a_path_reveals_and_focuses_existing_tab`), plus
   `oh_my_pi_is_only_claimed_once_it_answers` in `sirio_agents` — a crate
   this branch does not touch; it compares the adapter's ACP claim against
   the `omp` binary installed on the machine. So `CI OK` is not reachable
   here until that red is cleared, and no new failure came from this work.

   The split was also checked by eye: the app was launched with
   `Scripts/build-dev.sh`, driven to a two-pane state through
   `sirioctl surface changes open`, and the two halves, the divider drag and
   `ctrl-shift-b` all behaved. The run left no panic or error in
   `/tmp/sirio-dev.log`.

## Invariants to hold

- `close_tab` fallback: nearest remaining tab in the same role.
- `hide (ctrl-shift-b)` keeps tabs; `×` closes tabs — never conflate.
- Secondary pane auto-closes when its last tab closes; focus returns to the
  Primary pane's active tab.
- The ratio is a projection at render time, never written back by clamping.
- No drag may move the sidebars; only the center divider drag writes a ratio.
