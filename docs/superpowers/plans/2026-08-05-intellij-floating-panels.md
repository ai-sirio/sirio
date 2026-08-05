# IntelliJ-style Floating Panels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild Tiller's main window so the sidebar, central pane, and right panel are three cards resting on a darker canvas visible as a frame on all four sides.

**Architecture:** A canvas layer replaces the chrome material as the outermost background. The single existing floating surface (`FloatingMainSurface`) generalises into `FloatingCard`, applied to all three columns. The native window toolbar is dropped for `.hiddenTitleBar` plus a 28pt strip we draw ourselves, which removes the safe-area measurement the current layout is built around.

**Tech Stack:** Swift 6, SwiftUI + AppKit, swift-testing (`@Test` / `#expect`), XcodeGen, GRDB (untouched here).

## Global Constraints

- Design spec: `docs/superpowers/specs/2026-08-05-intellij-floating-panels-design.md`. It governs every value below.
- Approved geometry: card corner radius **6**, gap **10**, title strip height **28**, shadow radius **18**, shadow Y offset **6**.
- Approved colours: card `#1B1C1F` dark / `#E9EAED` light; canvas `#131417` dark / `#DCDDE2` light.
- Tests first, always: write the failing test, run it, watch it fail, then implement.
- swift-testing (`@Test` / `#expect`), never XCTest.
- All user-facing strings in English, even though this plan's prose is Italian-adjacent.
- Commit messages: Conventional Commits, lower-case imperative subject.
- **After creating, renaming, or deleting any file, run `xcodegen generate`** before building — `Tiller.xcodeproj` is generated from `project.yml` and never hand-edited.
- Package tests run with `cd Packages/<Name> && swift test`. App-target tests (sources in `AppTests/`) only run through `xcodebuild test`; do **not** pass `CODE_SIGNING_ALLOWED=NO` or `-derivedDataPath` to that command — either one hangs the test host in dyld on managed Macs.
- `Scripts/ci.sh` is the full gate and prints `CI OK`. Run it **once, in Task 8** — not after every task. Per-task verification uses the narrow commands given in each task.
- Do not "improve" adjacent code. Every changed line traces to a step below.

---

### Task 1: Collapse the surface palette and add the canvas

The app currently defines two surfaces: chrome (`#1B1C1F`) and central pane (`#28292C`). They become one, and a third, darker value is introduced for the canvas behind the cards.

`AppSurfaceColor` is shared: `AppTheme` (App target) reads its components, and `TillerTerminal` derives the Ghostty terminal background from its hex strings. Repointing the chat/terminal components at the chrome ones is what makes the terminal's own background follow the card colour instead of sitting as a lighter rectangle inside it.

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift:12-18`
- Test: `Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift:10`

**Interfaces:**
- Consumes: nothing.
- Produces: `AppSurfaceColor.canvasRed/canvasGreen/canvasBlue: Double`, `AppSurfaceColor.canvasLightRed/canvasLightGreen/canvasLightBlue: Double`, `AppSurfaceColor.canvasHex: String`, `AppSurfaceColor.canvasLightHex: String`. Task 2 reads all six components.

- [ ] **Step 1: Write the failing tests**

In `Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift`, replace the whole `chatSurfaceMatchesApprovedDarkAndLightValues` function (lines 12-18) with these three:

```swift
@Test func chatSurfaceIsTheSameSurfaceAsTheChrome() {
    #expect(AppSurfaceColor.chatRed == AppSurfaceColor.red)
    #expect(AppSurfaceColor.chatGreen == AppSurfaceColor.green)
    #expect(AppSurfaceColor.chatBlue == AppSurfaceColor.blue)
    #expect(AppSurfaceColor.chatHex == AppSurfaceColor.hex)
    #expect(AppSurfaceColor.chatLightHex == AppSurfaceColor.lightHex)
}

@Test func canvasMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.canvasHex == "131417")
    #expect(AppSurfaceColor.canvasLightHex == "DCDDE2")
}

