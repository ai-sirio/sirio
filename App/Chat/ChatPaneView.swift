import SwiftUI
import TillerACP
import TillerAgents
import TillerCore

enum ChatPaneLayoutRole: Hashable {
    case approvalPanel
    case composer
}

struct ChatPaneLayoutPreferenceKey: PreferenceKey {
    static let defaultValue: [ChatPaneLayoutRole: CGRect] = [:]

    static func reduce(value: inout [ChatPaneLayoutRole: CGRect],
                       nextValue: () -> [ChatPaneLayoutRole: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, new in new })
    }
}

private struct ChatPaneLayoutCaptureEnabledKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var chatPaneLayoutCaptureEnabled: Bool {
        get { self[ChatPaneLayoutCaptureEnabledKey.self] }
        set { self[ChatPaneLayoutCaptureEnabledKey.self] = newValue }
    }
}

/// A whole chat tab: transcript + composer. Agent identity/state live in
/// the window toolbar; state banners cover auth/disconnect/npx failures.
struct ChatPaneView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel
    @Environment(\.chatPaneLayoutCaptureEnabled) private var layoutCaptureEnabled

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
            captureLayout(.approvalPanel) {
                PendingQuestionBar(permissions: controller.composerPermissions,
                                   controller: controller)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .layoutPriority(1)
            }
            Divider()
            captureLayout(.composer) {
                ChatComposerView(controller: controller, worktreePath: worktree.path)
            }
        }
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        .coordinateSpace(name: "chat-pane")
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.start()
        }
    }

    @ViewBuilder
    private func captureLayout<Content: View>(_ role: ChatPaneLayoutRole,
                                              @ViewBuilder content: () -> Content) -> some View {
        if layoutCaptureEnabled {
            content()
                .background(GeometryReader { proxy in
                    Color.clear.preference(
                        key: ChatPaneLayoutPreferenceKey.self,
                        value: [role: proxy.frame(in: .named("chat-pane"))])
                })
        } else {
            content()
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
