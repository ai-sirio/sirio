# Native Titlebar Interaction and Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Tiller’s four custom chrome controls from the full-width SwiftUI canvas strip into native AppKit titlebar accessory regions so the controls receive hover, pressed, click, keyboard, and accessibility events while the unused titlebar remains native for dragging and the macOS double-click preference.

**Architecture:** Keep `WindowChromeConfigurator` as the only window-chrome boundary, but make it host two narrowly sized `NSTitlebarAccessoryViewController` instances: one leading accessory for the sidebar button and one trailing accessory for the right-panel, split, and permissions buttons. Keep `ContentView` responsible for the existing state, callbacks, labels, help text, and legacy/new split choice; pass those SwiftUI button groups into the configurator instead of rendering a full-width `TitleStrip` in the canvas. Put shared frame, spacing, height, and icon-size rules in `AppTheme` and `TitlebarGeometry` so both accessories use one alignment contract.

**Tech Stack:** Swift 6; SwiftUI; AppKit (`NSWindow`, `NSTitlebarAccessoryViewController`, `NSHostingView`); macOS 15+; `swift-testing` (`@Test`, `#expect`); Xcode project generated from `project.yml`; repository gate `Scripts/ci.sh`.

## Global Constraints

- Preserve the existing `.windowStyle(.hiddenTitleBar)` and `.fullSizeContentView` canvas treatment.
- Host only the control-bearing leading and trailing regions; do not cover the empty titlebar with a transparent full-width SwiftUI surface.
- Preserve the existing four-control order: sidebar leading; right panel, split, and permissions trailing.
- Preserve the existing callbacks, state, actions, shortcuts, help text, accessibility labels, and split-specific behavior.
- Keep `AppTheme.titleStripHeight` at 28pt and `AppTheme.titleStripIconSize` at 13pt unless manual optical verification demonstrates that one shared value must change.
- Use one common control frame, one common vertical centering rule, and one shared spacing value; do not add an icon-specific `offset`, including for the sidebar icon.
- Leave the empty titlebar area to AppKit so window dragging and the user’s macOS titlebar double-click preference remain native; do not add a custom double-click zoom handler.
- Keep the traffic lights native and outside both accessory frames.
- Verify both `WorkspaceRenderPath.legacyTerminal` and `WorkspaceRenderPath.workspace` paths without changing their semantics.
- Do not modify `App/Workspace/WorkspaceCoordinator.swift`, `AppTests/Workspace/WorkspaceCoordinatorTests.swift`, or any other pre-existing working-tree change.
- Do not regenerate or hand-edit `Tiller.xcodeproj`; no target membership change is needed because `App` and `AppTests` are already source roots in `project.yml`.
- Use `swift-testing` tests, not XCTest.

## File map

- Modify `App/AppTheme.swift`: add the shared titlebar control-frame and spacing tokens consumed by both accessory groups; leave unrelated theme tokens untouched.
- Create `App/TitlebarGeometry.swift`: expose the small, testable geometry contract for control frame, accessory height, icon size, group spacing, and traffic-light relationship; contain no AppKit event routing.
- Modify `App/TitleStrip.swift`: turn the current full-width canvas row into reusable leading/trailing control-group content that applies the shared frame and vertical-centering contract without reserving the old full-width traffic-light inset.
- Modify `App/WindowChromeConfigurator.swift`: retain window discovery and chrome setup, and add idempotent installation/update/removal of the two native titlebar accessories backed by `NSHostingView`.
- Modify `App/ContentView.swift`: remove the canvas-owned `TitleStrip` row and pass the existing button builders to `configuresWindowChrome`; preserve all existing action closures and the `workspaceEngineEnabled` split selection.
- Modify `AppTests/CardGeometryTests.swift`: add deterministic assertions for the shared titlebar geometry and absence of per-control correction values.
- Modify `AppTests/Workspace/WorkspaceMountTests.swift`: assert both render paths use the same titlebar hosting/geometry contract.
- Create `AppTests/WindowChromeConfiguratorTests.swift`: exercise the AppKit seam for accessory count, placement, bounded frames, idempotent updates, and native empty-region ownership where the test host can inspect it deterministically.
- Do not modify `project.yml`: the new files are automatically included by the existing `App` and `AppTests` source globs.

### Task 1: Establish the shared titlebar geometry contract

**Files:**
- Create: `App/TitlebarGeometry.swift`
- Modify: `App/AppTheme.swift:75-85`
- Modify: `AppTests/CardGeometryTests.swift:11-23`

