import Testing
@testable import TillerACP

struct AgentDriverTests {
    @Test func acpSessionConformsToAgentDriver() async throws {
        let transport = MockTransport()
        let session = ACPSession(
            client: ACPClient(transport: transport),
            fileSystem: NullFileSystem())
        let driver: any AgentDriver = session   // compiles only with conformance
        _ = driver.events                        // protocol exposes the stream
    }
}