/// The relationship, not the values: whatever the palette becomes, a card that
/// is darker than what it sits on stops reading as resting on it.
@Test func canvasIsDarkerThanTheCardInBothAppearances() {
    #expect(AppSurfaceColor.canvasRed < AppSurfaceColor.red)
    #expect(AppSurfaceColor.canvasGreen < AppSurfaceColor.green)
    #expect(AppSurfaceColor.canvasBlue < AppSurfaceColor.blue)
    #expect(AppSurfaceColor.canvasLightRed < AppSurfaceColor.lightRed)
    #expect(AppSurfaceColor.canvasLightGreen < AppSurfaceColor.lightGreen)
    #expect(AppSurfaceColor.canvasLightBlue < AppSurfaceColor.lightBlue)
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerCore && swift test --filter AppSurfaceColor`
Expected: compile error — `canvasRed`, `canvasHex`, `canvasLightHex` are not members of `AppSurfaceColor`.

- [ ] **Step 3: Add the canvas components**

In `Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`, insert after the `lightHex` computed property:

```swift
    /// Window canvas behind the floating cards (#131417). Every panel is a card
    /// on top of this; it is the only surface that reaches the window edges.
    public static let canvasRed: Double = 19.0 / 255.0
    public static let canvasGreen: Double = 20.0 / 255.0
    public static let canvasBlue: Double = 23.0 / 255.0

    public static var canvasHex: String {
        hexString(red: canvasRed, green: canvasGreen, blue: canvasBlue)
    }

    /// Window canvas in light appearance (#DCDDE2). Darker than the card, not
    /// lighter: a lighter canvas makes the cards sink instead of rest.
    public static let canvasLightRed: Double = 220.0 / 255.0
    public static let canvasLightGreen: Double = 221.0 / 255.0
    public static let canvasLightBlue: Double = 226.0 / 255.0

    public static var canvasLightHex: String {
        hexString(red: canvasLightRed, green: canvasLightGreen, blue: canvasLightBlue)
    }
```

- [ ] **Step 4: Repoint the chat components at the chrome ones**

In the same file, replace the six `chat*` stored properties (`chatRed` through `chatLightBlue`) with aliases. Keep the `chatHex`/`chatLightHex` computed properties exactly as they are.

```swift
    /// Central chat surface — the same colour as the sidebar chrome. Every
    /// floating card shares one surface; the canvas behind them is what
    /// separates them, not a difference in fill. Kept as its own name because
    /// `TillerTerminal` and `AppTheme` reach for it under this name.
    public static let chatRed: Double = red
    public static let chatGreen: Double = green
    public static let chatBlue: Double = blue
```

and

```swift
    /// Central chat surface in light appearance — see `chatRed`.
    public static let chatLightRed: Double = lightRed
    public static let chatLightGreen: Double = lightGreen
    public static let chatLightBlue: Double = lightBlue
```

The `terminal*` properties already alias the `chat*` ones and need no edit.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd Packages/TillerCore && swift test --filter AppSurfaceColor`
Expected: PASS.

- [ ] **Step 6: Fix the Ghostty theme test**

`Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift:10` asserts the old hex. Change it to assert the new one and the invariant behind it:

```swift
    #expect(AppSurfaceColor.terminalHex == "1B1C1F")
    #expect(AppSurfaceColor.terminalHex == AppSurfaceColor.hex)
```

- [ ] **Step 7: Run the terminal theme tests**

Run: `cd Packages/TillerTerminal && swift test --filter themeAppliesFontSizeToBothConfigurations`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift \
        Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift \
        Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift
git commit -m "feat: collapse card surfaces onto one colour and add the canvas"
```

---

### Task 2: Card geometry tokens in AppTheme

The five `mainSurface*` tokens describe one bottom-rounded surface glued to the window's top edge. They become card tokens plus the two the new title strip needs.

**Files:**
- Modify: `App/AppTheme.swift:53-63` (the geometry block and `mainSurfaceBorder`)
- Modify: `App/AppTheme.swift:11-18` (add `canvas` next to `background`)
- Delete: `AppTests/FloatingMainSurfaceTests.swift`
- Create: `AppTests/CardGeometryTests.swift`

**Interfaces:**
- Consumes: `AppSurfaceColor.canvas*` from Task 1.
- Produces: `AppTheme.cardCornerRadius: CGFloat`, `AppTheme.cardGap: CGFloat`, `AppTheme.cardShadowRadius: CGFloat`, `AppTheme.cardShadowYOffset: CGFloat`, `AppTheme.titleStripHeight: CGFloat`, `AppTheme.trafficLightInset: CGFloat`, `AppTheme.canvas: Color`. Tasks 3-7 all read these.

- [ ] **Step 1: Write the failing test**

Delete `AppTests/FloatingMainSurfaceTests.swift` and create `AppTests/CardGeometryTests.swift`:

```swift
import Testing
@testable import Tiller

@Test func cardsUseApprovedFloatingGeometry() {
    #expect(AppTheme.cardCornerRadius == 6)
    #expect(AppTheme.cardGap == 10)
    #expect(AppTheme.cardShadowRadius == 18)
    #expect(AppTheme.cardShadowYOffset == 6)
}

/// 28 is the height of the standard macOS titlebar — the clearance the traffic
/// lights need. Shrinking it clips them; growing it thickens the frame's top
/// band for nothing.
@Test func titleStripClearsTheTrafficLights() {
    #expect(AppTheme.titleStripHeight == 28)
    #expect(AppTheme.trafficLightInset == 78)
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/cardsUseApprovedFloatingGeometry 2>&1 | tail -20
```
Expected: compile error — `cardCornerRadius` is not a member of `AppTheme`.

- [ ] **Step 3: Replace the geometry tokens**

In `App/AppTheme.swift`, replace the block from `/// Border and inset geometry for the central floating surface.` through the end of the `mainSurfaceBorder` declaration with:

```swift
    /// Geometry for the floating cards. Every panel — sidebar, central pane,
    /// right panel — is one of these, sitting on the canvas.
    static let cardCornerRadius: CGFloat = 6
    /// One value for the space between two cards, for the margin against the
    /// window's left and right edges, and for the margin below the split. A
    /// different outer margin would stop the canvas reading as a frame.
    static let cardGap: CGFloat = 10
    static let cardShadowRadius: CGFloat = 18
    static let cardShadowYOffset: CGFloat = 6
    /// The canvas band above the cards, holding the traffic lights and the
    /// chrome buttons. 28pt is the standard macOS titlebar height; it is
    /// deliberately thicker than `cardGap` because the window controls need it.
    static let titleStripHeight: CGFloat = 28
    /// Leading space in the title strip reserved for the traffic lights, which
    /// AppKit keeps drawing itself even under `.hiddenTitleBar`.
    static let trafficLightInset: CGFloat = 78
```

`mainSurfaceBorder` is deleted outright — the approved design has no drawn border on any card.

- [ ] **Step 4: Add the canvas colour**

In `App/AppTheme.swift`, immediately after the `background` declaration:

```swift
    /// The window canvas: the only surface reaching the window edges, and the
    /// one every card floats on.
    static let canvas = dynamic(
        light: NSColor(srgbRed: AppSurfaceColor.canvasLightRed,
                       green: AppSurfaceColor.canvasLightGreen,
                       blue: AppSurfaceColor.canvasLightBlue,
                       alpha: 1),
        dark: NSColor(srgbRed: AppSurfaceColor.canvasRed,
                      green: AppSurfaceColor.canvasGreen,
                      blue: AppSurfaceColor.canvasBlue,
                      alpha: 1))
```

- [ ] **Step 5: Run the test to verify it passes**

The build will still fail in `App/FloatingMainSurface.swift`, which references the deleted tokens. That is expected and Task 3 fixes it. To keep this task's commit compiling, apply the minimal rename now — in `App/FloatingMainSurface.swift` swap `AppTheme.mainSurfaceCornerRadius` → `AppTheme.cardCornerRadius`, `AppTheme.mainSurfaceShadowRadius` → `AppTheme.cardShadowRadius`, `AppTheme.mainSurfaceShadowYOffset` → `AppTheme.cardShadowYOffset`, and delete the `.overlay { … strokeBorder … }` block that used `mainSurfaceBorder`. In `App/ContentView.swift:272,275` swap `AppTheme.mainSurfaceHorizontalInset` and `AppTheme.mainSurfaceVerticalInset` → `AppTheme.cardGap`.

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/cardsUseApprovedFloatingGeometry 2>&1 | tail -20
```
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add App/AppTheme.swift App/FloatingMainSurface.swift App/ContentView.swift \
        AppTests/CardGeometryTests.swift
git rm AppTests/FloatingMainSurfaceTests.swift
git commit -m "feat: replace main-surface tokens with card and title-strip geometry"
```

---

### Task 3: FloatingCard

`FloatingMainSurface` rounds only its bottom corners and draws its border 1pt taller so the top edge falls outside the clip. Both tricks exist solely because the surface touched `y=0`; with the card below a canvas strip, they are wrong.

**Files:**
- Delete: `App/FloatingMainSurface.swift`
- Create: `App/FloatingCard.swift`
- Modify: `App/ContentView.swift:252` (call site)

**Interfaces:**
- Consumes: `AppTheme.cardCornerRadius`, `AppTheme.cardShadowRadius`, `AppTheme.cardShadowYOffset` from Task 2.
- Produces: `FloatingCard<Content: View>` with `init(tint: Color = AppTheme.background, @ViewBuilder content: @escaping () -> Content)`. Task 6 wraps all three columns in it.

- [ ] **Step 1: Create the new view**

`App/FloatingCard.swift`:

```swift
import SwiftUI

/// One panel resting on the window canvas. All three columns — sidebar,
/// central pane, right panel — are these; the canvas showing through the gaps
/// between them is what separates them, so the card draws no border of its own.
struct FloatingCard<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let content: () -> Content

    init(tint: Color = AppTheme.background,
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.content = content
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.cardCornerRadius,
                                        style: .continuous))
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.cardShadowRadius,
                y: AppTheme.cardShadowYOffset)
    }
}
```

- [ ] **Step 2: Delete the old view and update the call site**

```bash
git rm App/FloatingMainSurface.swift
```

In `App/ContentView.swift:252`, change `FloatingMainSurface {` to `FloatingCard {`.

- [ ] **Step 3: Build**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build 2>&1 | tail -5
```
Expected: `BUILD SUCCEEDED`. There is no unit test here — a SwiftUI view's shape is not assertable without snapshot infrastructure the repo does not have. The geometry values it reads are covered by Task 2; the rendered result is covered by the manual checklist in Task 8.

- [ ] **Step 4: Commit**

```bash
git add App/FloatingCard.swift App/ContentView.swift
git commit -m "refactor: generalise the main surface into FloatingCard"
```

---

### Task 4: Canvas background layer

`SidebarMaterialContainer` currently paints the chrome behind the whole split. Its job changes: it becomes the canvas. It keeps its translucency branch — that is now the only place the desktop can show through, which is exactly the intent.

The usage bar loses its background entirely: the approved design puts its text directly on the canvas.

**Files:**
- Delete: `App/SidebarMaterialContainer.swift`
- Create: `App/WindowSurfaces.swift`
- Modify: `App/UsageBarView.swift:75`
- Modify: `App/ContentView.swift:188,223`
- Modify: `App/WindowChromeConfigurator.swift:4-7` (stale doc comment)

**Interfaces:**
- Consumes: `AppTheme.canvas` from Task 2.
- Produces: `CanvasBackground` (no arguments). `MainSurfaceMaterial(tint:)` moves file unchanged and keeps its signature — Task 3's `FloatingCard`, `TabBarView`, `PaneTabStripBar`, and `ChatPaneView` all still call it.

- [ ] **Step 1: Create the new file**

`App/WindowSurfaces.swift` — the contents of `App/SidebarMaterialContainer.swift` with the container renamed and repointed at the canvas. `MainSurfaceMaterial` moves across **unchanged**:

```swift
import SwiftUI
import AppKit
import TillerCore

/// The window canvas: one surface behind every floating card, reaching all four
/// window edges. With translucency on it is the only layer the desktop shows
/// through, so the desktop appears in the frame around the cards and not inside
/// the terminal.
struct CanvasBackground: View {
    /// Canvas translucency: < 1 lets the raw desktop show through the blur.
    /// Requires the non-opaque window set up in `WindowChromeConfigurator`.
    static let backgroundOpacity = AppSurfaceColor.translucentSurfaceOpacity
    static let tintOpacity = 0.30
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        if translucencyEnabled {
            SidebarMaterialView()
                .overlay(AppTheme.canvas.opacity(Self.tintOpacity))
                .opacity(Self.backgroundOpacity)
        } else {
            AppTheme.canvas
        }
    }
}

private struct SidebarMaterialView: NSViewRepresentable {
    func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = .sidebar
        view.blendingMode = .behindWindow
        view.state = .followsWindowActiveState
        return view
    }

    func updateNSView(_: NSVisualEffectView, context: Context) {}
}

