import SwiftUI
import TillerCore
import TillerGit
import Inject

enum ChangedFileCounts {
    /// Nil when there is nothing worth showing, so the row can fall back to
    /// blank space instead of a misleading "+0".
    static func label(for stat: GitDiffStat?) -> String? {
        guard let stat else { return nil }
        if stat.isBinary { return "bin" }
        var parts: [String] = []
        if stat.deletions > 0 { parts.append("−\(stat.deletions)") }
        if stat.additions > 0 { parts.append("+\(stat.additions)") }
        return parts.isEmpty ? nil : parts.joined(separator: " ")
    }
}

struct ChangedFileRow: View {
    @ObserveInjection private var inject

    let entry: GitStatusEntry
    let stat: GitDiffStat?
    let isExpanded: Bool
    let iconTheme: FileIconTheme
    let isStagedSection: Bool
    let onToggle: () -> Void
    let onStage: () -> Void
    let onUnstage: () -> Void
    let onDiscard: () -> Void
    let onOpenFile: () -> Void
    let onOpenDiff: () -> Void

    @State private var isHovering = false

    private var fileName: String {
        entry.path.value.split(separator: "/").last.map(String.init) ?? entry.path.value
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(AppTheme.meta)
                .rotationEffect(.degrees(isExpanded ? 90 : 0))
                .frame(width: 12)

            FileTypeIcon(
                key: FileIconKey.key(forFileName: fileName),
                theme: iconTheme,
                tint: GitStatusStyle.color(entry))
                .frame(width: 14)

            Text(entry.path.value)
                .font(.system(size: 12))
                .foregroundStyle(GitStatusStyle.color(entry))
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer(minLength: 6)

            if entry.isConflicted {
                Text("Resolve in terminal")
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
            } else if isHovering {
                actions
            } else if let counts = ChangedFileCounts.label(for: stat) {
                Text(counts)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(AppTheme.meta)
            }
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .draggable(DiffDragPayload(path: entry.path.value))
        .background(
            isHovering ? AppTheme.rowHover : Color.clear,
            in: RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
        .onTapGesture(perform: onToggle)
        .onTapGesture(count: 2, perform: onOpenDiff)
        .contextMenu {
            if !entry.isConflicted {
                if isStagedSection {
                    Button("Unstage", action: onUnstage)
                } else {
                    Button("Stage", action: onStage)
                    Button("Discard", role: .destructive, action: onDiscard)
                }
            }
            Button("Open in editor", action: onOpenFile)
            Button("Open Diff in Editor", action: onOpenDiff)
        }
        .help(entry.isConflicted ? "Conflicted" : entry.path.value)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            "\(GitStatusStyle.symbol(entry)) \(entry.path.value)")
        .accessibilityHint(isExpanded ? "Collapse diff" : "Expand diff")
    .enableInjection()
    }

    private var actions: some View {
        HStack(spacing: 6) {
            if isStagedSection {
                Button("Unstage", action: onUnstage)
                    .buttonStyle(.plain)
            } else {
                Button("Discard", role: .destructive, action: onDiscard)
                    .buttonStyle(.plain)
                Button("Stage", action: onStage)
                    .buttonStyle(.plain)
            }
            Button(action: onOpenFile) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
            }
            .buttonStyle(.plain)
            .help("Open in editor")
            Button(action: onOpenDiff) {
                Image(systemName: "arrow.left.and.right.text.vertical")
            }
            .buttonStyle(.plain)
            .help("Open Diff in Editor")
        }
        .font(.caption)
    }
}
