import Testing
@testable import TillerCore

@Suite struct WorkspaceLayoutFuzzTests {
    @Test func tenThousandGeneratedMutationsPreserveEveryInvariant() {
        let seed: UInt64 = 0x5449_4C4C_4552
        var rng = SeededRandom(seed: seed)
        var layout = WorkspaceLayout.empty()
        var commandKinds = Set<String>()
        var moveDestinations = Set<String>()

        for step in 0..<10_000 {
            let command = CommandGenerator.next(for: layout, rng: &rng)
            switch command {
            case .insertTab:
                commandKinds.insert("insertTab")
            case .splitGroup:
                commandKinds.insert("splitGroup")
            case .moveTab(_, to: .group):
                commandKinds.insert("moveTab")
                moveDestinations.insert("group")
            case .moveTab(_, to: .newSplit):
                commandKinds.insert("moveTab")
                moveDestinations.insert("newSplit")
            case .closeTab:
                commandKinds.insert("closeTab")
            case .activateTab:
                commandKinds.insert("activateTab")
            case .activateGroup:
                commandKinds.insert("activateGroup")
            case .setPreferredFraction:
                commandKinds.insert("setPreferredFraction")
            case .updateViewState:
                commandKinds.insert("updateViewState")
            case .renameTab:
                commandKinds.insert("renameTab")
            }

            switch WorkspaceLayoutEngine.apply(command, to: layout) {
            case .success(let transition):
                layout = transition.layout
                #expect(WorkspaceLayoutInvariants.validate(layout) == nil,
                        "invariant broken at step \(step) by \(command)")
            case .failure(let error):
                Issue.record("generator emitted an illegal command at \(step): \(command) -> \(error)")
            }
        }

        #expect(commandKinds.count == 9)
        #expect(moveDestinations == ["group", "newSplit"])
        #expect(layout.groups.count <= 10_000)
    }
}
