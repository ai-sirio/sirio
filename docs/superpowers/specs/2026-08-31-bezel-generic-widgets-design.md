# Bezel generic widgets adoption — design (sub-project 2)

Date: 2026-08-31
Status: approved
Umbrella: `docs/superpowers/specs/2026-08-31-bezel-gallery-adoption-design.md`
(sub-project 2 of 3). Requires sub-project 1 (bezel `=0.1.4`, gpui `=0.3.8`,
landed as `fe8d0989`).

## Goal

Replace `sirio_ui`'s hand-rolled generic widgets with their bezel 0.1.4
equivalents, by **direct adoption**: call sites import `bezel::ui::*` and the
hand-rolled implementations are deleted. No wrapper layer survives, with one
allowed exception — a local one-line helper where a family has many call
sites sharing Sirio-specific defaults.

## Scope boundary (decided)

Identity surfaces stay out and belong to sub-project 3: chat (including
`AnswerTextInput` and the composer), sidebar, changes/diff (including its
scroll handling), editor/file_view, orbit, project_identity avatars.

Families with no real call site in Sirio are not adopted: combobox, date,
pagination, stats, hover_card, palette.

## The five families, in execution order (rising risk)

One branch; one commit per family; each commit builds and passes per-crate
tests before the next family starts. A single user visual review gates the
end of the whole sub-project, not each family.

1. **tooltip** — bezel `Tooltip::text` / `Tooltip::with_keystroke` replaces
   `TextTooltip` and `controls::text_tooltip` (`controls.rs`), which are
   deleted. Call sites: `controls.rs`, `status_bar.rs`,
   `right_panel/history.rs`.
2. **menus/popover** — the five custom menus move to bezel `popover::Popup<T>`
   plus `floating::panel` (and `menubar::{Menu, Item}` only if the titlebar
   window menu already has menubar shape): sidebar row context menu, sidebar
   add-project menu, tab_bar overflow menu, right_panel/files context menu,
   titlebar window menu. Behavioral contract is invariant: same items, same
   actions, dismissal on outside-click/Esc; the existing tests over these
   menus are the net.
3. **table** — the commit list in `right_panel/history.rs` adopts
   `table::{Column, Sort, next_sort}` and bezel's `table`/`header`/`row`
   paint helpers.
4. **scroll** — the settings detail column's hand-rolled wheel handling
   adopts `scroll::ScrollbarState` / `transient`.
5. **input** — settings provider/cookie fields, `project_forms.rs` fields and
   the browser address bar adopt `input::TextField` (placeholder, undo,
   selection included). Requires wiring `input::init(cx)` into app bootstrap
   (`sirio`'s `main.rs`) and into every `TestAppContext` setup that exercises
   these fields; same for `menubar::init(cx)` if family 2 uses `Menubar`.

## Constraints

- No user-visible behavior change beyond the bezel look. Focus order,
  keyboard shortcuts, submit/dismiss semantics stay identical.
- Existing tests update only where they asserted rendering details of the
  old implementation; never where they assert behavior.
- Test failures compare as **lists** against the sub-project-1 baseline
  (known-red pool in `sirio`: 5 names, oscillating).
- Deletion per family: each commit removes exactly the hand-rolled code that
  family obsoletes, nothing else.
- Workspace gates (`Scripts/ci.sh`, `Scripts/ci-linux.sh`) only on the
  user's explicit request.

## Out of scope

- Sub-project 3 surfaces (listed above).
- Any bezel/gpui version change.
- New widget features not present in the current UI.
