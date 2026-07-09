import Testing
import Foundation
@testable import TillerCore

@Test func parsesPrimaryAndSecondaryWindows() {
    let json = """
    {"rateLimits":{"primary":{"usedPercent":26,"windowDurationMins":300,"resetsAt":1750000000},
    "secondary":{"usedPercent":53,"windowDurationMins":10080,"resetsAt":1750600000}}}
    """
    let usage = CodexRateLimitParser.parse(resultData: Data(json.utf8))
    #expect(usage?.session?.usedPercent == 26)
    #expect(usage?.weekly?.usedPercent == 53)
    #expect(usage?.session?.resetsAt == Date(timeIntervalSince1970: 1750000000))
}

@Test func missingUsedPercentBecomesNilWindow() {
    let json = #"{"rateLimits":{"primary":{"windowDurationMins":300},"secondary":{"usedPercent":10,"windowDurationMins":10080}}}"#
    let usage = CodexRateLimitParser.parse(resultData: Data(json.utf8))
    #expect(usage?.session == nil)
    #expect(usage?.weekly?.usedPercent == 10)
}

@Test func bothWindowsMissingReturnsNil() {
    let json = #"{"rateLimits":{}}"#
    #expect(CodexRateLimitParser.parse(resultData: Data(json.utf8)) == nil)
}

@Test func malformedJsonReturnsNil() {
    #expect(CodexRateLimitParser.parse(resultData: Data("not json".utf8)) == nil)
}

@Test func windowWithoutResetsAtLeavesResetsAtNil() {
    let json = #"{"rateLimits":{"primary":{"usedPercent":10,"windowDurationMins":300}}}"#
    let usage = CodexRateLimitParser.parse(resultData: Data(json.utf8))
    #expect(usage?.session?.resetsAt == nil)
}
