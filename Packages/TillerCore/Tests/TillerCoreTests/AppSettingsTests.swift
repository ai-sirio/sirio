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
