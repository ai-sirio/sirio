import Foundation
import Testing
@testable import Tiller

@Suite(.serialized)
struct WorkspaceEngineGateTests {
    @Test func environmentOverrideWinsOverTheUserDefaultsValue() {
        let suiteName = "WorkspaceEngineGateTests.override.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defaults.set(false, forKey: "workspace.universalEngine")

        #expect(WorkspaceEngineGate.value(
            defaults: defaults,
            environment: ["TILLER_UNIVERSAL_WORKSPACE": "1"]))

        defaults.set(true, forKey: "workspace.universalEngine")
        #expect(!WorkspaceEngineGate.value(
            defaults: defaults,
            environment: ["TILLER_UNIVERSAL_WORKSPACE": "0"]))
    }

    @Test func userDefaultsValueIsHonoredWhenNoEnvironmentOverrideIsSet() {
        let suiteName = "WorkspaceEngineGateTests.defaults.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!

        #expect(!WorkspaceEngineGate.value(defaults: defaults, environment: [:]))

        defaults.set(true, forKey: "workspace.universalEngine")
        #expect(WorkspaceEngineGate.value(defaults: defaults, environment: [:]))
    }
}