/// Translucent surface for a card's content: the same behind-window blur as the
/// canvas, tinted with `tint` at the shared opacity so it matches the
/// terminal's ghostty `background-opacity`.
struct MainSurfaceMaterial: View {
    var tint: Color = AppTheme.background
    @AppStorage(AppSettings.translucencyEnabledKey) private var translucencyEnabled = false

    var body: some View {
        if translucencyEnabled {
            SidebarMaterialView()
                .overlay(tint.opacity(AppSurfaceColor.translucentSurfaceOpacity))
        } else {
            tint
        }
    }
}
```

Note the default `tint` changed from `AppTheme.terminalSurface` to `AppTheme.background`. After Task 1 these resolve to the same colour; the new name is the honest one.

```bash
git rm App/SidebarMaterialContainer.swift
```

- [ ] **Step 2: Update the three call sites**

- `App/ContentView.swift:188`: `SidebarMaterialContainer().ignoresSafeArea()` → `CanvasBackground().ignoresSafeArea()`
- `App/ContentView.swift:223`: `SidebarMaterialContainer()` → `CanvasBackground()` (this 2pt strip covers the right divider; Task 6 revisits it)
- `App/UsageBarView.swift:75`: delete the line `.background(SidebarMaterialContainer())` entirely — the bar now sits on bare canvas.

- [ ] **Step 3: Fix the stale doc comment**

`App/WindowChromeConfigurator.swift:4-7` names `SidebarMaterialContainer.backgroundOpacity` and claims the terminal pane stays covered by an opaque background. Replace that comment with:

```swift
/// Makes the titlebar transparent and lets the canvas extend under it. The
/// window is non-opaque so the canvas material at reduced opacity
/// (`CanvasBackground.backgroundOpacity`) lets the desktop show through.
```

- [ ] **Step 4: Build**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build 2>&1 | tail -5
```
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 5: Commit**

