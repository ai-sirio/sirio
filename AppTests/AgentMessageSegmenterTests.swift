import Testing
@testable import Tiller

struct AgentMessageSegmenterTests {
    @Test("plain prose with no insight stays a single prose segment")
    func plainProseStaysWhole() {
        let segments = AgentMessageSegmenter.segments(from: "Just a normal reply.")
        #expect(segments == [.prose("Just a normal reply.")])
    }

    @Test("an insight block extracts to its inner body, without the star/dash decoration")
    func insightExtractsInnerBody() {
        let markdown = """
        ```
        ★ Insight ─────
        Some educational point.
        ─────
        ```
        """
        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments == [.insight("Some educational point.")])
    }

    @Test("prose before and after an insight block stays separate, in order")
    func proseInsightProseStaysOrdered() {
        let markdown = """
        Intro line.

        ```
        ★ Insight ─────
        Point.
        ─────
        ```

        Outro line.
        """
        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments == [
            .prose("Intro line.\n\n"),
            .insight("Point."),
            .prose("\n\nOutro line."),
        ])
    }

    @Test("an unterminated insight fence (mid-stream) is left as prose, not misparsed")
    func unterminatedInsightStaysProse() {
        let markdown = """
        ```
        ★ Insight ─────
        still typing
        """
        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments == [.prose(markdown)])
    }

    @Test("a regular fenced code block that isn't an insight stays prose")
    func regularCodeBlockStaysProse() {
        let markdown = """
        ```swift
        let x = 1
        ```
        """
        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments == [.prose(markdown)])
    }
}
