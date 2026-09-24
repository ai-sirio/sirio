# The Sessions view in the sidebar — design

**Date:** 2026-09-24
**Status:** approved design, not yet implemented
**Scope of this document:** a second view of the left sidebar that lists every
agent session — open ones across all projects, and chats the user closed —
ordered by when each last did something, switched with the existing Projects
view from the sidebar header. The right panel's Activity view is removed: this
list replaces it.

## What is there today

- **The sidebar has one view.** A `Projects` header, a filter field, then one
  band per project (`sidebar/section.rs`) and one two-line card per worktree
  (`sidebar/row.rs`): branch, the last task's title, a Bezel bloom for the
  worktree's aggregated status, and one pill per tab. Pills of a worktree that
  is not mounted come from its persisted strip (`parked_sidebar_tabs_for` in
  `sirio`'s `main.rs`).
- **Pills carry no status.** `Sidebar::set_worktree_tabs` builds every
  `SidebarPill` with `status: None`; `SidebarTab` has no status field at all.
  Only the worktree row receives a status, aggregated over its panes
  (`sync_worktree_activity`, which since #562 also counts the panes of a
  mounted, parked strip). `render_pills` already knows how to draw a pill's
  status dot; nothing feeds it.
- **The Activity view** (`right_panel/activity.rs`, `PanelView::Activity`) lists
  the selected worktree's tabs plus those of mounted, parked worktrees
  (`activity_surfaces`, `parked_activity_surfaces`), ordered by urgency
  (`AttentionSort`), not recency, and raises a badge on its rail icon for
  Error or NeedsInput (`RightPanel::activity_badge`).
- **A closed chat is deleted.** `AppDatabase::save_tabs` deletes every row no
  longer in the strip, and `chat_turn.tab_id` is `ON DELETE CASCADE`, so the
  transcript goes with it — deliberately, per the method's doc. The only copy
  that survives is `RetainedChat`, in memory, until the app quits. The chat's
  own History popover (`chat_sessions`) therefore lists only chats that are
  still tabs.
- **No wall-clock time of the last event exists.** `AgentActivityModel` keeps
  `status_changed_at` per pane as an `Instant`: monotonic, not convertible to a
  date, lost on restart.

## What changes

The sidebar header's `Projects` title becomes a two-segment switch,
`Projects | Sessions`. The Sessions view is **the Projects view flattened and
reordered, plus closed chats**: every agent tab that sits in some worktree's
strip — the same set the Projects view draws as pills — as one card each,
newest event first, then a collapsible `Closed` group.

```
┌─ sidebar (panel_width) ─────────────────────────┐
│ [ Projects | Sessions ]                     (+) │  + only in Projects
│ ⌕ Search sessions…                              │
│─────────────────────────────────────────────────│
│ ✳  Prototipi vista Sessioni               (◎)   │  agent mark · title · bloom
│    sirio  ⑂ worktree/green-harbor         now   │  project · branch · time
│ π  Tab spostabili tra i pane               ◉    │  settled amber: needs input
│    sirio  ⑂ feat/movable-tabs              2m   │
│ ⬡  Fix redirect dopo il login              ◉    │  settled green: done
│    orbit  ⑂ fix/login-redirect            14m   │
│ ▢  Menu New Chat nella sidebar          idle    │  idle: the word, no bloom
│    sirio  ⑂ worktree/lucky-stone           1h   │
│▔▔ Closed ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ 3  ⌄ ▔│  section band, collapsible
│ ✳  Spike SDK nativo Claude   (60% opacity)      │
│    sirio  ⑂ main                           2d   │
└─────────────────────────────────────────────────┘
```

Decisions taken while designing, and why:

| Question | Decision |
|---|---|
| Which sessions | Agent tabs in any strip — live, mounted-parked and unmounted — plus archived chats. Identical to the Projects view's pills, so the two views cannot disagree. |
| What "last updated" means | The last *agent event*: a transition of the tab's resolved status **into** Running, NeedsInput, Done or Error. A prompt counts because it produces a Running transition. Discrete events keep the order stable. |
| Where the list lives | Inside `Sidebar`, derived from its own rows and pills (one source of truth), not a second list pushed by the host and not a separate entity. |
| Closed chats | Archived instead of deleted: at most the newest 50, none older than 30 days. |
| Activity | Removed. Its attention badge moves onto the `Sessions` segment. |
| Look | Prototype A of the brainstorming canvas: the worktree card's 51px rhythm and its bloom. |

## 1. Data and persistence

### Per-tab facts on `SidebarTab`

`SidebarTab` (and, through `set_worktree_tabs`, `SidebarPill`) gains:

- `persistence_id: String` — the tab's stable database id. It keys the
  session's cached view and the host's event bookkeeping; a parked tab's
  strip index is not stable enough for either.
- `status: Option<ActivityStatus>` — the tab's own resolved status. Live tabs:
  `tab_status`. Tabs of a mounted, parked strip: the status of that tab's panes
  (`terminal_panes_by_tab[i]` ∪ `chat_panes_by_tab[i]`), which is the
  per-pane logic `parked_activity_surfaces` has today, moved here. Tabs of an
  unmounted worktree: `None`.
- `last_event_at: Option<i64>` — Unix milliseconds of the last agent event.

Consequence, accepted: the pill status dot `render_pills` already draws starts
appearing in the Projects view.

### `last_event_at`: detected at the projection, not in the layers

`sync_sidebar_tabs` is the one place that sees every tab's resolved status, so
it is the one place events are detected. The host keeps two maps keyed by
`persistence_id`: `last_seen_status` and `last_event_at`. For every tab with a
`Some` status:

- no previous status recorded → record it, **no event** (a restored or newly
  mounted tab's first observation is not something that just happened);
- previous status differs and the new one is Running, NeedsInput, Done or
  Error → event: `last_event_at = now`;
- anything else (same status, or a move to Idle) → nothing.

A tab created in this session starts with `last_event_at = now` at creation,
so a new session enters at the top. Pane ownership and the evidence layers stay
entirely inside `AgentActivityModel`; this reads resolved status only.

Every event is written at once with `AppDatabase::touch_tabs(&[(id, at)])`, one
`UPDATE` transaction per reconcile that produced any. `touch_tabs` is the
column's **only writer**: `save_tabs` neither reads nor writes it (its upsert
names its columns, so an existing value survives), which keeps a parked
worktree's events from depending on when its strip is next saved. On load,
`TabRecord.last_event_at` seeds the host's map.

### Migration v20

`tab` gains `last_event_at INTEGER NULL` and `closed_at INTEGER NULL`.
`TabRecord` gains both fields. Existing rows keep `NULL` in both: open, and
with no known event, so they sort after every session that has one.

### Archiving closed chats

- Closing a tab whose kind is `chat` calls `archive_tab(id, now)` before the
  strip is saved: `closed_at = now`, `is_active = 0`. It is an immediate write,
  like `save_session_ref`. Terminals are deleted as today.
- `save_tabs` deletes stale rows only `WHERE closed_at IS NULL`.
- `AppDatabase::tabs` and `AppDatabase::tabs_of_worktree` — what
  `SessionStore::persisted_tabs_for` and the layout restore read — add
  `closed_at IS NULL`, so restoring a worktree never reopens a closed chat.
  `closed_chats` is the only reader of archived rows.
- `closed_chats(limit)` returns `ClosedChatSummary { tab_id, worktree_path,
  title, agent_id, closed_at }` for archived chats **that have at least one
  `chat_turn`**, newest `closed_at` first.
- `prune_closed_chats(now)` runs after every archive. It deletes archived rows
  beyond the newest `CLOSED_CHAT_LIMIT = 50`, older than
  `CLOSED_CHAT_MAX_AGE = 30 days`, or with no `chat_turn` at all; the cascade
  takes their transcripts. Both constants live in `sirio_persistence`.
- `unarchive_tab(id, order_idx)` clears `closed_at` and places the row at the
  end of its strip.
- Removing a worktree deletes its archived chats through the existing
  `tab.worktree_id` cascade: a chat cannot be reopened without its worktree.
- `chat_sessions` does not filter `closed_at`, so the chat's History popover
  now lists closed chats too. Intended.

### The view setting

`appearance.sidebarView`, `"projects"` (default) or `"sessions"`, stored like
`appearance.sidebarWidth`. An unknown value reads as `"projects"`.

## 2. The sidebar

### State

`Sidebar` gains `view: SidebarView { Projects, Sessions }`, `closed_sessions:
Vec<ClosedSession>` (pushed by the host through `set_closed_sessions`),
`closed_expanded: bool` (default `true`, not persisted) and a cursor for the
session list. `ClosedSession` is `{ tab_id, worktree_path, title, agent:
Option<AgentMark>, closed_at }`.

### Header

- The `Projects` title becomes Bezel's `toggle_group`, two segments. Clicking a
  segment sets `view` and emits `SidebarEvent::ViewChanged(SidebarView)` so the
  host persists it.
- The `+` is drawn only in the Projects view.
- **Badge.** In the Projects view, the `Sessions` segment carries a 6px dot when
  an open session is in Error (`theme.danger`) or, failing that, NeedsInput
  (`theme.warning`). The Sessions view shows no badge: the rows say it.
- The filter's placeholder follows the view (`Search worktrees…` /
  `Search sessions…`), and switching views clears it.