```bash
git add App/WindowSurfaces.swift App/ContentView.swift App/UsageBarView.swift \
        App/WindowChromeConfigurator.swift
git commit -m "feat: turn the chrome material into the window canvas"
```

---

### Task 5: Hidden titlebar and the 28pt title strip

The native toolbar imposes the top inset. `.unifiedCompact` gives roughly 38pt and cannot be told to give 28. Dropping the toolbar hands us the number and removes the safe-area machinery built around it: `titlebarInset`, its `onGeometryChange`, its `.padding(.top,)`, and both `.ignoresSafeArea(.container, edges: .top)` calls, which existed only to fight an inset that will now be zero.

**Files:**
- Create: `App/TitleStrip.swift`
- Modify: `App/TillerApp.swift:56`
- Modify: `App/ContentView.swift:19-24, 101-147, 186-203, 205-235, 267-280`

**Interfaces:**
- Consumes: `AppTheme.titleStripHeight`, `AppTheme.trafficLightInset` from Task 2.
- Produces: `TitleStrip<Trailing: View>` with `init(@ViewBuilder trailing: () -> Trailing)`. Task 6 keeps it as the first row of `workspaceView`.

- [ ] **Step 1: Create the strip**

`App/TitleStrip.swift`:

```swift
import SwiftUI

/// The canvas band at the top of the window. AppKit still draws the traffic
/// lights over its leading edge under `.hiddenTitleBar`, so the strip reserves
/// room for them and puts the chrome buttons at the trailing end.
struct TitleStrip<Trailing: View>: View {
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: 2) {
            Color.clear.frame(width: AppTheme.trafficLightInset, height: 1)
            Spacer(minLength: 0)
            trailing
        }
        .padding(.trailing, AppTheme.cardGap)
        .frame(height: AppTheme.titleStripHeight)
    }
}
```

