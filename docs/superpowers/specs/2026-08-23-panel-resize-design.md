# Resizable shell panels — Design

Date: 2026-08-23
Branch: `main`

## Goal

Make the left sidebar and the right panel resizable by dragging their inner
edge, and persist the chosen widths across restarts.

Today both are fixed: `SIDEBAR_WIDTH` (`main.rs:318`) and `RIGHT_PANEL_WIDTH`
(`main.rs:319`) are constants applied as `.w(px(...)).flex_none()`
(`main.rs:10510`), under a comment reading *"Fixed panel geometry, measured off
the frozen reference shots"*. A third hardcoded copy of the same number lives in
`right_panel/mod.rs:32` (`PANEL_WIDTH`).

## Non-goals

- The IntelliJ-style filter toolbar above the git history. That is a separate
  spec; this one exists because the toolbar has to be designed against a panel
  whose width the user controls, and that panel is not resizable yet.
- Persisting panel *visibility*. `right_panel_visible` (`main.rs:4342`) is
  in-memory only today and stays that way.

## The dead settings this replaces

`SettingsPolicy` (`tiller_project/src/settings.rs:5`) declares `sidebar_width`
(default 240, clamped 160–480) and `right_panel_width` (default 320, clamped
220–640). Both are dead: one write site (their own clamp) and two read sites
(that clamp and its own test). No UI reads either.

Two consequences shape this design:

1. **The defaults disagree with what is drawn.** 240/320 in the model versus
   325/405 on screen. Wiring the existing values verbatim would shrink both
   panels on first launch.
2. **They are in the wrong crate.** Persisted settings clamp through
   `settings_ranges` (`model.rs:442`), applied by `clamp_setting` at load
   (`db.rs:908`) — not through `SettingsPolicy::from_values`. No persisted
   setting uses that path.

Both `SettingsPolicy` width fields and their clamp lines are removed, and
`values_are_clamped_to_safe_ranges` updated. Leaving them would recreate the
exact condition this spec exists to fix: a clamp with no reader.

## Decisions

- **Live state in `TillerWorkspace`**, two `f32` fields, seeded at startup from
  `AppSettings`. Considered and rejected: settings-store-as-only-source (a
  global write per drag frame, for a window-local property) and measured-width
  (a one-frame lag plus a feedback loop between the tab strip and the panel).
- **The persisted width is a preference, not the rendered width.** What is drawn
  is the preference narrowed to the space actually available. The viewport clamp
  is never written back to state, so shrinking a window and reopening it
  restores the chosen width instead of destroying it.
- **Stored as `i64` pixels.** Every other numeric key in `AppSettings` is `i64`;
  drag deltas are fractional, so the value is rounded at persistence.
- **The `main.rs` constants are deleted, not repurposed.** The defaults belong
  in `tiller_persistence`, which sits *below* `tiller` in the crate graph and
  cannot read them; keeping them would add a fourth copy of 405 rather than
  remove one. 325 and 405 land in `AppSettings::default()` (`model.rs:392`) and
  at their `clamp_setting` call sites, which is where every other setting's
  default already lives. A user who never drags sees no change.
- **`PANEL_WIDTH` (`right_panel/mod.rs:32`) is deleted.** The panel no longer
  sizes itself; the host sizes it.

## Architecture

### 1. Data flow

```
DB (key-value settings)
  → clamp_setting(…, settings_ranges::SIDEBAR_WIDTH, 325)        [db.rs:908 area]
  → AppSettings { sidebar_width: i64, right_panel_width: i64 }   [model.rs:347]
  → TillerWorkspace { sidebar_width: f32, right_panel_width: f32 }
  → resolve_panel_widths(…) → rendered widths
```

New entries:

- `settings_keys::SIDEBAR_WIDTH = "appearance.sidebarWidth"`
- `settings_keys::RIGHT_PANEL_WIDTH = "appearance.rightPanelWidth"`
- `settings_ranges::SIDEBAR_WIDTH: RangeInclusive<i64> = 160..=480`
- `settings_ranges::RIGHT_PANEL_WIDTH: RangeInclusive<i64> = 220..=640`

Both keys are Linux-rewrite-only and have no Swift antecedent, so they are
doc-commented the way `appearance.translucency` (F-SET-20) already is. Every
other key in `settings_keys` is verbatim from `TillerCore/AppSettings.swift`;
these two must not be mistaken for parity keys.

No database migration is required: settings are key-value rows, and an absent
key falls through to its default.

### 2. `resolve_panel_widths` — the whole feature in one pure function

```rust
fn resolve_panel_widths(
    viewport_width: f32,
    left_pref:  Option<f32>,        // None = hidden
    right_pref: Option<f32>,
    dragging:   Option<PanelSide>,  // who has priority right now
    outer_inset: f32,
    gap: f32,
) -> (Option<f32>, Option<f32>)     // rendered widths
```

No gpui, no state, no window. Rules, in order:

1. `budget = viewport − 2·outer_inset − gap·visible_count − MIN_CENTER_WIDTH`
2. If `left + right ≤ budget`, both preferences pass through untouched.
3. Otherwise, **if a drag is active**: the dragged panel keeps the preference
   the drag has just set; the other absorbs the remainder, floored at the low
   end of its own settings range (160 for the sidebar, 220 for the right
   panel). Dragging the right edge makes the left panel yield, which is what
   the hand expects.
