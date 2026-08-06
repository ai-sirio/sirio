import SwiftUI
import TillerACP
import Inject

/// A question the agent put to the user, answered in place. Unlike the old
/// composer panel this stays in the transcript, so a reopened chat still shows
/// what was asked and what was answered.
struct QuestionCardView: View {
    @ObserveInjection private var inject

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
            VStack(alignment: .leading, spacing: 10) {
                Text(question.header)
                    .font(.system(size: 15, weight: .semibold))
                    .textSelection(.enabled)
                if !question.prompt.isEmpty {
                    Text(question.prompt)
                        .font(.system(size: 14))
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
    .enableInjection()
    }

    @ViewBuilder
    private var unansweredControls: some View {
        if let input = question.textInput {
            HStack(spacing: 8) {
                TextField(input.placeholder ?? "Type an answer", text: $textAnswer)
                    .textFieldStyle(.roundedBorder)
                    .font(.system(size: 14))
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
            .controlSize(.large)
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
        VStack(alignment: .leading, spacing: 8) {
            ForEach(question.options) { option in
                Button {
                    Task { await controller.answerQuestion(question, optionId: option.id) }
                } label: {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(option.label)
                            .font(.system(size: 14))
                            .multilineTextAlignment(.leading)
                        if let detail = option.detail {
                            Text(detail).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    // Full-width rows: the options are the answer, not a
                    // toolbar, and a wrapped label needs the room.
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.vertical, 5)
                }
                .buttonStyle(.bordered)
                .tint(option.isRejection ? AppTheme.gitConflict : AppTheme.railQuestion)
                .controlSize(.large)
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