- [ ] **Step 2: Switch the window style**

`App/TillerApp.swift:56`: replace `.windowToolbarStyle(.unifiedCompact)` with `.windowStyle(.hiddenTitleBar)`.

- [ ] **Step 3: Move the toolbar buttons into the strip**

In `App/ContentView.swift`, delete the entire `.toolbar { … }` modifier (lines 101-146) and the `.toolbarBackgroundVisibility(.hidden, for: .windowToolbar)` line (147). Add this computed property to `ContentView`:

```swift
    /// The three chrome buttons that used to live in the window toolbar. They
    /// keep their actions, shortcuts, and help text; only their host changed.
    @ViewBuilder
    private var titleStripButtons: some View {
        Button {
            sidebarVisible.toggle()
        } label: {
            Image(systemName: "sidebar.left")
        }
        .buttonStyle(HoverIconButtonStyle())
        .help(sidebarVisible ? "Hide Sidebar (⌃⌘S)" : "Show Sidebar (⌃⌘S)")
        .accessibilityLabel("Sidebar")

        Button {
            rightPanelVisible.toggle()
        } label: {
            Image(systemName: "sidebar.right")
        }
        .buttonStyle(HoverIconButtonStyle())
        .help(rightPanelVisible ? "Hide right panel (⌃⌘I)" : "Show right panel (⌃⌘I)")
        .accessibilityLabel("Right panel")

        if workspaceEngineEnabled {
            universalSplitMenu
        } else {
            Button {
                model.workspaceSplitCurrent(.horizontal)
            } label: {
                Image(systemName: "square.split.1x2")
            }
            .buttonStyle(HoverIconButtonStyle())
            .help("Split terminal")
            .accessibilityLabel("Split terminal")
        }

        Button {
            model.settingsCategory = .permissions
            model.openSettings()
        } label: {
            Image(systemName: "lock.shield")
        }
        .buttonStyle(HoverIconButtonStyle())
        .help("Permissions")
        .accessibilityLabel("Permissions")
    }
```

