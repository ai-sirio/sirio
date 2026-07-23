import SwiftUI
import TillerACP
import TillerAgents
import TillerCore

/// A whole chat tab: transcript + composer. Agent identity/state live in
/// the window toolbar; state banners cover auth/disconnect/npx failures.
struct ChatPaneView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        VStack(spacing: 0) {
            switch controller.state {
            case .needsAuth:
                banner(
                    "Authentication required",
                    detail: "Log in from the CLI (e.g. `claude /login`) in a terminal, then restart the agent.",
                    actionTitle: "Retry") {
                    Task { await controller.start() }
                }
            case .disconnected(let message):
                banner(
                    "Agent disconnected",
                    detail: message ?? "The process has terminated.",
                    actionTitle: "Restart agent") {
                    Task { await controller.start() }
                }
            default:
                EmptyView()
            }
            TranscriptView(controller: controller, worktree: worktree,
                           appModel: appModel)
            if let promptError = controller.promptError {
                banner(
                    "Turn error",
                    detail: promptError,
                    actionTitle: "OK") {
                    controller.promptError = nil
                }
            }
            if let mcpWarning = controller.mcpWarning {
                banner(
                    "MCP configuration",
                    detail: mcpWarning,
                    actionTitle: "OK") {
                    controller.mcpWarning = nil
                }
            }
            if controller.hasPlanAwaitingApproval {
                banner(
                    "Plan awaiting approval",
                    detail: "Review the proposed plan in the transcript, then approve or reject it.",
                    actionTitle: "OK") {}
            }
            Divider()
            ChatComposerView(controller: controller, worktreePath: worktree.path)
        }
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.start()
        }
    }

    private func banner(_ title: String, detail: String,
                        actionTitle: String,
                        action: @escaping () -> Void) -> some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.callout.weight(.semibold))
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            Button(actionTitle, action: action).controlSize(.small)
        }
        .padding(10)
        .background(.yellow.opacity(0.12))
    }
}
