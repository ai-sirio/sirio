import Testing
import Foundation
@testable import TillerTerminal

@Test func outputBurstCoalescesToSingleSettle() async throws {
    let collected = SettleCounter()
    let debouncer = OutputSettleDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { Task { await collected.increment() } }
    for _ in 0..<20 {
        debouncer.push()
    }
    try await Task.sleep(for: .milliseconds(250))
    #expect(await collected.count == 1)
}

@Test func separateOutputBurstsSettleSeparately() async throws {
    let collected = SettleCounter()
    let debouncer = OutputSettleDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { Task { await collected.increment() } }
    debouncer.push()
    try await Task.sleep(for: .milliseconds(150))
    debouncer.push()
    try await Task.sleep(for: .milliseconds(150))
    #expect(await collected.count == 2)
}

@Test func cancelDropsPendingOutputSettle() async throws {
    let collected = SettleCounter()
    let debouncer = OutputSettleDebouncer(interval: .milliseconds(30))
    debouncer.onSettle = { Task { await collected.increment() } }
    debouncer.push()
    debouncer.cancel()
    try await Task.sleep(for: .milliseconds(150))
    #expect(await collected.count == 0)
}

private actor SettleCounter {
    private(set) var count = 0
    func increment() { count += 1 }
}
