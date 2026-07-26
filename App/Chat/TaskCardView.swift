import SwiftUI
import TillerACP
import TillerCore

/// A subagent spawn and the work it did. Children are collapsed by default:
/// a running task shows only its current action, so a forty-call subagent
/// stays one card tall while it streams.
struct TaskCardView: View {
    let info: SubagentTaskInfo
    let item: ToolCallItem
    let children: [ToolCallItem]
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    @State private var isExpanded = false

    private var isRunning: Bool {
        item.status == .pending || item.status == .inProgress
    }
    private var currentChild: ToolCallItem? {
        children.last { $0.status == .pending || $0.status == .inProgress }
            ?? children.last
    }

    var body: some View {
        ChatCard(kind: .task) {
            VStack(alignment: .leading, spacing: 6) {
                header
                if isExpanded {
                    ForEach(children) { child in
                        ToolCallCardView(item: child, controller: controller,
                                         worktree: worktree, appModel: appModel)
                    }
                } else if isRunning, let currentChild {
                    HStack(spacing: 6) {
                        RunningDots(color: AppTheme.railQuestion)
                        Text(currentChild.title)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(info.title)
                    .font(.callout.weight(.medium))
                    .lineLimit(isExpanded ? nil : 1)
                Text(info.subagentType)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 1)
                    .background(AppTheme.primaryPillBg,
                                in: RoundedRectangle(cornerRadius: 4))
                    .foregroundStyle(.secondary)
                Spacer()
                statusGlyph
            }
            Button {
                withAnimation(.easeOut(duration: 0.12)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    Text(children.count == 1 ? "1 tool call"
                                             : "\(children.count) tool calls")
                        .font(.caption2)
                }
                .foregroundStyle(.tertiary)
            }
            .buttonStyle(.plain)
            .opacity(children.isEmpty ? 0 : 1)
            .disabled(children.isEmpty)
        }
    }

    @ViewBuilder
    private var statusGlyph: some View {
        switch item.status {
        case .pending, .inProgress:
            ProgressView().controlSize(.small)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(AppTheme.railEdit).font(.caption)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(AppTheme.gitConflict).font(.caption)
        }
    }
}
