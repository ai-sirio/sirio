import SwiftUI
import TillerACP

/// A question the agent put to the user, answered in place. Unlike the old
/// composer panel this stays in the transcript, so a reopened chat still shows
/// what was asked and what was answered.
struct QuestionCardView: View {
    let question: ChatQuestion
    let controller: ChatController

    var body: some View {
        ChatCard(kind: .question, isHighlighted: !question.isAnswered) {
            VStack(alignment: .leading, spacing: 8) {
                Text(question.header)
                    .font(.callout.weight(.medium))
                if !question.prompt.isEmpty {
                    Text(question.prompt)
                        .font(.system(size: 13))
                        .textSelection(.enabled)
                }
                if let chosen = question.chosenOptionId {
                    answered(chosen)
                } else if question.isExpired {
                    Text("No answer — the turn ended")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                } else {
                    options
                }
            }
        }
    }

    private var options: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(question.options) { option in
                Button {
                    Task { await controller.answerQuestion(question, optionId: option.id) }
                } label: {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(option.label)
                        if let detail = option.detail {
                            Text(detail).font(.caption2).foregroundStyle(.secondary)
                        }
                    }
                }
                .buttonStyle(.bordered)
                .tint(option.isRejection ? AppTheme.gitConflict : AppTheme.railQuestion)
                .controlSize(.small)
            }
        }
    }

    private func answered(_ optionId: String) -> some View {
        let label = question.options.first { $0.id == optionId }?.label ?? optionId
        return HStack(spacing: 5) {
            Image(systemName: "checkmark.circle.fill")
                .font(.caption2)
                .foregroundStyle(AppTheme.railEdit)
            Text(label).font(.caption)
        }
        .foregroundStyle(.secondary)
    }
}
