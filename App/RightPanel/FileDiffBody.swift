import AppKit
import SwiftUI
import TillerCode
import TillerGit
import Inject

/// Rendered body of one expanded file: hunk headers plus numbered, syntax
/// highlighted lines. Scrolls with its parent — the changes list owns the
/// ScrollView so that expanding a file does not create a nested scroll area.
struct FileDiffBody: View {
    @ObserveInjection private var inject

    let diff: GitFileDiff
    let fileURL: URL?

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

    /// `diff --git` and `+++`/`---` headers carry nothing the row does not
    /// already show, so they never reach the screen.
    static func renderableLines(of diff: GitFileDiff) -> [GitDiffLine] {
        diff.lines.filter { $0.kind != .metadata }
    }

    private struct HighlightRequest: Hashable {
        let path: String?
        let oldHash: Int?
        let newHash: Int?
    }

    private var requestID: HighlightRequest {
        HighlightRequest(
            path: fileURL?.standardizedFileURL.path,
            oldHash: diff.oldText?.hashValue,
            newHash: diff.newText?.hashValue)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Self.renderableLines(of: diff)) { line in
                if line.kind == .hunk {
                    Text(line.text)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(AppTheme.subtitle)
                        .padding(.vertical, 1)
                        .padding(.leading, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(AppTheme.diffHunkBackground)
                } else {
                    lineRow(line)
                }
            }
        }
        .padding(.vertical, 4)
        .task(id: requestID) {
            guard let fileURL else { highlights = nil; return }
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL,
                oldText: diff.oldText,
                newText: diff.newText)
        }
    .enableInjection()
    }

    private func lineRow(_ line: GitDiffLine) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Text(line.oldLineNumber.map(String.init) ?? "")
                .frame(width: 32, alignment: .trailing)
                .foregroundStyle(AppTheme.meta)
            Text(line.newLineNumber.map(String.init) ?? "")
                .frame(width: 32, alignment: .trailing)
                .foregroundStyle(AppTheme.meta)
            codeText(for: line)
                .textSelection(.enabled)
                .padding(.leading, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.system(size: 11, design: .monospaced))
        .padding(.vertical, 1)
        .background(background(for: line))
    }

    private func ranges(for line: GitDiffLine) -> [SyntaxHighlightRange]? {
        switch line.kind {
        case .deletion:
            guard let number = line.oldLineNumber,
                  let map = highlights?.old else { return nil }
            return map.ranges(forLine: number)
        case .addition, .context:
            guard let number = line.newLineNumber,
                  let map = highlights?.new else { return nil }
            return map.ranges(forLine: number)
        case .hunk, .metadata:
            return nil
        }
    }

    @ViewBuilder
    private func codeText(for line: GitDiffLine) -> some View {
        if let ranges = ranges(for: line) {
            Text(AttributedCodeRenderer.renderLine(
                line.text,
                ranges: ranges,
                theme: .tiller(isDark: colorScheme == .dark),
                font: .monospacedSystemFont(ofSize: 11, weight: .regular)))
        } else {
            Text(line.text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(foreground(for: line))
        }
    }

    private func foreground(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAddition
        case .deletion: AppTheme.diffDeletion
        default: AppTheme.subtitle
        }
    }

    private func background(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAdditionBackground
        case .deletion: AppTheme.diffDeletionBackground
        default: .clear
        }
    }
}