4. Otherwise (window shrunk, no drag): both shrink **proportionally**, so the
   ratio between them survives.
5. If not even the two minima fit, both go to their minimum and the centre is
   allowed below `MIN_CENTER_WIDTH`. A window too small to honour everything
   must still draw something rather than produce a negative width.

`MIN_CENTER_WIDTH = 320.0`. The precedent for a floor of this kind is
`MIN_SPLIT_PANE_SIZE = 160.0` (`main.rs:3339`), which guards terminal splits;
320 is roughly forty columns at the terminal cell width.

This function is why the contention rules are testable at all: every interaction
between the two panels is decided here, with no window to drive.

### 3. The drag handle

A 6px strip (`SPLIT_DIVIDER_SIZE`, `main.rs:3338`), **absolutely positioned and
out of layout flow**, on the inner edge of each panel: the right edge of the
left sidebar, the left edge of the right panel.

Out of flow, and not a flex child, for a measurable reason: the shell row is
`.flex().flex_row().gap(shell_gap)` (`main.rs:10467`), and five existing tests
assert `right.left() − center.right() == px(4.0)`. A flex child would produce
two gaps where there is one and break all five. Out of flow, those five
assertions stay valid without being touched.

Inside the panel rather than straddling its border, because
`shell_chrome::panel` sets `.overflow_hidden()` (`shell_chrome.rs:79`), which
clips anything hanging outside. Being a child of the panel also means the handle
follows it: it cannot drift out of the window if the window is resized mid-drag.

Accepted cost, documented: 6px of adjacent content stops being clickable along
that edge.

Wiring mirrors `DraggedPaneDivider` (`main.rs:3332`) and its handler
(`main.rs:8921`):

- `on_drag_move::<DraggedPanelEdge>` updates live state and calls `cx.notify()`.
- `on_drop::<DraggedPanelEdge>` restores focus to the focused pane.

The focus restore is not optional. GPUI's drag machinery clears focus after
mouse-down, and the existing divider carries a comment recording that a
next-frame re-focus loses the race — drop is the only point that works.

### 4. Persistence cadence

| what | when |
|---|---|
| live state + `cx.notify()` | every drag-move event |
| `save_settings` | once, 500 ms after the last drag-move |

The debounce uses a **gpui timer inside `cx.spawn`**, with the
generation-counter shape of `Terminal::resize`
(`tiller_terminal/src/lib.rs:681`) but not its mechanism. `Terminal::resize`
uses `std::thread::spawn` + `std::thread::sleep`, and so does the session
flusher; neither can be advanced by `background_executor.advance_clock`, so
neither is deterministic under test. A gpui timer is.

The debounce is not redundant with `on_drop`. A drag can end without a drop —
Escape, a panel hidden by shortcut mid-gesture, a release outside the window —
and the rendered width and the stored preference would then diverge silently
until the next restart. The timer closes that, leaving `on_drop` doing only what
only it can do.

Known limitation, accepted: when no drop arrives, focus is not restored. That is
the existing behaviour of the pane divider, so it is not a new regression.

### 5. Call sites that assume fixed geometry

`tab_strip_available_width_for_shell` (`main.rs:332`) takes
`sidebar_visible: bool, right_panel_visible: bool` and subtracts the constants.
It changes to take `Option<f32>` per panel, `None` meaning hidden, so visibility
and width become one concept.

It must be passed the **rendered** widths, not the preferences. Passing
preferences would over-subtract whenever the viewport clamp is active, and the
tab strip would size itself against space that was never taken.

## Testing

| test | what it pins |
|---|---|
| unit, `resolve_panel_widths` | contention, drag priority, proportional shrink, both-at-minimum, hidden-panel cases |
| drawn drag with real mouse events | `debug_bounds("shell-right-panel").size.width` actually changes |
| **configured width ≠ default → rendered bounds match it** | the wiring itself; the test whose absence let the dead setting pass CI |
| `main.rs:20498` and `main.rs:20478`, rewritten | assert against the configured width instead of the constant |
| `main.rs:20515`, parameterised | 254/663/583/992 stop being 405 and 325 in disguise |
| `advance_clock(500ms)` → nothing stored before, last width after | the debounce, deterministically. Not a literal write count: that would need a spy `SessionStore`, and the codebase has no seam for one |
| focus restored at drop | the GPUI drag/focus interaction |

The drawn-drag pattern already exists at `right_panel/mod.rs:628`
(`a_payload_drag_reaches_the_drop_target_through_real_mouse_events`) and
`main.rs:16603`. Workspace fixtures: `palette_test_workspace` (`main.rs:13588`).

The two rewritten assertions are the point of that table. They read the same
constant the render code reads, so they cannot fail when the render ignores the
configured value — they verify that `405 == 405`. Comparing measured bounds
against the *configured* width is what makes them tests.

`PANEL_WIDTH`'s removal has a migration consequence: `right_panel/mod.rs` tests
render `RightPanel` standalone and rely on it sizing itself. Those fixtures must
wrap it in a sized container or they collapse to zero width.

## Documentation

`conformance.rs:58` records **"Right panel header 40px, activity rows 48px,
`PANEL_WIDTH` 405"** in the departures ledger as a frozen starting point. The
entry becomes false once the width is user-controlled, and is updated to state
the 220–640 range with 405 as the default.
