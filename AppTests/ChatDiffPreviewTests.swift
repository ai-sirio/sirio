import Testing
@testable import Tiller

struct ChatDiffPreviewTests {
    @Test func keepsEveryOldAndNewLine() {
        let old = (1...55).map { "old \($0)" }.joined(separator: "\n")
        let new = (1...60).map { "new \($0)" }.joined(separator: "\n")
        let rows = ChatDiffPreviewModel.rows(oldText: old, newText: new)
        #expect(rows.filter { $0.side == .old }.count == 55)
        #expect(rows.filter { $0.side == .new }.count == 60)
        #expect(rows.count == 115)
    }

    @Test func keepsContextLinesBetweenInlineChanges() {
        let rows = ChatDiffPreviewModel.rows(
            oldText: "let prefix = \"chat\"\nlet oldTitle = legacyTitle\nlet suffix = \"view\"",
            newText: "let prefix = \"chat\"\nlet title = currentTitle\nlet suffix = \"view\"")

        #expect(rows.map(\.side) == [.context, .old, .new, .context])
        #expect(rows[1].lineNumber == 2)
        #expect(rows[2].lineNumber == 2)
    }
}
