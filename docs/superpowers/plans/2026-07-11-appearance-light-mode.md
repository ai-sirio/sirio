# Appearance Settings & Light Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Light mode support and an Appearance section in Settings: System / Light / Dark theme switch plus terminal font size, both live without restart.

**Architecture:** Theme selection is persisted in UserDefaults and applied globally via `NSApp.appearance` (nil = follow macOS). All 13 `AppTheme` color tokens become appearance-adaptive via `NSColor(name:dynamicProvider:)` — zero call-site changes. The Ghostty terminal already observes `\.colorScheme` and has a light config (`.alabaster`); font size is injected into both light/dark `TerminalConfiguration`s and pushed live with `TerminalViewState.setTheme(_:)`.

**Tech Stack:** SwiftUI + AppKit (macOS), libghostty-spm (GhosttyTerminal), Swift Testing (`import Testing`, `@Test`, `#expect`), XcodeGen project.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-07-11-appearance-light-mode-design.md`
- UserDefaults keys: `"appearance.theme"` and `"appearance.terminalFontSize"` (exact strings).
- Terminal font size: default 13, allowed range 9–24 (clamped).
- Dark palette values must remain exactly as today; only light variants are added.
- Every Swift view in the App target uses Inject (`@ObserveInjection var inject` + `.enableInjection()`) — preserve this pattern when editing views.
- Full verification gate: `Scripts/ci.sh` (regenerates project via xcodegen, builds app, runs all package tests). Known flake: `PtyProcessTests.spawnCapturesOutput` in TillerTerminal — one retry is enough.
- Package tests can be run individually: `cd Packages/<Pkg> && swift test`.

---

### Task 1: TillerCore — AppAppearance enum + appearance settings constants

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/AppAppearance.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSettings.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppAppearanceTests.swift`

**Interfaces:**
- Consumes: nothing (Foundation-only, like the rest of TillerCore).
- Produces:
  - `public enum AppAppearance: String, CaseIterable, Sendable { case system, light, dark }` with `public var title: String` ("System" / "Light" / "Dark").
  - `AppSettings.appearanceThemeKey: String == "appearance.theme"`
  - `AppSettings.terminalFontSizeKey: String == "appearance.terminalFontSize"`
  - `AppSettings.defaultTerminalFontSize: Int == 13`
  - `AppSettings.terminalFontSizeRange: ClosedRange<Int> == 9...24`
  - `AppSettings.clampTerminalFontSize(_ size: Int) -> Int`

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerCore/Tests/TillerCoreTests/AppAppearanceTests.swift`:

```swift
import Testing
@testable import TillerCore

@Test func appAppearanceRawValuesAreStable() {
    #expect(AppAppearance.system.rawValue == "system")
    #expect(AppAppearance.light.rawValue == "light")
    #expect(AppAppearance.dark.rawValue == "dark")
}

@Test func appAppearanceCasesOrderIsSystemLightDark() {
    #expect(AppAppearance.allCases == [.system, .light, .dark])
}

@Test func appAppearanceTitles() {
    #expect(AppAppearance.system.title == "System")
    #expect(AppAppearance.light.title == "Light")
    #expect(AppAppearance.dark.title == "Dark")
}

@Test func appearanceSettingsKeys() {
    #expect(AppSettings.appearanceThemeKey == "appearance.theme")
    #expect(AppSettings.terminalFontSizeKey == "appearance.terminalFontSize")
}

@Test func defaultTerminalFontSizeIs13() {
    #expect(AppSettings.defaultTerminalFontSize == 13)
}

