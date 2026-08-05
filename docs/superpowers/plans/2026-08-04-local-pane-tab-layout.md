# Local Pane Tab Layout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the universal workspace's pill-like pane tabs with stable, connected local tab chrome that distinguishes active tab, focused pane, agent activity, overflow, and empty-pane creation without changing content lifecycle behavior.

**Architecture:** `WorkspaceLayout` remains the source of truth. `TillerWorkspace` exposes content identity, focused-group state, deterministic sizing, and app-provided strip/empty-state factories; the app target resolves dirty and agent presentation and renders SwiftUI chrome. Existing `WorkspaceIntent`, drag coordinator, close adapters, and layout-engine selection repair remain authoritative.

**Tech Stack:** Swift 6, SwiftUI, AppKit hosting views, Observation, `swift-testing`, XcodeGen, macOS 15+

**Reference spec:** `docs/superpowers/specs/2026-08-04-local-pane-tab-layout-design.md`

---

## File map

### Create

- `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripLayout.swift` — pure sizing and overflow policy shared by the strip model and view.
- `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripLayoutTests.swift` — deterministic width, overflow, and active-entry coverage.
- `App/Workspace/PaneTabPresentation.swift` — app-layer resolution of content icon, dirty state, agent identity/status, and accessibility text.
- `App/PaneTabStatusGlyph.swift` — semantic, reduced-motion-aware SwiftUI status glyph.
- `App/PaneEmptyStateView.swift` — centered creation affordances for the valid empty root pane.
- `AppTests/Workspace/PaneTabPresentationTests.swift` — pure presentation and accessibility tests using injected lookups.

### Modify

- `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripView.swift` — carry optional content identity in each `TabMenuEntry`.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift` — publish focused-group, viewport, content-width, overflow, and active-entry state; define the empty-state factory boundary.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift` — update focus state and mount/unmount the app-provided empty state.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift` — pass active-group state and the empty-state factory to each group controller.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift` — accept the empty-state factory from the app composition root.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift` — forward that factory to the reconciler.
- `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabChromeTests.swift` — content, focus, and empty-state contract tests.
- `App/Workspace/WorkspaceCoordinator.swift` — expose universal document dirty state for presentation only.
- `App/PaneTabStripBar.swift` — connected tab shape, icons, semantic status, fixed controls, overflow menu, reveal-on-activation, and accessibility.
- `App/ContentView.swift` — inject `AppModel`, coordinator, worktree, and empty-state menu into the app-side factories.
- `App/AppTheme.swift` — add dedicated tab focus and attention tokens instead of overloading selection colors.

No `project.yml` edit is required: the app and test targets already glob `App/` and `AppTests/`.

---

### Task 1: Carry content identity and focused-pane state to each strip

**Files:**
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripView.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabChromeTests.swift`

- [ ] **Step 1: Write failing content and focus propagation tests**

Add these tests to `PaneTabChromeTests`:

```swift
@Test func entriesCarryTheWorkspaceContentReference() throws {
    let only = tab("one")
    let controller = PaneGroupController(id: PaneGroupID())

    controller.update(
        group: group([only], active: only),
        isFocused: true,
        hostProvider: EmptyHostProvider())

    let entry = try #require(controller.tabEntries.first)
    #expect(entry.content == only.content)
    #expect(controller.strip.isFocusedGroup)
}

@Test func theReconcilerMarksOnlyTheActiveGroupAsFocused() throws {
    let first = tab("one")
    let second = tab("two")
    let firstGroup = group([first], active: first)
    let secondGroup = group([second], active: second)
    let split = SplitID()
    let layout = try #require(try WorkspaceLayout.make(
        root: .split(
            id: split, axis: .horizontal, fraction: 0.5,
            first: .group(firstGroup.id), second: .group(secondGroup.id)),
        groups: [firstGroup.id: firstGroup, secondGroup.id: secondGroup],
        activeGroupID: secondGroup.id
    ).get())
    let reconciler = WorkspaceReconciler(hostProvider: EmptyHostProvider())

    reconciler.reconcile(to: layout, delta: nil)

    #expect(reconciler.groupController(firstGroup.id)?.strip.isFocusedGroup == false)
    #expect(reconciler.groupController(secondGroup.id)?.strip.isFocusedGroup == true)
}
```

Update the existing direct `controller.update(...)` calls in this test file to pass `isFocused: false`.

- [ ] **Step 2: Run the focused suite and verify the new contract is red**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabChromeTests
```

Expected: compilation fails because `TabMenuEntry.content`, `PaneTabStripModel.isFocusedGroup`, and the `isFocused:` update parameter do not exist.

- [ ] **Step 3: Extend `TabMenuEntry` without breaking synthetic test entries**

Replace its stored properties and initializer in `PaneTabStripView.swift` with:

```swift
public struct TabMenuEntry: Equatable, Sendable {
    public let tabID: WorkspaceTabID
    public let title: String
    public let isActive: Bool
    public let content: WorkspaceContentRef?

