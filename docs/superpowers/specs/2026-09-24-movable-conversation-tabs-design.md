# Terminals and chats in either centre pane — design

**Date:** 2026-09-24
**Status:** approved in chat; implementation pending
**Reverses:** `2026-09-22-center-panes-design.md` §1, "Can a terminal live in
the Secondary pane? **No.**", and the first bullet of its §8.
**Scope of this document:** let a terminal or chat tab live in either half of
the centre split, open one directly in the Secondary half, and move one from
half to half by drag, context menu or shortcut — without restarting what runs
in it, and with its half surviving a restart.

## §0 Intent

The user wants two things they talk to side by side — two agents at once, or
a chat next to its terminal — using the split that already exists. Success:
drag a terminal running an agent from the left strip to the right and back;
the agent notices nothing beyond a resize, the activity mark stays right, and
after a restart each tab is back in the half it was left in.

## §1 Decisions taken

Settled with the user before this document was written:

| Question | Decision |
|---|---|
| Which kinds can move? | **Terminal and chat only.** Browser, Editor, Diff and Project Settings stay in the Secondary half. |
| Where does a new terminal or chat open? | **Left, as today** — Ctrl+T, the `+` menu, the sidebar and `sirioctl` are unchanged. The Secondary launcher gains Terminal and Chat tiles that open on the right. |
| Which gestures move a tab? | All four: **drop on the other strip** (at a position), **drop on the other pane's body** (at the end), a **context-menu item**, and a **shortcut**. |
| Implementation shape | **A stored `pane` field per tab** (approach 1). Not a side map of exceptions (two sources of truth), not a `CenterPane` entity refactor (its own project, per the 22/09 spec §8). |

## §2 Where it stops today

A tab's half is *derived*, never stored: `TabKind::pane_role()`
(`sirio_project/src/tab.rs`) maps Terminal and AgentChat to Primary and
everything else to Secondary, and 21 call sites in `sirio/src/main.rs` read
it. Three other facts are built on it:

- `TabKind::appears_in_sidebar()` is `pane_role() == Primary`, so a terminal
  drawn on the right would silently leave the sidebar while still running.
- `reorder_tabs_by_id` refuses a drag between tabs of different roles:
  `self.tabs` is **one list for both strips**, each strip's order being the
  relative order of its own role's tabs, and a cross-role reorder would
  interleave them.
- `tab_context_items` carries the comment "no pane-move; routing is derived
  from TabKind" where the Move-to-Pane family used to be (#325).

What already works in our favour:

- `render_pane_tree` takes a tab index, not a role; any tab renders in either
  half.
- A `TerminalView` entity survives being drawn somewhere else — "Attach to
  Current Terminal" (`attach_tab_to_current_terminal`) already moves one
  between tabs without restarting its PTY.
- `terminal_pane_cache` placements are already spelled
  `"primary-pane-N"` / `"secondary-pane-N"`, a leftover from the N-group days,
  and `move_within_worktree` already re-places a cached view.

## §3 Model

### `TabKind`

`pane_role()` is replaced by three facts, each an exhaustive `match` with no
`_` arm:

```rust
/// The half a new tab of this kind opens in.
pub fn default_pane(self) -> PaneRole;      // Terminal | AgentChat -> Primary, rest -> Secondary
/// Whether a tab of this kind may be moved to the other half.
pub fn can_move_between_panes(self) -> bool; // Terminal | AgentChat
/// Whether a tab of this kind is listed under its worktree in the sidebar.
pub fn appears_in_sidebar(self) -> bool;     // Terminal | AgentChat
```

`appears_in_sidebar` stops being derived from the pane: the sidebar lists the
work a worktree contains, and that is a fact about the kind, wherever the tab
is drawn. The doc comments on `PaneRole`, `default_pane` and
`appears_in_sidebar` that state the old rigidity are rewritten.

### `OpenTab.pane`

