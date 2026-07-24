# Surface color #1F1F26 + uniform translucency for terminal and chat

Date: 2026-07-24
Status: approved

## Goal

Terminal and chat surfaces adopt a new charcoal color `#1F1F26` (sRGB 0.122/0.122/0.149) and become translucent like the sidebar, so the whole app shares one perceived opacity. Sidebar, sheets, settings, and markdown editor keep the existing chrome color `#131313` and are not repainted.

## Design

### 1. Color — `AppSurfaceColor` (TillerCore)

- Replace the `terminal*` components with `0.122 / 0.122 / 0.149` (`#1F1F26`).
- Drop the ×0.7 "recessed" multiplier: terminal and chat use the identical color.
- `red/green/blue` (chrome `#131313`) stay unchanged; `AppTheme.background` continues to point at them for sheets/settings.

### 2. Terminal — `TillerTerminalTheme` (TillerTerminal)

Dark Ghostty configuration gains:

- `background = 1F1F26` (already wired via `AppSurfaceColor.terminalHex`)
- `background-opacity = 0.96`
- `background-blur-radius = 20` (starting value; may need visual tuning to match NSVisualEffectView blur)

### 3. Chat / main pane — `ContentView` (App)

The opaque `AppTheme.background` at `ContentView.swift:192` is replaced by a translucent surface reusing the `SidebarMaterialContainer` pattern (NSVisualEffectView + color tint overlay + reduced opacity), with tint `#1F1F26`. The window is already non-opaque (`WindowChromeConfigurator`), so no window-level change is needed.

### 4. Uniform opacity

One shared constant (0.96, the value the sidebar already uses) drives sidebar, terminal, and chat surface opacity.

## Out of scope

- Light mode palette (unchanged).
- Sheets, settings surfaces, markdown editor backgrounds (stay on chrome `AppTheme.background`).

## Verification

- `Scripts/ci.sh` prints `CI OK`.
- Manual smoke: terminal, chat, and sidebar side by side over a light wallpaper — blur and opacity read uniform; terminal text contrast acceptable.