    public init(
        tabID: WorkspaceTabID,
        title: String,
        isActive: Bool,
        content: WorkspaceContentRef? = nil
    ) {
        self.tabID = tabID
        self.title = title
        self.isActive = isActive
        self.content = content
    }
}
```

Make real engine entries carry the content:

```swift
TabMenuEntry(
    tabID: tab.id,
    title: tab.title,
    isActive: tab.id == group.activeTabID,
    content: tab.content
)
```

- [ ] **Step 4: Publish and propagate the focused group state**

Add to `PaneTabStripModel`:

```swift
public internal(set) var isFocusedGroup = false
```

Change `PaneGroupController.update` to:

```swift
func update(
    group: PaneGroup,
    isFocused: Bool,
    hostProvider: WorkspaceHostProvider
) {
    tabEntries = PaneTabStripView.overflowMenuItems(for: group)
    stripModel.entries = tabEntries
    stripModel.isFocusedGroup = isFocused
    // Keep the existing host reconciliation body unchanged below this point.
}
```

In `WorkspaceReconciler.build`, call it with the layout's active group:

```swift
controller.update(
    group: group,
    isFocused: id == layout.activeGroupID,
    hostProvider: hostProvider
)
```

Update direct test calls to the new signature; do not add a default value, so future call sites must make focus ownership explicit.

- [ ] **Step 5: Run the package tests**

Run:

```bash
cd Packages/TillerWorkspace && swift test
```

Expected: all `TillerWorkspace` tests pass.

- [ ] **Step 6: Commit the model boundary**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripView.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabChromeTests.swift
git commit -m "feat: expose pane tab focus and content"
```

---

### Task 2: Add deterministic sizing and overflow state

**Files:**
- Create: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripLayout.swift`
- Create: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripLayoutTests.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift`

- [ ] **Step 1: Write the failing sizing and overflow tests**

Create `PaneTabStripLayoutTests.swift`:

```swift
import CoreGraphics
import Testing
import TillerCore
@testable import TillerWorkspace

@Suite @MainActor
struct PaneTabStripLayoutTests {
    @Test func activeTabsKeepTheLargerMinimumWidth() {
        #expect(PaneTabStripLayout.minimumWidth(isActive: false) == 72)
        #expect(PaneTabStripLayout.minimumWidth(isActive: true) == 118)
        #expect(PaneTabStripLayout.maximumWidth == 160)
        #expect(PaneTabStripLayout.closeControlWidth == 16)
    }

    @Test func overflowUsesAToleranceForFractionalLayoutNoise() {
        #expect(!PaneTabStripLayout.isOverflowing(contentWidth: 200.5, viewportWidth: 200))
        #expect(PaneTabStripLayout.isOverflowing(contentWidth: 202, viewportWidth: 200))
    }

    @Test func theModelPublishesOverflowAndTheActiveEntry() {
        let active = WorkspaceTabID()
        let model = PaneTabStripModel()
        model.entries = [TabMenuEntry(tabID: active, title: "active", isActive: true)]

        model.updateContentWidth(240)
        model.updateViewportWidth(160)

        #expect(model.activeTabID == active)
        #expect(model.isOverflowing)
    }

    @Test func negativeMeasurementsClampToZero() {
        let model = PaneTabStripModel()

        model.updateContentWidth(-10)
        model.updateViewportWidth(-20)

        #expect(model.contentWidth == 0)
        #expect(model.viewportWidth == 0)
        #expect(!model.isOverflowing)
    }
}
```

- [ ] **Step 2: Run the new suite and verify it fails**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabStripLayoutTests
```

Expected: compilation fails because `PaneTabStripLayout` and the model width APIs are missing.

- [ ] **Step 3: Implement the pure layout policy**

Create `PaneTabStripLayout.swift`:

```swift
import CoreGraphics

public enum PaneTabStripLayout {
    public static let inactiveMinimumWidth: CGFloat = 72
    public static let activeMinimumWidth: CGFloat = 118
    public static let maximumWidth: CGFloat = 160
    public static let closeControlWidth: CGFloat = 16
    public static let overflowTolerance: CGFloat = 1

    public static func minimumWidth(isActive: Bool) -> CGFloat {
        isActive ? activeMinimumWidth : inactiveMinimumWidth
    }

    public static func isOverflowing(
        contentWidth: CGFloat,
        viewportWidth: CGFloat
    ) -> Bool {
        contentWidth > viewportWidth + overflowTolerance
    }
}
```

- [ ] **Step 4: Put width measurements and active identity on the strip model**

Add to `PaneTabStripModel`:

```swift
public private(set) var contentWidth: CGFloat = 0
public private(set) var viewportWidth: CGFloat = 0

public var activeTabID: WorkspaceTabID? {
    entries.first(where: \.isActive)?.tabID
}

public var isOverflowing: Bool {
    PaneTabStripLayout.isOverflowing(
        contentWidth: contentWidth,
        viewportWidth: viewportWidth
    )
}

public func updateContentWidth(_ width: CGFloat) {
    contentWidth = max(0, width)
}

public func updateViewportWidth(_ width: CGFloat) {
    viewportWidth = max(0, width)
}
```

- [ ] **Step 5: Run the focused and full package tests**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabStripLayoutTests
cd Packages/TillerWorkspace && swift test
```

Expected: both commands pass.

- [ ] **Step 6: Commit the layout policy**

```bash
git add Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripLayout.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripLayoutTests.swift
git commit -m "feat: model pane tab overflow"
```

