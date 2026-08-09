import SwiftUI
import MarkdownUI
import TillerACP
import TillerCore
import Inject

/// Scrolling transcript styled like a minimal chat log: tinted user bubbles on
/// the right with an avatar dot, full-width agent markdown, collapsed
/// "> Thought" rows, timestamped turn dividers. Autoscrolls while streaming.
struct TranscriptView: View {
    @ObserveInjection private var inject

    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel
    var bottomContentInset: CGFloat = 0
    @State private var scrollPosition = ScrollPosition(idType: String.self)
    @Environment(\.chatPaneLayoutCaptureEnabled) private var layoutCaptureEnabled

    // Stopgap: timeline-row rendering (work groups, turn folds, 700pt column)
    // is disabled — three main-thread layout storms were sampled with it
    // enabled (ScrollView re-measuring the whole lazy content per pass while
    // streaming). Rendering follows the pre-timeline flat item path until the
    // storm is isolated offline; `rowView` and the row views stay compiled
    // for that follow-up.
    var body: some View {
        let snapshot = controller.presentationSnapshot
        let grouped = snapshot.grouped

        ScrollViewReader { proxy in
            ScrollView {
                CenteredComposerLayout {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(grouped.roots) { item in
                            itemView(item, meta: nil, grouped: grouped, snapshot: snapshot)
                                .padding(.top, Self.topSpacing(for: item))
                                .id(item.id)
                        }
                        if controller.state == .prompting {
                            ThinkingRowView(
                                title: snapshot.currentActivity ?? "Thinking",
                                detail: latestThought(in: snapshot.items),
                                initiallyExpanded: true)
                                .padding(.top, 10)
                        }
                        Color.clear
                            .frame(height: max(1, bottomContentInset))
                            .id("bottom")
                            .captureLayout(.transcriptBottomSpacer,
                                           enabled: layoutCaptureEnabled)
                    }
                    .padding(.vertical, 14)
                    .captureLayout(.transcriptContent, enabled: layoutCaptureEnabled)
                }
            }
            .scrollPosition($scrollPosition)
            // Do not observe live scroll geometry here: the observation reads
            // the LazyVStack geometry while that same stack is laying out,
            // which can create an AttributeGraph invalidation loop. Rely on
            // ScrollPosition's user-ownership signal instead.
            .onChange(of: controller.streamTick) {
                guard !scrollPosition.isPositionedByUser else { return }
                scrollPosition.scrollTo(edge: .bottom)
            }
            .onChange(of: bottomContentInset) {
                guard !scrollPosition.isPositionedByUser else { return }
                pinToBottom()
            }
            // A new item re-pins whenever the user is still pinned at the
            // bottom — a freshly created message, question card, or finished
            // turn scrolls into view. The user's own send re-pins even after
            // scrolling up; agent-created content never yanks a reader.
            .onChange(of: snapshot.items.count) {
                guard let last = snapshot.items.last else { return }
                if case .userMessage = last {
                    pinToBottom()
                } else if !scrollPosition.isPositionedByUser {
                    pinToBottom()
                }
            }
            .onChange(of: controller.scrollTarget) {
                guard let target = controller.scrollTarget else { return }
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo(target, anchor: .center)
                }
                controller.scrollTarget = nil
            }
            .captureLayout(.transcriptViewport, enabled: layoutCaptureEnabled)
        }
        .enableInjection()
    }

    /// Re-pins the transcript to the bottom with the same short ease used
    /// for user-sent re-pins.
    private func pinToBottom() {
        withAnimation(.easeOut(duration: 0.15)) {
            scrollPosition.scrollTo(edge: .bottom)
        }
    }

    /// Vertical rhythm: a new user turn gets room, cards in a run stay tight,
    /// dividers keep their own breathing space.
    private static func topSpacing(for item: TranscriptItem) -> CGFloat {
        switch item {
        case .userMessage: 20
        case .turnDivider: 14
        case .agentMessage, .thought: 12
        // A question is addressed to the reader, not a step in a run of tool
        // calls — it gets the room a turn boundary gets.
        case .toolCall(let call):
            ChatQuestion.from(call)?.hasControls == true ? 16 : 6
        case .plan, .editSummary, .systemNotice: 6
        }
    }

    @ViewBuilder
    private func rowView(_ row: TimelineRow, grouped: ToolCallTree.Grouped) -> some View {
        switch row {
        case .message(let item, let meta):
            itemView(item, meta: meta, grouped: grouped,
                     snapshot: controller.presentationSnapshot)
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
            ThinkingRowView(title: controller.presentationSnapshot.currentActivity ?? "Thinking",
                            detail: latestThought(in: controller.presentationSnapshot.items),
                            initiallyExpanded: true)
        }
    }

    @ViewBuilder
    private func itemView(_ item: TranscriptItem, meta: TimelineRow.MessageMeta?,
                          grouped: ToolCallTree.Grouped,
                          snapshot: ChatPresentationSnapshot) -> some View {
        switch item {
        case .userMessage(_, let blocks):
            userBubble(blocks)
        case .agentMessage(let id, let text, let isComplete):
            switch AgentMessagePresentation.mode(isComplete: isComplete) {
            case .streaming:
                MessageRowView(timestamp: nil, duration: nil,
                               showsCopyButton: false, copyText: text) {
                    StreamingAgentTextView(text: text)
                }
            case .rich:
                let timestamp = meta?.at ?? messageTimestamp(for: id,
                                                              in: snapshot.items)
                MessageRowView(timestamp: timestamp, duration: meta?.duration,
                               showsCopyButton: meta?.showsCopyButton
                                   ?? (isComplete && timestamp != nil),
                               copyText: text) {
                    agentMessage(id: id, snapshot: snapshot)
                }
            }
        case .thought(_, let text):
            ThoughtRowView(text: text)
        case .toolCall(let toolCall):
            let question = ChatQuestion.from(toolCall)
            let subagent = SubagentTasks.info(for: toolCall)
            if let question, question.hasControls,
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
            ChatRowSurface(kind: .system, isFlat: true) {
                Label(text, systemImage: "info.circle")
                    .font(AppFont.caption)
                    .foregroundStyle(AppTheme.meta)
            }
        }
    }

    // MARK: - Agent message

    /// Splits out `★ Insight ───` callouts into their own card, so they read
    /// as an aside rather than getting stuck inside the single markdown
    /// NSTextView with the rest of the reply (see `AgentMessageSegmenter`).
    private func agentMessage(id: String, snapshot: ChatPresentationSnapshot) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(Array(snapshot.segments(for: id).enumerated()), id: \.offset) { _, segment in
                switch segment {
                case .prose(let chunk):
                    AgentMarkdownTextView(markdown: chunk) { url in
                        appModel.handleTerminalOpenURL(url.absoluteString, in: worktree)
                    }
                case .insight(let body):
                    InsightCardView(text: body)
                case .diff(let path, let oldText, let newText):
                    ChatDiffPreviewView(path: path,
                                        oldText: oldText,
                                        newText: newText,
                                        worktree: worktree,
                                        appModel: appModel)
                }
            }
        }
    }

    private func latestThought(in items: [TranscriptItem]) -> String? {
        for item in items.reversed() {
            if case .thought(_, let text) = item { return text }
        }
        return nil
    }

    /// The flat transcript path intentionally avoids TimelineBuilder while
    /// the streaming layout is being stabilized. Recover the closed-turn
    /// timestamp here so the message row still owns its metadata instead of
    /// leaving the only visible time on a detached divider.
    private func messageTimestamp(for messageID: String,
                                  in items: [TranscriptItem]) -> Date? {
        guard let index = items.firstIndex(where: { $0.id == messageID }) else {
            return nil
        }
        for item in items.dropFirst(index + 1) {
            switch item {
            case .turnDivider(_, let at): return at
            case .userMessage: return nil
            default: continue
            }
        }
        return nil
    }

    // MARK: - User bubble

    private func userBubble(_ blocks: [ContentBlock]) -> some View {
        ChatRowSurface(kind: .user, usesInsetChrome: true) {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                    userBlockView(block)
                }
            }
            .foregroundStyle(Color.primary)
        }
        .padding(.horizontal, 0)
    }

    @ViewBuilder
    private func userBlockView(_ block: ContentBlock) -> some View {
        switch block {
        case .text(let text):
            Text(text).font(AppFont.system(size: 13)).textSelection(.enabled)
        case .resourceLink(_, let name):
            Label(name, systemImage: "doc")
                .font(AppFont.caption)
        case .image:
            Label("Image", systemImage: "photo")
                .font(AppFont.caption)
        case .resource, .unknown:
            EmptyView()
        }
    }

    // MARK: - Turn divider

    private func turnDivider(_ date: Date) -> some View {
        HStack(spacing: 10) {
            Rectangle().fill(.separator).frame(height: 1)
            Text(date, format: .dateTime.hour().minute())
                .font(AppFont.caption2)
                .foregroundStyle(.tertiary)
                .fixedSize()
            Rectangle().fill(.separator).frame(height: 1)
        }
        .padding(.vertical, 6)
    }

    static func formatDuration(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        if total < 60 { return "\(total)s" }
        return "\(total / 60)m \(String(format: "%02d", total % 60))s"
    }

}
