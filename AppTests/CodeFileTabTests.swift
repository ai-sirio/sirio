import Foundation
import Testing
@testable import Tiller
import TillerCore
import TillerTerminal

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
        return Fixture(
            model: AppModel(paneRegistry: PaneRegistry(), registrationTimeoutMs: 100,
                            defaults: defaults),
            defaults: defaults,
            suiteName: name)
    }

    @Test func opensCodeAndMarkdownInTheirOwnTabKindsAndDeduplicates() throws {
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

        let code = try #require(model.openDocument(fileURL: swiftURL, in: worktree))
        let duplicate = model.openDocument(fileURL: swiftURL, in: worktree)
        let markdown = try #require(model.openDocument(fileURL: markdownURL, in: worktree))

        #expect(code.codeFileURL == swiftURL.standardizedFileURL)
        #expect(duplicate?.id == code.id)
        #expect(markdown.markdownFileURL == markdownURL.standardizedFileURL)
        #expect(model.workspaceTabs(for: worktree.id).count == 2)
    }

    @Test func saveActiveDocumentWritesCodeBuffer() throws {
        let fixture = makeModel()
        defer { fixture.cleanUp() }
        let model = fixture.model
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
        defer { try? FileManager.default.removeItem(at: url) }
        try "old\n".write(to: url, atomically: true, encoding: .utf8)
        let worktree = Worktree(id: UUID(), projectId: UUID(), branch: "main", path: url.deletingLastPathComponent().path)
        model.worktrees = [worktree.projectId: [worktree]]
        let tab = try #require(model.openDocument(fileURL: url, in: worktree))
        model.codeDocuments[tab.id]?.text = "new\n"
        #expect(model.isDocumentDirty(tabId: tab.id))
        model.saveActiveDocument()
        #expect(try String(contentsOf: url, encoding: .utf8) == "new\n")
        #expect(model.isDocumentDirty(tabId: tab.id) == false)
    }
}
