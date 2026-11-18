# The sidebar in Codex's shell

**Date:** 2026-11-17
**Status:** approved by Enzo, 2026-11-17
**Builds on:** `2026-11-08-zed-layout-parity-design.md` (the sidebar's metrics),
`2026-09-24-sidebar-sessions-view-design.md` (the Sessions view),
`2026-11-01-unread-sessions-design.md` (unread and status on a row).
**Mockup:** `2026-11-17-sidebar-codex-shell-mockup.html` beside this file
(direction D of four drawn on 2026-11-17). It is schematic: Italian placeholder
copy, made-up projects and branches. Where it differs from this document, the
document wins.

## §0 Intent

Enzo put a screenshot of the Codex desktop app's left sidebar next to Sirio
and asked for a Sirio sidebar like it, built with Ely. Four directions were
drawn: worktrees as plain rows (A), sessions listed under each project (B), a
three-level tree (C), and Codex's shell around Sirio's existing worktree cards
(D). **He picked D.**

D takes what makes Codex's sidebar feel calm — a titled header with
notification and search icons, a New Chat row, a foldable Projects section,
folder rows, actions and detail that appear only under the pointer, a hover
card beside the row — and keeps what Sirio's sidebar lists and how it lists it:
one two-line card per worktree, with its tab pills.

## §1 Decided with Enzo

In the order he decided them:

1. **Direction D.** Rows keep listing worktrees as today's cards; only the
   shell around them changes.
2. **The hover actions on a worktree card are existing actions**: ★ set or
   unset primary, and × remove worktree (the same confirmation as today). No
   pin, no archive, no new persisted data.
3. **Ely: today's `main` plus a targeted port.** The branch is rebased on
   `main` (which carries the 2026-11-17 targeted sync, #767). The components
   this sidebar uses are brought up to upstream
   (`ELY-Open-Source/Ely-GPUI-Components`) by porting them onto the current
   engine (`bezel-gpui =0.3.8`), the way #767 did. No engine swap and no full
   Ely sync here; those are the Ely programme's step 1
   (`2026-11-17-gpui-pre-engine-swap-design.md`, on branch `ely-chat`).
4. **The Bell carries the Sessions attention signal too** (error, needs
   input) as well as unread, since the Projects | Sessions switch that showed
   it goes away (§3.1).

## §2 Goals, non-goals, anti-goals

**Goals**
- The sidebar's chrome reads like Codex's: header, New Chat, Projects section,
  folder rows, hover-only actions, hover card, overlay scrollbar. (The list's focus ring
  shipped with #794 and was removed afterwards at Enzo's request: a focused
  list draws no blue outline.)
- Built from Ely components and `theme.ely` tokens.

**Non-goals**
- Changing what a row lists. Worktrees stay two-line cards with tab pills; no
  session rows under projects (that was B).
- Pin and archive (§1.2).
- Redesigning the Sessions view. Only its way in moves (§3.1).
- The engine swap or a full Ely sync (§1.3).
- The primary rail and its peek, which live in the centre split, not here.

**Anti-goals** — it fails even if it looks right when:
- any current capability is lost: the filter, drag reorder of projects and
  worktrees, arrow-key navigation (`tree_keys`), the project and worktree
  context menus, unread marks and status blooms, expand all / collapse all,
  add project, the Sessions view, the persisted view choice;
- the hover card takes focus, swallows a click meant for the row, or shows up
  during a drag or while a menu or popover is open;
- a colour is hard-coded instead of a `theme.ely` token (both themes must
  work, and Onice);
- anything new is persisted.

## §3 The design, top to bottom

UI copy stays in English like the rest of the app (the mockup's Italian is
placeholder). **(my call)**

### §3.1 Header — 40 px

- **Left: "Sirio" + ChevronDown.** 13 px semibold (the Zed-parity rule keeps
  interface text at 13). A click opens a menu with two checkable items,
  **Projects** and **Sessions**, the current one checked. This replaces the
  Projects | Sessions switch (`view_tab`, `sidebar/mod.rs`), and the choice is
  still persisted as `SidebarView` is today. **(my call; Enzo saw it in the
  playback and did not object)**
- **Right: Bell**, a ghost `IconButton` with a `DotBadge`. It carries **both**
  of today's signals (decided with Enzo, §1.4): the Sessions segment's
  attention dot (`sessions::attention`: error, or needs input) and unread
  (`TabAttention.unread`). The dot takes the most severe: `theme.ely.danger`
  for an error, `warning` for needs input, `info` for unread (the colour of a
  row's unread dot today); no dot when
  none applies. A click opens a popover listing those sessions, the most
  severe first and newest first within each: title, its reason ("Error",
  "Needs input", "Unread"), project · branch, relative time (`sessions.rs`
  `relative_time`). Choosing one does what choosing that session in the
  Sessions view does (`SelectTab` / `SelectParkedTab`) and closes the
  popover. Empty state: "Nothing needs you". The existing tests of the
  attention dot move from the segment to the Bell.
- **Right: Search**, a ghost `IconButton`. It toggles the existing filter field
  under the header: hidden → shown and focused; shown, empty and focused →
  hidden, focus to the list; otherwise (holding text, or not focused) → stays,
  focused. `Esc` clears and hides it.
  It never hides on blur (decided while building: a field that vanishes when
  focus leaves shifts the list under a click and strands focus that a dialog
  restores to it). Choosing a view from the title menu focuses the list. Matching (`row_matches`) and the "folded projects open
  while filtering" rule are unchanged. No shortcut focuses the filter today.
  Switching view clears the filter (as today) and now also hides it. The
  sidebar's `Focusable` handle (used to give focus back when Settings closes)
  is the filter while it is shown and the list otherwise. **(my call)**
- Expand all, collapse all and add project leave the header (§3.3).

### §3.2 New Chat row — 31 px

SquarePen icon + "New Chat". A click opens, anchored under the row, the same
list the worktree context menu's New Chat expands into (one
`NewChatAgent(id)` per agent, then "Other agents…"), aimed at the selected
worktree. Sirio has no default agent, so a one-click chat is not possible.
With no worktree selected the row is disabled, with the tooltip "Select a
worktree first". **(my call)**

In the Sessions view the row is the same.

### §3.3 Section header — 28 px

"Projects" (or "Sessions" in that view) in `fg_subtle` + ChevronDown, a
`Disclosure`. A click folds the whole list, in memory, like project folds. On
the right: **+ add project**, always visible; **expand all / collapse all**,
visible only under the pointer. In the Sessions view the right side carries
nothing new. **(my call)**

### §3.4 The list

- An overlay scrollbar that fades when idle: Sirio's `scroll::bar` over the
  list's `ScrollHandle`, which wraps Ely's `Scrollbar` (Ely's `ScrollArea` has
  no caller in Sirio; the wrapper is the established pattern).
- ~~A 1 px `theme.ely.focus` border, radius 8, only while the list has keyboard
  focus.~~ Shipped in #794, then removed at Enzo's request: the list draws no
  focus ring, focused or not.

### §3.5 Project row — 31 px

Replaces today's section-style project header. The height stays 31, the
Zed-parity section-header height, rather than Codex's 30. **(my call)**

- Leading: Ely `Folder` when folded, `FolderOpen` when expanded. No chevron:
  the folder's state is the fold signal, as in Codex.
- Name at 13: `fg` when expanded, `fg_muted` when folded.
- Trailing: the unread count while folded (as today). The worktree count is
  dropped. `+` and `⋯` show only under the pointer (today they are always
  drawn). **(my call)**
- Click, drag, context menu and keyboard behave as today's project header.
- 8 px of space after an expanded project's last card.

### §3.6 Worktree card — 53 px, as today

Today's two-line card (`row.rs` `render_row`), with these changes:

- The card's **content** is indented 24 px so it aligns with the project's
  name; the card's box stays full width, so the hover wash spans the sidebar
  as Codex's does and `test-zed-layout-e2e.sh`'s "card starts and ends at the
  panel's content edge" still holds. **(my call)**
- The leading slot is unchanged: GitBranch. (Corrected while planning: the
  identity-icon branch in `row.rs` never runs for a card today.)
- "idle" / "N agents" in `fg_subtle`.
- Under the pointer: `theme.ely.hover` wash, radius 6, and on the title line
  two ghost icon buttons replace the status bloom: **★** (Set Primary; filled
  and meaning Unset Primary on the primary; the same `ContextAction` the
  context menu emits) and **×** (not on the primary). The × does exactly what
  today's hover × does: it opens the worktree close menu
  (`open_worktree_close_menu`: "Remove from disk", "Remove … remote and
  disk"), which leads to the confirmation. (Corrected while planning: the
  draft said it went straight to `request_remove_worktree`.) Off the pointer
  the bloom and the ★ primary marker show as today.
- Pills, their rings, the unread dot and the sub-line are unchanged.

### §3.7 Hover card

- Opens after **500 ms** under the pointer on a worktree card and stays
  **200 ms** after leaving (Ely `HoverCard`'s timings).
- Placed **beside the sidebar**: its left edge 4 px past the sidebar's right
  edge, its top aligned with the card's top, clamped inside the window.
- Content, 290 px wide, `theme.ely.overlay`, `border`, radius 12:
  - branch name (at most 2 lines) and, right-aligned, clock + time since the
    worktree's last event;
  - folder icon + project name;
  - a rule, then one line per tab: its status (bloom, unread dot, or nothing)
    or a terminal icon, then its title, ellipsised.
- Read-only: no buttons, never focused, never intercepts clicks on the row.
- Times (here and in the Bell's popover) are computed from the wall clock
  **when the card or popover opens**, not from the sidebar's `now_ms`, which
  only the Sessions view refreshes. The minute clock still runs only in the
  Sessions view (`the_minute_clock_runs_only_in_the_sessions_view` stays
  true); a card left open for minutes may show a stale time. **(my call)**
- Never shown during a drag, while a context menu or popover is open, or while
  the sidebar is narrower than 200 px. It closes when the list scrolls or the
  row it describes leaves the list (filtered out, folded, removed). A key
  press may close it (bezel reports a keypress as hover loss). **(my calls,
  made while planning)**

### §3.8 Sessions view

Content unchanged. It is reached through the title menu (§3.1), shares the
header, the New Chat row and the list's scroll area.

## §4 Ely

**Already vendored and used as they are:** `IconName::{Folder, FolderOpen,
Bell, Search, SquarePen, Star, X, Plus, Ellipsis, ChevronDown}` (Sirio's own
`icons.rs` has no bell, pen or star, so the sidebar's chrome uses Ely's set),
`IconButton` (Ghost), `DotBadge`, `Disclosure`, `menus::{Menu,
layer, Hang}`, `Tooltip`.

**Port reference: upstream `main` at `2d827752`** (2026-11-15, the newest; Enzo
asked for the new Ely), not #767's `7f7a715d`. Upstream is now split into
crates on `gpui-pre =0.3.12`, so each port is translated to bezel-gpui's API.
These components are also drawn by the chat (`Disclosure` in `agent/tools.rs`,
`chat/status.rs`, `chat/code.rs`; `HoverCard` in `chat/cite.rs`): **the chat
keeps today's look and behaviour**. An upstream change of default that would
alter it is either kept behind the sidebar's call site or left out and listed
under "Not ported" in `LOCAL-CHANGES.md`. The chat's E2E must stay green.
**(my call)**

**Result of the comparison (made while planning): nothing is ported.**
Against `2d827752`, `Disclosure`, `Collapsible` and `DotBadge` differ only in
doc comments; `HoverCard`'s only change needs `HoverListenerMode`, which
bezel-gpui 0.3.8 does not have; `ScrollArea`/`Scrollbar` gained a new source
API and a press-blocking track strip that would change the chat's bars. No
upstream hover or popover places beside its trigger. The comparison and its
reasons go into `LOCAL-CHANGES.md` ("Compared, not ported, 2026-11-17"). The
sidebar's hover card is Sirio's own: placed beside the sidebar and drawn with
Ely's `raise` (deferred draw), because the sidebar's root clips its children.

**Not used:** upstream `lists::{Tree, List, Sections}` and `layout::Sidebar`
(a width-animated rail with no sections). D keeps Sirio's own rows.

## §5 What must keep working

The anti-goals' list (§2), plus: the Zed-parity measures that do not change
(sidebar width 300, card 53); `set_projects` carrying folds over by catalog id;
the filter's placeholder strings; the persisted selected worktree.

## §6 Testing

End to end, per the repo's conventions: a framed E2E (see
`docs/agents/e2e-tests.md`) that drives a real instance in light and dark and
captures, with the transcript as the re-runnable artifact:

- the header, New Chat row and section header;
- a folded and an expanded project;
- a hovered card with its ★ and × and its hover card beside the sidebar;
- the bell's popover with one unread session, and choosing it;
- the filter shown by Search and hidden by `Esc`;
- the list focused and unfocused, with no focus ring drawn.

The existing sidebar tests keep passing. The Zed-parity E2E that measures "a
sidebar section header 31" must still pass, now against the project row. A new
`#[test]` only for a unit proven in isolation, written failure-first: the hover
card's placement and clamping is the likely one.

## §7 Left to the builder

- The hover card's placement code, and the ordering between it, menus and drags.
- Animations (fold, card fade), within what Ely's motion already does.
- How the work is split into commits.
