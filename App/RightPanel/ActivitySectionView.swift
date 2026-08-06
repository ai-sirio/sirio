import SwiftUI
import TillerCore
import Inject

/// A row awaiting close confirmation, held out of the row itself so the alert
/// survives the list being rebuilt underneath it.
struct PendingActivityClose: Identifiable {
    let row: ActivityRow
    var id: String { row.id }
}

/// Bottom section of the right panel: every terminal and chat tab of every open
/// worktree. Click focuses the owning tab; the trailing button closes it.
struct ActivitySectionView: View {
    @ObserveInjection private var inject

    @Bindable var appModel: AppModel
    @Binding var isExpanded: Bool
    @State private var pendingClose: PendingActivityClose?
    @AppStorage("sidebar.visible") private var sidebarVisible = true

    private var rows: [ActivityRow] { ActivityPanelModel.rows(appModel: appModel) }
    private var runningCount: Int { rows.count { $0.status == .running } }

    /// Tab ID of the active tab in the currently selected worktree.
    private var activeTabIds: Set<UUID> {
        guard let worktreeId = appModel.selectedWorktree?.id,
              let layout = appModel.workspaceCoordinator.layouts[worktreeId],
              let tabId = layout.group(layout.activeGroupID)?.activeTabID?.rawValue
        else { return [] }
        return [tabId]
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if isExpanded {
                Divider()
                content
            }
        }
        .alert(item: $pendingClose) { pending in
            Alert(
                title: Text("Close \(pending.row.title)?"),
                message: Text(
                    "The process running in \(pending.row.worktreeLabel) will be terminated."),
                primaryButton: .destructive(Text("Close")) { close(pending.row) },
                secondaryButton: .cancel(Text("Cancel")))
        }
    .enableInjection()
    }

    private var header: some View {
        Button {
            isExpanded.toggle()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text("Activity")
                    .font(.subheadline.weight(.semibold))
                Spacer()
                if runningCount > 0 {
                    Text("\(runningCount) running")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .contentShape(Rectangle())
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isExpanded ? "Collapse Activity" : "Expand Activity")
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            ContentUnavailableView(
                "No activity",
                systemImage: "bolt.slash",
                description: Text("Terminals and chats appear here as you open them."))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(rows) { row in
                        rowView(row)
                    }
                }
                .padding(6)
            }
        }
    }

    private func rowView(_ row: ActivityRow) -> some View {
        ActivityRowView(
            row: row,
            isActive: activeTabIds.contains(row.tabId),
            onFocus: { focus(row) },
            onClose: { requestClose(row) })
    }

    private func focus(_ row: ActivityRow) {
        sidebarVisible = true
        appModel.focusActivityRow(tabId: row.tabId, worktreeId: row.worktreeId)
    }

    private func requestClose(_ row: ActivityRow) {
        if row.status.requiresCloseConfirmation {
            pendingClose = PendingActivityClose(row: row)
        } else {
            close(row)
        }
    }

    private func close(_ row: ActivityRow) {
        guard let worktree = appModel.worktree(byId: row.worktreeId) else { return }
        appModel.closeTab(row.tabId, in: worktree)
    }
}

private struct ActivityRowView: View {
    let row: ActivityRow
    let isActive: Bool
    let onFocus: () -> Void
    let onClose: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            rowIcon
                .frame(width: 14, height: 14)
            VStack(alignment: .leading, spacing: 1) {
                Text(row.title)
                    .font(.callout)
                    .lineLimit(1)
                    .truncationMode(.tail)
                Text(row.worktreeLabel)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer(minLength: 4)
            statusIndicator
            Button(action: onClose) {
                Image(systemName: "xmark")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Close")
            .accessibilityLabel("Close \(row.title)")
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 4)
        .background(
            isActive ? AppTheme.selectionFill : Color.clear,
            in: RoundedRectangle(cornerRadius: 5))
        .contentShape(Rectangle())
        .onTapGesture(perform: onFocus)
    }

    @ViewBuilder
    private var rowIcon: some View {
        if let agentId = row.agentId {
            AgentIcon(agentId: agentId)
        } else {
            Image(systemName: row.kind == .chat
                  ? "bubble.left.and.text.bubble.right" : "terminal")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var statusIndicator: some View {
        switch row.status {
        case .running:
            RunningDots(color: .green, dotSize: 3)
        case .needsInput:
            Circle().fill(.yellow).frame(width: 7, height: 7)
        case .done:
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2).foregroundStyle(.secondary)
        case .error:
            Image(systemName: "xmark.circle.fill")
                .font(.caption2).foregroundStyle(.red)
        case .idle:
            Circle().strokeBorder(.secondary, lineWidth: 1).frame(width: 7, height: 7)
        }
    }
}
