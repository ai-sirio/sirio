import SwiftUI
import TillerACP

/// Pending permission requests anchored above the composer (t3code's
/// ComposerPendingApprovalPanel): the first request shows its options;
/// further queued requests appear as a count.
struct ComposerApprovalPanel: View {
    let permissions: [ComposerPermission]
    let controller: ChatController

    var body: some View {
        if let current = permissions.first {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Image(systemName: "lock.shield")
                        .font(.caption)
                        .foregroundStyle(.orange)
                    Text(current.title)
                        .font(.callout.weight(.medium))
                        .lineLimit(2)
                    Spacer()
                    if permissions.count > 1 {
                        Text("+\(permissions.count - 1) more")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
                HStack(spacing: 8) {
                    ForEach(current.options, id: \.optionId) { option in
                        Button(option.name) {
                            Task {
                                await controller.answerPermission(
                                    requestId: current.requestId,
                                    optionId: option.optionId)
                            }
                        }
                        .buttonStyle(.bordered)
                        .tint(option.kind == .allowOnce || option.kind == .allowAlways
                              ? .green : .red)
                        .controlSize(.small)
                    }
                }
            }
            .padding(10)
            .background(.quaternary.opacity(0.4),
                        in: RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10)
                .strokeBorder(Color.orange.opacity(0.6), lineWidth: 1))
        }
    }
}
