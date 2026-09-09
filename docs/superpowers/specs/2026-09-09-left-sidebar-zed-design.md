# Left sidebar, Zed-flavoured — design

Date: 2026-09-09
Status: design approved in brainstorming, spec pending user review

## Problem

The left sidebar (`rust/crates/sirio_ui/src/sidebar.rs`, 9,873 lines) draws a
three-level tree: project → worktree → tab. Every level is a row; the project
row carries a disclosure triangle and a `New Worktree` action row among its
children; a tab is a row of its own. The reference the user brought is Zed's
own left sidebar (`crates/sidebar/src/sidebar.rs` in a local `zed-ref`
checkout): a flat search field, a lowercase project header that stays put, one
compact entry per thread, a full-bleed selection, and text that **fades** at
the right edge instead of ending in an ellipsis.

The ask is not a reskin. Sirio keeps its own subject — worktrees, not threads —
but adopts Zed's grammar: sections instead of an expandable project row, one
row per worktree, tabs demoted from rows to pills inside their worktree's row,
and no indentation anywhere.

## Decisions

Each was put to the user as a set of options; the chosen one is listed.

| Question | Chosen |
|---|---|
| Depth of the change | Skin **and** structure **and** row content, keeping today's details |
| What a row is | Worktree; its tabs become pills inside the same row, one visible level |
| Pill interactivity | Click opens the tab, hover shows a `×` that closes it, a `+` opens a new agent |
| Project header | Name + count, `+` and `⋯` on hover, no `New Worktree` row, header sticky while its section scrolls |
| Search field | Flat and full-width (bottom border only), matching comments and agent names too |
| Relative time (`12m`) | **Not shown.** Plain status text takes that slot |
| Second line | Comment when present, otherwise an abbreviated path, with the pills at its right |
| Fade | On the branch title, the second line, and the project name |
| Implementation shape | Extract `sidebar.rs` into a module directory |

Two of these removed work rather than adding it. Dropping relative time means
no new `last_activity` has to be recorded for terminal panes — only chat tabs
persist one today (`sirio_persistence/src/db.rs:599`, `MAX(chat_turn.updated_at)`).
Choosing the extraction over a feature-flagged double render means one render
path, not two, and no flag to retire later.

Prototypes that settled the visual questions, both throwaway and both in the
repo:

- `docs/prototypes/sidebar-zed.prototype.html` — four sidebar shapes side by
  side (single-line row, two-line card, hover rail, today's tree as baseline).
  The two-line card won.
- `docs/prototypes/sidebar-zed-row.prototype.html` — three second-line layouts
  under the real 325px width, with the deliberate edge cases (long branch, long
  comment, no comment, three tabs, zero tabs), plus the fade and a draggable
  edge to watch text slide under it. Layout A won.

## The fade

Zed implements this as `GradientFade` (`crates/ui/src/components/gradient_fade.rs`):
an absolutely positioned div on the right edge, full row height, filled with
`linear_gradient(90°, row_background → transparent)`. The sidebar uses it at
92px wide with the gradient stop at 0.7 (`crates/sidebar/src/sidebar.rs:2322-2330`),
passing three colours — normal, hover, active — and switching between them with
`group_hover`, so the veil always matches the row underneath. Two details are
not obvious from the screenshot: the colours are flattened onto the opaque app
background with `blend` first, and on transparent windows the fade is disabled
in favour of a plain ellipsis, because the gradient shows up as a visible patch
(`sidebar.rs:2292-2295`).

**That file is not copied.** Zed's `crates/ui` is GPL-3.0-or-later and Sirio is
MIT. Sirio reimplements the effect against `gpui::linear_gradient` and
`linear_color_stop` (Apache-2.0, and already used for the thought-block fade at
`sirio_ui/src/chat/thought.rs:187`). The numbers above — 92px, stop 0.7, the
three-colour switch — are adopted as measurements, not as code.

## Modules

`sidebar.rs` becomes a directory, following what `right_panel/` already does
(`mod.rs` plus one file per surface):

- **`sidebar/mod.rs`** — the `Sidebar` entity: state, events, `visible_rows`,
  context menus, the worktree prompt, the project form, drag and reorder. The
  crate's public surface (`Sidebar`, `SidebarRow`, `SidebarEvent`,
  `SidebarProject`, `SidebarWorktree`, …) is re-exported from here unchanged, so
  `sirio`'s `main.rs` does not move.
- **`sidebar/section.rs`** — the project header: name, count, `+`, `⋯`,
  collapse, sticky behaviour.
- **`sidebar/row.rs`** — `RowView` and the two-line worktree card, including the
  pills.
