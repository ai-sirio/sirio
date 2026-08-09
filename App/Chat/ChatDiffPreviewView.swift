import AppKit
import SwiftUI
import TillerACP
import TillerCode
import TillerCore
import Inject

struct ChatDiffPreviewRow: Identifiable, Equatable {
    enum Side: Equatable { case old, new, context }
    let id: Int
    let side: Side
    let lineNumber: Int
    let text: String
}

enum ChatDiffPreviewModel {
    static func rows(oldText: String?, newText: String) -> [ChatDiffPreviewRow] {
        let old = split(oldText)
        let new = split(newText)
        var rows: [ChatDiffPreviewRow] = []
        var oldIndex = 0
        var newIndex = 0

        // Keep the diff readable without running a quadratic full-file diff:
        // matching lines stay in context, while replacements are interleaved
        // as `−` then `+` at the point where they diverge.
        while oldIndex < old.count || newIndex < new.count {
            if oldIndex < old.count, newIndex < new.count,
               old[oldIndex] == new[newIndex] {
                rows.append(ChatDiffPreviewRow(
                    id: rows.count, side: .context,
                    lineNumber: newIndex + 1, text: new[newIndex]))
                oldIndex += 1
                newIndex += 1
            } else {
                if oldIndex < old.count {
                    rows.append(ChatDiffPreviewRow(
                        id: rows.count, side: .old,
                        lineNumber: oldIndex + 1, text: old[oldIndex]))
                    oldIndex += 1
                }
                if newIndex < new.count {
                    rows.append(ChatDiffPreviewRow(
                        id: rows.count, side: .new,
                        lineNumber: newIndex + 1, text: new[newIndex]))
                    newIndex += 1
                }
            }
        }
        return rows
    }

    private static func split(_ value: String?) -> [String] {
        guard let value, !value.isEmpty else { return [] }
        var lines = value.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
        if lines.last?.isEmpty == true { lines.removeLast() }
        return lines
    }
}

struct ChatDiffPreviewView: View {
    @ObserveInjection private var inject

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
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Button {
                    appModel.openFileReference(path, in: worktree)
                } label: {
                    HStack(spacing: 5) {
                        Image(systemName: "chevron.left.forwardslash.chevron.right")
                        Text((path as NSString).lastPathComponent)
                    }
                }
                .buttonStyle(.plain)
                .font(AppFont.caption2.weight(.semibold))
                .foregroundStyle(AppTheme.title)
                Text("Proposal")
                    .font(AppFont.caption2.weight(.semibold))
                    .foregroundStyle(AppTheme.meta)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 2)
                    .background(AppTheme.primaryPillBg.opacity(0.62),
                                in: Capsule(style: .continuous))
                Spacer(minLength: 0)
                Text("+\(stats.added)")
                    .font(AppFont.caption2).foregroundStyle(AppTheme.diffAddition)
                Text("−\(stats.removed)")
                    .font(AppFont.caption2).foregroundStyle(AppTheme.diffDeletion)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 8)
            .background(AppTheme.cardFill.opacity(0.78))

            VStack(alignment: .leading, spacing: 1) {
                ForEach(ChatDiffPreviewModel.rows(oldText: oldText, newText: newText)) {
                    row($0)
                }
            }
            .padding(8)
            .background(AppTheme.codeInsetFill.opacity(0.52),
                        in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            .padding(8)
        }
        .background(AppTheme.cardFill.opacity(0.74),
                    in: RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(AppTheme.hairline.opacity(0.38), lineWidth: 0.5)
        }
        .textSelection(.enabled)
        .task(id: requestID) {
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL, oldText: oldText, newText: newText)
        }
    .enableInjection()
    }

    private func row(_ row: ChatDiffPreviewRow) -> some View {
        let tint: Color = switch row.side {
        case .old: AppTheme.diffDeletion
        case .new: AppTheme.diffAddition
        case .context: AppTheme.subtitle
        }
        let sign: String = switch row.side {
        case .old: "−"
        case .new: "+"
        case .context: " "
        }
        return HStack(alignment: .top, spacing: 0) {
            Text(String(format: "%3d", row.lineNumber))
                .font(AppFont.mono(size: 10))
                .foregroundStyle(AppTheme.meta)
                .frame(width: 30, alignment: .trailing)
                .padding(.trailing, 8)
            Text(sign)
                .font(AppFont.mono(size: 11, weight: .bold))
                .foregroundStyle(tint)
                .frame(width: 14, alignment: .leading)
            highlightedText(for: row)
        }
        .padding(.vertical, 2)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background {
            if row.side != .context {
                (row.side == .old
                    ? AppTheme.diffDeletionBackground
                    : AppTheme.diffAdditionBackground)
                    .opacity(0.08)
            }
        }
        .overlay(alignment: .leading) {
            if row.side != .context {
                Rectangle()
                    .fill(tint)
                    .frame(width: 2)
            }
        }
    }

    @ViewBuilder
    private func highlightedText(for row: ChatDiffPreviewRow) -> some View {
        let map = row.side == .old ? highlights?.old : highlights?.new
        if let map {
            Text(AttributedCodeRenderer.renderLine(
                row.text,
                ranges: map.ranges(forLine: row.lineNumber),
                theme: .tiller(isDark: colorScheme == .dark),
                font: AppFont.nsMono(size: 10)))
        } else {
            Text(row.text)
                .font(AppFont.mono(size: 11))
                .foregroundStyle(row.side == .old
                    ? AppTheme.diffDeletion
                    : row.side == .new ? AppTheme.diffAddition : AppTheme.subtitle)
        }
    }
}
