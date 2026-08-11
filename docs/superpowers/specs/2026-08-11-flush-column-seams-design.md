# Flush column seams with hover-only resize highlight

**Date:** 2026-08-11
**Status:** Approved

## Goal

Remove the two dark vertical bands that currently separate the sidebar, the
central pane, and the right panel in the main window. The columns should sit
flush against each other, with no visible band at rest. The divider becomes
visible only on hover, as a thin grey line that reuses the existing
`AppTheme.hairline` token, so the resize affordance survives.

Out of scope: the recursive pane splits inside the central pane (those go
through `TillerWorkspace`'s `DividerTracking` / `DividerCursorRects`, not
through `ContentView`'s `HSplitView`); giving `SidebarView` a surface of its
own; any new light-mode color value.

## Current behavior (baseline)

`ContentView.workspaceView` (`App/ContentView.swift:221`) paints a
`CanvasBackground` behind an `HSplitView` of up to three columns
(`splitColumns()`, `App/ContentView.swift:283`). Each column pads itself
`.horizontal, AppTheme.cardGap / 2`, and the split container pads the same
amount again (`App/ContentView.swift:243`). The sum is `cardGap` (10pt) of bare
canvas between two columns and `cardGap` against each window edge.

Three things make that inter-column region read as black:

1. **Card shadow spill.** `FloatingCard` applies
   `.shadow(color: .black.opacity(0.36), radius: 18, y: 6)`
   (`App/FloatingCard.swift:25-28`). The shadow is omnidirectional, so it
   darkens the canvas on both sides of every card. This is the dominant
   contributor.
2. **The gap itself.** `AppTheme.canvas` is `#131417`
   (`Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift:29-31`),
   against a central card surface of `#28292C`.
3. **`dividerCover`.** `App/ContentView.swift:276` paints a 2pt strip of
   `CanvasBackground` over `NSSplitView`'s own hairline. Today it is invisible
   because the surrounding gap is already canvas.

`SidebarView`'s root (`App/SidebarView.swift:18`) sets no background, so the
sidebar column is bare canvas. The band the user sees on the left seam is
therefore the central card's shadow falling onto that canvas, not a drawn
divider.

`DividerCursorStrip` (`App/DividerCursorStrip.swift`) already provides a 10pt
hover band that installs the `resizeLeftRight` cursor while letting clicks fall
through to the real `NSSplitView` divider underneath, governed by
`DividerCursorHitPolicy` (`Packages/TillerWorkspace/Sources/TillerWorkspace/DividerCursorRects.swift:15`).

## Design

### 1. Zero-width seams, unchanged window margins

Move each column's horizontal padding to its outward side only. The split
container keeps its `.horizontal, cardGap / 2`.

| Column       | leading      | trailing     |
| ------------ | ------------ | ------------ |
| sidebar      | `cardGap / 2`| 0            |
| central pane | 0            | 0            |
| right panel  | 0            | `cardGap / 2`|

Window edges keep their full `cardGap` margin. The seams collapse to zero
(plus `NSSplitView`'s own ~1pt divider, handled in §3).

**`CardLayout.dividerCenterX` does not change.** Today the seam centre sits at
`gap/2 + sidebarFrameWidth`, where the measured frame includes both of the
sidebar's own paddings and the next column begins where that frame ends. With
asymmetric padding the frame includes only the leading half, but the next
column still begins where the frame ends — the same expression. Therefore
`App/CardLayout.swift` and the `DividerCursorStrip` offsets
(`App/ContentView.swift:248-266`) are left alone.

Side effect, cosmetic only: the sidebar's *content* gains 5pt of usable width
for the same frame width, because it no longer pays a trailing inset.
`AppSettings.sidebarWidthRange` is unchanged.

### 2. `FloatingCard` gains a `flushEdges` parameter

`FloatingCard` takes `flushEdges: Edge.Set` (default `[]`) which drives two
things at once:

- **Corners.** `clipShape` becomes an `UnevenRoundedRectangle` whose radius is
  0 on the corners belonging to a flush edge and `AppTheme.cardCornerRadius`
  elsewhere. Without this, two rounded corners meeting at a zero-width seam
  leave notches of canvas above and below it.
- **Shadow bleed.** `.compositingGroup()` then `.shadow(...)` then a
  `clipShape` of a rect grown by `AppTheme.cardShadowRadius * 2` on the
  non-flush edges and by 0 on the flush ones. This is what actually removes the
  dark band.

Assignment:

- central card: `.leading` when `sidebarVisible`, `.trailing` when
  `rightPanelVisible`
- right panel card: `.leading` always
- sidebar: nothing — it is not a `FloatingCard`

Both shadow spills that are actually visible are on a card's *leading* edge:
the central card's, falling onto the sidebar's canvas, and the right panel's,
falling onto the central card. The central card's trailing spill is already
hidden because the right panel is a later sibling and draws over it; covering
it via `flushEdges` anyway keeps the rule single and avoids a sliver if the
two frames ever fail to meet exactly.

### 3. `dividerCover` becomes `dividerSeam` and carries the highlight

Rather than adding a view, the existing 2pt cover changes job. It stays 2pt
wide and centred on the seam, and its fill becomes conditional:

- at rest: `AppTheme.chatSurface` (`#28292C`) — reads as 2pt more of the
  central card, invisible
- on hover: `AppTheme.hairline` (`#40424F` dark, `#D1D4DE` light) — the grey
  line
- transition: `.easeInOut(duration: 0.12)`

One element covers both concerns: hiding `NSSplitView`'s hairline (which at a
zero-width seam would otherwise be the most visible black line in the window)
and drawing the hover highlight. `hairline` is the token every other boundary
in the app already uses, and it is lighter than both adjoining surfaces, so it
reads as relief rather than as a new dark line.

`dividerSeam` keeps `.allowsHitTesting(false)`.

### 4. Hover plumbing

`DividerCursorStripView` installs an `NSTrackingArea`
(`.mouseEnteredAndExited`, `.activeInKeyWindow`, `.inVisibleRect`) in an
`updateTrackingAreas()` override, and `DividerCursorStrip` exposes
`onHoverChange: (Bool) -> Void`. `ContentView` holds `@State` for
`leftSeamHovered` and `rightSeamHovered` and feeds them to the two
`dividerSeam` instances.

`hitTest` is not touched. Tracking areas deliver enter/exit directly to the
owning view rather than through hit testing, so clicks keep falling through to
the `NSSplitView` divider exactly as before.

Highlight persistence during a drag needs no extra work: the strip's offset is
bound to `sidebarWidth` / `liveRightPanelWidth`, so it tracks the moving
divider and stays under the pointer for the whole drag.

## Testing

Pure geometry only, added to `App/CardLayout.swift` and covered in the existing
`AppTests/CardGeometryTests.swift`:

1. `CardLayout.cardCorners(flushEdges:radius:)` — radius 0 on corners of a
   flush edge, `radius` elsewhere; `flushEdges == []` yields four equal radii.
2. `CardLayout.shadowBleed(flushEdges:radius:)` — zero inset on flush edges,
   `radius * 2` on the others.
3. Regression: the seam centre stays at `gap/2 + columnWidth` when a column's
   padding moves from symmetric to leading-only. This pins the invariant that
   lets §1 leave `dividerCenterX` untouched.

No new AppKit-level tests. `DividerCursorStripTests` is already recorded as
flaky in the CI gate, so the tracking-area change deliberately adds no test
surface there.

## Verification

Automated: `Scripts/ci.sh` must print `CI OK`. A red result is compared against
the known 2026-08-06 baseline by running the gate twice on the same tree, not
by comparing against `HEAD`.

Visual verification is performed manually by the user, not by the agent.
Checklist to hand over:

- [ ] At rest, neither seam shows a dark band.
- [ ] Hovering either seam shows the grey line and the `resizeLeftRight`
      cursor.
- [ ] Dragging either seam resizes as before, and the line stays lit for the
      whole drag.
- [ ] Hiding then re-showing the sidebar restores the central card's rounded
      leading corners.
- [ ] Hiding the right panel restores the central card's rounded trailing
      corners.
- [ ] Light mode: the seam is invisible at rest and the hover line is visible.

## Risks

The one assumption that only visual inspection can settle is `dividerSeam`'s
at-rest fill (§3). It assumes the central card's edge shows `chatSurface`; if
the content occupying those 2pt paints a different color, the strip will read
as a faint line and the token needs adjusting to match.
