import AppKit
import Testing
import TillerCore

@testable import TillerWorkspace

@Suite("DividerHitTestDiagnostic")
@MainActor
struct DividerHitTestDiagnosticTests {
    /// AppKit dispatches `cursorUpdate` to the view that answers `hitTest`, so
    /// the divider band only shows a resize cursor if the split view itself is
    /// what answers there.
    @Test func splitViewAnswersHitTestOnTheDividerBand() throws {
        let controller = WorkspaceSplitController(
            id: SplitID(), axis: .horizontal, preferredFraction: 0.5)
        controller.update(
            axis: .horizontal, preferredFraction: 0.5,
            first: NSViewController(), second: NSViewController())

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 800, height: 600),
            styleMask: [.titled], backing: .buffered, defer: true)
        window.contentView = controller.view
        controller.view.frame = NSRect(x: 0, y: 0, width: 800, height: 600)
        controller.view.layoutSubtreeIfNeeded()
        controller.viewDidLayout()
        controller.view.layoutSubtreeIfNeeded()

        let splitView = controller.splitView
        let bands = DividerCursorRects.rects(
            subviewFrames: splitView.subviews.map(\.frame), bounds: splitView.bounds,
            isVertical: splitView.isVertical)

        let band = try #require(bands.first, "no divider band was measured")
        let hit = splitView.hitTest(
            splitView.convert(CGPoint(x: band.midX, y: band.midY), to: splitView.superview))

        #expect(hit === splitView)
    }

    /// The same question against the whole reconciled tree — pane groups, tab
    /// strips and the drag overlay included. This is the composition the app
    /// actually ships, and the isolated split view above does not prove it.
    @Test func fullTreeAnswersHitTestOnTheDividerBand() throws {
        let fixture = makeDiagnosticLayout()
        let controller = WorkspaceViewController(
            hostProvider: DiagnosticHostProvider(tabs: fixture.layout.allTabs),
            intentSink: DiagnosticIntentSink())
        controller.update(layout: fixture.layout, delta: nil)

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.contentView = controller.view
        controller.view.frame = NSRect(x: 0, y: 0, width: 900, height: 600)
        controller.view.layoutSubtreeIfNeeded()

        let split = try #require(controller.splitController(fixture.splitID))
        let splitView = split.splitView
        let bands = DividerCursorRects.rects(
            subviewFrames: splitView.subviews.map(\.frame), bounds: splitView.bounds,
            isVertical: splitView.isVertical)
        let band = try #require(bands.first, "no divider band was measured")

        let inWindow = splitView.convert(
            CGPoint(x: band.midX, y: band.midY), to: nil)
        let hit = window.contentView?.hitTest(inWindow)

        print("divider band: \(band)")
        print("hitTest from the window root returned: \(type(of: hit as Any))")

        #expect(hit === splitView)
    }
}

@MainActor
private func makeDiagnosticLayout() -> (layout: WorkspaceLayout, splitID: SplitID) {
    let firstTab = WorkspaceTab(
        id: WorkspaceTabID(), title: "First", titleIsAutoNamed: false,
        content: .chat(ChatContentID("first-\(UUID().uuidString)")))
    let secondTab = WorkspaceTab(
        id: WorkspaceTabID(), title: "Second", titleIsAutoNamed: false,
        content: .chat(ChatContentID("second-\(UUID().uuidString)")))
    let firstGroup = PaneGroup(
        id: PaneGroupID(), tabs: [firstTab], activeTabID: firstTab.id)
    let secondGroup = PaneGroup(
        id: PaneGroupID(), tabs: [secondTab], activeTabID: secondTab.id)
    let splitID = SplitID()
    let root = LayoutNode.split(
        id: splitID, axis: .horizontal, fraction: 0.5,
        first: .group(firstGroup.id), second: .group(secondGroup.id))
    guard case .success(let layout) = WorkspaceLayout.make(
        root: root,
        groups: [firstGroup, secondGroup].reduce(into: [:]) { $0[$1.id] = $1 },
        activeGroupID: firstGroup.id
    ) else {
        preconditionFailure("test fixture must be a valid workspace layout")
    }
    return (layout, splitID)
}

@MainActor
private final class DiagnosticHostProvider: WorkspaceHostProvider {
    private let hosts: [WorkspaceTabID: DiagnosticContentHost]

    init(tabs: [WorkspaceTab]) {
        hosts = Dictionary(uniqueKeysWithValues: tabs.map {
            ($0.id, DiagnosticContentHost(tabID: $0.id))
        })
    }

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? { hosts[tabID] }
}

@MainActor
private final class DiagnosticContentHost: WorkspaceContentHost {
    let tabID: WorkspaceTabID
    let viewController = NSViewController()

    init(tabID: WorkspaceTabID) {
        self.tabID = tabID
        viewController.view = NSView()
    }

    func setVisible(_ isVisible: Bool) {}

    @discardableResult
    func fulfill(_ intent: FocusIntent) -> Bool { true }
}

private final class DiagnosticIntentSink: WorkspaceIntentSink, @unchecked Sendable {
    func send(_ intent: WorkspaceIntent) {}
}
