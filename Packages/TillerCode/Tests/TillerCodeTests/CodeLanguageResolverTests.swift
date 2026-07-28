import Foundation
import Testing
import CodeEditLanguages
@testable import TillerCode

@Test func detectsLanguagesFromFileNamesAndShebangs() {
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/App.swift")).id == .swift)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/Dockerfile")).id == .dockerfile)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/script"),
        contents: "#!/usr/bin/env python3\nprint('ok')").id == .python)
    #expect(CodeLanguageResolver.language(
        for: URL(fileURLWithPath: "/tmp/notes.unknown")).id == .plainText)
}

@Test func resolvesMarkdownFenceAliases() {
    #expect(CodeLanguageResolver.language(forFence: "js").id == .javascript)
    #expect(CodeLanguageResolver.language(forFence: "typescript").id == .typescript)
    #expect(CodeLanguageResolver.language(forFence: "py").id == .python)
    #expect(CodeLanguageResolver.language(forFence: "sh").id == .bash)
    #expect(CodeLanguageResolver.language(forFence: "text").id == .plainText)
}