@Test func clampTerminalFontSizeClampsIntoRange() {
    #expect(AppSettings.clampTerminalFontSize(4) == 9)
    #expect(AppSettings.clampTerminalFontSize(99) == 24)
    #expect(AppSettings.clampTerminalFontSize(13) == 13)
    #expect(AppSettings.clampTerminalFontSize(9) == 9)
    #expect(AppSettings.clampTerminalFontSize(24) == 24)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AppAppearance 2>&1 | tail -20`
Expected: compile FAILURE — `cannot find 'AppAppearance' in scope` (and missing AppSettings members).

- [ ] **Step 3: Write the implementation**

Create `Packages/TillerCore/Sources/TillerCore/AppAppearance.swift`:

```swift
import Foundation

/// User-selectable app appearance. `system` follows macOS; the App target
/// maps cases to `NSAppearance` (kept out of here — TillerCore is
/// Foundation-only).
public enum AppAppearance: String, CaseIterable, Sendable {
    case system
    case light
    case dark

    public var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
}
```

Append to the `AppSettings` enum body in `Packages/TillerCore/Sources/TillerCore/AppSettings.swift` (after `resumeAgentSessionsKey`):

```swift
    /// UserDefaults key for the app appearance (AppAppearance rawValue).
    /// Missing value means `.system`.
    public static let appearanceThemeKey = "appearance.theme"

    /// UserDefaults key for the terminal font size in points.
    public static let terminalFontSizeKey = "appearance.terminalFontSize"
    public static let defaultTerminalFontSize = 13
    public static let terminalFontSizeRange: ClosedRange<Int> = 9...24

    /// Clamp a stored terminal font size into the allowed range.
    public static func clampTerminalFontSize(_ size: Int) -> Int {
        min(max(size, terminalFontSizeRange.lowerBound), terminalFontSizeRange.upperBound)
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test 2>&1 | tail -5`
Expected: all TillerCore tests PASS (including the 6 new ones).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppAppearance.swift \
        Packages/TillerCore/Sources/TillerCore/AppSettings.swift \
        Packages/TillerCore/Tests/TillerCoreTests/AppAppearanceTests.swift
git commit -m "feat: add AppAppearance enum and appearance settings constants to TillerCore"
```

---

### Task 2: TillerTerminal — font-size-aware terminal theme + live pane wiring

**Files:**
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift` (the `PtyTerminalPane` struct, ~line 63)
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/ExecTerminalPane.swift`
- Test: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift`

**Interfaces:**
- Consumes: `AppSettings.terminalFontSizeKey` / `defaultTerminalFontSize` / `clampTerminalFontSize(_:)` from Task 1 (TillerTerminal already depends on TillerCore); `TerminalConfiguration.alabaster` / `.afterglow` / `.appending(_:)`, `TerminalTheme(light:dark:)`, `TerminalViewState.setTheme(_:)` from GhosttyTerminal (all public).
- Produces:
  - `TillerTerminalTheme.theme(fontSize: Float) -> TerminalTheme`
  - `TillerTerminalTheme.current(defaults: UserDefaults = .standard) -> TerminalTheme`
  - (removes the old `TillerTerminalTheme.theme` static let — its only users are the two panes modified here)

- [ ] **Step 1: Write the failing tests**

Create `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift`:

```swift
import Foundation
import Testing
import GhosttyTerminal
import TillerCore
@testable import TillerTerminal

@Test func themeAppliesFontSizeToBothConfigurations() {
    let theme = TillerTerminalTheme.theme(fontSize: 14)
    #expect(theme.light == TerminalConfiguration.alabaster.appending(.fontSize(14)))
    #expect(theme.dark == TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(.fontSize(14)))
}

@Test func themeDiffersByFontSize() {
    #expect(TillerTerminalTheme.theme(fontSize: 12) != TillerTerminalTheme.theme(fontSize: 14))
}

@Test func currentReadsStoredFontSizeAndClamps() throws {
    let suiteName = "TillerTerminalThemeTests-\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suiteName))
    defer { defaults.removePersistentDomain(forName: suiteName) }

    // No stored value -> default size.
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: Float(AppSettings.defaultTerminalFontSize)))

    // Stored value used.
    defaults.set(16, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 16))

    // Out-of-range value clamped.
    defaults.set(99, forKey: AppSettings.terminalFontSizeKey)
    #expect(TillerTerminalTheme.current(defaults: defaults)
        == TillerTerminalTheme.theme(fontSize: 24))
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerTerminal && swift test --filter TillerTerminalTheme 2>&1 | tail -20`
Expected: compile FAILURE — `theme(fontSize:)` / `current(defaults:)` not found.

- [ ] **Step 3: Implement the theme factory**

Replace the whole body of `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`:

```swift
import Foundation
import GhosttyTerminal
import TillerCore

/// Terminal color theme matching the app's unified surface color family
/// (`AppTheme.background` in the App target) — a bit darker than the chrome
/// (see `AppSurfaceColor.terminalHex`) instead of falling back to Ghostty's
/// default "afterglow" background (#212121). Light mode uses the stock
/// alabaster preset. Font size is injected into both configurations.
enum TillerTerminalTheme {
    static func theme(fontSize: Float) -> TerminalTheme {
        TerminalTheme(
            light: TerminalConfiguration.alabaster
                .appending(.fontSize(fontSize)),
            dark: TerminalConfiguration.afterglow
                .appending(.background(AppSurfaceColor.terminalHex))
                .appending(.fontSize(fontSize))
        )
    }

    /// Theme for the currently stored font size — used at pane creation,
    /// before SwiftUI property wrappers are available.
    static func current(defaults: UserDefaults = .standard) -> TerminalTheme {
        let stored = defaults.object(forKey: AppSettings.terminalFontSizeKey) as? Int
            ?? AppSettings.defaultTerminalFontSize
        return theme(fontSize: Float(AppSettings.clampTerminalFontSize(stored)))
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerTerminal && swift test --filter TillerTerminalTheme 2>&1 | tail -5`
Expected: 3 new tests PASS. (The package won't fully build yet if the panes still reference `TillerTerminalTheme.theme` — fix in the next step before running if so.)

- [ ] **Step 5: Wire the panes for live updates**

In `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`, inside `PtyTerminalPane`:

Change the state line:

```swift
    @StateObject private var state = TerminalViewState(theme: TillerTerminalTheme.current())
```

Add below the other property wrappers (next to `@State private var runtime`):

```swift
    @AppStorage(AppSettings.terminalFontSizeKey)
    private var terminalFontSize = AppSettings.defaultTerminalFontSize
```

In `body`, chain onto `TerminalSurfaceView(context: state)` after the existing `.onAppear { ... }` block:

```swift
            .onAppear { applyFontSize() }
            .onChange(of: terminalFontSize) { _, _ in applyFontSize() }
```

Add a private helper at the end of the struct:

```swift
    private func applyFontSize() {
        let size = Float(AppSettings.clampTerminalFontSize(terminalFontSize))
        state.setTheme(TillerTerminalTheme.theme(fontSize: size))
    }
```

`AppSettings` comes from `TillerCore`, already imported by this file (verify; add `import TillerCore` if missing). `.onAppear` re-applies for panes revived from `TerminalPaneCache`, where `.onChange` doesn't fire while detached.

In `Packages/TillerTerminal/Sources/TillerTerminal/ExecTerminalPane.swift`, apply the same pattern (this pane is a spike surface; keep it compiling):

```swift
import SwiftUI
import GhosttyTerminal
import TillerCore
import Inject

/// Spike surface: libghostty spawns and owns the login shell itself
/// (`.exec` backend), exactly like Ghostty.app. Zero PTY code on our side —
/// this isolates "does the engine render/input correctly" from our PTY work.
public struct ExecTerminalPane: View {
    @ObserveInjection var inject
    @StateObject private var state = TerminalViewState(theme: TillerTerminalTheme.current())
    @AppStorage(AppSettings.terminalFontSizeKey)
    private var terminalFontSize = AppSettings.defaultTerminalFontSize

    public init() {}

    public var body: some View {
        TerminalSurfaceView(context: state)
            .onAppear {
                state.configuration = TerminalSurfaceOptions(backend: .exec)
                applyFontSize()
            }
            .onChange(of: terminalFontSize) { _, _ in applyFontSize() }
            .enableInjection()
    }

    private func applyFontSize() {
        let size = Float(AppSettings.clampTerminalFontSize(terminalFontSize))
        state.setTheme(TillerTerminalTheme.theme(fontSize: size))
    }
}
```

- [ ] **Step 6: Run the full package test suite**

Run: `cd Packages/TillerTerminal && swift test 2>&1 | tail -5`
Expected: PASS. If only `PtyProcessTests.spawnCapturesOutput` fails, retry once (known flake).

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift \
        Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift \
        Packages/TillerTerminal/Sources/TillerTerminal/ExecTerminalPane.swift \
        Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift
git commit -m "feat: configurable terminal font size with live setTheme updates"
```

---

### Task 3: App — adaptive AppTheme palette

**Files:**
- Modify: `App/AppTheme.swift` (full rewrite of the token list)

**Interfaces:**
- Consumes: `AppSurfaceColor.red/green/blue` (TillerCore) for the dark background, exactly as today.
- Produces: same public surface as before — `AppTheme.background`, `.chromeTint`, `.hairline`, `.rowHover`, `.selectionFill`, `.selectionRing`, `.title`, `.titleSelected`, `.subtitle`, `.meta`, `.primaryPillBg`, `.filterFieldBg`, `.treeGuide` — all `Color`, now appearance-adaptive. **No call-site changes anywhere.**

There is no App-target unit test infrastructure; this task's gate is the app build (Step 2) plus the manual visual pass in Task 6. Light values are hand-tuned starting points from the spec and may be refined visually later.

- [ ] **Step 1: Rewrite AppTheme with dynamic tokens**

Replace the whole body of `App/AppTheme.swift`:

```swift
// Tiller/App/AppTheme.swift
import SwiftUI
import AppKit
import TillerCore

/// Shared color tokens for Tiller's chrome, adaptive to the effective
/// appearance (dark values are the original palette; light is a hand-tuned
/// cool-gray mirror with a subtle indigo tint). `background` is the opaque
/// main-pane and terminal surface, and it tints the native sidebar material
/// through `SidebarMaterialContainer`. The remaining tokens style sidebar
/// rows, labels, filter controls, hover, and selection. Agent accent colors
/// live in `AgentIcon`.
enum AppTheme {
    static let background = dynamic(
        light: NSColor(srgbRed: 0.965, green: 0.965, blue: 0.975, alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.red, green: AppSurfaceColor.green, blue: AppSurfaceColor.blue, alpha: 1))
    /// Indigo tint for the sidebar/tab bar/usage bar material (opencode-style).
    static let chromeTint = dynamic(
        light: NSColor(srgbRed: 0.92, green: 0.92, blue: 0.96, alpha: 1),
        dark: NSColor(srgbRed: 0.110, green: 0.110, blue: 0.176, alpha: 1))
    static let hairline = dynamic(
        light: NSColor(srgbRed: 0.82, green: 0.83, blue: 0.87, alpha: 1),
        dark: NSColor(srgbRed: 0.165, green: 0.176, blue: 0.220, alpha: 1))
    static let rowHover = dynamic(
        light: NSColor(srgbRed: 0.90, green: 0.905, blue: 0.93, alpha: 1),
        dark: NSColor(srgbRed: 0.125, green: 0.137, blue: 0.176, alpha: 1))
    static let selectionFill = dynamic(
        light: NSColor(srgbRed: 0.85, green: 0.86, blue: 0.91, alpha: 1),
        dark: NSColor(srgbRed: 0.169, green: 0.184, blue: 0.227, alpha: 1))
    static let selectionRing = dynamic(
        light: NSColor(srgbRed: 0.72, green: 0.74, blue: 0.82, alpha: 1),
        dark: NSColor(srgbRed: 0.227, green: 0.251, blue: 0.314, alpha: 1))
    static let title = dynamic(
        light: NSColor(srgbRed: 0.15, green: 0.16, blue: 0.20, alpha: 1),
        dark: NSColor(srgbRed: 0.85, green: 0.86, blue: 0.89, alpha: 1))
    static let titleSelected = dynamic(
        light: NSColor(srgbRed: 0.05, green: 0.05, blue: 0.08, alpha: 1),
        dark: .white)
    static let subtitle = dynamic(
        light: NSColor(srgbRed: 0.35, green: 0.37, blue: 0.45, alpha: 1),
        dark: NSColor(srgbRed: 0.72, green: 0.74, blue: 0.82, alpha: 1))
    static let meta = dynamic(
        light: NSColor(srgbRed: 0.42, green: 0.44, blue: 0.52, alpha: 1),
        dark: NSColor(srgbRed: 0.66, green: 0.68, blue: 0.77, alpha: 1))
    static let primaryPillBg = dynamic(
        light: NSColor(srgbRed: 0.88, green: 0.885, blue: 0.92, alpha: 1),
        dark: NSColor(srgbRed: 0.200, green: 0.204, blue: 0.239, alpha: 1))
    static let filterFieldBg = dynamic(
        light: .white,
        dark: NSColor(srgbRed: 0.078, green: 0.082, blue: 0.106, alpha: 1))
    /// Guide dell'albero in sidebar: abbastanza chiare da leggersi sul
    /// materiale traslucido, abbastanza tenui da non competere col testo.
    static let treeGuide = dynamic(
        light: NSColor.black.withAlphaComponent(0.12),
        dark: NSColor.white.withAlphaComponent(0.14))

    /// Resolves at draw time against the view's effective appearance — the
    /// same mechanism behind Apple's semantic colors.
    private static func dynamic(light: NSColor, dark: NSColor) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        })
    }
}
```

- [ ] **Step 2: Build to verify**

Run:
```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller && xcodegen generate && \
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -3
```
Expected: `BUILD SUCCEEDED`. (App still forces dark at this point — visuals unchanged.)

- [ ] **Step 3: Commit**

```bash
git add App/AppTheme.swift
git commit -m "feat: make AppTheme tokens appearance-adaptive with light palette"
```

---

### Task 4: App — stop forcing dark, apply selected appearance

**Files:**
- Create: `App/AppAppearance+NSAppearance.swift`
- Modify: `App/TillerApp.swift` (remove `.preferredColorScheme(.dark)` at line 29, add appearance application)
- Modify: `App/SidebarMaterialContainer.swift` (remove forced `.darkAqua` at line 22)

**Interfaces:**
- Consumes: `AppAppearance` + `AppSettings.appearanceThemeKey` (Task 1); adaptive `AppTheme` (Task 3).
- Produces: `AppAppearance.nsAppearance: NSAppearance?` (nil for `.system`); the app follows the stored preference live.

- [ ] **Step 1: Add the NSAppearance mapping**

Create `App/AppAppearance+NSAppearance.swift`:

```swift
import AppKit
import TillerCore