- [ ] **Step 4: Put the strip in the layout and delete the safe-area machinery**

Replace `workspaceView` (lines 175-203) with:

```swift
    // HSplitView instead of NavigationSplitView: on macOS 26 the system sidebar
    // renders as an inset floating glass card with no opt-out; owning the split
    // is what lets every column be one of our own cards instead.
    private var workspaceView: some View {
        // The canvas is painted as the outermost layer, behind the whole split:
        // HSplitView (NSSplitView) clips each pane to its own bounds, so a
        // background nested inside a column could never reach the window edges.
        ZStack {
            CanvasBackground().ignoresSafeArea()
            VStack(spacing: 0) {
                if model.route == .workspace {
                    TitleStrip { titleStripButtons }
                }
                splitContent
                UsageBarView(
                    store: model.usage,
                    worktree: model.selectedWorktree,
                    onOpenSettings: { model.openSettings() })
            }
        }
    }
```

Then delete, in the same file:
- the `titlebarInset` property and its doc comment (lines 19-24)
- the `Color.clear.onGeometryChange { … titlebarInset = $0 }` block
- `.padding(.top, titlebarInset)` in the centre column
- `.ignoresSafeArea(.container, edges: .top)` in `splitContent` **and** the one on the centre column

Remove the two comment blocks that explain those calls along with them — they describe a problem that no longer exists.

- [ ] **Step 5: Build and run the existing app tests**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages 2>&1 | tail -5
```
Expected: `TEST SUCCEEDED`. If `DividerCursorStripTests` fails, note it and re-run once — it is a known order-dependent flake that races on `NSApp.currentEvent` under test parallelism, not a regression from this task.

- [ ] **Step 6: Commit**

```bash
git add App/TitleStrip.swift App/TillerApp.swift App/ContentView.swift
git commit -m "feat: replace the window toolbar with a 28pt canvas title strip"
```

---

### Task 6: Three cards, gaps, and divider alignment

`HSplitView` has no `spacing`, so the gap is padding split across the two columns that face it. The trap: `sidebarWidth` is measured on `SidebarView` itself, and the resize cursor strips are positioned from that number. Padding applied inside the column is invisible to that measurement, and the cursor lands half a gap away from the visible gap — the same "hover yes, cursor no" bug already fixed once in July.

**Files:**
- Create: `App/CardLayout.swift`
- Create: `AppTests/CardLayoutTests.swift`
- Modify: `App/ContentView.swift` (`splitContent`, `splitColumns`)

**Interfaces:**
- Consumes: `FloatingCard` (Task 3), `CanvasBackground` (Task 4), `AppTheme.cardGap` (Task 2).
- Produces: `CardLayout.dividerCenterX(columnWidth:gap:) -> CGFloat`.

- [ ] **Step 1: Write the failing test**

`AppTests/CardLayoutTests.swift`:

```swift
import Testing
@testable import Tiller

/// Half the gap sits inside each column, so a column's measured width already
/// contains its own half. The divider's centre is therefore the outer margin
/// plus that width — not the width alone, and not the width plus a whole gap.
@Test func dividerCentreLandsInTheMiddleOfTheVisibleGap() {
    #expect(CardLayout.dividerCenterX(columnWidth: 250, gap: 10) == 255)
}

/// With no gap at all the divider sits exactly on the column edge: the formula
/// degrades to today's flush layout instead of drifting.
@Test func dividerCentreIsTheColumnEdgeWhenThereIsNoGap() {
    #expect(CardLayout.dividerCenterX(columnWidth: 250, gap: 0) == 250)
}
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/dividerCentreLandsInTheMiddleOfTheVisibleGap 2>&1 | tail -20
```
Expected: compile error — `CardLayout` is not defined.

- [ ] **Step 3: Write the implementation**

`App/CardLayout.swift`:

```swift
import CoreGraphics

