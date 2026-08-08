import Foundation
import Testing
import TillerCore
import TillerWorkspace
@testable import Tiller

@Suite(.serialized)
@MainActor
struct DiffTabDedupTests {
    @Test func droppingTheSameDiffPathTwiceLeavesOneTabAndActivatesIt() async {
        let coordinator = makeCoordinator()
        let worktree = fixtureWorktree()
        await coordinator.restore(worktree: worktree)
        let groupID = coordinator.layouts[worktree.id]!.activeGroupID
        let intent = WorkspaceIntent.requestOpenDiff(
            path: "Sources/Feature.swift", target: .center(groupID))

        await coordinator.handle(intent, in: worktree)
        guard let first = coordinator.layouts[worktree.id]?.allTabs.first else {
            Issue.record("the first diff drop must create a tab")
            return
        }
        await coordinator.handle(intent, in: worktree)

        let tabs = coordinator.layouts[worktree.id]?.allTabs ?? []
        #expect(tabs.count == 1)
        #expect(tabs[0].id == first.id)
        #expect(coordinator.layouts[worktree.id]?.group(groupID)?.activeTabID == first.id)
    }

    @Test func anExternalDiffIntentRetainsTheResolvedTabStripIndex() {
        let groupID = PaneGroupID()
        let intent = WorkspaceIntent.requestOpenDiff(
            path: "Sources/Feature.swift",
            target: .tabStrip(groupID, insertionIndex: 3))

        #expect(intent == .requestOpenDiff(
            path: "Sources/Feature.swift",
            target: .tabStrip(groupID, insertionIndex: 3)))
    }

    private func makeCoordinator() -> WorkspaceCoordinator {
        WorkspaceCoordinator(
            persistence: FakeWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.diff: DiffContentAdapter()])
    }

    private func fixtureWorktree() -> Worktree {
        Worktree(
            id: UUID(uuidString: "00000000-0000-4000-8000-000000000202")!,
            projectId: UUID(uuidString: "00000000-0000-4000-8000-000000000001")!,
            branch: "diff-dedup", path: "/tmp/tiller-diff-dedup")
    }
}
