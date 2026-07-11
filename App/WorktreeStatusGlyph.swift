// Tiller/App/WorktreeStatusGlyph.swift
import SwiftUI
import TillerCore

/// SwiftUI color + glow for each semantic lifecycle-dot color. Kept in the App
/// layer so TillerCore stays SwiftUI-free.
extension SidebarDotColor {
    var color: Color {
        switch self {
        case .amber: Color(red: 0.89, green: 0.63, blue: 0.03)
        case .green: Color(red: 0.25, green: 0.73, blue: 0.31)
        case .red:   Color(red: 0.94, green: 0.28, blue: 0.28)
        }
    }
    /// Amber/red get a soft glow to draw attention; done (green) stays flat.
    var glows: Bool { self != .green }
}

/// Leading status glyph for a worktree row: the animated per-agent loader while
/// running, a static lifecycle dot otherwise, or empty space (fixed width so
/// rows stay aligned).
struct WorktreeStatusGlyph: View {
    let status: AgentStatus?
    let agentId: String?

    var body: some View {
        Group {
            switch SidebarGlyphKind.forStatus(status) {
            case .none:
                Color.clear
            case .running:
                RunningDots(color: AgentIcon.color(for: agentId ?? ""))
            case .dot(let dotColor):
                Circle()
                    .fill(dotColor.color)
                    .frame(width: 8, height: 8)
                    .shadow(
                        color: dotColor.glows ? dotColor.color.opacity(0.8) : .clear,
                        radius: dotColor.glows ? 3 : 0
                    )
            }
        }
        .frame(width: 18, height: 18)
        .help(status?.humanLabel ?? "")
    }
}

#Preview {
    VStack(alignment: .leading, spacing: 8) {
        WorktreeStatusGlyph(status: nil, agentId: nil)
        WorktreeStatusGlyph(status: .running, agentId: "claude")
        WorktreeStatusGlyph(status: .running, agentId: "codex")
        WorktreeStatusGlyph(status: .needsInput, agentId: nil)
        WorktreeStatusGlyph(status: .done, agentId: nil)
        WorktreeStatusGlyph(status: .error, agentId: nil)
    }
    .padding()
}
