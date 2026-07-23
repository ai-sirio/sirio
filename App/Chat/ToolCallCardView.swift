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
        VStack(alignment: .leading, spacing: 6) {
            header
            if expanded || isPermissionPending {
                contentBody
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8)
            .strokeBorder(isPermissionPending ? Color.orange.opacity(0.7) : .clear))
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
                        appModel.handleTerminalOpenURL(location.path, in: worktree)
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

    private func diffView(path: String, oldText: String?, newText: String) -> some View {
        VStack(alignment: .leading, spacing: 1) {
            Text((path as NSString).lastPathComponent)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
            if let oldText, !oldText.isEmpty {
                ForEach(Array(oldText.split(separator: "\n", omittingEmptySubsequences: false)
                    .prefix(40).enumerated()), id: \.offset) { _, line in
                    Text("- " + line)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.red.opacity(0.9))
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.red.opacity(0.08))
                }
            }
            ForEach(Array(newText.split(separator: "\n", omittingEmptySubsequences: false)
                .prefix(60).enumerated()), id: \.offset) { _, line in
                Text("+ " + line)
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.green.opacity(0.9))
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.green.opacity(0.08))
            }
        }
        .textSelection(.enabled)
    }

}
