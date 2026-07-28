import Foundation
import Testing
@testable import TillerCore

@Test func resolvesRelativeAbsoluteAndFileURLReferences() {
    #expect(FileLink.resolve("Sources/App.swift", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/repo/Sources/App.swift").standardizedFileURL)
    #expect(FileLink.resolve("/tmp/test.py", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/tmp/test.py").standardizedFileURL)
    #expect(FileLink.resolve("file:///tmp/data.json", worktreePath: "/repo") ==
            URL(fileURLWithPath: "/tmp/data.json").standardizedFileURL)
}

@Test func stripsTerminalLineAndColumnSuffixes() {
    #expect(FileLink.resolve("Sources/App.swift:42", worktreePath: "/repo")?.path ==
            "/repo/Sources/App.swift")
    #expect(FileLink.resolve("Sources/App.swift:42:7", worktreePath: "/repo")?.path ==
            "/repo/Sources/App.swift")
}

@Test func rejectsNonFileSchemes() {
    #expect(FileLink.resolve("https://example.com/file.swift", worktreePath: "/repo") == nil)
}
