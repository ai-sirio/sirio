import AppKit
import Testing
import TillerCore
@testable import TillerWorkspace

@Suite @MainActor
struct ReconcilerPerformanceTests {
    @Test
    func aLocalActivationRebuildsZeroSplitControllers() throws {
        let layout = makeLayout(groupCount: 8, tabsPerGroup: 4)
        let reconciler = WorkspaceReconciler(hostProvider: EmptyHostProvider())

        reconciler.reconcile(to: layout, delta: nil)
        let baselineGroupCreations = reconciler.groupControllerCreationCount
        let baselineSplitCreations = reconciler.splitControllerCreationCount

        var groups = layout.groups
        let activeGroupID = layout.orderedGroupIDs[1]
        let activeTabID = try #require(groups[activeGroupID]?.tabs[1].id)
        groups[activeGroupID]?.activeTabID = activeTabID
        let activated = try #require(try WorkspaceLayout.make(
            root: layout.root, groups: groups, activeGroupID: activeGroupID
        ).get())
        let activation = try #require(try WorkspaceLayoutEngine.apply(
            .activateTab(activeTabID), to: layout).get())
        #expect(activation.layout == activated)

        reconciler.reconcile(
            to: activation.layout, delta: activation.delta)

        #expect(baselineGroupCreations == 8)
        #expect(baselineSplitCreations == 7)
        #expect(reconciler.groupControllerCreationCount == baselineGroupCreations)
        #expect(reconciler.splitControllerCreationCount == baselineSplitCreations)
    }

    private func makeLayout(groupCount: Int, tabsPerGroup: Int) -> WorkspaceLayout {
        let groupIDs = (0..<groupCount).map { _ in PaneGroupID() }
        let groups = Dictionary(uniqueKeysWithValues: groupIDs.enumerated().map { index, id in
            let tabs = (0..<tabsPerGroup).map { tabIndex in
                WorkspaceTab(
                    id: WorkspaceTabID(),
                    title: "Group \(index) Tab \(tabIndex)",
                    titleIsAutoNamed: true,
                    content: .chat(ChatContentID("fixture-\(index)-\(tabIndex)")))
            }
            return (id, PaneGroup(id: id, tabs: tabs, activeTabID: tabs[0].id))
        })

        func makeNode(_ range: Range<Int>) -> LayoutNode {
            guard range.count > 1 else { return .group(groupIDs[range.lowerBound]) }
            let midpoint = range.lowerBound + range.count / 2
            return .split(
                id: SplitID(), axis: .horizontal, fraction: 0.5,
                first: makeNode(range.lowerBound..<midpoint),
                second: makeNode(midpoint..<range.upperBound))
        }

        return try! WorkspaceLayout.make(
            root: makeNode(0..<groupCount), groups: groups, activeGroupID: groupIDs[0]
        ).get()
    }
}

@MainActor
private final class EmptyHostProvider: WorkspaceHostProvider {
    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? { nil }
}
