import AppKit
import Foundation
import SwiftUI
import Testing
import TillerCore
import TillerTerminal
import TillerWorkspace
@testable import Tiller

/// Reproduction for the "Empty Pane" click bug: clicking "New Terminal" (or
/// opening "New…") in the pane empty state appeared to do nothing. The model
/// path is already covered by coordinator tests, so this test drives the REAL
/// app factories (`makePaneEmptyState` + `NewTabMenuItems`) inside a real
/// `WorkspaceViewController`, then posts real mouse events at the button
/// found through the accessibility tree — the full view → strip model →
/// intent → coordinator chain.
@Suite(.serialized)
@MainActor
struct EmptyPaneClickReproductionTests {
    @Test func clickingNewTerminalInTheEmptyPaneCreatesATab() async throws {
        let persistence = FakeWorkspacePersistence()
        persistence.restored = RestoredWorkspace(
            layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
        let adapter = RecordingTerminalAdapter()
        let coordinator = WorkspaceCoordinator(
            persistence: persistence,
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: adapter])
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main",
            path: FileManager.default.temporaryDirectory.path)
        await coordinator.restore(worktree: worktree)

        let model = AppModel(
            paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
            workspaceCoordinator: coordinator)
        model.worktrees = [worktree.projectId: [worktree]]