---

### Task 3: Resolve app-specific tab presentation without crossing package boundaries

**Files:**
- Create: `App/Workspace/PaneTabPresentation.swift`
- Create: `AppTests/Workspace/PaneTabPresentationTests.swift`
- Modify: `App/Workspace/WorkspaceCoordinator.swift`

- [ ] **Step 1: Write failing resolver tests with injected activity lookups**

Create `PaneTabPresentationTests.swift`:

```swift
import Foundation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite @MainActor
struct PaneTabPresentationTests {
    @Test func terminalPresentationUsesTheLivePaneForAgentState() {
        let tabID = WorkspaceTabID()
        let contentID = TerminalContentID()
        let livePane = UUID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Codex", isActive: true,
            content: .terminal(contentID))
        let resolver = makeResolver(
            terminalPane: livePane,
            statuses: [livePane: .running],
            agents: [livePane: "codex"])

        let result = resolver.resolve(entry)

        #expect(result.icon == .terminal(agentID: "codex"))
        #expect(result.agentStatus == .running)
        #expect(!result.isDirty)
    }

    @Test func chatUsesItsTabIDAsTheActivityPaneID() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Review", isActive: false,
            content: .chat(ChatContentID("session")))
        let resolver = makeResolver(
            statuses: [tabID.rawValue: .needsInput],
            agents: [tabID.rawValue: "claude"])

        let result = resolver.resolve(entry)

        #expect(result.icon == .chat(agentID: "claude"))
        #expect(result.agentStatus == .needsInput)
    }

    @Test func documentPresentationCarriesEditorAndDirtyState() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "README.md", isActive: false,
            content: .document(
                DocumentID.makeCanonical(
                    worktreeID: UUID(), path: "/tmp/README.md"),
                editor: .markdown))
        let resolver = makeResolver(dirtyTabs: [tabID])

        let result = resolver.resolve(entry)

        #expect(result.icon == .document(.markdown))
        #expect(result.isDirty)
        #expect(result.accessibilityLabel(for: entry).contains("modified"))
    }

    @Test func accessibilityNamesSelectionAndAgentStatus() {
        let tabID = WorkspaceTabID()
        let entry = TabMenuEntry(
            tabID: tabID, title: "Fix tests", isActive: true,
            content: .chat(ChatContentID("chat")))
        let resolver = makeResolver(statuses: [tabID.rawValue: .error])

        let label = resolver.resolve(entry).accessibilityLabel(for: entry)

        #expect(label == "Chat, Fix tests, selected, error")
    }

    private func makeResolver(
        terminalPane: UUID? = nil,
        dirtyTabs: Set<WorkspaceTabID> = [],
        statuses: [UUID: AgentStatus] = [:],
        agents: [UUID: String] = [:]
    ) -> PaneTabPresentationResolver {
        PaneTabPresentationResolver(
            isDirty: { dirtyTabs.contains($0) },
            liveTerminalPane: { _ in terminalPane },
            status: { ids in AgentStatus.highestPriority(in: ids.compactMap { statuses[$0] }) },
            agentID: { ids in ids.compactMap { agents[$0] }.first })
    }
}
```

- [ ] **Step 2: Regenerate the project and verify the test is red**

Run:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/PaneTabPresentationTests
```

Expected: compilation fails because the presentation types do not exist.

- [ ] **Step 3: Implement the pure app-layer presentation types**

Create `App/Workspace/PaneTabPresentation.swift`:

```swift
import Foundation
import TillerCore
import TillerWorkspace

enum PaneTabIconKind: Equatable {
    case terminal(agentID: String?)
    case chat(agentID: String?)
    case document(DocumentEditorKind)
}

struct PaneTabPresentation: Equatable {
    let icon: PaneTabIconKind
    let isDirty: Bool
    let agentStatus: AgentStatus?

    var kindLabel: String {
        switch icon {
        case .terminal: "Terminal"
        case .chat: "Chat"
        case .document(.markdown): "Markdown document"
        case .document(.code): "Code document"
        }
    }

    func accessibilityLabel(for entry: TabMenuEntry) -> String {
        return [
            kindLabel,
            entry.title,
            entry.isActive ? "selected" : nil,
            isDirty ? "modified" : nil,
            agentStatus?.humanLabel
        ].compactMap { $0 }.joined(separator: ", ")
    }
}

@MainActor
struct PaneTabPresentationResolver {
    let isDirty: (WorkspaceTabID) -> Bool
    let liveTerminalPane: (TerminalContentID) -> UUID?
    let status: ([UUID]) -> AgentStatus?
    let agentID: ([UUID]) -> String?

