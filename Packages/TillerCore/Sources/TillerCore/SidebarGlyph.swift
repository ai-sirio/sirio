/// Semantic color of a static lifecycle dot in the sidebar. SwiftUI-free so
/// TillerCore stays independent of the UI layer; the App target maps each case
/// to a concrete Color.
public enum SidebarDotColor: Equatable, Sendable {
    case amber, green, red
}

/// What the leading status column of a worktree row should show.
/// `running` → the animated 3-dot loader (tinted per agent by the App layer);
/// `dot` → a static lifecycle dot; `none` → empty (keeps column width stable).
public enum SidebarGlyphKind: Equatable, Sendable {
    case none
    case running
    case dot(SidebarDotColor)

    public static func forStatus(_ status: AgentStatus?) -> SidebarGlyphKind {
        switch status {
        case .none: .none
        case .running: .running
        case .needsInput: .dot(.amber)
        case .done: .dot(.green)
        case .error: .dot(.red)
        }
    }
}
