import AppKit
import SwiftUI
import MarkdownUI
import TillerACP
import TillerCore

/// Scrolling transcript styled like a minimal chat log: tinted user bubbles on
/// the right with an avatar dot, full-width agent markdown, collapsed
/// "> Thought" rows, timestamped turn dividers. Autoscrolls while streaming.
struct TranscriptView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel
    @State private var scrollPosition = ScrollPosition(idType: String.self)

    // Stopgap: timeline-row rendering (work groups, turn folds, 700pt column)
    // is disabled — three main-thread layout storms were sampled with it
    // enabled (ScrollView re-measuring the whole lazy content per pass while
    // streaming). Rendering follows the pre-timeline flat item path until the
    // storm is isolated offline; `rowView` and the row views stay compiled
    // for that follow-up.
    var body: some View {
        let grouped = controller.grouped

        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(grouped.roots) { item in
                        itemView(item, meta: nil, grouped: grouped)
                            .padding(.top, Self.topSpacing(for: item))
                            .id(item.id)
                    }
                    if controller.state == .prompting {
                        thinkingRow
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .scrollPosition($scrollPosition)
            // Streaming growth: follow only while the user has not taken over
            // the scroll. No geometry reads — `isPositionedByUser` is the
            // scroll view's own state.
            .onChange(of: controller.streamTick) {
                guard !scrollPosition.isPositionedByUser else { return }
                scrollPosition.scrollTo(edge: .bottom)
            }
            // A new item re-pins only when the user just sent something —
            // an agent's new tool call must not yank the view while reading.
            .onChange(of: controller.items.count) {
                guard let last = controller.items.last,
                      case .userMessage = last else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    scrollPosition.scrollTo(edge: .bottom)
                }
            }
            .onChange(of: controller.scrollTarget) {
                guard let target = controller.scrollTarget else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(target, anchor: .center)
                }
                controller.scrollTarget = nil
            }
        }
    }

    /// Vertical rhythm: a new user turn gets room, cards in a run stay tight,
    /// dividers keep their own breathing space.
    private static func topSpacing(for item: TranscriptItem) -> CGFloat {
        switch item {
        case .userMessage: 20
        case .turnDivider: 14
        case .agentMessage, .thought: 12
        case .toolCall, .plan, .editSummary, .systemNotice: 6
        }
    }

    @ViewBuilder
    private func rowView(_ row: TimelineRow, grouped: ToolCallTree.Grouped) -> some View {
        switch row {
        case .message(let item, let meta):
            itemView(item, meta: meta, grouped: grouped)
        case .work(let groupId, let entries, let isExpanded):
            WorkGroupView(groupId: groupId, entries: entries,
                          isExpanded: isExpanded, controller: controller,
                          worktree: worktree, appModel: appModel)
        case .turnFold(let turnId, let label, let at):
            TurnFoldRow(turnId: turnId, label: label, at: at,
                        controller: controller)
        case .turnDivider(_, let at):
            turnDivider(at)
        case .proposedPlan(_, let entries, let approval):
            PlanCardView(entries: entries, approval: approval, controller: controller)
        case .working:
            thinkingRow
        }
    }

    @ViewBuilder
    private func itemView(_ item: TranscriptItem, meta: TimelineRow.MessageMeta?,
                          grouped: ToolCallTree.Grouped) -> some View {
        switch item {
        case .userMessage(_, let blocks):
            userBubble(blocks)
        case .agentMessage(_, let text, _):
            VStack(alignment: .leading, spacing: 4) {
                agentMessage(text)
                if let meta, meta.showsCopyButton || meta.duration != nil {
                    messageMetaRow(meta, text: text)
                }
            }
        case .thought(_, let text):
            ThoughtRow(text: text)
        case .toolCall(let toolCall):
            let question = ChatQuestion.from(toolCall)
            let subagent = SubagentTasks.info(for: toolCall)
            if let question, !question.options.isEmpty,
               !(question.isResolved && subagent != nil) {
                QuestionCardView(question: question, controller: controller)
            } else if let subagent {
                TaskCardView(info: subagent, item: toolCall,
                             children: grouped.children(of: toolCall.toolCallId),
                             controller: controller, worktree: worktree,
                             appModel: appModel)
            } else {
                ToolCallCardView(item: toolCall, controller: controller,
                                 worktree: worktree, appModel: appModel)
            }
        case .plan(_, let entries):
            PlanCardView(entries: entries,
                         approval: grouped.pendingPlanApproval,
                         controller: controller)
        case .turnDivider(_, let date):
            turnDivider(date)
        case .editSummary(_, let paths):
            EditSummaryCardView(paths: paths, worktree: worktree,
                                appModel: appModel)
        case .systemNotice(_, let text):
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.vertical, 6)
        }
    }

    // MARK: - Agent message

    /// Splits out `★ Insight ───` callouts into their own card, so they read
    /// as an aside rather than getting stuck inside the single markdown
    /// NSTextView with the rest of the reply (see `AgentMessageSegmenter`).
    private func agentMessage(_ text: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(AgentMessageSegmenter.segments(from: text).enumerated()), id: \.offset) { _, segment in
                switch segment {
                case .prose(let chunk):
                    AgentMarkdownTextView(markdown: chunk)
                case .insight(let body):
                    InsightCardView(text: body)
                }
            }
        }
    }

    // MARK: - Thinking indicator

    /// Live status while a turn is in flight. Naming the current tool call
    /// turns a mute spinner into an answer to "what is it doing?".
    private var thinkingRow: some View {
        HStack(spacing: 6) {
            RunningDots(color: AppTheme.railQuestion)
            Text(controller.currentActivity ?? "Thinking")
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
    }

    // MARK: - User bubble

    private func userBubble(_ blocks: [ContentBlock]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                userBlockView(block)
            }
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .foregroundStyle(Color.primary)
        .background(.quaternary.opacity(0.7), in: RoundedRectangle(cornerRadius: 8))
    }

    @ViewBuilder
    private func userBlockView(_ block: ContentBlock) -> some View {
        switch block {
        case .text(let text):
            Text(text).font(.system(size: 13)).textSelection(.enabled)
        case .resourceLink(_, let name):
            Label(name, systemImage: "doc")
                .font(.caption)
        case .image:
            Label("Image", systemImage: "photo")
                .font(.caption)
        case .resource, .unknown:
            EmptyView()
        }
    }

    // MARK: - Turn divider

    private func turnDivider(_ date: Date) -> some View {
        HStack(spacing: 10) {
            Rectangle().fill(.separator).frame(height: 1)
            Text(date, format: .dateTime.hour().minute())
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize()
            Rectangle().fill(.separator).frame(height: 1)
        }
        .padding(.vertical, 6)
    }

    private func messageMetaRow(_ meta: TimelineRow.MessageMeta, text: String) -> some View {
        HStack(spacing: 8) {
            if let duration = meta.duration {
                Text(Self.formatDuration(duration))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            if meta.showsCopyButton {
                Button {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(text, forType: .string)
                } label: {
                    Image(systemName: "doc.on.doc").font(.caption2)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Copy message")
            }
        }
    }

    static func formatDuration(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        if total < 60 { return "\(total)s" }
        return "\(total / 60)m \(String(format: "%02d", total % 60))s"
    }

}

/// Collapsed-by-default "> Thought" row; the chevron rotates when expanded.
private struct ThoughtRow: View {
    let text: String
    @State private var isExpanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation(.easeOut(duration: 0.12)) { isExpanded.toggle() }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "chevron.right")
                        .font(.caption2.weight(.semibold))
                        .rotationEffect(.degrees(isExpanded ? 90 : 0))
                    Text("Thought")
                        .font(.caption)
                }
                .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            if isExpanded {
                Text(text)
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .padding(.leading, 14)
            }
        }
    }
}
