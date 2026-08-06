import Foundation
import Testing
import TillerCore
@testable import TillerTerminal

/// The bytes a terminal drop writes to the PTY: the insertion string with no
/// trailing newline, because the user decides when to run the command.
@Test func terminalDropWritesQuotedPathsWithoutRunningThem() async {
    let registry = PaneRegistry()
    let paneId = UUID()
    let payload = FileDrop.terminalInsertion([
        URL(fileURLWithPath: "/tmp/a b.txt"),
        URL(fileURLWithPath: "/tmp/c.txt"),
    ])

    #expect(payload == "'/tmp/a b.txt' '/tmp/c.txt'")
    #expect(payload.hasSuffix("\n") == false)
    // No pane is registered, so the write is refused rather than silently
    // going nowhere — the same signal the drop handler relies on.
    #expect(await registry.write(paneId: paneId, data: Data(payload.utf8)) == false)
}