### The projection: `sidebar/sessions.rs`

A pure function, `session_list(rows, closed, filter, now_ms) -> SessionList {
open, closed }`, beside `row.rs` and `section.rs`:

- walks **all** of `self.rows`, not the visible ones, so a collapsed project
  still contributes its sessions; each pill takes its project name from the
  project row above it and its branch and path from its worktree row;
- keeps a pill when it is `TabKind::AgentChat`, or `TabKind::Terminal` with an
  agent mark (a terminal where an agent was identified after the fact counts
  from the moment it is identified);
- sorts open sessions by `last_event_at`, newest first, `None` after every
  `Some`; the sort is stable, so ties keep tree order (project, worktree,
  strip);
- takes closed sessions in `closed_at` order, resolving project and branch
  from the worktree row whose `path` equals `worktree_path`, and drops one
  whose worktree is not among the rows;
- filters both lists by a case-insensitive substring match on title, agent
  name (from `AgentBrandColor`), project name and branch.

`relative_time(now_ms, at_ms) -> String`, also pure, on elapsed time (not
calendar days): under 60 s `now`; under 60 min `{n}m`; under 24 h `{n}h`;
under 48 h `yesterday`; under 7 days `{n}d`; then `{day} {Mon}`
(`18 Sep`). A timestamp in the future reads `now`.

