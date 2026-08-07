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
        leading.view.layoutSubtreeIfNeeded()
        trailing.view.layoutSubtreeIfNeeded()

        #expect(leading.view.frame.size == CGSize(width: 24, height: 28))
        #expect(trailing.view.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(host.leadingHostingView?.frame.size == CGSize(width: 24, height: 28))
        #expect(host.trailingHostingView?.frame.size == CGSize(width: 3 * 24 + 2 * 2, height: 28))
        #expect(leading.view.frame.origin.x == 0)
        #expect(trailing.view.frame.origin.x == 0)
        #expect(host.leadingHostingView?.frame.origin.x == 0)
        #expect(host.trailingHostingView?.frame.origin.x == 0)
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

    @Test func configuredWindowDoesNotMoveFromContentBackground() {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false)
        window.isMovableByWindowBackground = true

        configureWindowSurface(window)

        #expect(window.styleMask.contains(.fullSizeContentView))
        #expect(window.titlebarAppearsTransparent)
        #expect(window.isOpaque == false)
        #expect(window.backgroundColor == .clear)
        #expect(window.isMovableByWindowBackground == false)
    }

    @Test func installingOnReplacementWindowMovesAccessoriesAndResetsEvidence() {
        let firstWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                                   styleMask: [.titled, .closable, .resizable],
                                   backing: .buffered,
                                   defer: false)
        let replacementWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                                         styleMask: [.titled, .closable, .resizable],
                                         backing: .buffered,
                                         defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: firstWindow)
        host.update(leading: AnyView(EmptyView()), trailing: AnyView(EmptyView()))
        host.recordRenderedControl(slot: .split, accessibilityIdentifier: "legacy")

        host.install(on: replacementWindow)

        #expect(firstWindow.titlebarAccessoryViewControllers.isEmpty)
        #expect(replacementWindow.titlebarAccessoryViewControllers.count == 2)
        #expect(host.updateCount == 0)
        #expect(host.renderedControlProbes.isEmpty)

        host.remove(from: firstWindow)
        #expect(replacementWindow.titlebarAccessoryViewControllers.count == 2)

        host.remove(from: replacementWindow)
        #expect(replacementWindow.titlebarAccessoryViewControllers.isEmpty)
    }

    @Test func repeatedProbeUpdatesRemainBounded() {
        let host = TitlebarAccessoryHost()

        for _ in 0..<100 {
            host.recordRenderedControl(slot: .split, accessibilityIdentifier: "legacy")
            host.recordRenderedControl(slot: .split, accessibilityIdentifier: "workspace")
        }

        #expect(host.renderedControlProbes.count == 2)
    }

    @Test func removingFromWrongWindowLeavesOwnedAccessoriesInstalled() throws {
        let ownerWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                                   styleMask: [.titled, .closable, .resizable],
                                   backing: .buffered,
                                   defer: false)
        let otherWindow = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 560),
                                   styleMask: [.titled, .closable, .resizable],
                                   backing: .buffered,
                                   defer: false)
        let host = TitlebarAccessoryHost()

        host.install(on: ownerWindow)
        host.remove(from: otherWindow)

        #expect(ownerWindow.titlebarAccessoryViewControllers.count == 2)
        #expect(otherWindow.titlebarAccessoryViewControllers.isEmpty)
        #expect(host.leadingController != nil)
        #expect(host.trailingController != nil)

        host.remove(from: ownerWindow)

        #expect(ownerWindow.titlebarAccessoryViewControllers.isEmpty)
        #expect(host.leadingController == nil)
        #expect(host.trailingController == nil)
        #expect(host.leadingHostingView == nil)
        #expect(host.trailingHostingView == nil)

        host.remove(from: ownerWindow)
        #expect(ownerWindow.titlebarAccessoryViewControllers.isEmpty)
    }

    @Test func titlebarDoubleClickBandUsesTheFixedChromeHeight() {
        let band = titlebarDoubleClickBand(
            in: NSRect(x: 12, y: 20, width: 900, height: 560),
            height: TitlebarGeometry.accessoryHeight)

        #expect(band == NSRect(x: 12, y: 552, width: 900, height: 28))
    }

    @Test func titlebarDoubleClickActionRespectsSystemPreferences() {
        #expect(resolvedStandardTitlebarDoubleClickAction(globalDefaults: [
            "AppleActionOnDoubleClick": "Fill",
        ]) == .zoom)
        #expect(resolvedStandardTitlebarDoubleClickAction(globalDefaults: [
            "AppleActionOnDoubleClick": "Minimize",
        ]) == .miniaturize)
        #expect(resolvedStandardTitlebarDoubleClickAction(globalDefaults: [
            "AppleActionOnDoubleClick": "No Action",
        ]) == .none)
        #expect(resolvedStandardTitlebarDoubleClickAction(globalDefaults: [
            "AppleMiniaturizeOnDoubleClick": true,
        ]) == .miniaturize)
        #expect(resolvedStandardTitlebarDoubleClickAction(globalDefaults: [:]) == .zoom)
    }

    @Test func hiddenTitlebarDoubleClickUsesConfiguredSystemAction() throws {
        let window = TitlebarActionProbeWindow(
            contentRect: NSRect(x: 120, y: 160, width: 900, height: 560),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false)
        let contentView = TitlebarMouseEventProbeView(frame: window.contentView?.bounds ?? .zero)
        let host = TitlebarAccessoryHost()

        window.contentView = contentView
        window.styleMask.insert(.fullSizeContentView)
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        host.install(on: window)
        window.makeKeyAndOrderFront(nil)
        window.layoutIfNeeded()
        RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        defer {
            host.remove(from: window)
            window.orderOut(nil)
            window.close()
        }

        let contentBoundsInWindow = contentView.convert(contentView.bounds, to: nil)
        let titlebarBand = titlebarDoubleClickBand(
            in: contentBoundsInWindow,
            height: TitlebarGeometry.accessoryHeight)
        let clickLocation = NSPoint(x: titlebarBand.midX, y: titlebarBand.midY)
        let mouseDown = try #require(NSEvent.mouseEvent(
            with: .leftMouseDown,
            location: clickLocation,
            modifierFlags: [],
            timestamp: ProcessInfo.processInfo.systemUptime,
            windowNumber: window.windowNumber,
            context: nil,
            eventNumber: 7_000,
            clickCount: 2,
            pressure: 1))

        NSApp.sendEvent(mouseDown)
        RunLoop.current.run(until: Date().addingTimeInterval(0.05))

        #expect(contentView.mouseDownCount == 0)

        let globalDefaults = UserDefaults.standard.persistentDomain(
            forName: UserDefaults.globalDomain) ?? [:]
        switch resolvedStandardTitlebarDoubleClickAction(globalDefaults: globalDefaults) {
        case .miniaturize:
            #expect(window.miniaturizeCallCount == 1)
            #expect(window.zoomCallCount == 0)
        case .zoom:
            #expect(window.miniaturizeCallCount == 0)
            #expect(window.zoomCallCount == 1)
        case .none:
            #expect(window.miniaturizeCallCount == 0)
            #expect(window.zoomCallCount == 0)
        }
    }
}

private final class TitlebarMouseEventProbeView: NSView {
    private(set) var mouseDownCount = 0

    override var isOpaque: Bool { true }
    override var mouseDownCanMoveWindow: Bool { false }

    override func mouseDown(with event: NSEvent) {
        mouseDownCount += 1
    }
}

private final class TitlebarActionProbeWindow: NSWindow {
    private(set) var miniaturizeCallCount = 0
    private(set) var zoomCallCount = 0

    override func miniaturize(_ sender: Any?) {
        miniaturizeCallCount += 1
    }

    override func zoom(_ sender: Any?) {
        zoomCallCount += 1
    }
}
