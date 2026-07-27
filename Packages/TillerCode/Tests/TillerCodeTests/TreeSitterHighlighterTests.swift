import AppKit
import Foundation
import Testing
@testable import TillerCode

private let fixtureTheme = CodeHighlightTheme(
    text: .init(color: .init(red: 0.8, green: 0.8, blue: 0.8)),
    roles: [
        .keyword: .init(color: .init(red: 0.8, green: 0.3, blue: 0.9), bold: true),
        .string: .init(color: .init(red: 0.4, green: 0.8, blue: 0.4)),
        .number: .init(color: .init(red: 0.9, green: 0.6, blue: 0.2)),
        .function: .init(color: .init(red: 0.3, green: 0.7, blue: 0.9)),
        .property: .init(color: .init(red: 0.3, green: 0.7, blue: 0.9))
    ])

@MainActor
private var fixtureFont: NSFont {
    NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
}

@MainActor
@Test func headlessHighlightProducesColorRunsForFourLanguages() throws {
    let fixtures = [
        ("let answer = \"forty-two\"", "/tmp/Test.swift"),
        ("def greet(name):\n    return name", "/tmp/test.py"),
        ("const answer = 42;", "/tmp/test.js"),
        ("{\"enabled\": true}", "/tmp/test.json")
    ]

    for (code, path) in fixtures {
        let output = try #require(TreeSitterHighlighter.highlight(
            code: code,
            language: CodeLanguageResolver.language(
                for: URL(fileURLWithPath: path), contents: code),
            theme: fixtureTheme,
            font: fixtureFont))
        #expect(output.length == (code as NSString).length)
        var hasSyntaxColor = false
        output.enumerateAttribute(
            .foregroundColor,
            in: NSRange(location: 0, length: output.length)) { value, _, _ in
                guard let color = value as? NSColor else { return }
                if !color.isEqual(fixtureTheme.text.color.nsColor) {
                    hasSyntaxColor = true
                }
            }
        #expect(hasSyntaxColor)
    }
}

@Test func swiftHighlightingAlsoExposesValidSemanticRanges() throws {
    let code = "let answer = \"forty-two\""
    let ranges = try #require(TreeSitterHighlighter.ranges(
        in: code,
        language: CodeLanguageResolver.language(
            for: URL(fileURLWithPath: "/tmp/Test.swift"))))
    #expect(ranges.contains { $0.role == .keyword })
    #expect(ranges.contains { $0.role == .string })
    #expect(ranges.allSatisfy { NSMaxRange($0.range) <= (code as NSString).length })
}

@MainActor
@Test func plainTextHasNoHeadlessHighlightResult() {
    #expect(TreeSitterHighlighter.highlight(
        code: "plain",
        language: CodeLanguageResolver.plainText,
        theme: fixtureTheme,
        font: fixtureFont) == nil)
}
