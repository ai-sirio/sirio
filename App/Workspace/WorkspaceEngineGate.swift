import Foundation

enum WorkspaceEngineGate {
    private static let userDefaultsKey = "workspace.universalEngine"
    private static let environmentKey = "TILLER_UNIVERSAL_WORKSPACE"

    static var isEnabled: Bool {
        value(defaults: .standard, environment: ProcessInfo.processInfo.environment)
    }

    static func value(
        defaults: UserDefaults,
        environment: [String: String]
    ) -> Bool {
        if let override = environment[environmentKey] {
            switch override {
            case "1": return true
            case "0": return false
            default: break
            }
        }
        return defaults.object(forKey: userDefaultsKey) as? Bool ?? false
    }
}
