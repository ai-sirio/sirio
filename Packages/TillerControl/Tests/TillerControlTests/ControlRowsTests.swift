import Testing
@testable import TillerControl

@Suite struct ControlRowsTests {
    @Test func roundTripsRows() {
        let rows = [["b": "2", "a": "1"], ["a": "3", "b": "4"]]
        let json = ControlRows.encode(rows)
        #expect(ControlRows.decode(json) == rows)
    }

    @Test func encodesDeterministically() {
        // sorted keys → stable output for tests and diffing
        #expect(ControlRows.encode([["b": "2", "a": "1"]]) == #"[{"a":"1","b":"2"}]"#)
    }

    @Test func emptyAndGarbage() {
        #expect(ControlRows.encode([]) == "[]")
        #expect(ControlRows.decode("not json") == nil)
    }
}
