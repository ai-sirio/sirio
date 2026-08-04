import AppKit
import Testing
import TillerCore
import TillerWorkspace

@Suite @MainActor
struct WorkspaceGeometryTests {
    @Test
    func effectiveFractionClampsWithoutOverwritingThePreferredValue() throws {
        let fixture = makeLayout(axis: .horizontal, fraction: 0.05)
        let controller = WorkspaceViewController(
            hostProvider: GeometryHostProvider(tabs: fixture.layout.allTabs),
            intentSink: RecordingIntentSink()
        )

        controller.update(layout: fixture.layout, delta: nil)
        let window = install(controller)

        #expect(controller.effectiveFraction(for: fixture.splitID)! > 0.05)
        #expect(controller.currentLayout.preferredFraction(for: fixture.splitID) == 0.05)
        _ = window
    }

    @Test
    func horizontalAxisPlacesFirstChildOnTheLeft() throws {
        let fixture = makeLayout(axis: .horizontal, fraction: 0.5)
        let controller = makeController(for: fixture.layout)

        let window = install(controller)
        let split = try #require(controller.splitController(fixture.splitID))
        let first = split.splitView.subviews[0].frame
        let second = split.splitView.subviews[1].frame

        #expect(split.splitView.isVertical == true)
        #expect(first.minX < second.minX)
        _ = window
    }

    @Test
    func verticalAxisPlacesFirstChildOnTop() throws {
        let fixture = makeLayout(axis: .vertical, fraction: 0.5)
        let controller = makeController(for: fixture.layout)

        let window = install(controller)
        let split = try #require(controller.splitController(fixture.splitID))
        let first = split.splitView.subviews[0].frame
        let second = split.splitView.subviews[1].frame

        #expect(split.splitView.isVertical == false)
        #expect(first.maxY > second.maxY)
        _ = window
    }

    @Test
    func dividerThicknessMatchesTheNativeThickDivider() throws {
        let fixture = makeLayout(axis: .horizontal, fraction: 0.5)
        let controller = makeController(for: fixture.layout)
        let window = install(controller)
        let split = try #require(controller.splitController(fixture.splitID))

        #expect(WorkspaceMetrics.dividerThickness == 10)
        #expect(split.splitView.dividerThickness == WorkspaceMetrics.dividerThickness)
        _ = window
    }
}

@MainActor
private func makeController(for layout: WorkspaceLayout) -> WorkspaceViewController {
    WorkspaceViewController(
        hostProvider: GeometryHostProvider(tabs: layout.allTabs),
        intentSink: RecordingIntentSink()
    ).then { $0.update(layout: layout, delta: nil) }
}

@MainActor
private func install(_ controller: WorkspaceViewController) -> NSWindow {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
        styleMask: [.titled], backing: .buffered, defer: false
    )
    window.contentView = controller.view
    controller.view.frame = NSRect(x: 0, y: 0, width: 600, height: 400)
    controller.view.layoutSubtreeIfNeeded()
    return window
}

private struct GeometryLayoutFixture {
    let layout: WorkspaceLayout
    let splitID: SplitID
}

private func makeLayout(axis: WorkspaceSplitAxis, fraction: Double) -> GeometryLayoutFixture {
    let firstTab = WorkspaceTab(
        id: WorkspaceTabID(), title: "First", titleIsAutoNamed: false,
        content: .chat(ChatContentID("first-\(UUID().uuidString)"))
    )
    let secondTab = WorkspaceTab(
        id: WorkspaceTabID(), title: "Second", titleIsAutoNamed: false,
        content: .chat(ChatContentID("second-\(UUID().uuidString)"))
    )
    let firstGroup = PaneGroup(
        id: PaneGroupID(), tabs: [firstTab], activeTabID: firstTab.id
    )
    let secondGroup = PaneGroup(
        id: PaneGroupID(), tabs: [secondTab], activeTabID: secondTab.id
    )
    let splitID = SplitID()
    let root = LayoutNode.split(
        id: splitID, axis: axis, fraction: fraction,
        first: .group(firstGroup.id), second: .group(secondGroup.id)
    )
    guard case .success(let layout) = WorkspaceLayout.make(
        root: root,
        groups: [firstGroup, secondGroup].reduce(into: [:]) { $0[$1.id] = $1 },
        activeGroupID: firstGroup.id
    ) else {
        preconditionFailure("test fixture must be a valid workspace layout")
    }
    return GeometryLayoutFixture(layout: layout, splitID: splitID)
}

@MainActor
private final class GeometryHostProvider: WorkspaceHostProvider {
    private let hosts: [WorkspaceTabID: FakeContentHost]

    init(tabs: [WorkspaceTab]) {
        hosts = Dictionary(uniqueKeysWithValues: tabs.map {
            ($0.id, FakeContentHost(tabID: $0.id))
        })
    }

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? {
        hosts[tabID]
    }
}

@MainActor
private final class RecordingIntentSink: WorkspaceIntentSink {
    private(set) var intents: [WorkspaceIntent] = []

    func send(_ intent: WorkspaceIntent) {
        intents.append(intent)
    }
}

private extension WorkspaceViewController {
    func then(_ body: (WorkspaceViewController) -> Void) -> WorkspaceViewController {
        body(self)
        return self
    }
}
