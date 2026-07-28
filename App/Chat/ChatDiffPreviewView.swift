import AppKit
import SwiftUI
import TillerACP
import TillerCode
import TillerCore

struct ChatDiffPreviewRow: Identifiable, Equatable {
    enum Side: Equatable { case old, new }
    let id: Int
    let side: Side
    let lineNumber: Int
    let text: String
}

enum ChatDiffPreviewModel {
    static func rows(oldText: String?, newText: String) -> [ChatDiffPreviewRow] {
        let old = split(oldText).enumerated().map {
            ChatDiffPreviewRow(id: $0.offset, side: .old,
                               lineNumber: $0.offset + 1, text: $0.element)
        }
        let new = split(newText).enumerated().map {
            ChatDiffPreviewRow(id: old.count + $0.offset, side: .new,
                               lineNumber: $0.offset + 1, text: $0.element)
        }
        return old + new
    }

    private static func split(_ value: String?) -> [String] {
        guard let value, !value.isEmpty else { return [] }
        var lines = value.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        if lines.last?.isEmpty == true { lines.removeLast() }
        return lines
    }
}

struct ChatDiffPreviewView: View {
    let path: String
    let oldText: String?
    let newText: String
    let worktree: Worktree
    let appModel: AppModel

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

    private var fileURL: URL {
        FileLink.resolve(path, worktreePath: worktree.path)
            ?? URL(fileURLWithPath: worktree.path).appendingPathComponent(path)
    }

    private var requestID: String {
        "\(fileURL.standardizedFileURL.path)|\(oldText?.hashValue ?? 0)|\(newText.hashValue)"
    }

    var body: some View {
        let stats = DiffStats.counts(oldText: oldText, newText: newText)
        VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Button {
                    appModel.openFileReference(path, in: worktree)
                } label: {
                    Label((path as NSString).lastPathComponent,
                          systemImage: "chevron.left.forwardslash.chevron.right")
                }
                .buttonStyle(.plain)
                .font(.caption2.weight(.semibold))
                Text("+\(stats.added)")
                    .font(.caption2).foregroundStyle(AppTheme.diffAddition)
                Text("−\(stats.removed)")
                    .font(.caption2).foregroundStyle(AppTheme.diffDeletion)
            }
            ForEach(ChatDiffPreviewModel.rows(oldText: oldText, newText: newText)) {
                row($0)
            }
        }
        .textSelection(.enabled)
        .task(id: requestID) {
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL, oldText: oldText, newText: newText)
        }
    }

    private func row(_ row: ChatDiffPreviewRow) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Text(row.side == .old ? "- " : "+ ")
                .foregroundStyle(row.side == .old
                    ? AppTheme.diffDeletion : AppTheme.diffAddition)
            highlightedText(for: row)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(row.side == .old
            ? AppTheme.diffDeletionBackground : AppTheme.diffAdditionBackground)
    }

    @ViewBuilder
    private func highlightedText(for row: ChatDiffPreviewRow) -> some View {
        let map = row.side == .old ? highlights?.old : highlights?.new
        if let map {
            Text(AttributedCodeRenderer.renderLine(
                row.text,
                ranges: map.ranges(forLine: row.lineNumber),
                theme: .tiller(isDark: colorScheme == .dark),
                font: .monospacedSystemFont(ofSize: 10, weight: .regular)))
        } else {
            Text(row.text)
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(row.side == .old
                    ? AppTheme.diffDeletion : AppTheme.diffAddition)
        }
    }
}
