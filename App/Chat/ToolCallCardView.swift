import SwiftUI
import TillerACP
import TillerCore
import Inject

/// One tool call as a quiet transcript row: kind icon, title, status badge and
/// line-count badge; expandable content (diff/output) stays available without
/// making every tool call look like a separate panel.
struct ToolCallCardView: View {
    @ObserveInjection private var inject

    let item: ToolCallItem
    let controller: ChatController
    let worktree: Worktree
    let appModel: AppModel

    @State private var expanded = false

    private var isPermissionPending: Bool { item.permission?.isPending == true }

    var body: some View {
        // Flat: a tool call is one line in the log, not a panel. Only a
        // pending permission still paints a fill, because that one is a
        // question to the reader.
        ChatRowSurface(kind: .tool,
                       isActive: isPermissionPending,
                       isFlat: true) {
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
            Image(systemName: expanded ? "chevron.down" : "chevron.right")
                .font(AppFont.caption2.weight(.semibold))
                .foregroundStyle(AppTheme.meta)
            Image(systemName: kindSymbol)
                .foregroundStyle(AppTheme.meta)
                .font(AppFont.caption)
                .frame(width: 13)
            Text(item.title)
                .font(AppFont.caption)
                .foregroundStyle(AppTheme.subtitle)
                .lineLimit(1)
            Spacer()
            if ChatRowMetrics.lineCount(for: item) > 0 {
                ChatCountBadge(count: ChatRowMetrics.lineCount(for: item), label: "lines")
            }
            ChatStatusBadge(status: item.status)
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
                textContent(text)
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
                            .font(AppFont.caption)
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.tint)
                }
            }
        }
    }

    /// Bounded like `TerminalOutputView`'s tail cap: MCP tool results in
    /// particular can be huge, and an unconstrained `Text` with selection
    /// enabled freezes AppKit's layout the moment this row expands.
    @ViewBuilder
    private func textContent(_ text: String) -> some View {
        let display = ChatRowMetrics.truncatedTail(text)
        VStack(alignment: .leading, spacing: 4) {
            if display.wasTruncated {
                Text("Showing last \(ChatRowMetrics.maxRenderedCharacters) characters")
                    .font(AppFont.caption2)
                    .foregroundStyle(AppTheme.meta)
            }
            ScrollView {
                Text(display.text)
                    .font(AppFont.mono(size: 11))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 260)
        }
    }

}
