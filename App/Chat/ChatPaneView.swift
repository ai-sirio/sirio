import SwiftUI
import AppKit
import TillerACP
import TillerAgents
import TillerCore
import UniformTypeIdentifiers
import Inject

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
    @ObserveInjection private var inject

    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel
    /// Owned here rather than inside the composer because the whole pane is
    /// the drop target, and a drop has to reach the draft.
    @State private var document = ComposerDocument()
    @State private var isDropTargeted = false
    @Environment(\.chatPaneLayoutCaptureEnabled) private var layoutCaptureEnabled

    var body: some View {
        let snapshot = controller.presentationSnapshot
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
            if snapshot.hasPlanAwaitingApproval {
                banner(
                    "Plan awaiting approval",
                    detail: "Review the proposed plan in the transcript, then approve or reject it.",
                    actionTitle: "OK") {}
            }
            captureLayout(.approvalPanel) {
                PendingQuestionBar(permissions: snapshot.composerPermissions,
                                   controller: controller)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .layoutPriority(1)
            }
            Divider()
            captureLayout(.composer) {
                ChatComposerView(controller: controller, worktreePath: worktree.path,
                                 document: document)
            }
        }
        .background { MainSurfaceMaterial(tint: AppTheme.chatSurface) }
        .onDrop(of: [.fileURL], isTargeted: $isDropTargeted) { providers in
            handleDrop(providers)
        }
        .overlay {
            if isDropTargeted && canAcceptDrop {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(Color.accentColor,
                                  style: StrokeStyle(lineWidth: 2, dash: [6, 4]))
                    .background(Color.accentColor.opacity(0.06),
                                in: RoundedRectangle(cornerRadius: 12))
                    .overlay {
                        Text("Drop files to attach")
                            .font(.callout.weight(.medium))
                            .foregroundStyle(AppTheme.title)
                    }
                    .padding(8)
                    .allowsHitTesting(false)
            }
        }
        .coordinateSpace(name: "chat-pane")
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.activate()
        }
    .enableInjection()
    }

    /// Drops follow the same rule as typing: a composer that cannot accept
    /// input must not quietly accumulate chips while a permission prompt is
    /// waiting.
    private var canAcceptDrop: Bool {
        (controller.state == .ready || controller.state == .prompting
            || controller.state == .detached)
            && !controller.presentationSnapshot.hasPendingPermission
    }

    private func handleDrop(_ providers: [NSItemProvider]) -> Bool {
        guard canAcceptDrop else { return false }
        let worktreePath = worktree.path
        // NSItemProvider supports asynchronous loading but is not Sendable.
        // This local binding is only read by the loader task below.
        nonisolated(unsafe) let providers = providers
        Task { @MainActor in
            let urls = await DroppedFileLoader.urls(from: providers)
            guard !urls.isEmpty else { return }
            let inputs = urls.map { (url: $0, byteCount: DroppedFileLoader.byteCount(of: $0)) }
            let items = FileDrop.classify(inputs, worktreePath: worktreePath)
            for message in ComposerDropApplier.apply(items, to: document) {
                appModel.showTransientMessage(message)
            }
        }
        return true
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
