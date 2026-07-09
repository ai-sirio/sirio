import Testing
import Foundation
@testable import TillerCore

@Test func projectDefaultsHaveNoCustomizationApplied() {
    let project = Project(id: UUID(), name: "demo", rootPath: "/tmp/demo")
    #expect(project.iconKind == .icon)
    #expect(project.displayName == nil)
    #expect(project.iconValue == nil)
    #expect(project.avatarImage == nil)
    #expect(project.defaultWorktreeBase == nil)
    #expect(project.worktreeLocationOverride == nil)
}

@Test func projectAcceptsExplicitCustomization() {
    let project = Project(
        id: UUID(), name: "demo", rootPath: "/tmp/demo",
        displayName: "Demo", iconKind: .emoji, iconValue: "🚀",
        defaultWorktreeBase: "develop", worktreeLocationOverride: "/tmp/worktrees"
    )
    #expect(project.displayName == "Demo")
    #expect(project.iconKind == .emoji)
    #expect(project.iconValue == "🚀")
    #expect(project.defaultWorktreeBase == "develop")
    #expect(project.worktreeLocationOverride == "/tmp/worktrees")
}
