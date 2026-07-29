import Testing
import Foundation
@testable import TillerCore

@Suite struct WorkspaceIDsTests {
    @Test func documentIDResolvesSymlinksAndStandardizesPath() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let real = dir.appendingPathComponent("real.md")
        try Data("x".utf8).write(to: real)
        let link = dir.appendingPathComponent("link.md")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: real)
        let worktree = UUID()

        let viaReal = DocumentID.make(worktreeID: worktree, fileURL: real)
        let viaLink = DocumentID.make(worktreeID: worktree, fileURL: link)

        #expect(viaReal == viaLink)
    }

    @Test func sameFileInDifferentWorktreesIsDistinct() {
        let url = URL(fileURLWithPath: "/tmp/a.md")
        #expect(DocumentID.make(worktreeID: UUID(), fileURL: url)
                != DocumentID.make(worktreeID: UUID(), fileURL: url))
    }

    @Test func contentIdentifierStringIsStableAcrossEncodingRoundTrip() throws {
        let ref = WorkspaceContentRef.chat(ChatContentID("session-1"))
        let data = try JSONEncoder().encode(ref)
        let decoded = try JSONDecoder().decode(WorkspaceContentRef.self, from: data)
        #expect(decoded == ref)
        #expect(decoded.contentIdentifierString == ref.contentIdentifierString)
    }
}
