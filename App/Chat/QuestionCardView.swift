import SwiftUI
import TillerACP

/// A question the agent put to the user, answered in place. Unlike the old
/// composer panel this stays in the transcript, so a reopened chat still shows
/// what was asked and what was answered.
struct QuestionCardView: View {
    let question: ChatQuestion
    let controller: ChatController
    @State private var textAnswer: String

    init(question: ChatQuestion, controller: ChatController) {
        self.question = question
        self.controller = controller
        _textAnswer = State(initialValue: question.textInput?.prefill ?? "")
    }

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
                    unansweredControls
                }
            }
        }
    }

    @ViewBuilder
    private var unansweredControls: some View {
        if let input = question.textInput {
            HStack(spacing: 6) {
                TextField(input.placeholder ?? "Type an answer", text: $textAnswer)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit { submitTextAnswer() }
                Button("Send") { submitTextAnswer() }
                    .disabled(textAnswer.trimmingCharacters(
                        in: .whitespacesAndNewlines).isEmpty)
                Button("Cancel", role: .cancel) {
                    Task {
                        await controller.answerPermission(
                            requestId: question.requestId, optionId: nil)
                    }
                }
            }
        } else {
            options
        }
    }

    private func submitTextAnswer() {
        let answer = textAnswer
        guard !answer.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        Task { await controller.answerQuestion(question, text: answer) }
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