    func resolve(_ entry: TabMenuEntry) -> PaneTabPresentation {
        switch entry.content {
        case .terminal(let contentID):
            let paneIDs = liveTerminalPane(contentID).map { [$0] } ?? []
            let agent = agentID(paneIDs)
            return PaneTabPresentation(
                icon: .terminal(agentID: agent),
                isDirty: false,
                agentStatus: status(paneIDs))
        case .chat:
            let paneIDs = [entry.tabID.rawValue]
            let agent = agentID(paneIDs)
            return PaneTabPresentation(
                icon: .chat(agentID: agent),
                isDirty: false,
                agentStatus: status(paneIDs))
        case .document(_, let editor):
            return PaneTabPresentation(
                icon: .document(editor),
                isDirty: isDirty(entry.tabID),
                agentStatus: nil)
        case nil:
            return PaneTabPresentation(
                icon: .terminal(agentID: nil),
                isDirty: false,
                agentStatus: nil)
        }
    }
}
```

- [ ] **Step 4: Expose universal document dirty state through the coordinator**

Add this read-only helper near the other universal-tab lookup methods in `WorkspaceCoordinator`:

```swift
func isDocumentDirty(tabID: WorkspaceTabID) -> Bool {
    guard let adapter = adapters[.document] as? DocumentContentAdapter else { return false }
    return adapter.isDirty(tabID: tabID)
}
```

This is presentation data only; do not move dirty-close policy out of `AppModel.closeTab`.

- [ ] **Step 5: Run the focused app tests**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/PaneTabPresentationTests
```

Expected: `PaneTabPresentationTests` passes.

- [ ] **Step 6: Commit the presentation resolver**

```bash
git add App/Workspace/PaneTabPresentation.swift App/Workspace/WorkspaceCoordinator.swift AppTests/Workspace/PaneTabPresentationTests.swift
git commit -m "feat: resolve pane tab presentation"
```

---

### Task 4: Render connected tabs with distinct focus and semantic status

**Files:**
- Create: `App/PaneTabStatusGlyph.swift`
- Modify: `App/AppTheme.swift`
- Modify: `App/PaneTabStripBar.swift`
- Modify: `App/ContentView.swift`
- Test: `AppTests/Workspace/PaneTabPresentationTests.swift`

- [ ] **Step 1: Add failing semantic-glyph mapping tests**

Append to `PaneTabPresentationTests`:

```swift
@Test func everyAgentStatusHasADistinctSemanticGlyph() {
    #expect(PaneTabStatusGlyphKind.forStatus(nil) == .none)
    #expect(PaneTabStatusGlyphKind.forStatus(.running) == .running)
    #expect(PaneTabStatusGlyphKind.forStatus(.needsInput) == .needsInput)
    #expect(PaneTabStatusGlyphKind.forStatus(.done) == .done)
    #expect(PaneTabStatusGlyphKind.forStatus(.error) == .error)
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/PaneTabPresentationTests
```

Expected: compilation fails because `PaneTabStatusGlyphKind` does not exist.

- [ ] **Step 3: Implement the status glyph with shape and reduced-motion behavior**

Create `App/PaneTabStatusGlyph.swift`:

```swift
import SwiftUI
import TillerCore

enum PaneTabStatusGlyphKind: Equatable {
    case none, running, needsInput, done, error

    static func forStatus(_ status: AgentStatus?) -> Self {
        switch status {
        case nil: .none
        case .running: .running
        case .needsInput: .needsInput
        case .done: .done
        case .error: .error
        }
    }
}

struct PaneTabStatusGlyph: View {
    let status: AgentStatus?
    let agentID: String?

    var body: some View {
        Group {
            switch PaneTabStatusGlyphKind.forStatus(status) {
            case .none:
                Color.clear
            case .running:
                RunningDots(color: AgentIcon.color(for: agentID ?? ""), dotSize: 3)
            case .needsInput:
                Image(systemName: "exclamationmark")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(AppTheme.tabNeedsInput)
            case .done:
                Image(systemName: "checkmark")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(AppTheme.tabDone)
            case .error:
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(AppTheme.tabError)
            }
        }
        .frame(width: 14, height: 14)
        .help(status?.humanLabel ?? "")
        .accessibilityLabel(status?.humanLabel ?? "No agent activity")
        .accessibilityHidden(status == nil)
    }
}
```

`RunningDots` already honors Reduce Motion; do not add a second animation loop.

- [ ] **Step 4: Add dedicated theme tokens**

Add adaptive colors to `AppTheme`:

```swift
static let tabFocusAccent = dynamic(
    light: NSColor(srgbRed: 0.24, green: 0.38, blue: 0.78, alpha: 1),
    dark: NSColor(srgbRed: 0.55, green: 0.64, blue: 1.00, alpha: 1))
static let tabNeedsInput = dynamic(
    light: NSColor(srgbRed: 0.67, green: 0.42, blue: 0.02, alpha: 1),
    dark: NSColor(srgbRed: 0.95, green: 0.72, blue: 0.28, alpha: 1))
static let tabDone = dynamic(
    light: NSColor(srgbRed: 0.10, green: 0.45, blue: 0.22, alpha: 1),
    dark: NSColor(srgbRed: 0.48, green: 0.78, blue: 0.57, alpha: 1))
static let tabError = dynamic(
    light: NSColor(srgbRed: 0.68, green: 0.12, blue: 0.17, alpha: 1),
    dark: NSColor(srgbRed: 0.94, green: 0.43, blue: 0.47, alpha: 1))
```

- [ ] **Step 5: Inject the app presentation inputs into the strip factory**

Change the `makePaneTabStrip` API and `PaneTabStripBar` stored properties:

