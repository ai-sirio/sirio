import Testing
import Foundation
@testable import TillerCore

@Test func agentStatusRawValuesAreStable() {
    #expect(AgentStatus.running.rawValue == "running")
    #expect(AgentStatus.needsInput.rawValue == "needs-input")
    #expect(AgentStatus.done.rawValue == "done")
    #expect(AgentStatus.error.rawValue == "error")
}

@Test func agentStatusInitFromRawRoundTrips() {
    #expect(AgentStatus(rawValue: "needs-input") == .needsInput)
    #expect(AgentStatus(rawValue: "bogus") == nil)
}

@Test func humanLabelMapsEachStatus() {
    #expect(AgentStatus.running.humanLabel == "running")
    #expect(AgentStatus.needsInput.humanLabel == "needs input")
    #expect(AgentStatus.done.humanLabel == "finished")
    #expect(AgentStatus.error.humanLabel == "failed")
}

@Test func highestPriorityReturnsErrorOverNeedsInput() {
    let statuses: [AgentStatus] = [.running, .needsInput, .error]
    #expect(AgentStatus.highestPriority(in: statuses) == .error)
}

@Test func highestPriorityReturnsNeedsInputOverRunning() {
    let statuses: [AgentStatus] = [.running, .done, .needsInput]
    #expect(AgentStatus.highestPriority(in: statuses) == .needsInput)
}

@Test func highestPriorityReturnsRunningOverDone() {
    let statuses: [AgentStatus] = [.done, .running]
    #expect(AgentStatus.highestPriority(in: statuses) == .running)
}

@Test func highestPriorityReturnsDoneWhenOnlyDone() {
    let statuses: [AgentStatus] = [.done]
    #expect(AgentStatus.highestPriority(in: statuses) == .done)
}

@Test func highestPriorityReturnsNilForEmpty() {
    #expect(AgentStatus.highestPriority(in: [AgentStatus]()) == nil)
}

@Test func priorityOrderIsStable() {
    #expect(AgentStatus.priorityOrder == [.error, .needsInput, .running, .done])
}
