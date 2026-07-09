import Testing
@testable import TillerCore

@Test func returnsNilForEmptyBody() {
    #expect(OllamaCloudUsageParser.extractUsage(from: "") == nil)
}

@Test func returnsNilForUnrecognizedPage() {
    #expect(OllamaCloudUsageParser.extractUsage(from: "<html><body>Settings</body></html>") == nil)
}

@Test func parsesUsagePercentWhenPresent() {
    let usage = OllamaCloudUsageParser.extractUsage(from: "usagePercent: 42")
    #expect(usage?.session?.usedPercent == 42)
}

@Test func clampsOutOfRangeUsagePercent() {
    let usage = OllamaCloudUsageParser.extractUsage(from: "usagePercent: 150")
    #expect(usage?.session?.usedPercent == 100)
}
