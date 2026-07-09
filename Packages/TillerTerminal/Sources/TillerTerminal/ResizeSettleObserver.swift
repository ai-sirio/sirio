import AppKit
import GhosttyTerminal

/// After a rapid NSSplitView divider drag (sidebar ↔ terminal), ghostty's
/// CAMetalLayer drawable can retain stale glyphs — duplicated prompt
/// fragments that appear composited over the terminal but are NOT in the PTY
/// grid (so `clear` won't remove them). The package's resize tick
/// (`setFrameSize` / `layout` → `core.fitToSize()`) runs per-event but has
/// no settle hook to re-validate the drawable once dragging stops.
///
/// This observer debounces `NSSplitView.didResizeSubviewsNotification` and
/// `NSWindow.didEndLiveResizeNotification`, then walks the container subtree
/// calling `AppTerminalView.fitToSize()` — a public full-resync that
/// repaints a clean frame.
///
/// TODO(upstream): live-resize settle handling belongs in GhosttyTerminal.
/// libghostty-spm's AppTerminalView+Lifecycle.swift already calls
/// `core.fitToSize()` per resize tick but has no settle hook; worth filing
/// an issue on https://github.com/Lakr233/libghostty-spm.
@MainActor
final class TerminalResizeSettleObserver {
    private weak var containerView: NSView?
    /// nonisolated(unsafe): accessed only on the main thread (AppKit
    /// notifications are delivered there); Swift 6 Sendable checking
    /// treats DispatchWorkItem as non-Sendable but we own the lifecycle.
    private nonisolated(unsafe) var settleWorkItem: DispatchWorkItem?
    private let settleDelay: DispatchTimeInterval = .milliseconds(150)

    init(containerView: NSView) {
        self.containerView = containerView
        let center = NotificationCenter.default
        center.addObserver(
            self,
            selector: #selector(scheduleSettle),
            name: NSSplitView.didResizeSubviewsNotification,
            object: nil
        )
        center.addObserver(
            self,
            selector: #selector(windowDidEndLiveResize(_:)),
            name: NSWindow.didEndLiveResizeNotification,
            object: nil
        )
    }

    deinit {
        settleWorkItem?.cancel()
        NotificationCenter.default.removeObserver(self)
    }

    @objc private func windowDidEndLiveResize(_ notification: Notification) {
        // Only resync when the resize is our own window's.
        guard let containerView,
              let window = containerView.window,
              let notifWindow = notification.object as? NSWindow,
              window === notifWindow
        else { return }
        scheduleSettle()
    }

    @objc private func scheduleSettle() {
        settleWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.performSettle()
        }
        settleWorkItem = workItem
        DispatchQueue.main.asyncAfter(
            deadline: .now() + settleDelay,
            execute: workItem
        )
    }

    private func performSettle() {
        guard let containerView else { return }
        for view in allSubviews(of: containerView) {
            if let terminalView = view as? AppTerminalView {
                terminalView.fitToSize()
            }
        }
    }

    private func allSubviews(of view: NSView) -> [NSView] {
        var result: [NSView] = []
        for subview in view.subviews {
            result.append(subview)
            result.append(contentsOf: allSubviews(of: subview))
        }
        return result
    }
}
