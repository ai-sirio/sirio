# Two centre panes from the first frame — design

**Date:** 2026-09-22
**Status:** implemented
**Parent work:** the centre split, #319–#325 (landed as #328,
`53ad9e6c feat: split the centre column into a Primary and a Secondary pane`)
**Reference:** Zed's workspace — an agent thread on the left, an editor pane
on the right that is there, empty, before anything is opened in it.
**Scope of this document:** make the Secondary pane a permanent part of the
layout, give both panes a real empty state, give the no-worktree state a way
to pick a worktree, and make every Secondary tab survive a restart.

## §0 Where the split stops today

The centre column has had two panes since #328, but only one of them is
visible by default. `secondary_pane_visible()` (`rust/crates/sirio/src/main.rs`)
is

```rust
self.secondary_pane_open
    && self.tabs.iter().any(|tab| tab.kind.pane_role() == PaneRole::Secondary)
```

so the Secondary half is drawn only while it holds a tab, and #320 wrote the
consequence down as an invariant: the Secondary pane "opens with the first
Secondary tab and auto-closes with the last" (`sirio_project/src/tab.rs`).
A user who has not opened a Browser, a file, a Changes view or Project
Settings never sees that the second pane exists.

Four more things fall short of "the right pane comes back the way it was
left":

| State | Today |
|---|---|
| Secondary pane, no tabs | not drawn at all |
| Primary pane, no tabs | "No Terminals or Chats", two text buttons |
| No worktree selected | "No worktree selected / Add a project, then select a worktree." — no way to select one from there |
| `secondary_pane_open` in the database | `DEFAULT 0` (v16), so every existing worktree reads as "closed" and `false` cannot tell "never opened" from "hidden on purpose" |

And restore has five holes, one of which is a plain bug:

