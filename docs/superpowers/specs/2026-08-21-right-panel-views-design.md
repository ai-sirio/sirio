# Right panel: four icon-selected views

Date: 2026-08-21
Status: approved, not implemented

## Problem

The right panel is single-purpose. Its header is a fixed `"Files"` label plus
a decorative `✕`, its body is the worktree file tree, and Activity is a
collapsible footer pinned below the tree. Two things users want from a
checkout — the working-tree diff and the commit history — are either
elsewhere (the Changes tab, mounted as pane content) or absent entirely
(there is no commit history anywhere in the app).

## Solution

The header becomes a rail of four centered icons. Each selects one view that
occupies the whole panel body:

| View     | Icon              | Body                                          |
|----------|-------------------|-----------------------------------------------|
| Files    | `zed/file_tree`   | today's tree, unchanged                       |
| Activity | `zed/thread`      | today's footer rows, full height               |
| Diff     | `zed/diff`        | `ChangesTab`, mounted as-is                    |
| History  | `zed/git_graph`   | new lane-coloured commit graph                 |

The `✕` is removed. It never had an `on_click` — it was decorative, and the
panel is closed from the titlebar or `Ctrl+Shift+I`. The Activity footer is
removed as a footer; `activity_expanded` and its chevron go with it.

## Decisions

| # | Decision | Rejected alternative |
|---|----------|----------------------|
| 1 | Icons centered in the header, `✕` removed | keep `✕`; make `✕` close the current view |
| 2 | Mount `ChangesTab` unchanged in the panel | a `compact` flag; a separate narrow component |
| 3 | Clicking a commit opens a dedicated tab | read-only selection; commit detail inside the panel |
| 4 | `git log --branches` (local refs) | `HEAD` only; `--all` including remote-tracking |
| 5 | Selected view is a GPUI global | struct field; durable in `tiller_persistence` |
| 6 | Dot badge on the Activity icon | no indicator; badge plus a change count on Diff |
| 7 | Container module, views as siblings | everything in `right_panel.rs`; state in `main.rs` |

## Architecture

```
tiller_ui/src/right_panel/
    mod.rs        RightPanel: rail, PanelView, dispatch, public API
    files.rs      tree, walk, git markers, context menu      (moved)
    activity.rs   activity rows                              (moved)
    history.rs    commit graph                               (new)
```