**Interfaces:**
- Produces `enum TitlebarGeometry` with `static let accessoryHeight: CGFloat`, `static let controlFrame: CGSize`, `static let controlSpacing: CGFloat`, `static let iconSize: CGFloat`, and `static let trafficLightInset: CGFloat`.
- Produces `static func verticalCenter(in accessoryHeight: CGFloat) -> CGFloat` returning `accessoryHeight / 2`.
- Produces `static func controlFrames(count: Int, in bounds: CGRect) -> [CGRect]`, returning one bounded frame per control with the shared spacing and a common vertical centerline.
- Keeps `AppTheme.titleStripHeight`, `AppTheme.titleStripIconSize`, and `AppTheme.trafficLightInset` as the existing source values; `TitlebarGeometry` references them rather than duplicating them.

- [ ] **Step 1: Write the failing tests**

Add focused `swift-testing` tests to `AppTests/CardGeometryTests.swift`:

```swift
import SwiftUI

@Test func titlebarControlsShareOneFrameAndCenterline() {
    #expect(TitlebarGeometry.accessoryHeight == AppTheme.titleStripHeight)
    #expect(TitlebarGeometry.iconSize == AppTheme.titleStripIconSize)
    #expect(TitlebarGeometry.controlFrame == CGSize(width: 24, height: 24))
    #expect(TitlebarGeometry.verticalCenter(in: TitlebarGeometry.accessoryHeight) == 14)
}

@Test func titlebarUsesOneSharedSpacingAndNoPerIconCorrection() {
    #expect(TitlebarGeometry.controlSpacing == 2)
    #expect(TitlebarGeometry.sidebarVerticalCorrection == 0)
    #expect(TitlebarGeometry.trafficLightInset == AppTheme.trafficLightInset)
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/CardGeometryTests
```

Expected: FAIL because `TitlebarGeometry` and its members do not yet exist.

- [ ] **Step 3: Add the minimal geometry implementation**

Add the following contract in `App/TitlebarGeometry.swift`, with the frame and spacing values owned by `AppTheme` and no per-icon correction:

```swift
import CoreGraphics

enum TitlebarGeometry {
    static let accessoryHeight = AppTheme.titleStripHeight
    static let controlFrame = AppTheme.titlebarControlFrame
    static let controlSpacing = AppTheme.titlebarControlSpacing
    static let iconSize = AppTheme.titleStripIconSize
    static let trafficLightInset = AppTheme.trafficLightInset
    static let sidebarVerticalCorrection: CGFloat = 0

    static func verticalCenter(in accessoryHeight: CGFloat) -> CGFloat {
        accessoryHeight / 2
    }
}
```

Do not change the existing 28pt, 78pt, or 13pt `AppTheme` values. Add only these shared titlebar tokens beside them:

```swift
static let titlebarControlFrame = CGSize(width: 24, height: 24)
static let titlebarControlSpacing: CGFloat = 2
```

The new `TitlebarGeometry` contract must reference these tokens rather than duplicate their values.

- [ ] **Step 4: Run the focused test and verify it passes**

Run the same `xcodebuild test` command. Expected: PASS for all `CardGeometryTests`.

- [ ] **Step 5: Commit the implementation**

```bash
git add App/TitlebarGeometry.swift App/AppTheme.swift AppTests/CardGeometryTests.swift
git commit -m "feat: define shared titlebar geometry"
```

### Task 2: Make `TitleStrip` a bounded accessory control group

**Files:**
- Modify: `App/TitleStrip.swift:3-20`
- Modify: `AppTests/CardGeometryTests.swift`

**Interfaces:**
- Replace the full-width `TitleStrip<Leading: View, Trailing: View>` row with `struct TitleStripGroup<Content: View>: View`, initialized as `init(@ViewBuilder content: () -> Content)`; the group applies the shared control frame to its content and is vertically centered in `TitlebarGeometry.accessoryHeight`.
- Keep the existing `HoverIconButtonStyle` on every real SwiftUI `Button`; do not replace buttons with gestures or labels that cannot receive keyboard activation.

- [ ] **Step 1: Write the failing geometry/group tests**

Extend `AppTests/CardGeometryTests.swift` with a concrete four-control layout test. It must calculate the frames the group will apply, then assert each frame is bounded, the four controls fit inside the accessory bounds, and every control is vertically centered:

```swift
import CoreGraphics
import SwiftUI

@Test func fourTitlebarControlsHaveBoundedCenteredFrames() {
    let bounds = CGRect(x: 0, y: 0, width: 4 * 24 + 3 * 2, height: 28)
    let frames = TitlebarGeometry.controlFrames(count: 4, in: bounds)

    #expect(frames.count == 4)
    #expect(frames.allSatisfy { $0.size == CGSize(width: 24, height: 24) })
    #expect(frames.allSatisfy { bounds.contains($0) })
    #expect(frames.allSatisfy { $0.midY == bounds.midY })
    #expect(frames.dropFirst().enumerated().allSatisfy { index, frame in
        frame.minX == frames[index].maxX + 2
    })
}
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/CardGeometryTests
```

