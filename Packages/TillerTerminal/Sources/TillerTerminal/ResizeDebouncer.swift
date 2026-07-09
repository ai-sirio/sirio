import Foundation

/// Coalesces the TIOCSWINSZ storm a live window/divider drag produces.
///
/// Every SwiftUI layout tick resizes the ghostty grid; forwarding each tick
/// to the PTY delivers a SIGWINCH per tick, and zsh repaints its prompt on
/// every one. ghostty's live reflow then rewraps the rows the shell thinks
/// it just erased, leaking a stale prompt copy into the grid per tick
/// ("prompt soup"). Debouncing means the shell sees exactly one settled
/// size per gesture.
///
/// `@unchecked Sendable` justification: `pending` is confined to the
/// private serial `queue`; `onSettle` is assigned once during PtyRuntime
/// init, before the surface exists and therefore before any push can
/// arrive.
final class ResizeDebouncer: @unchecked Sendable {
    private let queue = DispatchQueue(label: "tiller.pty.resize-debounce")
    private let interval: DispatchTimeInterval
    private var pending: DispatchWorkItem?
    var onSettle: (@Sendable (_ cols: UInt16, _ rows: UInt16) -> Void)?

    init(interval: DispatchTimeInterval = .milliseconds(120)) {
        self.interval = interval
    }

    func push(cols: UInt16, rows: UInt16) {
        queue.async { [self] in
            pending?.cancel()
            let item = DispatchWorkItem { [self] in
                pending = nil
                onSettle?(cols, rows)
            }
            pending = item
            queue.asyncAfter(deadline: .now() + interval, execute: item)
        }
    }

    func cancel() {
        queue.async { [self] in
            pending?.cancel()
            pending = nil
        }
    }
}
