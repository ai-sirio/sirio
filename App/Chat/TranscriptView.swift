import SwiftUI
import MarkdownUI
import TillerACP
import TillerCore

/// Scrolling transcript styled like a minimal chat log: warm user bubbles on
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
                    ForEach(controller.items) { item in
                        itemView(item)
                            .id(item.id)
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
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
    private func itemView(_ item: TranscriptItem) -> some View {
        switch item {
        case .userMessage(_, let blocks):
            userBubble(blocks)
        case .agentMessage(_, let text, let isComplete):
            VStack(alignment: .leading, spacing: 2) {
                Markdown(text)
                    .markdownTheme(.tiller)
                    .textSelection(.enabled)
                if !isComplete {
                    RunningDots(color: .secondary)
                }
            }
        case .thought(_, let text):
            ThoughtRow(text: text)
        case .toolCall(let toolCall):
            ToolCallCardView(item: toolCall, controller: controller,
                             worktree: worktree, appModel: appModel)
        case .plan(_, let entries):
            planCard(entries)
        case .turnDivider(_, let date):
            turnDivider(date)
        case .editSummary(_, let paths):
            EditSummaryCardView(paths: paths, worktree: worktree,
                                appModel: appModel)
        }
    }

    // MARK: - User bubble

    private func userBubble(_ blocks: [ContentBlock]) -> some View {
        HStack(alignment: .top) {
            Spacer(minLength: 80)
            VStack(alignment: .trailing, spacing: 4) {
                ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                    userBlockView(block)
                }
            }
            .padding(.horizontal, 12).padding(.vertical, 8)
            .foregroundStyle(Color.black.opacity(0.85))
            .background(Color(red: 0.93, green: 0.78, blue: 0.60),
                        in: RoundedRectangle(cornerRadius: 12))
            .overlay(alignment: .topTrailing) {
                Circle()
                    .fill(Color.orange)
                    .frame(width: 9, height: 9)
                    .offset(x: 4, y: -3)
            }
        }
        .padding(.trailing, 6)
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

    // MARK: - Plan

    private func planCard(_ entries: [PlanEntry]) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Piano", systemImage: "checklist")
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
