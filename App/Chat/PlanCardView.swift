import SwiftUI
import TillerACP

/// The agent's proposed plan. Shows progress at a glance and collapses once
/// every entry is done, so a finished plan stops competing with live output.
struct PlanCardView: View {
    let entries: [PlanEntry]
    let approval: PermissionState?
    let controller: ChatController

    @State private var isExpanded: Bool?

    private var completedCount: Int {
        entries.filter { $0.status == "completed" }.count
    }
    private var isComplete: Bool {
        !entries.isEmpty && completedCount == entries.count
    }
    private var showsEntries: Bool { isExpanded ?? !isComplete }

    var body: some View {
        ChatCard(kind: .plan) {
            VStack(alignment: .leading, spacing: 4) {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        isExpanded = !showsEntries
                    }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "checklist")
                        Text("Plan")
                        Text("\(completedCount)/\(entries.count)")
                            .foregroundStyle(.tertiary)
                        Spacer()
                        Image(systemName: "chevron.right")
                            .font(.caption2.weight(.semibold))
                            .rotationEffect(.degrees(showsEntries ? 90 : 0))
                    }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)

                if showsEntries {
                    ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            Image(systemName: symbol(for: entry.status))
                                .foregroundStyle(entry.status == "completed"
                                                 ? AppTheme.railEdit : .secondary)
                                .font(.caption)
                            Text(entry.content).font(.callout)
                        }
                    }
                }

                if let approval, approval.isPending {
                    HStack(spacing: 8) {
                        ForEach(approval.options, id: \.optionId) { option in
                            Button(option.name) {
                                Task {
                                    await controller.answerPermission(
                                        requestId: approval.requestId,
                                        optionId: option.optionId)
                                }
                            }
                            .buttonStyle(.bordered)
                            .tint(option.kind == .allowOnce || option.kind == .allowAlways
                                  ? AppTheme.railEdit : AppTheme.gitConflict)
                            .controlSize(.small)
                        }
                    }
                    .padding(.top, 4)
                }
            }
        }
    }

    private func symbol(for status: String) -> String {
        switch status {
        case "completed": "checkmark.circle.fill"
        case "in_progress": "circle.dotted"
        default: "circle"
        }
    }
}
