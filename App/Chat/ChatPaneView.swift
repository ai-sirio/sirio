import SwiftUI
import TillerACP
import TillerAgents
import TillerCore

/// A whole chat tab: header (agent identity + state + new conversation),
/// transcript, composer. State banners cover auth/disconnect/npx failures.
struct ChatPaneView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            switch controller.state {
            case .needsAuth:
                banner(
                    "Autenticazione richiesta",
                    detail: "Esegui il login dalla CLI (es. `claude /login`) in un terminale, poi riavvia l'agente.",
                    actionTitle: "Riprova") {
                    Task { await controller.start() }
                }
            case .disconnected(let message):
                banner(
                    "Agente disconnesso",
                    detail: message ?? "Il processo è terminato.",
                    actionTitle: "Riavvia agente") {
                    Task { await controller.start() }
                }
            default:
                EmptyView()
            }
            if let promptError = controller.promptError {
                banner(
                    "Errore nel turno",
                    detail: promptError,
                    actionTitle: "OK") {
                    controller.promptError = nil
                }
            }
            if let mcpWarning = controller.mcpWarning {
                banner(
                    "Configurazione MCP",
                    detail: mcpWarning,
                    actionTitle: "OK") {
                    controller.mcpWarning = nil
                }
            }
            TranscriptView(controller: controller, worktree: worktree,
                           appModel: appModel)
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

    private var header: some View {
        HStack(spacing: 8) {
            AgentIcon(agentId: controller.agentId, size: 14)
            Text(agentDisplayName).font(.callout.weight(.semibold))
            stateChip
            Spacer()
            Button {
                controller.isFollowing.toggle()
            } label: {
                Label("Segui l'agente",
                      systemImage: controller.isFollowing ? "eye.fill" : "eye")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(controller.isFollowing ? Color.accentColor : .secondary)
            .help("Apre nel pannello di destra i file che l'agente sta modificando")
            Button {
                Task { await controller.newConversation() }
            } label: {
                Label("Nuova conversazione", systemImage: "plus.bubble")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
        }
        .padding(.horizontal, 12).padding(.vertical, 6)
    }

    private var agentDisplayName: String {
        AgentCatalog.all.first { $0.id == controller.agentId }?.displayName
            ?? controller.agentId
    }

    @ViewBuilder
    private var stateChip: some View {
        switch controller.state {
        case .connecting:
            Label("connessione…", systemImage: "circle.dotted")
                .font(.caption).foregroundStyle(.secondary)
        case .ready:
            Label("pronto", systemImage: "circle.fill")
                .font(.caption).foregroundStyle(.green)
        case .prompting:
            Label("al lavoro", systemImage: "circle.fill")
                .font(.caption).foregroundStyle(.orange)
        case .needsAuth, .disconnected, .idle:
            Label("disconnesso", systemImage: "circle")
                .font(.caption).foregroundStyle(.secondary)
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
