import Testing
@testable import TillerCore

@Test func glyphKindMapsNilToNone() {
    #expect(SidebarGlyphKind.forStatus(nil) == .none)
}

@Test func glyphKindMapsRunningToRunning() {
    #expect(SidebarGlyphKind.forStatus(.running) == .running)
}

@Test func glyphKindMapsLifecycleStatusesToDots() {
    #expect(SidebarGlyphKind.forStatus(.needsInput) == .dot(.amber))
    #expect(SidebarGlyphKind.forStatus(.done) == .dot(.green))
    #expect(SidebarGlyphKind.forStatus(.error) == .dot(.red))
}
