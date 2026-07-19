import Foundation
import Testing
@testable import TillerAgents

func repositorySkill() throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }
    return try String(
        contentsOf: url.appendingPathComponent("skills/tiller/SKILL.md"),
        encoding: .utf8
    )
}

private func temporaryDirectory() throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    try FileManager.default.createDirectory(
        at: root,
        withIntermediateDirectories: true
    )
    return root
}

@Test func everyAdapterReceivesTheCanonicalSkill() throws {
    let markdown = try repositorySkill()
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }

    for adapter in AgentCatalog.all {
        let worktree = root.appendingPathComponent(adapter.id, isDirectory: true)
        try FileManager.default.createDirectory(
            at: worktree,
            withIntermediateDirectories: true
        )
        try TillerSkillProvisioner.install(
            markdown: markdown,
            agentID: adapter.id,
            worktreePath: worktree.path
        )
        let destination = try TillerSkillProvisioner.destination(
            agentID: adapter.id,
            worktreePath: worktree.path
        )
        #expect(try String(contentsOf: destination, encoding: .utf8) == markdown)

        let updated = markdown + "\nUpdated managed content.\n"
        try TillerSkillProvisioner.install(
            markdown: updated,
            agentID: adapter.id,
            worktreePath: worktree.path
        )
        #expect(try String(contentsOf: destination, encoding: .utf8) == updated)
    }
}

@Test func unmanagedSkillIsNeverOverwritten() throws {
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }
    let destination = root.appendingPathComponent(
        ".agents/skills/tiller/SKILL.md"
    )
    try FileManager.default.createDirectory(
        at: destination.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try "user content".write(to: destination, atomically: true, encoding: .utf8)

    #expect(throws: TillerSkillProvisioner.Error.unmanagedFile(destination.path)) {
        try TillerSkillProvisioner.install(
            markdown: try repositorySkill(),
            agentID: "codex",
            worktreePath: root.path
        )
    }
    #expect(try String(contentsOf: destination, encoding: .utf8) == "user content")
}

@Test func legacyMarkedSkillIsOverwritten() throws {
    // Pre-refactor installs used a different marker sentence. Files stamped
    // with it must still be recognized as Tiller-managed, or every existing
    // worktree gets permanently locked out with unmanagedFile.
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }
    let destination = root.appendingPathComponent(".claude/skills/tiller/SKILL.md")
    try FileManager.default.createDirectory(
        at: destination.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    let legacy = """
    <!-- Machine-managed by Tiller (ClaudeCodeAdapter). Overwritten on agent pane
         setup — do not hand-edit. -->
    old content
    """
    try legacy.write(to: destination, atomically: true, encoding: .utf8)

    let markdown = try repositorySkill()
    try TillerSkillProvisioner.install(
        markdown: markdown,
        agentID: "claude",
        worktreePath: root.path
    )
    #expect(try String(contentsOf: destination, encoding: .utf8) == markdown)
}

@Test func unmarkedInputIsRejected() {
    #expect(throws: TillerSkillProvisioner.Error.missingMarker) {
        try TillerSkillProvisioner.install(
            markdown: "unsafe",
            agentID: "pi",
            worktreePath: "/tmp/worktree"
        )
    }
}

@Test func unsupportedAgentIsRejected() {
    #expect(throws: TillerSkillProvisioner.Error.unsupportedAgent("unknown")) {
        _ = try TillerSkillProvisioner.destination(
            agentID: "unknown",
            worktreePath: "/tmp/worktree"
        )
    }
}

@Test func preservingPrepareOverloadProvisionsBeforeAdapterHooks() throws {
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }
    let markdown = try repositorySkill()

    try ClaudeCodeAdapter().prepare(
        worktreePath: root.path,
        paneId: UUID(),
        tillerctlPath: "/usr/local/bin/tillerctl",
        skillMarkdown: markdown
    )

    #expect(FileManager.default.fileExists(
        atPath: root.appendingPathComponent(".claude/skills/tiller/SKILL.md").path
    ))
    #expect(FileManager.default.fileExists(
        atPath: root.appendingPathComponent(".claude/settings.local.json").path
    ))
}

@Test func threeArgumentPrepareDoesNotProvisionTheSkill() throws {
    let root = try temporaryDirectory()
    defer { try? FileManager.default.removeItem(at: root) }

    try OhMyPiAdapter().prepare(
        worktreePath: root.path,
        paneId: UUID(),
        tillerctlPath: "/usr/local/bin/tillerctl"
    )

    #expect(FileManager.default.fileExists(
        atPath: root.appendingPathComponent(".tiller/omp-hook.ts").path
    ))
    #expect(!FileManager.default.fileExists(
        atPath: root.appendingPathComponent(".agents/skills/tiller/SKILL.md").path
    ))
}
