import AppKit
import Highlightr

/// Wraps Highlightr (highlight.js via JavaScriptCore) with a cache so
/// streaming re-renders don't re-highlight unchanged blocks. `nil` means
/// "keep the monochrome fallback" — this class never surfaces an error.
@MainActor
final class CodeHighlighter {
    static let shared = CodeHighlighter()
    /// Blocks beyond this stay monochrome until complete (streaming guard).
    static let maxHighlightableLength = 20_000
    private static let maxCacheEntries = 200

    private let highlightr: Highlightr?
    private var cache: [Key: NSAttributedString] = [:]
    private var currentThemeIsDark: Bool?

    private struct Key: Hashable {
        let code: String
        let language: String
        let isDark: Bool
    }

    init() {
        highlightr = Highlightr()
    }

    func highlight(code: String, language: String, isDark: Bool) -> NSAttributedString? {
        guard let highlightr, code.count <= Self.maxHighlightableLength else { return nil }
        let key = Key(code: code, language: language, isDark: isDark)
        if let cached = cache[key] { return cached }
        if currentThemeIsDark != isDark {
            highlightr.setTheme(to: isDark ? "atom-one-dark" : "atom-one-light")
            highlightr.theme.setCodeFont(
                .monospacedSystemFont(ofSize: MarkdownAttributedStringRenderer.codeSize,
                                      weight: .regular))
            currentThemeIsDark = isDark
        }
        guard highlightr.supportedLanguages().contains(language) else { return nil }
        guard let result = highlightr.highlight(code, as: language, fastRender: true) else {
            return nil
        }
        if cache.count >= Self.maxCacheEntries { cache.removeAll() }
        cache[key] = result
        return result
    }
}