extension AppAppearance {
    /// nil means "follow macOS" (clears the app-level override).
    var nsAppearance: NSAppearance? {
        switch self {
        case .system: nil
        case .light: NSAppearance(named: .aqua)
        case .dark: NSAppearance(named: .darkAqua)
        }
    }
}
```

- [ ] **Step 2: Apply the appearance in TillerApp**

In `App/TillerApp.swift`:

Add the stored preference next to the existing `@AppStorage("sidebar.visible")`:

```swift
    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
```

Replace the `WindowGroup` content block

```swift
            ContentView(model: model, updater: updater)
                .preferredColorScheme(.dark)
                .onAppear {
                    appDelegate.model = model
                    updater.start()
                }
```

with

```swift
            ContentView(model: model, updater: updater)
                .onAppear {
                    appDelegate.model = model
                    updater.start()
                    applyAppearance()
                }
                .onChange(of: appearanceRaw) { _, _ in applyAppearance() }
```

Add a private helper inside the `TillerApp` struct (after `body`):

```swift
    private func applyAppearance() {
        let appearance = AppAppearance(rawValue: appearanceRaw) ?? .system
        NSApp.appearance = appearance.nsAppearance
    }
```

- [ ] **Step 3: Let the sidebar material follow the appearance**

In `App/SidebarMaterialContainer.swift`, delete the line

```swift
        view.appearance = NSAppearance(named: .darkAqua)
