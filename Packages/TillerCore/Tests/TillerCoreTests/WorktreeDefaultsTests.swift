import Testing
import Foundation
@testable import TillerCore

@Test func resolveBasePrefersExplicitOverride() {
    let project = Project(id: UUID(), name: "p", rootPath: "/tmp/p", defaultWorktreeBase: "release")
    let primary = Worktree(id: UUID(), projectId: project.id, branch: "main", path: "/tmp/p", isPrimary: true)
    #expect(WorktreeDefaults.resolveBase(project: project, worktrees: [primary]) == "release")
}

@Test func resolveBaseFallsBackToPrimaryWorktree() {
    let project = Project(id: UUID(), name: "p", rootPath: "/tmp/p")
    let other = Worktree(id: UUID(), projectId: project.id, branch: "feature", path: "/tmp/f", isPrimary: false)
    let primary = Worktree(id: UUID(), projectId: project.id, branch: "main", path: "/tmp/p", isPrimary: true)
    #expect(WorktreeDefaults.resolveBase(project: project, worktrees: [other, primary]) == "main")
}

@Test func resolveBaseNilWithoutOverrideOrPrimary() {
    let project = Project(id: UUID(), name: "p", rootPath: "/tmp/p")
    let other = Worktree(id: UUID(), projectId: project.id, branch: "feature", path: "/tmp/f", isPrimary: false)
    #expect(WorktreeDefaults.resolveBase(project: project, worktrees: [other]) == nil)
}

@Test func resolveParentDirectoryPrefersExplicitOverride() {
    let project = Project(
        id: UUID(), name: "p", rootPath: "/tmp/projects/p",
        worktreeLocationOverride: "/custom/worktrees"
    )
    #expect(WorktreeDefaults.resolveParentDirectory(project: project) == "/custom/worktrees")
}

@Test func resolveParentDirectoryFallsBackToSiblingOfRoot() {
    let project = Project(id: UUID(), name: "p", rootPath: "/tmp/projects/p")
    #expect(WorktreeDefaults.resolveParentDirectory(project: project) == "/tmp/projects")
}