/// Where the split's divider sits once the columns are padded apart.
///
/// The gap is built from three paddings: half a gap inside each column, plus
/// half a gap on the outside of the split container. A column's measured width
/// includes its own halves, so from the container's leading edge the divider
/// centre is the outer half-gap plus that width.
enum CardLayout {
    static func dividerCenterX(columnWidth: CGFloat, gap: CGFloat) -> CGFloat {
        gap / 2 + columnWidth
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages \
  -only-testing:TillerTests/dividerCentreLandsInTheMiddleOfTheVisibleGap 2>&1 | tail -5
```
Expected: PASS.

- [ ] **Step 5: Wrap the sidebar and right panel in cards**

In `App/ContentView.swift`, `splitColumns()`. The sidebar column becomes:

```swift
            if sidebarVisible {
                FloatingCard { SidebarView(model: model) }
                    .padding(.horizontal, AppTheme.cardGap / 2)
                    .frame(
                        minWidth: CGFloat(AppSettings.sidebarWidthRange.lowerBound),
                        idealWidth: CGFloat(AppSettings.defaultSidebarWidth),
                        maxWidth: CGFloat(AppSettings.sidebarWidthRange.upperBound),
                        maxHeight: .infinity)
                    // Measured outside the padding, so the width includes the
                    // half-gap the divider maths expects.
                    .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                        sidebarWidth = $0
                    }
            }
```

The centre column loses its `ZStack { AppTheme.background … }` wrapper — the canvas behind the split already provides that — and becomes:

```swift
            FloatingCard {
                VStack(spacing: 0) {
                    if let worktree = model.selectedWorktree {
                        if !workspaceEngineEnabled {
                            TabBarView(model: model, worktree: worktree)
                            Divider()
                        }
                    }
                    if workspaceEngineEnabled {
                        workspaceStack
                            .contextMenu { paneContextMenu }
                    } else {
                        terminalStack
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .padding(.horizontal, AppTheme.cardGap / 2)
            .frame(minWidth: 320, maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
```

Keep the existing `.dropDestination(for: URL.self) { … }` on that column exactly as it is.

The right panel column keeps its `.frame` and its debounced `onGeometryChange` unchanged; wrap its `RightPanelView(…)` in `FloatingCard { }` and add `.padding(.horizontal, AppTheme.cardGap / 2)` before the `.frame`.

- [ ] **Step 6: Pad the container and re-aim the cursor strips**

Replace `splitContent` with:

```swift
    private var splitContent: some View {
        splitColumns()
            .padding(.horizontal, AppTheme.cardGap / 2)
            .padding(.bottom, AppTheme.cardGap)
            .overlay(alignment: .leading) {
                if sidebarVisible {
                    dividerCover
                        .offset(x: CardLayout.dividerCenterX(
                            columnWidth: sidebarWidth, gap: AppTheme.cardGap) - 1)
                    DividerCursorStrip()
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: CardLayout.dividerCenterX(
                            columnWidth: sidebarWidth, gap: AppTheme.cardGap)
                            - DividerCursorStrip.width / 2)
                }
            }
            .overlay(alignment: .trailing) {
                if rightPanelVisible {
                    dividerCover
                        .offset(x: -CardLayout.dividerCenterX(
                            columnWidth: liveRightPanelWidth, gap: AppTheme.cardGap) + 1)
                    DividerCursorStrip()
                        .frame(width: DividerCursorStrip.width)
                        .offset(x: -CardLayout.dividerCenterX(
                            columnWidth: liveRightPanelWidth, gap: AppTheme.cardGap)
                            + DividerCursorStrip.width / 2)
                }
            }
            .animation(.easeInOut(duration: 0.2), value: sidebarVisible)
            .animation(.easeInOut(duration: 0.2), value: rightPanelVisible)
    }

    /// NSSplitView draws its own hairline between columns. Inside a gap that is
    /// supposed to read as bare canvas, that line is the one thing giving the
    /// old flush layout away, so it gets painted over.
    private var dividerCover: some View {
        CanvasBackground()
            .frame(width: 2)
            .ignoresSafeArea()
            .allowsHitTesting(false)
    }
```

- [ ] **Step 7: Build and run the app tests**

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates \
  -clonedSourcePackagesDirPath DerivedData/SourcePackages 2>&1 | tail -5
```
Expected: `TEST SUCCEEDED`.

- [ ] **Step 8: Commit**

```bash
git add App/CardLayout.swift App/ContentView.swift AppTests/CardLayoutTests.swift
git commit -m "feat: float all three columns as cards on the canvas"
```

---

### Task 7: Projects header in the sidebar

`SidebarView` has its own `.toolbar`, which is where the Add Project button lives. `.hiddenTitleBar` takes that away, so the button needs a home — a header row above the filter field, since the filter acts on the projects the label names.

**Files:**
- Modify: `App/SidebarView.swift:20-25` (add the header), `App/SidebarView.swift:89-94` (delete the toolbar)

**Interfaces:**
- Consumes: `HoverIconButtonStyle` (already in `App/AppTheme.swift`), `AppTheme.meta`.
- Produces: nothing other tasks depend on.

- [ ] **Step 1: Add the header row**

In `App/SidebarView.swift`, inside `VStack(spacing: 0)`, immediately before `FilterField(text: $filterText)`:

```swift
                HStack(spacing: 4) {
                    Text("Projects")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(AppTheme.meta)
                    Spacer(minLength: 0)
                    Button {
                        showAddProjectSheet = true
                    } label: {
                        Image(systemName: "plus")
                    }
                    .buttonStyle(HoverIconButtonStyle())
                    .help("Add Project")
                    .accessibilityLabel("Add Project")
                }
                .padding(.horizontal, 10)
                .padding(.top, 8)
```

Then change the `FilterField`'s `.padding(.top, 8)` to `.padding(.top, 6)` — the header now supplies the top margin, and 8 on both would double it.

- [ ] **Step 2: Delete the sidebar toolbar**

Remove the whole modifier at `App/SidebarView.swift:89-94`:

```swift
            .toolbar {
                Button {
                    showAddProjectSheet = true
                } label: {
                    Label("Add Project", systemImage: "plus")
                }
            }
```

The `showAddProjectSheet` state and its `.sheet(isPresented:)` stay — only the button moved.

- [ ] **Step 3: Build**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO \
  -skipPackagePluginValidation -skipMacroValidation -skipPackageUpdates build 2>&1 | tail -5
```
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 4: Commit**

```bash
git add App/SidebarView.swift
git commit -m "feat: move add-project into a sidebar header row"
```

---

### Task 8: Full gate and visual verification

Everything above changes how the window looks, and almost none of it is assertable in a unit test. This task is where the design is actually checked.

**Files:** none modified unless a check fails.

- [ ] **Step 1: Run the full gate**

```bash
Scripts/ci.sh
```
Expected: `CI OK` as the final line.

If `DividerCursorStripTests` fails: run `Scripts/ci.sh` a second time on the same tree before investigating. It is a known order-dependent flake racing on `NSApp.currentEvent` under test parallelism. Two failures on the same tree means it is real; one means it is the flake.

- [ ] **Step 2: Launch and walk the visual checklist**

```bash
open DerivedData/Build/Products/Debug/Tiller.app
```

Check each, and capture a screenshot of the window for the record:

1. Traffic lights sit clear and vertically centred in the 28pt strip, not clipped by the cards.
2. The canvas is visible as a frame: 10pt left, right, and below the split; 28pt above.
3. The three cards are the same colour as each other, and the terminal's own background matches its card — no lighter rectangle inside.
4. Resize cursor appears over the gap on **both** dividers, and the drag still resizes.
5. Toggle the sidebar and the right panel: the frame stays uniform with either one hidden.
6. Usage bar text is legible directly on the canvas.
7. Switch to light appearance in Settings: cards sit on a darker canvas, not a lighter one.
8. Enable translucency in Settings: the desktop shows through the frame.
9. The `+` in the sidebar header opens the Add Project sheet.
10. Open a chat tab: its transcript cards (`AppTheme.cardFill`, `#343539`) now sit two steps above their background instead of one. Confirm they read as cards and not as blocks — this is a knock-on change nobody asked for, and the spec defers the judgement to here.

- [ ] **Step 3: Record the result**

If every check passes, the feature is done. If a check fails, fix it in a follow-up commit on this branch and re-run Step 1 — do not mark the plan complete with a known-failing check.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: <the specific check that failed>"
```
