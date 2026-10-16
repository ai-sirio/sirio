# The Secondary pane on demand, and the Open list

**Date:** 2026-10-15
**Status:** proposed
**Supersedes:** #323's "the pane is part of the layout from the first frame,
and an empty one shows its launcher", and the "Secondary launcher" of
`2026-09-24-movable-conversation-tabs-design.md` §5. The rest of that spec
stands: `move_tab_to_pane` as the one writer of a tab's half, Ctrl+Shift+M,
the drop on the other pane's body.

## §0 Intent

Today the centre is split in two from the first frame. An empty Secondary pane
takes half the centre to show a row of 64px tiles (Terminal, Chat, Browser,
Changes, Open File, Project Settings, Hide Pane). The user wants:

1. **The Secondary pane not open at launch.** The centre starts as a single
   pane.
2. **Those actions in the right sidebar instead.** A new button beside Files,
   Changes, History, References and Change Requests shows them as a vertical
   list of full-width, compact rows.

Decided with the user, in order:

1. **The Secondary pane exists only while it holds tabs.** It appears when
   something opens in it and disappears when its last tab closes. The tile
   launcher goes away.
2. **Chat expands inline.** Its row opens the available agents as indented
   rows beneath it; a click on one opens that chat in the Secondary pane.
3. **Dragging a tab right still works.** While the Secondary pane is not
   drawn, dragging a movable Primary tab shows a temporary drop zone where the
   pane would be.

The Primary pane's empty launcher (Terminal, Chat) is unchanged.

## §1 When the Secondary pane is drawn

`SirioWorkspace::secondary_pane_visible()` becomes:

> drawn ⇔ a worktree is selected **and** at least one tab has
> `pane == PaneRole::Secondary` **and** `!secondary_pane_hidden`

`resolve_center_split`, `min_center_width` and the centre column already read
this predicate, so the layout follows with no change of its own.

Consequences:

- **Launch.** A worktree with no Secondary tabs draws the Primary pane alone,
  at full width. Restored Secondary tabs bring the pane back, unless the user
  had hidden it. The existing rule that a restored *active* Secondary tab
  clears the hidden flag (`main.rs`, the `restored_active_secondary` branch)
  stays.
- **Last tab closed.** The pane disappears. `sync_center_split` already moves
  focus to Primary when the Secondary half has no tabs; nothing new is needed
  there.
- **Opening a Secondary surface** shows the pane through the existing path:
  `move_tab_to_pane` / `open_secondary_pane` clear the hidden flag, and the
  new tab satisfies the predicate.
- **Ctrl+Shift+B** (`ToggleSecondaryPane`) hides or shows a Secondary pane
  that holds tabs. With no Secondary tabs it does nothing, and it still
  consumes the chord: `ctrl-shift-` chords are Sirio's even inside a
  terminal (`YIELDS_TO_TERMINAL`), and with no Secondary tab being the
  common state, propagating would send Ctrl+Shift+B to a focused terminal's
  PTY. It must not set `secondary_pane_hidden = true` invisibly, which would
  swallow the next open.
- **The strip's `×`** keeps hiding the pane with its tabs, as today.
- **No worktree.** The pane is not drawn. The `secondary-no-worktree`
  placeholder goes.

**Persistence: no migration.** `worktree.secondary_pane_hidden` (v19) keeps its
column and its default. Its meaning narrows to "the user hid a pane that holds
tabs". An existing `0` row behaves correctly under the new rule, since an empty
pane is not drawn whatever the flag says.

**Removed:**

- `secondary_launcher_items`, the `secondary-empty` branch of
  `render_group_surfaces`, and the `secondary-launcher` element;
- `LauncherAction::HidePane` and the `launcher-hide-pane` tile;
- the `PaneRole::Secondary` arm of `empty_chat_picker_open` and of
  `render_empty_chat_picker` (the `secondary-empty-chat-` selectors). The
  picker becomes Primary-only, so `empty_chat_picker_open` narrows from
  `Option<PaneRole>` to a `bool` and `render_empty_chat_picker` loses its
  `target` parameter.

## §2 The Open list in the right sidebar

### The button

A sixth view, `PanelView::Open`, last in `PanelView::ORDER` (after
`ChangeRequests`):

| | |
|---|---|
| icon | `Icon::SquarePlus` |
| element id | `right-panel-tab-open` |
| shown | always, in git and non-git projects alike |

