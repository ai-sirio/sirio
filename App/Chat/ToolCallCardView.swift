import SwiftUI
import TillerACP
import TillerCore

/// One tool call as a card: kind icon, title, status; expandable content
/// (diff/output); pending permission requests remain highlighted.
struct ToolCallCardView: View {
    let item: ToolCallItem
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    @State private var expanded = false

    private var isPermissionPending: Bool { item.permission?.isPending == true }

    var body: some View {
        ChatCard(kind: .tool, isHighlighted: isPermissionPending) {
            VStack(alignment: .leading, spacing: 6) {
                header
                if expanded || isPermissionPending {
                    contentBody
                }
            }
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            Image(systemName: kindSymbol)
                .foregroundStyle(.secondary)
                .font(.caption)
            Text(item.title)
                .font(.callout.weight(.medium))
                .lineLimit(expanded ? nil : 1)
            Spacer()
            statusGlyph
        }
        .contentShape(Rectangle())
        .onTapGesture { expanded.toggle() }
    }

    private var kindSymbol: String {
        switch item.kind {
        case .read: "doc.text.magnifyingglass"
        case .edit: "pencil"
        case .delete: "trash"
        case .move: "arrow.right.doc.on.clipboard"
        case .search: "magnifyingglass"
        case .execute: "terminal"
        case .think: "brain"
        case .fetch: "globe"
        case .switchMode: "arrow.triangle.2.circlepath"
        case .other: "wrench.and.screwdriver"
        }
    }

    @ViewBuilder
    private var statusGlyph: some View {
        switch item.status {
        case .pending, .inProgress:
            ProgressView().controlSize(.small)
        case .completed:
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green).font(.caption)
        case .failed:
            Image(systemName: "xmark.circle.fill")
                .foregroundStyle(.red).font(.caption)
        }
    }

    @ViewBuilder
    private var contentBody: some View {
        ForEach(Array(item.content.enumerated()), id: \.offset) { _, content in
            switch content {
            case .diff(let path, let oldText, let newText):
                diffView(path: path, oldText: oldText, newText: newText)
            case .content(.text(let text)):
                Text(text)
                    .font(.system(.caption, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            case .terminal:
                TerminalOutputView(
                    output: item.terminalOutput ?? "",
                    exit: item.terminalExit,
                    isRunning: item.status == .pending || item.status == .inProgress)
            case .content, .unknown:
                EmptyView()
            }
        }
        if !item.locations.isEmpty {
            HStack(spacing: 8) {
                ForEach(Array(item.locations.enumerated()), id: \.offset) { _, location in
                    Button {
                        appModel.openFileReference(location.path, in: worktree)
                    } label: {
                        Label((location.path as NSString).lastPathComponent,
                              systemImage: "arrow.up.forward.square")
                            .font(.caption)
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.tint)
                }
            }
        }
    }

    private static let previewLineLimit = 40

    private func diffView(path: String, oldText: String?, newText: String) -> some View {
        let stats = DiffStats.counts(oldText: oldText, newText: newText)
        let oldLines = lines(of: oldText).prefix(Self.previewLineLimit)
        let newLines = lines(of: newText).prefix(Self.previewLineLimit)
        let hidden = (stats.removed + stats.added) - (oldLines.count + newLines.count)
        return VStack(alignment: .leading, spacing: 1) {
            HStack(spacing: 6) {
                Text((path as NSString).lastPathComponent)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(.secondary)
                Button {
                    appModel.openFileReference(path, in: worktree)
                } label: {
                    Image(systemName: "chevron.left.forwardslash.chevron.right")
                }
                .buttonStyle(.plain)
                .help("Open in editor")
                Text("+\(stats.added)")
                    .font(.caption2).foregroundStyle(AppTheme.diffAddition)
                Text("−\(stats.removed)")
                    .font(.caption2).foregroundStyle(AppTheme.diffDeletion)
            }
            ForEach(Array(oldLines.enumerated()), id: \.offset) { _, line in
                diffLine("- " + line, color: AppTheme.diffDeletion,
                         background: AppTheme.diffDeletionBackground)
            }
            ForEach(Array(newLines.enumerated()), id: \.offset) { _, line in
                diffLine("+ " + line, color: AppTheme.diffAddition,
                         background: AppTheme.diffAdditionBackground)
            }
            if hidden > 0 {
                Button {
                    appModel.openFileReference(path, in: worktree)
                } label: {
                    Text("\(hidden) more lines — open file")
                        .font(.caption2)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tint)
                .padding(.top, 2)
            }
        }
        .textSelection(.enabled)
    }

    private func lines(of text: String?) -> [String] {
        guard let text, !text.isEmpty else { return [] }
        var split = text.split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
        if split.last?.isEmpty == true { split.removeLast() }
        return split
    }

    private func diffLine(_ text: String, color: Color,
                          background: Color) -> some View {
        Text(text)
            .font(.system(.caption, design: .monospaced))
            .foregroundStyle(color)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(background)
    }

}