`OpenTab` gains `pane: PaneRole`, with one invariant: **a kind that cannot
move is always in its `default_pane()`**. Every `OpenTab { … }` literal (26
today) initialises it with `kind.default_pane()`; restore goes through
`placement_for` (§6), which enforces the invariant on persisted input.
`SplitTab::split_role()` returns `self.pane`, so `CenterSplit` needs no new
logic.

### One writer

```rust
fn move_tab_to_pane(
    &mut self,
    tab_id: usize,
    target: PaneRole,
    anchor: Option<(usize, bool)>, // (target tab id, insert before it)
    window: Option<&mut Window>,
    cx: &mut Context<Self>,
) -> bool;
```

The only way `pane` changes after construction. Every gesture in §5 calls it.

1. Returns `false` and does nothing for a kind that cannot move, or a tab
   already in `target`.
2. Reads the tab's position in its source strip *before* moving it, and hands
   the source half to the **nearest remaining tab** — the #319 rule
   `close_tab` applies. That rule is extracted into one helper both call, so
   closing and moving cannot pick different neighbours.
3. Removes the tab from `self.tabs` and re-inserts it before/after `anchor`,
   or after the target strip's last tab when `anchor` is `None` (at the end of
   `self.tabs` when the target strip is empty). Sets `pane = target`, then
   `rebuild_center_split` and `select_tab`: the target half takes focus and
   shows the moved tab.
4. A Secondary target that is hidden is revealed (`open_secondary_pane`) —
   the existing rule that opening a Secondary surface un-hides the pane.
5. Updates the tab's `terminal_pane_cache` placements, refocuses the moved
   surface, ends an in-progress rename the way activating another tab ends
   it, and calls `schedule_save` and `mark_activity_dirty`.

### The 21 call sites

Every `tab.kind.pane_role()` in `main.rs` asks "which half is this tab in?"
and becomes `tab.pane`. `reorder_tabs_by_id` keeps its guard against
interleaving the strips, comparing `pane` instead of kind. The Project
Settings test that asserts its role reads `default_pane()`.

### The resize

The two halves have different widths, so a moved terminal is resized once
and its agent receives a SIGWINCH and redraws — the same thing a divider drag
does. That is why a cross-half move **commits on drop only** and is never
previewed live the way an in-strip reorder is: a live preview would resize
the PTY every time the pointer crossed between strips.

## §4 Glossary

`CONTEXT.md` "Primary pane role" / "Secondary pane role" are rewritten: the
left and right halves of the centre split; every kind has a home half; a
terminal or chat may live in either. The stale "Absent by default … gone
again when the last of them closes" sentence (already untrue since #323) goes
with it. The 22/09 spec gets an *Amended* note on its §1 row and §8 bullet,
the way its `×` row was amended in 0.25.

## §5 Gestures

### Drag state

The payload stays `RowDrag { scope: ReorderScope::Tabs, id, group: None }`;
the source half is read from `tab.pane`. A new field

```rust
pane_drop_target: Option<PaneDropTarget>, // { pane: PaneRole, anchor: Option<(usize, bool)> }
```

says where the tab would land if released now. It is computed in
`on_drag_move` and **never mutates `tabs`**.

