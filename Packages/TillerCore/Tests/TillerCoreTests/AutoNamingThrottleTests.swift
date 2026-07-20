import Testing
import Foundation
@testable import TillerCore

@Test func firstRunAlwaysAllowed() {
    let throttle = AutoNamingThrottle()
    #expect(throttle.shouldRun(transcriptLength: 50, now: Date()) == true)
}

@Test func blocksWithinMinInterval() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 400, now: now.addingTimeInterval(5)) == false)
}

@Test func blocksWhenGrowthTooSmall() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 150, now: now.addingTimeInterval(60)) == false)
}

@Test func allowsAfterIntervalAndGrowth() {
    let now = Date()
    let throttle = AutoNamingThrottle().recording(transcriptLength: 100, now: now)
    #expect(throttle.shouldRun(transcriptLength: 400, now: now.addingTimeInterval(60)) == true)
}
