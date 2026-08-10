import SwiftUI

/// A compact, collapsible status row for the live turn.  It keeps the status
/// visible while the agent works without turning every transient thought into
/// a permanent card in the transcript.
struct ThinkingRowView: View {
    let title: String
    let detail: String?

    @State private var isExpanded: Bool

    init(title: String, detail: String?, initiallyExpanded: Bool = false) {
        self.title = title
        self.detail = detail
        _isExpanded = State(initialValue: initiallyExpanded)
    }

    var body: some View {
        ChatRowSurface(kind: .thinking,
                       isActive: false,
                       isFlat: true) {
            VStack(alignment: .leading, spacing: 6) {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        isExpanded.toggle()
                    }
                } label: {
                    HStack(spacing: 5) {
                        RunningDots(color: AppTheme.meta, dotSize: 3)
                        Text(title)
                            .font(AppFont.caption2)
                            .foregroundStyle(AppTheme.meta.opacity(0.82))
                            .lineLimit(1)
                        Spacer(minLength: 8)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if let detail, !detail.isEmpty {
                    HStack(alignment: .top, spacing: 7) {
                        Capsule(style: .continuous)
                            .fill(AppTheme.meta.opacity(0.62))
                            .frame(width: 2.5, height: 24)
                        Text(detail)
                            .font(AppFont.system(size: 12.5, weight: .regular))
                            .foregroundStyle(AppTheme.title.opacity(0.92))
                            .textSelection(.enabled)
                            .lineSpacing(2)
                            .lineLimit(isExpanded ? nil : 1)
                    }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 6)
                    .transition(.opacity.combined(with: .move(edge: .top)))
                }
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(detail.map { "Thinking: \(title). \($0)" } ?? "Thinking: \(title)")
    }
}

/// Historical reasoning rows remain in the transcript as user-expandable
/// prose, but share the same row surface and hover treatment as live status.
struct ThoughtRowView: View {
    let text: String
    @State private var isExpanded = false

    var body: some View {
        ChatRowSurface(kind: .thinking, isFlat: true) {
            VStack(alignment: .leading, spacing: 5) {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) { isExpanded.toggle() }
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: "chevron.right")
                            .font(AppFont.caption2.weight(.semibold))
                            .rotationEffect(.degrees(isExpanded ? 90 : 0))
                        Image(systemName: "brain")
                            .font(AppFont.caption)
                            .foregroundStyle(AppTheme.meta)
                            .frame(width: 13)
                        Text("Thinking")
                            .font(AppFont.caption.weight(.medium))
                        Spacer(minLength: 0)
                        Text(isExpanded ? "Hide" : "Show")
                            .font(AppFont.caption2)
                            .foregroundStyle(AppTheme.meta)
                    }
                    .foregroundStyle(AppTheme.subtitle)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)

                if isExpanded {
                    Text(text)
                        .font(AppFont.system(size: 12.5))
                        .foregroundStyle(AppTheme.subtitle)
                        .textSelection(.enabled)
                        .padding(.leading, 22)
                }
            }
        }
    }
}
