import Testing
@testable import TillerCore

@Test func parsesSessionAndWeeklyFromCarriageReturnStream() {
    let raw = "Current session\r12% used\rResets 4:00pm\rCurrent week (all models)\r34% used\r"
    let usage = parseClaudeUsage(raw)
    #expect(usage?.session?.usedPercent == 12)
    #expect(usage?.weekly?.usedPercent == 34)
    #expect(usage?.fableWeekly == nil)
}

@Test func parsesDistinctFableWeeklyWindow() {
    let raw = """
    Current session
    8% used

    Current week (all models)
    33% used

    Current week (Fable)
    62% used
    """
    let usage = parseClaudeUsage(raw)
    #expect(usage?.session?.usedPercent == 8)
    #expect(usage?.weekly?.usedPercent == 33)
    #expect(usage?.fableWeekly?.usedPercent == 62)
}

@Test func percentLeftIsInvertedToUsed() {
    let usage = parseClaudeUsage("Current week (all models)\n84% left\n")
    #expect(usage?.weekly?.usedPercent == 16)
}

@Test func stripsAnsiBeforeParsing() {
    let raw = "\u{1B}[38;5;7mCurrent session\u{1B}[0m\n\u{1B}[1m50% used\u{1B}[0m\n"
    #expect(parseClaudeUsage(raw)?.session?.usedPercent == 50)
}

@Test func returnsNilWhenNothingParses() {
    #expect(parseClaudeUsage("") == nil)
    #expect(parseClaudeUsage("just some unrelated banner text") == nil)
}

@Test func clampsOutOfRangePercent() {
    #expect(UsageWindow(label: "5h", usedPercent: 250).usedPercent == 100)
    #expect(UsageWindow(label: "5h", usedPercent: -5).usedPercent == 0)
}

@Test func monthlyDefaultsToNilAndCountsTowardHasAny() {
    let withoutMonthly = ProviderUsage(session: nil, weekly: nil)
    #expect(withoutMonthly.monthly == nil)
    #expect(withoutMonthly.hasAny == false)

    let withMonthly = ProviderUsage(session: nil, weekly: nil, monthly: UsageWindow(label: "mo", usedPercent: 5))
    #expect(withMonthly.monthly?.usedPercent == 5)
    #expect(withMonthly.hasAny == true)
}
