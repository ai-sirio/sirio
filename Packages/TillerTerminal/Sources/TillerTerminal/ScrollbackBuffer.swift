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
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackAppend", id: sid)
        storage.append(data)
        if storage.count > capacity {
            storage.removeFirst(storage.count - capacity)
        }
        SignpostMetrics.endInterval("scrollbackAppend", state, message: "bytes: \(data.count)")
    }

    public func snapshot() -> Data {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackSnapshot", id: sid)
        let result = storage
        SignpostMetrics.endInterval("scrollbackSnapshot", state, message: "bytes: \(result.count)")
        return result
    }
}
