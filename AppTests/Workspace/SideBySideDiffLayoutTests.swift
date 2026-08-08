import AppKit
import Testing
import TillerGit
@testable import Tiller

@Suite
struct SideBySideDiffLayoutTests {
    @Test func theLongestLineDeterminesOneSharedColumnWidth() throws {
        let longLine = String(repeating: "W", count: 120)
        let path = try GitPath("Sources/Feature.swift")
        let longestOnRight = try GitDiff.parse(
            "@@ -1 +1 @@\n-short\n+\(longLine)", path: path)
        let longestOnLeft = try GitDiff.parse(
            "@@ -1 +1 @@\n-\(longLine)\n+short", path: path)

        let rightWidth = SideBySideDiffLayout.columnWidth(for: longestOnRight)
        let leftWidth = SideBySideDiffLayout.columnWidth(for: longestOnLeft)
        let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)
        let requiredWidth = ceil(
            48 + (longLine as NSString).size(withAttributes: [.font: font]).width)

        #expect(rightWidth > SideBySideDiffLayout.minimumColumnWidth)
        #expect(rightWidth >= requiredWidth)
        #expect(leftWidth == rightWidth)
    }
}