In the pinned `bezel-gpui` (0.3.8), `on_drag_move` is not hover-gated: every
element that registered it receives *every* pointer move of a drag of that
type, in the capture phase, with its own `bounds` and no containment check
(`Interactivity::on_drag_move`, `elements/div.rs`). The in-strip preview gets
away without a check because re-placing the dragged tab before or after
every tab in turn converges on the pointer's slot; a drop target cannot,
because it must also become `None` when the pointer is over no target. So
the target is **recomputed on every move**: one handler on the centre-split
container resets it to `None`, and each strip tab and body overlay sets it
only when `event.bounds` contains the pointer. The reset runs first because
a div registers its mouse listeners before painting its children
(`paint_mouse_listeners` precedes the children closure in
`Interactivity::paint`) and the capture phase walks listeners in
registration order (`Window::dispatch_mouse_event`). A test drags over the sidebar
and asserts no indicator is drawn. The indicator and
overlay are drawn only while a tab drag is in flight. The pinned gpui has no
typed accessor for the active drag (`App::active_drag` is `pub(crate)`; only
`has_active_drag()` is public), so "a tab drag" is `tab_drag_snapshot`
(which gains `dragged: usize`, the tab being dragged) together with
`cx.has_active_drag()`. A drag that ends without a drop (release outside the
window — the case `schedule_panel_width_save` documents) would leave both the
snapshot and the target behind, and a later divider drag would then read as a
tab drag; so `render` clears both whenever no drag is active. That is sound
because the snapshot exists only for Escape *during* a drag.
`cancel_tab_drag` clears the target too.

### Drop on the other strip

The per-tab `on_drag_move::<RowDrag>` splits in two:

- dragged tab in the **same** half → `preview_tab_reorder`, unchanged;
- dragged tab in the **other** half and movable → `pane_drop_target =
  { pane: this half, anchor: Some((id, before)) }`; the strip draws a 2 px
  vertical insertion bar in `theme.text` — the colour of the focused tab's
  underline — on that side of the tab (`tab-drop-indicator-<id>`). Not
  `theme.accent`: that token is the quantity blue of meters and progress
  bars.

The strip container's existing `on_drop::<RowDrag>` calls
`move_tab_to_pane` when a target is set, then clears the snapshot and the
target as today. An empty strip accepts the drop as "at the end".

The in-strip live preview only ever moves the dragged tab, so the other
tabs' relative order is intact when the pointer crosses to the other half;
`move_tab_to_pane` re-places the dragged tab regardless, and nothing needs
restoring.

### Drop on the other pane's body

While a movable tab is being dragged, the *other* half's surface is covered
by an absolutely positioned drop overlay (`pane-drop-overlay-<half>`) that
sets `pane_drop_target = { pane, anchor: None }` and, on drop, moves the tab
to the end of that strip. It is transparent until it is the target, then
filled with `theme.element_active` and outlined in `theme.border_strong`.
This includes an empty half showing its launcher.

A Browser is a native child window above GPUI's surface (#376), so it would
paint over the overlay and swallow the release. `overlay_obscures_browsers`
gains a `tab_drag_toward_secondary` parameter — true exactly when the overlay
is drawn over the Secondary half, i.e. a movable tab is being dragged out of
Primary — fed in `sync_browser_overlay_obscured`; the page is unmapped for
that drag and mapped again after it. A drag of a Secondary tab never blanks
the page.

### Refused silently

A kind that cannot move, dragged towards the other half, shows no indicator
and its drop does nothing. A drop on the body of the tab's own half does
nothing. No toast: the missing indicator is the signal.

### Context menu

"Move to Right Pane" on a tab in Primary, "Move to Left Pane" on a tab in
Secondary — `TabContextAction::MoveToOtherPane` — in the Move Earlier / Move
Later group, where the Move-to-Pane family was. The item is **absent**, not
disabled, for kinds that cannot move. The rule: *disabled* means "not now"
(Attach to Current Terminal, which depends on the active tab); *absent* means
"never, for this kind".

### Shortcut

`Ctrl+Shift+M` → a new `MoveTabToOtherPane` action and `WindowCommand`,
bound globally like Ctrl+Shift+B — the `ctrl-shift-` family is the one meant
to survive inside a terminal (`YIELDS_TO_TERMINAL`). It moves the focused
half's active tab, and focus follows it, so pressing it twice brings the tab
back. On a kind that cannot move it calls `cx.propagate()` (#227's lesson: a
matched binding otherwise eats the key). It gets a row in
`linux_window_shortcuts` (array 9 → 10), in `window_shortcut_hint`, in
`window_command_availability` (disabled with no movable active tab) and in the
command palette. On Windows the chord must be probed for delivery the way
#374 probed S/I/O (`RegisterHotKey`); if it does not arrive, the Windows arm
slides it to a free letter.

