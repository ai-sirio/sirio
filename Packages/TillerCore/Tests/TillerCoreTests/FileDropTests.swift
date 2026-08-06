import Foundation
import Testing
@testable import TillerCore

private func fileURL(_ path: String) -> URL { URL(fileURLWithPath: path) }

@Test func recognisesTheFiveImageTypesUnderTheSizeCap() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/a.png"), byteCount: 1_024),
        (url: fileURL("/repo/b.jpg"), byteCount: 1_024),
        (url: fileURL("/repo/c.jpeg"), byteCount: 1_024),
        (url: fileURL("/repo/d.gif"), byteCount: 1_024),
        (url: fileURL("/repo/e.webp"), byteCount: 1_024),
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo") == [
        .image(url: fileURL("/repo/a.png"), mimeType: "image/png"),
        .image(url: fileURL("/repo/b.jpg"), mimeType: "image/jpeg"),
        .image(url: fileURL("/repo/c.jpeg"), mimeType: "image/jpeg"),
        .image(url: fileURL("/repo/d.gif"), mimeType: "image/gif"),
        .image(url: fileURL("/repo/e.webp"), mimeType: "image/webp"),
    ])
}

@Test func matchesImageExtensionsCaseInsensitively() {
    let inputs: [(url: URL, byteCount: Int)] = [(url: fileURL("/repo/A.PNG"), byteCount: 10)]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.image(url: fileURL("/repo/A.PNG"), mimeType: "image/png")])
}

@Test func rejectsImagesOverTheSizeCap() {
    let tooBig = FileDrop.maxImageBytes + 1
    let inputs: [(url: URL, byteCount: Int)] = [(url: fileURL("/repo/big.png"), byteCount: tooBig)]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.rejected(url: fileURL("/repo/big.png"),
                          reason: .imageTooLarge(byteCount: tooBig))])
}

@Test func acceptsAnImageExactlyAtTheCap() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/edge.png"), byteCount: FileDrop.maxImageBytes)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.image(url: fileURL("/repo/edge.png"), mimeType: "image/png")])
}

@Test func treatsNonImagesAndFoldersAsFilesWhateverTheirSize() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/notes.txt"), byteCount: FileDrop.maxImageBytes * 3),
        (url: fileURL("/repo/Sources"), byteCount: 0),
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "notes.txt"), .file(path: "Sources")])
}

@Test func relativisesFilesInsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo/Sources/App.swift"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "Sources/App.swift")])
}

@Test func keepsAbsolutePathsForFilesOutsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/Users/me/Downloads/notes.txt"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "/Users/me/Downloads/notes.txt")])
}

@Test func doesNotTreatASiblingSharingAPrefixAsInsideTheWorktree() {
    let inputs: [(url: URL, byteCount: Int)] = [
        (url: fileURL("/repo-backup/notes.txt"), byteCount: 10)
    ]
    #expect(FileDrop.classify(inputs, worktreePath: "/repo")
            == [.file(path: "/repo-backup/notes.txt")])
}

@Test func quotesEveryTerminalPathAndJoinsThemWithSpaces() {
    #expect(FileDrop.terminalInsertion([fileURL("/tmp/a b.txt"), fileURL("/tmp/it's.txt")])
            == "'/tmp/a b.txt' '/tmp/it'\\''s.txt'")
}

@Test func neverEndsATerminalInsertionWithANewline() {
    #expect(FileDrop.terminalInsertion([fileURL("/tmp/a.txt")]).contains("\n") == false)
}

@Test func producesAnEmptyTerminalInsertionForNoURLs() {
    #expect(FileDrop.terminalInsertion([]) == "")
}