- **`sidebar/fade.rs`** — the veil, as a small `RenderOnce` element taking the
  three background colours, a width and a stop. It stays local to the sidebar
  until a second caller exists.

No behaviour changes in this step; it is a move plus `mod` declarations.

## Data model

`SidebarRow` stops describing tabs and starts owning them.

- `RowKind::Tab` and `RowKind::NewWorktree` leave the flattened list.
  `RowKind::Project` stays as the section header's identity, no longer a
  selectable row.
- A worktree row gains a list of pills, each carrying what a tab row carries
  today: its `tab_id` **or** `parked_tab` position, `agent_icon`,
  `agent_brand`, its `ActivityStatus`, and a label for the tooltip. A parked
  tab's pill is drawn dimmed and restores the tab on click, the same
  distinction `SidebarTabRef::Parked` makes today.
- `running_agents` (the trailing badge) is subsumed by the pills: a running
  agent is a pill with a running status dot. The field is removed rather than
  drawn twice.
- `visible_rows` filtering widens from titles only to project name, branch,
  comment and agent name.
- Two events join the existing set: close-tab (from a pill's `×`) and
  new-agent-for-worktree (from the `+`). Closing a tab from the sidebar is new —
  today it is only reachable from the tab bar — so it emits the same host
  transition the tab bar's close does, rather than a second path into teardown.

## Render and interaction

- **Card**, 51px, the height today's two-line card already uses. Line 1: status
  dot, branch, `★` when primary, status text on the right, coloured by status
  (`running`, `needs input`, `done`, `error`, `idle`, and `N agents` when a
  still worktree holds more than one tab — the UI's copy is English, as
  everywhere else in the sidebar). Line 2: the comment when present,
  otherwise an abbreviated path, with the pills right-aligned and the `+` shown
  only on the selected row.
- **Pill**, 20px: the agent glyph in its brand colour, a status dot at the top
  right, and on hover a `×` replacing the glyph. The active tab's pill carries
  an accent outline.
- **Section header**: sticky within its own section, with `+` and `⋯` appearing
  on hover; clicking the name collapses the section.
- **Search**: full width, bottom border only, no fill.
- **Fade**: title, second line and project name, per the numbers above.
- **Keyboard**: `↑`/`↓` move between worktree rows, `←`/`→` move between the
  pills of the cursor row, `Enter` opens, `Backspace` closes the focused pill.
  This replaces today's cursor walk over tab rows.

gpui has no `position: sticky`. The sticky header is drawn by reading the
list's scroll offset and painting the current section's header at the top of
the viewport. It is the most expensive item here and the only one whose
absence leaves the sidebar correct, which is why it ships last.

## Delivery

Six phases, each verifiable with `cargo test -p sirio_ui` before the next
starts:

1. **Extraction.** `sidebar.rs` → `sidebar/{mod,row,section,fade}.rs`, no
   visual change. Verify: the suite's pass/fail list is identical to the one
   recorded before the move.
2. **Section header and search.** Sticky excluded. Header actions replace the
   `New Worktree` row; the search field goes flat and its match widens.
   Verify: new tests for header actions and for a comment-only match.
3. **The card.** Two-line layout with status text; tab rows disappear from
   `visible_rows`. Verify: row-metric assertions in `conformance.rs` updated to
   the new heights, and a test that a worktree with tabs yields exactly one row.
4. **Pills.** Click, `×`, `+`. Verify: one event test per action, including a
   parked pill restoring rather than opening.
5. **Fade.** Verify: a test that a title too long for the row still renders
   without an ellipsis glyph, and that the veil's colour follows the row state.
6. **Sticky header.** Verify: with a section scrolled past its start, the
   header is still drawn at the top of the viewport.

Tests to touch: everything that looks up a tab row by debug selector, and the
row-metric inventory in `sirio_ui/src/conformance.rs`.

## Out of scope

- The right panel (`right_panel/`) keeps its icon rail and its four views.
- No persistence change: no new timestamp, no schema migration.
- No change to how worktrees are mounted or how panes are torn down; the
  sidebar keeps emitting the transitions the host already handles.

## References

- Zed's sidebar and its fade: `crates/sidebar/src/sidebar.rs:2280-2430`,
  `crates/ui/src/components/gradient_fade.rs` (GPL-3.0-or-later — read, not
  copied).
- Sirio's existing gradient use: `sirio_ui/src/chat/thought.rs:187`.
- Module layout precedent: `sirio_ui/src/right_panel/`.
- Prototypes: `docs/prototypes/sidebar-zed.prototype.html`,
  `docs/prototypes/sidebar-zed-row.prototype.html`.
