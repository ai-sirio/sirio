import Foundation
import TillerPersistence

/// One entry in the chat history menu.
struct ChatHistoryRow: Identifiable, Equatable {
    let id: String
    let title: String
    let agentId: String
    let lastActivityAt: Date
}

/// Turns session records into menu rows. Kept free of SwiftUI and of any
/// locale-dependent formatting so the title fallback can be tested directly;
/// the view injects the formatter.
enum ChatHistoryRows {
    static func make(sessions: [ChatSessionRecord],
                     displayName: (String) -> String,
                     timeFormatter: (Date) -> String) -> [ChatHistoryRow] {
        sessions.map { session in
            let trimmed = session.title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let title = trimmed.isEmpty
                ? "\(displayName(session.agentId)) · \(timeFormatter(session.lastActivityAt))"
                : trimmed
            return ChatHistoryRow(id: session.id, title: title,
                                  agentId: session.agentId,
                                  lastActivityAt: session.lastActivityAt)
        }
    }
}
