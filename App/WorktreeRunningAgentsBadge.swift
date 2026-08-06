import SwiftUI
import Inject

/// Trailing worktree-row indicator: one small per-agent icon for each
/// distinct agent currently .running in the worktree. Purely additive to
/// the existing leading WorktreeStatusGlyph (generic lifecycle dot/loader)
/// — this only ever shows agents in the .running state, not done/error/
/// needs-input.
struct WorktreeRunningAgentsBadge: View {
    @ObserveInjection private var inject

    let agentIds: [String]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(agentIds, id: \.self) { AgentIcon(agentId: $0, size: 12) }
        }
    .enableInjection()
    }
}

#Preview {
    VStack(spacing: 8) {
        WorktreeRunningAgentsBadge(agentIds: ["claude"])
        WorktreeRunningAgentsBadge(agentIds: ["claude", "codex"])
        WorktreeRunningAgentsBadge(agentIds: ["claude", "codex", "opencode"])
    }
    .padding()
}
