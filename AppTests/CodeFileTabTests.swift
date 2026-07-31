import Foundation
import Testing
@testable import Tiller
import TillerCore
import TillerTerminal
import TillerWorkspace

@MainActor
struct CodeFileTabTests {
    private struct Fixture {
        let model: AppModel
        let defaults: UserDefaults
        let suiteName: String

        func cleanUp() { defaults.removePersistentDomain(forName: suiteName) }
    }

    private func makeModel() -> Fixture {
        let name = "CodeFileTabTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        defaults.removePersistentDomain(forName: name)
        let workspaceCoordinator = WorkspaceCoordinator(
            persistence: CodeFileTabWorkspacePersistence(),
            registry: WorkspaceContentRegistry(),
            adapters: [.document: DocumentContentAdapter()]
        )
        return Fixture(
            model: AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
                            defaults: defaults, workspaceCoordinator: workspaceCoordinator),
            defaults: defaults,
            suiteName: name)
    }

    /// Polls layout.allTabs until it reaches `count`, mirroring the wait
    /// AppModelControlTests uses for universal-engine tab materialization
    /// (DocumentContentAdapter.prepare is plain file I/O, so this settles
    /// fast, but it is still async — fired via Task in openDocument).
    private func waitForTabs(_ model: AppModel, worktreeID: UUID, count: Int) async -> [WorkspaceTab] {
        var tabs: [WorkspaceTab] = []
        for _ in 0..<200 where tabs.count < count {
            tabs = model.workspaceCoordinator.layouts[worktreeID]?.allTabs ?? []
            if tabs.count < count { try? await Task.sleep(for: .milliseconds(10)) }
        }
        return tabs
    }

    @Test func opensCodeAndMarkdownInTheirOwnTabKindsAndDeduplicates() async throws {
        let fixture = makeModel()
        defer { fixture.cleanUp() }
        let model = fixture.model
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let swiftURL = dir.appendingPathComponent("App.swift")
        let markdownURL = dir.appendingPathComponent("README.md")
        try "let value = 1\n".write(to: swiftURL, atomically: true, encoding: .utf8)
        try "# Readme\n".write(to: markdownURL, atomically: true, encoding: .utf8)
        let worktree = Worktree(id: UUID(), projectId: UUID(), branch: "main", path: dir.path)
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        // Each open is awaited to settle before the next fires: openDocument's
        // universal-engine branch is fire-and-forget (Task { requestNewTab }),
        // and two racing inserts can hit WorkspaceCoordinator's optimistic
        // revision check and silently drop one.
        model.openDocument(fileURL: swiftURL, in: worktree)
        let tabsAfterFirst = await waitForTabs(model, worktreeID: worktree.id, count: 1)
        model.openDocument(fileURL: markdownURL, in: worktree)
        let tabs = await waitForTabs(model, worktreeID: worktree.id, count: 2)
        model.openDocument(fileURL: swiftURL, in: worktree)
        try await Task.sleep(for: .milliseconds(50))
        let tabsAfterDuplicate = model.workspaceCoordinator.layouts[worktree.id]?.allTabs ?? []

        #expect(tabsAfterFirst.count == 1)
        #expect(tabs.count == 2)
        #expect(tabsAfterDuplicate.count == 2)
        let code = try #require(tabs.first { tab in
            if case .document(_, .code) = tab.content { return true }
            return false
        })
        let markdown = try #require(tabs.first { tab in
            if case .document(_, .markdown) = tab.content { return true }
            return false
        })
        guard case .document(let codeID, _) = code.content,
              case .document(let markdownID, _) = markdown.content else {
            Issue.record("expected document content")
            return
        }
        #expect(codeID.canonicalPath == swiftURL.standardizedFileURL.path)
        #expect(markdownID.canonicalPath == markdownURL.standardizedFileURL.path)
    }

    @Test func saveActiveDocumentWritesCodeBuffer() async throws {
        let fixture = makeModel()
        defer { fixture.cleanUp() }
        let model = fixture.model
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
        defer { try? FileManager.default.removeItem(at: url) }
        try "old\n".write(to: url, atomically: true, encoding: .utf8)
        let worktree = Worktree(id: UUID(), projectId: UUID(), branch: "main", path: url.deletingLastPathComponent().path)
        model.worktrees = [worktree.projectId: [worktree]]
        await model.workspaceCoordinator.restore(worktree: worktree)

        model.openDocument(fileURL: url, in: worktree)
        let tabs = await waitForTabs(model, worktreeID: worktree.id, count: 1)
        let tab = try #require(tabs.first)
        guard let documentAdapter = model.workspaceCoordinator.adapters[.document] as? DocumentContentAdapter
        else {
            Issue.record("no document adapter")
            return
        }

        // Force the host to materialize — prepare alone does not build the
        // CodeDocument the adapter owns, makeHost does (mirrors the real
        // .chat case in AgentsPanelModel needing a hydrated tab).
        _ = documentAdapter.makeHost(tab: tab, worktree: worktree)
        let document = try #require(documentAdapter.codeDocument(for: tab.id))
        document.text = "new\n"
        #expect(documentAdapter.isDirty(tabID: tab.id))

        model.saveActiveDocument()
        #expect(try String(contentsOf: url, encoding: .utf8) == "new\n")
        #expect(documentAdapter.isDirty(tabID: tab.id) == false)
    }
}

private actor CodeFileTabWorkspacePersistence: WorkspaceLayoutPersistence {
    func restore(worktreeID: UUID) async -> RestoredWorkspace {
        RestoredWorkspace(
            layout: .empty(groupID: PaneGroupID(worktreeID)),
            tabs: [:], revision: 0, diagnostics: [])
    }

    func commitStructural(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot,
                          tabs: [WorkspaceTab],
                          terminalContents: [TerminalContentRecordValue]) async throws {}

    func checkpoint(worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) async {}
    func flush(worktreeID: UUID) async throws {}
    nonisolated func writeRecoverySidecar(
        worktreeID: UUID, revision: Int, snapshot: WorkspaceSnapshot) throws {}
    func purge(worktreeID: UUID) async throws {}
}
