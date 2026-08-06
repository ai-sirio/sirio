import SwiftUI
import TillerACP
import TillerCore
import Inject

/// One tool call as a card: kind icon, title, status; expandable content
/// (diff/output); pending permission requests remain highlighted.
struct ToolCallCardView: View {
    @ObserveInjection private var inject

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
                dismissControl
            }
        }
    .enableInjection()
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

    /// A permission the question card cannot render — no options, no text
    /// field — would otherwise wait forever, and the composer stays locked
    /// while it does. The escape hatch keeps that unrecoverable.
    @ViewBuilder
    private var dismissControl: some View {
        if isPermissionPending, ChatQuestion.from(item)?.hasControls != true,
           let requestId = item.permission?.requestId {
            Button("Dismiss") {
                Task { await controller.answerPermission(requestId: requestId,
                                                         optionId: nil) }
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }

    @ViewBuilder
    private var contentBody: some View {
        ForEach(Array(item.content.enumerated()), id: \.offset) { _, content in
            switch content {
            case .diff(let path, let oldText, let newText):
                ChatDiffPreviewView(
                    path: path,
                    oldText: oldText,
                    newText: newText,
                    worktree: worktree,
                    appModel: appModel)
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

}
