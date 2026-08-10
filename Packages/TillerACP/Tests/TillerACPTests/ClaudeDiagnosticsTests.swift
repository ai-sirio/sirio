import Testing
import Foundation
@testable import TillerACP

@Suite struct ClaudeDiagnosticsTests {
    private func event(_ json: String) throws -> ClaudeSystemEvent {
        guard case .systemEvent(let event) = try JSONDecoder()
            .decode(ClaudeWireMessage.self, from: Data(json.utf8)) else {
            throw DecodingError.dataCorrupted(
                .init(codingPath: [], debugDescription: "not a system event"))
        }
        return event
    }

    @Test func mapsHookLifecycleToDiagnostics() throws {
        let started = try event(#"{"type":"system","subtype":"hook_started","hook_name":"SessionStart:startup"}"#)
        let diagnostic = ClaudeDiagnostics.diagnostic(for: started)
        #expect(diagnostic?.kind == .hook)
        #expect(diagnostic?.label == "SessionStart:startup")
    }

    @Test func mapsThinkingTokens() throws {
        let tokens = try event(#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":86,"estimated_tokens_delta":36}"#)
        let diagnostic = ClaudeDiagnostics.diagnostic(for: tokens)
        #expect(diagnostic?.kind == .thinkingTokens)
        #expect(diagnostic?.detail == "86")
    }

    @Test func ignoresSubtypesRenderedInline() throws {
        let compact = try event(#"{"type":"system","subtype":"compact_boundary"}"#)
        #expect(ClaudeDiagnostics.diagnostic(for: compact) == nil)
    }
}
