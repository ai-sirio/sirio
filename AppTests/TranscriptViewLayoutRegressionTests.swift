import Foundation
import Testing

struct TranscriptViewLayoutRegressionTests {
    @Test("transcript auto-follow does not observe live scroll geometry")
    func autoFollowAvoidsScrollGeometryFeedback() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let transcriptViewURL = repositoryRoot
            .appendingPathComponent("App/Chat/TranscriptView.swift")
        let source = try String(contentsOf: transcriptViewURL, encoding: .utf8)

        #expect(
            !source.contains(".onScrollGeometryChange("),
            "Reading scroll geometry while LazyVStack is laying out can create an AttributeGraph feedback loop"
        )
    }

    @Test("transcript bottom clearance does not observe live scroll geometry")
    func bottomClearanceAvoidsScrollGeometryFeedback() throws {
        let repositoryRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let source = try String(
            contentsOf: repositoryRoot.appendingPathComponent("App/Chat/TranscriptView.swift"),
            encoding: .utf8
        )
        #expect(source.contains("bottomContentInset"))
        #expect(!source.contains(".onScrollGeometryChange("))
    }
}