### Secondary launcher

Becomes Terminal, Chat, Browser, Changes, Open File, Project Settings, Hide
Pane.

- `LauncherAction::NewTerminal` and `NewChat` carry the target half:
  `NewTerminal(PaneRole)`, `NewChat(PaneRole)` (`PaneRole` is `Copy`). The
  Primary launcher passes `Primary`, the Secondary one `Secondary`.
- The new tiles are `launcher-terminal` and `launcher-chat`, following the
  Secondary launcher's own `launcher-` prefix. The Primary tiles keep
  `empty-worktree-new-terminal` / `empty-worktree-new-chat`.
- The Secondary Terminal tile shows **no** shortcut: Ctrl+T opens on the
  left, and showing it there would be false.
- The Chat tile opens the same agent picker, anchored under the right-hand
  tile: `empty_chat_picker_open: bool` becomes `Option<PaneRole>`. The right
  picker's selector is `secondary-empty-chat-agent-menu`; the left keeps
  `empty-chat-agent-menu` and its `empty-chat-agent-<id>` rows.
- The creation paths are untouched: a Secondary tile opens its tab through
  the same `open_action` / `open_chat_agent` as everywhere else — so in its
  home half — and then calls `move_tab_to_pane(new_id, Secondary, None, …)`
  in the same update (`open_in_pane`). No frame is drawn in between, so the
  PTY is sized once, in the right half, and `move_tab_to_pane` stays the only
  writer of `pane`. A creation that opens nothing (a chat refused with a
  toast) moves nothing. Ctrl+T, `+`, the sidebar, Resume Chat and `sirioctl`
  behave exactly as today.

## §6 Persistence and restore

### One field, no migration

`SessionTabState` (`sirio/src/session.rs`) gains

```rust
#[serde(default)]
pub pane: String, // "primary" | "secondary" | "" (the kind's default)
```

A `String` rather than a serde enum, like `browser_url`, `commit_sha` and
`changes_focus`: an unknown enum value would fail to decode the *whole* tab
state, while an unknown string degrades to the default.

- **Written** by `layout()`, beside `shown_in_pane`, which itself becomes
  `center_split.active(tab.pane) == Some(tab.id)`.
- **Read** through one pure function,
  `placement_for(kind: TabKind, persisted: &str) -> PaneRole`: `""` or an
  unknown spelling → `kind.default_pane()`; a kind that cannot move →
  `kind.default_pane()` whatever the file says; otherwise the persisted half.
  `restore_tabs` and `restore_tabs_in_workspace` build every `OpenTab` with
  it.
- `seed_shown_tabs` matches on the restored `pane`, so each half comes back
  on the tab it showed, including a terminal on the right.