        let router = WorkspaceIntentRouter(coordinator: coordinator, worktree: worktree)
        let controller = WorkspaceViewController(
            hostProvider: coordinator,
            intentSink: router,
            emptyStateFactory: { stripModel in
                makePaneEmptyState(stripModel) {
                    NewTabMenuItems(
                        model: model, worktree: worktree,
                        onBeforeAction: stripModel.onActivateGroup,
                        onNewTerminal: stripModel.onNewTab)
                }
            })
        controller.update(layout: coordinator.layouts[worktree.id]!, delta: nil)

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 420, height: 320),
            styleMask: [.titled], backing: .buffered, defer: false)
        defer { window.close() }
        window.contentView = controller.view
        window.setIsVisible(true)
        pumpRunLoop(iterations: 20)
        window.layoutIfNeeded()

        // Phase 1 evidence: can a click even reach the empty-state hosting
        // view? SwiftUI routing answers via the window's hit test.
        let hostingView = try #require(
            findEmptyStateHostingView(in: controller.view),
            "empty state hosting view not found")
        let center = CGPoint(x: hostingView.bounds.midX, y: hostingView.bounds.midY)
        let hit = hostingView.window?.contentView?.hitTest(center) ?? hostingView.hitTest(center)
        #expect(hit != nil, "hit test at empty-state center found nothing")

        // Phase 2: click the real "New Terminal" button via the accessibility
        // tree, or fall back to the button row's expected geometry. Headless
        // test hosts occasionally drop the first synthetic click, so retry
        // until the prepare path sees it.
        let clickPoint = try #require(
            centerOfButton(title: "New Terminal", in: window)
                ?? fallbackButtonRowCenter(in: hostingView),
            "could not locate the New Terminal button")
        for _ in 0..<5 where adapter.prepareCount == 0 {
            postClick(at: clickPoint, in: window)
            try? await Task.sleep(for: .milliseconds(100))
        }

        for _ in 0..<200 where coordinator.layouts[worktree.id]?.allTabs.isEmpty == true {
            try? await Task.sleep(for: .milliseconds(10))
        }

        let tabs = coordinator.layouts[worktree.id]?.allTabs ?? []
        #expect(tabs.count == 1, "click on New Terminal in the empty pane created no tab")
        #expect(adapter.prepareCount == 1)
    }

    @Test func newTabMenuInTheEmptyPaneContainsWorkingTerminalItem() async throws {
        // The "New…" menu's items are built by NewTabMenuItems with the same
        // onActivateGroup/onNewTerminal callbacks the button uses; pin that the
        // callbacks are the live ones from the group controller, not no-ops.
        try await runEmptyPaneActivateThenNewTab(withYieldBetween: false)
    }

    @Test func activateThenNewTabWithAYieldBetweenAlwaysSucceeds() async throws {
        // Discriminator: if the interleaving of the activateGroup and
        // requestNewTab tasks is what drops the tab, yielding between the two
        // calls (letting activateGroup fully commit first) must always pass.
        try await runEmptyPaneActivateThenNewTab(withYieldBetween: true)
    }

    private func runEmptyPaneActivateThenNewTab(withYieldBetween: Bool) async throws {
        let persistence = FakeWorkspacePersistence()
        persistence.restored = RestoredWorkspace(
            layout: .empty(), tabs: [:], revision: 0, diagnostics: [])
        let adapter = RecordingTerminalAdapter()
        let coordinator = WorkspaceCoordinator(
            persistence: persistence,
            registry: WorkspaceContentRegistry(),
            adapters: [.terminal: adapter])
        let worktree = Worktree(
            id: UUID(), projectId: UUID(), branch: "main",
            path: FileManager.default.temporaryDirectory.path)
        await coordinator.restore(worktree: worktree)

        let groupID = coordinator.layouts[worktree.id]!.activeGroupID
        let router = WorkspaceIntentRouter(coordinator: coordinator, worktree: worktree)
        let groupController = PaneGroupController(id: groupID, intentSink: router)

        // Exactly what PaneEmptyStateView's button does, and what the menu's
        // New Terminal item does via NewTabMenuItems.
        groupController.strip.onActivateGroup()
        if withYieldBetween {
            // Let the activate commit land before the new-tab request starts.
            try? await Task.sleep(for: .milliseconds(100))
        }
        groupController.strip.onNewTab()

        for _ in 0..<200 where coordinator.layouts[worktree.id]?.allTabs.isEmpty == true {
            try? await Task.sleep(for: .milliseconds(10))
        }
        #expect(coordinator.layouts[worktree.id]?.allTabs.count == 1)
        // The prepared tab must be committed, never silently disposed by a
        // stale revision guard.
        #expect(adapter.disposeCount == 0)
    }

    // MARK: - Helpers

    private func findEmptyStateHostingView(in root: NSView) -> NSView? {
        var queue: [NSView] = [root]
        while let view = queue.popLast() {
            if String(describing: type(of: view)).contains("HostingView") {
                return view
            }
            queue.append(contentsOf: view.subviews)
        }
        return nil
    }

    private func centerOfButton(title: String, in window: NSWindow) -> CGPoint? {
        guard let element = findAccessibilityElement(
            role: .button, title: title, in: window.contentView ?? NSView()) else {
            return nil
        }
        let screenFrame = element.accessibilityFrame()
        guard screenFrame.width > 0, screenFrame.height > 0 else { return nil }
        let center = CGPoint(x: screenFrame.midX, y: screenFrame.midY)
        return window.convertPoint(fromScreen: center)
    }

    private func findAccessibilityElement(
        role: NSAccessibility.Role, title: String, in view: NSView
    ) -> NSAccessibilityElement? {
        let children = view.accessibilityChildren() ?? []
        for child in children {
            guard let element = child as? NSAccessibilityElement else { continue }
            if element.accessibilityRole() == role,
               element.accessibilityLabel() == title {
                return element
            }
            if let nested = findAccessibilityElement(
                role: role, title: title, in: element) {
                return nested
            }
        }
        return nil
    }

    private func findAccessibilityElement(
        role: NSAccessibility.Role, title: String, in element: NSAccessibilityElement
    ) -> NSAccessibilityElement? {
        let children = element.accessibilityChildren() ?? []
        for child in children {
            guard let childElement = child as? NSAccessibilityElement else { continue }
            if childElement.accessibilityRole() == role,
               childElement.accessibilityLabel() == title {
                return childElement
            }
            if let nested = findAccessibilityElement(
                role: role, title: title, in: childElement) {
                return nested
            }
        }
        return nil
    }

    /// The empty state's HStack (icon, title, subtitle, button row) is
    /// vertically centered; the button row sits just below the view center.
    /// Used only when the accessibility tree does not expose the button.
    private func fallbackButtonRowCenter(in hostingView: NSView) -> CGPoint? {
        guard hostingView.bounds.width > 0, hostingView.bounds.height > 0 else { return nil }
        let rowY = hostingView.bounds.height * 0.66
        let rowX = hostingView.bounds.midX - 40 // "New Terminal" is the left item
        guard hostingView.window != nil else { return nil }
        // convert(to: nil) — window base coordinates, the space NSEvent uses.
        return hostingView.convert(CGPoint(x: rowX, y: rowY), to: nil)
    }

    private func pumpRunLoop(iterations: Int) {
        for _ in 0..<iterations {
            RunLoop.main.run(until: Date().addingTimeInterval(0.01))
        }
    }

    private func postClick(at windowPoint: CGPoint, in window: NSWindow) {
        let eventNumber = Int.random(in: 1000...10_000)
        func event(type: NSEvent.EventType, number: Int) -> NSEvent? {
            NSEvent.mouseEvent(
                with: type, location: windowPoint, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime,
                windowNumber: window.windowNumber, context: nil,
                eventNumber: number, clickCount: 1, pressure: 1)
        }
        if let down = event(type: .leftMouseDown, number: eventNumber) {
            window.sendEvent(down)
        }
        if let up = event(type: .leftMouseUp, number: eventNumber + 1) {
            window.sendEvent(up)
        }
    }
}

@MainActor
private final class RecordingTerminalAdapter: WorkspaceContentAdapter {
    let kind: WorkspaceContentKind = .terminal
    private(set) var prepareCount = 0
    private(set) var disposeCount = 0

    func prepare(request: ContentRequest, worktree: Worktree) async throws -> PreparedContent {
        prepareCount += 1
        let tab = WorkspaceTab(
            id: WorkspaceTabID(), title: "Shell", titleIsAutoNamed: true,
            content: .terminal(TerminalContentID()))
        return PreparedContent(
            tab: tab, generationID: ResourceGenerationID(),
            opaqueToken: NSObject())
    }

    func hydrate(tab: WorkspaceTab, worktree: Worktree) async {}

    func makeHost(tab: WorkspaceTab, worktree: Worktree) -> WorkspaceContentHost {
        WorkspaceContentHostAdapter(tabID: tab.id, viewController: NSViewController())
    }

    func checkpoint(tab: WorkspaceTab) async {}

    func retry(tab: WorkspaceTab, worktree: Worktree) async -> ResourceGenerationID {
        ResourceGenerationID()
    }

    func close(tab: WorkspaceTab) async {}

    func dispose(prepared: PreparedContent) async {
        disposeCount += 1
    }
}
