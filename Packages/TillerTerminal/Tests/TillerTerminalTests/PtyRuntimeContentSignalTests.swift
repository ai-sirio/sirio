import Foundation
import Testing
@testable import TillerTerminal

/// The content-signal callback reaches `AppModel.handleContentSignal`
/// (@MainActor) but is stored as a non-isolated closure: delivering it from
/// the settle executor trips Swift 6's dynamic isolation check and crashes
/// (EXC_BREAKPOINT in dispatch_assert_queue, seen in the field).
@Suite struct PtyRuntimeContentSignalTests {
    @Test func contentSignalIsDeliveredOnTheMainThread() async throws {
        let runtime = PtyRuntime()
        await runtime.scrollback.append(Data("hello from the pty\n".utf8))
        let onMain = await withCheckedContinuation { continuation in
            runtime.onContentSignal = { _, _ in
                continuation.resume(returning: Thread.isMainThread)
            }
            Task.detached { await runtime.emitContentSignal() }
        }
        #expect(onMain)
    }
}
