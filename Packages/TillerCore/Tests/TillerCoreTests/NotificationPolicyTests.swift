import Testing
import Foundation
@testable import TillerCore

@Test func runningNeverNotifiesRegardlessOfContext() {
    for old in [AgentStatus.running, .needsInput, .done, .error] as [AgentStatus?] + [nil] {
        for active in [true, false] {
            for visible in [true, false] {
                #expect(!NotificationPolicy.shouldNotify(
                    old: old, new: .running, appActive: active, paneVisible: visible))
            }
        }
    }
}

@Test func sameStatusNeverNotifies() {
    for status in [AgentStatus.needsInput, .done, .error] {
        #expect(!NotificationPolicy.shouldNotify(
            old: status, new: status, appActive: false, paneVisible: false))
    }
}

@Test func activeAndVisibleSuppressesAllTerminalStates() {
    for status in [AgentStatus.needsInput, .done, .error] {
        #expect(!NotificationPolicy.shouldNotify(
            old: .running, new: status, appActive: true, paneVisible: true))
    }
}

@Test func activeButHiddenNotifies() {
    #expect(NotificationPolicy.shouldNotify(
        old: .running, new: .needsInput, appActive: true, paneVisible: false))
    #expect(NotificationPolicy.shouldNotify(
        old: .running, new: .done, appActive: true, paneVisible: false))
    #expect(NotificationPolicy.shouldNotify(
        old: .running, new: .error, appActive: true, paneVisible: false))
}

@Test func inactiveNotifiesRegardlessOfVisible() {
    for visible in [true, false] {
        #expect(NotificationPolicy.shouldNotify(
            old: .running, new: .needsInput, appActive: false, paneVisible: visible))
    }
}

@Test func firstTransitionFromNilNotifiesWhenBackgroundedButNotWhenWatching() {
    #expect(NotificationPolicy.shouldNotify(
        old: nil, new: .needsInput, appActive: false, paneVisible: false))
    #expect(!NotificationPolicy.shouldNotify(
        old: nil, new: .needsInput, appActive: true, paneVisible: true))
}
