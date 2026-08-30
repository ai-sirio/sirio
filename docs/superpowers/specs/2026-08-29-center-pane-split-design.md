# Splitting the center area into two fixed-role panes

The work area gains a second pane. The layout becomes LeftSidebar — Primary pane —
Secondary pane — RightSidebar. Chat and terminals live in the Primary pane; a
browser, an editor, a diff live in the Secondary pane, which is absent until one
of those is opened.

Charted as [#318](https://github.com/ai-sirio/sirio/issues/318); every decision
below was settled in one of its seven child tickets, linked at the point it
applies.

## What this changes

- `TabGroup` / `TabMachinery` (`rust/crates/sirio/src/tab_machinery.rs`) narrow
  from N interchangeable, equal-width groups to exactly two panes with fixed
  roles.
- Which pane a tab belongs to stops being **stored** and becomes **derived** from
  its `TabKind`. `OpenTab.group_id` is deleted.
- The single tab strip above the whole center area becomes one strip **inside**
  each pane.
- The two panes are resizable by a persisted ratio; `MIN_CENTER_WIDTH` stops
  being a constant and becomes a function of whether the Secondary pane is open.
- `move_selected_tab_to_new_pane` and the whole "move a tab to another pane"
  family are removed.

Nothing about the split tree **inside** a tab (`PaneNode`, `SplitPaneRight`,
`SplitPaneDown`) changes. It is a different, working feature.

## Vocabulary

The word *pane* already meant two things before this work — the split tree inside
a tab, and the pty records the control socket's `panel.*` verbs address. Adding a
third meaning was rejected.

`CONTEXT.md` now defines **Pane** (reserved for the in-tab split tree),
**Center split**, **Primary pane role** and **Secondary pane role**. In code the
new concepts are never a bare `Pane`: they appear only as `PaneRole::Primary`,
`PaneRole::Secondary`, and the `CenterSplit` container.

## The model

| | |
|---|---|
| Primary pane | `TabKind::Terminal`, `TabKind::AgentChat` |
| Secondary pane | `TabKind::Browser`, `TabKind::Editor`, `TabKind::Diff` |
| Routing | Derived from `TabKind`. Rigid — no dragging tabs between panes |
| Secondary default | Closed; the Primary pane takes the full width |
| Orientation | Side by side only |

The routing rule is the spine of the design. Because the pane is derived, there is
no `pane_id` to persist, no migration for it, and no way for "which pane" to
disagree with "what kind of tab this is" — they are the same fact read twice.

## The type — [#319](https://github.com/ai-sirio/sirio/issues/319)

`PaneRole` lives in `sirio_project`, next to `TabKind`:

```rust
pub enum PaneRole { Primary, Secondary }

impl TabKind {
    pub fn pane_role(&self) -> PaneRole { /* Terminal|AgentChat => Primary, else Secondary */ }
    pub fn appears_in_sidebar(&self) -> bool { self.pane_role() == PaneRole::Primary }
}
```

`appears_in_sidebar()` (`sirio_project/src/tab.rs:35`) is **redefined** in terms of
`pane_role()` rather than kept as a second list. The LeftSidebar listing only
Primary-pane tabs and the routing rule are one fact, so they get one definition.
`sirio_project` is the right home because that predicate already lived there.

`TabMachinery` is replaced by `CenterSplit`, in `sirio`, still `pub(crate)`:

```rust
struct CenterSplit {
    active_tab: EnumMap<PaneRole, Option<usize>>, // or two Option<usize> fields
    focused: PaneRole,
}
```

**No `Vec`.** Keying by `PaneRole` makes "zero panes", "three panes" and "unknown
pane id" unrepresentable, so four of `TabMachineryError`'s six variants disappear
from the type rather than being checked at runtime.

**No `Result`.** With membership derived and a single tab list, the only invariant
left is "a role's remembered active tab still exists in that role".
`remove_tab` (`tab_machinery.rs:198-208`) already answers that case by falling back
to the nearest remaining tab; promoting that fallback to the general rule makes the
constructor total and removes the `.expect("workspace tabs must form a valid tab
placement model")` at `main.rs:7219`.

**One ordered tab list per worktree.** `self.tabs: Vec<OpenTab>` keeps the order
the user chose; each pane *filters* it by `pane_role()`. `TabGroup.tabs` conflated
membership with order — membership is now derived, order stays where it already
was. Two lists could disagree; one cannot.

### Knock-on renames

- `activate_group` (`main.rs:7379`) → `set_focused_pane(role: PaneRole)`. It no
  longer rebuilds the whole machinery just to change a field, because there is
  nothing left to validate.
- `rebuild_tab_machinery` (`main.rs:7160`) reduces to recomputing each role's
  active-tab fallback from `self.tabs`.
- The terminal pane cache placement key `format!("group-{group_id}-pane-{pane_id}")`
  (`main.rs:7326`) becomes role-keyed. Changing its format is free: `TerminalPaneCache`
  (`sirio_terminal/src/lifecycle.rs:63`) is in-memory only and never persisted.

## Layout — [#320](https://github.com/ai-sirio/sirio/issues/320)

**Each pane owns its tab strip, inside itself.** `center_column` (`main.rs:11426`)
stops being a single vertical column: each pane becomes its own stack — strip at
`TAB_BAR_HEIGHT` on top, surface below — and the two stacks sit side by side with
the divider between them. The shared strip row at `main.rs:11433-11444` is gone.

The alternative — keeping one strip on top, split into two segments at the divider —
was cheaper but rejected: the strip would remain a *sibling* of the panes, so its
seam would have to be kept aligned with the divider by hand on every resize and
every ratio change. That is the same number held in two places, which is what this
whole effort removes elsewhere. Inside the pane, misalignment is unrepresentable.

A third option (Primary strip full width, Secondary opening beneath it) was rejected
for contradicting the model: the two panes are peers, and it drew one as chrome and
the other as a guest.

Prototype: `docs/prototypes/center-split-tab-strip.prototype.html` (commit `a92b559b`).

### Strip details

- **Focus is marked one way**: an accent underline on the focused pane's active tab,
  using `tab_focus_accent` (`sirio_theme/src/lib.rs:140`), whose stated purpose is
  already "focus rings, caret, live". No coloured pane border — a pane in Sirio has
  never had one, and it pulls the eye to the frame instead of the content.
- **One `+`, in the Primary strip only.** It routes by itself. A second `+` in the
  Secondary strip would have to decide what happens when a chat is opened from it —
  a button that sends you elsewhere.
- **The Primary strip widens when the Secondary pane closes**, because it is inside
  its pane. This is the property variant B was chosen for, not a side effect.
- **The empty-pane prompt is Primary-only.** The Secondary pane auto-closes and so
  is never empty; the Primary pane does not and can be (close every terminal and
  chat), so it keeps the prompt — inside its own strip-bearing frame.

## Resize — [#321](https://github.com/ai-sirio/sirio/issues/321)

The split reuses `panel_layout.rs` — the module, its discipline and its 320 — but
not `PanelSide`.

**`PanelSide` does not grow a `Center` variant.** The module is declared as "width
resolution for the two shell **side** panels", and `PanelSide::range()` returns a
pixel `(floor, ceiling)`. A ratio-driven divider has no pixel range to give; a
`Center` variant would be a third case in a two-case enum with a meaningless
`range()`. The center divider gets its own drag payload; `DraggedPanelEdge`
(`main.rs:3539`) and `update_panel_width` (`main.rs:11623`) are untouched, because
the center drag writes a ratio, not a width.

**`MIN_CENTER_WIDTH` becomes derived, and 320 becomes a per-pane floor.** Its own
comment (`panel_layout.rs:34-38`) justifies the number: a terminal split's floor is
`MIN_SPLIT_PANE_SIZE = 160`, and the center column gets twice that because it
"carries a whole tab strip above a terminal". With the strip now inside each pane,
that sentence describes a single pane. So:

- Secondary closed → center floor `320.0`
- Secondary open → center floor `320.0 + SPLIT_DIVIDER_SIZE + 320.0`

`resolve_panel_widths` takes this as a parameter instead of reading the constant.

**The ratio is a preference, never truth.** Verbatim the module's existing
discipline (`panel_layout.rs:3-7`): the clamp is a projection applied at render
time, never written back, so re-widening the window restores the chosen split.
Narrowing clamps a pane at 320 without rewriting the stored ratio. Saving reuses
the debounced `schedule_panel_width_save` path (`main.rs:11655`) — one write, one
generation counter, not a second timer.

**When two 320s do not fit**, both panes go below the floor *together*, keeping
their ratio — the existing no-drag rule (`panel_layout.rs:80-94`), matching
`a_window_too_small_for_both_floors_still_returns_the_floors`. The Secondary pane is
never auto-hidden to make room: that would remove the user's tabs from view without
being asked.

**A center-divider drag stops at the floor.** The sidebars are not recruited, even
though the machinery exists, because the module's most-emphasised rule is that
"moving the panel the user is *not* touching reads as a bug"
(`panel_layout.rs:113-115`). Sidebars yield when the *window* shrinks — nobody's
hand is on anything. A drag is the opposite.

New pure function, same module:

```rust
pub(crate) fn resolve_center_split(
    center_width: f32,
    ratio_millis: i64,
    secondary_open: bool,
    divider: f32,
) -> (f32, Option<f32>)
```

## Persistence — [#323](https://github.com/ai-sirio/sirio/issues/323)

Two pieces of new state, different scopes, neither needing a new shape.

**The ratio — global, a `setting` row, no migration.**

```rust
// AppSettings, sirio_persistence/src/model.rs
/// "appearance.centerSplitRatio" — default 500, clamped to 100...900.
pub center_split_ratio: i64,
```

Thousandths, not a float: `AppSettings` has no float anywhere, and
`PaneEvent::SetRatio { ratio_millis: u16 }` (`session.rs:67-69`) is the in-repo
precedent for exactly this quantity. `setting` is key/value
(`migrations.rs:60-63`), so a new key is a new row.

The `100..=900` range in `settings_ranges` is a safety net only — it stops a
corrupt or hand-edited value driving a pane to zero width. The real constraint is
the runtime 320px floor. `panel_ranges_match_the_persisted_settings_ranges`
(`panel_layout.rs:170-186`) asserts each side by hand rather than iterating, so the
new range neither breaks it nor is covered by it.

**The open/closed flag — per worktree, one column, schema v16.**

```sql
ALTER TABLE worktree ADD COLUMN secondary_pane_open INTEGER NOT NULL DEFAULT 0;
```

`WorktreeRecord` (`model.rs:60`) is already the per-worktree record and already
carries non-tab-shaped UI state (`comment`). A dedicated table for one boolean was
rejected. The migration is routine and forward-only, identical in shape to
`migrate_v10` and `migrate_v12`.

This flag is the one place the effort deliberately breaks its own
derive-don't-store discipline, and it should be commented as such where declared.
It exists **only** because the keyboard toggle produces a "closed but still holding
tabs" state that cannot be derived from "are there Secondary tabs?". Without the
toggle it would be pure derived state and must not exist.

**Restore.** Tabs whose target no longer resolves — a diff on a vanished commit, an
editor on a deleted file — are dropped silently, filtered **at materialisation**
(around `main.rs:13537-13650`) rather than at `SessionLayout` load. Deciding whether
a target still exists needs git or the filesystem, which already happens there and
nowhere else in the restore pipeline; `SessionLayout` (`session.rs:399`) stays a
pure data container with no I/O.

If no Secondary tab survives, the pane **starts closed**. The persisted flag is
honoured, not overridden — there is simply nothing left for it to open onto.

## Opening, closing, focus

**Opening.** Diff and Editor are promoted from the RightSidebar; the browser comes
from the `+` menu. Re-opening something already open reuses its tab and brings it
forward rather than adding a duplicate.

**Focus** moves into the Secondary pane when it opens, and returns to the Primary
pane's active tab when it closes.

**Two distinct closing gestures, and they must stay distinct:**

- `ctrl-shift-b` **hides** the pane and **keeps** its tabs. This is the state
  `secondary_pane_open` persists.
- The `×` at the end of the Secondary strip **closes its tabs**, which triggers the
  auto-close. "I'm done here", not "get this out of my way".

If the `×` merely hid the pane it would duplicate the toggle and should not exist.

### The keybinding — [#324](https://github.com/ai-sirio/sirio/issues/324)

`ctrl-shift-b`. Zed (`cmd-alt-b`) and VS Code (`cmd+opt+b` / `ctrl+alt+b`) both
settled on **B** for the secondary-panel toggle, and `ctrl-shift-` is the family
Sirio's own panel toggles use — `toggle_sidebar` is `ctrl-shift-s`,
`toggle_right_panel` is `ctrl-shift-i` (`main.rs:4677-4685`). Wiring touches the
same five points as its siblings, ending at the dispatch arm near `main.rs:12325`.

Two facts anyone touching keybindings needs:

1. **There are two keybinding mechanisms.** Besides the declarative table
   (`actions!` / `KeyBinding`, `main.rs:190-316`, `linux_window_shortcuts()` at
   `main.rs:236-253`, `panes::bind_keys` at `panes.rs:116-142`), a hand-written
   capture-phase handler `handle_root_key_down` (`main.rs:12116-12169`, wired at
   `main.rs:13004` and `main.rs:13074`) intercepts `ctrl-shift-p` and `ctrl-k`
   *before* declarative dispatch. Those chords are taken but invisible to a grep for
   `KeyBinding::new`.
2. **Registration context, not the chord, decides who beats the PTY.** A binding
   registered with context `None`, as both existing panel toggles are, wins over a
   focused terminal (see the `cmd-w` vs `ctrl-w` note at `main.rs:3937-3945`).

No per-OS chord precedent exists; the table is shared across platforms and uses
literal `ctrl` even on macOS. The new toggle follows suit.

## The RightSidebar — [#322](https://github.com/ai-sirio/sirio/issues/322)

**Almost nothing changes here, because the intended behaviour already ships.**

- **Changes**: a single click calls `toggle_change` (`changes.rs:1344-1348`) and
  expands the diff **inline, in place**. Promotion is a separate explicit
  "Open diff" button (`changes.rs:1425-1431`), drawn only when `embedded_in_panel`
  is set — i.e. only in the sidebar. `ChangesTab::in_right_panel`
  (`changes.rs:461-465`) is the constructor that sets it.
- **Files**: already opens on **double** click — `event.click_count >= 2`
  (`right_panel/files.rs:488-489`), covered by
  `double_clicking_a_drawn_file_row_emits_open_file`.

So "single click previews, double click promotes" was reached independently on both
halves before this effort started.

**The promotion needs no call-site change.** `add_file_tab` builds `TabKind::Editor`
and `add_changes_tab` / `add_commit_tab` build `TabKind::Diff`; all are Secondary by
derivation. `RightPanelEvent::OpenFile` (`main.rs:4695`),
`RightPanelActionEvent::OpenDiff` (`main.rs:4703`) and `OpenCommit`
(`main.rs:4708`) are untouched. Routing was never expressed at the call site, so
there is nothing there to update.

**Files gets no preview, deliberately.** A file preview inside a 220–640px panel
would show truncated code and push the user to widen the sidebar — the cost the
split exists to remove. A diff reads fine narrow because it is short, marked lines.
The asymmetry is by nature, not oversight.

**The one piece of new work: Enter promotes the selected row**, matching the double
click. No keyboard path to open exists today.

## The removal cascade — [#325](https://github.com/ai-sirio/sirio/issues/325)

`move_selected_tab_to_new_pane` (`main.rs:7640`) has exactly **one** production
entry point — the "Move to New Pane" context-menu item (`main.rs:10559-10564` →
`main.rs:10699-10701`) — and is the **only** production path that can grow
`TabMachinery` past one group. Session restore (`main.rs:4372`) and
`rebuild_tab_machinery` (`main.rs:7208`) only ever yield one, consistent with
`SessionTab` (`session.rs:379`) having no `group_id`.

Because nothing else can create a second group, removing it does not merely break
the rest of the family — it makes it permanently unreachable. All of it goes:

- `MoveTabToOtherPane` action (`main.rs:180`) and `handle_move_tab_to_other_pane`
  (`main.rs:12038-12055`). It never had a keybinding.
- `TabCommand::MoveTabToOtherPane` (`command_palette.rs:39`), its `has_other_pane`
  gate (`main.rs:12093`) and its dispatch (`main.rs:12384-12386`).
- The "Move to Pane {id}" context items (`main.rs:10566-10572`) and
  `move_selected_tab(MoveTarget::Group(id))`.
- `TabMachinery::add_group` (`tab_machinery.rs:153`), whose only caller is
  `main.rs:7655`.
- `TabMachinery::move_tab`, `MoveCandidates`, `move_candidates`
  (`tab_machinery.rs:32,235,267`) — already compiled out, N-group support.

The `#[allow(dead_code)]` items in `panes.rs` (`PaneSize`, `SplitDisabledReason`,
`TabSelection`) belong to the in-tab split tree and are **not** removed.

### The control socket is unaffected

No `sirio_control` verb and no flat `sirioctl` command reaches
`move_selected_tab_to_new_pane` or constructs a `TabGroup`. Despite the names,
`pane.*` drives the split tree inside one tab and `panel.*` drives `PaneRegistry`,
a separate pty registry. Only `tab.cycle` and `tab.select` touch group scoping, and
they already scope to the active group's tabs — which reads correctly as "the
focused pane's tabs". They need re-pointing at the new accessor, not a behaviour
change.

### Tests to reckon with

Five tests hand-build a second group, bypassing the removed function:
`tab_machinery.rs:317-410+` (five unit tests on the `ids 10/20` fixture), and in
`main.rs` — `tab_context_menu_never_offers_a_this_pane_move` (`:15281`, F-TAB-12),
`drawn_tab_context_menu_moves_a_tab_to_another_pane_group` (`:19082`),
`moving_a_tab_to_another_pane_returns_focus_to_the_moved_tab` (`:19135`,
F-CORE-WSP-05), `drawn_tab_context_menu_move_records_the_terminal_in_the_pane_cache`
(`:19210`, F-TERM-PTY-08).

**One needs rethinking rather than updating**:
`drawn_detached_pane_group_offers_the_real_empty_prompt` (`main.rs:24995`) is the
only test that drives the function, and it asserts a *detached, tabless* group
survives and renders the empty-pane prompt. That encodes the old "groups never
collapse" invariant, which auto-close reverses. The empty prompt itself survives,
Primary-only.

## The standing constraint

**A worktree keeps one tab list. The pane is only where a tab is drawn.**

This is what keeps the effort clear of
`docs/linux-rewrite/CENTER-PANE-DESYNC.md` (cited at `main.rs:6200`), which
describes a deferred fix around `mounted` worktrees and
`tab_has_live_foreground_process` (`main.rs:5652`). That gate walks `self.tabs`
entirely; giving the panes separate lists would mean it and the mount/unmount unit
no longer look at the same set. Keeping one list means this work cannot make that
problem worse, and does not attempt to fix it.

## Out of scope, deliberately

- **Dragging tabs between the two panes.** The routing rule is rigid by decision,
  and the drag model is the expensive half of Zed's design.
- **Stacked (horizontal) split.** Side by side only.
- **Fixing `CENTER-PANE-DESYNC.md`.** A separate effort; here the standing
  constraint only avoids making it worse.
- **Removing or emptying the RightSidebar.** It stays as the compact view.
- **Per-OS keybindings.** Whether Sirio should have macOS-native `cmd` chords at
  all is its own question.

## Deliverables

Ordered so each step compiles before the next begins.

1. `PaneRole` in `sirio_project`, `TabKind::pane_role()`, `appears_in_sidebar()`
   redefined in terms of it.
2. `CenterSplit` replaces `TabGroup` / `TabMachinery`; `OpenTab.group_id` deleted;
   `activate_group` and `rebuild_tab_machinery` reshaped.
3. The removal cascade, including the five tests and the one that needs rethinking.
   `tab.cycle` / `tab.select` re-pointed.
4. One tab strip per pane; `center_column` restructured; focus underline; the `+`
   in the Primary strip; the `×` in the Secondary strip; empty prompt scoped to
   Primary.
5. `resolve_center_split` in `panel_layout.rs`; `MIN_CENTER_WIDTH` parameterised;
   the center divider's drag payload and handle.
6. `appearance.centerSplitRatio` in `AppSettings` and `settings_ranges`; schema v16
   adding `worktree.secondary_pane_open`; the materialisation-time restore filter.
7. `ctrl-shift-b` wired through all five points.
8. Enter promotes the selected RightSidebar row.

`Scripts/ci.sh` must print `CI OK` before a PR is opened.

## References

- Map: [#318](https://github.com/ai-sirio/sirio/issues/318)
- Tickets: [#319](https://github.com/ai-sirio/sirio/issues/319),
  [#320](https://github.com/ai-sirio/sirio/issues/320),
  [#321](https://github.com/ai-sirio/sirio/issues/321),
  [#322](https://github.com/ai-sirio/sirio/issues/322),
  [#323](https://github.com/ai-sirio/sirio/issues/323),
  [#324](https://github.com/ai-sirio/sirio/issues/324),
  [#325](https://github.com/ai-sirio/sirio/issues/325)
- Prototype: `docs/prototypes/center-split-tab-strip.prototype.html` (`a92b559b`)
- Glossary: `CONTEXT.md` — Pane, Center split, Primary pane role, Secondary pane role
