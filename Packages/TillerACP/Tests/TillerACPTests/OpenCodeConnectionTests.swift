import Testing
import Foundation
@testable import TillerACP

@Suite struct OpenCodeConnectionTests {
    @Test func parsesServerURL() {
        let url = OpenCodeProcessConnection.parseServerURL(
            fromStdoutLine: "opencode server listening on http://127.0.0.1:53422")
        #expect(url?.absoluteString == "http://127.0.0.1:53422")
    }

    @Test func sessionDoesNotTimeOutDuringLongTurns() {
        // POST /session/{id}/message blocks for the whole agent turn, so the
        // 60s URLSession.shared default aborts any turn longer than a minute.
        let configuration = OpenCodeProcessConnection.sessionConfiguration()
        #expect(configuration.timeoutIntervalForRequest >= 3_600)
    }

    @Test func sseParserHandlesSplitFrames() {
        var parser = SSEParser()
        let first = parser.feed(Data("data: {\"a\":".utf8))
        let second = parser.feed(Data("1}\n\ndata: {\"b\":2}\n\n".utf8))
        #expect(first.isEmpty)
        #expect(second.count == 2)
    }
}
