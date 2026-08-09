import SwiftUI
import TillerACP

/// Shared surface for transcript rows.  The transcript stays on one quiet
/// canvas; hover and active state are carried by a single low-contrast rail
/// instead of a stack of unrelated cards.
struct ChatRowSurface<Content: View>: View {
    enum Kind {
        case user, assistant, thinking, tool, system

        var accent: Color {
            switch self {
            case .user: AppTheme.tabFocusAccent
            case .assistant: AppTheme.railTask
            case .thinking: AppTheme.meta
            case .tool: AppTheme.railTool
            case .system: AppTheme.meta
            }
        }
    }

    let kind: Kind
    var isActive = false
    var usesInsetChrome = false
    var activeFill: Color? = nil
    var isFlat = false
    /// Preview-only override used by deterministic visual fixtures. Runtime
    /// hover still comes from the pointer via `onHover`.
    var hoverOverride: Bool? = nil
    @ViewBuilder let content: Content
    @State private var isHovering = false

    var body: some View {
        let hovering = hoverOverride ?? isHovering
        content
            .padding(.vertical, isFlat ? 4 : (usesInsetChrome ? 5 : 7))
            .padding(.horizontal, 10)
            // Rows fill the transcript column, which `CenteredComposerLayout`
            // already sizes to match the composer below.
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                isActive
                    ? AnyShapeStyle((activeFill ?? AppTheme.selectionFill.opacity(0.36)))
                    : hovering
                        ? AnyShapeStyle(AppTheme.rowHover)
                        : usesInsetChrome
                            ? AnyShapeStyle(AppTheme.cardFill)
                            : AnyShapeStyle(Color.clear),
                in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            .overlay {
                if usesInsetChrome && !isFlat {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(AppTheme.hairline.opacity(0.12), lineWidth: 0.5)
                }
            }
            .overlay(alignment: .leading) {
                if !isFlat {
                    Capsule(style: .continuous)
                        .fill(kind.accent)
                        .opacity(isActive ? 0.62 : 1)
                        .frame(width: isActive ? 1 : 0)
                        .padding(.vertical, 5)
                }
            }
            .shadow(color: usesInsetChrome && !isFlat ? .black.opacity(0.04) : .clear,
                    radius: 2, y: 1)
            .contentShape(Rectangle())
            .onHover { isHovering = $0 }
            .animation(.easeOut(duration: 0.12), value: hovering)
    }
}

struct ChatStatusBadge: View {
    let status: ToolCallStatus

    private var label: String {
        switch status {
        case .pending: "Queued"
        case .inProgress: "Running"
        case .completed: "Done"
        case .failed: "Failed"
        }
    }

    private var tint: Color {
        switch status {
        case .pending, .inProgress: AppTheme.railQuestion
        case .completed: AppTheme.tabDone
        case .failed: AppTheme.tabError
        }
    }

    var body: some View {
        HStack(spacing: 4) {
            if status == .pending || status == .inProgress {
                ProgressView().controlSize(.mini)
            } else {
                Image(systemName: status == .completed
                    ? "checkmark" : "exclamationmark")
                    .font(AppFont.system(size: 8, weight: .bold))
            }
            Text(label)
        }
        .font(AppFont.caption2.weight(.medium))
        .foregroundStyle(tint)
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .background(tint.opacity(0.07), in: Capsule(style: .continuous))
        .overlay {
            Capsule(style: .continuous)
                .stroke(tint.opacity(0.18), lineWidth: 0.5)
        }
        .accessibilityLabel(label)
    }
}

struct ChatCountBadge: View {
    let count: Int
    let label: String

    var body: some View {
        Text("\(count) \(label)")
            .font(AppFont.caption2.monospacedDigit())
            .foregroundStyle(AppTheme.meta)
            .padding(.horizontal, 5)
            .padding(.vertical, 2)
            .background(AppTheme.primaryPillBg.opacity(0.62), in: Capsule(style: .continuous))
    }
}

enum ChatRowMetrics {
    static func lineCount(for item: ToolCallItem) -> Int {
        let contentLines = item.content.reduce(into: 0) { total, content in
            switch content {
            case .diff(_, let oldText, let newText):
                total += lineCount(oldText) + lineCount(newText)
            case .content(.text(let text)):
                total += lineCount(text)
            case .terminal, .content, .unknown:
                break
            }
        }
        let terminalLines = item.terminalOutput.map(lineCount) ?? 0
        return max(contentLines + terminalLines, 1)
    }

    static func lineCount(_ text: String?) -> Int {
        guard let text, !text.isEmpty else { return 0 }
        return text.split(separator: "\n", omittingEmptySubsequences: false).count
    }
}
