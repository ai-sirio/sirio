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
}
