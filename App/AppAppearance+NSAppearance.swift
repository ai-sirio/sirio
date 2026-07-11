import AppKit
import TillerCore

extension AppAppearance {
    /// nil means "follow macOS" (clears the app-level override).
    var nsAppearance: NSAppearance? {
        switch self {
        case .system: nil
        case .light: NSAppearance(named: .aqua)
        case .dark: NSAppearance(named: .darkAqua)
        }
    }
}
