import Testing
import Foundation
@testable import TillerACP

@Suite struct WorktreeFileSystemTests {
    private func makeWorktree() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-fs-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    @Test func readsAndWritesInsideWorktree() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        try fs.writeTextFile(path: root.appendingPathComponent("a.txt").path, content: "hello")
        let content = try fs.readTextFile(
            path: root.appendingPathComponent("a.txt").path, line: nil, limit: nil)
        #expect(content == "hello")
    }

    @Test func readSupportsLineAndLimit() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        let path = root.appendingPathComponent("b.txt").path
        try fs.writeTextFile(path: path, content: "l1\nl2\nl3\nl4")
        #expect(try fs.readTextFile(path: path, line: 2, limit: 2) == "l2\nl3")
    }

    @Test func rejectsPathsOutsideWorktree() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        #expect(throws: WorktreeFileSystem.PathOutsideWorktree.self) {
            _ = try fs.readTextFile(path: "/etc/hosts", line: nil, limit: nil)
        }
        #expect(throws: WorktreeFileSystem.PathOutsideWorktree.self) {
            try fs.writeTextFile(path: root.path + "/../evil.txt", content: "x")
        }
    }

    @Test func writeCreatesIntermediateDirectories() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        let nested = root.appendingPathComponent("new/dir/file.txt").path
        try fs.writeTextFile(path: nested, content: "x")
        #expect(try fs.readTextFile(path: nested, line: nil, limit: nil) == "x")
    }
}
