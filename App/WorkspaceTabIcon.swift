import SwiftUI
import TillerCore

/// Icona per tipo di tab (markdown / chat / terminale, con icona agente
/// quando un agente è stato rilevato nei pane). Condivisa tra la TabRow
/// della sidebar e la tab bar.
struct WorkspaceTabIcon: View {
    @Bindable var model: AppModel
    let tab: LegacyWorkspaceTab

    var body: some View {
        if tab.markdownFileURL != nil {
            Image(systemName: "doc.text")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        } else if tab.codeFileURL != nil {
            Image(systemName: "chevron.left.forwardslash.chevron.right")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        } else if let agentId = tab.chatAgentId {
            AgentIcon(agentId: agentId, size: 12)
        } else if let agentId = tab.leafIds.compactMap({ model.agentActivity.paneAgents[$0] }).first {
            AgentIcon(agentId: agentId, size: 12)
        } else {
            Image(systemName: "terminal")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        }
    }
}
