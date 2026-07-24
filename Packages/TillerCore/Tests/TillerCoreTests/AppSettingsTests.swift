import Testing
@testable import TillerCore

@Test func clampRefreshBelowRangeClampsUp() {
    #expect(AppSettings.clampRefresh(10) == 60)
}

@Test func clampRefreshAboveRangeClampsDown() {
    #expect(AppSettings.clampRefresh(99999) == 3600)
}

@Test func clampRefreshInRangePassesThrough() {
    #expect(AppSettings.clampRefresh(300) == 300)
}

@Test func clampRefreshBoundsAreInclusive() {
    #expect(AppSettings.clampRefresh(60) == 60)
    #expect(AppSettings.clampRefresh(3600) == 3600)
}

@Test func defaultRefreshIs300() {
    #expect(AppSettings.defaultRefreshSeconds == 300)
}
@Test func controlSocketDefaultsToEnabled() {
    #expect(AppSettings.controlSocketEnabled(defaultsValue: nil, env: [:]) == true)
}

@Test func controlSocketRespectsStoredPreference() {
    #expect(AppSettings.controlSocketEnabled(defaultsValue: false, env: [:]) == false)
    #expect(AppSettings.controlSocketEnabled(defaultsValue: true, env: [:]) == true)
}

@Test func controlSocketEnvOverrideBeatsPreference() {
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": "1"]) == true)
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: true, env: ["TILLER_SOCKET_ENABLE": "off"]) == false)
}

@Test func controlSocketAcceptsBooleanSpellings() {
    for truthy in ["1", "true", "TRUE", "on", "On"] {
        #expect(AppSettings.controlSocketEnabled(
            defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": truthy]) == true)
    }
    for falsy in ["0", "false", "off", "OFF"] {
        #expect(AppSettings.controlSocketEnabled(
            defaultsValue: true, env: ["TILLER_SOCKET_ENABLE": falsy]) == false)
    }
}

@Test func controlSocketIgnoresGarbageEnvValue() {
    #expect(AppSettings.controlSocketEnabled(
        defaultsValue: false, env: ["TILLER_SOCKET_ENABLE": "maybe"]) == false)
}

@Test func rightPanelDefaultsAreStable() {
    #expect(AppSettings.defaultRightPanelVisible == false)
    #expect(AppSettings.defaultRightPanelWidth == 360)
    #expect(AppSettings.rightPanelWidthRange == 280...600)
}

@Test func rightPanelWidthClampsToSupportedRange() {
    #expect(AppSettings.clampRightPanelWidth(120) == 280)
    #expect(AppSettings.clampRightPanelWidth(420) == 420)
    #expect(AppSettings.clampRightPanelWidth(900) == 600)
}

@Test func signpostMetricsKeyIsCorrect() {
    #expect(AppSettings.signpostMetricsKey == "debug.signpostMetrics")
}
 
@Test func autoNamingEnabledKeyDefaultsToFalseWhenAbsent() {
    // The UI reads this key via @AppStorage(default: false).
    #expect(AppSettings.autoNamingEnabledKey == "autoNaming.enabled")
}

@Test func summarizerAgentIdDefaultsToClaude() {
    #expect(AppSettings.summarizerAgentId(defaultsValue: nil) == "claude")
    #expect(AppSettings.summarizerAgentId(defaultsValue: "") == "claude")
    #expect(AppSettings.summarizerAgentId(defaultsValue: "codex") == "codex")
    #expect(AppSettings.summarizerAgentIdKey == "autoNaming.summarizerAgentId")
}
