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

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(controller.timelineRows) { row in
                        rowView(row)
                            .id(row.id)
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .frame(maxWidth: 700)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .onChange(of: controller.items.count) {
                withAnimation(.easeOut(duration: 0.15)) {
                    proxy.scrollTo("bottom", anchor: .bottom)
                }
            }
        }
    }

    @ViewBuilder
    private func rowView(_ row: TimelineRow) -> some View {
        switch row {
        case .message(let item, let meta):
            itemView(item, meta: meta)
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
            planCard(entries, approval: approval)
        case .working:
            thinkingRow
        }
    }

    @ViewBuilder
    private func itemView(_ item: TranscriptItem, meta: TimelineRow.MessageMeta?) -> some View {
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
            ToolCallCardView(item: toolCall, controller: controller,
                             worktree: worktree, appModel: appModel)
        case .plan(_, let entries):
            planCard(entries, approval: nil)
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

    /// Live status while a turn is in flight; driven by `controller.state`
    /// rather than a per-message completion flag so it can't get stuck once
    /// the turn actually ends.
    private var thinkingRow: some View {
        HStack(spacing: 6) {
            RunningDots(color: .orange)
            Text("Thinking").font(.caption).foregroundStyle(.orange)
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
            Label("Immagine", systemImage: "photo")
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

    // MARK: - Plan

    private func planCard(_ entries: [PlanEntry], approval: PermissionState?) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Plan", systemImage: "checklist")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(Array(entries.enumerated()), id: \.offset) { _, entry in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: entry.status == "completed"
                          ? "checkmark.circle.fill"
                          : entry.status == "in_progress" ? "circle.dotted" : "circle")
                        .foregroundStyle(entry.status == "completed" ? .green : .secondary)
                        .font(.caption)
                    Text(entry.content).font(.callout)
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
                              ? .green : .red)
                        .controlSize(.small)
                    }
                }
                .padding(.top, 4)
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
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