```swift
struct PaneTabStripBar<NewTabMenu: View>: View {
    @Bindable var model: PaneTabStripModel
    @Bindable var appModel: AppModel
    let workspaceCoordinator: WorkspaceCoordinator
    let worktree: Worktree
    @ViewBuilder var newTabMenu: () -> NewTabMenu

    private var resolver: PaneTabPresentationResolver {
        PaneTabPresentationResolver(
            isDirty: workspaceCoordinator.isDocumentDirty,
            liveTerminalPane: {
                workspaceCoordinator.liveControlPaneId(
                    contentID: $0, in: worktree.id)
            },
            status: { appModel.agentActivity.statusForWorktree(paneIds: $0) },
            agentID: { appModel.agentActivity.agentIdForWorktree(paneIds: $0) })
    }
}

@MainActor
func makePaneTabStrip<NewTabMenu: View>(
    _ model: PaneTabStripModel,
    appModel: AppModel,
    workspaceCoordinator: WorkspaceCoordinator,
    worktree: Worktree,
    @ViewBuilder newTabMenu: @escaping () -> NewTabMenu
) -> NSView {
    NSHostingView(rootView: PaneTabStripBar(
        model: model,
        appModel: appModel,
        workspaceCoordinator: workspaceCoordinator,
        worktree: worktree,
        newTabMenu: newTabMenu))
}
```

Update `ContentView.workspaceStack` to pass `model`, `workspaceCoordinator`, and `worktree` to the factory.

- [ ] **Step 6: Replace the pill item with connected tab anatomy**

Change `PaneTabStripItem` to accept `presentation` and `isFocusedGroup`. Preserve its existing drag callbacks and reserved close frame. The central body should be:

```swift
HStack(spacing: 6) {
    PaneTabIcon(presentation: presentation)
        .frame(width: 14, height: 14)

    Text(entry.title)
        .font(.system(size: 12))
        .foregroundStyle(entry.isActive ? AppTheme.titleSelected : AppTheme.subtitle)
        .lineLimit(1)
        .truncationMode(.tail)

    if presentation.isDirty {
        Circle().fill(AppTheme.meta).frame(width: 5, height: 5)
    }

    PaneTabStatusGlyph(
        status: presentation.agentStatus,
        agentID: presentation.icon.agentID)

    if hovering {
        Button(action: onClose) {
            Image(systemName: "xmark")
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(AppTheme.meta)
        }
        .buttonStyle(HoverIconButtonStyle())
        .frame(
            width: PaneTabStripLayout.closeControlWidth,
            height: PaneTabStripLayout.closeControlWidth)
        .help("Close tab (⌘W)")
    } else {
        Color.clear.frame(
            width: PaneTabStripLayout.closeControlWidth,
            height: PaneTabStripLayout.closeControlWidth)
    }
}
.padding(.horizontal, 9)
.frame(height: 25)
.contentShape(Rectangle())
.background {
    ConnectedPaneTabShape(cornerRadius: 7)
        .fill(entry.isActive ? AppTheme.terminalSurface : (hovering ? AppTheme.rowHover : .clear))
        .overlay {
            if entry.isActive {
                ConnectedPaneTabShape(cornerRadius: 7)
                    .stroke(AppTheme.hairline, lineWidth: 1)
            }
        }
        .overlay(alignment: .top) {
            if entry.isActive && isFocusedGroup {
                Capsule()
                    .fill(AppTheme.tabFocusAccent)
                    .frame(height: 2)
                    .padding(.horizontal, 5)
            }
        }
        .overlay(alignment: .bottom) {
            if entry.isActive {
                Rectangle()
                    .fill(AppTheme.terminalSurface)
                    .frame(height: 1)
            }
        }
}
.accessibilityLabel(presentation.accessibilityLabel(for: entry))
.accessibilityAddTraits(entry.isActive ? .isSelected : [])
```

Add these private view types in the same file:

```swift
private struct PaneTabIcon: View {
    let presentation: PaneTabPresentation

    @ViewBuilder
    var body: some View {
        switch presentation.icon {
        case .terminal(let agentID):
            if let agentID {
                AgentIcon(agentId: agentID, size: 12)
            } else {
                Image(systemName: "terminal")
                    .font(.system(size: 10))
                    .foregroundStyle(AppTheme.meta)
            }
        case .chat(let agentID):
            if let agentID {
                AgentIcon(agentId: agentID, size: 12)
            } else {
                Image(systemName: "bubble.left")
                    .font(.system(size: 10))
                    .foregroundStyle(AppTheme.meta)
            }
        case .document(.markdown):
            Image(systemName: "doc.text")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        case .document(.code):
            Image(systemName: "chevron.left.forwardslash.chevron.right")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        }
    }
}

private struct ConnectedPaneTabShape: Shape {
    let cornerRadius: CGFloat

    func path(in rect: CGRect) -> Path {
        let radius = min(cornerRadius, rect.width / 2, rect.height)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + radius, y: rect.minY),
            control: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY + radius),
            control: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}
```

Give `PaneTabIconKind` this helper in `PaneTabPresentation.swift`:

```swift
var agentID: String? {
    switch self {
    case .terminal(let id), .chat(let id): id
    case .document: nil
    }
}
```

Overlay one point of `AppTheme.terminalSurface` at the active tab's bottom edge so the strip divider does not cut through the selected tab.

