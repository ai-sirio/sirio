import Testing
import Foundation
@testable import TillerCore

private func makeTempFile(_ content: String) throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("md-doc-test-\(UUID().uuidString).md")
    try content.write(to: url, atomically: true, encoding: .utf8)
    return url
}

@MainActor
@Test func loadsContentAndStartsClean() throws {
    let url = try makeTempFile("# Titolo\n")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    #expect(doc.text == "# Titolo\n")
    #expect(!doc.isDirty)
}

@MainActor
@Test func editingMakesDirtyAndSaveCleans() throws {
    let url = try makeTempFile("a")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "ab"
    #expect(doc.isDirty)
    try doc.save()
    #expect(!doc.isDirty)
    #expect(try String(contentsOf: url, encoding: .utf8) == "ab")
}

@MainActor
@Test func externalChangeOnCleanBufferReloads() throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    doc.handleExternalChange()
    #expect(doc.text == "v2")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func externalChangeOnDirtyBufferFlagsConflict() throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "mio"
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    doc.handleExternalChange()
    #expect(doc.text == "mio")
    #expect(doc.externalChangeConflict)
    doc.reloadFromDisk()
    #expect(doc.text == "v2")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func echoOfOwnSaveIsIgnored() throws {
    let url = try makeTempFile("a")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "b"
    try doc.save()
    doc.handleExternalChange()
    #expect(doc.text == "b")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func fileDeletedFlagsAndSaveRecreates() throws {
    let url = try makeTempFile("a")
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try FileManager.default.removeItem(at: url)
    doc.handleFileGone()
    #expect(doc.fileDeleted)
    try doc.save()
    #expect(!doc.fileDeleted)
    #expect(FileManager.default.fileExists(atPath: url.path))
    try? FileManager.default.removeItem(at: url)
}

@MainActor
@Test func watcherDeliversRealExternalWrite() async throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    for _ in 0..<40 where doc.text != "v2" {
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(doc.text == "v2")
}
