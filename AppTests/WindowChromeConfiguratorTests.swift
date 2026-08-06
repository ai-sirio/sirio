import AppKit
import SwiftUI
import Testing
@testable import Tiller

@Suite(.serialized)
@MainActor
struct WindowChromeConfiguratorTests {
    @Test func hostInstallsOneLeadingAndOneTrailingBoundedAccessory() throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                              styleMask: [.titled, .closable, .resizable],
                              backing: .buffered,
                              defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: window)
        host.update(
            leading: AnyView(TitleStripGroup { Button("Sidebar") {} }),
            trailing: AnyView(TitleStripGroup {
                HStack(spacing: TitlebarGeometry.controlSpacing) {
                    Button("Right panel") {}
                    Button("Split terminal") {}
                    Button("Permissions") {}
                }
            }))
        window.layoutIfNeeded()

        #expect(window.titlebarAccessoryViewControllers.count == 2)
        #expect(window.titlebarAccessoryViewControllers.map(\.layoutAttribute) == [.left, .right])

        let leading = try #require(host.leadingController)
        let trailing = try #require(host.trailingController)
        let contentBounds = try #require(window.contentView).bounds
        leading.view.layoutSubtreeIfNeeded()
        trailing.view.layoutSubtreeIfNeeded()

        #expect(leading.view.frame.size == CGSize(width: 24, height: 28))
        #expect(trailing.view.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(host.leadingHostingView?.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingHostingView?.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(leading.view.frame.maxX < contentBounds.maxX)
        #expect(trailing.view.frame.minX >= contentBounds.minX)
        #expect(trailing.view.frame.maxX <= contentBounds.maxX)
    }

    @Test func installingAndUpdatingDoesNotDuplicateAccessories() {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                              styleMask: [.titled, .closable, .resizable],
                              backing: .buffered,
                              defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: window)
        host.install(on: window)
        host.update(
            leading: AnyView(TitleStripGroup { Button("Sidebar") {} }),
            trailing: AnyView(TitleStripGroup {
                HStack(spacing: TitlebarGeometry.controlSpacing) {
                    Button("Right panel") {}
                    Button("Split terminal") {}
                    Button("Permissions") {}
                }
            }))
        window.layoutIfNeeded()

        #expect(window.titlebarAccessoryViewControllers.count == 2)
        #expect(host.leadingController?.view.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingController?.view.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(host.leadingHostingView?.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingHostingView?.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
    }
}
