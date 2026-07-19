# TillerACP Package Implementation Plan (Plan 1 of 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `TillerACP` Swift package — a generic Agent Client Protocol (ACP) client: JSON-RPC 2.0 over stdio, typed protocol messages, a session actor, and a pure transcript reducer — fully unit-tested with a mock transport.

**Architecture:** New leaf package `Packages/TillerACP` (no local dependencies, like TillerGit). Layers: `JSONValue`/`JSONRPC` (wire coding) → typed ACP messages → `ACPTransport` (line-framed stdio, `ProcessTransport` child process) → `ACPClient` actor (request/response correlation + incoming dispatch) → `ACPSession` actor (lifecycle, permissions, fs delegation with worktree guard) → `TranscriptReducer` (pure `session/update` stream → `[TranscriptItem]`). Spec: `docs/superpowers/specs/2026-07-18-chat-interface-acp-design.md`. Plans 2 (persistence/catalog) and 3 (chat UI) build on this.

**Tech Stack:** Swift 6, macOS 15+, swift-testing (`@Test`/`#expect`), Foundation only (no new dependencies).

## Global Constraints

- Swift tools version 6.0, platform `.macOS(.v15)` (match `Packages/TillerCore/Package.swift`).
- Tests use swift-testing (`import Testing`, `@Test`, `#expect`) — never XCTest.
- Domain types are structs/enums (`Sendable`, `Equatable`); classes only for real identity (transport owning a Process), actor-isolated or lock-protected.
- Never hand-edit `Tiller.xcodeproj`. This plan does NOT touch `project.yml` — the App target links TillerACP only in Plan 3. `Scripts/ci.sh` picks up the new package automatically (it iterates `Packages/*/`).
- Conventional Commits, lower-case imperative subject.
- ACP wire format: newline-delimited JSON-RPC 2.0 on stdin/stdout. Protocol version `1`.
- Final gate: `Scripts/ci.sh` prints `CI OK`. Note: `TillerTerminal.spawnCapturesOutput` is a known flaky test — retry ci.sh up to 5-6 times if it is the only failure.
- All public API gets a doc comment (match existing package style).

---

### Task 1: Package scaffold + JSONValue

**Files:**
- Create: `Packages/TillerACP/Package.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/JSONValue.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/JSONValueTests.swift`

**Interfaces:**
- Produces: `JSONValue` enum (`.null/.bool/.number/.string/.array/.object`), `subscript(String) -> JSONValue?`, `stringValue/intValue/boolValue/arrayValue` accessors, `decoded(_:) throws -> T`, `static encoding(_:) throws -> JSONValue`. Every later task uses it for untyped JSON-RPC params.

- [ ] **Step 1: Create the package manifest**

```swift
// Packages/TillerACP/Package.swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerACP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerACP", targets: ["TillerACP"])],
    targets: [
        .target(name: "TillerACP"),
        .testTarget(name: "TillerACPTests", dependencies: ["TillerACP"])
    ]
)
```

- [ ] **Step 2: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/JSONValueTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct JSONValueTests {
    @Test func decodesNestedObject() throws {
        let data = Data(#"{"a":1,"b":"x","c":[true,null],"d":{"e":2.5}}"#.utf8)
        let value = try JSONDecoder().decode(JSONValue.self, from: data)
        #expect(value["a"]?.intValue == 1)
        #expect(value["b"]?.stringValue == "x")
        #expect(value["c"]?.arrayValue?.first?.boolValue == true)
        #expect(value["c"]?.arrayValue?.last == .null)
        #expect(value["d"]?["e"] == .number(2.5))
        #expect(value["missing"] == nil)
    }

    @Test func roundTripsThroughCodable() throws {
        let original: JSONValue = .object([
            "list": .array([.string("a"), .number(1), .bool(false)]),
            "nested": .object(["k": .null])
        ])
        let encoded = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(JSONValue.self, from: encoded)
        #expect(decoded == original)
    }

    struct Sample: Codable, Equatable { var name: String; var count: Int }

    @Test func bridgesTypedValues() throws {
        let value = try JSONValue.encoding(Sample(name: "t", count: 3))
        #expect(value["name"]?.stringValue == "t")
        let back = try value.decoded(Sample.self)
        #expect(back == Sample(name: "t", count: 3))
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test`
Expected: compile FAILURE — `cannot find 'JSONValue' in scope`.

- [ ] **Step 4: Implement JSONValue**

```swift
// Packages/TillerACP/Sources/TillerACP/JSONValue.swift
import Foundation

/// A generic JSON value used for JSON-RPC params/results whose shape is not
/// known statically. Typed ACP messages bridge through it via `decoded(_:)`.
public enum JSONValue: Sendable, Equatable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])
}

extension JSONValue: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let bool = try? container.decode(Bool.self) {
            self = .bool(bool)
        } else if let number = try? container.decode(Double.self) {
            self = .number(number)
        } else if let string = try? container.decode(String.self) {
            self = .string(string)
        } else if let array = try? container.decode([JSONValue].self) {
            self = .array(array)
        } else if let object = try? container.decode([String: JSONValue].self) {
            self = .object(object)
        } else {
            throw DecodingError.dataCorruptedError(
                in: container, debugDescription: "Unsupported JSON value")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        }
    }
}

public extension JSONValue {
    subscript(key: String) -> JSONValue? {
        guard case .object(let dict) = self else { return nil }
        return dict[key]
    }

    var stringValue: String? {
        guard case .string(let value) = self else { return nil }
        return value
    }

    var intValue: Int? {
        guard case .number(let value) = self else { return nil }
        return Int(exactly: value.rounded())
    }

    var boolValue: Bool? {
        guard case .bool(let value) = self else { return nil }
        return value
    }

    var arrayValue: [JSONValue]? {
        guard case .array(let value) = self else { return nil }
        return value
    }

    /// Re-decodes this value as a typed `Decodable`.
    func decoded<T: Decodable>(_ type: T.Type) throws -> T {
        let data = try JSONEncoder().encode(self)
        return try JSONDecoder().decode(type, from: data)
    }

    /// Encodes any `Encodable` into a generic value.
    static func encoding(_ value: some Encodable) throws -> JSONValue {
        let data = try JSONEncoder().encode(value)
        return try JSONDecoder().decode(JSONValue.self, from: data)
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: scaffold tilleracp package with jsonvalue"
```

---

### Task 2: JSON-RPC message coding

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/JSONRPC.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/JSONRPCTests.swift`

**Interfaces:**
- Consumes: `JSONValue` (Task 1).
- Produces: `JSONRPCID` (`.number(Int)`/`.string(String)`, `Hashable`, `Codable`), `JSONRPCError` (`code/message/data`, conforms to `Error`), `JSONRPCMessage` enum with `.request(id:method:params:)`, `.notification(method:params:)`, `.response(id:result:error:)`, plus `static decode(_ line: Data) throws -> JSONRPCMessage` and `encodedLine() throws -> Data` (single JSON line ending in `\n`).

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/JSONRPCTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct JSONRPCTests {
    @Test func decodesRequest() throws {
        let line = Data(#"{"jsonrpc":"2.0","id":3,"method":"fs/read_text_file","params":{"path":"/a"}}"#.utf8)
        let message = try JSONRPCMessage.decode(line)
        #expect(message == .request(id: .number(3), method: "fs/read_text_file",
                                    params: .object(["path": .string("/a")])))
    }

    @Test func decodesNotification() throws {
        let line = Data(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1"}}"#.utf8)
        let message = try JSONRPCMessage.decode(line)
        #expect(message == .notification(method: "session/update",
                                         params: .object(["sessionId": .string("s1")])))
    }