It is drawn like the other five (28px square, `theme.ely.hover` when active).
The selected view stays in the `PanelViewSetting` global, so it survives a
worktree switch like the others. With no worktree selected, the panel shows
`render_no_worktree` for this view too, as for every other.

`PanelView` is a `pub enum`; outside `sirio_ui` it is only used through
`PanelView::get`/`set`, never matched, so the new variant breaks no other
crate. The plan still builds `--workspace --all-targets` once.

### The list

A new module, `sirio_ui/src/right_panel/open_list.rs`, renders:

```
 OPEN IN RIGHT PANE              ← caption2, fg_subtle
 ⊞  Terminal
 💬 Chat                     ▾
      ✳ Claude Code             ← indented agent rows, only while expanded
      ◎ Codex
      Other agents…             ← opens Settings → Agents
 🌐 Browser
 ±  Changes                     ← disabled when the worktree is not git
 📄 Open File
 ⚙  Project Settings            ← disabled when there is no project
```

- Each row spans the panel's width, about the height of a Files tree row
  (~26px), with an `IconSize::Small` icon and the panel's sidebar typography.
  Hover is `theme.ely.hover`.
- A disabled row is drawn at 50% opacity, does not react to a click, and
  shows its reason in a tooltip — the same reasons and wording as the tiles
  today ("This worktree is not a git repository", "This worktree belongs to
  no project").
- Chat carries a chevron (`ChevronRight` collapsed, `ChevronDown` expanded).
  An agent row uses `Icon::for_agent_id(id)`, falling back to
  `Icon::MessageSquare`. With no agent available, the expansion shows only
  "Other agents…" with the line "No supported agent found on PATH", as the
  picker does today.
- Rows are clickable, so they use no `selectable_text`.
- Each row has its own element id and debug selector:
  `open-list-terminal`, `open-list-chat`, `open-list-agent-<id>`,
  `open-list-other-agents`, `open-list-browser`, `open-list-changes`,
  `open-list-open-file`, `open-list-project-settings`.
- Whether Chat is expanded is a plain field of `RightPanel`. It collapses on a
  worktree switch, because `select_worktree` builds a new `RightPanel`.

### The seam between `sirio_ui` and `sirio`

`RightPanel` cannot see `LauncherAction`, the agent catalogue or the project
catalogue; all three live in `sirio`. Two types cross the boundary, both in
`sirio_ui::right_panel`:

```rust
/// What a row of the Open list asks the host to open in the Secondary pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenSurface {
    Terminal,
    /// An adapter id from `sirio_agents::ALL`.
    Chat(&'static str),
    OtherAgents,
    Browser,
    Changes,
    OpenFile,
    ProjectSettings,
}

/// What only the host knows, pushed in every render.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenListFacts {
    /// Agents a chat can open with today, as (adapter id, display name).
    pub agents: Vec<(&'static str, SharedString)>,
    pub changes_disabled: Option<SharedString>,
    pub project_settings_disabled: Option<SharedString>,
}
```

- **Out:** a click emits `RightPanelActionEvent::OpenSurface(OpenSurface)`.
- **In:** the host calls `RightPanel::set_open_list_facts(facts, cx)` from its
  own render, the way it pushes `panel_width`. The setter calls `cx.notify()`
  only when the facts changed, so it cannot cause a render loop.

The agent list is computed by the same filter the picker uses today
(`AGENT_CATALOG` filtered by `agent_command_for(&launch_source_for(id))`),
moved into one function both callers share.

### In the host

`subscribe_right_panel` maps each `OpenSurface` to a path that already exists.
Nothing is implemented twice:

| `OpenSurface` | Existing path |
|---|---|
| `Terminal` | `handle_launcher_action(NewTerminal(Secondary))` → `open_in_pane(Secondary, NewTerminal)` |
| `Chat(id)` | `open_in_pane(Secondary, open_chat_agent(id))` |
| `OtherAgents` | `open_settings(Some(SettingsCategory::Agents))` |
| `Browser` | `handle_launcher_action(NewBrowser)` |
| `Changes` | `handle_launcher_action(Changes)` |
| `OpenFile` | `handle_launcher_action(OpenFile)` |
| `ProjectSettings` | `handle_launcher_action(ProjectSettings)` |

`OpenFile` behaves exactly as the tile does today: wherever `handle_open_file`
puts the file. Opening any Secondary surface shows the pane, by §1.

## §3 The drop zone while the pane is not drawn

While `pane_drop_overlay_role(cx) == Some(PaneRole::Secondary)` and the
Secondary pane is **not** drawn (empty, or hidden with tabs), the Primary
surface draws, last among its children, an absolute overlay:

- **Selector and handlers:** the same as the real overlay —
  `pane-drop-overlay-secondary`, `on_drag_move` → `drag_over_pane(drag,
  Secondary, None, inside)`, `on_drop` → `drop_on_pane(drag, Secondary, …)`.
  `move_tab_to_pane` stays the one writer of a tab's half.
- **Geometry:** anchored to the right edge of the Primary surface, as wide as
  the Secondary pane will be after the drop — `resolve_center_split` evaluated
  as if the pane were visible, from the current `center_split_ratio`.
- **Look:** clear until targeted, then `theme.ely.active` with a
  `theme.sirio.border_strong` border, like the real overlay, plus a centred
  label "Move to right pane".
- **Layout:** nothing moves during the drag. On drop, `move_tab_to_pane`
  clears the hidden flag and the moved tab makes the pane drawn.

Drawn last, it is the topmost hitbox, so a terminal or chat under it never
takes the drop. `overlay_obscures_browsers` already unmaps native browser
surfaces while `pane_drop_overlay_role == Secondary`; this case is covered.

## §4 Testing

The project's rules apply: every way the unit can fail is written before the
code, tests run against the real `SirioWorkspace`, and the feature ends in an
artifact anyone can re-run.

### In-process tests on the real shell (`sirio`), written first

Real clicks (`simulate_click` on debug bounds), real drags (two moves, see the
gpui drag notes), real key bindings:

1. A worktree with no Secondary tabs draws neither the Secondary pane nor
   `center-divider`, and Primary takes the centre's full width.
2. Clicking `right-panel-tab-open` then `open-list-terminal` puts a terminal
   tab in the Secondary half and draws the pane.
3. `open-list-chat` expands; clicking `open-list-agent-<id>` opens that chat in
   the Secondary half. A disabled `open-list-changes` emits nothing.
4. Closing the last Secondary tab removes the pane and focuses Primary.
5. Ctrl+Shift+B with no Secondary tabs leaves `secondary_pane_hidden` and its
   persisted value unchanged; the next open shows the pane.
6. With the pane not drawn, dragging a Primary tab onto
   `pane-drop-overlay-secondary` moves it there and draws the pane.

### `sirio_ui`

`open_list.rs` gets one harness test, in the shape of `pane_launcher`'s: every
row draws under its id, a disabled row reports no click, Chat's expansion
reveals one row per agent plus "Other agents…".

### Tests that change

The existing tests that encode "drawn from the first frame" and the Secondary
launcher (in `sirio/src/main.rs`, around the assertions on
`secondary_pane_visible()` and the `launcher-*` / `secondary-empty-chat-*`
selectors) are **rewritten** to the rule in §1, not deleted. Tests of the Hide
Pane tile go with the tile.

### End to end

`Scripts/Tests/test-open-list-e2e.sh` drives an isolated debug Sirio on a
private Xvfb (Linux); `--state-only` runs without captures (macOS). Two
control verbs are added:

- `surface.layout.read` (every build, read-only): `secondaryVisible`,
  `secondaryHidden`, `primaryTabs`, `secondaryTabs`, `secondaryKinds`, and
  `secondaryPositions` (the 1-based positions in `surface tabs read`'s list,
  so a script can close a Secondary tab), then `rightPanelView`,
  `openListChatExpanded` and `openListAgents`.
- `surface.open_list.open <entry>` (debug builds only, like
  `surface change-request act`): calls the handler a row's click calls.

The script checks, in order:

1. at launch the Secondary pane is not drawn;
2. opening a terminal draws the pane with it, and Chat's expansion with no
   agent on PATH says so (agent chats are proven in-process);
3. closing that terminal removes the pane;
4. after quit and relaunch, a Secondary pane that held a tab comes back and an
   empty one does not;
5. an unknown `agent:<id>` is refused.

It ends with `OPEN LIST E2E OK`. Under `--out-dir` it leaves the socket
transcript and PID-matched captures: the list collapsed, Chat expanded, the
centre with and without the pane, in light and dark.

## §5 Out of scope

- Keyboard shortcuts on the list rows (no tile has one today).
- Moving the Primary pane's empty launcher anywhere.
- Any change to where Ctrl+T, the `+` menu, the sidebar or `sirioctl` open
  their surfaces.
