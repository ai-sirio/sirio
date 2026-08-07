import AppKit
import SwiftUI
import Testing
@testable import Tiller

@Suite(.serialized)
@MainActor
struct FileExplorerRowInteractionTests {
    @Test func firstClickSelectsWithoutWaitingForDoubleClick() throws {
        let recorder = FileExplorerInteractionRecorder()
        let root = HStack {
            Text("file.swift")
                .onTapGesture(count: 2) { recorder.openCount += 1 }
        }
        .frame(width: 180, height: 28)
        .contentShape(Rectangle())
        .draggable(URL(fileURLWithPath: "/tmp/file.swift"))
        .fileExplorerRowSelection { recorder.selectionCount += 1 }

        let hostingView = NSHostingView(rootView: root)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 180, height: 28),
            styleMask: [.titled],
            backing: .buffered,
            defer: false)
        window.contentView = hostingView
        window.makeKeyAndOrderFront(nil)
        window.layoutIfNeeded()
        defer {
            window.orderOut(nil)
            window.close()
        }

        try sendFileRowClick(to: window, at: NSPoint(x: 90, y: 14), clickCount: 1)
        RunLoop.current.run(until: Date().addingTimeInterval(0.02))

        #expect(recorder.selectionCount == 1)
        #expect(recorder.openCount == 0)
    }
}

@MainActor
private final class FileExplorerInteractionRecorder {
    var selectionCount = 0
    var openCount = 0
}

@MainActor
private func sendFileRowClick(
    to window: NSWindow,
    at location: NSPoint,
    clickCount: Int
) throws {
    let timestamp = ProcessInfo.processInfo.systemUptime
    let mouseDown = try #require(NSEvent.mouseEvent(
        with: .leftMouseDown,
        location: location,
        modifierFlags: [],
        timestamp: timestamp,
        windowNumber: window.windowNumber,
        context: nil,
        eventNumber: 8_000,
        clickCount: clickCount,
        pressure: 1))
    let mouseUp = try #require(NSEvent.mouseEvent(
        with: .leftMouseUp,
        location: location,
        modifierFlags: [],
        timestamp: timestamp + 0.001,
        windowNumber: window.windowNumber,
        context: nil,
        eventNumber: 8_001,
        clickCount: clickCount,
        pressure: 0))

    NSApp.sendEvent(mouseDown)
    NSApp.sendEvent(mouseUp)
}