Expected: FAIL because `TitlebarGeometry.controlFrames(count:in:)` is not yet defined.

- [ ] **Step 3: Implement the minimal bounded group API**

Replace `TitleStrip` so it no longer contains `Color.clear.frame(width: AppTheme.trafficLightInset)`, `Spacer(minLength: 0)`, full-width trailing padding, or a full-width hit-testing surface. Define the bounded group used by each native accessory:

```swift
struct TitleStripGroup<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: TitlebarGeometry.controlSpacing) {
            content
                .frame(width: TitlebarGeometry.controlFrame.width,
                       height: TitlebarGeometry.controlFrame.height)
        }
        .frame(height: TitlebarGeometry.accessoryHeight, alignment: .center)
    }
}
```

Add the geometry helper used by the group layout and by the test:

```swift
static func controlFrames(count: Int, in bounds: CGRect) -> [CGRect] {
    let y = bounds.midY - controlFrame.height / 2
    return (0..<count).map { index in
        CGRect(
            x: bounds.minX + CGFloat(index) * (controlFrame.width + controlSpacing),
            y: y,
            width: controlFrame.width,
            height: controlFrame.height)
    }
}
```

Render the leading group with one bounded slot and the trailing group with three bounded slots using the same frame and spacing contract represented by `controlFrames(count:in:)`; center every slot vertically in the accessory height. Do not add a sidebar-only offset.

- [ ] **Step 4: Preserve the existing button builders while removing canvas ownership**

Keep `titleStripLeadingButtons` and `titleStripButtons` in `ContentView` unchanged in action, labels, help strings, accessibility labels, and `workspaceEngineEnabled` branch. The next task will host the two groups; this task must only make their layout reusable.

- [ ] **Step 5: Run the focused tests and verify they pass**

Run the same `xcodebuild test` command. Expected: PASS for the original card geometry tests and the new bounded-group tests.

- [ ] **Step 6: Commit the implementation**

```bash
git add App/TitleStrip.swift App/ContentView.swift AppTests/CardGeometryTests.swift
git commit -m "refactor: bound titlebar control groups"
```

### Task 3: Host the groups in native AppKit titlebar accessories

**Files:**
- Modify: `App/WindowChromeConfigurator.swift:4-28`
- Create: `AppTests/WindowChromeConfiguratorTests.swift`

**Interfaces:**
- Replace the zero-argument private representable with `private struct WindowChromeConfigurator<LeadingAccessory: View, TrailingAccessory: View>: NSViewRepresentable`.
- Its initializer is `init(@ViewBuilder leadingAccessory: () -> LeadingAccessory, @ViewBuilder trailingAccessory: () -> TrailingAccessory)`.
- Keep `func makeNSView(context: Context) -> NSView` and `func updateNSView(_ nsView: NSView, context: Context)`; both update a single internal host object rather than creating duplicate window coordinators.
- Add internal `@MainActor final class TitlebarAccessoryHost` as the deterministic AppKit seam. It owns exactly two `NSTitlebarAccessoryViewController` instances, one `.left` and one `.right`, each with an `NSHostingView<AnyView>` root and bounded `view.frame`.
- Expose the actual owned `NSHostingView<AnyView>` instances as `private(set)` `leadingHostingView` and `trailingHostingView` references so tests can inspect hosted frames and lifecycle evidence without depending on SwiftUI's private view tree. Do not add synthetic mode metadata to the host.
- `TitlebarAccessoryHost.install(on window: NSWindow)`, `update(leading: AnyView, trailing: AnyView)`, and `remove(from window: NSWindow)` must be idempotent. The host must not install a double-click recognizer or a full-width event-catching view.

- [ ] **Step 1: Write the failing AppKit seam tests**

Create `AppTests/WindowChromeConfiguratorTests.swift`:

```swift
import AppKit
import SwiftUI
import Testing
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WindowChromeConfiguratorTests {
    @Test func hostInstallsOneLeadingAndOneTrailingBoundedAccessory() throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                              styleMask: [.titled, .closable, .resizable],
                              backing: .buffered,
                              defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: window)
        host.update(
            leading: AnyView(TitleStripGroup { Button("Sidebar") {} }),
            trailing: AnyView(TitleStripGroup {
                HStack(spacing: TitlebarGeometry.controlSpacing) {
                    Button("Right panel") {}
                    Button("Split terminal") {}
                    Button("Permissions") {}
                }
            }))
        window.layoutIfNeeded()

        #expect(window.titlebarAccessoryViewControllers.count == 2)
        #expect(window.titlebarAccessoryViewControllers.map(\.layoutAttribute) == [.left, .right])

        let leading = try #require(host.leadingController)
        let trailing = try #require(host.trailingController)
        let contentBounds = try #require(window.contentView).bounds
        leading.view.layoutSubtreeIfNeeded()
        trailing.view.layoutSubtreeIfNeeded()

        #expect(leading.view.frame.size == CGSize(width: 24, height: 28))
        #expect(trailing.view.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(host.leadingHostingView?.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingHostingView?.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(leading.view.frame.maxX < contentBounds.maxX)
        #expect(trailing.view.frame.minX > contentBounds.minX)
    }

    @Test func installingAndUpdatingDoesNotDuplicateAccessories() {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                              styleMask: [.titled, .closable, .resizable],
                              backing: .buffered,
                              defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: window)
        host.install(on: window)
        host.update(
            leading: AnyView(TitleStripGroup { Button("Sidebar") {} }),
            trailing: AnyView(TitleStripGroup {
                HStack(spacing: TitlebarGeometry.controlSpacing) {
                    Button("Right panel") {}
                    Button("Split terminal") {}
                    Button("Permissions") {}
                }
            }))
        window.layoutIfNeeded()

        #expect(window.titlebarAccessoryViewControllers.count == 2)
        #expect(host.leadingController?.view.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingController?.view.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(host.leadingHostingView?.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingHostingView?.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
    }
}
```

- [ ] **Step 2: Run the new tests and verify they fail**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/WindowChromeConfiguratorTests
```

Expected: FAIL because `TitlebarAccessoryHost` does not yet exist and no native accessories are installed.

- [ ] **Step 3: Implement the minimal host and representable**

In `WindowChromeConfigurator.swift`, keep the existing `fullSizeContentView`, transparent titlebar, non-opaque window, clear background, and `installHideOnClose()` configuration. Add the host with this behavioral shape:

```swift
@MainActor
final class TitlebarAccessoryHost {
    private(set) var leadingController: NSTitlebarAccessoryViewController?
    private(set) var trailingController: NSTitlebarAccessoryViewController?
    private(set) var leadingHostingView: NSHostingView<AnyView>?
    private(set) var trailingHostingView: NSHostingView<AnyView>?
    func install(on window: NSWindow) {
        guard leadingController == nil, trailingController == nil else { return }
        let leading = NSTitlebarAccessoryViewController()
        leading.layoutAttribute = .left
        let trailing = NSTitlebarAccessoryViewController()
        trailing.layoutAttribute = .right
        window.addTitlebarAccessoryViewController(leading)
        window.addTitlebarAccessoryViewController(trailing)
        leadingController = leading
        trailingController = trailing
    }

    func update(leading: AnyView, trailing: AnyView) {
        leadingHostingView = NSHostingView(rootView: leading)
        trailingHostingView = NSHostingView(rootView: trailing)
        leadingController?.view = leadingHostingView!
        trailingController?.view = trailingHostingView!
        leadingHostingView?.frame = CGRect(
            x: 0, y: 0,
            width: TitlebarGeometry.controlFrame.width,
            height: TitlebarGeometry.accessoryHeight)
        trailingHostingView?.frame = CGRect(
            x: 0, y: 0,
            width: 3 * TitlebarGeometry.controlFrame.width + 2 * TitlebarGeometry.controlSpacing,
            height: TitlebarGeometry.accessoryHeight)
    }

