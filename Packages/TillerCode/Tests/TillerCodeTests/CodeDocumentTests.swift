import Foundation
import Testing
@testable import TillerCode

@MainActor
@Test func codeDocumentLoadsTracksDirtyAndSaves() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
    defer { try? FileManager.default.removeItem(at: url) }
    try "let old = 1\n".write(to: url, atomically: true, encoding: .utf8)
    let document = try CodeDocument(fileURL: url)
    #expect(document.text == "let old = 1\n")
    #expect(document.isDirty == false)
    document.text = "let new = 2\n"
    #expect(document.isDirty)
    try document.save()
    #expect(document.isDirty == false)
    #expect(try String(contentsOf: url, encoding: .utf8) == "let new = 2\n")
    document.stopWatching()
}

@MainActor
@Test func missingCodeDocumentCanBeRecreated() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("swift")
    let document = try CodeDocument(fileURL: url)
    #expect(document.fileDeleted)
    document.text = "let recreated = true\n"
    try document.save()
    defer { try? FileManager.default.removeItem(at: url) }
    #expect(document.fileDeleted == false)
    #expect(FileManager.default.fileExists(atPath: url.path))
    document.stopWatching()
}

@MainActor
@Test func nonUTF8CodeDocumentFailsToLoad() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("bin")
    defer { try? FileManager.default.removeItem(at: url) }
    try Data([0xFF, 0xFE, 0x00]).write(to: url)
    #expect(throws: (any Error).self) {
        try CodeDocument(fileURL: url)
    }
}

@MainActor
@Test func externalChangeReloadsCleanBufferButConflictsWithDirtyBuffer() throws {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString).appendingPathExtension("py")
    defer { try? FileManager.default.removeItem(at: url) }
    try "one\n".write(to: url, atomically: true, encoding: .utf8)
    let document = try CodeDocument(fileURL: url)

    try "two\n".write(to: url, atomically: true, encoding: .utf8)
    document.handleExternalChange()
    #expect(document.text == "two\n")
    #expect(document.externalChangeConflict == false)

    document.text = "local\n"
    try "remote\n".write(to: url, atomically: true, encoding: .utf8)
    document.handleExternalChange()
    #expect(document.text == "local\n")
    #expect(document.externalChangeConflict)

    document.keepLocalBuffer()
    #expect(document.externalChangeConflict == false)
    document.stopWatching()
}
