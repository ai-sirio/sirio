import SwiftUI
import TillerACP
import TillerCore

/// Grouped tool work: "▸ N steps" toggle plus either the latest entry
/// (collapsed) or every entry as a full card (expanded).
struct WorkGroupView: View {
    let groupId: String
    let entries: [TimelineRow.WorkEntry]
    let isExpanded: Bool
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if entries.count > 1 {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        if isExpanded {
                            controller.expandedWorkGroups.remove(groupId)
                        } else {
                            controller.expandedWorkGroups.insert(groupId)
                        }
                    }
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "chevron.right")
                            .font(.caption2.weight(.semibold))
                            .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        Text("\(entries.count) steps").font(.caption)
                    }
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
            if isExpanded {
                ForEach(entries) { entry in
                    ToolCallCardView(item: entry.item, controller: controller,
                                     worktree: worktree, appModel: appModel)
                }
            } else {
                ForEach(entries.suffix(TimelineBuilder.maxVisibleWorkEntries)) { entry in
                    compactRow(entry)
                }
            }
        }
    }

    private func compactRow(_ entry: TimelineRow.WorkEntry) -> some View {
        HStack(spacing: 6) {
            statusGlyph(entry.item.status)
            Text(entry.label)
                .font(.callout)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
        .onTapGesture { controller.expandedWorkGroups.insert(groupId) }
    }

    @ViewBuilder
    private func statusGlyph(_ status: ToolCallStatus) -> some View {
        switch status {
        case .pending, .inProgress:
            ProgressView().controlSize(.mini)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green).font(.caption)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(.red).font(.caption)
        }
    }
}

/// A collapsed older turn: tapping re-opens it in place.
struct TurnFoldRow: View {
    let turnId: String
    let label: String
    let at: Date
    let controller: ChatController

    var body: some View {
        Button {
            withAnimation(.easeOut(duration: 0.15)) {
                _ = controller.unfoldedTurns.insert(turnId)
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "chevron.right")
                    .font(.caption2.weight(.semibold))
                Text("Turn: \(label)")
                    .font(.caption)
                    .lineLimit(1)
                Spacer()
                Text(at, format: .dateTime.hour().minute())
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .foregroundStyle(.secondary)
            .padding(.vertical, 6).padding(.horizontal, 8)
            .background(.quaternary.opacity(0.3),
                        in: RoundedRectangle(cornerRadius: 8))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}