    func remove(from window: NSWindow) {
        for controller in [leadingController, trailingController].compactMap({ $0 })
        where window.titlebarAccessoryViewControllers.contains(where: { $0 === controller }) {
            window.removeTitlebarAccessoryViewController(at: window.titlebarAccessoryViewControllers.firstIndex(of: controller)!)
        }
        leadingController = nil
        trailingController = nil
        leadingHostingView = nil
        trailingHostingView = nil
    }
}
```

Use `NSTitlebarAccessoryViewController.layoutAttribute = .left` for the sidebar accessory and `.right` for the trailing group. Set each hosted view’s frame/intrinsic sizing from `TitlebarGeometry`, leave the remaining titlebar width unowned, and set no opaque accessory background. Use `NSHostingView(rootView:)` with `AnyView` only at this AppKit boundary.

Make the representable create one host-backed `NSView`, install when `view.window` becomes available, call `update` on subsequent SwiftUI updates, and remove owned controllers when the host leaves the window. Do not use `NSEvent`, `NSClickGestureRecognizer`, `mouseDown`, `performDrag`, or a double-click action.

- [ ] **Step 4: Run the AppKit seam tests and verify they pass**

Run the same `xcodebuild test` command. Expected: PASS, with exactly two native accessory controllers and no duplicate after repeated installation.

- [ ] **Step 5: Commit the implementation**

```bash
git add App/WindowChromeConfigurator.swift AppTests/WindowChromeConfiguratorTests.swift
git commit -m "feat: host controls in native titlebar accessories"
```

### Task 4: Wire `ContentView` without changing action or split semantics

**Files:**
- Modify: `App/ContentView.swift:58-95, 175-199`
- Modify: `AppTests/Workspace/WorkspaceMountTests.swift`

**Interfaces:**
- Let `ContentView` accept an injectable `TitlebarAccessoryHost`, and make the actual `ContentView.body` apply `.configuresWindowChrome(configuration: { titlebarAccessories }, host: titlebarAccessoryHost)`.
- Keep `TitlebarAccessoryHost` centered on its actual `leadingHostingView` and `trailingHostingView` references plus rendered-control lifecycle evidence; do not add `TitlebarSplitControl`, `lastAppliedConfiguration`, or any other synthetic mode metadata.
- Add a lifecycle probe to each real per-control slot: sidebar, right panel, split, and permissions. The probe is embedded in the actual legacy and workspace split control branches, not in test-only metadata.
- Keep `ContentView.renderPath(gateEnabled:)`, `workspaceEngineEnabled`, `universalSplitMenu`, the legacy `Button` action `model.workspaceSplitCurrent(.horizontal)`, and the existing `splitContent` branches unchanged.
- Remove only the `TitleStrip(...)` canvas row and the now-invalid top-safe-area explanation that says the strip is inside the reserved band; preserve the canvas, split, usage bar, and unrelated layout.

- [ ] **Step 1: Write the failing wiring/parity tests**

Replace the marker-only assertions in `AppTests/Workspace/WorkspaceMountTests.swift` with an integration test that renders the actual `ContentView` twice. Do not call a configuration factory directly: the test must wait for SwiftUI to evaluate `ContentView.body`, create the real `WindowChromeConfigurator`, and install it into the injected `TitlebarAccessoryHost` attached to an `NSWindow`.

The supported macOS 15/Xcode 16 combination does not expose a stable public API for enumerating descendants or AX identifiers below these `NSHostingView<AnyView>` roots. Recursive AX inspection timed out and is not deterministic, so the test must not inspect the accessibility hierarchy. Add `import Foundation` and `import SwiftUI` for the `NSHostingView`, `NSWindow`, and bounded main-run-loop drain.

Use these exact probe and host seams in `App/WindowChromeConfigurator.swift`:

```swift
enum TitlebarControlSlot: Hashable {
    case sidebar
    case rightPanel
    case split
    case permissions
}

struct TitlebarControlProbeRecord: Equatable {
    let slot: TitlebarControlSlot
    let accessibilityIdentifier: String?
}

struct TitlebarControlProbe: NSViewRepresentable {
    let slot: TitlebarControlSlot
    let accessibilityIdentifier: String?
    let host: TitlebarAccessoryHost

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        host.recordRenderedControl(slot: slot, accessibilityIdentifier: accessibilityIdentifier)
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        host.recordRenderedControl(slot: slot, accessibilityIdentifier: accessibilityIdentifier)
    }
}
```

`TitlebarAccessoryHost` records `renderedControlProbes: [TitlebarControlProbeRecord]` and `updateCount: Int`, and exposes these exact operations alongside the Task 3 APIs:

```swift
private(set) var renderedControlProbes: [TitlebarControlProbeRecord] = []
private(set) var updateCount = 0

