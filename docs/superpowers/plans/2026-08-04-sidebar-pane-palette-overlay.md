# Sidebar and Central Pane Palette Overlay Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Apply the approved light/dark surface palette and render the central chat/terminal column as an inset floating card without changing divider geometry or hit behavior.

**Architecture:** AppSurfaceColor owns exact numeric/hex values shared with the terminal theme. AppTheme maps them to SwiftUI colors and owns the card geometry constants. A small FloatingMainSurface wrapper adds the inset card treatment; ContentView only places that wrapper inside its existing HSplitView item.

**Tech Stack:** Swift 6, SwiftUI/AppKit, swift-testing, Ghostty terminal configuration, XcodeGen, Scripts/ci.sh.

---

## File map

- Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift: exact dark/light chrome and central-surface tokens.
- Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift: token and chat/terminal equality tests.
- App/AppTheme.swift: appearance-aware SwiftUI colors and stable card geometry constants.
- App/FloatingMainSurface.swift: new reusable rounded-card material/border/shadow wrapper.
- App/ContentView.swift: place the wrapper inside the existing center HSplitView column.
- App/SidebarMaterialContainer.swift: update stale palette comments only; keep divider behavior unchanged.
- Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift: exact light terminal background.
- Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift: light-theme expectation.
- AppTests/FloatingMainSurfaceTests.swift: geometry contract.
- project.yml / generated Tiller.xcodeproj: regenerate after adding the new App source; never hand-edit the generated project.

### Task 1: Define the approved palette in tests first (RED)

Files: modify Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift; modify Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift; create AppTests/FloatingMainSurfaceTests.swift.

- [ ] Step 1: Replace old surface expectations with exact token assertions.

The core test file must assert the contract below (retain the existing opacity tests):

~~~swift
@Test func chromeMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.red == 8.0 / 255.0)
    #expect(AppSurfaceColor.green == 9.0 / 255.0)
    #expect(AppSurfaceColor.blue == 10.0 / 255.0)
    #expect(AppSurfaceColor.hex == "08090A")
    #expect(AppSurfaceColor.lightHex == "E9EAED")
}

@Test func chatSurfaceMatchesApprovedDarkAndLightValues() {
    #expect(AppSurfaceColor.chatRed == 16.0 / 255.0)
    #expect(AppSurfaceColor.chatGreen == 17.0 / 255.0)
    #expect(AppSurfaceColor.chatBlue == 18.0 / 255.0)
    #expect(AppSurfaceColor.chatHex == "101112")
    #expect(AppSurfaceColor.chatLightHex == "F6F6F8")
}

@Test func terminalSurfaceMatchesChatSurfaceInBothAppearances() {
    #expect(AppSurfaceColor.terminalRed == AppSurfaceColor.chatRed)
    #expect(AppSurfaceColor.terminalGreen == AppSurfaceColor.chatGreen)
    #expect(AppSurfaceColor.terminalBlue == AppSurfaceColor.chatBlue)
    #expect(AppSurfaceColor.terminalHex == AppSurfaceColor.chatHex)
    #expect(AppSurfaceColor.terminalLightHex == AppSurfaceColor.chatLightHex)
}
~~~

Remove the old warm-graphite name and the blue > red assertion because the approved dark palette is neutral.

- [ ] Step 2: Add the geometry RED test.

Create AppTests/FloatingMainSurfaceTests.swift:

~~~swift
import Testing
@testable import Tiller

@Test func centralSurfaceUsesApprovedFloatingCardGeometry() {
    #expect(AppTheme.mainSurfaceCornerRadius == 18)
    #expect(AppTheme.mainSurfaceHorizontalInset == 10)
    #expect(AppTheme.mainSurfaceVerticalInset == 12)
    #expect(AppTheme.mainSurfaceShadowRadius == 18)
    #expect(AppTheme.mainSurfaceShadowYOffset == 6)
}
~~~

- [ ] Step 3: Run RED before production edits.

~~~bash
cd Packages/TillerCore && swift test --filter AppSurfaceColorTests
cd ../TillerTerminal && swift test --filter TillerTerminalThemeTests
cd ../.. && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build-for-testing
~~~

Expected: failures mention the old 1A1A1E/121216 values, missing light hex properties, or missing card constants.

- [ ] Step 4: Commit the test contract.

~~~bash
git add Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift \
        Packages/TillerTerminal/Tests/TillerTerminalThemeTests/TillerTerminalThemeTests.swift \
        AppTests/FloatingMainSurfaceTests.swift
git commit -m "test: define approved surface palette"
~~~

### Task 2: Implement exact light/dark surface tokens (GREEN)

Files: modify Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift, App/AppTheme.swift, Packages/TillerTerminal/Sources/TillerTerminalTheme.swift, and its tests.

- [ ] Step 1: Update AppSurfaceColor while preserving existing dark property names.

Set red/green/blue to 8/255, 9/255, 10/255; set chatRed/chatGreen/chatBlue to 16/255, 17/255, 18/255. Add light properties and hex accessors:

