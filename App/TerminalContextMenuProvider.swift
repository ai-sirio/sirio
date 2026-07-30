import SwiftUI
import AppKit
import TillerCore
import TillerTerminal

@MainActor
final class TerminalContextMenuProvider {
    private weak var model: AppModel?

    init(model: AppModel) {
        self.model = model
    }

    func items(for paneId: UUID, proxy: TerminalSurfaceProxy) -> [TerminalContextMenuItem] {
        var items = [
            TerminalContextMenuItem(title: "Copy", systemImage: "doc.on.doc", action: .copy),
            TerminalContextMenuItem(title: "Paste", systemImage: "doc.on.clipboard", action: .paste),
            TerminalContextMenuItem(title: "Copy Context", systemImage: "doc.text", action: .copyContext),
            TerminalContextMenuItem(title: "Clear Terminal", systemImage: "eraser", action: .clear),
            TerminalContextMenuItem(title: "Set Title", systemImage: "pencil", action: .setTitle),
            TerminalContextMenuItem(title: "Copy Pane ID", systemImage: "number", action: .copyPaneId),
            TerminalContextMenuItem(title: "Copy Terminal ID", systemImage: "terminal", action: .copyTerminalId),
            TerminalContextMenuItem(title: "Split Terminal Right", systemImage: "square.split.1x2", action: .splitRight),
            TerminalContextMenuItem(title: "Split Terminal Down", systemImage: "square.split.2x1", action: .splitDown),
        ]
        if let tuple = model?.workspaceTabContaining(paneId: paneId), tuple.tab.leafIds.count > 1 {
            items.append(TerminalContextMenuItem(title: "Close Terminal", systemImage: "xmark.square", action: .close))
        }
        return items
    }

    func handle(_ action: TerminalContextMenuAction, paneId: UUID, proxy: TerminalSurfaceProxy) {
        switch action {
        case .copy:
            _ = proxy.copySelectionToPasteboard()
        case .paste:
            _ = proxy.pasteFromPasteboard()
        case .copyContext:
            proxy.copyContextToPasteboard()
        case .clear:
            break // Gestito inline da TerminalContextMenuPresenter sul proxy, non arriva mai qui.
        case .setTitle:
            showSetTitleAlert(paneId: paneId)
        case .copyPaneId:
            copyToPasteboard(paneId.uuidString)
        case .copyTerminalId:
            guard let tuple = model?.workspaceTabContaining(paneId: paneId) else { return }
            copyToPasteboard(tuple.tab.id.uuidString)
        case .splitRight:
            model?.workspaceSplit(paneId: paneId, axis: .horizontal)
        case .splitDown:
            model?.workspaceSplit(paneId: paneId, axis: .vertical)
        case .close:
            showCloseConfirmAlert(paneId: paneId)
        }
    }

    private func showCloseConfirmAlert(paneId: UUID) {
        let alert = NSAlert()
        alert.messageText = "Close Terminal"
        alert.informativeText = "Termina il processo in esecuzione in questo pane."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Cancel")
        alert.addButton(withTitle: "Close")
        alert.buttons[1].hasDestructiveAction = true
        if alert.runModal() == .alertSecondButtonReturn {
            model?.workspaceClosePane(paneId: paneId)
        }
    }

    private func copyToPasteboard(_ string: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(string, forType: .string)
    }

    private func showSetTitleAlert(paneId: UUID) {
        guard let tuple = model?.workspaceTabContaining(paneId: paneId) else { return }
        let alert = NSAlert()
        alert.messageText = "Set Title"
        alert.informativeText = "Enter the new title for \"\(tuple.tab.title):\""
        alert.alertStyle = .informational
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")
        let input = NSTextField(string: tuple.tab.title)
        input.frame = NSRect(x: 0, y: 0, width: 240, height: 22)
        alert.accessoryView = input
        if alert.runModal() == .alertFirstButtonReturn {
            model?.renameTab(tuple.tab.id, in: tuple.worktree.id, to: input.stringValue)
        }
    }
}

extension TerminalContextMenuProvider: TerminalContextMenuDelegate {
    func contextMenuPresenter(
        _ presenter: TerminalContextMenuPresenter,
        didSelect action: TerminalContextMenuAction,
        paneId: UUID,
        proxy: TerminalSurfaceProxy
    ) {
        handle(action, paneId: paneId, proxy: proxy)
    }
}