- [ ] **Step 7: Run focused tests and build the app**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/PaneTabPresentationTests
xcodebuild build -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO
```

Expected: presentation tests pass and the app builds.

- [ ] **Step 8: Commit the connected chrome**

```bash
git add App/PaneTabStatusGlyph.swift App/AppTheme.swift App/PaneTabStripBar.swift App/ContentView.swift App/Workspace/PaneTabPresentation.swift AppTests/Workspace/PaneTabPresentationTests.swift
git commit -m "feat: render connected pane tabs"
```

---

### Task 5: Add adaptive widths, active reveal, and fixed overflow controls

**Files:**
- Modify: `App/PaneTabStripBar.swift`
- Modify: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripLayoutTests.swift`

- [ ] **Step 1: Add a red test for the overflow control contract**

Append to `PaneTabStripLayoutTests`:

```swift
@Test func theOverflowControlAppearsOnlyWhenTheSequenceDoesNotFit() {
    let model = PaneTabStripModel()

    model.updateContentWidth(180)
    model.updateViewportWidth(200)
    #expect(!model.showsOverflowMenu)

    model.updateContentWidth(240)
    #expect(model.showsOverflowMenu)
}
```

- [ ] **Step 2: Run the layout suite and verify the new contract is red**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabStripLayoutTests
```

Expected: compilation fails because `PaneTabStripModel.showsOverflowMenu` does not exist.

- [ ] **Step 3: Expose the view-facing overflow decision**

Add to `PaneTabStripModel`:

```swift
public var showsOverflowMenu: Bool { isOverflowing }
```

- [ ] **Step 4: Make the tab sequence scrollable while controls remain fixed**

Replace the leading section of `PaneTabStripBar.body` with this structure:

```swift
HStack(spacing: 0) {
    ScrollViewReader { proxy in
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 2) {
                ForEach(model.entries, id: \.tabID) { entry in
                    let presentation = resolver.resolve(entry)
                    PaneTabStripItem(
                        entry: entry,
                        presentation: presentation,
                        isFocusedGroup: model.isFocusedGroup,
                        onClose: { model.onClose(entry.tabID) },
                        onFrameChange: { model.setTabFrame($0, for: entry.tabID) },
                        onDragChanged: { model.onDragChanged(entry.tabID, $0) },
                        onDragEnded: { model.onDragEnded(entry.tabID) })
                        .frame(
                            minWidth: PaneTabStripLayout.minimumWidth(isActive: entry.isActive),
                            maxWidth: PaneTabStripLayout.maximumWidth,
                            alignment: .leading)
                        .id(entry.tabID)
                }
            }
            .padding(.leading, 6)
            .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
                model.updateContentWidth($0)
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: {
            model.updateViewportWidth($0)
        }
        .onChange(of: model.activeTabID) { _, active in
            guard let active else { return }
            withAnimation(.easeInOut(duration: 0.15)) {
                proxy.scrollTo(active)
            }
        }
    }

    HStack(spacing: 10) {
        if model.showsOverflowMenu {
            overflowMenu
        }
        newTabMenuButton
    }
    .padding(.horizontal, 8)
    .frame(height: WorkspaceMetrics.tabStripHeight)
    .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
}
```

Keep the existing named coordinate space on the outer view so drag frames remain comparable between tabs.

Extract the existing `+` menu into this fixed control without changing its contents:

```swift
private var newTabMenuButton: some View {
    Menu {
        newTabMenu()
    } label: {
        Image(systemName: "plus")
            .font(.system(size: 11))
            .foregroundStyle(AppTheme.meta)
    }
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .help("New tab (⌘T)")
    .accessibilityLabel("New tab")
}
```

- [ ] **Step 5: Implement the overflow menu with complete local state**

Add:

```swift
private var overflowMenu: some View {
    Menu {
        ForEach(model.entries, id: \.tabID) { entry in
            let presentation = resolver.resolve(entry)
            Button {
                model.onActivate(entry.tabID)
            } label: {
                if entry.isActive {
                    Label(menuTitle(entry, presentation), systemImage: "checkmark")
                } else {
                    Text(menuTitle(entry, presentation))
                }
            }
        }
    } label: {
        Image(systemName: "chevron.down")
            .font(.system(size: 10, weight: .medium))
            .foregroundStyle(AppTheme.meta)
    }
    .buttonStyle(.plain)
    .menuIndicator(.hidden)
    .help("All tabs")
    .accessibilityLabel("All tabs")
}