~~~swift
public static let lightRed = 233.0 / 255.0
public static let lightGreen = 234.0 / 255.0
public static let lightBlue = 237.0 / 255.0
public static var lightHex: String {
    hexString(red: lightRed, green: lightGreen, blue: lightBlue)
}

public static let chatLightRed = 246.0 / 255.0
public static let chatLightGreen = 246.0 / 255.0
public static let chatLightBlue = 248.0 / 255.0
public static var chatLightHex: String {
    hexString(red: chatLightRed, green: chatLightGreen, blue: chatLightBlue)
}

public static let terminalLightRed = chatLightRed
public static let terminalLightGreen = chatLightGreen
public static let terminalLightBlue = chatLightBlue
public static var terminalLightHex: String {
    hexString(red: terminalLightRed, green: terminalLightGreen, blue: terminalLightBlue)
}
~~~

Keep terminalRed/Green/Blue aliased to the dark chat values and update comments to neutral near-black/near-white wording.

- [ ] Step 2: Map AppTheme to those values.

background and chromeTint must use the chrome pair; chatSurface and terminalSurface must use the central pair:

~~~swift
static let background = dynamic(
    light: NSColor(srgbRed: AppSurfaceColor.lightRed,
                   green: AppSurfaceColor.lightGreen,
                   blue: AppSurfaceColor.lightBlue,
                   alpha: 1),
    dark: NSColor(srgbRed: AppSurfaceColor.red,
                  green: AppSurfaceColor.green,
                  blue: AppSurfaceColor.blue,
                  alpha: 1))

static let terminalSurface = dynamic(
    light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                   green: AppSurfaceColor.chatLightGreen,
                   blue: AppSurfaceColor.chatLightBlue,
                   alpha: 1),
    dark: NSColor(srgbRed: AppSurfaceColor.terminalRed,
                  green: AppSurfaceColor.terminalGreen,
                  blue: AppSurfaceColor.terminalBlue,
                  alpha: 1))

static let chatSurface = dynamic(
    light: NSColor(srgbRed: AppSurfaceColor.chatLightRed,
                   green: AppSurfaceColor.chatLightGreen,
                   blue: AppSurfaceColor.chatLightBlue,
                   alpha: 1),
    dark: NSColor(srgbRed: AppSurfaceColor.chatRed,
                  green: AppSurfaceColor.chatGreen,
                  blue: AppSurfaceColor.chatBlue,
                  alpha: 1))

static let chromeTint = background
~~~

Add the constants required by the geometry test and an adaptive border color:

~~~swift
static let mainSurfaceCornerRadius: CGFloat = 18
static let mainSurfaceHorizontalInset: CGFloat = 10
static let mainSurfaceVerticalInset: CGFloat = 12
static let mainSurfaceShadowRadius: CGFloat = 18
static let mainSurfaceShadowYOffset: CGFloat = 6
static let mainSurfaceBorder = dynamic(
    light: NSColor.white.withAlphaComponent(0.72),
    dark: NSColor.white.withAlphaComponent(0.08))
~~~

- [ ] Step 3: Make the Ghostty light terminal surface exact.

In TillerTerminalTheme.theme, build the light branch as:

~~~swift
let lightConfiguration = TerminalConfiguration.alabaster
    .appending(.background(AppSurfaceColor.terminalLightHex))
    .appending(.fontSize(fontSize))
    .appending(scrollbackLimit)
~~~

Return the complete theme as follows, then update themeAppliesFontSizeToBothConfigurations to include the new light background command:

~~~swift
return TerminalTheme(
    light: lightConfiguration,
    dark: darkConfiguration
        .appending(.fontSize(fontSize))
        .appending(scrollbackLimit)
)
~~~

- [ ] Step 4: Run GREEN tests and commit.

~~~bash
cd Packages/TillerCore && swift test --filter AppSurfaceColorTests
cd ../TillerTerminal && swift test --filter TillerTerminalThemeTests
cd ../.. && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build-for-testing
git add Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift App/AppTheme.swift \
        Packages/TillerTerminal/Sources/TillerTerminal/TillerTerminalTheme.swift \
        Packages/TillerTerminal/Tests/TillerTerminalTests/TillerTerminalThemeTests.swift \
        Packages/TillerCore/Tests/TillerCoreTests/AppSurfaceColorTests.swift
git commit -m "feat: apply approved light and dark surface palette"
~~~

Expected: all focused tests pass and the app target compiles.

### Task 3: Add the floating central-surface wrapper

Files: create App/FloatingMainSurface.swift; regenerate Tiller.xcodeproj; use AppTests/FloatingMainSurfaceTests.swift from Task 1.

- [ ] Step 1: Add the focused wrapper.

~~~swift
import SwiftUI

struct FloatingMainSurface<Content: View>: View {
    @Environment(\.colorScheme) private var colorScheme
    private let tint: Color
    private let content: () -> Content

    init(tint: Color = AppTheme.terminalSurface,
         @ViewBuilder content: @escaping () -> Content) {
        self.tint = tint
        self.content = content
    }

