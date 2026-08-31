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

> **Revision (2026-08-31, pre-plan verification):** two families dropped by
> the spec's own no-real-site rule — **table** (the history commit list has
> no header and no sort; its adaptive column budget is app logic, not a
> table) and **scroll** (settings scrolls via gpui's native
> `overflow_y_scroll`; the only hand-rolled scrollbar logic lives in chat, a
> sub-project-3 surface). The titlebar window menu is the native platform
> menu (`Window::show_window_menu`) and leaves the menus family; the twin
> `render_tab_context_menu` in `sirio/src/main.rs` joins it instead. A new
> prerequisite emerged: nothing calls bezel's `Theme::install_custom`, so
> bezel components that read `Theme::of(cx)` internally (Tooltip, menu
> chrome) would render with the unbranded palette — wiring Sirio's branded
> theme into bezel's registry precedes the families. Every editable surface
> in `sirio_ui` is a custom `StyledText` editor sharing `caret::Blink`; this
> sub-project converts only the three input sites listed below, the other
> caret consumers stay for sub-project 3 or later.

## The families, in execution order (rising risk)

One branch; one commit per family; each commit builds and passes per-crate
tests before the next family starts. A single user visual review gates the
end of the whole sub-project, not each family.

0. **theme wiring (prerequisite)** — install Sirio's branded bezel theme
   into bezel's own registry (`bezel::theme::Theme::install_custom`, kept in
   sync where `sirio_theme` already syncs appearance), so `Theme::of(cx)`
   inside bezel components resolves to the branded palette, not the default.
1. **tooltip** — bezel `Tooltip::text` / `Tooltip::with_keystroke` replaces
   `TextTooltip` and `controls::text_tooltip` (`controls.rs`), which are
   deleted. Call sites: `controls.rs:293` and three `.tooltip(...)` lines in
   `changes.rs` (mechanical swap only — changes stays a sub-project-3
   surface otherwise).
2. **menus/popover** — the five custom menus move to bezel
   `popover::{Popup, menu_at, anchored_menu_below, popover_card, menu_row,
   reap_popup}`: sidebar row context menu, sidebar add-project menu,
   tab_bar tab context menu, right_panel/files context menu, and the twin
   tab context menu in `sirio/src/main.rs`. Behavioral contract is
   invariant: same items, same actions, dismissal on outside-click/Esc; the
   existing tests over these menus are the net.
3. **input** — `project_forms.rs` fields and the browser address bar
   (`AddressEditor`) adopt `input::TextField` (placeholder, undo, selection
   included). Requires wiring `input::init(cx)` into app bootstrap and into
   every `TestAppContext` setup that exercises these fields.
   **Settings credential fields excluded (execution revision):** bezel 0.1.4's
   `TextField` has no secure/password display mode — it always paints the
   real shaped text. The OpenCode/Ollama credential fields render mask dots
   and must never paint the secret, so they keep the existing masked caret
   editor until bezel ships a secure input mode (worth an upstream issue).

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
