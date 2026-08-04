# Airy Graphite Dark Palette — Design

**Date:** 2026-08-04
**Status:** Approved (pending implementation)

## Goal

Replace Tiller's current default dark palette with the approved A3 **“Più
ariosa”** neutral graphite palette. This is one complete dark-only palette
retune: it changes the neutral surface and text hierarchy without introducing
a selectable variant or changing layout, behavior, or the existing light mode.

## Approved visual contract

| Semantic role | Dark value |
| --- | --- |
| Chrome and sidebar | `#1B1C1F` |
| Central chat and terminal | `#28292C` |
| Elevated card and control | `#343539` |
| Primary text | `#F2F3F5` |
| Secondary text | `#A8ABB2` |

The central chat and terminal surfaces must remain identical. The palette is
neutral graphite rather than blue- or brown-shifted. There is no user-facing
choice between A1, A2, A3, or any other dark palette.

The following remain unchanged:

- light-mode values and behavior;
- accent colors and semantic status colors, including Git and diff colors;
- geometry, layout, split behavior, and component placement;
- shared surface opacity `0.96`;
- Ghostty blur radius `20`;
- terminal ANSI and afterglow colors.

Hover, selection, hairline, and border tokens stay unchanged in this
implementation. If manual validation finds a legibility problem in one of
those tokens, completion is blocked and a separate explicit palette decision
is requested; no unapproved neutral value is invented in this scope.

## Rationale and alternatives

The current dark neutral hierarchy is too compressed and visually dense for a
workspace that places sidebar chrome, chat, terminals, cards, and controls next
to one another. A3 increases separation between the three neutral surface
levels while keeping the interface quiet: chrome is the darkest layer, the
working surface is visibly lighter, and cards and controls sit one step above
the working surface. The lighter primary and secondary text values preserve a
clear reading hierarchy without adding color noise.

Considered alternatives:

- **Warm graphite:** rejected because its brown/red bias competes with accent
  and status colors and changes Tiller's neutral identity.
- **Cool slate:** rejected because its blue bias makes the interface feel
  tinted rather than neutral and reduces separation from existing accents.
- **A1/A2 brightness variants:** rejected because they do not provide A3's
  more open surface hierarchy. A3 is the approved balance of airiness,
  contrast, and restraint.

## Architecture and data flow

`AppSurfaceColor` remains the cross-package source of truth for shared chrome,
chat, and terminal surface values. It owns the numeric RGB components and hex
representations consumed by TillerCore tests and by the Ghostty theme in
TillerTerminal. The terminal surface token continues to equal the chat surface
token in both appearances.

`AppTheme` remains the App-target source for appearance-aware SwiftUI semantic
colors. Its dark mappings consume the shared surface values where the token is
cross-package, and expose the approved elevated and text roles to SwiftUI
components. Its light mappings are preserved exactly.

The data flow is:

`AppSurfaceColor` → `AppTheme` and `TillerTerminalTheme` → SwiftUI chrome/chat
and Ghostty terminal rendering.

The existing `0.96` opacity path remains in place for sidebar material, main
surface material, and terminal configuration. The existing Ghostty
`background-blur-radius 20` configuration remains in place. No new palette
model, preference, migration, or runtime configuration path is introduced.

Direct `.quaternary` fills are not globally replaced. They are changed only
when the fill represents a targeted elevated card or control surface, so those
components use the approved `#343539` semantic role. Unrelated system fills
remain untouched.

## Impacted component categories and direct audit files

The implementation is limited to the following categories and named files:

- **Shared palette constants and exact-value tests**
  - `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`
  - `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`
- **Appearance-aware SwiftUI semantic colors**
  - `App/AppTheme.swift`
- **Shared chrome and central surface composition**
  - `App/SidebarMaterialContainer.swift`
  - `App/ContentView.swift`
  - `App/FloatingMainSurface.swift`
- **Direct-fill audit candidates**
  - `App/ChatComposerView.swift`
  - `App/ComposerControlBar.swift`
  - `App/TranscriptView.swift`
  - `App/ComposerChipAttachmentView.swift`
  - `App/ModelPickerPopover.swift`
  - `App/AIProvidersSettingsView.swift`
  - `App/CodeBlockStyle.swift`
  - each candidate is changed only if its existing direct fill semantically
    represents the approved elevated/card/control role
- **Ghostty terminal surface configuration and exact-value tests**
  - `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`
  - `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift`

The implementation must preserve package boundaries: TillerCore owns shared
values, TillerTerminal consumes them for Ghostty, and App maps them into
appearance-aware SwiftUI colors. No unnamed view is included in this change.

## In scope

- Retune the default dark neutral palette to the five approved value roles.
- Keep chat and terminal dark surfaces equal.
- Map shared surfaces through `AppSurfaceColor` and `AppTheme` rather than
  duplicating per-view hex values.
- Retune targeted elevated cards and controls to `#343539`.
- Retune the primary and secondary neutral text roles to the approved values.
- Update exact RGB/hex expectations in TillerCore and TillerTerminal tests.
- Preserve the existing opacity, Ghostty blur, terminal ANSI/afterglow colors,
  light palette, accents, semantic colors, and geometry.
- Validate dark and light appearances manually after automated verification.

## Out of scope

- A palette picker, variant selector, or persisted palette preference.
- Any light-mode palette or appearance behavior change.
- Layout, spacing, sizing, split fractions, pane lifecycle, or window changes.
- Changes to accent, Git, diff, status, ANSI, or afterglow colors.
- Changes to opacity or blur behavior.
- Global replacement of `.quaternary`.
- Broad hover, selection, hairline, or border redesign.
- Runtime migration, compatibility handling, or an error path: this is a
  compile-time palette update with no persisted data shape change.

## Test-first strategy

1. Update the deterministic TillerCore palette tests to assert the approved
   dark chrome, chat, and terminal values, plus the preserved equality
   relationship.
   Preserve assertions for the unchanged light values and assert that terminal
   and chat remain equal.
2. Update TillerTerminal exact theme expectations to use the new dark terminal
   surface while preserving the afterglow configuration, `0.96` opacity, blur
   radius `20`, light configuration, and ANSI behavior.
3. Do not introduce AppTheme or view-test infrastructure: existing palette
   coverage is package-token based. Validate exact AppTheme text and elevation
   rendering manually in the built app.
4. Run the focused TillerCore and TillerTerminal tests while iterating.
5. Run `Scripts/ci.sh` as the repository verification gate.
6. Manually inspect the built app in dark and light modes, including sidebar
   chrome, central chat, terminal panes, elevated cards, controls, text, hover,
   selection, borders, and status/accent colors.

## Acceptance criteria

- Default dark chrome and sidebar surfaces render as `#1B1C1F`.
- Default dark chat and terminal surfaces render identically as `#28292C`.
- Targeted elevated cards and controls render as `#343539`; unrelated
  `.quaternary` fills are not changed automatically.
- Primary dark text renders as `#F2F3F5` and secondary dark text as `#A8ABB2`.
- Light-mode values and appearance remain unchanged.
- Accent, semantic status, Git, diff, ANSI, and afterglow colors remain
  unchanged.
- Surface opacity remains `0.96`, and Ghostty blur radius remains `20`.
- Geometry, layout, pane behavior, and package ownership remain unchanged.
- TillerCore and TillerTerminal exact-value tests pass, including chat-terminal
  equality and preserved light values.
- `Scripts/ci.sh` passes.
- Manual inspection confirms the dark palette is more spacious and legible
  without introducing a warm or cool tint, and light mode has not regressed.
