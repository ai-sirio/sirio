import Testing
import Foundation
@testable import TillerCore

@Suite struct ControlListingTests {
    private func makeProject(name: String) -> Project {
        Project(id: UUID(), name: name, rootPath: "/tmp/\(name)")
    }

    @Test func workspaceRowsFlattenAndMarkSelection() {
        let p = makeProject(name: "tiller")
        let a = Worktree(id: UUID(), projectId: p.id, branch: "main", path: "/tmp/tiller")
        let b = Worktree(id: UUID(), projectId: p.id, branch: "feature", path: "/tmp/tiller-feature")
        let rows = ControlListing.workspaceRows(
            projects: [p], worktrees: [p.id: [a, b]], selectedWorktreeId: b.id
        )
        #expect(rows.count == 2)
        #expect(rows[0]["branch"] == "feature")   // sorted by branch within project
        #expect(rows[0]["selected"] == "true")
        #expect(rows[1]["selected"] == "false")
        #expect(rows[1]["project"] == "tiller")
        #expect(rows[1]["id"] == a.id.uuidString)
        #expect(rows[1]["path"] == "/tmp/tiller")
    }

    @Test func workspaceRowsOrderByProjectName() {
        let p1 = makeProject(name: "zebra")
        let p2 = makeProject(name: "alpha")
        let w1 = Worktree(id: UUID(), projectId: p1.id, branch: "main", path: "/z")
        let w2 = Worktree(id: UUID(), projectId: p2.id, branch: "main", path: "/a")
        let rows = ControlListing.workspaceRows(
            projects: [p1, p2], worktrees: [p1.id: [w1], p2.id: [w2]],
            selectedWorktreeId: nil
        )
        #expect(rows.map { $0["project"]! } == ["alpha", "zebra"])
    }

    @Test func paneRowsListLeavesWithTabTitleAndActiveFlag() {
        let paneA = UUID(); let paneB = UUID(); let paneC = UUID()
        let tab1 = LegacyWorkspaceTab(id: UUID(), title: "Shell 1",
                                tree: .split(axis: .horizontal,
                                             first: .leaf(id: paneA),
                                             second: .leaf(id: paneB)))
        let tab2 = LegacyWorkspaceTab(id: UUID(), title: "claude", tree: .leaf(id: paneC))
        let rows = ControlListing.paneRows([
            ["id": paneA.uuidString, "tab": tab1.title, "title": "zsh",
             "agent": "", "active": "false"],
            ["id": paneB.uuidString, "tab": tab1.title, "title": "",
             "agent": "", "active": "false"],
            ["id": paneC.uuidString, "tab": tab2.title, "title": "",
             "agent": "claude", "active": "true"],
        ])
        #expect(rows.count == 3)
        #expect(rows[0]["id"] == paneA.uuidString)
        #expect(rows[0]["tab"] == "Shell 1")
        #expect(rows[0]["title"] == "zsh")
        #expect(rows[0]["agent"] == "")
        #expect(rows[0]["active"] == "false")
        #expect(rows[2]["id"] == paneC.uuidString)
        #expect(rows[2]["agent"] == "claude")
        #expect(rows[2]["active"] == "true")
    }
}
