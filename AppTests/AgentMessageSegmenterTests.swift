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

    @Test("an insight wrapped in a single backtick span (the real Explanatory-style convention) still extracts")
    func singleBacktickInsightExtractsInnerBody() {
        let markdown = "`★ Insight ─────\nSome educational point.\n─────`"
        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments == [.insight("Some educational point.")])
    }

    @Test("a unified git diff becomes an inline diff segment")
    func unifiedDiffExtractsAsDiff() {
        let markdown = """
        Here is the proposed change.

        diff --git a/visual-review.md b/visual-review.md
        new file mode 100644
        index 0000000..4c20585
        --- /dev/null
        +++ b/visual-review.md
        @@ -0,0 +1,3 @@
        +# Visual Review Proposal
        +
        +Add a lightweight visual regression check.
        """

        let segments = AgentMessageSegmenter.segments(from: markdown)
        #expect(segments.first == .prose("Here is the proposed change.\n\n"))
        #expect(segments.contains {
            guard case .diff(let path, let oldText, let newText) = $0 else { return false }
            return path == "visual-review.md"
                && oldText == nil
                && newText == "# Visual Review Proposal\n\nAdd a lightweight visual regression check."
        })
    }
}