`RightPanel` keeps its name and its entire public API — `new`,
`with_activity`, `set_activity`, `clear_worktree`, `bind_worktree`,
`refresh`. `main.rs` names `RightPanel` in 31 places and none of them
change.

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelView { #[default] Files, Activity, Diff, History }
```

The selection lives in a GPUI global (`PanelViewSetting`), mirroring
`DiffViewMode` in `changes.rs`. This is not a style choice: `select_worktree`
discards the whole `RightPanel` entity and builds a fresh one via
`with_activity`, so a struct field would silently reset on every worktree
switch — the exact failure `DiffViewMode`'s own comment documents.

`ChangesTab` and the history view are built lazily on first selection, held
in `Option<Entity<_>>`, and dropped in `clear_worktree` / `bind_worktree`
alongside `file_tree` and `git_markers`. A user who never opens History never
runs `git log`.

`ensure_tree_refresh` runs only while Files is the active view, so the other
three do not pay for the once-a-second walk.

## The icon rail

Four 28x28 buttons, `IconSize::Small`, centered in the existing 40px header.
Active: `theme.title` on `theme.row_hover`, rounded. Inactive:
`theme.subtitle`, hover as today. Each carries a `debug_selector`
(`right-panel-tab-files`, `-activity`, `-diff`, `-history`) because the
headless tests drive the app by selector.

Four new `Icon` variants resolve to SVGs taken from the commit already pinned
in `rust/assets/icons/zed/ATTRIBUTION.md` (`875e2a1c`): `file_tree.svg`,
`thread.svg`, `diff.svg`, `git_graph.svg`. No catalog refresh; the
attribution commit is unchanged.

Badge: a 6px dot at the top-right of the Activity icon, `theme.tab_needs_input`
when any row is `NeedsInput`, `theme.tab_error` when any row is `Error`,
absent otherwise. Both colours already exist in `Theme`. The data is a
`filter().count()` over the `Vec<ActivitySurface>` the host already pushes
through `set_activity` on every render.

## Diff view

`ChangesTab` mounted unchanged. `RightPanel` subscribes to the child and
re-emits its events on the channels the host already handles:

| `ChangesTab` emits | `RightPanel` re-emits | Host handler |
|---|---|---|
| `ChangesTabEvent::OpenFile` | `RightPanelEvent::OpenFile` | `main.rs:6717` |
| `ChangesTabActionEvent::OpenDiff` | `RightPanelActionEvent::OpenDiff` | `main.rs:4283` |
| `ChangesTabActionEvent::ResolveInTerminal` | `RightPanelActionEvent::ResolveInTerminal` (new variant) | `main.rs:6727` |

**Known limitation, accepted deliberately.** `ChangesTab`'s toolbar
(Unified/Split segmented control, stage-all, refresh) is designed for a wide
pane. At the panel's 405px it is cramped, and because `DiffViewMode` is an
app-wide global, choosing Split from a centre tab also makes the panel's diff
unreadable. Not worked around here.

## Git history

### Data (`tiller_git`, two new files, no new dependencies)

`log.rs`, through the existing `GitRunner`:

```rust
pub struct CommitRecord {
    pub sha: String,
    pub parents: Vec<String>,
    pub refs: Vec<String>,
    pub author: String,
    pub timestamp: i64,
    pub subject: String,
}
```

```
git log --branches --date-order --skip=N -n LIMIT \
    --format=%x1e%H%x1f%P%x1f%D%x1f%an%x1f%at%x1f%s
```

Field separator `US` (0x1f), record separator `RS` (0x1e) — not `NUL`, which
collides with `-z` and which a subject may not contain anyway. `parse_log`
is a pure function over the command's stdout, tested on fixed strings.

`--date-order` is not decoration: without an explicit order, `git log`'s
default can reorder rewritten history between chunks and make lanes jump
mid-scroll.

### Layout (`graph.rs`, pure, no gpui — testable without a window)

```rust
pub struct GraphRow {
    pub lane: usize,                     // column of the commit dot
    pub color: usize,                    // palette index
    pub through: Vec<Option<usize>>,     // colour per column passing through
    pub joins_in: Vec<(usize, usize)>,   // (source column, colour) merging in
    pub edges_out: Vec<(usize, usize)>,  // (target column, colour) to extra parents
}
pub fn layout(commits: &[CommitRecord]) -> Vec<GraphRow>
```

Standard lane assignment — a commit takes the lane already expecting its sha
or the leftmost free one, its first parent inherits that lane, extra parents
open lanes — with three refinements, each fixing a real legibility failure:

1. **Ended lanes leave holes; columns never compact.** Compacting slides live
   branches sideways on every commit, which reads as the graph dancing during
   a scroll.
2. **A new lane's colour avoids the colours of active and recently ended
   lanes.** Two adjacent same-coloured branches read as one branch.
3. **`sha -> index` is precomputed per chunk**; everything else is one
   forward pass.

Branch-head "whiskers" are deliberately not implemented — paint-only, and
nothing in the requirement needs them.

Provenance: these three refinements were arrived at by reading the *approach*
of [GitComet](https://github.com/Auto-Explore/GitComet)'s gpui commit graph,
not its code. GitComet is AGPL-3.0 and Tiller is MIT; no GitComet code is
copied, adapted, or transcribed here. The precedent that also informed this
design is Zed's own git graph (`crates/git_ui`, chunked `git log` with
`--format=%H%x00%P%x00%D`), which is GPL-3.0 and likewise not copied.

### Rendering (`right_panel/history.rs`)

`uniform_list`, fixed 26px rows. The graph column is painted with `canvas` +
`Path`: straight segments for lanes passing through, Béziers for joins.
Column width is peak active lanes x 12px, capped at 6 lanes — beyond that,
excess lanes are not drawn, because a 12-column graph in 405px is noise, not
information. Right of the graph: subject (`flex_1`, truncated), author, date.
Merge commits render in `theme.meta`.

Lane colours come from `Theme`, but as an accessor over hues the palette
already measures rather than six new palette entries:

```rust
impl Theme {
    /// Lane colour `index`, cycling. Reuses measured hues; adds no new
    /// palette entry, so the frozen provenance record stays untouched.
    pub fn graph_lane(&self, index: usize) -> Rgba
}
```

Order: `tab_focus_accent`, `git_untracked`, `tab_done`, `tab_needs_input`,
`tab_error`, `favorite`. This matters beyond taste: `tiller_theme` has
provenance tests that pin exact RGB triples for four tokens against a frozen
Swift record (`lib.rs:1625`), and every added colour is a colour someone must
measure and justify. No hardcoded colour in the renderer either way, per the
crate's existing rule.

Pagination: 500-commit chunks; reaching the end of the list fetches the next
chunk and re-runs `layout` over the accumulated list. `layout` is O(commits);
the `git log` is the slow part and runs on `background_spawn`, like the tree
walk.

### Opening a commit

Clicking a row emits `RightPanelActionEvent::OpenCommit(sha)`. `main.rs`
opens a tab holding `ChangesTab::for_commit(repo_root, sha)` — the existing
`PaneContent::Changes` variant, so no new variant and no new match arm in a
22k-line file.

Two additions to `tiller_git/src/diff.rs`:

- `commit_files(repo, sha)` — `git show --name-status --format= <sha>`
- `commit_diff_entry(repo, sha, path)` — `git show --format= <sha> -- <path>`,
  which is already the per-file patch and works on the root commit, where
  `<sha>^` does not exist.

In commit mode `ChangesTab` disables stage/unstage/discard: a commit is
immutable and those buttons have nowhere to go.

## Edge cases and errors

The no-worktree empty state is unchanged and identical for all four views;
the rail renders but builds nothing while `worktree_selected` is false.
History specifics: no `.git` → "Not a git repository"; initialised repo with
no commits → "No commits yet" (`git log` exits non-zero with `does not have
any commits yet`, which must be told apart from a real failure); failed
`git log` → error panel with Retry, in the shape of the tree's
`refresh_error`.

No error is swallowed. A failed follow-up chunk does not clear the commits
already shown; it marks pagination as interrupted and offers Retry.

`git log` is single-flight with a generation counter, like `walk_generation`:
a chunk returning after a `bind_worktree` belongs to a different repository
and is discarded, not applied late.

## Testing

Tests first, standard `#[test]`.

| Level | Coverage |
|---|---|
| `tiller_git::log` | `parse_log`: single commit, multi-parent, empty `%D`, subjects with spaces/unicode, empty output |
| `tiller_git::graph` | `layout`: linear chain is one lane; branch + merge joins on the right column; a closed lane's hole is reused; adjacent lanes never share a colour |
| `tiller_git::diff` | `commit_files` / `commit_diff_entry` against a temporary repo, root commit included |
| `tiller_ui` | view switch updates the global; badge appears only for `NeedsInput`/`Error`; activity rows still emit `SelectActivity`/`CloseActivity`; the History entity is not constructed until its icon is clicked |
| host | `OpenCommit(sha)` opens a Changes tab in commit mode |

Gate: `Scripts/ci.sh` prints `CI OK`.

## Risk

Splitting `right_panel.rs` moves ~1800 lines of existing tests into three
files. The work is mechanical, but its failure mode is silent: a test moved
without being registered in its `mod` simply stops running. Count tests
before and after and state both numbers.
