import Testing
@testable import TillerACP

@Suite struct DiffStatsTests {
    @Test func countsBothSides() {
        let stats = DiffStats.counts(oldText: "a\nb", newText: "a\nb\nc")
        #expect(stats.removed == 2)
        #expect(stats.added == 3)
    }

    @Test func treatsMissingOldTextAsAPureAddition() {
        let stats = DiffStats.counts(oldText: nil, newText: "a\nb")
        #expect(stats.removed == 0)
        #expect(stats.added == 2)
    }

    @Test func emptyTextCountsAsZeroLines() {
        let stats = DiffStats.counts(oldText: "", newText: "")
        #expect(stats.removed == 0)
        #expect(stats.added == 0)
    }

    @Test func trailingNewlineDoesNotAddAPhantomLine() {
        #expect(DiffStats.counts(oldText: nil, newText: "a\n").added == 1)
    }
}