private func menuTitle(
    _ entry: TabMenuEntry,
    _ presentation: PaneTabPresentation
) -> String {
    var parts = ["\(presentation.kindLabel): \(entry.title)"]
    if presentation.isDirty { parts.append("modified") }
    if let status = presentation.agentStatus { parts.append(status.humanLabel) }
    return parts.joined(separator: " — ")
}
```

Keep `+` as a separate `Menu`; do not merge it with overflow. Its action continues to call `onActivateGroup` before app-owned menu actions.

- [ ] **Step 6: Run package tests and the app build**

Run:

```bash
cd Packages/TillerWorkspace && swift test
xcodebuild build -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO
```

Expected: all package tests pass and the app builds.

- [ ] **Step 7: Commit overflow behavior**

```bash
git add App/PaneTabStripBar.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripLayoutTests.swift
git commit -m "feat: add pane tab overflow controls"
```

---

### Task 6: Render the valid empty root pane without creating content

**Files:**
- Create: `App/PaneEmptyStateView.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift`
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift`
- Modify: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabChromeTests.swift`
- Modify: `App/ContentView.swift`

- [ ] **Step 1: Write a failing empty-state mount test**

Add to `PaneTabChromeTests`:

```swift
@Test func anEmptyGroupShowsTheInjectedEmptyStateUntilAHostMounts() {
    let groupID = PaneGroupID()
    let empty = PaneGroup(id: groupID, tabs: [], activeTabID: nil)
    let tab = self.tab("terminal")
    let populated = PaneGroup(id: groupID, tabs: [tab], activeTabID: tab.id)
    let provider = MutableHostProvider()
    let controller = PaneGroupController(
        id: groupID,
        emptyStateFactory: { _ in NSView() })
    _ = controller.view

    controller.update(group: empty, isFocused: true, hostProvider: provider)
    #expect(controller.isShowingEmptyState)

    provider.hosts[tab.id] = FakeContentHost(tabID: tab.id)
    controller.update(group: populated, isFocused: true, hostProvider: provider)
    #expect(!controller.isShowingEmptyState)
}

@MainActor
private final class MutableHostProvider: WorkspaceHostProvider {
    var hosts: [WorkspaceTabID: WorkspaceContentHost] = [:]

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? {
        hosts[tabID]
    }
}
```

`FakeContentHost` already belongs to the `TillerWorkspaceTests` module, so use it directly without widening production access.

- [ ] **Step 2: Run the focused suite and verify it fails**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabChromeTests
```

Expected: compilation fails because `PaneEmptyStateFactory`, the initializer parameter, and `isShowingEmptyState` do not exist.

- [ ] **Step 3: Define and thread the empty-state factory**

In `PaneTabStripModel.swift`, add:

```swift
public typealias PaneEmptyStateFactory = @MainActor (PaneTabStripModel) -> NSView
```

Add optional `emptyStateFactory: PaneEmptyStateFactory? = nil` parameters beside `stripFactory` in:

- `WorkspaceView.init`
- `WorkspaceViewController.init`
- `WorkspaceReconciler.init`
- `PaneGroupController.init`

Store and forward the factory at every layer. In `WorkspaceReconciler.build`, construct a group controller with both factories:

```swift
let created = PaneGroupController(
    id: id,
    intentSink: intentSink,
    stripFactory: stripFactory,
    emptyStateFactory: emptyStateFactory)
```

- [ ] **Step 4: Mount the empty state inside the content container**

In `PaneGroupController`, keep a reference and expose read-only test state:

```swift
private let emptyStateFactory: PaneEmptyStateFactory?
private var emptyStateView: NSView?

public var isShowingEmptyState: Bool {
    emptyStateView?.isHidden == false
}
```

After creating `contentContainer` in `loadView`, create and constrain the empty view if a factory exists:

```swift
if let empty = emptyStateFactory?(stripModel) {
    empty.translatesAutoresizingMaskIntoConstraints = false
    empty.isHidden = true
    contentContainer.addSubview(empty)
    NSLayoutConstraint.activate([
        empty.leadingAnchor.constraint(equalTo: contentContainer.leadingAnchor),
        empty.trailingAnchor.constraint(equalTo: contentContainer.trailingAnchor),
        empty.topAnchor.constraint(equalTo: contentContainer.topAnchor),
        empty.bottomAnchor.constraint(equalTo: contentContainer.bottomAnchor)
    ])
    emptyStateView = empty
}
```

In `update`, after detaching the old host and before the `guard let nextHost` return:

```swift
emptyStateView?.isHidden = nextTabID != nil
guard let nextHost, let nextTabID else { return }
emptyStateView?.isHidden = true
```

Replace `detachMountedHost()` with this implementation so it removes only the mounted content host and preserves the persistent empty-state view:

```swift
private func detachMountedHost() {
    if let mountedHost {
        let controller = mountedHost.viewController
        controller.removeFromParent()
        self.mountedHost = nil
    }
    contentContainer.subviews
        .filter { subview in
            guard let emptyStateView else { return true }
            return subview !== emptyStateView
        }
        .forEach { $0.removeFromSuperview() }
    mountedTabID = nil
}
```

The filtered cleanup is required even when the weak `mountedHost` has already deallocated; it preserves the existing regression guarantee that a released host cannot leave a stale view attached.

- [ ] **Step 5: Implement the app-owned empty state**

Create `App/PaneEmptyStateView.swift`:

