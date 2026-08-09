import AppKit
import TillerCode

@MainActor
final class CodeHighlighter {
    static let shared = CodeHighlighter()
    static let maxHighlightableLength = 20_000
    private static let maxCacheEntries = 200

    private struct Key: Hashable {
        let code: String
        let language: String
        let isDark: Bool
        /// Part of the key, not a reason to flush: the font is baked into the
        /// attributed string, so a cached entry is only valid at its own size.
        let fontSize: CGFloat
    }
    private var cache: [Key: NSAttributedString] = [:]

    func highlight(code: String, language: String, isDark: Bool) -> NSAttributedString? {
        guard code.count <= Self.maxHighlightableLength else { return nil }
        let codeFont = AppFont.nsMono(size: MarkdownAttributedStringRenderer.codeSize)
        let key = Key(code: code, language: language, isDark: isDark,
                      fontSize: codeFont.pointSize)
        if let cached = cache[key] { return cached }
        let resolved = CodeLanguageResolver.language(forFence: language)
        guard resolved.id != .plainText,
              let attributed = TreeSitterHighlighter.highlight(
                code: code,
                language: resolved,
                theme: .tiller(isDark: isDark),
                font: codeFont) else { return nil }
        if cache.count >= Self.maxCacheEntries { cache.removeAll(keepingCapacity: true) }
        cache[key] = attributed
        return attributed
    }
}
