# Surface Color #1F1F26 + Uniform Translucency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Terminal and chat surfaces become translucent charcoal `#1F1F26` with the same 0.96 opacity the sidebar already uses; chrome (`#131313`) elsewhere is untouched.

**Architecture:** `AppSurfaceColor` (TillerCore) stays the single source of truth: its `terminal*` components become the new color and it gains a shared `surfaceOpacity` constant. The Ghostty dark config gains `background-opacity`/`background-blur-radius`. The App main pane swaps its opaque background for an `NSVisualEffectView`-based translucent surface tinted with the new color.

**Tech Stack:** Swift 6, SwiftUI/AppKit, swift-testing (`@Test`/`#expect`), libghostty config keys, XcodeGen project (do not touch `Tiller.xcodeproj`).

## Global Constraints

- New color: sRGB `0.122 / 0.122 / 0.149` → hex `1F1F26` (spec).
- Shared opacity: `0.96` (spec; value the sidebar already uses).
- Ghostty dark config: `background-opacity = 0.96`, `background-blur-radius = 20` (spec; blur value is a starting point, visual tuning allowed).
- Chrome color `#131313` (components `0.075`) unchanged; sheets/settings/markdown editor keep `AppTheme.background`.
- Light mode unchanged.
- Tests first (swift-testing, not XCTest). Conventional Commits, lower-case imperative.
- Verification gate: `Scripts/ci.sh` prints `CI OK` (PTY test `spawnCapturesOutput` is flaky — retry the script up to 5-6 times if that is the only failure).

---

### Task 1: TillerCore — new terminal color + shared opacity constant

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`
- Test (create): `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`

**Interfaces:**
- Produces: `AppSurfaceColor.terminalRed/terminalGreen/terminalBlue == 0.122/0.122/0.149`, `AppSurfaceColor.terminalHex == "1F1F26"`, `AppSurfaceColor.hex == "131313"` (unchanged), `public static let surfaceOpacity: Double = 0.96`. Tasks 2 and 3 consume all of these.

**Note on rounding:** `hexString` currently truncates (`Int(red * 255)`). `0.149 * 255 = 37.995` would truncate to `0x25`, giving `1F1F25`. The implementation switches to rounding; the chrome hex is unaffected (`0.075 * 255 = 19.125` rounds to 19 = `0x13` either way).

- [ ] **Step 1: Write the failing test**

Create `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`:

```swift
import Testing
@testable import TillerCore

@Test func chromeHexIsUnchanged() {
    #expect(AppSurfaceColor.hex == "131313")
}

@Test func terminalSurfaceIsCharcoal1F1F26() {
    #expect(AppSurfaceColor.terminalRed == 0.122)
    #expect(AppSurfaceColor.terminalGreen == 0.122)
    #expect(AppSurfaceColor.terminalBlue == 0.149)
    #expect(AppSurfaceColor.terminalHex == "1F1F26")
}

@Test func sharedSurfaceOpacityIs096() {
    #expect(AppSurfaceColor.surfaceOpacity == 0.96)
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AppSurfaceColorTests`
Expected: compile error — `surfaceOpacity` not defined (and, once defined, `terminalSurfaceIsCharcoal1F1F26` fails against the old `×0.7` values).

- [ ] **Step 3: Implement**

In `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`, replace the `terminal*` block and `hexString`:

```swift
    /// Charcoal surface for terminal and chat panes (#1F1F26) — slightly
    /// blue-tinted, distinct from the neutral chrome above.
    public static let terminalRed: Double = 0.122
    public static let terminalGreen: Double = 0.122
    public static let terminalBlue: Double = 0.149

    public static var terminalHex: String {
        hexString(red: terminalRed, green: terminalGreen, blue: terminalBlue)
    }

    /// Shared perceived opacity for every translucent surface (sidebar
    /// material, Ghostty background-opacity, chat main pane).
    public static let surfaceOpacity: Double = 0.96

    private static func hexString(red: Double, green: Double, blue: Double) -> String {
        String(format: "%02X%02X%02X",
               Int((red * 255).rounded()),
               Int((green * 255).rounded()),
               Int((blue * 255).rounded()))
    }
```

Also update the stale doc comment on lines 16-17 (the "bit darker / recessed" note no longer applies): replace it with the comment shown above.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter AppSurfaceColorTests`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift
git commit -m "feat: charcoal #1F1F26 terminal surface color and shared surface opacity"
```

---

### Task 2: TillerTerminal — translucent Ghostty dark config

**Files:**
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift`
- Modify: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift`

**Interfaces:**
- Consumes: `AppSurfaceColor.terminalHex` (now `"1F1F26"`), `AppSurfaceColor.surfaceOpacity` (Task 1).
- Produces: dark theme config additionally contains `background-opacity = 0.96` and `background-blur-radius = 20`. No API change — `TillerTerminalTheme.theme(fontSize:)` signature untouched.

- [ ] **Step 1: Update the existing test to expect the new config (failing first)**

In `TillerTerminalThemeTests.swift`, replace the body of `themeAppliesFontSizeToBothConfigurations` with:

```swift
@Test func themeAppliesFontSizeToBothConfigurations() {
    let theme = TillerTerminalTheme.theme(fontSize: 14)
    let scrollbackLimit = TerminalConfigCommand.custom(key: "scrollback-limit", value: "262144")
    #expect(theme.light == TerminalConfiguration.alabaster
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
    #expect(theme.dark == TerminalConfiguration.afterglow
        .appending(.background(AppSurfaceColor.terminalHex))
        .appending(TerminalConfigCommand.custom(key: "background-opacity", value: "\(AppSurfaceColor.surfaceOpacity)"))
        .appending(TerminalConfigCommand.custom(key: "background-blur-radius", value: "20"))
        .appending(.fontSize(14))
        .appending(scrollbackLimit))
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerTerminal && swift test --filter themeAppliesFontSizeToBothConfigurations`
Expected: FAIL — dark configuration mismatch (missing background-opacity / background-blur-radius).

- [ ] **Step 3: Implement**

In `TillerTerminalTheme.swift`, inside `theme(fontSize:)`, add the two commands and append them to the dark config only (light stays opaque alabaster):

```swift
        // Translucent terminal surface matching the sidebar's perceived
        // opacity; blur radius is an empirical starting point to visually
        // match NSVisualEffectView's sidebar material.
        let backgroundOpacity = TerminalConfigCommand.custom(
            key: "background-opacity", value: "\(AppSurfaceColor.surfaceOpacity)")
        let backgroundBlur = TerminalConfigCommand.custom(
            key: "background-blur-radius", value: "20")
        return TerminalTheme(
            light: TerminalConfiguration.alabaster
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit),
            dark: TerminalConfiguration.afterglow
                .appending(.background(AppSurfaceColor.terminalHex))
                .appending(backgroundOpacity)
                .appending(backgroundBlur)
                .appending(.fontSize(fontSize))
                .appending(scrollbackLimit)
        )
