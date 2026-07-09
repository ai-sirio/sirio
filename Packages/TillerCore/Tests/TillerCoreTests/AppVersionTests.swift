import Testing
@testable import TillerCore

@Test func appVersionIsSemver() {
    let parts = AppVersion.current.split(separator: ".")
    #expect(parts.count == 3)
}
