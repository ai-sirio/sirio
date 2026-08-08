import Testing
@testable import Tiller

struct DiffDragPayloadTests {
    @Test func payloadCarriesTheRelativeGitPath() {
        let payload = DiffDragPayload(path: "Sources/Feature.swift")

        #expect(payload.path == "Sources/Feature.swift")
    }
}
