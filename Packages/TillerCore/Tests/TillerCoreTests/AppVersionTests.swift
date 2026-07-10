import Testing
@testable import TillerCore

@Test func versionReadsShortVersionFromInfo() {
    #expect(AppVersion.version(fromInfo: ["CFBundleShortVersionString": "1.2.3"]) == "1.2.3")
}

@Test func versionFallsBackWhenInfoMissing() {
    #expect(AppVersion.version(fromInfo: nil) == "0.0.0")
    #expect(AppVersion.version(fromInfo: [:]) == "0.0.0")
}
