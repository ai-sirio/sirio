# Local Pane Tab Layout Design

**Date:** 2026-08-04

**Status:** Approved

**Scope:** The tab chrome owned by each workspace pane group

## Summary

Tiller will present tabs as local navigation for each pane group, not as one global row for the whole worktree. Every pane keeps a thin tab strip visible, including panes with only one tab. The active tab is visually connected to its content, while a separate accent identifies the pane that currently receives keyboard input.

The design favors stable geometry and unambiguous ownership in split workspaces. Adaptive tab widths, horizontal overflow, and fixed trailing controls keep the strip usable in narrow panes. Agent activity is displayed with semantic glyphs that do not compete with the focus treatment.

## Goals

- Make it immediately clear which tabs belong to which pane.
- Distinguish the active tab in every pane from the single pane that owns keyboard focus.
- Keep agent status visible without relying only on color.
- Remain usable with many tabs and narrow split panes.
- Preserve the workspace engine's existing tab, drag, close, and pane-collapse semantics.
- Provide deterministic tests for layout behavior, interaction, and accessibility.

## Non-goals

- Changing terminal, editor, chat, or agent lifecycle behavior.
- Redesigning the sidebar or worktree navigation.
- Changing dirty-document confirmation or agent-termination policy.
- Adding tab pinning, tab groups, multiple tab rows, or a vertical tab rail.
- Keeping a second global tab strip visible above the local pane strips.

## Selected Direction

The approved direction combines the following decisions:

1. Tabs belong to individual pane groups.
2. Each pane's strip remains visible even when it contains one tab.
3. Tabs use a connected, IDE-like shape; the active tab visually joins the content surface.
4. Widths adapt to titles within minimum and maximum bounds. Excess tabs overflow horizontally.
5. Every pane shows its active tab, but only the focused pane gives its active tab the focus accent.
6. Agent activity uses a compact semantic glyph based on both shape and color.
7. The close button appears on hover, with its space reserved to prevent width jitter.
8. The overflow menu and new-tab button remain fixed at the right edge of each strip.

Alternatives considered and rejected were floating chips, equal-width tabs, icon-only inactive tabs, full-pane focus rings, dimmed unfocused panes, status-colored underlines, persistent text status badges, and hiding the strip when only one tab exists. Each alternative either weakened pane ownership, reduced title recognition, conflated state with focus, introduced visual noise, or made frequent actions less discoverable.

## Visual Structure

### Pane strip

Each pane group renders one strip approximately 30–32 points high. The strip is part of the pane chrome and does not float above the workspace. A one-tab pane retains the same height and control positions as a many-tab pane, keeping split geometry stable.

The primary target is `PaneTabStripBar` in `App/PaneTabStripBar.swift`, backed by `PaneTabStripModel` and the pane group state. `App/TabBarView.swift` must not appear as a simultaneous global tab strip in the universal workspace. If a legacy or fallback path still renders it, it should use compatible visual tokens but remain outside the main split-pane experience.

### Connected tabs

Tabs have rounded upper corners and a straight lower edge. The active tab uses the content surface as its fill and interrupts the strip's lower divider, making the tab and content read as one region. Inactive tabs remain visually quiet and receive a hover fill without becoming separate pill-shaped containers.

Each tab can contain, in order:

1. Content or agent icon.
2. Ellipsized title.
3. Dirty marker, when applicable.
4. Agent-status glyph, when applicable.
5. Reserved close-button area.

The close button becomes visible only while the pointer is over the tab. The reserved area remains the same width in both states, so hover never changes tab width or moves neighboring tabs. Status and close remain separate; hovering must not hide an agent that requires input.

### Focus hierarchy

There are three distinct signals:

- **Active tab:** connected shape and content-surface fill.
- **Focused pane:** a thin accent on that pane's active tab.
- **Agent activity:** a semantic trailing glyph.

Every pane may have an active tab, but only one pane is focused. Unfocused panes remain fully legible and are not dimmed. The focus signal must remain recognizable without depending exclusively on a color change, using placement and border shape as additional cues.

### Agent activity

Agent tabs show a compact status glyph:

- Running: animated spinner-like shape with a reduced-motion static equivalent.
- Idle: neutral filled point.
- Needs input: attention shape such as an exclamation mark.

The glyph has an accessibility label and tooltip. Its shape remains meaningful without color. It must not reuse the focus accent or the dirty marker.

## Sizing and Overflow

Tab widths are content-aware within an implementation-tuned range, initially targeting roughly 72–160 points. The active tab may preserve a larger minimum when space permits. Titles truncate at the tail and expose their full value in a tooltip and accessibility label.

The scrollable tab region and fixed control region are separate:

- Tabs scroll horizontally as one ordered sequence.
- Activating a hidden or partially visible tab reveals it automatically.
- Edge autoscroll remains available during drag.
- A trailing overflow control (`⌄`) is shown when required and lists every tab in current order.
- The new-tab control (`+`) is always visible next to the overflow control.
- The fixed controls never scroll away and have a subtle boundary from the tab sequence.

The overflow menu shows content type, full title, agent status when present, and a checkmark on the active tab. Selecting an item activates it and focuses the owning pane.

## Interaction Model

### Activation and creation

Clicking a tab activates both the tab and its pane group. The pane focus accent follows the actual keyboard-input destination. The `+` button opens the existing new-tab menu and targets the pane group that owns that strip.

### Reordering and cross-pane drag

Dragging horizontally reorders a tab within its current strip. Crossing into another pane changes the target to that pane group and shows a local insertion indicator. The interaction preserves existing workspace intent and engine boundaries rather than mutating view state directly.

The drag behavior includes:

- A movement threshold before reordering begins.
- A stable drag representation that preserves the source tab's size.
- Insertion feedback in the destination strip.
- Edge autoscroll only while a drag is active.
- Escape cancellation that restores the original order and ownership.

### Closing and empty panes

Closing the active tab selects the nearest tab on the left, then the right if no left neighbor exists, matching the deterministic repair behavior in `WorkspaceLayoutEngine`.

Closing or moving the final tab out of a secondary pane removes that pane and collapses its parent split. Closing the final tab in the root pane leaves one valid empty root group. That empty pane displays a focused empty state with existing creation choices, including new terminal, new agent or chat, and the local `+` menu. It must not create content automatically.

Dirty-document confirmation and agent shutdown continue through existing close adapters and policies. The tab strip only initiates the close request.

## State and Package Boundaries

`WorkspaceLayout` and its `PaneGroup` values remain the source of truth for tab order, active tab, active pane group, and structural changes. `PaneGroupController` continues translating UI actions into `WorkspaceIntent` values. `PaneTabStripModel` remains the view-facing interaction model.

The current `TabMenuEntry` exposes only identifier, title, and active state. The visual design additionally needs content kind, dirty state, and optional agent presentation. These must be supplied as small value-type presentation metadata without importing AppKit or SwiftUI into `TillerCore`, and without making `TillerWorkspace` depend back on the app target. App-specific icons and theme rendering stay in `App/PaneTabStripBar.swift`.

No view should independently decide tab ownership or repair selection. Those decisions remain in the workspace engine.

## Accessibility

- The strip is exposed as the local tab group for its pane, not duplicated in a global rotor.
- Each tab announces content type, full title, selected state, dirty state, and agent status.
- Activation and close are distinct accessible actions.
- The focused pane is conveyed through accessibility focus as well as visual styling.
- Status meanings use shape, label, and color; color is never the only signal.
- Truncated titles and icon-only controls have tooltips and explicit accessibility labels.
- Motion used for running state respects the system Reduce Motion preference.

## Error and Edge Handling

- A tab removed while a menu is open disappears from the next model update; selecting a stale menu item becomes a no-op through the existing intent validation path.
- When the active tab changes because of close or a remote model update, the strip reveals the repaired selection after layout settles.
- A drag cancelled by Escape, source removal, or invalid destination clears all temporary frames and indicators.
- Very narrow panes preserve the active tab and fixed controls before inactive title width. All tabs remain reachable through overflow.
- Missing optional icon or status metadata falls back to a neutral content icon and no status glyph; it never prevents a tab from rendering.

## Verification Strategy

Use `swift-testing` and deterministic geometry rather than timing-dependent UI assertions.

Focused coverage should verify:

- One `TabMenuEntry` per workspace tab, in engine order.
- Active-tab and focused-pane states remain distinct.
- Width clamping and overflow detection at narrow and wide viewport sizes.
- Automatic reveal of an activated tab.
- Fixed trailing controls remain outside the scrollable region.
- Hover close visibility does not change measured tab width.
- Agent glyph semantics and accessibility labels for running, idle, and needs-input states.
- Reorder, cross-pane move, edge autoscroll, and Escape cancellation.
- Closing the active tab repairs selection to the left, then right.
- Closing the last secondary tab collapses its split; closing the root's final tab keeps an empty root group.
- The local `+` targets the pane group that owns the strip.
- Overflow-menu activation focuses the correct pane and tab.

Run package-focused tests during iteration, then execute the repository gate:

```bash
Scripts/ci.sh
```

The work is not complete unless the command ends with `CI OK`.

## Completion Criteria

The design is implemented when every pane renders stable local tab chrome, the three visual state layers remain unambiguous, overflow and drag work in narrow split panes, accessibility exposes all tab semantics, existing lifecycle behavior is preserved, focused tests pass, and the full repository gate prints `CI OK`.
