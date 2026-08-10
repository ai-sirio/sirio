import Testing
import Foundation
@testable import TillerACP

@Suite struct FileMentionIndexTests {
    private func makeTree() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("mention-\(UUID().uuidString)")
        for path in ["App/AppModel.swift", "App/Chat/ChatView.swift",
                     "Sources/model-utils.swift", ".git/config",
                     "node_modules/x/index.js"] {
            let url = root.appendingPathComponent(path)
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data().write(to: url)
        }
        return root
    }

    @Test func matchesSubstringCaseInsensitive() throws {
        let root = try makeTree()
        defer { try? FileManager.default.removeItem(at: root) }
        let hits = FileMentionIndex.candidates(
            worktreePath: root.path, query: "model", limit: 10)
        #expect(hits.contains("App/AppModel.swift"))
        #expect(hits.contains("Sources/model-utils.swift"))
    }

    @Test func ranksFilenamePrefixFirstAndRespectsLimit() throws {
        let root = try makeTree()
        defer { try? FileManager.default.removeItem(at: root) }
        let hits = FileMentionIndex.candidates(
            worktreePath: root.path, query: "chat", limit: 1)
        #expect(hits == ["App/Chat/ChatView.swift"])
    }

    @Test func skipsIgnoredDirectories() throws {
        let root = try makeTree()
        defer { try? FileManager.default.removeItem(at: root) }
        let hits = FileMentionIndex.candidates(
            worktreePath: root.path, query: "", limit: 100)
        #expect(!hits.contains { $0.hasPrefix(".git/") })
        #expect(!hits.contains { $0.hasPrefix("node_modules/") })
    }
}