```

Also update the file's header doc comment: the theme is no longer "a bit darker than the chrome" — it is the charcoal `#1F1F26` translucent surface.

- [ ] **Step 4: Run the package tests**

Run: `cd Packages/TillerTerminal && swift test --filter TillerTerminalThemeTests`
Expected: all theme tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift
git commit -m "feat: translucent ghostty dark background with shared opacity and blur"
```

---

### Task 3: App — translucent chat/main pane surface

**Files:**
- Modify: `App/AppTheme.swift` (new `terminalSurface` token)
- Modify: `App/SidebarMaterialContainer.swift` (shared opacity + new `MainSurfaceMaterial` view)
- Modify: `App/ContentView.swift:192` (background swap)

**Interfaces:**
- Consumes: `AppSurfaceColor.terminal*` components and `AppSurfaceColor.surfaceOpacity` (Task 1).
- Produces: `AppTheme.terminalSurface: Color`, `MainSurfaceMaterial: View`. App-target only; no package API changes. No unit test seam exists for these SwiftUI views — verification is the Task 4 build/CI gate plus manual smoke.

- [ ] **Step 1: Add the `terminalSurface` token to `App/AppTheme.swift`**

Insert directly after the `background` token (line 16):

```swift
    /// Charcoal surface for the terminal/chat main pane (#1F1F26 dark).
    /// Light mode mirrors `background` — translucency is a dark-mode look.
    static let terminalSurface = dynamic(
        light: NSColor(srgbRed: 0.965, green: 0.965, blue: 0.975, alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.terminalRed, green: AppSurfaceColor.terminalGreen, blue: AppSurfaceColor.terminalBlue, alpha: 1))
```

- [ ] **Step 2: Share the opacity constant and add `MainSurfaceMaterial` in `App/SidebarMaterialContainer.swift`**

Ensure the file imports TillerCore (add `import TillerCore` if missing). Replace the hardcoded constant:

```swift
    static let backgroundOpacity = AppSurfaceColor.surfaceOpacity
```

Append at the end of the file (same file so it can reuse the private `SidebarMaterialView`):

```swift
/// Translucent surface for the terminal/chat main pane: the same
/// behind-window blur as the sidebar, tinted with the charcoal surface
/// color at the shared opacity so it visually matches the terminal's
/// ghostty `background-opacity`.
struct MainSurfaceMaterial: View {
    var body: some View {
        SidebarMaterialView()
            .overlay(AppTheme.terminalSurface.opacity(AppSurfaceColor.surfaceOpacity))
    }
}
```

- [ ] **Step 3: Swap the main-pane background in `App/ContentView.swift`**

At line ~192, replace:

```swift
            // Opaque terminal surface only below the titlebar; the shared
            // material behind the whole ZStack shows through above it, so
            // the titlebar matches the sidebar.
            .background(AppTheme.background, ignoresSafeAreaEdges: [])
```

with:

```swift
            // Translucent terminal/chat surface only below the titlebar; the
            // shared material behind the whole ZStack shows through above it,
            // so the titlebar matches the sidebar.
            .background { MainSurfaceMaterial() }
```

(The view-based `.background {}` does not extend into safe areas, matching the previous `ignoresSafeAreaEdges: []` behavior.)

- [ ] **Step 4: Build the app target**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build -quiet`
Expected: BUILD SUCCEEDED (no new warnings about unresolved symbols).

- [ ] **Step 5: Commit**

```bash
git add App/AppTheme.swift App/SidebarMaterialContainer.swift App/ContentView.swift
git commit -m "feat: translucent charcoal main-pane surface for chat and terminal"
```

---

### Task 4: Verification gate

**Files:** none (verification only).

- [ ] **Step 1: Run the repo CI gate**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`. If the only failure is the flaky `spawnCapturesOutput` PTY test in TillerTerminal, re-run the script (up to 5-6 attempts).

- [ ] **Step 2: Manual smoke (user)**

Launch the app over a light wallpaper: terminal, chat, and sidebar side by side — blur and opacity read uniform; terminal text contrast acceptable; sheets/settings still chrome `#131313`. If the Ghostty blur visibly differs from the sidebar material, tune `background-blur-radius` (Task 2 value `20`).