    var body: some View {
        content()
            .background { MainSurfaceMaterial(tint: tint) }
            .clipShape(RoundedRectangle(
                cornerRadius: AppTheme.mainSurfaceCornerRadius,
                style: .continuous))
            .overlay {
                RoundedRectangle(
                    cornerRadius: AppTheme.mainSurfaceCornerRadius,
                    style: .continuous)
                    .stroke(AppTheme.mainSurfaceBorder, lineWidth: 1)
            }
            .shadow(
                color: .black.opacity(colorScheme == .dark ? 0.36 : 0.14),
                radius: AppTheme.mainSurfaceShadowRadius,
                y: AppTheme.mainSurfaceShadowYOffset)
    }
}
~~~

- [ ] Step 2: Regenerate and compile.

Run xcodegen generate, then:

~~~bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build-for-testing
~~~

Expected: the new source is included and the five geometry assertions pass.

- [ ] Step 3: Commit the wrapper.

~~~bash
git add App/FloatingMainSurface.swift AppTests/FloatingMainSurfaceTests.swift project.yml
git commit -m "feat: add floating central surface wrapper"
~~~

### Task 4: Place the card without changing divider geometry

Files: modify App/ContentView.swift in ContentView.splitContent; update only stale comments in App/SidebarMaterialContainer.swift.

- [ ] Step 1: Replace only the center HSplitView child shell.

Keep the existing tab bar, workspace/terminal stack, usage bar, context menu, URL filtering, and model.openDocument(fileURL:in:) body unchanged. Wrap that current VStack as follows:

~~~swift
ZStack {
    AppTheme.background

    FloatingMainSurface {
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
            if showUsageBar {
                Divider()
                UsageBarView(store: model.usage, worktree: model.selectedWorktree)
            }
        }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    .padding(.horizontal, AppTheme.mainSurfaceHorizontalInset)
    .padding(.vertical, AppTheme.mainSurfaceVerticalInset)
}
.frame(minWidth: 320, maxWidth: .infinity, minHeight: 160, maxHeight: .infinity)
~~~

Attach the existing dropDestination(for: URL.self) to the outer ZStack. The outer frame remains the HSplitView item; the card is only an inner visual surface.

- [ ] Step 2: Do not touch divider overlays.

Leave the leading/trailing overlay blocks, sidebarWidth and liveRightPanelWidth offsets, DividerCursorStrip.width, and visibility animations byte-for-byte equivalent. Update SidebarMaterialContainer comments from indigo/old palette language to neutral chrome wording without changing its modifiers.

- [ ] Step 3: Run divider/layout regression tests and commit.

~~~bash
cd Packages/TillerWorkspace && swift test --filter DividerCursorRectsTests
cd Packages/TillerWorkspace && swift test --filter DividerHitTestDiagnosticTests
cd Packages/TillerWorkspace && swift test --filter WorkspaceGeometryTests
git add App/ContentView.swift App/SidebarMaterialContainer.swift
git commit -m "feat: elevate central pane above sidebar chrome"
~~~

Expected: all three filters pass and no divider geometry code changes are present.

### Task 5: Full verification and visual acceptance

- [ ] Step 1: Run the focused suite and build.

~~~bash
cd Packages/TillerCore && swift test --filter AppSurfaceColorTests
cd ../TillerTerminal && swift test --filter TillerTerminalThemeTests
cd ../TillerWorkspace && swift test --filter DividerCursorRectsTests
cd ../TillerWorkspace && swift test --filter DividerHitTestDiagnosticTests
cd ../TillerWorkspace && swift test --filter WorkspaceGeometryTests
cd ../.. && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build
~~~

Expected: every command exits 0 and produces DerivedData/Build/Products/Debug/Tiller.app.

- [ ] Step 2: Run the repository gate.

Run Scripts/ci.sh; expected output ends with CI OK and exit status 0.

- [ ] Step 3: Inspect both appearances from the fresh build.

Launch with open -n /Users/enzopiopalmisano/Desktop/Progetti/tiller/DerivedData/Build/Products/Debug/Tiller.app, inspect Light and Dark appearances, and confirm:

- both sidebars/right chrome are E9EAED / 08090A;
- chat and terminal are F6F6F8 / 101112;
- the center is inset, rounded, bordered, and softly shadowed;
- divider cursors and dragging remain at the same boundaries.

- [ ] Step 4: Review the worktree.

Run git diff --check, git status --short, and git log -5 --oneline. Expected: no whitespace errors, intended palette/overlay files are present, and no generated project file was hand-edited.

## Plan self-review

- Exact dark/light values are covered by Task 1 tests and Task 2 implementation.
- Chat/terminal equality, including the Ghostty light background, is covered by package tests.
- Card radius, insets, border, and shadow are covered by Task 1 and implemented in Task 3.
- Divider geometry and cursor behavior are explicitly preserved and re-tested in Task 4.
- Build, CI, both appearance modes, and final worktree hygiene are covered by Task 5.
