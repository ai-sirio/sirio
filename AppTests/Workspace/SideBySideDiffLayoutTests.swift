import AppKit
import Testing
import TillerGit
@testable import Tiller

@Suite
struct SideBySideDiffLayoutTests {
    /// The column width used to come from the longest line in the diff, which
    /// meant a single long line pushed the right-hand column off screen and
    /// there were no longer two columns to compare. It comes from the viewport
    /// now, so both halves are always visible whatever the file contains.
    @Test func bothColumnsSplitTheViewportRegardlessOfLineLength() {
        let viewport: CGFloat = 1000
        let width = SideBySideDiffLayout.columnWidth(forViewport: viewport)

        // Two columns plus the divider must fit: anything wider is the old bug,
        // where the second column started past the right edge of the pane.
        #expect(width * 2 + SideBySideDiffLayout.dividerWidth <= viewport)
        // And they must actually use the pane, not collapse to a sliver.
        #expect(width * 2 > viewport / 2)
    }

    /// A viewport narrower than two minimum columns still yields a usable
    /// column rather than a negative or zero width.
    @Test func aViewportTooNarrowForTwoColumnsStillYieldsAPositiveWidth() {
        #expect(SideBySideDiffLayout.columnWidth(forViewport: 40) > 0)
        #expect(SideBySideDiffLayout.columnWidth(forViewport: 0) > 0)
    }

    /// A new or untracked file is all additions, so the left half would be
    /// entirely filler: half the pane spent showing nothing. Those render as
    /// one full-width column instead.
    @Test func aDiffWithNoDeletionsRendersAsASingleColumn() throws {
        let path = try GitPath("Sources/Feature.swift")
        let added = try GitDiff.parse("@@ -0,0 +1,2 @@\n+one\n+two", path: path)

        #expect(SideBySideDiffLayout.isSingleColumn(added))
    }

    @Test func aDiffThatDeletesAnythingKeepsBothColumns() throws {
        let path = try GitPath("Sources/Feature.swift")
        let modified = try GitDiff.parse("@@ -1 +1 @@\n-before\n+after", path: path)

        #expect(!SideBySideDiffLayout.isSingleColumn(modified))
    }
}
