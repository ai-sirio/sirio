# Appearance Settings & Light Mode — Design

**Date:** 2026-07-11
**Status:** Approved

## Goal

Add light mode support and an Appearance section in Settings. The user can
switch between System / Light / Dark and adjust the terminal font size. Both
take effect live, no restart.

## Current State

- Dark is forced in two places: `TillerApp.swift` (`.preferredColorScheme(.dark)`)
  and `SidebarMaterialContainer.swift` (`NSAppearance(named: .darkAqua)` on the
  material view).
- `App/AppTheme.swift` defines 13 hardcoded dark color tokens (`static let`).
- `AppSurfaceColor` (TillerCore) holds the shared dark surface RGB; the terminal
  dark background derives from it.
- Ghostty is already light-ready: `TillerTerminalTheme` defines
  `TerminalTheme(light: .alabaster, dark: .afterglow + background override)`,
  and `TerminalSurfaceView` observes `\.colorScheme` and adopts on change.
  `TerminalViewState.setTheme(_:)` supports live config updates.
- Settings scaffold exists: `SettingsSurface` already has an `.appearance`
  category rendering a placeholder `AppearanceSettingsView`.

## Decisions

- Theme selector: **System / Light / Dark** (3-way).
- Appearance section contents: **theme + terminal font size** only. Terminal
  color-theme picker, accent color, etc. deferred.
- Light palette: **hand-tuned** neutral cool grays with a subtle indigo tint,
  mirroring the existing dark chrome identity (not system semantic colors, not
  sampled from Orca).
- Adaptive color mechanism: **`NSColor(name:dynamicProvider:)`** per token
  (approach chosen over an Environment-injected palette struct — zero call-site
  changes for 2 themes; palette struct is YAGNI until N custom themes exist).

## Design

### 1. Theme model and application

- New `AppAppearance: String, CaseIterable` enum in the App target:
  `.system / .light / .dark`, persisted via `@AppStorage("appearance.theme")`
  (same pattern as existing settings keys).
- Applying: `NSApp.appearance = nil` (system) or
  `NSAppearance(named: .aqua / .darkAqua)`. Applied at launch and on change of
  the stored value — centralized in `TillerApp`, the settings view only writes
  the preference.
- Removals: `.preferredColorScheme(.dark)` in `TillerApp.swift`; the forced
  `.darkAqua` in `SidebarMaterialContainer.swift` (the material follows the
  global appearance).
- Terminal font size: `@AppStorage("appearance.terminalFontSize")`, default 13,
  range 9–24.

### 2. Adaptive AppTheme palette

Private helper in `AppTheme`:

```swift
private static func dynamic(light: NSColor, dark: NSColor) -> Color {
    Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
    })
}
```

Every token becomes `dynamic(light:dark:)`. Dark values unchanged. Light
starting values (refined visually after first build):

| Token | Light (r/g/b) |
|---|---|
| background | 0.965 / 0.965 / 0.975 |
| chromeTint | 0.92 / 0.92 / 0.96 |
| hairline | 0.82 / 0.83 / 0.87 |
| rowHover | 0.90 / 0.905 / 0.93 |
| selectionFill | 0.85 / 0.86 / 0.91 |
| selectionRing | 0.72 / 0.74 / 0.82 |
| title | 0.15 / 0.16 / 0.20 |
| titleSelected | 0.05 / 0.05 / 0.08 |
| subtitle | 0.35 / 0.37 / 0.45 |
| meta | 0.42 / 0.44 / 0.52 |
| primaryPillBg | 0.88 / 0.885 / 0.92 |
| filterFieldBg | white |
| treeGuide | black @ 12% |

Terminal in light mode uses the stock `.alabaster` preset (light paper
background, slight contrast step from the chrome — intentional, mirrors dark).

### 3. Terminal font size wiring

- `TillerTerminalTheme.theme` (static let) becomes
  `static func theme(fontSize: Float) -> TerminalTheme`, applying `.fontSize`
  to both light and dark configurations.
- The `"appearance.terminalFontSize"` key lives as a constant in TillerCore
  (TillerTerminal already imports TillerCore; the App target reuses it in the
  settings UI).
- `PtyTerminalPane` / `ExecTerminalPane`: `@AppStorage` on the key, initialize
  `TerminalViewState` with the current size, then `.onChange` + `.onAppear`
  call `state.setTheme(...)` — live updates, including cached panes
  (`TerminalPaneCache`) when they become visible again. `setTheme` regenerates
  the Ghostty config on the live surface; the PTY is untouched.

### 4. Settings UI

`AppearanceSettingsView` replaces the placeholder, styled consistently with
`GeneralSettingsView`:

- **Theme**: segmented `Picker` — System / Light / Dark, bound to
  `@AppStorage("appearance.theme")`.
- **Terminal**: font size `Stepper` with value label ("13 pt"), range 9–24.

## Testing

- Unit: `AppAppearance` → `NSAppearance.Name?` mapping (nil / aqua / darkAqua);
  `TillerTerminalTheme.theme(fontSize:)` renders `font-size = N` in both
  configurations (TillerTerminal package, existing test infra).
- Manual checklist (post-build): live theme switch without restart; sidebar
  material correct in both themes; terminal switches alabaster↔afterglow; font
  size updates live on active and cached panes; preferences persist across
  relaunch.

## Out of Scope

- Terminal color-theme picker (preset selection)
- Accent color, sidebar transparency, font family
- Per-pane font size