### Rendering

One `SessionRowView` per session, cached and keyed by `persistence_id` (closed
ones by `tab_id`), on the `RowView` model: inputs pushed in, notify only on
change. A travelling bloom's lease then re-renders its own row only.

- **Line 1**, 18px: the agent mark in its brand colour (`pill_icon_color`),
  the title (`text`, ellipsis), then `RowStatusGlyph::for_status` — the same
  bloom as the worktree card. `Some(Idle)` shows the word `idle`
  (`text_faint`); `None` shows nothing.
- **Line 2**, 15px, indented under the title: project (`text_muted`), branch
  glyph and branch (`text_faint`, ellipsis), relative time (`text_faint`).
- Card height `CARD_TWO_LINE_HEIGHT`, `ROW_V_GAP` between cards. The session
  whose pill is `selected` is highlighted as a selected worktree card is.
- **Hover**: on an open session a close button replaces the time; on a closed
  one a delete button (`theme.danger` glyph) does.
- **Closed group**: a band drawn like a project section — `Closed`, the count,
  a chevron that toggles `closed_expanded`. Closed rows draw line 1 at 60%
  opacity and no status glyph.
- **Empty**: `No sessions`, centred, `text_faint`, as the Activity view did.
- A 60-second timer, alive only while the view is Sessions, re-runs the
  projection so time labels advance; only rows whose label changed notify.

### Interactions

| Gesture | Emits |
|---|---|
| Click a live session | `SelectTab(id)` (exists) |
| Click a parked session | `SelectParkedTab { .. }` (exists) |
| Close button on a live session | `CloseTab(id)` (exists) |
| Close button on a parked session | new `CloseParkedTab { path, index }` |
| Click a closed chat | new `ReopenClosedChat(tab_id)` |
| Delete button on a closed chat | first click arms: the button becomes a `Delete` label in `theme.danger`; a second click emits new `DeleteClosedChat(tab_id)`; Escape or a click elsewhere disarms. Same two-step as the History popover (F-CHAT-34). |
| ↑ / ↓ / Enter (list focused) | the cursor moves over open sessions, then closed ones when the group is expanded; Enter selects or reopens |

No context menu in this version. Switching views changes neither the selected
worktree nor the active tab.

## 3. The host (`sirio`'s `main.rs`)

### `sync_sidebar_tabs`

Builds every `SidebarTab` with `persistence_id`, `status` and `last_event_at`
as section 1 defines them, runs the event detection, and issues `touch_tabs`
for the events it found.

### Closed chats

- `close_tab`, for a chat tab: `archive_tab`, then `prune_closed_chats`, then
  refresh the list.
- `refresh_closed_sessions` reads `closed_chats(CLOSED_CHAT_LIMIT)` and calls
  `Sidebar::set_closed_sessions`. It runs at startup and after every archive,
  reopen and delete.