```

and update the stale comment above the struct: replace "One native dark-glass surface" with "One native glass surface" (it now adapts to the effective appearance).

- [ ] **Step 4: Build and smoke-test**

Run:
```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller && xcodegen generate && \
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -3
```
Expected: `BUILD SUCCEEDED`.

Note: until Task 5 lands there is no UI to change the preference; default is `.system`, so on a dark-mode Mac nothing visibly changes. To spot-check light mode early: `defaults write com.tiller.app appearance.theme light` (bundle id: check `project.yml` — use the `PRODUCT_BUNDLE_IDENTIFIER` there) and relaunch.

- [ ] **Step 5: Commit**

```bash
git add App/AppAppearance+NSAppearance.swift App/TillerApp.swift App/SidebarMaterialContainer.swift
git commit -m "feat: apply user-selected appearance instead of forcing dark mode"
```

---

### Task 5: App — Appearance settings UI

**Files:**
- Modify: `App/AppearanceSettingsView.swift` (replace the placeholder)

**Interfaces:**
- Consumes: `AppAppearance` (+ `.title`), `AppSettings.appearanceThemeKey` / `terminalFontSizeKey` / `defaultTerminalFontSize` / `terminalFontSizeRange` (Task 1). Writing the defaults keys is enough: `TillerApp` (Task 4) reacts to the theme, the terminal panes (Task 2) react to the font size.
- Produces: the Appearance detail pane already routed by `SettingsSurface` (case `.appearance`).

- [ ] **Step 1: Implement the view**

Replace the whole body of `App/AppearanceSettingsView.swift`:

```swift
import SwiftUI
import TillerCore
import Inject

