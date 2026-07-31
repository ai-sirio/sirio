import Foundation
import Testing
@testable import TillerCore

@Suite("AgentSessionRestorePlan")
struct AgentSessionRestorePlanTests {
    private func ref(_ contentID: TerminalContentID, _ sessionRef: String) -> AgentSessionRef {
        AgentSessionRef(contentID: contentID, agentId: "claude", sessionRef: sessionRef)
    }

    /// The regression this whole phase exists to prevent: a terminal that is
    /// still in the restored layout must keep its ref. Keying refs on the live
    /// pane id (a per-relaunch ResourceGenerationID) made this impossible, so
    /// every ref looked orphaned and was deleted on launch.
    @Test func aRefWhoseTerminalIsStillPresentSurvives() {
        let alive = TerminalContentID()

        let plan = AgentSessionRestorePlan.plan(
            refs: [ref(alive, "abc")], liveContentIDs: [alive])

        #expect(plan.resumable == [ref(alive, "abc")])
        #expect(plan.prunable.isEmpty)
    }

    @Test func aRefWhoseTerminalIsGoneIsPruned() {
        let gone = TerminalContentID()

        let plan = AgentSessionRestorePlan.plan(
            refs: [ref(gone, "abc")], liveContentIDs: [TerminalContentID()])

        #expect(plan.resumable.isEmpty)
        #expect(plan.prunable == [ref(gone, "abc")])
    }

    @Test func refsAreSplitIndependently() {
        let alive = TerminalContentID()
        let gone = TerminalContentID()

        let plan = AgentSessionRestorePlan.plan(
            refs: [ref(alive, "keep"), ref(gone, "drop")], liveContentIDs: [alive])

        #expect(plan.resumable.map(\.sessionRef) == ["keep"])
        #expect(plan.prunable.map(\.sessionRef) == ["drop"])
    }

    /// A worktree with no terminals left legitimately prunes everything — but
    /// callers must only reach this with a *loaded* layout. Running it before
    /// restore populates the layout is what would destroy the user's refs.
    @Test func anEmptyLiveSetPrunesEverything() {
        let plan = AgentSessionRestorePlan.plan(
            refs: [ref(TerminalContentID(), "a"), ref(TerminalContentID(), "b")],
            liveContentIDs: [])

        #expect(plan.resumable.isEmpty)
        #expect(plan.prunable.count == 2)
    }

    @Test func noRefsProducesAnEmptyPlan() {
        let plan = AgentSessionRestorePlan.plan(refs: [], liveContentIDs: [TerminalContentID()])

        #expect(plan.resumable.isEmpty)
        #expect(plan.prunable.isEmpty)
    }
}
