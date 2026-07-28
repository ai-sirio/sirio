import Foundation
import Testing
@testable import TillerCode

@Test func slicesGlobalRangesIntoOneBasedUTF16Lines() throws {
    let text = "let first = 1\nlet second = \"two\"\n"
    let map = try #require(LineHighlightMap(
        text: text,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift"))))
    #expect(map.ranges(forLine: 1).contains { $0.role == .keyword })
    #expect(map.ranges(forLine: 2).contains { $0.role == .string })
    #expect(map.ranges(forLine: 3).isEmpty)
    #expect(map.ranges(forLine: 0).isEmpty)
}

@Test func diffHighlightsUseIndependentOldAndNewDocuments() {
    let language = CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/Test.swift"))
    let highlights = DiffHighlights(
        oldText: "let old = \"before\"\n",
        newText: "let new = 42\n",
        language: language)
    #expect(highlights.old?.ranges(forLine: 1).contains { $0.role == .string } == true)
    #expect(highlights.new?.ranges(forLine: 1).contains { $0.role == .number } == true)
}

@Test func oversizedDiffSnapshotFallsBackToNoMap() {
    let text = String(repeating: "a", count: 500_001)
    let highlights = DiffHighlights(
        oldText: nil, newText: text,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift")))
    #expect(highlights.new == nil)
}
