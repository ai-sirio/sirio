# Translucency Toggle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make window translucency opt-in via a new `appearance.translucencyEnabled` setting (default: opaque), applied live to the sidebar, main surface, and all open terminal panes.

**Architecture:** A single UserDefaults flag resolved through `AppSettings`; `AppSurfaceColor` exposes opacity as a function of the flag; `TillerTerminalTheme` conditionally appends the ghostty `background-opacity`/`background-blur-radius` commands; SwiftUI chrome views and terminal panes observe the flag via `@AppStorage` and re-render / re-apply the theme on change (the same live-update pattern already used for font size).

**Tech Stack:** Swift 6, SwiftUI (`@AppStorage`), SwiftPM packages (TillerCore, TillerTerminal), swift-testing, libghostty.

## Global Constraints

- Work in the worktree `/Users/enzopiopalmisano/Desktop/Progetti/tiller-translucency` on branch `feat/translucency-toggle`. All paths below are relative to it.
- Setting key: `appearance.translucencyEnabled`, default **false** (opaque), opt-in — mirror the existing `autoNaming.enabled` pattern.
- No changes to `App/WindowChromeConfigurator.swift`. Never touch user-global agent config.
- Translucent values stay exactly as today: surface opacity `0.96`, terminal `background-blur-radius` `"20"`, terminal dark background `#1F1F26`.
- Tests use swift-testing (`@Test` / `#expect`), never XCTest.
- Commit messages: Conventional Commits, lower-case imperative subject.
- Design spec: `docs/superpowers/specs/2026-07-24-translucency-toggle-design.md`.

---

### Task 1: AppSettings key + resolver

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift`

**Interfaces:**
- Consumes: nothing new.
- Produces: `AppSettings.translucencyEnabledKey: String` (`"appearance.translucencyEnabled"`) and `AppSettings.translucencyEnabled(defaultsValue: Bool?) -> Bool` (nil → false). Consumed by Tasks 3–6.

- [ ] **Step 1: Write the failing tests**

Append to `AppSettingsTests.swift` (match the file's existing flat `@Test func` style):

```swift
@Test func translucencyEnabledDefaultsToFalseWhenAbsent() {
    #expect(AppSettings.translucencyEnabled(defaultsValue: nil) == false)
}

@Test func translucencyEnabledReturnsStoredValue() {
    #expect(AppSettings.translucencyEnabled(defaultsValue: true) == true)
    #expect(AppSettings.translucencyEnabled(defaultsValue: false) == false)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter translucencyEnabled`
Expected: FAIL / build error — `translucencyEnabled` is not defined yet.

- [ ] **Step 3: Implement**

In `AppSettings.swift`, next to the existing `autoNamingEnabledKey` / `autoNamingEnabled(defaultsValue:)` pair, add:

```swift
public static let translucencyEnabledKey = "appearance.translucencyEnabled"

public static func translucencyEnabled(defaultsValue: Bool?) -> Bool {
    defaultsValue ?? false
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter translucencyEnabled`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSettings.swift Packages/TillerCore/Tests/TillerCoreTests/AppSettingsTests.swift
git commit -m "feat: add appearance.translucencyEnabled setting key"
```

### Task 2: AppSurfaceColor opacity function

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `AppSurfaceColor.translucentSurfaceOpacity: Double` (`0.96`) and `AppSurfaceColor.surfaceOpacity(translucencyEnabled: Bool) -> Double` (true → 0.96, false → 1.0). The legacy `AppSurfaceColor.surfaceOpacity: Double` property is kept as a computed var so existing callers (TillerTerminalTheme, SidebarMaterialContainer) still compile; it is removed in Task 7.

- [ ] **Step 1: Write the failing tests**

Append to `AppSurfaceColorTests.swift`:

```swift
@Test func surfaceOpacityIsOpaqueWhenTranslucencyDisabled() {
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: false) == 1.0)
}

@Test func surfaceOpacityMatchesLegacyValueWhenTranslucencyEnabled() {
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true) == 0.96)
    #expect(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true) == AppSurfaceColor.translucentSurfaceOpacity)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter surfaceOpacity`
Expected: FAIL / build error — new symbols not defined yet.

- [ ] **Step 3: Implement**

In `AppSurfaceColor.swift`, replace the line `public static let surfaceOpacity: Double = 0.96` with:

```swift
public static let translucentSurfaceOpacity: Double = 0.96

public static var surfaceOpacity: Double { translucentSurfaceOpacity }

public static func surfaceOpacity(translucencyEnabled: Bool) -> Double {
    translucencyEnabled ? translucentSurfaceOpacity : 1.0
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test`
Expected: PASS — new tests plus all pre-existing tests (the existing `sharedSurfaceOpacityIs096` still passes because the legacy property remains).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift
git commit -m "feat: make surface opacity a function of the translucency setting"
```

### Task 3: Gate terminal theme opacity/blur behind the setting

**Files:**
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`
- Test: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift`

**Interfaces:**
- Consumes: `AppSettings.translucencyEnabledKey`, `AppSettings.translucencyEnabled(defaultsValue:)` (Task 1); `AppSurfaceColor.surfaceOpacity(translucencyEnabled:)` (Task 2).
- Produces: `TillerTerminalTheme.theme(fontSize: Float, translucencyEnabled: Bool = true) -> TerminalTheme` — default `true` preserves existing terminal appearance for pre-Task-4 callers, and Task 4 passes the persisted value explicitly. `current(defaults:)` reads both the font-size key and the translucency key. Dark configuration when disabled: `afterglow + background + fontSize + scrollback-limit` only (opacity/blur commands omitted entirely). Light configuration never changes.

- [ ] **Step 1: Update existing tests and add failing tests**

In `TillerTerminalThemeTests.swift`: every existing call `theme(fontSize: X)` becomes `theme(fontSize: X, translucencyEnabled: true)` so current expectations stay valid unchanged. Then append:

```swift
@Test func themeOmitsOpacityAndBlurWhenTranslucencyDisabled() {
    let theme = TillerTerminalTheme.theme(fontSize: 13, translucencyEnabled: false)
    let expected = TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(.fontSize(13))
        .appending(TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144"))
    #expect(theme.dark == expected)
}

@Test func currentReadsStoredTranslucencyPreference() {
    let suiteName = "TillerTerminalThemeTests-\(UUID().uuidString)"
    let defaults = UserDefaults(suiteName: suiteName)!
    defer { defaults.removePersistentDomain(forName: suiteName) }
    defaults.set(true, forKey: AppSettings.translucencyEnabledKey)

    let theme = TillerTerminalTheme.current(defaults: defaults)

    let expected = TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(TerminalConfigCommand.custom(key: "background-opacity", value: "0.96"))
        .appending(TerminalConfigCommand.custom(key: "background-blur-radius", value: "20"))
        .appending(.fontSize(Float(AppSettings.defaultTerminalFontSize)))
        .appending(TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144"))
    #expect(theme.dark == expected)
}
```

If the existing test file builds expected configurations with a different style than `TerminalConfigCommand.custom(...)` appended inline, match the file's existing style exactly while keeping the same command sequence.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerTerminal && swift test`
Expected: build error / FAIL — `theme(fontSize:translucencyEnabled:)` does not exist yet.

- [ ] **Step 3: Implement**

Replace the full contents of `TillerTerminalTheme.swift` with (preserve any header comment lines the file already has):

```swift
import Foundation
import GhosttyTerminal
import TillerCore

enum TillerTerminalTheme {
    static func theme(fontSize: Float, translucencyEnabled: Bool = true) -> TerminalTheme {
        let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
        let light = TerminalConfiguration.alabaster
            .appending(.fontSize(fontSize))
            .appending(scrollbackLimit)
        var dark = TerminalConfiguration.afterglow
            .appending(.background(AppSurfaceColor.terminalHex))
        if translucencyEnabled {
            let backgroundOpacity = TerminalConfigCommand.custom(
                key: "background-opacity",
                value: "\(AppSurfaceColor.surfaceOpacity(translucencyEnabled: true))"
            )
            let backgroundBlur = TerminalConfigCommand.custom(key: "background-blur-radius", value: "20")
            dark = dark
                .appending(backgroundOpacity)
                .appending(backgroundBlur)
        }
        dark = dark
            .appending(.fontSize(fontSize))
            .appending(scrollbackLimit)
        return TerminalTheme(light: light, dark: dark)
    }

    static func current(defaults: UserDefaults = .standard) -> TerminalTheme {
        let storedFontSize = defaults.object(forKey: AppSettings.terminalFontSizeKey) as? Int ?? AppSettings.defaultTerminalFontSize
        let translucencyEnabled = AppSettings.translucencyEnabled(
            defaultsValue: defaults.object(forKey: AppSettings.translucencyEnabledKey) as? Bool
        )
        return theme(
            fontSize: Float(AppSettings.clampTerminalFontSize(storedFontSize)),
            translucencyEnabled: translucencyEnabled
        )
    }
}
```

Note: the appended-command order in the dark configuration MUST be `background` → (`background-opacity`, `background-blur-radius` only when enabled) → `fontSize` → `scrollback-limit`, matching the previous behavior when enabled. If `TerminalConfiguration` proves to be a reference type where `var` + reassignment misbehaves, restructure with a `let` ternary instead — but keep the exact same final command order.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerTerminal && swift test`
Expected: PASS (whole suite).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift
git commit -m "feat: gate terminal background opacity and blur behind translucency setting"
```

### Task 4: Live re-apply in terminal panes

**Files:**
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/ExecTerminalPane.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`

**Interfaces:**
- Consumes: `TillerTerminalTheme.theme(fontSize:translucencyEnabled:)` (Task 3), `AppSettings.translucencyEnabledKey` (Task 1).
- Produces: both pane views observe the flag and call `state.setTheme(...)` when it changes. No public API. These are SwiftUI view-wiring edits; the App target has no view tests, so `Scripts/ci.sh` is the verification gate (per the spec).

- [ ] **Step 1: Edit both pane files identically**

In EACH of `ExecTerminalPane.swift` and `PtyTerminalPane.swift`:

1a. Immediately below the existing line `@AppStorage(AppSettings.terminalFontSizeKey) private var terminalFontSize = AppSettings.defaultTerminalFontSize`, add:

```swift
@AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false
```

1b. Rename `applyFontSize` to `applyTheme` everywhere it appears in the file (declaration and all call sites, including inside `.onAppear` and `.onChange(of: terminalFontSize)`), and change the function body to:

```swift
private func applyTheme() {
    let size = Float(AppSettings.clampTerminalFontSize(terminalFontSize))
    state.setTheme(TillerTerminalTheme.theme(fontSize: size, translucencyEnabled: translucencyEnabled))
}
```

1c. Immediately after the existing `.onChange(of: terminalFontSize) { _, _ in applyTheme() }` modifier, add:

```swift
.onChange(of: translucencyEnabled) { _, _ in applyTheme() }
```

In `PtyTerminalPane.swift` the relevant current code is around lines 64-65 (`@AppStorage`), 136-137 (`.onAppear { applyFontSize() }` / `.onChange`), and 140-143 (`applyFontSize` body). `ExecTerminalPane.swift` is 28 lines with the same shapes. Do not touch anything else in these files.

- [ ] **Step 2: Verify**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/ExecTerminalPane.swift Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift
git commit -m "feat: re-apply terminal theme live when translucency changes"
```

### Task 5: Flat fills for sidebar and main surface

**Files:**
- Modify: `App/SidebarMaterialContainer.swift`

**Interfaces:**
- Consumes: `AppSettings.translucencyEnabledKey` (Task 1), `AppSurfaceColor.translucentSurfaceOpacity` (Task 2).
- Produces: translucency OFF → flat fills (`AppTheme.background` for the sidebar chrome, `AppTheme.terminalSurface` for the main surface — no `NSVisualEffectView` material in the hierarchy); translucency ON → today's exact material stack. No view tests exist for the App target — `Scripts/ci.sh` is the gate.

- [ ] **Step 1: Edit `SidebarMaterialContainer`**

In `App/SidebarMaterialContainer.swift`, inside `struct SidebarMaterialContainer`:

1a. Change the static constant line to:

```swift
static let backgroundOpacity = AppSurfaceColor.translucentSurfaceOpacity
```

1b. Add the stored property:

```swift
@AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false
```

1c. Replace the body (currently `SidebarMaterialView()` + `.overlay(AppTheme.chromeTint.opacity(Self.tintOpacity))` + `.opacity(Self.backgroundOpacity)` — match the file's exact current formatting when editing) with:

```swift
var body: some View {
    if translucencyEnabled {
        SidebarMaterialView()
            .overlay(AppTheme.chromeTint.opacity(Self.tintOpacity))
            .opacity(Self.backgroundOpacity)
    } else {
        AppTheme.background
    }
}
```

- [ ] **Step 2: Edit `MainSurfaceMaterial`**

In the same file, inside `struct MainSurfaceMaterial`, add the same `@AppStorage` property and replace the body (currently `SidebarMaterialView()` + `.overlay(AppTheme.terminalSurface.opacity(AppSurfaceColor.surfaceOpacity))`) with:

```swift
var body: some View {
    if translucencyEnabled {
        SidebarMaterialView()
            .overlay(AppTheme.terminalSurface.opacity(AppSurfaceColor.translucentSurfaceOpacity))
    } else {
        AppTheme.terminalSurface
    }
}
```

Do not modify `SidebarMaterialView` (the private `NSViewRepresentable`) or any other file. Consumers (`App/ContentView.swift`, `App/UsageBarView.swift`) need no changes — they instantiate these views with no arguments.

- [ ] **Step 3: Verify**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`.

- [ ] **Step 4: Commit**

```bash
git add App/SidebarMaterialContainer.swift
git commit -m "feat: use flat fills for sidebar and main surface when translucency is off"
```

### Task 6: Settings toggle

**Files:**
- Modify: `App/AppearanceSettingsView.swift`

**Interfaces:**
- Consumes: `AppSettings.translucencyEnabledKey` (Task 1).
- Produces: user-facing `Toggle("Translucency")` writing to `appearance.translucencyEnabled`.

- [ ] **Step 1: Edit `AppearanceSettingsView.swift`**

1a. Add next to the existing `@AppStorage` property declarations:

```swift
@AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false
```

1b. Inside `Section("Theme")`, directly below the existing Appearance `Picker`, add:

```swift
Toggle("Translucency", isOn: $translucencyEnabled)
```

- [ ] **Step 2: Verify**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`.

- [ ] **Step 3: Manual smoke check**

Build and run the app (`xcodegen generate`, then build/run `Tiller.xcodeproj`). Confirm: with a fresh launch the app is fully opaque; flipping the toggle ON makes the sidebar, main surface, and every open terminal pane translucent immediately (no restart); flipping it OFF returns everything to flat opaque.

- [ ] **Step 4: Commit**

```bash
git add App/AppearanceSettingsView.swift
git commit -m "feat: add translucency toggle to appearance settings"
```

### Task 7: Remove legacy surfaceOpacity property

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`

**Interfaces:**
- Consumes: Tasks 3 and 5 (all production callers now use `surfaceOpacity(translucencyEnabled:)` or `translucentSurfaceOpacity`).
- Produces: `AppSurfaceColor` no longer has the legacy `surfaceOpacity: Double` property.

- [ ] **Step 1: Confirm no production callers remain**

Run: `rg "surfaceOpacity" --glob "*.swift" Packages App`
Expected: only (a) `surfaceOpacity(translucencyEnabled:` function definitions/calls, (b) `translucentSurfaceOpacity` references, (c) the legacy `public static var surfaceOpacity` declaration, and (d) test references to the legacy property. If any production call site still reads the legacy property, migrate it to `translucentSurfaceOpacity` first.

- [ ] **Step 2: Remove the property and update tests**

Delete from `AppSurfaceColor.swift`:

```swift
public static var surfaceOpacity: Double { translucentSurfaceOpacity }
```

In `AppSurfaceColorTests.swift`, update the pre-existing `sharedSurfaceOpacityIs096` test (and any other reference to the legacy property) to assert against the new constant instead — expected value unchanged:

```swift
@Test func sharedSurfaceOpacityIs096() {
    #expect(AppSurfaceColor.translucentSurfaceOpacity == 0.96)
}
```

- [ ] **Step 3: Run tests**

Run: `cd Packages/TillerCore && swift test`
Expected: PASS.

- [ ] **Step 4: Full verification**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift
git commit -m "refactor: remove legacy surfaceOpacity constant"
```

### Final verification

- `Scripts/ci.sh` prints `CI OK` on the final commit.
- Manual smoke: fresh launch defaults to fully opaque; toggling Translucency in Settings → Appearance updates the sidebar, main surface, and all open terminal panes live in both directions.
