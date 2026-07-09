import Testing
import Foundation
@testable import TillerControl

@Test func requestRoundTripsThroughFraming() throws {
    let request = ControlRequest(id: "r1", method: "panel.create", params: ["worktree": "W", "cmd": "ls"])
    let line = try ControlFraming.encodeLine(request)
    #expect(line.last == UInt8(ascii: "\n"))
    let decoded = try ControlFraming.decodeRequest(line: line.dropLast())
    #expect(decoded == request)
}

@Test func responseHelpers() throws {
    let ok = ControlResponse.success(id: "r1", result: ["panelId": "P"])
    #expect(ok.ok && ok.result?["panelId"] == "P" && ok.error == nil)
    let ko = ControlResponse.failure(id: "r2", error: "unknown panel")
    #expect(!ko.ok && ko.error == "unknown panel")
    let line = try ControlFraming.encodeLine(ko)
    let decoded = try ControlFraming.decodeResponse(line: line.dropLast())
    #expect(decoded == ko)
}

@Test func socketPathEnvOverrideWins() {
    let path = ControlSocket.defaultPath(environment: ["TILLER_SOCKET": "/tmp/x.sock"])
    #expect(path == "/tmp/x.sock")
    let fallback = ControlSocket.defaultPath(environment: [:])
    #expect(fallback.hasSuffix("Library/Application Support/Tiller/control.sock"))
}
