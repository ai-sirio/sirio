import SwiftUI
import TillerACP
import Inject

/// One line above the composer while a question is unanswered. It never
/// measures the transcript: it exists exactly while a pending request does.
struct PendingQuestionBar: View {
    @ObserveInjection private var inject

    let permissions: [ComposerPermission]
    let controller: ChatController

    var body: some View {
        Group {
            if let current = permissions.first {
                Button {
                    controller.scrollTarget = current.toolCallId
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "questionmark.circle.fill")
                            .font(.caption)
                            .foregroundStyle(AppTheme.railQuestion)
                        Text(permissions.count > 1
                             ? "\(permissions.count) questions waiting · \(current.title)"
                             : "Question waiting · \(current.title)")
                            .font(.caption)
                            .lineLimit(1)
                        Spacer()
                        Text("Show").font(.caption2).foregroundStyle(.secondary)
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 10)
                    .background(AppTheme.cardFill, in: RoundedRectangle(cornerRadius: 8))
                    .overlay(RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(AppTheme.railQuestion.opacity(0.6), lineWidth: 1))
                }
                .buttonStyle(.plain)
            }
        }
    .enableInjection()
    }
}
