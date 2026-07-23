import SwiftUI

typealias ChatState = ChatController.ChatState

/// Maps the chat controller state to the status dot shown on the toolbar
/// agent picker (replaces the old ChatPaneView header state chip).
func agentStatusDotColor(for state: ChatState) -> Color {
    switch state {
    case .ready: .green
    case .prompting: .orange
    case .idle, .connecting, .needsAuth, .disconnected: .gray
    }
}
