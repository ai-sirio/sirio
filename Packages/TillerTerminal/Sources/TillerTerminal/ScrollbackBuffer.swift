import Foundation
import TillerCore

/// Fixed-capacity byte ring buffer: appends raw PTY output and keeps only
/// the last `capacity` bytes. Uses explicit `head` + `count` semantics into
/// a fixed `storage` array, avoiding `Data.removeFirst()` O(n) shifts.
///
/// Thread safety: actor isolation (all methods are async).
public actor ScrollbackBuffer {
    private var storage: Data
    private let capacity: Int
    private var head: Int = 0
    private var count: Int = 0

    public init(capacity: Int = 256 * 1024) {
        self.capacity = capacity
        self.storage = Data(count: capacity)
    }

    public func append(_ data: Data) {
        guard capacity > 0, !data.isEmpty else { return }
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackAppend", id: sid)
        defer { SignpostMetrics.endInterval("scrollbackAppend", state, message: "bytes: \(data.count)") }
        if data.count >= capacity {
            // Data larger than capacity: keep only the last `capacity` bytes.
            let offset = data.count - capacity
            data.withUnsafeBytes { src in
                storage.withUnsafeMutableBytes { dst in
                    dst.copyBytes(from: UnsafeRawBufferPointer(rebasing: src[offset...]))
                }
            }
            head = 0
            count = capacity
            return
        }
        // Normal case: the destination is at most two contiguous regions —
        // from the write cursor to the end of storage, then from the start
        // after the wrap. Copy them in bulk instead of byte by byte.
        let writeStart = (head + count) % capacity
        let firstChunk = min(data.count, capacity - writeStart)
        let secondChunk = data.count - firstChunk
        data.withUnsafeBytes { src in
            storage.withUnsafeMutableBytes { dst in
                UnsafeMutableRawBufferPointer(rebasing: dst[writeStart..<writeStart + firstChunk])
                    .copyBytes(from: UnsafeRawBufferPointer(rebasing: src[..<firstChunk]))
                if secondChunk > 0 {
                    UnsafeMutableRawBufferPointer(rebasing: dst[..<secondChunk])
                        .copyBytes(from: UnsafeRawBufferPointer(rebasing: src[firstChunk...]))
                }
            }
        }
        // Bytes beyond capacity evict the oldest ones by advancing `head`.
        let evicted = max(0, count + data.count - capacity)
        head = (head + evicted) % capacity
        count = min(count + data.count, capacity)
    }

    public func snapshot() -> Data {
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("scrollbackSnapshot", id: sid)
        defer { SignpostMetrics.endInterval("scrollbackSnapshot", state, message: "bytes: \(count)") }
        guard count > 0 else { return Data() }
        if head + count <= capacity {
            return Data(storage[head..<head + count])
        }
        let first = storage[head..<capacity]
        let second = storage[0..<(head + count) % capacity]
        return first + second
    }

    /// Returns the last `maxBytes` bytes as a single `Data` in logical
    /// oldest-to-newest order within the returned window, or fewer if the
    /// buffer contains less data.
    public func tail(_ maxBytes: Int) -> Data {
        guard count > 0, maxBytes > 0 else { return Data() }
        let n = min(maxBytes, count)
        let start = (head + count - n) % capacity
        if start + n <= capacity {
            return Data(storage[start..<start + n])
        }
        let first = storage[start..<capacity]
        let second = storage[0..<(start + n) % capacity]
        return first + second
    }
}
