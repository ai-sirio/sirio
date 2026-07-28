import AppKit
import Foundation
import CodeEditLanguages
import SwiftTreeSitter

public enum TreeSitterHighlighter {
    @MainActor
    public static func highlight(
        code: String,
        language: TillerCodeLanguage,
        theme: CodeHighlightTheme,
        font: NSFont
    ) -> NSAttributedString? {
        guard let ranges = ranges(in: code, language: language) else { return nil }
        let output = NSMutableAttributedString(
            string: code,
            attributes: [.font: font, .foregroundColor: theme.text.color.nsColor])
        for range in ranges {
            guard NSMaxRange(range.range) <= output.length else { continue }
            let style = theme.style(for: range.role)
            var traits: NSFontTraitMask = []
            if style.bold { traits.insert(.boldFontMask) }
            if style.italic { traits.insert(.italicFontMask) }
            output.addAttributes(
                [
                    .foregroundColor: style.color.nsColor,
                    .font: traits.isEmpty
                        ? font
                        : NSFontManager.shared.convert(font, toHaveTrait: traits)
                ],
                range: range.range)
        }
        return output
    }

    public static func ranges(
        in code: String,
        language: TillerCodeLanguage
    ) -> [SyntaxHighlightRange]? {
        guard let parserLanguage = language.language,
              let query = TreeSitterModel.shared.query(for: language.id)
                ?? fallbackQuery(for: language, parserLanguage: parserLanguage) else { return nil }
        do {
            let parser = Parser()
            try parser.setLanguage(parserLanguage)
            guard let tree = parser.parse(code) else { return nil }
            var accepted: [NSRange: Int] = [:]
            return query.execute(in: tree)
                .resolve(with: .init(string: code))
                .flatMap(\.captures)
                .reversed()
                .compactMap { capture in
                    let range = capture.range
                    if let existing = accepted[range], existing <= capture.index { return nil }
                    guard let name = capture.name,
                          let role = role(for: name) else { return nil }
                    accepted[range] = capture.index
                    return SyntaxHighlightRange(range: range, role: role)
                }
                .sorted { $0.range.location < $1.range.location }
        } catch {
            return nil
        }
    }

    /// CodeEditLanguages 0.1.20 appends `Resources` to `Bundle.module.resourceURL`,
    /// which SwiftPM already resolves to the Resources directory. The resulting
    /// `/Resources/Resources/` makes `TreeSitterModel.shared.query(for:)` nil for every language.
    /// Repair only that segment via public `CodeLanguage.queryURL` and
    /// `SwiftTreeSitter.Query(language:url:)`; failures stay silent. Delete once fixed upstream.
    private static func fallbackQuery(
        for language: TillerCodeLanguage,
        parserLanguage: Language
    ) -> Query? {
        guard let queryURL = language.queryURL else { return nil }
        let correctedPath = queryURL.path.replacingOccurrences(
            of: "/Resources/Resources/",
            with: "/Resources/")
        guard FileManager.default.fileExists(atPath: correctedPath) else { return nil }
        return try? Query(
            language: parserLanguage,
            url: URL(fileURLWithPath: correctedPath))
    }

    private static func role(for capture: String) -> SyntaxHighlightRole? {
        let name = capture.lowercased()
        if name.hasPrefix("comment") { return .comment }
        if name.hasPrefix("string") || name.hasPrefix("character") { return .string }
        if name.hasPrefix("number") || name.hasPrefix("float") { return .number }
        if name.hasPrefix("type") { return .type }
        if name.hasPrefix("function") || name.hasPrefix("constructor") { return .function }
        if name.hasPrefix("method") { return .method }
        if name.hasPrefix("property") { return .property }
        if name.hasPrefix("parameter") { return .parameter }
        if name.hasPrefix("attribute") { return .attribute }
        if name.hasPrefix("tag") { return .tag }
        if name.hasPrefix("constant") || name.hasPrefix("boolean") { return .value }
        if name.hasPrefix("variable") { return .variable }
        if ["keyword", "conditional", "repeat", "include", "exception"].contains(
            where: { name.hasPrefix($0) }) { return .keyword }
        return nil
    }
}