/// Appearance settings: app theme (system/light/dark) and terminal font
/// size. Writes preferences only — TillerApp applies the theme, terminal
/// panes pick up the font size live.
struct AppearanceSettingsView: View {
    @ObserveInjection var inject
    @AppStorage(AppSettings.appearanceThemeKey) private var appearanceRaw = AppAppearance.system.rawValue
    @AppStorage(AppSettings.terminalFontSizeKey) private var terminalFontSize = AppSettings.defaultTerminalFontSize

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance", selection: $appearanceRaw) {
                    ForEach(AppAppearance.allCases, id: \.rawValue) { appearance in
                        Text(appearance.title).tag(appearance.rawValue)
                    }
                }
                .pickerStyle(.segmented)
            }
            Section("Terminal") {
                Stepper(value: $terminalFontSize, in: AppSettings.terminalFontSizeRange) {
                    Text("Font size")
                    Text("\(terminalFontSize) pt")
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .enableInjection()
    }
}
```

Style matches `GeneralSettingsView` (grouped Form, hidden scroll background, Inject).

- [ ] **Step 2: Build**

Run:
```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller && xcodegen generate && \
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -3
```
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 3: Commit**

```bash
git add App/AppearanceSettingsView.swift
git commit -m "feat: appearance settings UI with theme picker and terminal font size"
```

---

### Task 6: Full verification + manual checklist

**Files:** none (verification only).

- [ ] **Step 1: Run the CI gate**

Run: `cd /Users/enzopiopalmisano/Desktop/Progetti/tiller && Scripts/ci.sh 2>&1 | tail -10`
Expected: `CI OK`. If only `PtyProcessTests.spawnCapturesOutput` fails, retry once (known flake).

- [ ] **Step 2: Launch the app and run the manual checklist**

Build and launch (`Scripts/build-dev.sh` or run the Debug product from `DerivedData/Build/Products/Debug/Tiller.app`). Verify, and report each item to the user:

1. Settings → Appearance shows segmented System / Light / Dark and the font-size stepper.
2. Switching Light ↔ Dark restyles chrome, sidebar material, titlebar and settings live — no restart.
3. In Light: terminal surface switches to alabaster (paper-white); in Dark: back to afterglow with navy background.
4. Font size stepper changes the text size on the visible terminal pane immediately; switching to a cached pane (other tab) shows the new size on appear.
5. System mode follows macOS appearance (toggle in System Settings → Appearance).
6. Quit and relaunch: theme and font size persist.

Items 2–6 need a human eye — if running as an agent, ask the user to confirm rather than claiming success.

- [ ] **Step 3: Final commit (if any fixups) and report**

Report checklist results. Do not merge/push without user confirmation.