```swift
import AppKit
import SwiftUI
import TillerWorkspace

struct PaneEmptyStateView<NewTabMenu: View>: View {
    @Bindable var stripModel: PaneTabStripModel
    @ViewBuilder var newTabMenu: () -> NewTabMenu

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: "rectangle.on.rectangle.slash")
                .font(.system(size: 24, weight: .light))
                .foregroundStyle(AppTheme.meta)
            Text("Empty Pane")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(AppTheme.title)
            Text("Open a terminal, agent, or chat in this pane.")
                .font(.system(size: 12))
                .foregroundStyle(AppTheme.subtitle)
            HStack(spacing: 8) {
                Button("New Terminal") {
                    stripModel.onActivateGroup()
                    stripModel.onNewTab()
                }
                Menu("New…") { newTabMenu() }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(AppTheme.terminalSurface)
        .accessibilityElement(children: .contain)
    }
}

@MainActor
func makePaneEmptyState<NewTabMenu: View>(
    _ stripModel: PaneTabStripModel,
    @ViewBuilder newTabMenu: @escaping () -> NewTabMenu
) -> NSView {
    NSHostingView(rootView: PaneEmptyStateView(
        stripModel: stripModel,
        newTabMenu: newTabMenu))
}
```

The injected `NewTabMenuItems` must receive `onBeforeAction: stripModel.onActivateGroup`; this activates the group per menu action without relying on a gesture attached to `Menu`.

- [ ] **Step 6: Inject the empty state from `ContentView`**

Beside `stripFactory`, pass:

```swift
emptyStateFactory: { stripModel in
    makePaneEmptyState(stripModel) {
        NewTabMenuItems(
            model: model,
            worktree: worktree,
            onBeforeAction: stripModel.onActivateGroup,
            onNewTerminal: stripModel.onNewTab)
    }
}
```

The existing engine behavior remains unchanged: only the root group can be valid and empty; secondary groups still collapse in `WorkspaceLayoutEngine`.

- [ ] **Step 7: Run focused tests and the app build**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabChromeTests
cd Packages/TillerWorkspace && swift test
xcodebuild build -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO
```

Expected: package tests pass and the app builds.

- [ ] **Step 8: Commit the empty state**

```bash
git add App/PaneEmptyStateView.swift App/ContentView.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneTabStripModel.swift Packages/TillerWorkspace/Sources/TillerWorkspace/PaneGroupController.swift Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceReconciler.swift Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabChromeTests.swift
git commit -m "feat: add pane tab empty state"
```

---

### Task 7: Verify accessibility, interaction regressions, and the repository gate

**Files:**
- Test: `AppTests/Workspace/PaneTabPresentationTests.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/PaneTabStripDragTests.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/OverflowTests.swift`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragCoordinatorTests.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/Workspace/WorkspaceLayoutEngineStructuralTests.swift`

- [ ] **Step 1: Run the presentation and accessibility tests**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests/PaneTabPresentationTests
```

Expected: all presentation tests pass, including exact selected/modified/status labels and non-color-only glyph mapping.

- [ ] **Step 2: Run the tab, overflow, and drag package suites**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter PaneTabChromeTests
cd Packages/TillerWorkspace && swift test --filter PaneTabStripLayoutTests
cd Packages/TillerWorkspace && swift test --filter PaneTabStripDragTests
cd Packages/TillerWorkspace && swift test --filter OverflowTests
cd Packages/TillerWorkspace && swift test --filter WorkspaceDragCoordinatorTests
```

Expected: all commands pass. This verifies local ownership, fixed entry order, overflow state, active reveal policy, cross-pane drag, edge autoscroll, and Escape cancellation.

- [ ] **Step 3: Re-run the engine's close-selection and pane-collapse coverage**

Run:

```bash
cd Packages/TillerCore && swift test --filter WorkspaceLayoutEngineStructuralTests
```

Expected: tests pass, including left-neighbor selection repair, secondary-pane collapse, and preservation of one empty root group.

- [ ] **Step 4: Perform the visual acceptance pass in the real app**

Run:

```bash
xcodegen generate
open Tiller.xcodeproj
```

Launch with `⌘R`, then verify this deterministic scenario:

1. Open one terminal tab and confirm the 32-point local strip remains visible.
2. Add terminal, chat/agent, Markdown, and code tabs; confirm icon, title, dirty marker, status glyph, and hover-only close area do not shift widths.
3. Split the workspace and activate one tab in each pane; confirm both active shapes remain visible but only the keyboard-focused pane has the focus accent.
4. Narrow one pane until tabs overflow; confirm `⌄` and `+` remain fixed and the active tab scrolls into view.
5. Drag a tab within the strip, between panes, and to an edge; press Escape during one drag and confirm order/ownership restore.
6. Close the final tab in a secondary pane and confirm the split collapses. Close the root's final tab and confirm the empty state appears without creating content.
7. Enable Reduce Motion and confirm running activity remains legible without animation.
8. Use VoiceOver on a truncated, dirty, needs-input tab and confirm type, full title, selection, dirty state, status, activation, and close are announced.

Capture screenshots of wide, narrow-overflow, two-pane focus, and empty-root states as execution evidence; do not add them to Git unless the user requests fixture assets.

- [ ] **Step 5: Run the authoritative repository gate**

Run:

```bash
Scripts/ci.sh
```

Expected: the final line is exactly `CI OK`.

- [ ] **Step 6: Inspect the final diff and commit any verification-only fixes**

Run:

```bash
git status --short
git diff --check
git log --oneline -7
```

Expected: no uncommitted implementation changes, no whitespace errors, and one conventional commit per completed task. If the acceptance pass required a code or test fix, repeat that task's focused red/green command and commit the fix with a scoped `fix:` or `test:` message before rerunning `Scripts/ci.sh`.
