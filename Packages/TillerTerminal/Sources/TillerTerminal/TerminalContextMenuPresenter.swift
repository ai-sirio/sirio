#if canImport(AppKit)
import AppKit
#endif
import Foundation
import TillerCore

/// Builds an AppKit context menu for a terminal pane from a provider-supplied
/// list of items. Surface-level actions (copy/paste/copy context) are handled
/// directly on the proxy; all other actions are forwarded to the delegate so
/// the host (e.g. Tiller/App) can implement split, rename, ID copying, etc.
@MainActor
public final class TerminalContextMenuPresenter {
    public let paneId: UUID
    public let proxy: TerminalSurfaceProxy
    public weak var delegate: TerminalContextMenuDelegate?

    public let provider: (UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem]
    public let onAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)?

    public init(
        paneId: UUID,
        proxy: TerminalSurfaceProxy,
        provider: @escaping (UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem],
        onAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)? = nil
    ) {
        self.paneId = paneId
        self.proxy = proxy
        self.provider = provider
        self.onAction = onAction
    }

    #if canImport(AppKit)
    public func menu() -> NSMenu {
        let menu = NSMenu()
        for item in provider(paneId, proxy) {
            let nsItem = NSMenuItem(
                title: item.title,
                action: #selector(handle(_:)),
                keyEquivalent: ""
            )
            nsItem.target = self
            nsItem.image = NSImage(
                systemSymbolName: item.systemImage,
                accessibilityDescription: item.title
            )
            nsItem.representedObject = item.action
            switch item.action {
            case .copy:
                nsItem.isEnabled = proxy.canCopy
            case .paste:
                nsItem.isEnabled = proxy.canPaste
            default:
                break
            }
            menu.addItem(nsItem)
        }
        return menu
    }
    #endif

    /// `internal` (non `private`) così i test di regressione sul dispatch
    /// possono invocarlo via `@testable import` senza passare dal runtime
    /// AppKit target-action.
    @objc func handle(_ sender: NSMenuItem) {
        guard let action = sender.representedObject as? TerminalContextMenuAction else { return }
        switch action {
        case .copy:
            proxy.copySelectionToPasteboard()
        case .paste:
            proxy.pasteFromPasteboard()
        case .copyContext:
            proxy.copyContextToPasteboard()
        case .clear:
            proxy.clearScreen()
        default:
            delegate?.contextMenuPresenter(self, didSelect: action, paneId: paneId, proxy: proxy)
            onAction?(action, paneId, proxy)
        }
    }
}

/// Hook for delegating non-surface context-menu actions to the host.
@MainActor
public protocol TerminalContextMenuDelegate: AnyObject {
    func contextMenuPresenter(
        _ presenter: TerminalContextMenuPresenter,
        didSelect action: TerminalContextMenuAction,
        paneId: UUID,
        proxy: TerminalSurfaceProxy
    )
}
