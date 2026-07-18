import SwiftUI
import MarkdownUI
import TillerACP
import TillerCore

/// Scrolling transcript: user bubbles, agent markdown, thoughts, plans,
/// tool-call cards. Autoscrolls while the agent streams.
struct TranscriptView: View {
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 10) {
                    ForEach(controller.items) { item in
                        itemView(item)
                            .id(item.id)
                    }
                    Color.clear.frame(height: 1).id("bottom")
                }
                .padding(12)
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
            HStack {
                Spacer(minLength: 60)
                VStack(alignment: .trailing, spacing: 4) {
                    ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                        userBlockView(block)
                    }
                }
                .padding(.horizontal, 10).padding(.vertical, 6)
                .background(Color.accentColor.opacity(0.18),
                            in: RoundedRectangle(cornerRadius: 10))
            }
        case .agentMessage(_, let text, let isComplete):
            VStack(alignment: .leading, spacing: 2) {
                Markdown(text)
                    .markdownTheme(.gitHub)
                    .textSelection(.enabled)
                if !isComplete {
                    RunningDots(color: .secondary)
                }
            }
        case .thought(_, let text):
            DisclosureGroup {
                Text(text)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            } label: {
                Label("Ragionamento", systemImage: "brain")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
            }
        case .toolCall(let toolCall):
            ToolCallCardView(item: toolCall, controller: controller,
                             worktree: worktree, appModel: appModel)
        case .plan(_, let entries):
            planCard(entries)
        }
    }

    @ViewBuilder
    private func userBlockView(_ block: ContentBlock) -> some View {
        switch block {
        case .text(let text):
            Text(text).textSelection(.enabled)
        case .resourceLink(_, let name):
            Label(name, systemImage: "doc")
                .font(.caption)
                .foregroundStyle(.secondary)
        case .image:
            Label("Immagine", systemImage: "photo")
                .font(.caption)
                .foregroundStyle(.secondary)
        case .resource, .unknown:
            EmptyView()
        }
    }

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