1. **Editor tabs never come back.** `layout()` saves them as `"file"` with
   `editor_path` (#323), `restore_tabs` knows how to build them — and
   `tabs_for_worktree` (`rust/crates/sirio/src/session.rs`) admits only
   `chat | terminal | diff | browser`, logging every `"file"` record as "not
   restorable in this build". Three lists of kinds, and nothing makes them
   agree.
2. **A commit tab comes back as a generic Changes tab.** The SHA it was
   opened with (`ChangesTab::for_commit`) is never saved; `"diff"` always
   restores as `ChangesTab::new`.
3. **A Changes tab opened on a file forgets the file.** `focus_path` is
   applied at open time and never saved.
4. **Project Settings tabs are not saved at all.** `layout()` filters them
   out as "ephemeral scratch".
5. **Only one tab per layout is marked `active`.** After a restart the pane
   that did not hold focus shows its *first* tab, not the one it was
   showing.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Can a terminal live in the Secondary pane? | **No.** The rigid `TabKind::pane_role()` routing stays. Each pane's launcher offers only the surfaces that belong to it. |
| Where does "Select a worktree" go? | **Left pane**, with both panes drawn; the right one shows a faint placeholder. |
| Can the Secondary pane still be hidden? | **Yes.** Visible by default, even empty; Ctrl+Shift+B hides it and the choice is persisted per worktree. Existing worktrees reset to visible. |
| Secondary launcher contents | Browser, Changes, Open File, Project Settings. |
| Primary empty state | Restyled onto the same launcher: Terminal, Chat. |
| Restore holes to close | All five above. |
| Implementation shape | New `sirio_ui` components plus additive persistence (approach A) — not inline in `main.rs`, not a pane-entity refactor. |

## §2 Visibility

`secondary_pane_visible()` becomes `!self.secondary_pane_hidden`. It no longer
looks at tabs. With no worktree selected the pane follows the default —
visible — because there is no worktree to own a preference. Ctrl+Shift+B
does nothing while no worktree is selected.

### Persistence

- **v19** adds `worktree.secondary_pane_hidden INTEGER NOT NULL DEFAULT 0`.
  The v16 column `secondary_pane_open` stays in the table, unread: dropping
  it means a table rebuild for no behavioural gain. The v16 comment is
  updated to say it is superseded.
- `WorktreeRecord::secondary_pane_open` is replaced by `secondary_pane_hidden`.
  `SessionStore::secondary_pane_open_for` / `save_secondary_pane_open` become
  `secondary_pane_hidden_for` / `save_secondary_pane_hidden`.
- No row, a database in fallback mode, or a read error all answer `false` —
  visible. That is also what every existing worktree reads after v19, which
  is the reset the user asked for.

### Gestures

- **Ctrl+Shift+B** (`ToggleSecondaryPane`) flips `secondary_pane_hidden` and
  persists it. Hiding the focused Secondary pane moves focus to Primary, as
  now.
- **Opening any Secondary surface un-hides the pane** — Browser, file,
  Changes, commit, Project Settings, and a restore whose active tab is
  Secondary. `open_secondary_pane` keeps this job under a new body; the
  startup repair next to `persisted_secondary_pane_open` keeps its job too.
- **The `×` at the end of the Secondary strip** (`render_secondary_pane_close`)
  closes the pane: the same hide as Ctrl+Shift+B, tabs kept, drawn even over
  an empty strip and absent only with no worktree selected. The launcher's
  last tile, **Hide Pane**, does the same. All three go through
  `set_secondary_pane_hidden`. *Amended in 0.25:* as first implemented, the
  `×` closed every Secondary tab and left the pane on its launcher (#324).

### Geometry

Nothing new. `panel_layout::min_center_width(true, …)` already asks for two
320 px floors plus the divider whenever the pane is visible, and
`resolve_panel_widths` already makes the side panels yield first. The persisted
`center_split_ratio` is used as is.

### What is reversed

The #320 invariant is reversed: the Secondary pane no longer auto-closes. The
comments that state it (`PaneRole::Secondary`, `center_pane_width`,
`render_group_surfaces`) and the tests that pin it are updated. The stubbed
`drawn_detached_pane_group_offers_the_real_empty_prompt` becomes a real test
of the Secondary launcher.

### The pane-close modal

`sirio_ui::modal::render_modal` now occludes its backdrop (`.occlude()`), so
a modal blocks pointer events to what lies beneath it while its own buttons
stay clickable as its children. With the Secondary pane always drawn, its
launcher sits under the pane-close modal, and a click on a modal button used
to reach the tile beneath as well. The two close-confirmation tests
(`drawn_pane_close_prompt_is_held_for_every_status_including_idle_and_done`,
`close_anyway_removes_a_tabs_sole_pane_instead_of_silently_no_opping`) are
the regression tests for it.

## §3 Empty states

### `sirio_ui::pane_launcher`

A pure render function, not an entity:

```rust
pub struct LauncherItem<A> {
    pub id: &'static str,              // debug selector, verbatim
    pub action: A,
    pub icon: Icon,
    pub label: SharedString,
    pub shortcut: Option<SharedString>, // from `window_shortcut_hint`
    pub disabled: Option<SharedString>, // the reason, shown as a tooltip
}

pub fn pane_launcher<A: Copy + 'static>(
    container_id: &'static str,
    items: &[LauncherItem<A>],
    theme: Theme,
    on_click: impl Fn(A, &mut Window, &mut App) + 'static,
) -> AnyElement;
```

`id` is the debug selector **verbatim** — the Primary tiles keep
`empty-worktree-new-terminal` / `empty-worktree-new-chat` — and the host's
`A` is `LauncherAction`, matched exhaustively.

It draws one row of tiles, centred in the pane, wrapping when the pane is
narrow. bezel has no launcher tile: `option_card` is a fixed 148 px preview frame and
`row_tile` a 36 px identity mark. The tile is therefore drawn from bezel's
tokens — `panel_radius`, `border`, `ink(0.03)` — the way
`settings::agents_page` draws its identity tile: a 64 px square with the icon,
the label under it, the shortcut under that in faint text. A disabled tile is dimmed, never calls `on_click`, and
explains itself through a bezel tooltip. There is no headline and no orbit
mark — the tiles are the whole state, as in Zed.

`shortcut` is read from `window_shortcut_hint`, never written out again, so
the #374 Windows chords stay right automatically.

### Primary, worktree selected, no tabs

| Tile | Action |
|---|---|
| Terminal (`Icon::SquareTerminal`, Ctrl+T) | `add_terminal_tab` |
| Chat (`Icon::MessageSquare`) | opens the existing agent picker (`empty-chat-agent-menu`), anchored under the tile |

The debug selectors current tests use (`empty-worktree`,
`empty-worktree-new-terminal`, `empty-worktree-new-chat`,
`empty-chat-agent-menu`, `empty-chat-agent-<id>`, `empty-chat-other-agents`)
are kept wherever the element survives, so no test changes without a reason.

### Secondary, worktree selected, no tabs

| Tile | Action |
|---|---|
| Browser (`Icon::Globe`, Ctrl+Shift+L) | `open_action(NewTabAction::NewBrowser, …)` — exactly what `handle_new_browser` calls |
| Changes (`Icon::Diff`) | `add_changes_tab(None)`; **disabled** when the worktree is not a git repository |
| Open File (`Icon::File`, Ctrl+O) | dispatches `OpenFile` — the existing native dialog in `handle_open_file`; a cancelled dialog does nothing |
| Project Settings (`Icon::Settings`) | `add_project_settings_tab` for the project the current worktree belongs to |
| Hide Pane (`Icon::Close`, Ctrl+Shift+B) | `set_secondary_pane_hidden(true)`, the strip's `×` as a tile (added in 0.25) |

### No worktree selected

- **Left pane:** "Select a worktree" over a `WorktreePicker` (§4). With an
   empty catalog the picker is replaced by a *No projects yet* headline over an
   **Add Project** button that
  starts the sidebar's existing add-project flow. That flow's entry,
  `Sidebar::start_open_project`, is private today; it gets a public entry
  rather than a copy, so the folder picker, the not-a-git-repository prompt
  (F-PRJ-03) and `SidebarEvent::AddProject` stay one flow.
- **Right pane:** a faint "No worktree" placeholder and no tiles — nothing
  can be opened without a worktree.
- Both strips are drawn empty. The `+` keeps its current behaviour.

### Language

The app's strings are English throughout, so these are too: *Select a
worktree*, *No worktree*, *Terminal*, *Chat*, *Browser*, *Changes*, *Open
File*, *Project Settings*, *Add Project*.

## §4 `sirio_ui::worktree_picker`

An entity wrapping bezel's `Combobox` (`bezel::ui::combobox`), which is an
entity because it owns its query field.

```rust
pub struct WorktreeChoice {
    pub project: SharedString,
    pub branch: SharedString,
    pub path: PathBuf,
}

pub enum WorktreePickerEvent {
    Selected(PathBuf),
}

impl WorktreePicker {
    pub fn new(choices: Vec<WorktreeChoice>, cx: &mut Context<Self>) -> Self;
    pub fn set_choices(&mut self, choices: Vec<WorktreeChoice>, cx: &mut Context<Self>);
}
```

- One item per worktree, labelled `project / branch`, in sidebar order. The
  combobox searches them.
- `ComboboxEvent::Selected(index)` is an index into the **original** list,
  never the filtered one (bezel's contract), so the picker maps index → path
  directly.
- `main.rs` subscribes and routes `Selected(path)` through **the same
  selection path a sidebar click takes** — there is no second way to select
  a worktree.
- The host calls `set_choices` whenever the catalog changes. If the chosen
  worktree disappeared between listing and click, the selection is ignored
  and the list refreshed.
- Nothing calls `bezel::ui::combobox::init` today. It is added beside the
  existing `bezel::ui::input::init` call at startup, and to the test
  fixtures that build a workspace.

## §5 Restore

### One map of kinds

`TabKind ↔ &str` becomes one exhaustive pair of functions — no `_` arm —
beside the existing string-to-kind mapping in `main.rs`. `layout()` writes
with it, `tabs_for_worktree` derives its accepted set from it, and
`restore_tabs` / `restore_tabs_in_workspace` build every kind it names. A
round-trip test over every `TabKind` fails the day a kind is saved but not
loaded. That closes hole 1 by construction; it is the same "one list, two
independent answers" failure CLAUDE.md records for languages, and the same
remedy.

### New `SessionTabState` fields

All `#[serde(default)]`, so there is no schema migration, and all read live in
`layout()` next to the existing arms (scrollback, chat draft, browser URL,
editor path):

| Field | Captured from | Restored as |
|---|---|---|
| `shown_in_pane: bool` | `center_split.active(role) == Some(tab.id)` | each pane is seeded back to the tab it showed; the existing `active` flag only decides which pane holds focus |
| `commit_sha: String` | new `ChangesTab::commit() -> Option<&str>`, from `ChangesSource::Commit` | `ChangesTab::for_commit`; a SHA that no longer resolves shows the git error `ChangesTab` already draws |
| `changes_focus: String` | new getter for the file last opened through `ChangesTab::focus_path` or a row expansion in the tab; collapsing that file's last expanded row clears it | `focus_path(path)`, which already defers when the first snapshot has not landed |
| `settings_project_id: String` | `ProjectSettingsView::project_id()` | built after the free function returns, with `&mut self` and the sidebar's `project_settings_seed`, at its persisted strip position — the same after-pass `bind_file_tabs` is for; a project that no longer exists drops the tab silently, like an editor tab whose file is gone |

`layout()` stops filtering Project Settings tabs out. Their content is already
persisted on every edit; only the tab comes back.

### Both paths

Launch (`session::restore`) and worktree switch (`restore_tabs_for`) both go
through `tabs_for_worktree`, so every fix above applies to both, and to the
sidebar's parked rows (`persisted_tabs_for`).

### Old sessions

Missing fields take their defaults and behave exactly as today: a generic
Changes tab, each pane on its first tab. Nothing is rewritten or lost.

## §6 Failure behaviour

Everything degrades toward the safe answer:

- database in fallback mode → the pane reads as visible;
- worktree gone between menu and click → ignored, list refreshed;
- Open File dialog cancelled → nothing happens;
- file, commit or project gone at restore → as in §5.

## §7 Tests

Written before the code.

| Where | What it pins |
|---|---|
| `sirio_persistence` | v19 on a database holding `secondary_pane_open` 0 and 1 leaves `secondary_pane_hidden = 0` everywhere; `secondary_pane_hidden` round-trips |
| `sirio` `session.rs` | the four new fields round-trip; JSON written before them decodes to defaults; the kind map is exhaustive and round-trips; `tabs_for_worktree` keeps `file` and `settings` |
| `sirio_ui::pane_launcher` | one tile per item (each tile's debug selector is its item's `id`, verbatim); a disabled item never calls `on_click` |
| `sirio_ui::worktree_picker` | `Selected(index)` maps to the right path (bezel reports the index into the original list; the test drives the combobox's event directly, since its rows carry no debug selector); an empty list does not panic |
| `sirio` `main.rs` (gpui, `VisualTestContext`) | Primary-only worktree draws `pane-secondary` and its launcher; Ctrl+Shift+B hides it and a reload keeps it hidden; `×` closes the tabs and leaves the launcher; no worktree draws the picker left and the placeholder right; picking from the picker selects the worktree; Changes is disabled outside git; after a simulated restart the Editor tab, the commit, the focused Changes file, the Project Settings tab and each pane's shown tab are all back |

Tests that pin the #320 auto-close are updated, not deleted.

Verification: iterate with `cargo nextest run -p <crate>` (plain `cargo test
-p sirio` shows four process-sharing false failures). The `WorktreeRecord`
field rename breaks struct literals in other crates' test targets, so
`cargo build --workspace --all-targets` must pass before the work is called
done. `Scripts/ci.sh` runs only on the user's request.

## §8 Deliberately absent

- **Terminals or chats in the Secondary pane.** The rigid routing stays;
  relaxing it needs a per-tab pane field and a decision about the sidebar,
  which lists exactly the Primary kinds.
- **A pane entity** (`CenterPane` owning strip, surface and empty state).
  Cleaner, and closer to Zed, but it re-cuts all of #319–#325; its own
  project.
- **Dropping `secondary_pane_open`.** Left unread rather than rebuilt away.
- **A launcher in the no-worktree right pane.** Nothing to launch without a
  worktree.
