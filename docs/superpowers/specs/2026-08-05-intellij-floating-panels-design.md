# IntelliJ-style floating panels

Date: 2026-08-05

## Goal

Rebuild the main window so every panel — sidebar, central pane, right panel —
reads as a card resting on a darker canvas, the way IntelliJ's tool windows sit
inside the IDE frame. The canvas is visible as a uniform frame on all four
sides, including a strip above the cards where the traffic lights live.

The current window has only one floating surface (the central pane) and paints
the chrome material behind everything else, so the sidebar and right panel run
edge to edge. That one-card arrangement is what changes.

## Decisions

Each of these was chosen explicitly; the alternatives are recorded because they
are the ones an implementer would otherwise re-open.

| Decision | Chosen | Rejected |
|---|---|---|
| Top edge | Cards start below a 28pt canvas strip | Cards flush to `y=0` (today's geometry) |
| Frame | Canvas gap only, no drawn border | 1pt hairline per card; gap + hairline |
| Usage bar | Text directly on the canvas | A fourth card; today's full-width bar |
| Card colour | Identical to the sidebar chrome | Central pane keeps its own lighter surface |
| Window chrome | `.hiddenTitleBar`, custom 28pt strip | Native toolbar (~38pt, cannot reach 28) |

## Geometry

```
ZStack
├── CanvasBackground              #131417 dark / #DCDDE2 light, ignoresSafeArea
└── VStack(spacing: 0)
    ├── TitleStrip (28pt)         traffic lights + 3 chrome buttons, on canvas
    ├── HSplitView
    │   ├── FloatingCard { SidebarView }
    │   ├── FloatingCard { workspaceStack }
    │   └── FloatingCard { RightPanelView }
    └── UsageBarView (30pt)       text on canvas, no surface
```

`FloatingMainSurface` generalises into `FloatingCard`: same shadow, but a
`RoundedRectangle` on all four corners instead of the bottom-only
`UnevenRoundedRectangle`, and no `.strokeBorder`. The trick where the border was
drawn 1pt taller so its top edge fell outside the clip goes away with it — it
existed only because the card touched the window's top edge.

### Tokens (`AppTheme`)

| Token | Value | Replaces |
|---|---|---|
| `cardCornerRadius` | 6 | `mainSurfaceCornerRadius` (18) |
| `cardGap` | 10 | `mainSurfaceHorizontalInset` (10), `mainSurfaceVerticalInset` (12) |
| `cardShadowRadius` | 18 | `mainSurfaceShadowRadius` |
| `cardShadowYOffset` | 6 | `mainSurfaceShadowYOffset` |
| `titleStripHeight` | 28 | — (new) |

`mainSurfaceBorder` is deleted.

One gap value serves the space between cards, the left and right margins, and
the margin below the split. A different outer margin on those edges would stop
the frame reading as uniform, which is the whole point of the borderless choice.

The top band is the exception: it is `titleStripHeight` (28pt), not `cardGap`,
because the traffic lights need that clearance. The frame is therefore thicker
above than at the sides — as in the approved mockup, and as in IntelliJ, where
the window controls also get their own band.

### Building the gap

`HSplitView` has no `spacing`, so the gap is padding and the divider lives
inside it:

- each column: `.padding(.horizontal, cardGap / 2)`
- the split container: `.padding(.horizontal, cardGap / 2)`
- `.padding(.bottom, cardGap)` below the split, above the usage bar

`DividerCursorStrip` positions itself from `sidebarWidth` and
`liveRightPanelWidth`. `sidebarWidth` is measured with `onGeometryChange` on
`SidebarView` itself, which does not include padding applied inside the column —
measuring the padded column instead is required, or the resize cursor lands 5pt
off the visible gap.

## Colours

`AppSurfaceColor` (TillerCore) stays the single source of truth. It currently
holds two pairs (chrome and pane); it collapses to one pair plus the canvas.

| Role | Dark | Light |
|---|---|---|
| Card (sidebar, central pane, right panel) | `#1B1C1F` | `#E9EAED` |
| Canvas | `#131417` | `#DCDDE2` |

In light appearance the canvas is *darker* than the cards, mirroring the dark
relationship. A lighter canvas would make the cards sink instead of rest.

`terminalRed/Green/Blue` and `chatRed/Green/Blue` become aliases of the chrome
components rather than values of their own. They are not deleted:
`TillerTerminal` derives the Ghostty theme background from
`AppSurfaceColor.chatHex`/`hex`, and repointing them makes the terminal follow
without touching a package unrelated to layout.

`AppTheme.cardFill` (`#343539`, the chat transcript cards) is unchanged, but it
now sits two steps above its background instead of one. This is a visual change
nobody asked for; it is checked in QA rather than pre-emptively corrected.

## Window chrome

`.windowToolbarStyle(.unifiedCompact)` becomes `.windowStyle(.hiddenTitleBar)`
(`App/TillerApp.swift`). The safe-area top inset drops to 0 and the 28pt strip
becomes ours to draw. 28pt is the height of the standard macOS titlebar — the
clearance the traffic lights need — and is unreachable while a native toolbar
holds buttons.

Removed, not replaced:

- `.toolbar { … }` and `.toolbarBackgroundVisibility` (`App/ContentView.swift`)
- `titlebarInset`, its `onGeometryChange`, and `.padding(.top, titlebarInset)`
- both `.ignoresSafeArea(.container, edges: .top)` calls — they existed only to
  fight the safe area that no longer exists
- `SidebarView`'s `.toolbar`

`TitleStrip` replaces them: a 28pt row with a ~78pt leading spacer for the
traffic lights (AppKit still draws them), then the three chrome buttons — toggle
sidebar, split menu, permissions — using the existing `HoverIconButtonStyle`.

`WindowChromeConfigurator` is unchanged: `fullSizeContentView`,
`titlebarAppearsTransparent`, and the non-opaque window are all still required.

## Sidebar header

The sidebar has no heading today; it starts with `FilterField`. The Add Project
button loses its home when the toolbar goes, so it moves into a new header row:

```
┌─────────────────────┐
│ Projects        [+] │
│ ⌕ filter…           │
├─────────────────────┤
│ ▾ tiller            │
```

`Projects` uses `AppTheme.meta`, matching the existing section labels. The `+`
uses `HoverIconButtonStyle` and keeps its current action
(`showAddProjectSheet = true`). The header sits above the filter: the filter
acts on the projects, so it belongs under the label that names them.

## Translucency

The translucency setting is opt-in and defaults off. When on, the canvas takes
the `NSVisualEffectView` material and the cards keep their solid tint, so the
desktop shows through the frame and not through the terminal.

## Verification

Automated (swift-testing, AppTests) — `FloatingMainSurfaceTests` updated in
place, not a new file:

- the five tokens hold their approved values
- one-surface invariant: the terminal and chat components equal the chrome ones
- the canvas is darker than the card in both appearances — the relationship, not
  a fixed value

Any test asserting the Ghostty theme hex values will fail. That failure is
correct and documents the change; the expectations get updated.

Manual, after the build:

1. traffic lights clear and centred in the 28pt strip
2. resize cursor aligned to the gap on both dividers
3. translucency on: desktop visible through the frame, not the terminal
4. light mode: cards rest on the canvas rather than sinking into it
5. usage bar legible on bare canvas — the weak point of that choice
6. right panel hidden: the frame stays uniform

## Out of scope

- The narrow strip at the far left holds nothing. It is canvas showing through
  the margin, not a component.
- No icon rail, no new tool-window buttons.
- Tab placement is unchanged: tabs already live inside the central card.