func recordRenderedControl(slot: TitlebarControlSlot, accessibilityIdentifier: String?)
func update(leading: AnyView, trailing: AnyView)
```

`recordRenderedControl` is called only by the probe’s `makeNSView`/`updateNSView`, and `update` increments `updateCount`. The probe is placed inside the actual four control slots, including the real legacy split `Button` branch and the real workspace split control branch (and its fallback `Button`, when present). It must not be rendered by a fixture or injected as a mode marker.

Add this exact integration-test shape to `AppTests/Workspace/WorkspaceMountTests.swift`:

```swift
@Test @MainActor func renderedContentViewReportsOnlyTheMountedSplitBranch() throws {
    let legacyHost = TitlebarAccessoryHost()
    let legacyWindow = renderContentView(
        workspaceEngineEnabled: false,
        titlebarAccessoryHost: legacyHost)
    #expect(legacyHost.updateCount > 0)
    #expect(legacyHost.renderedControlProbes.contains {
        $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.legacy"
    })
    #expect(!legacyHost.renderedControlProbes.contains {
        $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.workspace"
    })

    let workspaceHost = TitlebarAccessoryHost()
    let workspaceWindow = renderContentView(
        workspaceEngineEnabled: true,
        titlebarAccessoryHost: workspaceHost)
    #expect(workspaceHost.updateCount > 0)
    #expect(workspaceHost.renderedControlProbes.contains {
        $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.workspace"
    })
    #expect(!workspaceHost.renderedControlProbes.contains {
        $0.slot == .split && $0.accessibilityIdentifier == "tiller.titlebar.split.legacy"
    })

    let expectedSlots: Set<TitlebarControlSlot> = [.sidebar, .rightPanel, .split, .permissions]
    #expect(Set(legacyHost.renderedControlProbes.map(\.slot)).isSuperset(of: expectedSlots))
    #expect(Set(workspaceHost.renderedControlProbes.map(\.slot)).isSuperset(of: expectedSlots))
    legacyWindow.orderOut(nil)
    workspaceWindow.orderOut(nil)
}

@MainActor
private func renderContentView(
    workspaceEngineEnabled: Bool,
    titlebarAccessoryHost: TitlebarAccessoryHost
) -> NSWindow {
    let model = AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100)
    let updater = UpdaterModel()
    let contentView = ContentView(
        model: model,
        updater: updater,
        workspaceEngineEnabled: workspaceEngineEnabled,
        titlebarAccessoryHost: titlebarAccessoryHost)
    let hostingView = NSHostingView(rootView: contentView)
    let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                          styleMask: [.titled, .closable, .resizable],
                          backing: .buffered,
                          defer: false)
    window.contentView = hostingView
    window.makeKeyAndOrderFront(nil)
    window.layoutIfNeeded()
    for _ in 0..<20 where titlebarAccessoryHost.updateCount == 0 {
        RunLoop.main.run(until: Date().addingTimeInterval(0.01))
    }
    window.layoutIfNeeded()
    return window
}
```

The exact collection assertions use `Set` conversion because `TitlebarControlSlot` is `Hashable`; they must verify all four per-control slots were reported and that the split slot reports exactly the active branch. Do not use synthetic mode metadata, direct factory-only tests, source markers, accessibility-tree recursion, or any unstable AX query.

- [ ] **Step 2: Run the focused tests and verify they fail**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/WorkspaceMountTests -only-testing:TillerTests/CardGeometryTests
```

Expected: FAIL because `ContentView` does not yet accept the injected host, its body does not yet pass `configuration: { titlebarAccessories }` to `.configuresWindowChrome`, and the real branches do not yet mount the required lifecycle probes. The test must fail if `.configuresWindowChrome` is removed from `ContentView.body`, if the host is not updated/injected, or if both modes render the same split branch.

- [ ] **Step 3: Move hosting responsibility without changing callbacks**

Route the actual body through the injected host. `TitlebarAccessoryHost` must expose the actual `leadingHostingView` and `trailingHostingView` references plus the rendered-control lifecycle evidence; production code still owns installation and updates through the same host instance.

Add the host injection to `ContentView.init`, add the computed titlebar accessory content using the existing builders, and make the actual body pass that content to `configuresWindowChrome(configuration:host:)`. The modifier must invoke the real host's `install(on:)` and `update(leading:trailing:)` from the `NSViewRepresentable` lifecycle:

```swift
private let titlebarAccessoryHost: TitlebarAccessoryHost

private var titlebarAccessories: (leading: AnyView, trailing: AnyView) {
    (
        leading: AnyView(TitleStripGroup { titleStripLeadingButtons }),
        trailing: AnyView(TitleStripGroup { titleStripButtons })
    )
}

.configuresWindowChrome(
    configuration: { titlebarAccessories },
    host: titlebarAccessoryHost)
```

Use these exact modifier seams in `App/WindowChromeConfigurator.swift`:

```swift
private struct WindowChromeConfigurator: NSViewRepresentable {
    let configuration: @MainActor () -> (leading: AnyView, trailing: AnyView)
    let host: TitlebarAccessoryHost

    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.styleMask.insert(.fullSizeContentView)
            window.titlebarAppearsTransparent = true
            window.isOpaque = false
            window.backgroundColor = .clear
            window.installHideOnClose()
            host.install(on: window)
            let content = configuration()
            host.update(leading: content.leading, trailing: content.trailing)
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let window = nsView.window else { return }
        host.install(on: window)
        let content = configuration()
        host.update(leading: content.leading, trailing: content.trailing)
    }
}

extension View {
    func configuresWindowChrome(
        configuration: @escaping @MainActor () -> (leading: AnyView, trailing: AnyView),
        host: TitlebarAccessoryHost
    ) -> some View {
        background(WindowChromeConfigurator(configuration: configuration, host: host))
    }
}
```

Delete only the `TitleStrip` construction from `workspaceView` and remove `.ignoresSafeArea(.container, edges: .top)` from the canvas stack if it exists solely to place the old full-width row in the titlebar band. Retain any safe-area behavior needed by the canvas itself, and do not touch `splitContent`, `UsageBarView`, sidebar/right-panel state, or split actions.

Apply identifiers to the actual controls, not to a wrapper or a test-only marker; the probe records the same identifier while that real control branch is hosted:

```swift
Button("Split terminal") {
    model.workspaceSplitCurrent(.horizontal)
}
.accessibilityIdentifier("tiller.titlebar.split.legacy")

universalSplitMenu
    .accessibilityIdentifier("tiller.titlebar.split.workspace")
```

If the workspace menu has a fallback `Button` for environments where the menu cannot be presented, apply `tiller.titlebar.split.workspace` to that fallback control as well and embed its workspace split probe in that actual branch. The test must rely only on the probe records and `updateCount` emitted by the real hosted branch; it must never inspect synthetic mode metadata, source markers, or the accessibility hierarchy.

- [ ] **Step 4: Verify both split implementations remain represented**

Confirm the trailing accessory still selects `universalSplitMenu` when `workspaceEngineEnabled == true` and the legacy `Button` with accessibility label `Split terminal` otherwise. Confirm the actual lifecycle probes report exactly one of `tiller.titlebar.split.legacy` or `tiller.titlebar.split.workspace` for the split slot, all four per-control slots are reported, and `updateCount > 0`; no path-specific AppKit host or synthetic mode marker is introduced.

- [ ] **Step 5: Run the focused tests and verify they pass**

Run the same focused `xcodebuild test` command. Expected: PASS for `WorkspaceMountTests` and `CardGeometryTests`.

- [ ] **Step 6: Commit the implementation**

```bash
git add App/ContentView.swift AppTests/Workspace/WorkspaceMountTests.swift
git commit -m "fix: route titlebar controls through native hosting"
```

### Task 5: Verify accessibility, native event ownership, and the complete gate

**Files:**
- Inspect only: `App/ContentView.swift`, `App/AppTheme.swift`, `App/TitleStrip.swift`, `App/WindowChromeConfigurator.swift`, `AppTests/CardGeometryTests.swift`, `AppTests/Workspace/WorkspaceMountTests.swift`, `AppTests/WindowChromeConfiguratorTests.swift`
- No source changes are expected unless a focused verification exposes a contract mismatch; if a correction is necessary, edit only the titlebar files named above and add it to the preceding task’s commit rather than touching unrelated work.

**Interfaces:**
- Manual verification consumes the native accessory installation from `TitlebarAccessoryHost` and the existing `ContentView` controls.
- Final verification must leave the pre-existing modifications in `App/Workspace/WorkspaceCoordinator.swift` and `AppTests/Workspace/WorkspaceCoordinatorTests.swift` untouched.

- [ ] **Step 1: Run the focused test suite before manual verification**

Run:

```bash
xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' -only-testing:TillerTests/CardGeometryTests -only-testing:TillerTests/WorkspaceMountTests -only-testing:TillerTests/WindowChromeConfiguratorTests
```

Expected: PASS, including geometry, both render-path, bounded-accessory, and idempotent-host tests.

- [ ] **Step 2: Launch and verify the legacy terminal split**

Run the Tiller app with the universal workspace engine disabled through the existing gate mechanism. In the main window, verify all of the following:

1. Hovering each of Sidebar, Right panel, Split terminal, and Permissions shows `HoverIconButtonStyle` feedback.
2. Pressing and releasing each control shows the pressed scale and invokes the existing action: sidebar visibility toggles; right-panel visibility toggles; the terminal split action runs; Permissions opens settings with `settingsCategory == .permissions`.
3. VoiceOver can focus and activate all four real SwiftUI buttons in logical leading-to-trailing order, and the labels remain exactly Sidebar, Right panel, Split terminal, and Permissions. Help text remains state-dependent for Sidebar and Right panel.
4. The empty titlebar can drag from multiple points between the native traffic lights and the trailing accessory, with no accessory-sized dead band beyond the control groups.
5. Double-clicking the empty titlebar once with the macOS preference set not to zoom and once with it set to zoom follows the preference in both cases; Tiller does not force a zoom behavior.
6. The traffic lights remain native, visible, clickable, and not covered by either accessory.
7. The four icon centers share the traffic-light centerline; the sidebar icon is no longer high, and no icon relies on a unique offset.
8. Repeat the visual and interaction check in both light and dark appearances.

