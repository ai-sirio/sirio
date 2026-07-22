import AppKit
import Testing
@testable import Tiller

@MainActor
struct CodeHighlighterTests {
    @Test("swift code gets more than one foreground color")
    func highlightsSwift() {
        let result = CodeHighlighter.shared.highlight(
            code: "let x = \"hi\"", language: "swift", isDark: true)
        #expect(result != nil)
        var colors = Set<NSColor>()
        result?.enumerateAttribute(.foregroundColor,
                                   in: NSRange(location: 0, length: result?.length ?? 0)) { value, _, _ in
            if let c = value as? NSColor { colors.insert(c) }
        }
        #expect(colors.count > 1)  // keyword vs string literal
    }

    @Test("identical requests hit the cache")
    func cacheHit() {
        let a = CodeHighlighter.shared.highlight(code: "let y = 2", language: "swift", isDark: true)
        let b = CodeHighlighter.shared.highlight(code: "let y = 2", language: "swift", isDark: true)
        #expect(a != nil)
        #expect(a === b)
    }

    @Test("oversized code is not highlighted")
    func oversizedGuard() {
        let big = String(repeating: "a", count: CodeHighlighter.maxHighlightableLength + 1)
        #expect(CodeHighlighter.shared.highlight(code: big, language: "swift", isDark: true) == nil)
    }

    @Test("unknown language returns nil, never crashes")
    func unknownLanguage() {
        let result = CodeHighlighter.shared.highlight(
            code: "hello", language: "not-a-real-language-xyz", isDark: true)
        #expect(result == nil)
    }
}
