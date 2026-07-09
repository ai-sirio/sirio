import Testing
@testable import TillerCore

private let sample = ProviderUsage(
    session: UsageWindow(label: "5h", usedPercent: 10),
    weekly: nil, fableWeekly: nil)

@Test func successBecomesLoaded() {
    let s = UsageStateReducer.reduce(outcome: .success(sample), previous: .loading)
    #expect(s == .loaded(sample))
}

@Test func timeoutWithPriorValueBecomesStale() {
    let s = UsageStateReducer.reduce(outcome: .timedOut, previous: .loaded(sample))
    #expect(s == .stale(sample))
}

@Test func timeoutStaleFromStaleKeepsStale() {
    let s = UsageStateReducer.reduce(outcome: .timedOut, previous: .stale(sample))
    #expect(s == .stale(sample))
}

@Test func timeoutWithoutPriorValueBecomesUnavailable() {
    let s = UsageStateReducer.reduce(outcome: .timedOut, previous: .loading)
    #expect(s == .unavailable(.timedOut))
}

@Test func unavailableOutcomePassesThrough() {
    let s = UsageStateReducer.reduce(outcome: .unavailable(.notInstalled), previous: .loaded(sample))
    #expect(s == .unavailable(.notInstalled))
}