- [ ] **Step 3: Launch and verify the new workspace split**

Enable the existing universal workspace engine gate and repeat every check from Step 2. Confirm the Split Right With… control remains the existing `SplitContentMenu`, keeps its help text and accessibility label, and uses the same native accessory, frame, centerline, drag region, double-click behavior, and VoiceOver path as the legacy split button.

- [ ] **Step 4: Inspect the final diff for scope and native-event regressions**

Run:

```bash
git diff --check
git diff -- App/AppTheme.swift App/TitlebarGeometry.swift App/TitleStrip.swift App/WindowChromeConfigurator.swift App/ContentView.swift AppTests/CardGeometryTests.swift AppTests/Workspace/WorkspaceMountTests.swift AppTests/WindowChromeConfiguratorTests.swift
```

Expected: only titlebar geometry, bounded SwiftUI group layout, native accessory hosting, focused tests, and their comments are changed. Confirm no `NSEvent`, gesture recognizer, full-width transparent hit-test layer, per-icon `offset`, callback signature change, or modification to the existing unrelated working-tree files appears.

- [ ] **Step 5: Run the repository CI gate**

Run:

```bash
Scripts/ci.sh
```

Expected: the command exits successfully and prints exactly `CI OK` before completion.

- [ ] **Step 6: Commit only implementation corrections, never the plan**

If the manual or CI checks require a focused correction, commit only the already scoped source/test files with a Conventional Commit message such as:

```bash
git add App/AppTheme.swift App/TitlebarGeometry.swift App/TitleStrip.swift App/WindowChromeConfigurator.swift App/ContentView.swift AppTests/CardGeometryTests.swift AppTests/Workspace/WorkspaceMountTests.swift AppTests/WindowChromeConfiguratorTests.swift
git commit -m "fix: preserve native titlebar interaction"
```

Do not stage or commit `docs/superpowers/plans/2026-08-06-titlebar-interaction-alignment.md` as part of implementation unless the user explicitly requests it.

## Self-review

- **Spec coverage:** Tasks 1–2 cover the shared frame, 28pt/13pt baseline, concrete four-control bounds, common centerline, spacing, traffic-light relationship, and prohibition on per-icon offsets. Task 3 covers native AppKit accessories, direct controller and hosting-container frame inspection, bounded ownership, idempotent lifecycle, native dragging, native double-click behavior, and native traffic lights. Task 4 renders the actual `ContentView` into an `NSWindow`, observes the actual trailing hosting view installed by its real `WindowChromeConfigurator` through an injected `TitlebarAccessoryHost`, and proves through lifecycle probes and host updates that legacy and workspace modes mount mutually exclusive split branches while sharing the same host path. It explicitly avoids unstable AX recursion and synthetic mode metadata. Task 5 keeps hover, pressed state, click actions, accessibility, VoiceOver, light/dark appearance, drag, preference-respecting double-click, and `Scripts/ci.sh` printing `CI OK` as manual or final-gate verification.
- **Placeholder scan:** No `TODO`, `TBD`, `FIXME`, “implement later”, “add appropriate handling”, or “write tests for the above” placeholders remain. Every test step contains concrete Swift code, an exact command, and an expected failure or pass result.
- **Type consistency:** `TitlebarGeometry` is defined before use; `controlFrames(count:in:)` returns `CGRect` values consumed by the bounded group contract; `TitleStripGroup<Content: View>` consumes its `CGSize` and `CGFloat` values; `TitlebarAccessoryHost` consumes `AnyView`, owns `NSTitlebarAccessoryViewController` and `NSHostingView<AnyView>` instances, exposes the actual leading/trailing hosting views, records `TitlebarControlProbeRecord` values, and counts real host updates; `TitlebarControlProbe` is an `NSViewRepresentable` that reports only from `makeNSView` and `updateNSView`; `ContentView` supplies the existing leading/trailing builders and keeps their callbacks intact while its rendered body reaches the injected host through `configuresWindowChrome(configuration:host:)`.
- **Scope protection:** The plan explicitly leaves the two current modified files untouched, does not alter `project.yml`, does not add a custom event-forwarding layer, and does not change unrelated model, split, card, or command behavior.
