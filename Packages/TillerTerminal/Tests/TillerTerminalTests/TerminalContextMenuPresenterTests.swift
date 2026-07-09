import Testing
import AppKit
import Foundation
import GhosttyTerminal
@testable import TillerTerminal
import TillerCore

@MainActor
struct TerminalContextMenuPresenterTests {
    @Test func clearIsDispatchedInlineNeverForwarded() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let proxy = TerminalSurfaceProxy(session: session)
        var forwarded: TerminalContextMenuAction?
        let presenter = TerminalContextMenuPresenter(
            paneId: UUID(),
            proxy: proxy,
            provider: { _, _ in [] },
            onAction: { action, _, _ in forwarded = action }
        )
        let item = NSMenuItem(title: "Clear Terminal", action: nil, keyEquivalent: "")
        item.representedObject = TerminalContextMenuAction.clear

        presenter.handle(item)

        #expect(forwarded == nil)
    }

    @Test func closeIsForwardedToOnAction() {
        let session = InMemoryTerminalSession(write: { _ in }, resize: { _ in })
        let proxy = TerminalSurfaceProxy(session: session)
        var forwarded: TerminalContextMenuAction?
        let presenter = TerminalContextMenuPresenter(
            paneId: UUID(),
            proxy: proxy,
            provider: { _, _ in [] },
            onAction: { action, _, _ in forwarded = action }
        )
        let item = NSMenuItem(title: "Close Terminal", action: nil, keyEquivalent: "")
        item.representedObject = TerminalContextMenuAction.close

        presenter.handle(item)

        #expect(forwarded == .close)
    }
}
