import Testing
import Foundation
import os
@testable import TillerCore

@Test func beginIntervalReturnsNilWhenGateIsOff() {
    // Default state: the gate key is not set in UserDefaults, so enabled is false.
    let state = SignpostMetrics.beginInterval("test", id: .exclusive)
    #expect(state == nil)
}

@Test func phaseOneIntervalsRemainDisabledWhenGateIsOff() {
    #expect(SignpostMetrics.beginInterval("markdownRender") == nil)
    #expect(SignpostMetrics.beginInterval("streamCommit") == nil)
    #expect(SignpostMetrics.beginInterval("streamLayout") == nil)
    #expect(SignpostMetrics.beginInterval("transcriptPersist") == nil)
    #expect(SignpostMetrics.beginInterval("processScan") == nil)
    #expect(SignpostMetrics.beginInterval("panelRefresh") == nil)
    #expect(SignpostMetrics.beginInterval("bootstrapInteractive") == nil)
}

@Test func endIntervalIsNoopWhenStateIsNil() {
    // Passing nil state should not crash or throw.
    SignpostMetrics.endInterval("test", nil)
    SignpostMetrics.endInterval("test", nil, message: "hello")
}

@Test func makeSignpostIDReturnsNonExclusiveID() {
    let id = SignpostMetrics.makeSignpostID()
    // A freshly-made ID should differ from the exclusive sentinel.
    #expect(id != OSSignpostID.exclusive)
}
