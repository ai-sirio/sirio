import SwiftUI
import AppKit
import TillerCore

/// Menu-bar popover content: one row per worktree with an active agent,
/// sorted by the same urgency rule as the sidebar. Selecting a row reopens
/// the window if it was closed, activates the app, and jumps straight to
/// that worktree's worst-status tab.
struct AgentRosterView: View {
    let model: AppModel

    @Environment(\.openWindow) private var openWindow

    var body: some View {
        let entries = AttentionSort.sorted(model.activeAgentWorktrees, statusOf: model.statusForWorktree)
        VStack(alignment: .leading, spacing: 0) {
            if entries.isEmpty {
                Text("No active agents")
                    .foregroundStyle(.secondary)
                    .padding(12)
            } else {
                ForEach(entries) { worktree in
                    RosterRow(
                        worktree: worktree,
                        projectName: projectName(for: worktree),
                        status: model.statusForWorktree(worktree)
                    ) {
                        select(worktree)
                    }
                }
            }
            Divider()
            Button("Quit Tiller") { NSApp.terminate(nil) }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .padding(8)
        }
        .frame(minWidth: 260)
    }

    private func projectName(for worktree: Worktree) -> String {
        model.projects.first(where: { $0.id == worktree.projectId })?.name ?? ""
    }

    /// ponytail: visibility check via `NSApp.windows` rather than tracking
    /// scene state ourselves — Tiller has exactly one `WindowGroup` and no
    /// other window-producing scene, so this can't misfire on an unrelated
    /// window. Revisit if a second window-producing scene is ever added.
    private func select(_ worktree: Worktree) {
        if !NSApp.windows.contains(where: \.isVisible) {
            openWindow(id: "main")
        }
        NSApp.activate(ignoringOtherApps: true)
        model.route = .workspace
        model.selectedWorktree = worktree
        if let tab = model.worstStatusTab(in: worktree) {
            model.activeTabId[worktree.id] = tab.id
        }
    }
}

private struct RosterRow: View {
    let worktree: Worktree
    let projectName: String
    let status: AgentStatus?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Circle()
                    .fill(status?.badgeColor ?? .secondary)
                    .frame(width: 7, height: 7)
                VStack(alignment: .leading, spacing: 1) {
                    Text(worktree.branch)
                    Text(status.map { "\(projectName) — \($0.humanLabel)" } ?? projectName)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
