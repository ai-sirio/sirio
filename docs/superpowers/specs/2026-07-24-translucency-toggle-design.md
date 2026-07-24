# Translucency Toggle — Design

**Date:** 2026-07-24
**Status:** Approved (pending implementation)

## Context

Tiller's UI is translucent by default: the sidebar/titlebar/usage bar use an `NSVisualEffectView` material at 96% opacity, the main pane uses a charcoal overlay at 96% opacity, and dark-mode terminal surfaces are configured in ghostty with `background-opacity 0.96` + `background-blur-radius 20`. The requested change inverts the default: fully opaque UI out of the box, with translucency as an opt-in setting that applies **live everywhere** — sidebar, main pane, and all open terminal panes — using the same propagation mechanics as the terminal font size.

## Goals

- Opaque UI by default; translucency opt-in via Settings → Appearance.
- Toggling the setting updates every surface instantly, no restart.
- Reuse existing machinery (`@AppStorage` + `setTheme`); no new infrastructure.

## Non-goals

- No opacity slider — binary toggle only.
- No changes to `WindowChromeConfigurator` (window stays `isOpaque = false` / clear background — harmless under fully opaque content).
- No changes to translucent *color alphas* in chat views (`ChatComposerView`, `ToolCallCardView`, etc.) — those are not window transparency.
- Light-mode terminal stays as-is (the `.alabaster` preset is already opaque).

## Current state

The single constant `AppSurfaceColor.surfaceOpacity = 0.96` (`Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift:28`) drives three surfaces:

1. `SidebarMaterialContainer` (`App/SidebarMaterialContainer.swift`): `NSVisualEffectView` (material `.sidebar`, blending `.behindWindow`) + `AppTheme.chromeTint.opacity(0.30)` overlay + `.opacity(surfaceOpacity)`. Used for the sidebar, the titlebar strip, and the usage bar (`App/ContentView.swift`, `App/UsageBarView.swift`).
2. `MainSurfaceMaterial` (same file): material view + `AppTheme.terminalSurface.opacity(surfaceOpacity)` overlay behind the terminal/chat main pane.
3. Ghostty dark config (`Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`): `.afterglow` preset + background `#1F1F26` + custom `background-opacity 0.96` + custom `background-blur-radius 20`. The light config (`.alabaster`) has no opacity settings.

Live-update machinery already exists: pane views (`PtyTerminalPane`, `ExecTerminalPane`) hold `@AppStorage(AppSettings.terminalFontSizeKey)` and call `state.setTheme(TillerTerminalTheme.theme(fontSize:))` from `.onAppear` and `.onChange` — `setTheme` re-applies the full light+dark configuration to the live ghostty surface.

Settings are defined in `AppSettings` (TillerCore): static key constants plus resolver functions (e.g. `autoNamingEnabled(defaultsValue:)`, where missing = false). The settings UI is `App/AppearanceSettingsView.swift` (grouped Form with Theme / Terminal / Files sections).

Opaque-fill color tokens already exist in `App/AppTheme.swift`: `AppTheme.background` (adaptive; dark = `#131313` chrome) and `AppTheme.terminalSurface` (adaptive; dark = `#1F1F26`).

## Design

### 1. Value resolution — TillerCore

`AppSettings`:
- New key: `public static let translucencyEnabledKey = "appearance.translucencyEnabled"`.
- New resolver: `public static func translucencyEnabled(defaultsValue: Any?) -> Bool` returning `(defaultsValue as? Bool) ?? false` (missing or wrong type → opaque). Same opt-in pattern as `autoNamingEnabled`.

`AppSurfaceColor`:
- The `surfaceOpacity` constant stays `0.96`.
- New: `public static func surfaceOpacity(translucencyEnabled: Bool) -> Double` returning `translucencyEnabled ? surfaceOpacity : 1.0`.

### 2. Terminal theme — TillerTerminal

`TillerTerminalTheme.theme(fontSize:)` becomes `theme(fontSize:translucencyEnabled:)`:
- Dark config, toggle ON: unchanged (`background-opacity 0.96`, `background-blur-radius 20`).
- Dark config, toggle OFF: omit both `background-opacity` and `background-blur-radius` custom commands (ghostty defaults to fully opaque, no blur).
- Light config: unchanged.
- `current(defaults:)` additionally reads the flag via `AppSettings.translucencyEnabled(defaultsValue: defaults.object(forKey: AppSettings.translucencyEnabledKey))`.
- All callers are updated (`PtyTerminalPane`, `ExecTerminalPane`, tests).

### 3. SwiftUI chrome — App

`SidebarMaterialContainer` and `MainSurfaceMaterial` gain `@AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false`; the body becomes conditional:
- ON: current stack (`SidebarMaterialView` + tint overlay at 0.30, container `.opacity(0.96)`; `MainSurfaceMaterial`: material view + `terminalSurface.opacity(0.96)`).
- OFF: flat adaptive fill with no material view at all — `AppTheme.background` for `SidebarMaterialContainer` (sidebar/titlebar/usage bar), `AppTheme.terminalSurface` for `MainSurfaceMaterial`.

Rationale: the `.sidebar` `NSVisualEffectView` is inherently see-through, so setting opacity to 1 is not enough — opaque mode swaps the material for a solid fill. `WindowChromeConfigurator` is untouched.

### 4. Live propagation to panes — App

`PtyTerminalPane` and `ExecTerminalPane`:
- Add `@AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false`.
- Rename `applyFontSize()` → `applyTheme()`, calling `state.setTheme(TillerTerminalTheme.theme(fontSize: Float(AppSettings.clampTerminalFontSize(terminalFontSize)), translucencyEnabled: translucencyEnabled))`.
- Wire `.onAppear { applyTheme() }`, `.onChange(of: terminalFontSize) { _, _ in applyTheme() }`, and `.onChange(of: translucencyEnabled) { _, _ in applyTheme() }`.

### 5. Settings UI — App

`AppearanceSettingsView`: in the **Theme** section, add `Toggle("Translucency", isOn: $translucencyEnabled)` backed by `@AppStorage(AppSettings.translucencyEnabledKey)`.

### Data flow

Toggle → `UserDefaults` → `@AppStorage` invalidates (a) the chrome views → SwiftUI re-renders with the solid fill; (b) every open pane → `setTheme` → the ghostty surface is reconfigured live. No restart, no new channels.

### Error handling

Nothing fallible: the resolver coerces missing/wrong-type values to `false`; a Bool needs no clamping; `setTheme` is the already-exercised live path.

## Testing

- TillerCore (`AppSettingsTests` / `AppSurfaceColorTests`): resolver missing → false, explicit true/false; `surfaceOpacity(translucencyEnabled:)` → 1.0 / 0.96. The existing `surfaceOpacity == 0.96` assertion stays valid.
- TillerTerminal (`TillerTerminalThemeTests`): dark + ON → exact current command list; dark + OFF → no `background-opacity` / `background-blur-radius` entries; light unchanged in both modes.
- App: no view unit tests (repo convention); manual visual verification (sidebar, main pane, terminals, light/dark, toggle live).
