import Testing
@testable import TillerCore

@Suite
struct WorkspacePerformanceTests {
    @Test
    func applyingASingleCommandTouchesOnlyTheAffectedGroupsAndSplits() throws {
        let layout = Fixtures.acceptanceEnvelope()
        let splitID = try #require(layout.splitIDs()[layout.splitIDs().count / 2])
        let result = WorkspaceLayoutEngine.apply(
            .setPreferredFraction(splitID, 0.65), to: layout)
        let transition = try #require(try result.get())

        #expect(transition.delta.preferredFractionChanges == [splitID: 0.65])
        #expect(transition.delta.insertedTabs.isEmpty)
        #expect(transition.delta.removedTabs.isEmpty)
        #expect(transition.delta.insertedGroups.isEmpty)
        #expect(transition.delta.removedGroups.isEmpty)
        #expect(transition.delta.insertedSplits.isEmpty)
        #expect(transition.delta.collapsedSplits.isEmpty)
        #expect(transition.layout.groups == layout.groups)
        #expect(transition.layout.allTabs == layout.allTabs)
        #expect(transition.layout.splitIDs() == layout.splitIDs())
        #expect(transition.layout.preferredFraction(for: splitID) == 0.65)
    }
}
