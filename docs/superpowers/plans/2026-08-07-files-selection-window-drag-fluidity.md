# Files Selection and Window Drag Fluidity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Files-row selection react on the first click without waiting for double-click/drag recognition, and allow window movement only from the native title bar.

**Architecture:** Keep the existing SwiftUI file-row layout and external `URL` drag source, but isolate single-click selection in a reusable simultaneous gesture so it does not depend on the double-click recognizer attached to the file name. Move NSWindow surface configuration into one testable AppKit boundary and disable background window movement there; the bounded native titlebar accessories and unused native titlebar region remain unchanged.

**Tech Stack:** Swift 6, SwiftUI, AppKit, swift-testing, XcodeBuildMCP, repository `Scripts/ci.sh` gate.

---

## Scope and invariants

- Preserve file-row single selection, directory expand/collapse, keyboard navigation, file-name double-click open, context menu, and file drag to Finder/chat/terminal.
- Preserve `.fullSizeContentView`, transparent titlebar, clear window background, bounded native titlebar accessories, and native titlebar double-click behavior.
- Do not touch the pre-existing local edit in `App/TillerApp.swift` or the untracked `docs/superpowers/plans/2026-08-06-navigation-performance.md`.
- Do not add dependencies or edit generated `Tiller.xcodeproj` by hand.

### Task 1: Make Files selection independent from double-click and drag recognition

**Files:**
- Create: `AppTests/FileExplorerRowInteractionTests.swift`
- Modify: `App/RightPanel/FileExplorerView.swift:72-149`

- [ ] **Step 1: Write the failing interaction test**

Create a serialized `@MainActor` AppKit-hosted SwiftUI test. The test view must keep the production interaction shape: a child file-name double-click recognizer, a full-row `URL` drag source, and the new selection modifier on the row.

```swift
import AppKit
import SwiftUI
import Testing
@testable import Tiller

@Suite(.serialized)
@MainActor
struct FileExplorerRowInteractionTests {
    @Test func firstClickSelectsWithoutWaitingForDoubleClick() throws {
        let recorder = FileExplorerInteractionRecorder()
        let root = HStack {
            Text("file.swift")
                .onTapGesture(count: 2) { recorder.openCount += 1 }
        }
        .frame(width: 180, height: 28)
        .contentShape(Rectangle())
        .draggable(URL(fileURLWithPath: "/tmp/file.swift"))
        .fileExplorerRowSelection { recorder.selectionCount += 1 }

        let hostingView = NSHostingView(rootView: root)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 180, height: 28),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)
        window.layoutIfNeeded()
        defer { window.orderOut(nil); window.close() }

        let point = NSPoint(x: 90, y: 14)
        try sendFileRowClick(to: window, at: point, clickCount: 1)
        RunLoop.current.run(until: Date().addingTimeInterval(0.02))

        #expect(recorder.selectionCount == 1)
        #expect(recorder.openCount == 0)
    }
}

@MainActor
private final class FileExplorerInteractionRecorder {
    var selectionCount = 0
    var openCount = 0
}
```

The helper `sendFileRowClick` creates matching `.leftMouseDown` and `.leftMouseUp` events for the window number and sends them through `NSApp`. Keep it in the same test file.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
xcodebuildmcp macos test --project-path "$PWD/Tiller.xcodeproj" --scheme Tiller \
  --json '{"extraArgs":["-skipPackagePluginValidation","-skipMacroValidation","-skipPackageUpdates","-only-testing:TillerTests/FileExplorerRowInteractionTests"]}' \
  --output text
```

Expected: FAIL at compile time because `fileExplorerRowSelection` does not exist yet.

- [ ] **Step 3: Implement the minimal simultaneous selection modifier**

Add beside `FileExplorerView` in `App/RightPanel/FileExplorerView.swift`:

```swift
private struct FileExplorerRowSelectionModifier: ViewModifier {
    let action: () -> Void

    func body(content: Content) -> some View {
        content.simultaneousGesture(
            TapGesture(count: 1).onEnded(action)
        )
    }
}