- The startup repair beside `persisted_secondary_pane_hidden` ("the restored
  active tab is Secondary but the pane was hidden → un-hide") reads
  `tab.pane`, so it covers an active terminal on the right too.

Launch (`session::restore`), worktree switch (`restore_tabs_for`) and
mounted worktrees (`restore_tabs_for_mounted_worktree`) all go through
`tabs_for_worktree` and the two functions above. A mounted worktree's live
`OpenTab`s keep their `pane` in memory.

### Both directions of compatibility

- Old session, new build: no `pane` → `""` → every tab in its home half,
  exactly as today.
- New session, old build: serde ignores the unknown field and a moved
  terminal comes back on the left. Only the placement is lost.

### Deliberately unchanged

- **Sidebar and parked rows** (`persisted_tabs_for`) filter by kind through
  the new `appears_in_sidebar`, so a terminal on the right stays listed.
  Clicking its row activates it in its half and reveals a hidden Secondary
  (`reveal_secondary_for_active_tab` reads `tab.pane`).
- **Agent activity** (Layers A–D, `tab_has_live_foreground_process`) is keyed
  by tab and pane id, never by half.
- **Hiding the Secondary** (Ctrl+Shift+B, the strip's `×`) with an agent in
  it hides without closing, as today: the PTY keeps running and the sidebar
  shows it.
- **Control socket:** no new verb; creating verbs use `default_pane()`.

## §7 Edge cases

| Case | Behaviour |
|---|---|
| The last tab of a half moves out | That half shows its launcher. `sync_empty_pane_prompts` reads `tab.pane`. |
| A terminal tab split into several panes | The whole tab moves, splits included. A single split is never dragged to the other half. |
| A drag ends with no drop | Nothing moves; see "Drag state" (§5). |
| The pointer returns to its own strip | `pane_drop_target` clears; the in-strip preview resumes. |
| A Browser is active on the right during the drag | Unmapped for the drag (§5), mapped again after. |
| A rename is in progress | Ended the way activating another tab ends it. |
| The target strip overflows | It now holds focus, so `keep_active_tab_visible` scrolls it to the arrived tab. |
| Move to a hidden Secondary (menu, shortcut) | The pane is revealed. |

## §8 Tests

Written before the code.

| Where | What it pins |
|---|---|
| `sirio_project` `tab.rs` | `default_pane` routes as `pane_role` did (renamed `pane_role_routes_by_surface_kind`); `can_move_between_panes` is true exactly for Terminal and AgentChat, **spelled out kind by kind**, not looped, so a new kind does not compile until someone decides; `appears_in_sidebar` lists terminals and chats (renamed `the_sidebar_lists_exactly_the_primary_kinds`). |
| `sirio` `tab_machinery.rs` | `TestTab` gains `pane`. A Terminal placed in Secondary lands in Secondary's list; `select_tab` on it focuses Secondary; `rebuild` keeps it there. |
| `sirio` `session.rs` | `pane` round-trips; JSON written before it decodes to `""`; `placement_for`: `""` → default, `"secondary"` on Terminal → Secondary, `"primary"` on Editor → Secondary, garbage → default. |
| `sirio` `main.rs`, pure | `overlay_obscures_browsers` is true with a tab drag in flight; `window_shortcut_hints_match_bindings` covers the new command without change. |
| `sirio` `main.rs`, gpui (`VisualTestContext`) | Menu "Move to Right Pane" draws the terminal inside `pane-secondary` with **the same `TerminalView` `entity_id`** (the PTY was not restarted) and hands the left half to the nearest tab. The item is absent on a Browser tab. Ctrl+Shift+M moves the active tab and focus follows; twice brings it back; on a file tab nothing changes. A drop between two tabs of the other strip lands at that position; a drop on the other body lands last; a Browser dragged left changes nothing. Moving to a hidden Secondary reveals it. The Secondary launcher draws `launcher-terminal` and `launcher-chat`; the terminal tile opens on the right and shows no shortcut. After a simulated restart a terminal left on the right is back on the right, is the one shown, and is listed in the sidebar. Moving the last Primary tab leaves Primary on its launcher. |

Drag tests follow the existing divider drag test's convention (the first
simulated move is swallowed; see the comment near its `on_drag_move`).

**Verification:** iterate with `cargo nextest run -p sirio_project` and
`cargo nextest run -p sirio` (plain `cargo test -p sirio` shows four
process-sharing false failures). Renaming `TabKind::pane_role` changes
`sirio_project`'s public API, so `cargo build --workspace --all-targets` must
pass before the work is called done. `Scripts/ci.sh` runs only on the user's
request.

## §9 Deliberately absent

- **Browser, Editor, Diff or Project Settings in the Primary half.**
- **More than two halves**, or splitting the centre further.
- **Dragging one split of a terminal tab** to the other half.
- **Dragging sidebar rows onto a pane.**
- **A `sirioctl` verb to move a tab.** Added when an agent needs it.
- **The `CenterPane` entity refactor** — still its own project.
