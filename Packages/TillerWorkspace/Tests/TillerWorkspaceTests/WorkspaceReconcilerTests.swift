import AppKit
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct WorkspaceReconcilerTests {
    @Test
    func movingATabKeepsTheSameControllerInstance() throws {
        let groupA = PaneGroupID()
        let groupB = PaneGroupID()
        let split = SplitID()
        let tabA = makeTab()
        let tabB = makeTab()
        let tabC = makeTab()
        let layoutA = splitLayout(
            split: split,
            first: PaneGroup(id: groupA, tabs: [tabA, tabB], activeTabID: tabA.id),
            second: PaneGroup(id: groupB, tabs: [tabC], activeTabID: tabC.id)
        )
        let layoutAfterMove = splitLayout(
            split: split,
            first: PaneGroup(id: groupA, tabs: [tabB], activeTabID: tabB.id),
            second: PaneGroup(id: groupB, tabs: [tabC, tabA], activeTabID: tabA.id)
        )
        let provider = FakeHostProvider(tabs: [tabA, tabB, tabC])
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(to: layoutA, delta: nil)
        let before = ObjectIdentifier(try #require(provider.host(for: tabA.id)).viewController)

        reconciler.reconcile(
            to: layoutAfterMove,
            delta: WorkspaceLayoutDelta(movedTabs: [tabA.id], isStructural: true)
        )
        let after = ObjectIdentifier(try #require(provider.host(for: tabA.id)).viewController)

        #expect(before == after)
    }

    @Test
    func closingActiveTabDetachesViewEvenWhenHostWasReleased() throws {
        let tab = makeTab()
        let group = PaneGroup(id: PaneGroupID(), tabs: [tab], activeTabID: tab.id)
        let layout = makeLayout(root: .group(group.id), groups: [group], activeGroupID: group.id)
        let provider = FakeHostProvider(hosts: [tab.id: FakeContentHost(tabID: tab.id)])
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(to: layout, delta: nil)
        let mountedView = provider.hosts[tab.id]!.viewController.view
        #expect(mountedView.superview != nil)

        provider.hosts = [:]
        let emptyGroup = PaneGroup(id: group.id, tabs: [], activeTabID: nil)
        let layoutAfterClose = makeLayout(root: .group(group.id), groups: [emptyGroup], activeGroupID: group.id)
        reconciler.reconcile(to: layoutAfterClose, delta: WorkspaceLayoutDelta(removedTabs: [tab.id]))

        #expect(mountedView.superview == nil)
    }

    @Test
    func exactlyOneHostIsMountedPerVisibleGroup() {
        let groups = (0..<4).map { _ in
            PaneGroup(id: PaneGroupID(), tabs: [makeTab(), makeTab(), makeTab()], activeTabID: nil)
        }
        let normalizedGroups = groups.map { group in
            PaneGroup(id: group.id, tabs: group.tabs, activeTabID: group.tabs.first?.id)
        }
        let root = LayoutNode.split(
            id: SplitID(), axis: .horizontal, fraction: 0.5,
            first: .split(
                id: SplitID(), axis: .vertical, fraction: 0.5,
                first: .group(normalizedGroups[0].id), second: .group(normalizedGroups[1].id)
            ),
            second: .split(
                id: SplitID(), axis: .vertical, fraction: 0.5,
                first: .group(normalizedGroups[2].id), second: .group(normalizedGroups[3].id)
            )
        )
        let layout = makeLayout(root: root, groups: normalizedGroups, activeGroupID: normalizedGroups[0].id)
        let provider = FakeHostProvider(tabs: normalizedGroups.flatMap(\.tabs))
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(to: layout, delta: nil)

        #expect(reconciler.mountedHostCount == 4)
    }

    @Test
    func collapsingAGroupPrunesOnlyItsOwnCachedControllers() throws {
        let groupA = PaneGroup(id: PaneGroupID(), tabs: [makeTab()], activeTabID: nil)
        let groupB = PaneGroup(id: PaneGroupID(), tabs: [makeTab()], activeTabID: nil)
        let split = SplitID()
        let layoutA = splitLayout(split: split, first: groupA, second: groupB)
        let rootGroup = PaneGroup(
            id: groupA.id, tabs: groupA.tabs, activeTabID: groupA.tabs.first?.id
        )
        let layoutAfterCollapse = makeLayout(
            root: .group(groupA.id), groups: [rootGroup], activeGroupID: groupA.id
        )
        let provider = FakeHostProvider(tabs: groupA.tabs + groupB.tabs)
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(to: layoutA, delta: nil)
        let groupControllerBefore = try #require(reconciler.groupController(groupA.id))

        reconciler.reconcile(
            to: layoutAfterCollapse,
            delta: WorkspaceLayoutDelta(
                removedGroups: [groupB.id], collapsedSplits: [split], isStructural: true
            )
        )

        #expect(reconciler.groupController(groupA.id) === groupControllerBefore)
        #expect(reconciler.groupController(groupB.id) == nil)
        #expect(reconciler.splitController(split) == nil)
        #expect(reconciler.mountedHostCount == 1)
    }

    @Test
    func reorderWithinAGroupDoesNotRebuildSplitControllers() throws {
        let groupA = PaneGroup(id: PaneGroupID(), tabs: [makeTab(), makeTab()], activeTabID: nil)
        let groupB = PaneGroup(id: PaneGroupID(), tabs: [makeTab()], activeTabID: nil)
        let split = SplitID()
        let layoutA = splitLayout(split: split, first: groupA, second: groupB)
        let reorderedA = PaneGroup(
            id: groupA.id, tabs: Array(groupA.tabs.reversed()), activeTabID: groupA.tabs.last?.id
        )
        let layoutAfterReorder = splitLayout(split: split, first: reorderedA, second: groupB)
        let provider = FakeHostProvider(tabs: groupA.tabs + groupB.tabs)
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(to: layoutA, delta: nil)
        let splitControllerBefore = try #require(reconciler.splitController(split))

        reconciler.reconcile(
            to: layoutAfterReorder,
            delta: WorkspaceLayoutDelta(reorderedGroups: [groupA.id])
        )

        #expect(reconciler.splitController(split) === splitControllerBefore)
    }

    @Test
    func firstResponderSurvivesADetachReattachCycle() throws {
        let tab = makeTab()
        let group = PaneGroup(id: PaneGroupID(), tabs: [tab], activeTabID: tab.id)
        let layout = makeLayout(root: .group(group.id), groups: [group], activeGroupID: group.id)
        let paneView = FocusableTestView(frame: NSRect(x: 0, y: 0, width: 100, height: 100))
        let provider = FakeHostProvider(
            hosts: [tab.id: FakeContentHost(tabID: tab.id, viewController: FocusableViewController(paneView: paneView))]
        )
        let reconciler = WorkspaceReconciler(hostProvider: provider)
        let window = makeOffscreenWindow()

        reconciler.reconcile(to: layout, delta: nil)
        window.contentView = reconciler.rootViewController.view
        reconciler.rootViewController.view.frame = NSRect(x: 0, y: 0, width: 200, height: 200)
        reconciler.rootViewController.view.layoutSubtreeIfNeeded()
        #expect(window.makeFirstResponder(paneView))

        reconciler.reconcile(to: layout, delta: WorkspaceLayoutDelta(reorderedGroups: [group.id]))

        #expect(window.firstResponder === paneView)
    }

    @Test
    func focusIntentIsRetriedOnceAfterHostAttachment() async throws {
        let tab = makeTab()
        let group = PaneGroup(id: PaneGroupID(), tabs: [tab], activeTabID: tab.id)
        let layout = makeLayout(root: .group(group.id), groups: [group], activeGroupID: group.id)
        let host = FakeContentHost(tabID: tab.id, fulfillResult: false)
        let provider = FakeHostProvider(hosts: [tab.id: host])
        let reconciler = WorkspaceReconciler(hostProvider: provider)

        reconciler.reconcile(
            to: layout,
            delta: WorkspaceLayoutDelta(activeTabChanges: [group.id: tab.id])
        )
        host.fulfillResult = true
        for _ in 0..<100 where host.recordedFocusIntents.count < 2 {
            try await Task.sleep(for: .milliseconds(10))
        }

        #expect(host.recordedFocusIntents == [.focusTab(tab.id), .focusTab(tab.id)])
    }
}

@MainActor
private final class FakeHostProvider: WorkspaceHostProvider {
    var hosts: [WorkspaceTabID: FakeContentHost]

    init(hosts: [WorkspaceTabID: FakeContentHost] = [:]) {
        self.hosts = hosts
    }

    convenience init(tabs: [WorkspaceTab]) {
        self.init(hosts: Dictionary(uniqueKeysWithValues: tabs.map {
            ($0.id, FakeContentHost(tabID: $0.id))
        }))
    }

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? {
        hosts[tabID]
    }
}

private func makeTab() -> WorkspaceTab {
    WorkspaceTab(
        id: WorkspaceTabID(), title: "Tab", titleIsAutoNamed: false,
        content: .chat(ChatContentID(UUID().uuidString))
    )
}

private func makeLayout(
    root: LayoutNode,
    groups: [PaneGroup],
    activeGroupID: PaneGroupID
) -> WorkspaceLayout {
    guard case .success(let layout) = WorkspaceLayout.make(
        root: root,
        groups: Dictionary(uniqueKeysWithValues: groups.map { ($0.id, $0) }),
        activeGroupID: activeGroupID
    ) else {
        preconditionFailure("test fixture must be a valid workspace layout")
    }
    return layout
}

private func splitLayout(
    split: SplitID,
    first: PaneGroup,
    second: PaneGroup
) -> WorkspaceLayout {
    makeLayout(
        root: .split(
            id: split, axis: .horizontal, fraction: 0.5,
            first: .group(first.id), second: .group(second.id)
        ),
        groups: [first, second].map { PaneGroup(id: $0.id, tabs: $0.tabs, activeTabID: $0.tabs.first?.id) },
        activeGroupID: first.id
    )
}

@MainActor
private final class FocusableViewController: NSViewController {
    private let paneView: NSView

    init(paneView: NSView) {
        self.paneView = paneView
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = paneView
    }
}

private final class FocusableTestView: NSView {
    override var acceptsFirstResponder: Bool { true }
}

@MainActor
private func makeOffscreenWindow() -> NSWindow {
    NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 200, height: 200),
        styleMask: [.titled], backing: .buffered, defer: false
    )
}
