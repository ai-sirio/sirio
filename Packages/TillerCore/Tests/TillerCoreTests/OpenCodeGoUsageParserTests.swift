import Testing
@testable import TillerCore

@Test func normalizesbareTokenToAuthCookie() {
    #expect(OpenCodeGoUsageParser.normalizeCookie("Fe26.2**abc123") == "auth=Fe26.2**abc123")
}

@Test func leavesAuthPrefixedCookieUnchanged() {
    #expect(OpenCodeGoUsageParser.normalizeCookie("auth=Fe26.2**abc123") == "auth=Fe26.2**abc123")
}

@Test func leavesHostAuthPrefixedCookieUnchanged() {
    #expect(OpenCodeGoUsageParser.normalizeCookie("__Host-auth=token") == "__Host-auth=token")
}

@Test func trimsWhitespaceBeforeNormalizing() {
    #expect(OpenCodeGoUsageParser.normalizeCookie("  Fe26.2**abc  ") == "auth=Fe26.2**abc")
}

@Test func extractsWorkspaceIdFromServerResponse() {
    let body = #"id: "wrk_TESTWORKSPACEID123""#
    #expect(OpenCodeGoUsageParser.extractWorkspaceId(from: body) == "wrk_TESTWORKSPACEID123")
}

@Test func returnsNilWhenNoWorkspaceIdFound() {
    #expect(OpenCodeGoUsageParser.extractWorkspaceId(from: "no workspace id here") == nil)
}

@Test func extractsSessionWeeklyAndMonthlyWindows() {
    let body = """
    $R[20]={rollingUsage:$R[21]={status:"ok",resetInSec:7200,usagePercent:30},weeklyUsage:$R[22]={status:"ok",resetInSec:259200,usagePercent:51},monthlyUsage:$R[23]={status:"ok",resetInSec:1296000,usagePercent:89}};
    """
    let usage = OpenCodeGoUsageParser.extractUsage(from: body)
    #expect(usage?.session?.usedPercent == 30)
    #expect(usage?.weekly?.usedPercent == 51)
    #expect(usage?.monthly?.usedPercent == 89)
}

@Test func missingMonthlyIsNotAnError() {
    let body = """
    rollingUsage:$R[21]={status:"ok",resetInSec:3600,usagePercent:10},weeklyUsage:$R[22]={status:"ok",resetInSec:86400,usagePercent:20}
    """
    let usage = OpenCodeGoUsageParser.extractUsage(from: body)
    #expect(usage?.session?.usedPercent == 10)
    #expect(usage?.weekly?.usedPercent == 20)
    #expect(usage?.monthly == nil)
}

@Test func skipsNullMonthlyUsageAndFindsRealDataBlock() {
    // Regression from Orca: monthlyUsage:null appears before the real
    // monthlyUsage:$R[N]={...} block. Our regex only ever matches the
    // `{...}` form, so a bare `null` occurrence is structurally invisible
    // to it and cannot be picked up by mistake.
    let body = """
    rollingUsage:$R[21]={status:"ok",resetInSec:18000,usagePercent:0},weeklyUsage:$R[22]={status:"ok",resetInSec:57781,usagePercent:51},monthlyUsage:null,timeMonthlyUsageUpdated:null,monthlyUsage:$R[28]={status:"ok",resetInSec:1214779,usagePercent:89}
    """
    let usage = OpenCodeGoUsageParser.extractUsage(from: body)
    #expect(usage?.monthly?.usedPercent == 89)
}

@Test func returnsNilWhenNoUsageDataFound() {
    #expect(OpenCodeGoUsageParser.extractUsage(from: "<html>no usage data here</html>") == nil)
}
