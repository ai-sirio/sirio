import Foundation

/// Coalesces PTY output bursts so Layer-C content-signal detection runs
/// once after output settles, not on every chunk. Same coalescing idiom as
/// `ResizeDebouncer`, generalized to a plain no-argument settle callback —
/// there is no payload to carry, the settle just means "go read the buffer
/// now". `@unchecked Sendable` justification: `pending` is confined to the
/// private serial `queue`, identical to `ResizeDebouncer`.
final class OutputSettleDebouncer: @unchecked Sendable {
    private let queue = DispatchQueue(label: "tiller.pty.output-settle")
    private let interval: DispatchTimeInterval
    private var pending: DispatchWorkItem?
    var onSettle: (@Sendable () -> Void)?

    init(interval: DispatchTimeInterval = .milliseconds(200)) {
        self.interval = interval
    }

    func push() {
        queue.async { [self] in
            pending?.cancel()
            let item = DispatchWorkItem { [self] in
                pending = nil
                onSettle?()
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
