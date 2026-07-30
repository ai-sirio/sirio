import AppKit
import CoreGraphics
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct OverflowTests {
    @Test
    func minimumCanvasIsTheSumOfPreferredGroupSizesPlusDividers() {
        let layout = deepMixedOrientationLayout(groups: 16, tabs: 64)
        let size = OverflowCanvas.minimumCanvasSize(for: layout)

        #expect(size.width >= 240 * 4)
        #expect(size.height >= 160 * 4)
        #expect(OverflowCanvas.isOverflowing(canvas: size, viewport: CGSize(width: 600, height: 400)))
    }

    @Test
    func enteringOverflowDoesNotChangeAnyPreferredFraction() {
        let layout = deepMixedOrientationLayout(groups: 16, tabs: 64)
        let before = Dictionary(uniqueKeysWithValues: layout.splitIDs().compactMap { split in
            layout.preferredFraction(for: split).map { (split, $0) }
        })

        _ = OverflowCanvas.isOverflowing(
            canvas: OverflowCanvas.minimumCanvasSize(for: layout),
            viewport: CGSize(width: 400, height: 280)
        )

        let after = Dictionary(uniqueKeysWithValues: layout.splitIDs().compactMap { split in
            layout.preferredFraction(for: split).map { (split, $0) }
        })
        #expect(after == before)
    }

    @Test
    func leavingOverflowRestoresFittingFromTheSamePreferredFractions() {
        let layout = deepMixedOrientationLayout(groups: 16, tabs: 64)
        let preferred = layout.splitIDs().compactMap { layout.preferredFraction(for: $0) }
        let canvas = OverflowCanvas.minimumCanvasSize(for: layout)

        #expect(OverflowCanvas.isOverflowing(canvas: canvas, viewport: CGSize(width: 600, height: 400)))
        #expect(!OverflowCanvas.isOverflowing(canvas: canvas, viewport: canvas))
        #expect(layout.splitIDs().compactMap { layout.preferredFraction(for: $0) } == preferred)
    }

    @Test
    func revealingAnOffscreenGroupScrollsTheMinimumDistance() {
        let groupFrame = CGRect(x: 500, y: 0, width: 100, height: 160)
        let viewport = CGRect(x: 0, y: 0, width: 200, height: 160)

        #expect(
            OverflowCanvas.minimumScrollOffset(
                for: groupFrame, viewport: viewport, currentOffset: .zero
            ) == CGPoint(x: 400, y: 0)
        )
    }

    @Test
    func theOverflowMenuListsEveryLocalTabInStableOrder() {
        let tabs = makeTabs(count: 4)
        let group = PaneGroup(id: PaneGroupID(), tabs: tabs, activeTabID: tabs[2].id)

        let entries = PaneTabStripView.overflowMenuItems(for: group)

        #expect(entries.map(\.tabID) == tabs.map(\.id))
        #expect(entries.map(\.title) == tabs.map(\.title))
        #expect(entries.map(\.isActive) == [false, false, true, false])
    }

    @Test
    func theTabStripKeepsTheActiveTabVisibleAfterActivation() {
        let tabs = makeTabs(count: 3)
        let strip = PaneTabStripView()
        strip.setTabFrames([
            tabs[0].id: CGRect(x: 0, y: 0, width: 80, height: 32),
            tabs[1].id: CGRect(x: 80, y: 0, width: 80, height: 32),
            tabs[2].id: CGRect(x: 160, y: 0, width: 80, height: 32)
        ], viewport: CGRect(x: 0, y: 0, width: 100, height: 32))

        strip.activate(tab: tabs[2].id)

        #expect(strip.contentOffset == CGPoint(x: 140, y: 0))
        #expect(strip.isTabVisible(tabs[2].id))
    }

    @Test
    func edgeAutoscrollTriggersOnlyWhileADragIsActive() {
        let inactiveStrip = PaneTabStripView()
        let viewport = CGRect(x: 0, y: 0, width: 100, height: 32)
        #expect(inactiveStrip.edgeAutoscrollDelta(pointerX: 95, viewport: viewport, contentWidth: 300) == 0)

        let activeStrip = PaneTabStripView()
        activeStrip.dragSession.pressBegan(
            tab: WorkspaceTabID(), at: CGPoint(x: 10, y: 10),
            inTabFrame: CGRect(x: 0, y: 0, width: 80, height: 32)
        )
        activeStrip.dragSession.pointerMoved(to: CGPoint(x: 20, y: 10)) { _ in .none }

        #expect(activeStrip.edgeAutoscrollDelta(pointerX: 95, viewport: viewport, contentWidth: 300) > 0)
    }

    @Test
    func scrolledOutGroupsRemainVisibleFromTheReconcilerMountPointOfView() {
        let tab = makeTabs(count: 1)[0]
        let group = PaneGroup(id: PaneGroupID(), tabs: [tab], activeTabID: tab.id)
        let layout = makeLayout(root: .group(group.id), groups: [group], activeGroupID: group.id)
        let host = FakeContentHost(tabID: tab.id, isVisible: false)
        let provider = SingleHostProvider(host: host)
        let reconciler = WorkspaceReconciler(hostProvider: provider)
        reconciler.reconcile(to: layout, delta: nil)

        let canvas = OverflowCanvas(frame: CGRect(x: 0, y: 0, width: 100, height: 100))
        canvas.setCanvasSize(CGSize(width: 500, height: 160))
        canvas.reveal(groupFrame: CGRect(x: 400, y: 0, width: 100, height: 160), animated: false)

        #expect(host.isVisible)
    }
}

@MainActor
private final class SingleHostProvider: WorkspaceHostProvider {
    let host: FakeContentHost

    init(host: FakeContentHost) {
        self.host = host
    }

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? {
        tabID == host.tabID ? host : nil
    }
}

private func makeTabs(count: Int) -> [WorkspaceTab] {
    (0..<count).map { index in
        WorkspaceTab(
            id: WorkspaceTabID(), title: "Tab \(index)", titleIsAutoNamed: false,
            content: .chat(ChatContentID("overflow-\(UUID().uuidString)"))
        )
    }
}

private func deepMixedOrientationLayout(groups count: Int, tabs tabCount: Int) -> WorkspaceLayout {
    let tabsPerGroup = tabCount / count
    let groups = (0..<count).map { index in
        let tabs = makeTabs(count: tabsPerGroup).map { tab in
            WorkspaceTab(
                id: tab.id, title: "Group \(index) \(tab.title)", titleIsAutoNamed: false,
                content: tab.content
            )
        }
        return PaneGroup(id: PaneGroupID(), tabs: tabs, activeTabID: tabs.first?.id)
    }

    func node(_ ids: ArraySlice<PaneGroupID>, depth: Int) -> LayoutNode {
        if ids.count == 1 {
            return .group(ids[ids.startIndex])
        }
        let midpoint = ids.index(ids.startIndex, offsetBy: ids.count / 2)
        let axis: WorkspaceSplitAxis = depth.isMultiple(of: 2) ? .horizontal : .vertical
        return .split(
            id: SplitID(), axis: axis, fraction: 0.5,
            first: node(ids[..<midpoint], depth: depth + 1),
            second: node(ids[midpoint...], depth: depth + 1)
        )
    }

    return makeLayout(
        root: node(groups.map(\.id)[...], depth: 0),
        groups: groups,
        activeGroupID: groups[0].id
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
