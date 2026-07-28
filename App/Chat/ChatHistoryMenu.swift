import SwiftUI
import TillerCore

/// Chat history for the current worktree, opened from the icon beside the tab
/// bar "+". Delete lives in a submenu because NSMenu items take no context
/// menu on macOS — this keeps opening a chat at a single click.
struct ChatHistoryMenu: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    @State private var pendingDeletion: ChatHistoryRow?

    var body: some View {
        let rows = model.chatHistory(for: worktree)
        Menu {
            if rows.isEmpty {
                Text("No past chats")
            } else {
                ForEach(rows) { row in
                    Button {
                        model.openChatSession(sessionId: row.id, in: worktree)
                    } label: {
                        if let icon = AgentMenuIconCache.image(for: row.agentId) {
                            Label { Text(row.title) } icon: { Image(nsImage: icon) }
                        } else {
                            Text(row.title)
                        }
                    }
                }
                Divider()
                Menu("Delete") {
                    ForEach(rows) { row in
                        Button(row.title) { pendingDeletion = row }
                    }
                }
            }
        } label: {
            Image(systemName: "clock.arrow.circlepath")
                .font(.system(size: 11))
                .foregroundStyle(AppTheme.meta)
        }
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .help("Chat history")
        .alert("Delete this chat?",
               isPresented: Binding(get: { pendingDeletion != nil },
                                    set: { if !$0 { pendingDeletion = nil } })) {
            Button("Cancel", role: .cancel) { pendingDeletion = nil }
            Button("Delete", role: .destructive) {
                if let row = pendingDeletion {
                    model.deleteChatSession(sessionId: row.id, in: worktree)
                }
                pendingDeletion = nil
            }
        } message: {
            Text(pendingDeletion.map { "\"\($0.title)\" and its transcript will be removed." } ?? "")
        }
    }
}
