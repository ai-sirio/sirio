import CoreServices
import Foundation

public final class FileSystemEventMonitor: @unchecked Sendable {
    private let callbackQueue: DispatchQueue
    private let callbackQueueKey: DispatchSpecificKey<Void>

    private final class CallbackBox: @unchecked Sendable {
        let continuation: AsyncStream<[URL]>.Continuation
        let gate: EventCallbackGate

        init(continuation: AsyncStream<[URL]>.Continuation) {
            self.continuation = continuation
            self.gate = EventCallbackGate()
        }

        func deliver(_ urls: [URL]) {
            _ = gate.withDelivery {
                continuation.yield(urls)
            }
        }

        func stop() {
            gate.stop()
        }
    }

    public let events: AsyncStream<[URL]>
    private let lock = NSLock()
    private var stream: FSEventStreamRef?
    private var continuation: AsyncStream<[URL]>.Continuation?
    private var callbackBox: CallbackBox?

    public init?(roots: [URL], latency: TimeInterval = 0.1) {
        guard !roots.isEmpty else { return nil }

        let callbackQueue = DispatchQueue(
            label: "dev.tiller.filesystem-events",
            qos: .utility
        )
        let callbackQueueKey = DispatchSpecificKey<Void>()
        self.callbackQueue = callbackQueue
        self.callbackQueueKey = callbackQueueKey
        callbackQueue.setSpecific(key: callbackQueueKey, value: ())

        let pair = AsyncStream<[URL]>.makeStream()
        self.events = pair.stream
        self.continuation = pair.continuation

        let box = CallbackBox(continuation: pair.continuation)
        let boxInfo = Unmanaged.passRetained(box).toOpaque()
        var context = FSEventStreamContext(
            version: 0,
            info: boxInfo,
            retain: nil,
            release: { info in
                guard let info else { return }
                Unmanaged<CallbackBox>.fromOpaque(info).release()
            },
            copyDescription: nil
        )
        let callback: FSEventStreamCallback = { _, info, count, eventPaths, _, _ in
            guard let info else { return }
            let box = Unmanaged<CallbackBox>.fromOpaque(info).takeUnretainedValue()
            let strings = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] ?? []
            box.deliver(strings.prefix(count).map { URL(fileURLWithPath: $0) })
        }
        let paths = roots.map { $0.standardizedFileURL.path } as CFArray
        let flags = FSEventStreamCreateFlags(
            kFSEventStreamCreateFlagFileEvents
                | kFSEventStreamCreateFlagUseCFTypes
                | kFSEventStreamCreateFlagWatchRoot
        )
        guard let stream = FSEventStreamCreate(
            kCFAllocatorDefault,
            callback,
            &context,
            paths,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            latency,
            flags
        ) else {
            Unmanaged<CallbackBox>.fromOpaque(boxInfo).release()
            pair.continuation.finish()
            return nil
        }
        FSEventStreamSetDispatchQueue(stream, callbackQueue)
        guard FSEventStreamStart(stream) else {
            box.stop()
            FSEventStreamInvalidate(stream)
            if callbackQueue.getSpecific(key: callbackQueueKey) == nil {
                callbackQueue.sync {}
            }
            FSEventStreamRelease(stream)
            pair.continuation.finish()
            return nil
        }
        self.stream = stream
        self.callbackBox = box
    }

    public func stop() {
        lock.lock()
        let current = stream
        stream = nil
        let currentContinuation = continuation
        continuation = nil
        let currentCallbackBox = callbackBox
        callbackBox = nil
        lock.unlock()

        guard let current else { return }
        currentCallbackBox?.stop()
        FSEventStreamStop(current)
        FSEventStreamInvalidate(current)
        if callbackQueue.getSpecific(key: callbackQueueKey) == nil {
            callbackQueue.sync {}
        }
        FSEventStreamRelease(current)
        currentContinuation?.finish()
    }

    deinit { stop() }
}

final class EventCallbackGate: @unchecked Sendable {
    private let lock = NSLock()
    private var stopped = false

    func withDelivery(_ body: () -> Void) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !stopped else { return false }
        body()
        return true
    }

    func stop() {
        lock.lock()
        stopped = true
        lock.unlock()
    }
}
