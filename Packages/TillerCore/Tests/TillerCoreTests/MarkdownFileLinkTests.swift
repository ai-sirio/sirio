import Testing
import Foundation
@testable import TillerCore

@Test func resolvesFileURLToMarkdown() {
    let url = MarkdownFileLink.resolve("file:///tmp/wt/README.md", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/README.md"))
}

@Test func resolvesAbsolutePath() {
    let url = MarkdownFileLink.resolve("/tmp/wt/docs/piano.markdown", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/docs/piano.markdown"))
}

@Test func resolvesRelativePathAgainstWorktree() {
    let url = MarkdownFileLink.resolve("docs/piano.md", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/docs/piano.md"))
}

@Test func extensionMatchIsCaseInsensitive() {
    #expect(MarkdownFileLink.resolve("/tmp/wt/A.MD", worktreePath: "/tmp/wt") != nil)
}

@Test func rejectsNonMarkdownAndWebURLs() {
    #expect(MarkdownFileLink.resolve("https://example.com/x.md", worktreePath: "/tmp/wt") == nil)
    #expect(MarkdownFileLink.resolve("/tmp/wt/main.swift", worktreePath: "/tmp/wt") == nil)
    #expect(MarkdownFileLink.resolve("file:///tmp/wt/script.sh", worktreePath: "/tmp/wt") == nil)
}

@Test func isMarkdownChecksExtension() {
    #expect(MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.md")))
    #expect(MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.markdown")))
    #expect(!MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.txt")))
}
