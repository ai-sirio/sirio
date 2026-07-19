import SwiftUI

/// One `★ Insight ───` callout, extracted by `AgentMessageSegmenter` and
/// rendered as its own card — same visual language as `ToolCallCardView` —
/// instead of inline in the surrounding prose's `NSTextView`.
struct InsightCardView: View {
    let text: String

    var body: some View {
        HStack(alignment: .top, spacing: 6) {
            Image(systemName: "sparkles")
                .foregroundStyle(.yellow)
                .font(.caption)
            AgentMarkdownTextView(markdown: text)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.yellow.opacity(0.10), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8)
            .strokeBorder(Color.yellow.opacity(0.35)))
        .padding(.vertical, 6)
    }
}
