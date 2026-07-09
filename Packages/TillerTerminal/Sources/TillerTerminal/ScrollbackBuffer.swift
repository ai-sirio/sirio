import Foundation

/// Ring-ish byte buffer: appends raw PTY output and keeps only the last
/// `capacity` bytes. Raw VT stream replay is imperfect (a TUI mid-frame
/// restores oddly) but is the right 1a tradeoff for shell scrollback.
public actor ScrollbackBuffer {
    private var storage = Data()
    private let capacity: Int

    public init(capacity: Int = 256 * 1024) {
        self.capacity = capacity
    }

    public func append(_ data: Data) {
        storage.append(data)
        if storage.count > capacity {
            storage.removeFirst(storage.count - capacity)
        }
    }

    public func snapshot() -> Data { storage }
}
