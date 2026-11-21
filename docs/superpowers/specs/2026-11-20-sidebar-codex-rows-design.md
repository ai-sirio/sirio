# Sidebar: project rows, one-line cards, guide line

**Date:** 2026-11-20
**Status:** approved by Enzo, 2026-11-20 21:49 (the four changes below, as drawn in
"proposta b"); the calls marked **(my call)** were made while writing this.
**Finishes:** `2026-11-17-sidebar-codex-shell-design.md` (direction D, #794). Where
the two disagree, this document wins: §3.5's project row and §3.6's two-line
card are superseded here.
**Inputs:** Enzo's screenshot of the 0.46.2-nightly sidebar, and the mockup
"Sidebar di Sirio, proposta b" (left today, right the target).

## §0 Intent

D kept Zed's banded project header and the two-line worktree card. On Enzo's
Mac that reads as a stack of grey bands with half-empty cards: the second line
says "Claude Code" or "Terminal", which the tab icon already says, and two
cards (PIG, cricchetto) show no branch at all. Enzo asked for the calm list
Codex draws: plain folder rows, one line per worktree with only the tab icons,
a name on every card, and a line down each open project like Ely's Tree.

## §1 The four changes (decided by Enzo)

**a. Project rows without the band.** No `bg(surface)`, no top or bottom
hairline. Height stays 31. The folder lines up with the New Chat row above it.

**b. One line per worktree card, icons only.** The second line (worktree
comment or last task title) goes. The tab pills, with their rings, unread
borders and status dots, move onto the title line at the right, before the
trailing seat (★ / × under the pointer, the status bloom at rest). The branch
name ellipsises first. "(missing)" is drawn in `fg_subtle` after the name. The
task titles and the worktree comment move to the hover card.

**c. Never an empty branch.** A card whose worktree has no branch name shows
the worktree folder's name in `fg_muted`.

**d. A guide line under each open project**, from the project row's bottom to
its last card's bottom, centred under the folder: Ely Tree's `guides()` recipe
(`ely-core/src/lists/tree/row.rs`), a 1 px `border_l` in `theme.ely.border`
centred in the indent column. Reproduced in Sirio, not imported: Ely's Tree has
no drag reorder, pills, menus or `tree_keys`, and its `guides` is private. No
vendored change. A folded project draws none.

## §2 Why the branch is empty (c)

`catalog_project` (`sirio/src/session.rs`) gives a **plain folder project** (no
`.git`) one synthesized primary worktree with `branch: ""`, by design
(F-SID-11): the folder has no branch. `restore_catalog` relies on the same
convention ("a plain folder's one synthesized row has no branch") when a
project's disk is gone, and `sidebar_projects_with_comments` (`main.rs`) then
writes `format!("{} (missing)", "")`, which is cricchetto's " (missing)". So it
is not a bug in how git's branch is read; the card had nothing to draw. The fix
is in the card: an empty branch draws the folder's name, muted. The E2E
reproduces both cases (§5).

The E2E found a second cause, a write bug, and it is fixed here. The live
layout labels a plain folder "main" (`head_branch_label`, for its strip), and
the layout flush (`write_layout`) wrote that label over the row's empty branch.
Once the folder was gone, `restore_catalog` read the "main" as a repository's
branch and the card said "main (missing)". The flush now replaces only a branch
the row already has, and a row it creates (only ever a plain folder) gets none:
whether a row is a repository is the catalog writer's to say, and a `git init`
made outside Sirio reaches the row with the next catalog write.
Cost: a database where an older build already wrote "main" for a plain folder
keeps it until the catalog writer next rewrites the row while the folder is
reachable.

Found while looking, **not changed**: a primary worktree on a detached HEAD
reads "main" (`catalog_project` falls back to "main" for a primary with no
branch, and to the short sha only for a linked worktree). It never shows an
empty name, and changing it moves the persisted branch and the forge's lookup
by branch, so it is reported as a follow-up, not folded in. **(my call)**

## §3 The design

### §3.1 Geometry (my calls)

| Thing | Today | Now |
|---|---|---|
| Folder icon (16 px) | x 6 | x 13: its centre (21) is the New Chat icon's centre (8 + 6 + 14/2) |
| Folder → name gap | 6 | 7: the name starts at 36, where "New Chat" starts |
| Guide line | none | x 20, 1 px, per card: the pixel left of the folder's centre line (21), as a 1 px line cannot straddle it |
| Card content (branch mark) | x 30 | x 36, under the project name, 15 px clear of the line |
| Card height | 53 | **31** at interface font 13, + the font delta (36 at 18) |

- The folder follows the New Chat row, not the "Projects" label (x 20): the
  mockup aligns icons with icons and labels with labels. **(my call)**
- **31, not 34, for the card.** The approved mockup draws the one-line card at
  the project row's 31, so the whole list is one 31 px rhythm, as in Codex.
  The 20 px pills fit with 5.5 px to spare. Cost: the card is 3 px shorter
  than Zed's single-line 34, a slightly smaller target. **(my call)**
- `ROW_PAD_X`, `ROW_PAD_Y`, `CARD_TWO_LINE_HEIGHT` and the other shared row
  constants keep their values: the Sessions view's cards still use them and do
  not change.

### §3.2 Project row

- No band, no hairlines; it sits on the sidebar's `theme.ely.bg`, and the
  name's right fade now fades into `ely.bg`.
- No hover wash is added: D had none and Enzo did not ask for one. The `+` and
  `⋯` still appear under the pointer. So the fade has one colour, rest and
  hover alike. **(my call)**
- The pinned (sticky) project row gets `bg(ely.bg)` on its wrapper: the band
  used to be what hid the cards scrolling under it.
- The layout probe no longer reaches 1 px over the hairlines: the row's own
  31 px is what the person sees.

### §3.3 Worktree card, one line

Left to right: unread dot (in the gutter, as today), branch mark, the name,
"(missing)", the pills, the trailing seat.

- **Name.** The branch in `fg` (semibold while unread, as today). An empty
  branch draws the worktree folder's name in `fg_muted` (§1c). The name keeps
  at least 40 px and ellipsises first; on a missing worktree the floor also
  holds "(missing)", measured, so the label never runs onto the pills.
- **"(missing)"** follows the name in `fg_subtle` and never ellipsises: it is
  the state, the name is the detail. (The mockup ellipsises both together.)
  **(my call)**
- **Pills.** As today's sub-line pills, the `+` included on the selected
  card. When the strip is wider than what the name leaves it, it is clipped at
  its right edge: the trailing seat never leaves the card. The `+` sits after
  the clip, so it is never the part cut. Every tab is still listed in the
  hover card. Cost: on a worktree with more tabs than fit (6 or more at the
  default width) the last pills are hidden, and the keyboard's pill cursor can
  land on a hidden pill (Enter still opens it; the strip does not scroll to
  show it). **(my call)**
- **Trailing seat** (38 px: ★ seat, 6, × seat). At rest the primary's filled
  ★ now stands in the ★ seat, and the status bloom in the × seat (as today).
  Under the pointer the same ★ / × buttons appear as today, so the primary's
  star no longer jumps when the pointer arrives. Cost: at rest the primary
  star sits 22 px further right than before. **(my call)**
- **"idle" / "N agents" go.** The approved mockup has no words on the line,
  the pills show each agent's mark and the hover card lists each tab's state.
  Cost: an idle worktree no longer says "idle". **(my call)**
- **Guide segment.** Each card draws its own segment of the line, from
  2 px above its top (the seam between rows, so the segments join and the
  first one starts at the project row's bottom) to its bottom. It is drawn
  over the card's hover and selection wash, as Ely's Tree draws its guides
  over its rows. **(my call)**
- **The seat never moves.** The mark, the name, "(missing)" and the pills sit
  in one box clipped at its right edge, before the trailing seat. At the
  narrowest sidebar (220 px) their floors can exceed the line: what gives is
  the content's right end, never the seat, and nothing covers the ★ or the ×.
  Cost: there, on a selected missing card the `+` is cut, and at interface
  font 18 the end of "(missing)" too. **(my call)**
- The card's box, its click, drag, context menu, `tree_keys` and pill keys are
  unchanged.

### §3.4 Hover card

- The title is the card's name (the folder's name for a plain folder), with
  "(missing)" in `fg_subtle` when the worktree is missing.
- The worktree comment (F-SID-11), when there is one, sits under the title in
  `fg_muted`, at most two lines.
- The tab lines are unchanged: they carry the task titles the second line used
  to show.

### §3.5 Where the branch text is used as text

The Sessions view, the bell's popover and the filter read the card's name plus
" (missing)" when missing: the Sessions view keeps saying "(missing)", the
filter still finds it, and a plain folder's sessions name the folder instead
of nothing. **(my call)**

The worktree removal flow (the × menu, its question, the upstream lookup and
`git branch -D`) used the old title as the branch, so for a missing worktree it
named "<branch> (missing)", found no upstream and the branch delete was
refused: removing a missing row has always kept its branch. It is handed that
same string now, so nothing changes there. Cost: the dialog still says the
local branch will be deleted and it is not; whether removing a missing row
should delete its branch is a follow-up, not a side effect of this change.
**(my call)**

## §4 Anti-goals (from D, still binding)

Nothing lost: the filter, drag reorder of projects and worktrees, `tree_keys`,
the project and worktree context menus, unread marks and status blooms, expand
all / collapse all, add project, the Sessions view. Nothing new persisted. Only
`theme.ely` tokens; light, dark and Onice.

## §5 Testing

End to end, framed, light and dark, with the transcript, measurements and
frames as the re-runnable artifact:

- `test-sidebar-shell-e2e.sh`:
  - the card is 31 (was 53) and its pills sit on its one line;
  - the project row draws no band: its top and bottom pixel rows and a patch
    of its right side at rest are the sidebar's background;
  - the folder's centre is the New Chat icon's centre;
  - the guide: a 1 px line at the folder's centre from the project row's
    bottom to the last card's bottom, none after it and none under a folded
    project;
  - a plain folder project's card is named after its folder (fallback), and
    after the folder is moved away and Sirio relaunched it reads missing;
  - a worktree with many tabs keeps its ★ inside the card.
- `test-zed-layout-e2e.sh`: `sidebarCard.h` 53 → 31, and 58 → 36 at
  interface font 18 (intended changes). The section row stays 31.
- The card facts the E2E needs (name, fallback, missing) are read from the
  debug-only `sidebar` object of `surface attention read`.
- No new unit tests. The unit test of the removed second line goes.
