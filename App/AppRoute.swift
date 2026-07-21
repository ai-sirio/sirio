import Foundation

/// Top-level window route. Settings swaps the entire window, so it is a route
/// on AppModel rather than a NavigationStack push.
enum AppRoute {
    case workspace
    case settings
}

/// Categories in the settings sidebar.
enum SettingsCategory: String, CaseIterable, Identifiable {
    case aiProviders
    case agents
    case general
    case permissions
    case appearance

    var id: String { rawValue }

    var title: String {
        switch self {
        case .aiProviders: "AI Providers"
        case .agents: "Agents"
        case .general: "General"
        case .permissions: "Permissions"
        case .appearance: "Appearance"
        }
    }

    var symbol: String {
        switch self {
        case .aiProviders: "sparkles"
        case .agents: "cpu"
        case .general: "gearshape"
        case .permissions: "lock.shield"
        case .appearance: "paintbrush"
        }
    }
}
