import Foundation

/// User-selectable app appearance. `system` follows macOS; the App target
/// maps cases to `NSAppearance` (kept out of here — TillerCore is
/// Foundation-only).
public enum AppAppearance: String, CaseIterable, Sendable {
    case system
    case light
    case dark

    public var title: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
}