extension View {
    func fileExplorerRowSelection(action: @escaping () -> Void) -> some View {
        modifier(FileExplorerRowSelectionModifier(action: action))
    }
}
```

Replace the outer row `.onTapGesture` with `.fileExplorerRowSelection`, preserving its exact state updates and directory toggle:

```swift
.fileExplorerRowSelection {
    selectedPath = node.relativePath
    treeFocused = true
    if node.kind.isDirectory {
        Task { await panelModel.toggleDirectory(node.relativePath) }
    }
}
```

Do not remove the existing file-name double-click or full-row `.draggable` modifier.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the command from Step 2. Expected: PASS with one test executed.

### Task 2: Restrict window movement to the native title bar

**Files:**
- Modify: `AppTests/WindowChromeConfiguratorTests.swift`
- Modify: `App/WindowChromeConfigurator.swift:249-266`

- [ ] **Step 1: Write the failing window-configuration test**

Add to `WindowChromeConfiguratorTests`:

```swift
@Test func configuredWindowDoesNotMoveFromContentBackground() {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
        styleMask: [.titled, .closable, .resizable],
        backing: .buffered,
        defer: false)
    window.isMovableByWindowBackground = true

    configureWindowSurface(window)

    #expect(window.styleMask.contains(.fullSizeContentView))
    #expect(window.titlebarAppearsTransparent)
    #expect(window.isOpaque == false)
    #expect(window.backgroundColor == .clear)
    #expect(window.isMovableByWindowBackground == false)
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```bash
xcodebuildmcp macos test --project-path "$PWD/Tiller.xcodeproj" --scheme Tiller \
  --json '{"extraArgs":["-skipPackagePluginValidation","-skipMacroValidation","-skipPackageUpdates","-only-testing:TillerTests/WindowChromeConfiguratorTests/configuredWindowDoesNotMoveFromContentBackground"]}' \
  --output text
```

Expected: FAIL at compile time because `configureWindowSurface` does not exist.

- [ ] **Step 3: Extract and apply the AppKit surface configuration**

Add an internal `@MainActor` function in `App/WindowChromeConfigurator.swift`:

```swift
@MainActor
func configureWindowSurface(_ window: NSWindow) {
    window.styleMask.insert(.fullSizeContentView)
    window.titlebarAppearsTransparent = true
    window.isOpaque = false
    window.backgroundColor = .clear
    window.isMovableByWindowBackground = false
}
```

Call `configureWindowSurface(window)` from `WindowChromeConfigurator.makeNSView` and remove the duplicated property assignments there. Keep `installHideOnClose`, titlebar accessory installation, and accessory updates in their existing order.

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the command from Step 2. Expected: PASS.

### Task 3: Regression and repository verification

**Files:** none unless a test exposes a defect in the two scoped files above.

- [ ] **Step 1: Regenerate the project**

Run `xcodegen generate`. Expected: exit 0. Never hand-edit `Tiller.xcodeproj`.

- [ ] **Step 2: Run all app tests through XcodeBuildMCP**

Run:

```bash
xcodebuildmcp macos test --project-path "$PWD/Tiller.xcodeproj" --scheme Tiller \
  --json '{"extraArgs":["-skipPackagePluginValidation","-skipMacroValidation","-skipPackageUpdates"]}' \
  --output text
```

Expected: all `TillerTests` pass and the tool reports success.

- [ ] **Step 3: Run the repository gate**

Run `Scripts/ci.sh`. Expected: exit 0 with final line `CI OK`.

- [ ] **Step 4: Manual macOS interaction check**

1. Single-click file names repeatedly in Files: selection highlight changes on the first click without waiting for a second click.
2. Double-click a file name: it opens once in an editor tab.
3. Click a directory: it selects and expands/collapses once.
4. Drag a file row to Finder/chat/terminal: the URL payload remains usable.
5. Drag from workspace/sidebar/right-panel content: the window does not move.
6. Drag from empty native titlebar space: the window moves normally.
7. Double-click empty native titlebar space: macOS follows the configured system action.

If desktop automation is unavailable, report this step as not executed rather than inferring success from tests.