    @Test func decodesResponseAndError() throws {
        let ok = try JSONRPCMessage.decode(Data(#"{"jsonrpc":"2.0","id":"a","result":{"x":1}}"#.utf8))
        #expect(ok == .response(id: .string("a"), result: .object(["x": .number(1)]), error: nil))

        let err = try JSONRPCMessage.decode(Data(#"{"jsonrpc":"2.0","id":4,"error":{"code":-32601,"message":"nope"}}"#.utf8))
        guard case .response(let id, let result, let error) = err else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(4))
        #expect(result == nil)
        #expect(error?.code == -32601)
        #expect(error?.message == "nope")
    }

    @Test func encodedLineRoundTrips() throws {
        let original = JSONRPCMessage.request(
            id: .number(7), method: "initialize",
            params: .object(["protocolVersion": .number(1)]))
        let line = try original.encodedLine()
        #expect(line.last == UInt8(ascii: "\n"))
        #expect(try JSONRPCMessage.decode(line.dropLast()) == original)
    }

    @Test func rejectsGarbage() {
        #expect(throws: (any Error).self) {
            _ = try JSONRPCMessage.decode(Data("not json".utf8))
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter JSONRPCTests`
Expected: compile FAILURE — `cannot find 'JSONRPCMessage' in scope`.

- [ ] **Step 3: Implement JSON-RPC coding**

```swift
// Packages/TillerACP/Sources/TillerACP/JSONRPC.swift
import Foundation

/// JSON-RPC 2.0 request/response identifier (number or string).
public enum JSONRPCID: Sendable, Hashable {
    case number(Int)
    case string(String)
}

extension JSONRPCID: Codable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let number = try? container.decode(Int.self) {
            self = .number(number)
        } else {
            self = .string(try container.decode(String.self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .number(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        }
    }
}

/// JSON-RPC 2.0 error object.
public struct JSONRPCError: Error, Sendable, Equatable, Codable {
    public var code: Int
    public var message: String
    public var data: JSONValue?

    public init(code: Int, message: String, data: JSONValue? = nil) {
        self.code = code
        self.message = message
        self.data = data
    }
}

/// One decoded JSON-RPC 2.0 wire message (a single newline-delimited line).
public enum JSONRPCMessage: Sendable, Equatable {
    case request(id: JSONRPCID, method: String, params: JSONValue?)
    case notification(method: String, params: JSONValue?)
    case response(id: JSONRPCID, result: JSONValue?, error: JSONRPCError?)
}

public extension JSONRPCMessage {
    struct DecodeFailure: Error { public let reason: String }

    static func decode(_ line: Data) throws -> JSONRPCMessage {
        let value = try JSONDecoder().decode(JSONValue.self, from: line)
        guard case .object(let fields) = value else {
            throw DecodeFailure(reason: "top-level JSON is not an object")
        }
        let id = try fields["id"].map { try $0.decoded(JSONRPCID.self) }
        if case .string(let method)? = fields["method"] {
            if let id {
                return .request(id: id, method: method, params: fields["params"])
            }
            return .notification(method: method, params: fields["params"])
        }
        guard let id else {
            throw DecodeFailure(reason: "message has neither method nor id")
        }
        let error = try fields["error"].map { try $0.decoded(JSONRPCError.self) }
        return .response(id: id, result: fields["result"], error: error)
    }

    /// Encodes the message as one JSON line terminated by `\n`.
    func encodedLine() throws -> Data {
        var fields: [String: JSONValue] = ["jsonrpc": .string("2.0")]
        switch self {
        case .request(let id, let method, let params):
            fields["id"] = try JSONValue.encoding(id)
            fields["method"] = .string(method)
            fields["params"] = params
        case .notification(let method, let params):
            fields["method"] = .string(method)
            fields["params"] = params
        case .response(let id, let result, let error):
            fields["id"] = try JSONValue.encoding(id)
            fields["result"] = result
            fields["error"] = try error.map { try JSONValue.encoding($0) }
        }
        var data = try JSONEncoder().encode(JSONValue.object(fields))
        data.append(UInt8(ascii: "\n"))
        return data
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter JSONRPCTests`
Expected: PASS (5 tests). Then run the full package (`swift test`) — all pass.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add json-rpc 2.0 message coding to tilleracp"
```

---

### Task 3: ACP request/response types

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ACPTypes.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/ContentBlock.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ACPTypesTests.swift`

**Interfaces:**
- Consumes: `JSONValue` (Task 1).
- Produces (all `Codable`, `Equatable`, `Sendable`): `FileSystemCapability{readTextFile,writeTextFile}`, `ClientCapabilities{fs,terminal}`, `PromptCapabilities{image,audio,embeddedContext}`, `AgentCapabilities{loadSession,promptCapabilities}`, `AuthMethod{id,name,description?}`, `InitializeParams{protocolVersion,clientCapabilities}`, `InitializeResult{protocolVersion,agentCapabilities,authMethods}`, `NewSessionParams{cwd,mcpServers:[JSONValue]}`, `SessionMode{id,name}`, `SessionModeState{currentModeId,availableModes}`, `NewSessionResult{sessionId,modes?}`, `LoadSessionParams{sessionId,cwd,mcpServers}`, `PromptParams{sessionId,prompt:[ContentBlock]}`, `StopReason` (raw `end_turn|max_tokens|max_turn_requests|refusal|cancelled`), `PromptResult{stopReason}`, `CancelParams{sessionId}`, `SetModeParams{sessionId,modeId}`, `ContentBlock` (`.text/.image/.resourceLink/.resource/.unknown`).

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ACPTypesTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct ACPTypesTests {
    @Test func decodesInitializeResultWithDefaults() throws {
        // Minimal agent answer: absent capabilities default to false/empty.
        let json = Data(#"{"protocolVersion":1,"agentCapabilities":{"loadSession":true}}"#.utf8)
        let result = try JSONDecoder().decode(InitializeResult.self, from: json)
        #expect(result.protocolVersion == 1)
        #expect(result.agentCapabilities.loadSession == true)
        #expect(result.agentCapabilities.promptCapabilities.image == false)
        #expect(result.agentCapabilities.promptCapabilities.embeddedContext == false)
        #expect(result.authMethods.isEmpty)
    }

    @Test func encodesInitializeParams() throws {
        let params = InitializeParams(
            protocolVersion: 1,
            clientCapabilities: ClientCapabilities(
                fs: FileSystemCapability(readTextFile: true, writeTextFile: true),
                terminal: false))
        let value = try JSONValue.encoding(params)
        #expect(value["protocolVersion"]?.intValue == 1)
        #expect(value["clientCapabilities"]?["fs"]?["readTextFile"]?.boolValue == true)
        #expect(value["clientCapabilities"]?["terminal"]?.boolValue == false)
    }

    @Test func decodesNewSessionResultWithModes() throws {
        let json = Data("""
        {"sessionId":"sess-1","modes":{"currentModeId":"default",
         "availableModes":[{"id":"default","name":"Default"},{"id":"plan","name":"Plan"}]}}
        """.utf8)
        let result = try JSONDecoder().decode(NewSessionResult.self, from: json)
        #expect(result.sessionId == "sess-1")
        #expect(result.modes?.currentModeId == "default")
        #expect(result.modes?.availableModes.count == 2)
    }

    @Test func decodesStopReason() throws {
        let result = try JSONDecoder().decode(
            PromptResult.self, from: Data(#"{"stopReason":"end_turn"}"#.utf8))
        #expect(result.stopReason == .endTurn)
    }

    @Test func contentBlockVariantsRoundTrip() throws {
        let blocks: [ContentBlock] = [
            .text("hello"),
            .image(mimeType: "image/png", data: "aGk="),
            .resourceLink(uri: "file:///w/App/A.swift", name: "A.swift"),
            .resource(uri: "file:///w/App/A.swift", text: "let x = 1"),
        ]
        let data = try JSONEncoder().encode(blocks)
        let back = try JSONDecoder().decode([ContentBlock].self, from: data)
        #expect(back == blocks)
    }

    @Test func contentBlockUnknownTypeIsTolerated() throws {
        let json = Data(#"[{"type":"audio","data":"...","mimeType":"audio/wav"}]"#.utf8)
        let blocks = try JSONDecoder().decode([ContentBlock].self, from: json)
        #expect(blocks == [.unknown(type: "audio")])
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ACPTypesTests`
Expected: compile FAILURE — `cannot find 'InitializeResult' in scope`.

- [ ] **Step 3: Implement ContentBlock**

```swift
// Packages/TillerACP/Sources/TillerACP/ContentBlock.swift
import Foundation

/// One prompt/output content block, discriminated by `type` on the wire.
/// Unrecognized types decode as `.unknown` so protocol growth never breaks us.
public enum ContentBlock: Sendable, Equatable {
    case text(String)
    case image(mimeType: String, data: String)
    case resourceLink(uri: String, name: String)
    case resource(uri: String, text: String)
    case unknown(type: String)
}

extension ContentBlock: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, text, mimeType, data, uri, name, resource
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "text":
            self = .text(try container.decode(String.self, forKey: .text))
        case "image":
            self = .image(mimeType: try container.decode(String.self, forKey: .mimeType),
                          data: try container.decode(String.self, forKey: .data))
        case "resource_link":
            self = .resourceLink(uri: try container.decode(String.self, forKey: .uri),
                                 name: try container.decode(String.self, forKey: .name))
        case "resource":
            // Embedded resource: {type:"resource", resource:{uri, text}}
            let inner = try container.decode(EmbeddedResource.self, forKey: .resource)
            self = .resource(uri: inner.uri, text: inner.text)
        default:
            self = .unknown(type: type)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .text(let text):
            try container.encode("text", forKey: .type)
            try container.encode(text, forKey: .text)
        case .image(let mimeType, let data):
            try container.encode("image", forKey: .type)
            try container.encode(mimeType, forKey: .mimeType)
            try container.encode(data, forKey: .data)
        case .resourceLink(let uri, let name):
            try container.encode("resource_link", forKey: .type)
            try container.encode(uri, forKey: .uri)
            try container.encode(name, forKey: .name)
        case .resource(let uri, let text):
            try container.encode("resource", forKey: .type)
            try container.encode(EmbeddedResource(uri: uri, text: text), forKey: .resource)
        case .unknown(let type):
            try container.encode(type, forKey: .type)
        }
    }

    private struct EmbeddedResource: Codable {
        var uri: String
        var text: String
    }
}
```

- [ ] **Step 4: Implement the request/response types**

```swift
// Packages/TillerACP/Sources/TillerACP/ACPTypes.swift
import Foundation

// MARK: - initialize

public struct FileSystemCapability: Sendable, Equatable, Codable {
    public var readTextFile: Bool
    public var writeTextFile: Bool
    public init(readTextFile: Bool, writeTextFile: Bool) {
        self.readTextFile = readTextFile
        self.writeTextFile = writeTextFile
    }
}

public struct ClientCapabilities: Sendable, Equatable, Codable {
    public var fs: FileSystemCapability
    public var terminal: Bool
    public init(fs: FileSystemCapability, terminal: Bool) {
        self.fs = fs
        self.terminal = terminal
    }
}

public struct PromptCapabilities: Sendable, Equatable, Codable {
    public var image: Bool
    public var audio: Bool
    public var embeddedContext: Bool

    public init(image: Bool = false, audio: Bool = false, embeddedContext: Bool = false) {
        self.image = image
        self.audio = audio
        self.embeddedContext = embeddedContext
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        image = try container.decodeIfPresent(Bool.self, forKey: .image) ?? false
        audio = try container.decodeIfPresent(Bool.self, forKey: .audio) ?? false
        embeddedContext = try container.decodeIfPresent(Bool.self, forKey: .embeddedContext) ?? false
    }
}

public struct AgentCapabilities: Sendable, Equatable, Codable {
    public var loadSession: Bool
    public var promptCapabilities: PromptCapabilities

    public init(loadSession: Bool = false, promptCapabilities: PromptCapabilities = .init()) {
        self.loadSession = loadSession
        self.promptCapabilities = promptCapabilities
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        loadSession = try container.decodeIfPresent(Bool.self, forKey: .loadSession) ?? false
        promptCapabilities = try container.decodeIfPresent(
            PromptCapabilities.self, forKey: .promptCapabilities) ?? .init()
    }
}

public struct AuthMethod: Sendable, Equatable, Codable {
    public var id: String
    public var name: String
    public var description: String?
}

public struct InitializeParams: Sendable, Equatable, Codable {
    public var protocolVersion: Int
    public var clientCapabilities: ClientCapabilities
    public init(protocolVersion: Int, clientCapabilities: ClientCapabilities) {
        self.protocolVersion = protocolVersion
        self.clientCapabilities = clientCapabilities
    }
}

public struct InitializeResult: Sendable, Equatable, Codable {
    public var protocolVersion: Int
    public var agentCapabilities: AgentCapabilities
    public var authMethods: [AuthMethod]

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        protocolVersion = try container.decode(Int.self, forKey: .protocolVersion)
        agentCapabilities = try container.decodeIfPresent(
            AgentCapabilities.self, forKey: .agentCapabilities) ?? .init()
        authMethods = try container.decodeIfPresent([AuthMethod].self, forKey: .authMethods) ?? []
    }
}

// MARK: - sessions

public struct NewSessionParams: Sendable, Equatable, Codable {
    public var cwd: String
    public var mcpServers: [JSONValue]
    public init(cwd: String, mcpServers: [JSONValue] = []) {
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}

public struct SessionMode: Sendable, Equatable, Codable {
    public var id: String
    public var name: String
    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

public struct SessionModeState: Sendable, Equatable, Codable {
    public var currentModeId: String
    public var availableModes: [SessionMode]
    public init(currentModeId: String, availableModes: [SessionMode]) {
        self.currentModeId = currentModeId
        self.availableModes = availableModes
    }
}

public struct NewSessionResult: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modes: SessionModeState?
}

public struct LoadSessionParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var cwd: String
    public var mcpServers: [JSONValue]
    public init(sessionId: String, cwd: String, mcpServers: [JSONValue] = []) {
        self.sessionId = sessionId
        self.cwd = cwd
        self.mcpServers = mcpServers
    }
}

/// `session/load` result: same optional modes payload as `session/new`.
public struct LoadSessionResult: Sendable, Equatable, Codable {
    public var modes: SessionModeState?
}

// MARK: - prompting

public struct PromptParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var prompt: [ContentBlock]
    public init(sessionId: String, prompt: [ContentBlock]) {
        self.sessionId = sessionId
        self.prompt = prompt
    }
}

public enum StopReason: String, Sendable, Equatable, Codable {
    case endTurn = "end_turn"
    case maxTokens = "max_tokens"
    case maxTurnRequests = "max_turn_requests"
    case refusal
    case cancelled
}

public struct PromptResult: Sendable, Equatable, Codable {
    public var stopReason: StopReason
}

public struct CancelParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public init(sessionId: String) { self.sessionId = sessionId }
}

public struct SetModeParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var modeId: String
    public init(sessionId: String, modeId: String) {
        self.sessionId = sessionId
        self.modeId = modeId
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ACPTypesTests`
Expected: PASS (6 tests).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add acp handshake and session message types"
```

---

### Task 4: Session updates, tool calls, permissions, fs types

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ToolCall.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/Permission.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/FileSystemMessages.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift`

**Interfaces:**
- Consumes: `JSONValue`, `ContentBlock`.
- Produces: `ToolKind` (raw `read|edit|delete|move|search|execute|think|fetch|switch_mode|other`, unknown→`.other`), `ToolCallStatus` (raw `pending|in_progress|completed|failed`), `ToolCallLocation{path,line?}`, `ToolCallContent` (`.content(ContentBlock)`/`.diff(path:oldText:newText:)`/`.unknown(type:)`), `ToolCall{toolCallId,title,kind,status,content,locations,rawInput?}`, `ToolCallUpdate{toolCallId, title?,kind?,status?,content?,locations?,rawInput?}`, `SessionNotification{sessionId,update}`, `SessionUpdate` enum (`.userMessageChunk/.agentMessageChunk/.agentThoughtChunk(ContentBlock)`, `.toolCall(ToolCall)`, `.toolCallUpdate(ToolCallUpdate)`, `.plan([PlanEntry])`, `.availableCommandsUpdate([AvailableCommand])`, `.currentModeUpdate(String)`, `.unknown(String)`), `PlanEntry{content,priority,status}`, `AvailableCommand{name,description}`, `PermissionOptionKind` (raw `allow_once|allow_always|reject_once|reject_always`), `PermissionOption{optionId,name,kind}`, `RequestPermissionParams{sessionId,toolCall:ToolCallUpdate,options}`, `PermissionOutcome` (`.selected(optionId:)`/`.cancelled`), `RequestPermissionResult{outcome}`, `ReadTextFileParams{sessionId,path,line?,limit?}`, `ReadTextFileResult{content}`, `WriteTextFileParams{sessionId,path,content}`.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct SessionUpdateTests {
    private func decode(_ json: String) throws -> SessionNotification {
        try JSONDecoder().decode(SessionNotification.self, from: Data(json.utf8))
    }

    @Test func decodesAgentMessageChunk() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk",
         "content":{"type":"text","text":"Hello"}}}
        """)
        #expect(note.sessionId == "s1")
        #expect(note.update == .agentMessageChunk(.text("Hello")))
    }

    @Test func decodesToolCallWithDiffAndLocation() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"tc1",
         "title":"Edit Fetcher.swift","kind":"edit","status":"pending",
         "content":[{"type":"diff","path":"/w/Fetcher.swift","oldText":"a","newText":"b"}],
         "locations":[{"path":"/w/Fetcher.swift","line":12}]}}
        """)
        guard case .toolCall(let call) = note.update else {
            Issue.record("expected toolCall"); return
        }
        #expect(call.toolCallId == "tc1")
        #expect(call.kind == .edit)
        #expect(call.status == .pending)
        #expect(call.content == [.diff(path: "/w/Fetcher.swift", oldText: "a", newText: "b")])
        #expect(call.locations == [ToolCallLocation(path: "/w/Fetcher.swift", line: 12)])
    }

    @Test func decodesToolCallUpdateWithPartialFields() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call_update",
         "toolCallId":"tc1","status":"completed"}}
        """)
        guard case .toolCallUpdate(let update) = note.update else {
            Issue.record("expected toolCallUpdate"); return
        }
        #expect(update.toolCallId == "tc1")
        #expect(update.status == .completed)
        #expect(update.title == nil)
        #expect(update.content == nil)
    }

    @Test func decodesPlanCommandsAndMode() throws {
        let plan = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"plan","entries":
         [{"content":"Add retry","priority":"high","status":"in_progress"}]}}
        """)
        #expect(plan.update == .plan([PlanEntry(content: "Add retry", priority: "high",
                                                status: "in_progress")]))

        let commands = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"available_commands_update",
         "availableCommands":[{"name":"init","description":"Set up project"}]}}
        """)
        #expect(commands.update == .availableCommandsUpdate(
            [AvailableCommand(name: "init", description: "Set up project")]))

        let mode = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"current_mode_update","currentModeId":"plan"}}
        """)
        #expect(mode.update == .currentModeUpdate("plan"))
    }

    @Test func unknownUpdateAndKindAreTolerated() throws {
        let note = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"totally_new_thing","x":1}}
        """)
        #expect(note.update == .unknown("totally_new_thing"))

        let weirdKind = try decode("""
        {"sessionId":"s1","update":{"sessionUpdate":"tool_call","toolCallId":"tc2",
         "title":"?","kind":"quantum","status":"pending"}}
        """)
        guard case .toolCall(let call) = weirdKind.update else {
            Issue.record("expected toolCall"); return
        }
        #expect(call.kind == .other)
        #expect(call.content.isEmpty)
    }

    @Test func permissionRequestDecodesAndOutcomeEncodes() throws {
        let params = try JSONDecoder().decode(RequestPermissionParams.self, from: Data("""
        {"sessionId":"s1","toolCall":{"toolCallId":"tc1"},
         "options":[{"optionId":"allow","name":"Allow","kind":"allow_once"},
                    {"optionId":"reject","name":"Reject","kind":"reject_once"}]}
        """.utf8))
        #expect(params.toolCall.toolCallId == "tc1")
        #expect(params.options.map(\.kind) == [.allowOnce, .rejectOnce])

        let selected = try JSONValue.encoding(
            RequestPermissionResult(outcome: .selected(optionId: "allow")))
        #expect(selected["outcome"]?["outcome"]?.stringValue == "selected")
        #expect(selected["outcome"]?["optionId"]?.stringValue == "allow")

        let cancelled = try JSONValue.encoding(RequestPermissionResult(outcome: .cancelled))
        #expect(cancelled["outcome"]?["outcome"]?.stringValue == "cancelled")
    }

    @Test func fsMessagesRoundTrip() throws {
        let read = try JSONDecoder().decode(ReadTextFileParams.self, from: Data(
            #"{"sessionId":"s1","path":"/w/a.swift","line":10,"limit":50}"#.utf8))
        #expect(read.path == "/w/a.swift")
        #expect(read.line == 10)

        let write = try JSONDecoder().decode(WriteTextFileParams.self, from: Data(
            #"{"sessionId":"s1","path":"/w/a.swift","content":"let x = 1"}"#.utf8))
        #expect(write.content == "let x = 1")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter SessionUpdateTests`
Expected: compile FAILURE — `cannot find 'SessionNotification' in scope`.

- [ ] **Step 3: Implement tool call types**

```swift
// Packages/TillerACP/Sources/TillerACP/ToolCall.swift
import Foundation

/// Category the agent assigns to a tool call; drives the card icon.
public enum ToolKind: String, Sendable, Equatable, Codable {
    case read, edit, delete, move, search, execute, think, fetch
    case switchMode = "switch_mode"
    case other

    public init(from decoder: Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = ToolKind(rawValue: raw) ?? .other
    }
}

public enum ToolCallStatus: String, Sendable, Equatable, Codable {
    case pending
    case inProgress = "in_progress"
    case completed
    case failed
}

public struct ToolCallLocation: Sendable, Equatable, Codable {
    public var path: String
    public var line: Int?
    public init(path: String, line: Int? = nil) {
        self.path = path
        self.line = line
    }
}

/// Tool call output, discriminated by `type`: nested content block or a diff.
public enum ToolCallContent: Sendable, Equatable {
    case content(ContentBlock)
    case diff(path: String, oldText: String?, newText: String)
    case unknown(type: String)
}

extension ToolCallContent: Codable {
    private enum CodingKeys: String, CodingKey {
        case type, content, path, oldText, newText
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "content":
            self = .content(try container.decode(ContentBlock.self, forKey: .content))
        case "diff":
            self = .diff(path: try container.decode(String.self, forKey: .path),
                         oldText: try container.decodeIfPresent(String.self, forKey: .oldText),
                         newText: try container.decode(String.self, forKey: .newText))
        default:
            self = .unknown(type: type)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .content(let block):
            try container.encode("content", forKey: .type)
            try container.encode(block, forKey: .content)
        case .diff(let path, let oldText, let newText):
            try container.encode("diff", forKey: .type)
            try container.encode(path, forKey: .path)
            try container.encodeIfPresent(oldText, forKey: .oldText)
            try container.encode(newText, forKey: .newText)
        case .unknown(let type):
            try container.encode(type, forKey: .type)
        }
    }
}

public struct ToolCall: Sendable, Equatable, Codable {
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var status: ToolCallStatus
    public var content: [ToolCallContent]
    public var locations: [ToolCallLocation]
    public var rawInput: JSONValue?

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], rawInput: JSONValue? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        toolCallId = try container.decode(String.self, forKey: .toolCallId)
        title = try container.decode(String.self, forKey: .title)
        kind = try container.decodeIfPresent(ToolKind.self, forKey: .kind) ?? .other
        status = try container.decodeIfPresent(ToolCallStatus.self, forKey: .status) ?? .pending
        content = try container.decodeIfPresent([ToolCallContent].self, forKey: .content) ?? []
        locations = try container.decodeIfPresent([ToolCallLocation].self, forKey: .locations) ?? []
        rawInput = try container.decodeIfPresent(JSONValue.self, forKey: .rawInput)
    }
}

/// Partial tool call: every field except the id is optional.
public struct ToolCallUpdate: Sendable, Equatable, Codable {
    public var toolCallId: String
    public var title: String?
    public var kind: ToolKind?
    public var status: ToolCallStatus?
    public var content: [ToolCallContent]?
    public var locations: [ToolCallLocation]?
    public var rawInput: JSONValue?

    public init(toolCallId: String, title: String? = nil, kind: ToolKind? = nil,
                status: ToolCallStatus? = nil, content: [ToolCallContent]? = nil,
                locations: [ToolCallLocation]? = nil, rawInput: JSONValue? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.rawInput = rawInput
    }
}
```

- [ ] **Step 4: Implement session updates, permissions, fs messages**

```swift
// Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift
import Foundation

public struct PlanEntry: Sendable, Equatable, Codable {
    public var content: String
    public var priority: String
    public var status: String
    public init(content: String, priority: String, status: String) {
        self.content = content
        self.priority = priority
        self.status = status
    }
}

public struct AvailableCommand: Sendable, Equatable, Codable {
    public var name: String
    public var description: String
    public init(name: String, description: String) {
        self.name = name
        self.description = description
    }
}

/// One `session/update` payload, discriminated by `sessionUpdate` on the wire.
public enum SessionUpdate: Sendable, Equatable {
    case userMessageChunk(ContentBlock)
    case agentMessageChunk(ContentBlock)
    case agentThoughtChunk(ContentBlock)
    case toolCall(ToolCall)
    case toolCallUpdate(ToolCallUpdate)
    case plan([PlanEntry])
    case availableCommandsUpdate([AvailableCommand])
    case currentModeUpdate(String)
    case unknown(String)
}

extension SessionUpdate: Decodable {
    private enum CodingKeys: String, CodingKey {
        case sessionUpdate, content, entries, availableCommands, currentModeId
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let discriminator = try container.decode(String.self, forKey: .sessionUpdate)
        switch discriminator {
        case "user_message_chunk":
            self = .userMessageChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "agent_message_chunk":
            self = .agentMessageChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "agent_thought_chunk":
            self = .agentThoughtChunk(try container.decode(ContentBlock.self, forKey: .content))
        case "tool_call":
            self = .toolCall(try ToolCall(from: decoder))
        case "tool_call_update":
            self = .toolCallUpdate(try ToolCallUpdate(from: decoder))
        case "plan":
            self = .plan(try container.decode([PlanEntry].self, forKey: .entries))
        case "available_commands_update":
            self = .availableCommandsUpdate(
                try container.decode([AvailableCommand].self, forKey: .availableCommands))
        case "current_mode_update":
            self = .currentModeUpdate(try container.decode(String.self, forKey: .currentModeId))
        default:
            self = .unknown(discriminator)
        }
    }
}

/// Params of the `session/update` notification.
public struct SessionNotification: Sendable, Equatable, Decodable {
    public var sessionId: String
    public var update: SessionUpdate
}
```

```swift
// Packages/TillerACP/Sources/TillerACP/Permission.swift
import Foundation

public enum PermissionOptionKind: String, Sendable, Equatable, Codable {
    case allowOnce = "allow_once"
    case allowAlways = "allow_always"
    case rejectOnce = "reject_once"
    case rejectAlways = "reject_always"
}

public struct PermissionOption: Sendable, Equatable, Codable {
    public var optionId: String
    public var name: String
    public var kind: PermissionOptionKind
    public init(optionId: String, name: String, kind: PermissionOptionKind) {
        self.optionId = optionId
        self.name = name
        self.kind = kind
    }
}

/// Params of the agent-initiated `session/request_permission` request.
public struct RequestPermissionParams: Sendable, Equatable, Decodable {
    public var sessionId: String
    public var toolCall: ToolCallUpdate
    public var options: [PermissionOption]
}

/// User decision, encoded as `{outcome: {outcome: "selected", optionId}}`
/// or `{outcome: {outcome: "cancelled"}}`.
public enum PermissionOutcome: Sendable, Equatable {
    case selected(optionId: String)
    case cancelled
}

extension PermissionOutcome: Codable {
    private enum CodingKeys: String, CodingKey { case outcome, optionId }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(String.self, forKey: .outcome) {
        case "selected":
            self = .selected(optionId: try container.decode(String.self, forKey: .optionId))
        default:
            self = .cancelled
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .selected(let optionId):
            try container.encode("selected", forKey: .outcome)
            try container.encode(optionId, forKey: .optionId)
        case .cancelled:
            try container.encode("cancelled", forKey: .outcome)
        }
    }
}

public struct RequestPermissionResult: Sendable, Equatable, Codable {
    public var outcome: PermissionOutcome
    public init(outcome: PermissionOutcome) { self.outcome = outcome }
}
```

```swift
// Packages/TillerACP/Sources/TillerACP/FileSystemMessages.swift
import Foundation

/// Params of the agent-initiated `fs/read_text_file` request.
public struct ReadTextFileParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var path: String
    public var line: Int?
    public var limit: Int?
}

public struct ReadTextFileResult: Sendable, Equatable, Codable {
    public var content: String
    public init(content: String) { self.content = content }
}

/// Params of the agent-initiated `fs/write_text_file` request.
public struct WriteTextFileParams: Sendable, Equatable, Codable {
    public var sessionId: String
    public var path: String
    public var content: String
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter SessionUpdateTests`
Expected: PASS (7 tests). Then `swift test` — all pass.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add session update, tool call, permission and fs types"
```

---

### Task 5: TranscriptItem model + reducer for messages and turns

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift`

**Interfaces:**
- Consumes: `SessionUpdate`, `ContentBlock`, `ToolCall*`, `PermissionOption`, `PlanEntry`, `AvailableCommand`, `StopReason`, `JSONRPCID`.
- Produces:
  - `PermissionState{requestId: JSONRPCID, options: [PermissionOption], resolution: Resolution?}` with nested `enum Resolution: Codable, Equatable { case selected(optionId: String); case cancelled }`.
  - `ToolCallItem{toolCallId,title,kind,status,content,locations,permission: PermissionState?}` (`Identifiable`, `id == toolCallId`).
  - `TranscriptItem` enum (`Codable`, `Equatable`, `Sendable`, `Identifiable` with `var id: String`): `.userMessage(id:blocks:[ContentBlock])`, `.agentMessage(id:text:isComplete:)`, `.thought(id:text:)`, `.toolCall(ToolCallItem)`, `.plan(id:entries:[PlanEntry])`.
  - `TranscriptReducer` struct: `items: [TranscriptItem]`, `currentModeId: String?`, `availableCommands: [AvailableCommand]`; mutations `userPrompted(_ blocks:)`, `apply(_ update: SessionUpdate)`, `permissionRequested(requestId:toolCall:options:)`, `permissionResolved(requestId:resolution:)`, `turnEnded(_ reason: StopReason)`.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerTests.swift
import Testing
@testable import TillerACP

@Suite struct TranscriptReducerTests {
    @Test func streamsAgentMessageIntoOneItem() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("hi")])
        reducer.apply(.agentMessageChunk(.text("Hel")))
        reducer.apply(.agentMessageChunk(.text("lo")))
        #expect(reducer.items.count == 2)
        guard case .agentMessage(_, let text, let isComplete) = reducer.items[1] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(text == "Hello")
        #expect(isComplete == false)
    }

    @Test func turnEndCompletesOpenMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("done")))
        reducer.turnEnded(.endTurn)
        guard case .agentMessage(_, _, let isComplete) = reducer.items[0] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(isComplete == true)
    }

    @Test func newUserPromptStartsNewAgentMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("first")))
        reducer.turnEnded(.endTurn)
        reducer.userPrompted([.text("again")])
        reducer.apply(.agentMessageChunk(.text("second")))
        #expect(reducer.items.count == 3)
        guard case .agentMessage(_, let text, _) = reducer.items[2] else {
            Issue.record("expected second agentMessage"); return
        }
        #expect(text == "second")
    }

    @Test func thoughtsAccumulateSeparatelyFromMessages() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentThoughtChunk(.text("hmm ")))
        reducer.apply(.agentThoughtChunk(.text("ok")))
        reducer.apply(.agentMessageChunk(.text("answer")))
        #expect(reducer.items.count == 2)
        guard case .thought(_, let text) = reducer.items[0] else {
            Issue.record("expected thought"); return
        }
        #expect(text == "hmm ok")
    }

    @Test func nonTextChunksAreIgnoredForStreaming() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.image(mimeType: "image/png", data: "x")))
        // Non-text chunks in agent messages are rare; v1 drops them silently.
        #expect(reducer.items.isEmpty)
    }

    @Test func itemIdsAreUniqueAndStable() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("a")])
        reducer.apply(.agentMessageChunk(.text("b")))
        reducer.userPrompted([.text("c")])
        let ids = reducer.items.map(\.id)
        #expect(Set(ids).count == ids.count)
    }

    @Test func replayedUserMessageChunksBecomeUserMessages() {
        // session/load replays the conversation, including the user's turns.
        var reducer = TranscriptReducer()
        reducer.apply(.userMessageChunk(.text("fix ")))
        reducer.apply(.userMessageChunk(.text("bug")))
        reducer.apply(.agentMessageChunk(.text("ok")))
        #expect(reducer.items.count == 2)
        guard case .userMessage(_, let blocks) = reducer.items[0] else {
            Issue.record("expected userMessage"); return
        }
        #expect(blocks == [.text("fix bug")])
    }

    @Test func modeAndCommandsUpdateState() {
        var reducer = TranscriptReducer()
        reducer.apply(.currentModeUpdate("plan"))
        reducer.apply(.availableCommandsUpdate([AvailableCommand(name: "init", description: "d")]))
        #expect(reducer.currentModeId == "plan")
        #expect(reducer.availableCommands.map(\.name) == ["init"])
        #expect(reducer.items.isEmpty)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerTests`
Expected: compile FAILURE — `cannot find 'TranscriptReducer' in scope`.

- [ ] **Step 3: Implement TranscriptItem**

```swift
// Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift
import Foundation

/// Pending or resolved permission attached to a tool call.
public struct PermissionState: Sendable, Equatable, Codable {
    public enum Resolution: Sendable, Equatable, Codable {
        case selected(optionId: String)
        case cancelled
    }

    public var requestId: JSONRPCID
    public var options: [PermissionOption]
    public var resolution: Resolution?

    public init(requestId: JSONRPCID, options: [PermissionOption],
                resolution: Resolution? = nil) {
        self.requestId = requestId
        self.options = options
        self.resolution = resolution
    }

    public var isPending: Bool { resolution == nil }
}

/// A tool call as shown in the transcript, including its permission state.
public struct ToolCallItem: Sendable, Equatable, Codable, Identifiable {
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var status: ToolCallStatus
    public var content: [ToolCallContent]
    public var locations: [ToolCallLocation]
    public var permission: PermissionState?

    public var id: String { toolCallId }

    public init(toolCallId: String, title: String, kind: ToolKind,
                status: ToolCallStatus, content: [ToolCallContent] = [],
                locations: [ToolCallLocation] = [], permission: PermissionState? = nil) {
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.status = status
        self.content = content
        self.locations = locations
        self.permission = permission
    }

    init(_ call: ToolCall) {
        self.init(toolCallId: call.toolCallId, title: call.title, kind: call.kind,
                  status: call.status, content: call.content, locations: call.locations)
    }

    /// Merges the non-nil fields of a partial update.
    mutating func merge(_ update: ToolCallUpdate) {
        if let title = update.title { self.title = title }
        if let kind = update.kind { self.kind = kind }
        if let status = update.status { self.status = status }
        if let content = update.content { self.content = content }
        if let locations = update.locations { self.locations = locations }
    }
}

/// One entry of the chat transcript. Persisted as-is (Codable) by the
/// persistence layer in Plan 2.
public enum TranscriptItem: Sendable, Equatable, Codable, Identifiable {
    case userMessage(id: String, blocks: [ContentBlock])
    case agentMessage(id: String, text: String, isComplete: Bool)
    case thought(id: String, text: String)
    case toolCall(ToolCallItem)
    case plan(id: String, entries: [PlanEntry])

    public var id: String {
        switch self {
        case .userMessage(let id, _): id
        case .agentMessage(let id, _, _): id
        case .thought(let id, _): id
        case .toolCall(let item): item.id
        case .plan(let id, _): id
        }
    }
}
```

- [ ] **Step 4: Implement the reducer (messages/turns portion)**

```swift
// Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift
import Foundation

/// Pure state machine that folds the `session/update` stream (plus permission
/// requests and turn boundaries) into a renderable, persistable transcript.
/// No I/O, no concurrency — fully unit-testable.
public struct TranscriptReducer: Sendable, Equatable {
    public private(set) var items: [TranscriptItem] = []
    public private(set) var currentModeId: String?
    public private(set) var availableCommands: [AvailableCommand] = []

    private var openAgentMessageIndex: Int?
    private var openThoughtIndex: Int?
    private var openUserMessageIndex: Int?
    private var nextOrdinal = 0

    public init() {}

    private mutating func makeId(_ prefix: String) -> String {
        defer { nextOrdinal += 1 }
        return "\(prefix)-\(nextOrdinal)"
    }

    private mutating func closeOpenStreams() {
        if let index = openAgentMessageIndex,
           case .agentMessage(let id, let text, _) = items[index] {
            items[index] = .agentMessage(id: id, text: text, isComplete: true)
        }
        openAgentMessageIndex = nil
        openThoughtIndex = nil
        openUserMessageIndex = nil
    }

    /// Records the user's prompt (called by the session when a turn starts).
    public mutating func userPrompted(_ blocks: [ContentBlock]) {
        closeOpenStreams()
        items.append(.userMessage(id: makeId("user"), blocks: blocks))
    }

    /// Marks the turn finished; open streams complete, pending permissions
    /// attached to tool calls resolve as cancelled.
    public mutating func turnEnded(_ reason: StopReason) {
        closeOpenStreams()
        for index in items.indices {
            guard case .toolCall(var item) = items[index],
                  item.permission?.isPending == true else { continue }
            item.permission?.resolution = .cancelled
            items[index] = .toolCall(item)
        }
    }

    public mutating func apply(_ update: SessionUpdate) {
        switch update {
        case .agentMessageChunk(let block):
            guard case .text(let chunk) = block else { return }
            if let index = openAgentMessageIndex,
               case .agentMessage(let id, let text, false) = items[index] {
                items[index] = .agentMessage(id: id, text: text + chunk, isComplete: false)
            } else {
                items.append(.agentMessage(id: makeId("agent"), text: chunk, isComplete: false))
                openAgentMessageIndex = items.count - 1
            }
            openThoughtIndex = nil
            openUserMessageIndex = nil

        case .agentThoughtChunk(let block):
            guard case .text(let chunk) = block else { return }
            if let index = openThoughtIndex, case .thought(let id, let text) = items[index] {
                items[index] = .thought(id: id, text: text + chunk)
            } else {
                items.append(.thought(id: makeId("thought"), text: chunk))
                openThoughtIndex = items.count - 1
            }
            openAgentMessageIndex = nil
            openUserMessageIndex = nil

        case .userMessageChunk(let block):
            // Live turns never emit these; they arrive when session/load
            // replays the conversation, so they must rebuild user messages.
            guard case .text(let chunk) = block else { return }
            if let index = openUserMessageIndex,
               case .userMessage(let id, var blocks) = items[index] {
                if case .text(let existing) = blocks.last {
                    blocks[blocks.count - 1] = .text(existing + chunk)
                } else {
                    blocks.append(.text(chunk))
                }
                items[index] = .userMessage(id: id, blocks: blocks)
            } else {
                items.append(.userMessage(id: makeId("user"), blocks: [.text(chunk)]))
                openUserMessageIndex = items.count - 1
            }
            openAgentMessageIndex = nil
            openThoughtIndex = nil

        case .toolCall(let call):
            closeOpenStreams()
            upsert(ToolCallItem(call))

        case .toolCallUpdate(let update):
            applyToolCallUpdate(update)

        case .plan(let entries):
            if let index = items.lastIndex(where: {
                if case .plan = $0 { return true } else { return false }
            }), case .plan(let id, _) = items[index] {
                items[index] = .plan(id: id, entries: entries)
            } else {
                items.append(.plan(id: makeId("plan"), entries: entries))
            }

        case .availableCommandsUpdate(let commands):
            availableCommands = commands

        case .currentModeUpdate(let modeId):
            currentModeId = modeId

        case .unknown:
            break
        }
    }

    private mutating func upsert(_ item: ToolCallItem) {
        if let index = toolCallIndex(item.toolCallId),
           case .toolCall(let existing) = items[index] {
            var merged = item
            merged.permission = existing.permission
            items[index] = .toolCall(merged)
        } else {
            items.append(.toolCall(item))
        }
    }

    private mutating func applyToolCallUpdate(_ update: ToolCallUpdate) {
        if let index = toolCallIndex(update.toolCallId),
           case .toolCall(var item) = items[index] {
            item.merge(update)
            items[index] = .toolCall(item)
        } else {
            // Update for a call we never saw (e.g. mid-stream reconnect):
            // materialize a minimal card rather than dropping information.
            var item = ToolCallItem(toolCallId: update.toolCallId, title: update.title ?? "",
                                    kind: update.kind ?? .other,
                                    status: update.status ?? .pending)
            item.merge(update)
            closeOpenStreams()
            items.append(.toolCall(item))
        }
    }

    private func toolCallIndex(_ toolCallId: String) -> Int? {
        items.lastIndex {
            if case .toolCall(let item) = $0 { return item.toolCallId == toolCallId }
            return false
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerTests`
Expected: PASS (8 tests).

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add transcript item model and streaming reducer"
```

---

### Task 6: Reducer — tool call lifecycle and permissions

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift` (add two methods at the end of the struct)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerToolTests.swift`

**Interfaces:**
- Consumes: everything from Task 5.
- Produces: `TranscriptReducer.permissionRequested(requestId: JSONRPCID, toolCall: ToolCallUpdate, options: [PermissionOption])` and `TranscriptReducer.permissionResolved(requestId: JSONRPCID, resolution: PermissionState.Resolution)`.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerToolTests.swift
import Testing
@testable import TillerACP

@Suite struct TranscriptReducerToolTests {
    private let options = [
        PermissionOption(optionId: "y", name: "Allow", kind: .allowOnce),
        PermissionOption(optionId: "n", name: "Reject", kind: .rejectOnce),
    ]

    @Test func toolCallLifecycleUpdatesInPlace() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Read A.swift",
                                         kind: .read, status: .pending)))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "tc1", status: .completed,
            content: [.content(.text("let x = 1"))])))
        #expect(reducer.items.count == 1)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.status == .completed)
        #expect(item.content == [.content(.text("let x = 1"))])
    }

    @Test func toolCallClosesOpenAgentMessage() {
        var reducer = TranscriptReducer()
        reducer.apply(.agentMessageChunk(.text("Let me look…")))
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Read",
                                         kind: .read, status: .pending)))
        reducer.apply(.agentMessageChunk(.text("Found it.")))
        #expect(reducer.items.count == 3)
        guard case .agentMessage(_, let first, let complete) = reducer.items[0] else {
            Issue.record("expected agentMessage"); return
        }
        #expect(first == "Let me look…")
        #expect(complete == true)
    }

    @Test func permissionAttachesToExistingToolCall() {
        var reducer = TranscriptReducer()
        reducer.apply(.toolCall(ToolCall(toolCallId: "tc1", title: "Edit F.swift",
                                         kind: .edit, status: .pending)))
        reducer.permissionRequested(requestId: .number(9),
                                    toolCall: ToolCallUpdate(toolCallId: "tc1"),
                                    options: options)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.isPending == true)
        #expect(item.permission?.options == options)
    }

    @Test func permissionCreatesCardWhenToolCallUnseen() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(2),
            toolCall: ToolCallUpdate(toolCallId: "tcX", title: "Edit Y.swift", kind: .edit),
            options: options)
        #expect(reducer.items.count == 1)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.title == "Edit Y.swift")
        #expect(item.permission?.requestId == .number(2))
    }

    @Test func permissionResolvedRecordsSelection() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(2),
            toolCall: ToolCallUpdate(toolCallId: "tc1", title: "Edit", kind: .edit),
            options: options)
        reducer.permissionResolved(requestId: .number(2),
                                   resolution: .selected(optionId: "y"))
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.resolution == .selected(optionId: "y"))
        #expect(item.permission?.isPending == false)
    }

    @Test func cancelledTurnCancelsPendingPermissions() {
        var reducer = TranscriptReducer()
        reducer.permissionRequested(requestId: .number(5),
            toolCall: ToolCallUpdate(toolCallId: "tc1", title: "Edit", kind: .edit),
            options: options)
        reducer.turnEnded(.cancelled)
        guard case .toolCall(let item) = reducer.items[0] else {
            Issue.record("expected toolCall"); return
        }
        #expect(item.permission?.resolution == .cancelled)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerToolTests`
Expected: compile FAILURE — `value of type 'TranscriptReducer' has no member 'permissionRequested'`.

- [ ] **Step 3: Add the permission methods to TranscriptReducer**

Append inside the `TranscriptReducer` struct, after `applyToolCallUpdate`:

```swift
    /// Attaches a pending permission to its tool call, creating the card from
    /// the request's embedded tool call payload when we never saw the call.
    public mutating func permissionRequested(requestId: JSONRPCID,
                                             toolCall: ToolCallUpdate,
                                             options: [PermissionOption]) {
        applyToolCallUpdate(toolCall)
        guard let index = toolCallIndex(toolCall.toolCallId),
              case .toolCall(var item) = items[index] else { return }
        item.permission = PermissionState(requestId: requestId, options: options)
        items[index] = .toolCall(item)
    }

    /// Records the user's (or cancellation's) answer to a permission request.
    public mutating func permissionResolved(requestId: JSONRPCID,
                                            resolution: PermissionState.Resolution) {
        for index in items.indices {
            guard case .toolCall(var item) = items[index],
                  item.permission?.requestId == requestId,
                  item.permission?.isPending == true else { continue }
            item.permission?.resolution = resolution
            items[index] = .toolCall(item)
            return
        }
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducer`
Expected: PASS (14 tests across both reducer suites).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: handle tool call lifecycle and permissions in reducer"
```

---

### Task 7: Transport — protocol, mock, and ProcessTransport

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ACPTransport.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/ProcessTransport.swift`
- Create: `Packages/TillerACP/Tests/TillerACPTests/MockTransport.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ProcessTransportTests.swift`

**Interfaces:**
- Produces:
  - `protocol ACPTransport: Sendable { func start() async throws; func send(line: Data) async throws; func lines() -> AsyncThrowingStream<Data, Error>; func terminate() async }` — `lines()` yields complete lines WITHOUT the trailing `\n`; single consumer.
  - `ProcessTransport(executable: String, arguments: [String], cwd: String, environment: [String: String]?, onStderrLine: (@Sendable (String) -> Void)?)`.
  - Test-only `MockTransport` actor: `sent: [Data]`, `emit(_ json: String)`, `close()`, `waitForSent(count: Int) async -> [Data]`.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ProcessTransportTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct ProcessTransportTests {
    @Test func catEchoesLinesBack() async throws {
        let transport = ProcessTransport(
            executable: "/bin/cat", arguments: [],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()
        try await transport.send(line: Data("{\"a\":1}\n".utf8))
        try await transport.send(line: Data("{\"b\":2}\n".utf8))

        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
            if received.count == 2 { break }
        }
        #expect(received == ["{\"a\":1}", "{\"b\":2}"])
        await transport.terminate()
    }

    @Test func splitsPartialAndBatchedWrites() async throws {
        // printf writes two lines in one burst and one line without buffering:
        // the transport must reassemble on \n regardless of chunk boundaries.
        let transport = ProcessTransport(
            executable: "/bin/sh",
            arguments: ["-c", #"printf 'one\ntwo\n'; printf 'three\n'"#],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()

        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
        }
        #expect(received == ["one", "two", "three"])
    }

    @Test func streamFinishesOnProcessExit() async throws {
        let transport = ProcessTransport(
            executable: "/usr/bin/true", arguments: [],
            cwd: FileManager.default.temporaryDirectory.path)
        try await transport.start()
        var count = 0
        for try await _ in transport.lines() { count += 1 }
        #expect(count == 0)  // reaching here proves the stream terminated
    }

    @Test func stderrIsForwardedNotMixedIntoLines() async throws {
        let collector = StderrCollector()
        let transport = ProcessTransport(
            executable: "/bin/sh",
            arguments: ["-c", "echo err >&2; echo '{}' "],
            cwd: FileManager.default.temporaryDirectory.path,
            onStderrLine: { line in Task { await collector.append(line) } })
        try await transport.start()
        var received: [String] = []
        for try await line in transport.lines() {
            received.append(String(decoding: line, as: UTF8.self))
        }
        #expect(received == ["{}"])
        // stderr delivery is async; poll briefly.
        for _ in 0..<100 where await collector.lines.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(await collector.lines == ["err"])
    }
}

private actor StderrCollector {
    var lines: [String] = []
    func append(_ line: String) { lines.append(line) }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ProcessTransportTests`
Expected: compile FAILURE — `cannot find 'ProcessTransport' in scope`.

- [ ] **Step 3: Implement the transport protocol**

```swift
// Packages/TillerACP/Sources/TillerACP/ACPTransport.swift
import Foundation

/// Byte transport carrying newline-delimited JSON-RPC lines to/from an agent.
/// `lines()` yields complete lines without the trailing newline and must be
/// consumed by a single reader. The stream finishes when the peer closes.
public protocol ACPTransport: Sendable {
    func start() async throws
    func send(line: Data) async throws
    func lines() -> AsyncThrowingStream<Data, Error>
    func terminate() async
}
```

- [ ] **Step 4: Implement ProcessTransport**

```swift
// Packages/TillerACP/Sources/TillerACP/ProcessTransport.swift
import Foundation

/// ACP transport over a child process's stdin/stdout. Stderr lines are
/// forwarded to `onStderrLine` (diagnostics), never mixed into the protocol.
public final class ProcessTransport: ACPTransport, @unchecked Sendable {
    public struct LaunchFailure: Error { public let underlying: Error }

    private let process = Process()
    private let stdinPipe = Pipe()
    private let stdoutPipe = Pipe()
    private let stderrPipe = Pipe()
    private let onStderrLine: (@Sendable (String) -> Void)?
    private let lock = NSLock()
    private var lineContinuation: AsyncThrowingStream<Data, Error>.Continuation?
    private var stdoutBuffer = Data()
    private var stderrBuffer = Data()

    public init(executable: String, arguments: [String], cwd: String,
                environment: [String: String]? = nil,
                onStderrLine: (@Sendable (String) -> Void)? = nil) {
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.currentDirectoryURL = URL(fileURLWithPath: cwd)
        if let environment { process.environment = environment }
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        self.onStderrLine = onStderrLine
    }

    public func start() async throws {
        stdoutPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStdout(handle.availableData)
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { [weak self] handle in
            self?.consumeStderr(handle.availableData)
        }
        process.terminationHandler = { [weak self] _ in
            guard let self else { return }
            // Drain remaining buffered output, then finish the stream.
            self.consumeStdout(self.stdoutPipe.fileHandleForReading.availableData)
            self.lock.lock()
            let continuation = self.lineContinuation
            self.lineContinuation = nil
            self.lock.unlock()
            continuation?.finish()
        }
        do {
            try process.run()
        } catch {
            throw LaunchFailure(underlying: error)
        }
    }

    public func send(line: Data) async throws {
        try stdinPipe.fileHandleForWriting.write(contentsOf: line)
    }

    public func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            lock.lock()
            lineContinuation = continuation
            let buffered = drainBufferedLinesLocked()
            lock.unlock()
            for line in buffered { continuation.yield(line) }
            if !process.isRunning, process.processIdentifier != 0 {
                continuation.finish()
            }
        }
    }

    public func terminate() async {
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        if process.isRunning { process.terminate() }
    }

    private func consumeStdout(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        stdoutBuffer.append(data)
        let lines = drainBufferedLinesLocked()
        let continuation = lineContinuation
        lock.unlock()
        for line in lines { continuation?.yield(line) }
    }

    /// Caller must hold `lock`. Splits complete lines off `stdoutBuffer`.
    private func drainBufferedLinesLocked() -> [Data] {
        var lines: [Data] = []
        while let newline = stdoutBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            lines.append(stdoutBuffer.subdata(in: stdoutBuffer.startIndex..<newline))
            stdoutBuffer.removeSubrange(stdoutBuffer.startIndex...newline)
        }
        return lines
    }

    private func consumeStderr(_ data: Data) {
        guard !data.isEmpty, let onStderrLine else { return }
        lock.lock()
        stderrBuffer.append(data)
        var lines: [String] = []
        while let newline = stderrBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            let line = stderrBuffer.subdata(in: stderrBuffer.startIndex..<newline)
            lines.append(String(decoding: line, as: UTF8.self))
            stderrBuffer.removeSubrange(stderrBuffer.startIndex...newline)
        }
        lock.unlock()
        for line in lines { onStderrLine(line) }
    }
}
```

- [ ] **Step 5: Implement MockTransport (test target)**

```swift
// Packages/TillerACP/Tests/TillerACPTests/MockTransport.swift
import Foundation
@testable import TillerACP

/// In-memory transport for driving ACPClient/ACPSession tests: records what
/// the client sends, lets the test emit agent lines.
actor MockTransport: ACPTransport {
    private(set) var sent: [Data] = []
    private var continuation: AsyncThrowingStream<Data, Error>.Continuation?
    private var pendingLines: [Data] = []

    func start() {}

    func send(line: Data) {
        sent.append(line)
    }

    nonisolated func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            Task { await self.attach(continuation) }
        }
    }

    private func attach(_ continuation: AsyncThrowingStream<Data, Error>.Continuation) {
        self.continuation = continuation
        for line in pendingLines { continuation.yield(line) }
        pendingLines = []
    }

    /// Emits one agent → client line (a JSON string, no trailing newline).
    func emit(_ json: String) {
        let data = Data(json.utf8)
        if let continuation { continuation.yield(data) } else { pendingLines.append(data) }
    }

    func close() {
        continuation?.finish()
    }

    func terminate() {
        close()
    }

    /// Polls until the client has sent at least `count` lines.
    func waitForSent(count: Int) async throws -> [Data] {
        for _ in 0..<500 {
            if sent.count >= count { return sent }
            try await Task.sleep(for: .milliseconds(5))
        }
        return sent
    }

    /// Decodes the n-th sent line as a JSON-RPC message.
    func sentMessage(_ index: Int) throws -> JSONRPCMessage {
        try JSONRPCMessage.decode(sent[index].last == UInt8(ascii: "\n")
                                  ? sent[index].dropLast() : sent[index])
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd Packages/TillerACP && swift test --filter ProcessTransportTests`
Expected: PASS (4 tests). Then `swift test` — all pass (MockTransport compiles).

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add stdio transport with process and mock implementations"
```

---

### Task 8: ACPClient actor

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ACPClient.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ACPClientTests.swift`

**Interfaces:**
- Consumes: `ACPTransport`, `JSONRPCMessage`, `JSONValue`.
- Produces:
  - `enum ACPIncoming: Sendable { case notification(method: String, params: JSONValue?); case request(id: JSONRPCID, method: String, params: JSONValue?) }`
  - `actor ACPClient`: `init(transport: any ACPTransport)`, `let incoming: AsyncStream<ACPIncoming>`, `func start() async throws`, `func request<R: Decodable>(_ method: String, params: (some Encodable)?, as: R.Type) async throws -> R`, `func notify(_ method: String, params: some Encodable) async throws`, `func respond(to: JSONRPCID, result: some Encodable) async throws`, `func respondError(to: JSONRPCID, code: Int, message: String) async throws`, `func stop() async`.
  - `enum ACPClientError: Error { case transportClosed, agentError(JSONRPCError) }`

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ACPClientTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct ACPClientTests {
    struct Empty: Codable, Equatable {}

    @Test func correlatesRequestAndResponse() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        async let result: NewSessionResult = client.request(
            "session/new", params: NewSessionParams(cwd: "/w"), as: NewSessionResult.self)

        let sent = try await mock.waitForSent(count: 1)
        guard case .request(let id, let method, let params) = try await mock.sentMessage(0) else {
            Issue.record("expected request"); return
        }
        #expect(method == "session/new")
        #expect(params?["cwd"]?.stringValue == "/w")
        #expect(sent.count == 1)

        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"sessionId":"s9"}}"#)
        #expect(try await result.sessionId == "s9")
    }

    @Test func surfacesAgentErrors() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        async let result: NewSessionResult = client.request(
            "session/new", params: NewSessionParams(cwd: "/w"), as: NewSessionResult.self)
        _ = try await mock.waitForSent(count: 1)
        guard case .request(let id, _, _) = try await mock.sentMessage(0) else {
            Issue.record("expected request"); return
        }
        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"error":{"code":-32000,"message":"auth required"}}"#)

        await #expect(throws: ACPClientError.self) { _ = try await result }
    }

    @Test func deliversIncomingNotificationsAndRequests() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()

        await mock.emit(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#)
        await mock.emit(#"{"jsonrpc":"2.0","id":7,"method":"fs/read_text_file","params":{"sessionId":"s1","path":"/w/a"}}"#)

        var iterator = client.incoming.makeAsyncIterator()
        guard case .notification(let method, _)? = await iterator.next() else {
            Issue.record("expected notification"); return
        }
        #expect(method == "session/update")
        guard case .request(let id, let requestMethod, _)? = await iterator.next() else {
            Issue.record("expected request"); return
        }
        #expect(id == .number(7))
        #expect(requestMethod == "fs/read_text_file")
    }

    @Test func respondEncodesResultForGivenId() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()
        try await client.respond(to: .number(7), result: ReadTextFileResult(content: "x"))
        _ = try await mock.waitForSent(count: 1)
        guard case .response(let id, let result, let error) = try await mock.sentMessage(0) else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(7))
        #expect(result?["content"]?.stringValue == "x")
        #expect(error == nil)
    }

    @Test func transportCloseFailsPendingRequests() async throws {
        let mock = MockTransport()
        let client = ACPClient(transport: mock)
        try await client.start()
        async let result: Empty = client.request("session/prompt", params: Empty(), as: Empty.self)
        _ = try await mock.waitForSent(count: 1)
        await mock.close()
        await #expect(throws: ACPClientError.self) { _ = try await result }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ACPClientTests`
Expected: compile FAILURE — `cannot find 'ACPClient' in scope`.

- [ ] **Step 3: Implement ACPClient**

```swift
// Packages/TillerACP/Sources/TillerACP/ACPClient.swift
import Foundation

/// Agent-initiated traffic surfaced to the session layer.
public enum ACPIncoming: Sendable {
    case notification(method: String, params: JSONValue?)
    case request(id: JSONRPCID, method: String, params: JSONValue?)
}

public enum ACPClientError: Error, Sendable {
    case transportClosed
    case agentError(JSONRPCError)
}

/// JSON-RPC endpoint over an ACPTransport: correlates our requests with the
/// agent's responses, and forwards agent-initiated requests/notifications to
/// the `incoming` stream (single consumer: ACPSession).
public actor ACPClient {
    private let transport: any ACPTransport
    private var nextRequestId = 0
    private var pending: [JSONRPCID: CheckedContinuation<JSONValue?, Error>] = [:]
    private var readTask: Task<Void, Never>?
    private let incomingContinuation: AsyncStream<ACPIncoming>.Continuation
    public nonisolated let incoming: AsyncStream<ACPIncoming>

    public init(transport: any ACPTransport) {
        self.transport = transport
        (incoming, incomingContinuation) = AsyncStream.makeStream(of: ACPIncoming.self)
    }

    public func start() async throws {
        try await transport.start()
        readTask = Task { [weak self, transport] in
            do {
                for try await line in transport.lines() {
                    await self?.handle(line: line)
                }
            } catch {}
            await self?.closed()
        }
    }

    public func stop() async {
        readTask?.cancel()
        await transport.terminate()
        closed()
    }

    public func request<R: Decodable>(_ method: String,
                                      params: (some Encodable)? = Optional<JSONValue>.none,
                                      as type: R.Type) async throws -> R {
        nextRequestId += 1
        let id = JSONRPCID.number(nextRequestId)
        let paramsValue = try params.map { try JSONValue.encoding($0) }
        let line = try JSONRPCMessage.request(id: id, method: method, params: paramsValue)
            .encodedLine()
        let result: JSONValue? = try await withCheckedThrowingContinuation { continuation in
            pending[id] = continuation
            Task {
                do { try await transport.send(line: line) }
                catch { self.fail(id: id, error: ACPClientError.transportClosed) }
            }
        }
        return try (result ?? .null).decoded(R.self)
    }

    public func notify(_ method: String, params: some Encodable) async throws {
        let line = try JSONRPCMessage.notification(
            method: method, params: JSONValue.encoding(params)).encodedLine()
        try await transport.send(line: line)
    }

    public func respond(to id: JSONRPCID, result: some Encodable) async throws {
        let line = try JSONRPCMessage.response(
            id: id, result: JSONValue.encoding(result), error: nil).encodedLine()
        try await transport.send(line: line)
    }

    public func respondError(to id: JSONRPCID, code: Int, message: String) async throws {
        let line = try JSONRPCMessage.response(
            id: id, result: nil, error: JSONRPCError(code: code, message: message))
            .encodedLine()
        try await transport.send(line: line)
    }

    private func handle(line: Data) {
        guard let message = try? JSONRPCMessage.decode(line) else { return }
        switch message {
        case .response(let id, let result, let error):
            guard let continuation = pending.removeValue(forKey: id) else { return }
            if let error {
                continuation.resume(throwing: ACPClientError.agentError(error))
            } else {
                continuation.resume(returning: result)
            }
        case .notification(let method, let params):
            incomingContinuation.yield(.notification(method: method, params: params))
        case .request(let id, let method, let params):
            incomingContinuation.yield(.request(id: id, method: method, params: params))
        }
    }

    private func fail(id: JSONRPCID, error: Error) {
        pending.removeValue(forKey: id)?.resume(throwing: error)
    }

    private func closed() {
        for continuation in pending.values {
            continuation.resume(throwing: ACPClientError.transportClosed)
        }
        pending.removeAll()
        incomingContinuation.finish()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter ACPClientTests`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add acp json-rpc client actor"
```

---

### Task 9: ACPSession actor + worktree file system guard

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/ACPFileSystem.swift`
- Create: `Packages/TillerACP/Sources/TillerACP/ACPSession.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/WorktreeFileSystemTests.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift`

**Interfaces:**
- Consumes: `ACPClient`, all typed messages, `TranscriptReducer` (consumer side is Plan 3; the session only emits events).
- Produces:
  - `protocol ACPFileSystem: Sendable { func readTextFile(path: String, line: Int?, limit: Int?) throws -> String; func writeTextFile(path: String, content: String) throws }`
  - `struct WorktreeFileSystem: ACPFileSystem` — `init(root: String)`; throws `PathOutsideWorktree` for any path not under root (after standardizing, symlinks resolved).
  - `struct SessionHandle: Sendable, Equatable { var sessionId: String; var agentCapabilities: AgentCapabilities; var modes: SessionModeState?; var didResume: Bool }`
  - `enum ACPSessionEvent: Sendable { case update(SessionUpdate); case permissionRequested(requestId: JSONRPCID, toolCall: ToolCallUpdate, options: [PermissionOption]); case disconnected }`
  - `actor ACPSession`: `init(client: ACPClient, fileSystem: any ACPFileSystem)`, `nonisolated let events: AsyncStream<ACPSessionEvent>`, `func connect(cwd: String, resumeSessionId: String?) async throws -> SessionHandle`, `func prompt(_ blocks: [ContentBlock]) async throws -> StopReason`, `func cancel() async`, `func setMode(_ modeId: String) async throws`, `func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async`.

- [ ] **Step 1: Write the failing file-system test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/WorktreeFileSystemTests.swift
import Testing
import Foundation
@testable import TillerACP

@Suite struct WorktreeFileSystemTests {
    private func makeWorktree() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-fs-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        return root
    }

    @Test func readsAndWritesInsideWorktree() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        try fs.writeTextFile(path: root.appendingPathComponent("a.txt").path, content: "hello")
        let content = try fs.readTextFile(
            path: root.appendingPathComponent("a.txt").path, line: nil, limit: nil)
        #expect(content == "hello")
    }

    @Test func readSupportsLineAndLimit() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        let path = root.appendingPathComponent("b.txt").path
        try fs.writeTextFile(path: path, content: "l1\nl2\nl3\nl4")
        #expect(try fs.readTextFile(path: path, line: 2, limit: 2) == "l2\nl3")
    }

    @Test func rejectsPathsOutsideWorktree() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        #expect(throws: WorktreeFileSystem.PathOutsideWorktree.self) {
            _ = try fs.readTextFile(path: "/etc/hosts", line: nil, limit: nil)
        }
        #expect(throws: WorktreeFileSystem.PathOutsideWorktree.self) {
            try fs.writeTextFile(path: root.path + "/../evil.txt", content: "x")
        }
    }

    @Test func writeCreatesIntermediateDirectories() throws {
        let root = try makeWorktree()
        let fs = WorktreeFileSystem(root: root.path)
        let nested = root.appendingPathComponent("new/dir/file.txt").path
        try fs.writeTextFile(path: nested, content: "x")
        #expect(try fs.readTextFile(path: nested, line: nil, limit: nil) == "x")
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter WorktreeFileSystemTests`
Expected: compile FAILURE — `cannot find 'WorktreeFileSystem' in scope`.

- [ ] **Step 3: Implement the file system**

```swift
// Packages/TillerACP/Sources/TillerACP/ACPFileSystem.swift
import Foundation

/// Serves the agent's `fs/*` requests. Implementations decide policy;
/// `WorktreeFileSystem` confines all access to one worktree.
public protocol ACPFileSystem: Sendable {
    func readTextFile(path: String, line: Int?, limit: Int?) throws -> String
    func writeTextFile(path: String, content: String) throws
}

/// Disk-backed implementation that refuses any path outside its root —
/// the client-side guardrail from the spec's edge cases.
public struct WorktreeFileSystem: ACPFileSystem {
    public struct PathOutsideWorktree: Error {
        public let path: String
    }

    private let root: String

    public init(root: String) {
        self.root = URL(fileURLWithPath: root).standardizedFileURL
            .resolvingSymlinksInPath().path
    }

    private func validated(_ path: String) throws -> URL {
        let url = URL(fileURLWithPath: path).standardizedFileURL
        // Resolve symlinks on the deepest existing ancestor so a symlink
        // escape inside the path can't slip past the prefix check.
        let resolved = url.resolvingSymlinksInPath()
        guard resolved.path == root || resolved.path.hasPrefix(root + "/") else {
            throw PathOutsideWorktree(path: path)
        }
        return resolved
    }

    public func readTextFile(path: String, line: Int?, limit: Int?) throws -> String {
        let url = try validated(path)
        let content = try String(contentsOf: url, encoding: .utf8)
        guard line != nil || limit != nil else { return content }
        var lines = content.components(separatedBy: "\n")
        if let line, line >= 1 { lines = Array(lines.dropFirst(line - 1)) }
        if let limit { lines = Array(lines.prefix(limit)) }
        return lines.joined(separator: "\n")
    }

    public func writeTextFile(path: String, content: String) throws {
        let url = try validated(path)
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try content.write(to: url, atomically: true, encoding: .utf8)
    }
}
```

Run: `cd Packages/TillerACP && swift test --filter WorktreeFileSystemTests` → PASS (4 tests).

- [ ] **Step 4: Write the failing session test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/ACPSessionTests.swift
import Testing
import Foundation
@testable import TillerACP

/// Scripted fake agent: replies to initialize/session methods like a real
/// adapter so ACPSession's whole lifecycle runs against the mock transport.
private actor FakeAgent {
    let mock = MockTransport()
    var loadSession = false
    var lastPromptStopReason = "end_turn"

    init(loadSession: Bool = false) {
        self.loadSession = loadSession
    }

    /// Watches sent lines and answers protocol requests in the background.
    func run() {
        Task {
            var answered = 0
            while true {
                let sent = await mock.sent
                if sent.count > answered {
                    for index in answered..<sent.count {
                        try? await answer(try mock.sentMessage(index))
                    }
                    answered = sent.count
                }
                try? await Task.sleep(for: .milliseconds(5))
            }
        }
    }

    private func answer(_ message: JSONRPCMessage) async throws {
        guard case .request(let id, let method, _) = message else { return }
        let idJSON = String(decoding: try JSONEncoder().encode(id), as: UTF8.self)
        switch method {
        case "initialize":
            await mock.emit("""
            {"jsonrpc":"2.0","id":\(idJSON),"result":{"protocolVersion":1,
             "agentCapabilities":{"loadSession":\(loadSession)}}}
            """.replacingOccurrences(of: "\n", with: ""))
        case "session/new":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"sessionId":"sess-new"}}"#)
        case "session/load":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{}}"#)
        case "session/prompt":
            await mock.emit(#"{"jsonrpc":"2.0","id":\#(idJSON),"result":{"stopReason":"\#(lastPromptStopReason)"}}"#)
        default:
            break
        }
    }
}

@Suite struct ACPSessionTests {
    private func makeSession(agent: FakeAgent,
                             fileSystem: any ACPFileSystem = NullFileSystem())
    async throws -> ACPSession {
        let client = ACPClient(transport: agent.mock)
        let session = ACPSession(client: client, fileSystem: fileSystem)
        try await session.start()
        await agent.run()
        return session
    }

    @Test func connectCreatesNewSession() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: nil)
        #expect(handle.sessionId == "sess-new")
        #expect(handle.didResume == false)
        #expect(handle.agentCapabilities.loadSession == false)
    }

    @Test func connectResumesWhenSupported() async throws {
        let agent = FakeAgent(loadSession: true)
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: "old-1")
        #expect(handle.sessionId == "old-1")
        #expect(handle.didResume == true)
    }

    @Test func connectIgnoresResumeIdWhenUnsupported() async throws {
        let agent = FakeAgent(loadSession: false)
        let session = try await makeSession(agent: agent)
        let handle = try await session.connect(cwd: "/w", resumeSessionId: "old-1")
        #expect(handle.sessionId == "sess-new")
        #expect(handle.didResume == false)
    }

    @Test func promptReturnsStopReasonAndEmitsUpdates() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        _ = try await session.connect(cwd: "/w", resumeSessionId: nil)

        await agent.mock.emit(#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"sess-new","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hi"}}}}"#)
        let reason = try await session.prompt([.text("hello")])
        #expect(reason == .endTurn)

        var iterator = session.events.makeAsyncIterator()
        guard case .update(let update)? = await iterator.next() else {
            Issue.record("expected update event"); return
        }
        #expect(update == .agentMessageChunk(.text("hi")))
    }

    @Test func permissionRequestRoundTrips() async throws {
        let agent = FakeAgent()
        let session = try await makeSession(agent: agent)
        _ = try await session.connect(cwd: "/w", resumeSessionId: nil)

        await agent.mock.emit("""
        {"jsonrpc":"2.0","id":99,"method":"session/request_permission","params":
         {"sessionId":"sess-new","toolCall":{"toolCallId":"tc1"},
          "options":[{"optionId":"y","name":"Allow","kind":"allow_once"}]}}
        """.replacingOccurrences(of: "\n", with: ""))

        var iterator = session.events.makeAsyncIterator()
        guard case .permissionRequested(let requestId, let toolCall, let options)? =
                await iterator.next() else {
            Issue.record("expected permissionRequested"); return
        }
        #expect(requestId == .number(99))
        #expect(toolCall.toolCallId == "tc1")
        #expect(options.first?.optionId == "y")

        await session.answerPermission(requestId: requestId,
                                       outcome: .selected(optionId: "y"))
        let sent = try await agent.mock.waitForSent(count: 3)  // init, new, answer
        guard case .response(let id, let result, _) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected response"); return
        }
        #expect(id == .number(99))
        #expect(result?["outcome"]?["optionId"]?.stringValue == "y")
    }

    @Test func servesFsReadAndRejectsOutsidePaths() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-sess-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try "content-x".write(to: root.appendingPathComponent("f.txt"),
                              atomically: true, encoding: .utf8)

        let agent = FakeAgent()
        let session = try await makeSession(
            agent: agent, fileSystem: WorktreeFileSystem(root: root.path))
        _ = try await session.connect(cwd: root.path, resumeSessionId: nil)
        let baseline = await agent.mock.sent.count

        await agent.mock.emit(#"{"jsonrpc":"2.0","id":50,"method":"fs/read_text_file","params":{"sessionId":"sess-new","path":"\#(root.path)/f.txt"}}"#)
        var sent = try await agent.mock.waitForSent(count: baseline + 1)
        guard case .response(_, let result, _) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected response"); return
        }
        #expect(result?["content"]?.stringValue == "content-x")

        await agent.mock.emit(#"{"jsonrpc":"2.0","id":51,"method":"fs/read_text_file","params":{"sessionId":"sess-new","path":"/etc/hosts"}}"#)
        sent = try await agent.mock.waitForSent(count: baseline + 2)
        guard case .response(let id, _, let error) = try await agent.mock.sentMessage(sent.count - 1) else {
            Issue.record("expected error response"); return
        }
        #expect(id == .number(51))
        #expect(error != nil)
    }
}

/// File system that rejects everything — for tests that never touch fs.
private struct NullFileSystem: ACPFileSystem {
    struct Unsupported: Error {}
    func readTextFile(path: String, line: Int?, limit: Int?) throws -> String { throw Unsupported() }
    func writeTextFile(path: String, content: String) throws { throw Unsupported() }
}
```

- [ ] **Step 5: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter ACPSessionTests`
Expected: compile FAILURE — `cannot find 'ACPSession' in scope`.

- [ ] **Step 6: Implement ACPSession**

```swift
// Packages/TillerACP/Sources/TillerACP/ACPSession.swift
import Foundation

/// Result of a successful connect: what the app layer needs to render and
/// persist (Plan 2 stores `sessionId`; Plan 3 renders modes).
public struct SessionHandle: Sendable, Equatable {
    public var sessionId: String
    public var agentCapabilities: AgentCapabilities
    public var modes: SessionModeState?
    public var didResume: Bool
}

/// Session-level happenings the view model consumes.
public enum ACPSessionEvent: Sendable {
    case update(SessionUpdate)
    case permissionRequested(requestId: JSONRPCID, toolCall: ToolCallUpdate,
                             options: [PermissionOption])
    case disconnected
}

public enum ACPSessionError: Error, Sendable {
    case notConnected
    case unsupportedProtocolVersion(Int)
}

/// Drives one agent conversation over an ACPClient: handshake, session
/// creation/resume, prompting, cancellation, permission answers, and serving
/// the agent's fs requests through an ACPFileSystem.
public actor ACPSession {
    public static let protocolVersion = 1

    private let client: ACPClient
    private let fileSystem: any ACPFileSystem
    private var sessionId: String?
    private var pumpTask: Task<Void, Never>?
    private let eventContinuation: AsyncStream<ACPSessionEvent>.Continuation
    public nonisolated let events: AsyncStream<ACPSessionEvent>

    public init(client: ACPClient, fileSystem: any ACPFileSystem) {
        self.client = client
        self.fileSystem = fileSystem
        (events, eventContinuation) = AsyncStream.makeStream(of: ACPSessionEvent.self)
    }

    /// Starts the underlying client and the incoming-message pump.
    public func start() async throws {
        try await client.start()
        pumpTask = Task { [weak self] in
            guard let self else { return }
            for await incoming in self.client.incoming {
                await self.dispatch(incoming)
            }
            await self.finish()
        }
    }

    public func stop() async {
        pumpTask?.cancel()
        await client.stop()
        finish()
    }

    /// Handshakes and opens (or resumes) a session. Resume happens only when
    /// the agent advertises `loadSession` AND a resumeSessionId is given;
    /// otherwise falls back to a fresh session.
    public func connect(cwd: String, resumeSessionId: String?) async throws -> SessionHandle {
        let initialize = try await client.request(
            "initialize",
            params: InitializeParams(
                protocolVersion: Self.protocolVersion,
                clientCapabilities: ClientCapabilities(
                    fs: FileSystemCapability(readTextFile: true, writeTextFile: true),
                    terminal: false)),
            as: InitializeResult.self)
        guard initialize.protocolVersion == Self.protocolVersion else {
            throw ACPSessionError.unsupportedProtocolVersion(initialize.protocolVersion)
        }

        if let resumeSessionId, initialize.agentCapabilities.loadSession {
            let loaded = try await client.request(
                "session/load",
                params: LoadSessionParams(sessionId: resumeSessionId, cwd: cwd),
                as: LoadSessionResult.self)
            sessionId = resumeSessionId
            return SessionHandle(sessionId: resumeSessionId,
                                 agentCapabilities: initialize.agentCapabilities,
                                 modes: loaded.modes, didResume: true)
        }

        let created = try await client.request(
            "session/new", params: NewSessionParams(cwd: cwd), as: NewSessionResult.self)
        sessionId = created.sessionId
        return SessionHandle(sessionId: created.sessionId,
                             agentCapabilities: initialize.agentCapabilities,
                             modes: created.modes, didResume: false)
    }

    /// Sends one user turn; suspends until the agent finishes it.
    public func prompt(_ blocks: [ContentBlock]) async throws -> StopReason {
        guard let sessionId else { throw ACPSessionError.notConnected }
        let result = try await client.request(
            "session/prompt", params: PromptParams(sessionId: sessionId, prompt: blocks),
            as: PromptResult.self)
        return result.stopReason
    }

    public func cancel() async {
        guard let sessionId else { return }
        try? await client.notify("session/cancel", params: CancelParams(sessionId: sessionId))
    }

    public func setMode(_ modeId: String) async throws {
        guard let sessionId else { throw ACPSessionError.notConnected }
        _ = try await client.request(
            "session/set_mode", params: SetModeParams(sessionId: sessionId, modeId: modeId),
            as: JSONValue.self)
    }

    public func answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome) async {
        try? await client.respond(
            to: requestId, result: RequestPermissionResult(outcome: outcome))
    }

    private func dispatch(_ incoming: ACPIncoming) async {
        switch incoming {
        case .notification(let method, let params):
            guard method == "session/update", let params,
                  let note = try? params.decoded(SessionNotification.self) else { return }
            eventContinuation.yield(.update(note.update))

        case .request(let id, let method, let params):
            switch method {
            case "session/request_permission":
                guard let params,
                      let request = try? params.decoded(RequestPermissionParams.self) else {
                    try? await client.respondError(to: id, code: -32602,
                                                   message: "invalid permission params")
                    return
                }
                eventContinuation.yield(.permissionRequested(
                    requestId: id, toolCall: request.toolCall, options: request.options))

            case "fs/read_text_file":
                await serveRead(id: id, params: params)

            case "fs/write_text_file":
                await serveWrite(id: id, params: params)

            default:
                try? await client.respondError(to: id, code: -32601,
                                               message: "method not supported: \(method)")
            }
        }
    }

    private func serveRead(id: JSONRPCID, params: JSONValue?) async {
        do {
            guard let params else { throw ACPSessionError.notConnected }
            let read = try params.decoded(ReadTextFileParams.self)
            let content = try fileSystem.readTextFile(
                path: read.path, line: read.line, limit: read.limit)
            try await client.respond(to: id, result: ReadTextFileResult(content: content))
        } catch {
            try? await client.respondError(to: id, code: -32000,
                                           message: "read failed: \(error)")
        }
    }

    private func serveWrite(id: JSONRPCID, params: JSONValue?) async {
        do {
            guard let params else { throw ACPSessionError.notConnected }
            let write = try params.decoded(WriteTextFileParams.self)
            try fileSystem.writeTextFile(path: write.path, content: write.content)
            try await client.respond(to: id, result: JSONValue.null)
        } catch {
            try? await client.respondError(to: id, code: -32000,
                                           message: "write failed: \(error)")
        }
    }

    private func finish() {
        eventContinuation.yield(.disconnected)
        eventContinuation.finish()
    }
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter "ACPSessionTests|WorktreeFileSystemTests"`
Expected: PASS (10 tests).

- [ ] **Step 8: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add acp session actor with fs guard and permissions"
```

---

### Task 10: AgentLaunchSpec + full CI gate

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentLaunchSpecTests.swift`

**Interfaces:**
- Produces: `AgentLaunchSpec{executable: String, arguments: [String]}` with `static func claudeCode() -> AgentLaunchSpec`, `static func openCode() -> AgentLaunchSpec`, and `static let claudeCodeACPVersion: String`. Plan 3 feeds these into `ProcessTransport(executable:arguments:cwd:)`.

- [ ] **Step 1: Pin the adapter version**

Run: `npm view @zed-industries/claude-code-acp version`
Copy the exact version it prints into `claudeCodeACPVersion` below (shown as `0.5.1` — replace with the real output). Also verify OpenCode's ACP entry point: `opencode acp --help` should print usage without error (if the subcommand differs on the installed version, adjust `openCode()` accordingly and note it in the commit message).

- [ ] **Step 2: Write the failing test**

```swift
// Packages/TillerACP/Tests/TillerACPTests/AgentLaunchSpecTests.swift
import Testing
@testable import TillerACP

@Suite struct AgentLaunchSpecTests {
    @Test func claudeCodeRunsPinnedAdapterThroughLoginShell() {
        let spec = AgentLaunchSpec.claudeCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments.count == 2)
        #expect(spec.arguments[0] == "-lc")
        #expect(spec.arguments[1] ==
            "exec npx -y @zed-industries/claude-code-acp@\(AgentLaunchSpec.claudeCodeACPVersion)")
    }

    @Test func pinnedVersionLooksLikeSemver() {
        let parts = AgentLaunchSpec.claudeCodeACPVersion.split(separator: ".")
        #expect(parts.count == 3)
        #expect(parts.allSatisfy { Int($0) != nil })
    }

    @Test func openCodeRunsNativeACPThroughLoginShell() {
        let spec = AgentLaunchSpec.openCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments == ["-lc", "exec opencode acp"])
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchSpecTests`
Expected: compile FAILURE — `cannot find 'AgentLaunchSpec' in scope`.

- [ ] **Step 4: Implement AgentLaunchSpec**

```swift
// Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift
import Foundation

/// How to launch an ACP agent process for a worktree. Commands run through a
/// login shell (`zsh -lc`) so the user's PATH (node via nvm/homebrew, the
/// opencode binary) resolves exactly as it does in their terminal panes;
/// `exec` replaces the shell so the child IS the agent process.
public struct AgentLaunchSpec: Sendable, Equatable {
    public var executable: String
    public var arguments: [String]

    public init(executable: String, arguments: [String]) {
        self.executable = executable
        self.arguments = arguments
    }

    /// Pinned adapter version: a protocol-stable, reproducible launch.
    /// Update deliberately via `npm view @zed-industries/claude-code-acp version`.
    public static let claudeCodeACPVersion = "0.5.1"

    /// Claude Code via Zed's official ACP adapter, fetched on demand by npx
    /// (cached by npm after the first run).
    public static func claudeCode() -> AgentLaunchSpec {
        AgentLaunchSpec(
            executable: "/bin/zsh",
            arguments: ["-lc",
                "exec npx -y @zed-industries/claude-code-acp@\(claudeCodeACPVersion)"])
    }

    /// OpenCode's native ACP mode (binary installed by the user).
    public static func openCode() -> AgentLaunchSpec {
        AgentLaunchSpec(executable: "/bin/zsh", arguments: ["-lc", "exec opencode acp"])
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchSpecTests`
Expected: PASS (3 tests).

- [ ] **Step 6: Full package test + repo CI gate**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS, all suites.

Run: `Scripts/ci.sh`
Expected: prints `CI OK`. If `TillerTerminal` fails only on `spawnCapturesOutput`, re-run (known flaky, up to 5-6 retries).

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: add agent launch specs for claude code and opencode"
```

---

## Out of Scope (later plans)

- **Plan 2** — persistence & catalog: `chat_session`/`chat_item` GRDB tables, `ChatSessionStore`, finalized-item write policy, `AgentCatalog.supportsACP`, resume wiring.
- **Plan 3** — chat UI & app integration: `App/Chat/` views, `ChatViewModel` consuming `ACPSession.events` + `TranscriptReducer`, permission cards, composer (@-mentions, slash commands, images, modes), pane/tab integration, sidebar status layer, npx-missing error surface.
