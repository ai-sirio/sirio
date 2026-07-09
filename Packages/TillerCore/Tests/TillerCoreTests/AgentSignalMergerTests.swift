import Testing
import Foundation
@testable import TillerCore

@Test func noPriorHookAlwaysAppliesTitleSignal() {
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: nil, now: Date()) == true)
}

@Test func titleSignalDroppedWithinDebounceWindow() {
    let now = Date()
    let recentHook = now.addingTimeInterval(-0.5)
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: recentHook, now: now) == false)
}

@Test func titleSignalAppliedAfterDebounceWindowElapses() {
    let now = Date()
    let staleHook = now.addingTimeInterval(-2.0)
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: staleHook, now: now) == true)
}

@Test func exactlyAtDebounceBoundaryApplies() {
    let now = Date()
    let hookAtBoundary = now.addingTimeInterval(-AgentSignalMerger.debounceInterval)
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: hookAtBoundary, now: now) == true)
}

@Test func customDebounceIntervalIsRespected() {
    let now = Date()
    let hook = now.addingTimeInterval(-3.0)
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: hook, now: now, debounceInterval: 5.0) == false)
    #expect(AgentSignalMerger.shouldApplyTitleSignal(lastHookUpdateAt: hook, now: now, debounceInterval: 1.0) == true)
}
