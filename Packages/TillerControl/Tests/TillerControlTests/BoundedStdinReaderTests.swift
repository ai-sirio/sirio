import Testing
import Foundation
@testable import tillerctl

@Test func returnsFullPayloadOnceWriterClosesPromptly() {
    let pipe = Pipe()
    let payload = Data(#"{"session_id":"abc"}"#.utf8)
    pipe.fileHandleForWriting.write(payload)
    try? pipe.fileHandleForWriting.close()

    let result = readBoundedStdin(pipe.fileHandleForReading, timeoutSeconds: 2)
    #expect(result == payload)
}

@Test func stopsWaitingWhenWriterNeverClosesOrSendsData() {
    let pipe = Pipe()
    // Write end intentionally left open for the whole test — reproduces the hook-runner bug
    // where the peer of tillerctl's stdin pipe is never closed.
    let start = Date()
    let result = readBoundedStdin(pipe.fileHandleForReading, timeoutSeconds: 0.2)
    let elapsed = Date().timeIntervalSince(start)

    #expect(result.isEmpty)
    #expect(elapsed < 1.0)
    withExtendedLifetime(pipe) {}
}

@Test func returnsPartialDataWhenWriterStallsAfterFirstChunk() {
    let pipe = Pipe()
    let chunk = Data("partial".utf8)
    pipe.fileHandleForWriting.write(chunk)
    // No close — writer stalls indefinitely from here.

    let start = Date()
    let result = readBoundedStdin(pipe.fileHandleForReading, timeoutSeconds: 0.2)
    let elapsed = Date().timeIntervalSince(start)

    #expect(result == chunk)
    #expect(elapsed < 1.0)
    withExtendedLifetime(pipe) {}
}
