# Sidebar and Central Pane Palette Overlay Design

## Goal

Refresh Tiller's application surfaces so the two sidebars read as one chrome layer and the central chat/terminal area reads as a raised surface, matching the supplied light and dark reference screenshots.

## Approved visual contract

The following values are exact and must not be approximated by per-view colors:

| Semantic surface | Dark appearance | Light appearance |
| --- | --- | --- |
| Sidebar / primary chrome (both sidebars) | `#08090A` | `#E9EAED` |
| Central chat / terminal pane | `#101112` | `#F6F6F8` |

The right panel uses the same sidebar/chrome token as the left sidebar. There is no third right-panel color in this change. Existing text, Git-status, diff-highlight, and accent colors remain unchanged unless verification exposes a contrast regression.

## Layout treatment

The central column keeps its existing `HSplitView` geometry and divider hit regions. The visual surface inside that column becomes a floating card:

- keep the outer column and divider overlays in their current positions;
- inset the central content inside the column with consistent horizontal and vertical padding;
- apply an adaptive rounded rectangle, a subtle one-point edge, and a soft appearance-aware shadow;
- leave the sidebar/chrome surface visible in the inset area, so the center visibly sits above it;
- keep chat, terminal, tab chrome, usage bar, and pane content inside the same central surface where they currently belong;
- do not change split fractions, divider widths, cursor strips, drag tracking, or pane lifecycle behavior.

The card treatment is structural rather than an animation: it must remain stable while the user resizes panes. Any existing translucent-material setting may continue to affect the card's material, but it must not change the semantic colors above when translucency is disabled.

## Architecture and ownership

`AppSurfaceColor` remains the package-level source for numeric surface constants shared with the terminal theme. `AppTheme` exposes the appearance-aware semantic colors consumed by SwiftUI. The implementation should add or update the smallest set of semantic tokens necessary for:

- sidebar/primary chrome;
- central chat surface;
- central terminal surface (kept equal to the chat surface);
- card border and shadow treatment.

`ContentView` remains responsible for column composition and divider overlays. The floating-card decoration should be isolated in a small internal view or modifier so the HSplitView structure stays readable and geometry-sensitive code is not duplicated. `SidebarMaterialContainer` and `MainSurfaceMaterial` should continue to provide material behavior, with their tints sourced from the semantic tokens rather than hard-coded colors.

## Verification and tests

1. Add or update deterministic color-token tests so both dark and light hex values are asserted, including the equality of chat and terminal surface values.
2. Preserve and run the existing workspace divider and geometry tests to prove the inset card does not change hit regions or split calculations.
3. Build the macOS target and run the repository CI gate (`Scripts/ci.sh`).
4. Launch the built app and inspect both appearance modes. Verify that the sidebar/chrome colors, central surface colors, rounded inset, border, and shadow match the approved contract.

## Non-goals

- changing divider dimensions or cursor behavior;
- redesigning typography, icons, Git colors, or diff-line semantics;
- changing window size, sidebar widths, or right-panel persistence;
- introducing a separate color family for the right panel;
- changing terminal content colors beyond the terminal surface background.

## Acceptance criteria

- In dark appearance, both sidebars/chrome render as `#08090A` and the central chat/terminal surface renders as `#101112`.
- In light appearance, both sidebars/chrome render as `#E9EAED` and the central chat/terminal surface renders as `#F6F6F8`.
- The central surface is visibly inset and elevated with rounded corners, a subtle edge, and a soft shadow in both appearances.
- Divider drag and cursor hit behavior are unchanged.
- Automated tests and `Scripts/ci.sh` pass, and the built app launches for visual inspection.