- `ReopenClosedChat(tab_id)`, in this order, which is the same for the current,
  a mounted-parked and an unmounted worktree:
  1. select the chat's worktree, if it is not the current one — its record is
     still archived, so neither the parked layout nor the database restore
     brings it back a second time;
  2. `unarchive_tab(tab_id, end of strip)`;
  3. build that one tab with the session-restore builder
     (`restored_chat_spec`, the stored transcript, and `agent_session_id` when
     `resume_agent_sessions` is on, so the agent's conversation continues),
     append it and make it active;
  4. `refresh_closed_sessions`, `schedule_save`.
- `DeleteClosedChat(tab_id)`: `remove_tab`, then `refresh_closed_sessions`.
- `CloseParkedTab { path, index }`: the `Parked` branch of today's
  `request_close_activity`, renamed `request_close_parked_tab` — bring the
  worktree forward, then the existing confirmation.
- `ViewChanged(view)`: write `appearance.sidebarView`.

### Removing Activity

- `sirio_ui`: `PanelView::Activity` and its `ORDER` entry,
  `right_panel/activity.rs`, `ActivitySurface`, `ActivityRef`,
  `RightPanelEvent::{SelectActivity, CloseActivity}`, `set_activity`,
  `activity_badge` and the rail badge, and the `activity` argument of the
  `RightPanel::with_activity…` constructors.
- `ActivityStatus` and `status_color` move out of `right_panel/` into
  `sirio_ui/src/status.rs`: after this change the sidebar would otherwise
  depend on the right panel for nothing else.
- `sirio`: `activity_surfaces`, `parked_activity_surfaces` (its per-pane logic
  moves into `sync_sidebar_tabs`), `select_activity`.
- `sirio_activity`: `build_activity_rows` and its types — no production caller
  today, only the crate export and one integration test.
- `PanelView` is an in-memory global, never persisted, so no stored
  `"activity"` value needs migrating.

## 4. Tests and verification

Tests first; every suite runs under nextest.

**`sirio_persistence`**
- v20 migrates forward preserving existing tabs and transcripts.
- `archive_tab` / `unarchive_tab`; `save_tabs` leaves an archived chat alone;
  strip queries exclude it.
- `prune_closed_chats` keeps the newest 50, drops those older than 30 days and
  those without turns, transcripts included.
- `closed_chats` order and its turn requirement; `chat_sessions` includes
  archived chats; deleting a worktree cascades to its archived chats.
- `touch_tabs` writes; `save_tabs` does not overwrite `last_event_at`.

**`sirio_ui` — pure functions in `sidebar/sessions.rs`**
- Only agent tabs become sessions; a collapsed project still contributes.
- Order: newest first, `None` last, ties in tree order.
- Closed sessions resolve project and branch by path; an unknown path drops.
- Filter over title, agent, project, branch.
- `relative_time` at 59 s, 60 s, 59 min, 60 min, 23 h, 24 h, 47 h, 48 h,
  6 d, 7 d, and a future timestamp.

**`sirio_ui` — gpui tests on the sidebar**
- The switch changes the view and emits `ViewChanged`.
- Badge: danger beats warning; absent in the Sessions view.
- Each gesture of the interactions table emits its event; the delete button
  needs two clicks and Escape disarms it.
- Empty state; keyboard navigation across the open and closed groups.
- Render isolation: a running session does not re-render its neighbours
  (`render_count`, as the existing `RowView` test does).

**`sirio` — host**
- A transition into a notable status updates `last_event_at`; a move to Idle
  does not; a restored tab's first observation does not.
- Per-tab status reaches pills, including a mounted-parked strip's.
- Closing a chat archives it; closing a terminal deletes it.
- Reopen from the current worktree and from another one; the reopened chat
  appears once.
- The right panel's rail has four views.
- The test that drives `request_close_activity` with an `Open` reference is
  rewritten against the sidebar's close path; the Activity view's own tests
  and the badge test go with the code.

**Verification.** `SidebarTab` and `TabRecord` gain public fields, which breaks
struct literals in other crates' tests: iterate with
`cargo build -p sirio` and `cargo build --workspace --all-targets`, not only
the crate being changed, and compare registered test *names* before and after
each change, not just the count. `Scripts/ci.sh` runs on the user's request.

## Out of scope

- **`RetainedChat`.** The in-memory list `resume_chat` reopens from stays as it
  is. With archiving it becomes redundant; merging the two is its own change.
- **Closed terminal sessions.** Only chats are archived and reopened; an agent
  terminal's `session_ref` resume is not offered from this list.
- **Creating a session from the Sessions view** (its `+`), a context menu on a
  session row, and time-bucket headings (Today / Yesterday).
- **A pinned "Needs you" group** (prototype C): the badge and the event order
  cover it for now.
