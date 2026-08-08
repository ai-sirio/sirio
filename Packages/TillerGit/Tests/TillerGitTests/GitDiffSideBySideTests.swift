import Testing
@testable import TillerGit

@Suite("GitDiffSideBySide")
struct GitDiffSideBySideTests {
    private func line(
        _ id: Int, _ kind: GitDiffLineKind,
        old: Int? = nil, new: Int? = nil, text: String = ""
    ) -> GitDiffLine {
        GitDiffLine(id: id, kind: kind, oldLineNumber: old, newLineNumber: new, text: text)
    }

    @Test func contextLineAppearsOnBothSides() {
        let context = line(0, .context, old: 1, new: 1, text: "unchanged")
        let rows = GitDiffSideBySide.rows(from: [context])
        #expect(rows.count == 1)
        #expect(rows[0].left == context)
        #expect(rows[0].right == context)
    }

    @Test func pairedDeletionAndAdditionShareOneRow() {
        let deletion = line(0, .deletion, old: 3, text: "old")
        let addition = line(1, .addition, new: 3, text: "new")
        let rows = GitDiffSideBySide.rows(from: [deletion, addition])
        #expect(rows.count == 1)
        #expect(rows[0].left == deletion)
        #expect(rows[0].right == addition)
    }

    @Test func unbalancedBlockLeavesEmptySide() {
        let d1 = line(0, .deletion, old: 1, text: "a")
        let d2 = line(1, .deletion, old: 2, text: "b")
        let a1 = line(2, .addition, new: 1, text: "c")
        let rows = GitDiffSideBySide.rows(from: [d1, d2, a1])
        #expect(rows.count == 2)
        #expect(rows[0].left == d1)
        #expect(rows[0].right == a1)
        #expect(rows[1].left == d2)
        #expect(rows[1].right == nil)
    }

    @Test func additionOnlyBlockLeavesLeftEmpty() {
        let a1 = line(0, .addition, new: 5, text: "added")
        let rows = GitDiffSideBySide.rows(from: [a1])
        #expect(rows.count == 1)
        #expect(rows[0].left == nil)
        #expect(rows[0].right == a1)
    }

    @Test func hunkHeaderGetsFullWidthRowAndFlushesPending() {
        let d1 = line(0, .deletion, old: 1, text: "a")
        let hunk = line(1, .hunk, text: "@@ -10,2 +10,2 @@")
        let ctx = line(2, .context, old: 10, new: 10, text: "x")
        let rows = GitDiffSideBySide.rows(from: [d1, hunk, ctx])
        #expect(rows.count == 3)
        #expect(rows[0].left == d1)
        #expect(rows[0].right == nil)
        #expect(rows[1].isHunk)
        #expect(rows[1].left == hunk)
        #expect(rows[2].left == ctx)
        #expect(rows[2].right == ctx)
    }

    @Test func metadataLinesAreSkipped() {
        let meta = line(0, .metadata, text: "diff --git a/f b/f")
        let ctx = line(1, .context, old: 1, new: 1, text: "x")
        let rows = GitDiffSideBySide.rows(from: [meta, ctx])
        #expect(rows.count == 1)
        #expect(rows[0].left == ctx)
    }

    @Test func contextBetweenChangeBlocksSeparatesPairs() {
        let d1 = line(0, .deletion, old: 1, text: "a")
        let ctx = line(1, .context, old: 2, new: 1, text: "x")
        let a1 = line(2, .addition, new: 2, text: "b")
        let rows = GitDiffSideBySide.rows(from: [d1, ctx, a1])
        #expect(rows.count == 3)
        #expect(rows[0].right == nil)
        #expect(rows[1].left == ctx)
        #expect(rows[2].left == nil)
        #expect(rows[2].right == a1)
    }

    @Test func rowIdsAreStableAndUnique() {
        let lines = [
            line(0, .deletion, old: 1, text: "a"),
            line(1, .addition, new: 1, text: "b"),
            line(2, .context, old: 2, new: 2, text: "c"),
        ]
        let rows = GitDiffSideBySide.rows(from: lines)
        #expect(Set(rows.map(\.id)).count == rows.count)
    }

    @Test func aGitFileDiffAlignsMultipleHunksAndAnEmptyDiffHasNoRows() throws {
        let diff = try GitDiff.parse(
            """
            diff --git a/file.txt b/file.txt
            --- a/file.txt
            +++ b/file.txt
            @@ -1,2 +1,2 @@
             context
            -old
            +new
            @@ -10 +10,2 @@
            -removed
            +replacement
            +inserted
            """,
            path: GitPath("file.txt"))

        let rows = GitDiffSideBySide.rows(from: diff)
        #expect(rows.count == 6)
        #expect(rows[0].isHunk)
        #expect(rows[1].left?.text == "context")
        #expect(rows[1].right?.text == "context")
        #expect(rows[2].left?.text == "old")
        #expect(rows[2].right?.text == "new")
        #expect(rows[3].isHunk)
        #expect(rows[4].left?.text == "removed")
        #expect(rows[4].right?.text == "replacement")
        #expect(rows[5].left == nil)
        #expect(rows[5].right?.text == "inserted")

        let empty = try GitDiff.parse("", path: GitPath("empty.txt"))
        #expect(GitDiffSideBySide.rows(from: empty).isEmpty)
    }
}
