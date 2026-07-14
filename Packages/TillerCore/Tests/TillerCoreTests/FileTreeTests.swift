import Foundation
import Testing
@testable import TillerCore

private func makeTreeRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent("tiller-file-tree-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
}

@Test func fileTreeListsDirectoriesBeforeFilesAndExcludesDotGit() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(
        at: root.appendingPathComponent("Sources", isDirectory: true),
        withIntermediateDirectories: true)
    try FileManager.default.createDirectory(
        at: root.appendingPathComponent(".git", isDirectory: true),
        withIntermediateDirectories: true)
    try "z".write(to: root.appendingPathComponent("z.txt"), atomically: true, encoding: .utf8)
    try "a".write(to: root.appendingPathComponent("a.txt"), atomically: true, encoding: .utf8)
    try "env".write(to: root.appendingPathComponent(".env"), atomically: true, encoding: .utf8)

    let nodes = try FileTreeLoader.children(at: "", rootURL: root)

    #expect(nodes.first?.name == "Sources")
    #expect(nodes.filter { $0.kind == .file }.map(\.name) == [".env", "a.txt", "z.txt"])
    #expect(!nodes.contains { $0.name == ".git" })
}

@Test func fileTreeDoesNotTreatDirectorySymlinkAsDirectory() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let target = root.appendingPathComponent("Target", isDirectory: true)
    let link = root.appendingPathComponent("Target Link")
    try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: target)

    let node = try #require(FileTreeLoader.children(at: "", rootURL: root)
        .first { $0.name == "Target Link" })

    #expect(node.kind == .symbolicLink)
    #expect(!node.kind.isDirectory)
}

@Test func fileTreeRejectsTraversalOutsideRoot() throws {
    let root = try makeTreeRoot()
    defer { try? FileManager.default.removeItem(at: root) }

    #expect(throws: FileTreeError.pathOutsideRoot("../outside")) {
        _ = try FileTreeLoader.children(at: "../outside", rootURL: root)
    }
}
