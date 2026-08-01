# LSP Editor Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the Tiller code editor completion, go-to-definition, diagnostics and hover backed by real language servers the user already has installed, for as many languages as a declarative table can describe.

**Architecture:** A new leaf package `TillerRPC` holds the JSON-RPC 2.0 value types extracted from `TillerACP`. A new package `TillerLSP` (no SwiftUI, no AppKit) owns `Content-Length` framing, a child-process transport, the ~10 LSP types the four features need, a static server catalog, and a lifecycle host keyed by (server × workspace root). `App` owns the bridge: two delegates plugged into `SourceEditor`'s existing `completionDelegate` / `jumpToDefinitionDelegate` parameters, an `EmphasisManager`-based diagnostics painter, a Problems route in the right panel, a status indicator in the editor header, and a Settings category.

**Tech Stack:** Swift 6 (strict concurrency), swift-testing (`@Test` / `#expect`), `CodeEditSourceEditor` 0.15.2 + `CodeEditTextView`, SwiftUI + AppKit, XcodeGen (`project.yml`).

**Normative input:** the grilling session of 2026-07-29. The sixteen decisions it produced are restated verbatim in "Decisions this plan implements" below. No task may silently widen them.

---

## Global Constraints

- macOS 15+, Swift 6.0. `SWIFT_VERSION: "6.0"`, `MACOSX_DEPLOYMENT_TARGET: "15.0"` — unchanged from `project.yml`.
- Tests are written **before** the production change they cover, using swift-testing (`@Test` / `#expect`), never XCTest.
- Conventional Commits, lower-case imperative subject (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, `perf:`).
- Every task ends with the repository compiling and its own verification command green. Every **phase** ends with `Scripts/ci.sh` printing `CI OK`.
- `Tiller.xcodeproj` is generated: never hand-edit it. Edit `project.yml` and run `xcodegen generate`.
- `TillerRPC` has **zero** local dependencies. `TillerLSP` depends on `TillerRPC` only, and imports neither SwiftUI nor AppKit. `App` is the sole composition root.
- All user-facing strings are **English**, including when the conversation driving the work is not.
- `Scripts/ci.sh` discovers packages with `for pkg in Packages/*/`. Both new packages join the gate automatically — no edit to `ci.sh` is required or permitted by this plan.
- **No live language server process may run inside `Scripts/ci.sh`.** Tests replay recorded fixtures or drive a programmable fake. A test that spawns `rust-analyzer` is a task failure.
- No test may be skipped conditionally on a binary being present. This repo has already shipped a vacuous green run once; `ci.sh` now asserts a non-zero test count for exactly that reason.
- Debounce for `textDocument/didChange`: **150 ms**. Completion request timeout: **2 s**. `initialize` timeout: **10 s**.
- Concurrent server cap: **4**. Restart attempts after an unexpected exit: **2**, then the server stays down.
- LSP positions are **0-indexed**; `CursorPosition` from `CodeEditSourceEditor` is **1-indexed**. Every conversion goes through `LineIndex` (Task 8) and nowhere else.
- No `TBD`. Any detail this plan failed to resolve is raised with the author, not invented.

**How to read the test lists.** Every test in this plan is shown with a real body. There are no specification stubs: a task is not complete while any listed test is missing or empty.

---

## Decisions this plan implements

| # | Decision |
|---|---|
| 1 | Target is the **editor** experience, not agent tooling and not a build-verification loop. |
| 2 | The agent-facing surface (`tillerctl lsp-*`) is **out of scope**. `TillerLSP` carries no SwiftUI/AppKit so it stays reachable from `AppModel.handleControl` later. |
| 3 | Language coverage comes from a **static declarative table**. No per-language Swift code. |
| 4 | Tiller **never installs** a server. Settings reports what is present and what is missing. |
| 6 | Four features: completion, go-to-definition, diagnostics, hover. |
| 7 | The LSP client is **hand-written**, reusing the existing JSON-RPC value types. |
| 8 | `JSONValue` + `JSONRPC` are **extracted** into `TillerRPC`; `TillerLSP` is a new sibling. The ACP transport is left untouched. |
| 9 | Servers start **lazily** per (server × workspace root), with a hard cap of 4 and LRU eviction. |
| 10 | Document sync is **full-text** with a 150 ms debounce. |
| 11 | Diagnostics render as an **underline** plus a **Problems panel**. No diagnostics in the hover popover. |
| 12 | The Problems panel shows **everything the server publishes**, not only open files. |
| 13 | Go-to-definition outside the worktree (and inside build directories) opens **read-only**. |
| 14 | Tests use **recorded fixtures + a programmable fake**. No live process in CI. |
| 15 | Settings shows presence (resolved through a login shell) plus a **copyable install command**. |
| 16 | Failures surface as a **status indicator in the editor header**; at most 2 restarts. |

---

## File structure

### `Packages/TillerRPC/` — new, leaf, zero dependencies

| File | Responsibility |
|---|---|
| `Sources/TillerRPC/JSONValue.swift` | Moved verbatim from `TillerACP`. Generic JSON tree. |
| `Sources/TillerRPC/JSONRPC.swift` | Moved verbatim from `TillerACP`. `JSONRPCID`, `JSONRPCError`, `JSONRPCMessage`, newline codec. |

### `Packages/TillerLSP/` — new, depends on `TillerRPC`

| File | Responsibility |
|---|---|
| `LSPFraming.swift` | `Content-Length` header encode/decode over a rolling byte buffer. |
| `LSPTransport.swift` | Transport protocol + `LSPProcessTransport` (child process over stdio). |
| `LSPClient.swift` | Actor. Request/response correlation, notification stream, `$/cancelRequest`. |
| `LSPTypes.swift` | `LSPPosition`, `LSPRange`, `LSPLocation`, `LSPDiagnostic`, `LSPCompletionItem`, `LSPHoverContent`, and their union decoders. |
| `LanguageServerDefinition.swift` | One catalog row. Pure data. |
| `LanguageServerCatalog.swift` | The static table. Pure data. |
| `ExecutableLocator.swift` | `ExecutableLocating` protocol + login-shell implementation. |
| `LineIndex.swift` | UTF-16 line/character ↔ `NSRange`, and the 0/1-index bridge. |
| `LanguageServerHost.swift` | Actor. Lifecycle per (server × root), cap, LRU eviction, restart budget. |
| `DocumentSession.swift` | Per-document `didOpen`/`didChange`/`didClose` with debounce and version counter. |
| `LanguageServerStatus.swift` | `notInstalled` / `starting` / `indexing` / `ready` / `stopped`. |

### `App/` — bridge and UI

| File | Responsibility |
|---|---|
| `App/LanguageServers/LanguageServerCenter.swift` | `@MainActor @Observable`. Owns the host, the per-URI diagnostics store, and per-root status. The only App-side entry point. |
| `App/CodeEditor/LSPCompletionDelegate.swift` | `CodeSuggestionDelegate` implementation. |
| `App/CodeEditor/LSPJumpToDefinitionDelegate.swift` | `JumpToDefinitionDelegate` implementation. |
| `App/CodeEditor/LSPDiagnosticsPainter.swift` | Maps diagnostics to `Emphasis` under the `"lsp.diagnostics"` id. |
| `App/CodeEditor/LSPHoverPopover.swift` | The one piece of from-scratch UI. Reused by nothing else, by decision 11. |
| `App/CodeEditor/CodeEditorTabView.swift` | **Modify**: pass the two delegates, add the status indicator, add read-only mode. |
| `App/RightPanel/ProblemsView.swift` | The Problems list. |
| `App/RightPanel/RightPanelMode.swift` | **Modify**: add `case problems`. |
| `App/LanguageServersSettingsView.swift` | Presence table + install commands. |
| `App/AppRoute.swift` | **Modify**: add `SettingsCategory.languageServers`. |
| `Scripts/record-lsp-fixture.sh` | Records a real `rust-analyzer` session into a committed `.ndjson`. |

---

## Phase 0 — RPC extraction

### Task 1: Extract `TillerRPC` from `TillerACP`

Pure refactor. No behaviour changes, no new types. The point is that `TillerLSP` can reuse the JSON-RPC value types without inheriting GRDB through `TillerACP → TillerPersistence`.

**Files:**
- Create: `Packages/TillerRPC/Package.swift`
- Create: `Packages/TillerRPC/Sources/TillerRPC/JSONValue.swift` (moved)
- Create: `Packages/TillerRPC/Sources/TillerRPC/JSONRPC.swift` (moved)
- Create: `Packages/TillerRPC/Tests/TillerRPCTests/JSONRPCCodecTests.swift`
- Delete: `Packages/TillerACP/Sources/TillerACP/JSONValue.swift`
- Delete: `Packages/TillerACP/Sources/TillerACP/JSONRPC.swift`
- Modify: `Packages/TillerACP/Package.swift`
- Modify: 31 files under `Packages/TillerACP/` that reference `JSONValue` / `JSONRPC*` (add one `import TillerRPC`)
- Modify: `project.yml`

**Interfaces:**
- Consumes: nothing.
- Produces: module `TillerRPC` exporting `JSONValue`, `JSONRPCID`, `JSONRPCError`, `JSONRPCMessage` with today's exact public API — `JSONRPCMessage.decode(_ line: Data) throws -> JSONRPCMessage`, `func encodedLine() throws -> Data`, `JSONValue.subscript(key:)`, `.stringValue`, `.intValue`, `.decoded(_:)`, `.encoding(_:)`.

> **Note on the import churn.** 31 files gain a single `import TillerRPC` line. The alternative — one file in `TillerACP` doing `@_exported import TillerRPC` — was rejected: it hides a real module edge behind an underscored attribute in a repo that checks module boundaries in CI. The wide diff is mechanical and the compiler proves it complete.

- [ ] **Step 1: Create the package manifest**

```swift
// Packages/TillerRPC/Package.swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerRPC",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerRPC", targets: ["TillerRPC"])],
    targets: [
        .target(name: "TillerRPC"),
        .testTarget(name: "TillerRPCTests", dependencies: ["TillerRPC"])
    ]
)
```

- [ ] **Step 2: Move the two files unchanged**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
mkdir -p Packages/TillerRPC/Sources/TillerRPC Packages/TillerRPC/Tests/TillerRPCTests
git mv Packages/TillerACP/Sources/TillerACP/JSONValue.swift Packages/TillerRPC/Sources/TillerRPC/JSONValue.swift
git mv Packages/TillerACP/Sources/TillerACP/JSONRPC.swift  Packages/TillerRPC/Sources/TillerRPC/JSONRPC.swift
```

Do not edit their contents. Their `public` surface is already correct for cross-module use.

- [ ] **Step 3: Write the codec test that pins the moved behaviour**

```swift
// Packages/TillerRPC/Tests/TillerRPCTests/JSONRPCCodecTests.swift
import Foundation
import Testing
@testable import TillerRPC

@Test func decodesARequestWithANumericID() throws {
    let line = Data(#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{"a":1}}"#.utf8)
    let message = try JSONRPCMessage.decode(line)
    guard case .request(let id, let method, let params) = message else {
        Issue.record("expected a request, got \(message)"); return
    }
    #expect(id == .number(7))
    #expect(method == "initialize")
    #expect(params?["a"]?.intValue == 1)
}

@Test func decodesANotificationAsHavingNoID() throws {
    let line = Data(#"{"jsonrpc":"2.0","method":"$/progress","params":{}}"#.utf8)
    let message = try JSONRPCMessage.decode(line)
    guard case .notification(let method, _) = message else {
        Issue.record("expected a notification, got \(message)"); return
    }
    #expect(method == "$/progress")
}

@Test func decodesAnErrorResponse() throws {
    let line = Data(#"{"jsonrpc":"2.0","id":"a","error":{"code":-32601,"message":"nope"}}"#.utf8)
    let message = try JSONRPCMessage.decode(line)
    guard case .response(let id, _, let error) = message else {
        Issue.record("expected a response, got \(message)"); return
    }
    #expect(id == .string("a"))
    #expect(error?.code == -32601)
}

@Test func encodedLineRoundTripsThroughDecode() throws {
    let original = JSONRPCMessage.request(
        id: .number(1), method: "textDocument/hover", params: .object(["k": .string("v")]))
    var line = try original.encodedLine()
    #expect(line.last == UInt8(ascii: "\n"))
    line.removeLast()
    #expect(try JSONRPCMessage.decode(line) == original)
}

@Test func rejectsAMessageWithNeitherMethodNorID() {
    let line = Data(#"{"jsonrpc":"2.0"}"#.utf8)
    #expect(throws: JSONRPCMessage.DecodeFailure.self) { try JSONRPCMessage.decode(line) }
}
```

- [ ] **Step 4: Run the new package's tests**

Run: `cd Packages/TillerRPC && swift test`
Expected: 5 tests pass. (They exercise moved code, so they pass immediately — this is a refactor, not a feature; the tests exist to keep the move honest.)

- [ ] **Step 5: Point `TillerACP` at the new package**

```swift
// Packages/TillerACP/Package.swift — dependencies and both targets
    dependencies: [
        .package(path: "../TillerPersistence"),
        .package(path: "../TillerRPC")
    ],
    targets: [
        .target(name: "TillerACP", dependencies: ["TillerPersistence", "TillerRPC"]),
        .testTarget(
            name: "TillerACPTests",
            dependencies: ["TillerACP", "TillerPersistence", "TillerRPC"],
            resources: [.copy("Fixtures")]
        )
    ]
```

- [ ] **Step 6: Add the import to every file that needs it**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
for f in $(grep -rl 'JSONValue\|JSONRPC' Packages/TillerACP/Sources Packages/TillerACP/Tests); do
    grep -q '^import TillerRPC' "$f" || \
      perl -0pi -e 's/^(import Foundation\n)/$1import TillerRPC\n/m unless /^import TillerRPC/m' "$f"
done
```

Files without `import Foundation` are not touched by that substitution — the compiler will name them in step 7. Add `import TillerRPC` to each by hand, keeping imports alphabetical within the file's existing style.

- [ ] **Step 7: Build the ACP package and fix any file the script missed**

Run: `cd Packages/TillerACP && swift build`
Expected: PASS. Any `cannot find 'JSONValue' in scope` names a file that still needs the import.

- [ ] **Step 8: Register the package with the app target**

```yaml
# project.yml — under `packages:`
  TillerRPC:
    path: Packages/TillerRPC
```

`TillerRPC` is **not** added to the `Tiller` target's `dependencies:` list. The app reaches it transitively through `TillerACP`, and will reach it through `TillerLSP` in Task 2. Adding it directly would assert an app-level dependency that does not exist.

- [ ] **Step 9: Run the full gate**

Run: `xcodegen generate && bash Scripts/ci.sh`
Expected: `CI OK`.

If `TillerTerminal`'s `PtyProcessTests` fail, re-run — they are timing-flaky under load and this repo tolerates retries for that suite alone. Any other failure is a real regression from the move.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "refactor: extract JSON-RPC value types into TillerRPC

TillerLSP needs JSONValue and JSONRPCMessage without inheriting GRDB
through TillerACP -> TillerPersistence. Pure move: no type, signature or
behaviour changes."
```

---

## Phase 1 — Wire protocol

The three tasks below build the pipe. Nothing in this phase knows what a completion is; it moves `JSONRPCMessage` values in and out of a child process. All of it is testable without a real server.

### Task 2: `TillerLSP` package and `Content-Length` framing

LSP frames every message with an HTTP-style header block, not a newline. This is the single wire difference from ACP, and it is the only reason the ACP transport could not simply be reused.

**Files:**
- Create: `Packages/TillerLSP/Package.swift`
- Create: `Packages/TillerLSP/Sources/TillerLSP/LSPFraming.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LSPFramingTests.swift`
- Modify: `project.yml`

**Interfaces:**
- Consumes: `TillerRPC.JSONRPCMessage` (Task 1).
- Produces:
  - `LSPFraming.encode(_ message: JSONRPCMessage) throws -> Data`
  - `struct LSPFrameDecoder { mutating func append(_ data: Data); mutating func next() throws -> Data? }` — `next()` returns one complete JSON payload (headers stripped) or `nil` when the buffer holds no complete frame yet.
  - `LSPFrameDecoder.DecodeFailure` with cases `.malformedHeader(String)`, `.missingContentLength`.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LSPFramingTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

private func frame(_ json: String) -> Data {
    let body = Data(json.utf8)
    return Data("Content-Length: \(body.count)\r\n\r\n".utf8) + body
}

@Test func encodesAHeaderWithTheByteCountNotTheCharacterCount() throws {
    // "é" is one Character but two UTF-8 bytes: Content-Length must count bytes.
    let message = JSONRPCMessage.notification(
        method: "x", params: .object(["v": .string("é")]))
    let data = try LSPFraming.encode(message)
    let text = String(decoding: data, as: UTF8.self)
    let header = text.components(separatedBy: "\r\n\r\n")[0]
    let body = text.components(separatedBy: "\r\n\r\n")[1]
    let declared = Int(header.replacingOccurrences(of: "Content-Length: ", with: ""))
    #expect(declared == Data(body.utf8).count)
    #expect(declared != body.count)
}

@Test func decodesOneCompleteFrame() throws {
    var decoder = LSPFrameDecoder()
    decoder.append(frame(#"{"jsonrpc":"2.0","id":1,"result":null}"#))
    let payload = try decoder.next()
    #expect(payload != nil)
    #expect(try JSONRPCMessage.decode(payload!) == .response(id: .number(1), result: .null, error: nil))
}

@Test func returnsNilUntilTheWholeBodyHasArrived() throws {
    var decoder = LSPFrameDecoder()
    let whole = frame(#"{"jsonrpc":"2.0","method":"a"}"#)
    decoder.append(whole.prefix(whole.count - 5))
    #expect(try decoder.next() == nil)
    decoder.append(whole.suffix(5))
    #expect(try decoder.next() != nil)
}

@Test func decodesTwoFramesArrivingInOneChunk() throws {
    var decoder = LSPFrameDecoder()
    decoder.append(frame(#"{"jsonrpc":"2.0","method":"a"}"#) + frame(#"{"jsonrpc":"2.0","method":"b"}"#))
    let first = try #require(try decoder.next())
    let second = try #require(try decoder.next())
    #expect(try JSONRPCMessage.decode(first) == .notification(method: "a", params: nil))
    #expect(try JSONRPCMessage.decode(second) == .notification(method: "b", params: nil))
    #expect(try decoder.next() == nil)
}

@Test func toleratesAContentTypeHeaderAndHeaderOrder() throws {
    let body = Data(#"{"jsonrpc":"2.0","method":"a"}"#.utf8)
    var decoder = LSPFrameDecoder()
    decoder.append(Data("Content-Type: application/vscode-jsonrpc; charset=utf-8\r\n".utf8)
        + Data("Content-Length: \(body.count)\r\n\r\n".utf8) + body)
    #expect(try decoder.next() != nil)
}

@Test func throwsWhenTheHeaderBlockCarriesNoContentLength() {
    var decoder = LSPFrameDecoder()
    decoder.append(Data("Content-Type: text/plain\r\n\r\n{}".utf8))
    #expect(throws: LSPFrameDecoder.DecodeFailure.self) { try decoder.next() }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd Packages/TillerLSP && swift test`
Expected: FAIL — no such module `TillerLSP`.

- [ ] **Step 3: Write the manifest**

```swift
// Packages/TillerLSP/Package.swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "TillerLSP",
    platforms: [.macOS(.v15)],
    products: [.library(name: "TillerLSP", targets: ["TillerLSP"])],
    dependencies: [.package(path: "../TillerRPC")],
    targets: [
        .target(name: "TillerLSP", dependencies: ["TillerRPC"]),
        .testTarget(name: "TillerLSPTests", dependencies: ["TillerLSP", "TillerRPC"])
    ]
)
```

- [ ] **Step 4: Write the framing implementation**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LSPFraming.swift
import Foundation
import TillerRPC

/// LSP frames each JSON-RPC message with an HTTP-style header block:
/// `Content-Length: <bytes>\r\n\r\n<payload>`. This is the only wire-level
/// difference from ACP, which delimits messages with a newline.
public enum LSPFraming {
    public static func encode(_ message: JSONRPCMessage) throws -> Data {
        // `encodedLine()` appends a newline for the ACP wire; LSP counts bytes,
        // so that trailing byte would make Content-Length disagree with the body.
        var body = try message.encodedLine()
        if body.last == UInt8(ascii: "\n") { body.removeLast() }
        return Data("Content-Length: \(body.count)\r\n\r\n".utf8) + body
    }
}

/// Accumulates bytes from a stream and hands back one complete payload at a
/// time. A single reader owns an instance; it is a value type on purpose so it
/// cannot be shared across tasks by accident.
public struct LSPFrameDecoder {
    public enum DecodeFailure: Error, Equatable {
        case malformedHeader(String)
        case missingContentLength
    }

    private static let separator = Data("\r\n\r\n".utf8)
    private var buffer = Data()

    public init() {}

    public mutating func append(_ data: Data) { buffer.append(data) }

    /// Returns the next complete payload with its headers stripped, or `nil`
    /// when the buffer does not yet hold a whole frame.
    public mutating func next() throws -> Data? {
        guard let separatorRange = buffer.range(of: Self.separator) else { return nil }
        let headerData = buffer[buffer.startIndex..<separatorRange.lowerBound]
        guard let headerText = String(data: headerData, encoding: .utf8) else {
            throw DecodeFailure.malformedHeader("header block is not UTF-8")
        }
        guard let length = Self.contentLength(in: headerText) else {
            throw DecodeFailure.missingContentLength
        }
        let bodyStart = separatorRange.upperBound
        guard buffer.count - (bodyStart - buffer.startIndex) >= length else { return nil }
        let bodyEnd = bodyStart + length
        let payload = Data(buffer[bodyStart..<bodyEnd])
        buffer.removeSubrange(buffer.startIndex..<bodyEnd)
        return payload
    }

    private static func contentLength(in header: String) -> Int? {
        for line in header.components(separatedBy: "\r\n") {
            let parts = line.split(separator: ":", maxSplits: 1)
            guard parts.count == 2,
                  parts[0].trimmingCharacters(in: .whitespaces).lowercased() == "content-length"
            else { continue }
            return Int(parts[1].trimmingCharacters(in: .whitespaces))
        }
        return nil
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd Packages/TillerLSP && swift test`
Expected: 6 tests pass.

- [ ] **Step 6: Register the package**

```yaml
# project.yml — under `packages:`
  TillerLSP:
    path: Packages/TillerLSP
```

```yaml
# project.yml — under targets: Tiller: dependencies:
      - package: TillerLSP
```

- [ ] **Step 7: Run the gate**

Run: `xcodegen generate && bash Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: add TillerLSP package with Content-Length framing"
```

---

### Task 3: Transport protocol, child-process transport, and the programmable fake

The fake built here is the workhorse for every later task: it is how timing, cancellation and server death get tested without a binary.

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LSPTransport.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/Support/FakeLSPTransport.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LSPProcessTransportTests.swift`

**Interfaces:**
- Consumes: `LSPFraming`, `LSPFrameDecoder` (Task 2); `JSONRPCMessage` (Task 1).
- Produces:
  - `protocol LSPTransport: Sendable { func start() async throws; func send(_ message: JSONRPCMessage) async throws; func messages() -> AsyncThrowingStream<JSONRPCMessage, Error>; func terminate() async }`
  - `final class LSPProcessTransport: LSPTransport` with
    `init(executable: String, arguments: [String], cwd: String, environment: [String: String]?, onStderrLine: (@Sendable (String) -> Void)?)`
    and `var terminationStatus: Int32?`
  - `LSPProcessTransport.LaunchFailure`
  - Test-only `FakeLSPTransport` with
    `func emit(_ message: JSONRPCMessage)`, `func fail(_ error: Error)`, `func finish()`,
    `var sent: [JSONRPCMessage] { get async }`, `var isTerminated: Bool { get async }`.

> **Why this is a near-copy of `ProcessTransport`, not a shared abstraction.** `ProcessTransport` in `TillerACP` hardcodes newline splitting in `drainBufferedLinesLocked`. Parameterising it by a framing strategy would mean editing a file that every chat session depends on, to save ~40 lines. Decision 8 chose duplication of the transport and sharing of the types.

- [ ] **Step 1: Write the failing tests**

`/bin/cat` is used as the child process: it echoes stdin to stdout, so a framed message written in comes back out and the whole encode → pipe → decode path is exercised with no language server involved.

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LSPProcessTransportTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

@Test func roundTripsAFramedMessageThroughARealChildProcess() async throws {
    let transport = LSPProcessTransport(
        executable: "/bin/cat", arguments: [], cwd: NSTemporaryDirectory(),
        environment: nil, onStderrLine: nil)
    try await transport.start()
    let stream = transport.messages()
    var iterator = stream.makeAsyncIterator()

    let sent = JSONRPCMessage.request(
        id: .number(42), method: "initialize", params: .object(["rootUri": .string("file:///tmp")]))
    try await transport.send(sent)

    let received = try await iterator.next()
    #expect(received == sent)
    await transport.terminate()
}

@Test func streamFinishesWhenTheChildExits() async throws {
    let transport = LSPProcessTransport(
        executable: "/usr/bin/true", arguments: [], cwd: NSTemporaryDirectory(),
        environment: nil, onStderrLine: nil)
    try await transport.start()
    var iterator = transport.messages().makeAsyncIterator()
    #expect(try await iterator.next() == nil)
}

@Test func reportsStderrLinesWithoutMixingThemIntoTheProtocol() async throws {
    let lines = LineCollector()
    let transport = LSPProcessTransport(
        executable: "/bin/sh", arguments: ["-c", "echo boom 1>&2; sleep 0.2"],
        cwd: NSTemporaryDirectory(), environment: nil,
        onStderrLine: { line in Task { await lines.append(line) } })
    try await transport.start()
    var iterator = transport.messages().makeAsyncIterator()
    #expect(try await iterator.next() == nil)   // nothing on stdout
    try await Task.sleep(for: .milliseconds(50))
    #expect(await lines.all.contains("boom"))
    await transport.terminate()
}

@Test func launchingAMissingExecutableThrowsLaunchFailure() async {
    let transport = LSPProcessTransport(
        executable: "/nonexistent/server", arguments: [], cwd: NSTemporaryDirectory(),
        environment: nil, onStderrLine: nil)
    await #expect(throws: LSPProcessTransport.LaunchFailure.self) { try await transport.start() }
}

private actor LineCollector {
    private(set) var all: [String] = []
    func append(_ line: String) { all.append(line) }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LSPProcessTransportTests`
Expected: FAIL — `LSPProcessTransport` not defined.

- [ ] **Step 3: Write the transport**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LSPTransport.swift
import Foundation
import TillerRPC

/// Carries whole JSON-RPC messages to and from a language server.
/// `messages()` must be consumed by a single reader; the stream finishes when
/// the peer closes its end.
public protocol LSPTransport: Sendable {
    func start() async throws
    func send(_ message: JSONRPCMessage) async throws
    func messages() -> AsyncThrowingStream<JSONRPCMessage, Error>
    func terminate() async
}

/// A language server running as a child process, speaking framed JSON-RPC over
/// stdio. Stderr is surfaced separately and never enters the protocol stream —
/// servers log freely there and a single stray line would desynchronise a
/// reader that mixed the two.
public final class LSPProcessTransport: LSPTransport, @unchecked Sendable {
    public struct LaunchFailure: Error { public let underlying: Error }

    private let process = Process()
    private let stdinPipe = Pipe()
    private let stdoutPipe = Pipe()
    private let stderrPipe = Pipe()
    private let onStderrLine: (@Sendable (String) -> Void)?
    private let lock = NSLock()
    private var continuation: AsyncThrowingStream<JSONRPCMessage, Error>.Continuation?
    private var decoder = LSPFrameDecoder()
    private var stderrBuffer = Data()
    private var pendingMessages: [JSONRPCMessage] = []
    private var hasExited = false

    public var terminationStatus: Int32? {
        lock.lock(); defer { lock.unlock() }
        return hasExited ? process.terminationStatus : nil
    }

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
            self.consumeStdout(self.stdoutPipe.fileHandleForReading.availableData)
            self.lock.lock()
            self.hasExited = true
            let continuation = self.continuation
            self.continuation = nil
            self.lock.unlock()
            continuation?.finish()
        }
        do { try process.run() } catch { throw LaunchFailure(underlying: error) }
    }

    public func send(_ message: JSONRPCMessage) async throws {
        let data = try LSPFraming.encode(message)
        try stdinPipe.fileHandleForWriting.write(contentsOf: data)
    }

    public func messages() -> AsyncThrowingStream<JSONRPCMessage, Error> {
        AsyncThrowingStream { continuation in
            lock.lock()
            self.continuation = continuation
            let buffered = pendingMessages
            pendingMessages = []
            let exited = hasExited
            lock.unlock()
            for message in buffered { continuation.yield(message) }
            if exited { continuation.finish() }
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
        decoder.append(data)
        var decoded: [JSONRPCMessage] = []
        var failure: Error?
        do {
            while let payload = try decoder.next() {
                decoded.append(try JSONRPCMessage.decode(payload))
            }
        } catch { failure = error }
        let continuation = self.continuation
        if continuation == nil { pendingMessages.append(contentsOf: decoded) }
        lock.unlock()
        for message in decoded { continuation?.yield(message) }
        if let failure { continuation?.finish(throwing: failure) }
    }

    private func consumeStderr(_ data: Data) {
        guard !data.isEmpty, let onStderrLine else { return }
        lock.lock()
        stderrBuffer.append(data)
        var lines: [String] = []
        while let newline = stderrBuffer.firstIndex(of: UInt8(ascii: "\n")) {
            lines.append(String(decoding: stderrBuffer[stderrBuffer.startIndex..<newline], as: UTF8.self))
            stderrBuffer.removeSubrange(stderrBuffer.startIndex...newline)
        }
        lock.unlock()
        for line in lines { onStderrLine(line) }
    }
}
```

- [ ] **Step 4: Write the fake**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/Support/FakeLSPTransport.swift
import Foundation
import TillerRPC
@testable import TillerLSP

/// A transport whose peer is the test. Every later task uses it to drive
/// timing, cancellation and server death deterministically — behaviours a
/// recorded fixture cannot express.
final class FakeLSPTransport: LSPTransport, @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: AsyncThrowingStream<JSONRPCMessage, Error>.Continuation?
    private var _sent: [JSONRPCMessage] = []
    private var _isTerminated = false
    private var buffered: [JSONRPCMessage] = []

    var sent: [JSONRPCMessage] { lock.lock(); defer { lock.unlock() }; return _sent }
    var isTerminated: Bool { lock.lock(); defer { lock.unlock() }; return _isTerminated }

    func start() async throws {}

    func send(_ message: JSONRPCMessage) async throws {
        lock.lock(); _sent.append(message); lock.unlock()
    }

    func messages() -> AsyncThrowingStream<JSONRPCMessage, Error> {
        AsyncThrowingStream { continuation in
            lock.lock()
            self.continuation = continuation
            let pending = buffered
            buffered = []
            lock.unlock()
            for message in pending { continuation.yield(message) }
        }
    }

    func terminate() async { lock.lock(); _isTerminated = true; lock.unlock() }

    /// Delivers a server-to-client message. Safe to call before `messages()`.
    func emit(_ message: JSONRPCMessage) {
        lock.lock()
        let continuation = self.continuation
        if continuation == nil { buffered.append(message) }
        lock.unlock()
        continuation?.yield(message)
    }

    func fail(_ error: Error) {
        lock.lock(); let c = continuation; continuation = nil; lock.unlock()
        c?.finish(throwing: error)
    }

    /// Simulates the server exiting.
    func finish() {
        lock.lock(); let c = continuation; continuation = nil; lock.unlock()
        c?.finish()
    }
}
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `cd Packages/TillerLSP && swift test`
Expected: all pass (6 framing + 4 transport).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: add LSP process transport and programmable fake"
```

---

### Task 4: `LSPClient` — correlation, notifications, cancellation

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LSPClient.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LSPClientTests.swift`

**Interfaces:**
- Consumes: `LSPTransport` (Task 3), `JSONRPCMessage`, `JSONValue` (Task 1).
- Produces:
  - `actor LSPClient`
  - `init(transport: LSPTransport)`
  - `func start() async throws` — starts the transport and the read loop
  - `func request(_ method: String, params: JSONValue?, timeout: Duration = .seconds(2)) async throws -> JSONValue?`
  - `func notify(_ method: String, params: JSONValue?) async throws`
  - `func notifications() -> AsyncStream<(method: String, params: JSONValue?)>`
  - `func shutdown() async`
  - `enum LSPClientError: Error, Equatable { case serverError(JSONRPCError); case timedOut(method: String); case cancelled; case transportClosed }`

> **Cancellation matters more than it looks.** A completion request fired on keystroke *n* is worthless once keystroke *n+1* lands. Without `$/cancelRequest` the server keeps computing it, and on a cold rust-analyzer those stale requests queue up behind each other until the useful one is seconds late. Swift task cancellation must therefore propagate to the wire, which is what `withTaskCancellationHandler` does here.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LSPClientTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

@Test func requestResolvesWithTheMatchingResponse() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    async let result = client.request("initialize", params: .object([:]))
    try await waitUntil { await transport.sent.count == 1 }
    guard case .request(let id, let method, _) = transport.sent[0] else {
        Issue.record("expected a request"); return
    }
    #expect(method == "initialize")
    transport.emit(.response(id: id, result: .object(["capabilities": .object([:])]), error: nil))

    let value = try await result
    #expect(value?["capabilities"] != nil)
}

@Test func concurrentRequestsResolveToTheirOwnResponses() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    async let first = client.request("a", params: nil)
    async let second = client.request("b", params: nil)
    try await waitUntil { await transport.sent.count == 2 }

    // Answer out of order: correlation must be by id, not by arrival order.
    let ids: [JSONRPCID] = transport.sent.compactMap {
        if case .request(let id, _, _) = $0 { return id } else { return nil }
    }
    transport.emit(.response(id: ids[1], result: .string("second"), error: nil))
    transport.emit(.response(id: ids[0], result: .string("first"), error: nil))

    #expect(try await first?.stringValue == "first")
    #expect(try await second?.stringValue == "second")
}

@Test func requestThrowsServerErrorWhenTheResponseCarriesAnError() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    async let result = client.request("bad", params: nil)
    try await waitUntil { await transport.sent.count == 1 }
    guard case .request(let id, _, _) = transport.sent[0] else { Issue.record("no request"); return }
    transport.emit(.response(id: id, result: nil,
                             error: JSONRPCError(code: -32601, message: "method not found")))

    await #expect(throws: LSPClientError.self) { _ = try await result }
}

@Test func requestTimesOutAndSendsCancelRequest() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    await #expect(throws: LSPClientError.self) {
        _ = try await client.request("slow", params: nil, timeout: .milliseconds(50))
    }
    let cancels = await transport.sent.filter {
        if case .notification(let method, _) = $0 { return method == "$/cancelRequest" }
        return false
    }
    #expect(cancels.count == 1)
}

@Test func cancellingTheSwiftTaskSendsCancelRequest() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    let task = Task { try await client.request("slow", params: nil, timeout: .seconds(30)) }
    try await waitUntil { await transport.sent.count == 1 }
    task.cancel()
    _ = try? await task.value

    try await waitUntil {
        await transport.sent.contains {
            if case .notification(let method, _) = $0 { return method == "$/cancelRequest" }
            return false
        }
    }
}

@Test func serverNotificationsReachTheNotificationStream() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    var iterator = await client.notifications().makeAsyncIterator()
    transport.emit(.notification(
        method: "textDocument/publishDiagnostics",
        params: .object(["uri": .string("file:///a.rs"), "diagnostics": .array([])])))

    let received = await iterator.next()
    #expect(received?.method == "textDocument/publishDiagnostics")
    #expect(received?.params?["uri"]?.stringValue == "file:///a.rs")
}

@Test func pendingRequestsFailWhenTheServerExits() async throws {
    let transport = FakeLSPTransport()
    let client = LSPClient(transport: transport)
    try await client.start()

    async let result = client.request("initialize", params: nil, timeout: .seconds(30))
    try await waitUntil { await transport.sent.count == 1 }
    transport.finish()

    await #expect(throws: LSPClientError.self) { _ = try await result }
}

/// Polls `condition` until true or ~1 s elapses. Used instead of a fixed sleep
/// so the suite stays fast on a quiet machine and reliable on a loaded one.
private func waitUntil(_ condition: () async -> Bool) async throws {
    for _ in 0..<200 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(5))
    }
    Issue.record("condition never became true")
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LSPClientTests`
Expected: FAIL — `LSPClient` not defined.

- [ ] **Step 3: Write the client**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LSPClient.swift
import Foundation
import TillerRPC

public enum LSPClientError: Error, Equatable {
    case serverError(JSONRPCError)
    case timedOut(method: String)
    case cancelled
    case transportClosed
}

/// A JSON-RPC client speaking LSP to one server process.
///
/// Owns request/response correlation and one read loop. Server-initiated
/// *requests* (`window/showMessageRequest`, `workspace/configuration`) are
/// answered with a method-not-found error rather than ignored: a server that
/// never gets a reply may block waiting for one.
public actor LSPClient {
    private let transport: LSPTransport
    private var nextID = 1
    private var pending: [JSONRPCID: CheckedContinuation<JSONValue?, Error>] = [:]
    private var notificationContinuations: [UUID: AsyncStream<(method: String, params: JSONValue?)>.Continuation] = [:]
    private var readTask: Task<Void, Never>?
    private var isClosed = false

    public init(transport: LSPTransport) { self.transport = transport }

    public func start() async throws {
        try await transport.start()
        let stream = transport.messages()
        readTask = Task { [weak self] in
            do {
                for try await message in stream { await self?.handle(message) }
                await self?.closeAll(with: .transportClosed)
            } catch {
                await self?.closeAll(with: .transportClosed)
            }
        }
    }

    public func request(_ method: String, params: JSONValue?,
                        timeout: Duration = .seconds(2)) async throws -> JSONValue? {
        if isClosed { throw LSPClientError.transportClosed }
        let id = JSONRPCID.number(nextID)
        nextID += 1

        let timeoutTask = Task { [weak self] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            await self?.fail(id: id, with: .timedOut(method: method))
        }
        defer { timeoutTask.cancel() }

        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                pending[id] = continuation
                Task {
                    do { try await transport.send(.request(id: id, method: method, params: params)) }
                    catch { await self.fail(id: id, with: .transportClosed) }
                }
            }
        } onCancel: {
            Task { await self.fail(id: id, with: .cancelled) }
        }
    }

    public func notify(_ method: String, params: JSONValue?) async throws {
        try await transport.send(.notification(method: method, params: params))
    }

    public func notifications() -> AsyncStream<(method: String, params: JSONValue?)> {
        AsyncStream { continuation in
            let key = UUID()
            notificationContinuations[key] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.removeNotificationContinuation(key) }
            }
        }
    }

    public func shutdown() async {
        isClosed = true
        readTask?.cancel()
        _ = try? await request("shutdown", params: nil, timeout: .seconds(1))
        try? await notify("exit", params: nil)
        await transport.terminate()
        closeAll(with: .transportClosed)
    }

    // MARK: - Private

    private func removeNotificationContinuation(_ key: UUID) {
        notificationContinuations[key] = nil
    }

    private func handle(_ message: JSONRPCMessage) {
        switch message {
        case .response(let id, let result, let error):
            guard let continuation = pending.removeValue(forKey: id) else { return }
            if let error { continuation.resume(throwing: LSPClientError.serverError(error)) }
            else { continuation.resume(returning: result) }
        case .notification(let method, let params):
            for continuation in notificationContinuations.values {
                continuation.yield((method: method, params: params))
            }
        case .request(let id, _, _):
            // Answer rather than ignore: an unanswered server request can wedge
            // a server that waits on it before continuing.
            Task { [transport] in
                try? await transport.send(.response(
                    id: id, result: nil,
                    error: JSONRPCError(code: -32601, message: "unsupported by Tiller")))
            }
        }
    }

    /// Cancels one in-flight request on the wire and locally. Cancelling a
    /// request the server already answered is harmless and expected.
    private func fail(id: JSONRPCID, with error: LSPClientError) async {
        guard let continuation = pending.removeValue(forKey: id) else { return }
        let idValue: JSONValue = {
            switch id {
            case .number(let n): return .number(Double(n))
            case .string(let s): return .string(s)
            }
        }()
        try? await transport.send(.notification(
            method: "$/cancelRequest", params: .object(["id": idValue])))
        continuation.resume(throwing: error)
    }

    private func closeAll(with error: LSPClientError) {
        isClosed = true
        let waiting = pending
        pending = [:]
        for continuation in waiting.values { continuation.resume(throwing: error) }
        for continuation in notificationContinuations.values { continuation.finish() }
        notificationContinuations = [:]
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test`
Expected: all pass (6 framing + 4 transport + 7 client).

- [ ] **Step 5: Run the gate**

Run: `bash Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: add LSPClient with correlation, notifications and cancellation"
```

---

## Phase 2 — Protocol vocabulary and position arithmetic

### Task 5: LSP types and their union decoders

The four features need about ten types. This is the surface a full LSP library would have given for free — and the union shapes below are exactly where hand-written clients get it wrong, which is why each one has a test naming the shape it accepts.

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LSPTypes.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LSPTypesTests.swift`

**Interfaces:**
- Consumes: `JSONValue` (Task 1).
- Produces:
  - `struct LSPPosition: Sendable, Equatable { let line: Int; let character: Int }` — both 0-indexed, `character` in UTF-16 code units
  - `struct LSPRange: Sendable, Equatable { let start: LSPPosition; let end: LSPPosition }`
  - `struct LSPLocation: Sendable, Equatable { let uri: String; let range: LSPRange }`
  - `enum LSPDiagnosticSeverity: Int, Sendable { case error = 1, warning, information, hint }`
  - `struct LSPDiagnostic: Sendable, Equatable { let range: LSPRange; let severity: LSPDiagnosticSeverity; let message: String; let source: String? }`
  - `struct LSPCompletionItem: Sendable, Equatable { let label: String; let detail: String?; let documentation: String?; let insertText: String?; let kind: Int?; let sortText: String?; let deprecated: Bool }`
  - `LSPPosition.init?(json:)`, `LSPRange.init?(json:)`, `LSPDiagnostic.init?(json:)`, `LSPCompletionItem.init?(json:)`
  - `LSPCompletionItem.list(from: JSONValue?) -> [LSPCompletionItem]` — accepts both `CompletionItem[]` and `CompletionList`
  - `LSPLocation.list(from: JSONValue?) -> [LSPLocation]` — accepts `Location`, `Location[]`, `LocationLink[]`
  - `LSPHoverContent.plainText(from: JSONValue?) -> String?` — accepts `MarkedString | MarkedString[] | MarkupContent`

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LSPTypesTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

private func json(_ text: String) throws -> JSONValue {
    try JSONDecoder().decode(JSONValue.self, from: Data(text.utf8))
}

@Test func decodesAPositionAsZeroIndexed() throws {
    let position = try #require(LSPPosition(json: json(#"{"line":0,"character":0}"#)))
    #expect(position.line == 0)
    #expect(position.character == 0)
}

@Test func decodesADiagnosticAndDefaultsMissingSeverityToError() throws {
    let value = try json(#"""
    {"range":{"start":{"line":2,"character":4},"end":{"line":2,"character":9}},
     "message":"cannot find value `foo`","source":"rustc"}
    """#)
    let diagnostic = try #require(LSPDiagnostic(json: value))
    #expect(diagnostic.severity == .error)
    #expect(diagnostic.message == "cannot find value `foo`")
    #expect(diagnostic.source == "rustc")
    #expect(diagnostic.range.start.line == 2)
}

@Test func decodesDiagnosticSeverityWarning() throws {
    let value = try json(#"""
    {"range":{"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
     "severity":2,"message":"unused"}
    """#)
    #expect(LSPDiagnostic(json: value)?.severity == .warning)
}

@Test func completionAcceptsABareItemArray() throws {
    let value = try json(#"[{"label":"push"},{"label":"pop","detail":"fn()"}]"#)
    let items = LSPCompletionItem.list(from: value)
    #expect(items.map(\.label) == ["push", "pop"])
    #expect(items[1].detail == "fn()")
}

@Test func completionAcceptsACompletionListWrapper() throws {
    let value = try json(#"{"isIncomplete":true,"items":[{"label":"push"}]}"#)
    #expect(LSPCompletionItem.list(from: value).map(\.label) == ["push"])
}

@Test func completionReadsDocumentationFromBothStringAndMarkupContent() throws {
    let plain = try json(#"[{"label":"a","documentation":"docs"}]"#)
    #expect(LSPCompletionItem.list(from: plain)[0].documentation == "docs")
    let markup = try json(#"[{"label":"a","documentation":{"kind":"markdown","value":"**docs**"}}]"#)
    #expect(LSPCompletionItem.list(from: markup)[0].documentation == "**docs**")
}

@Test func definitionAcceptsASingleLocation() throws {
    let value = try json(#"""
    {"uri":"file:///a.rs","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":3}}}
    """#)
    let locations = LSPLocation.list(from: value)
    #expect(locations.count == 1)
    #expect(locations[0].uri == "file:///a.rs")
}

@Test func definitionAcceptsALocationArray() throws {
    let value = try json(#"""
    [{"uri":"file:///a.rs","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":3}}},
     {"uri":"file:///b.rs","range":{"start":{"line":9,"character":2},"end":{"line":9,"character":5}}}]
    """#)
    #expect(LSPLocation.list(from: value).map(\.uri) == ["file:///a.rs", "file:///b.rs"])
}

@Test func definitionAcceptsLocationLinksUsingTargetUriAndTargetSelectionRange() throws {
    let value = try json(#"""
    [{"targetUri":"file:///c.rs",
      "targetRange":{"start":{"line":0,"character":0},"end":{"line":20,"character":0}},
      "targetSelectionRange":{"start":{"line":4,"character":7},"end":{"line":4,"character":12}}}]
    """#)
    let locations = LSPLocation.list(from: value)
    #expect(locations.count == 1)
    #expect(locations[0].uri == "file:///c.rs")
    // The selection range is the symbol itself; the target range is its whole
    // body. Jumping to the body would land the caret on the wrong line.
    #expect(locations[0].range.start.line == 4)
    #expect(locations[0].range.start.character == 7)
}

@Test func definitionOfNullIsAnEmptyList() {
    #expect(LSPLocation.list(from: .null).isEmpty)
    #expect(LSPLocation.list(from: nil).isEmpty)
}

@Test func hoverAcceptsMarkupContent() throws {
    let value = try json(#"{"contents":{"kind":"markdown","value":"# Title"}}"#)
    #expect(LSPHoverContent.plainText(from: value) == "# Title")
}

@Test func hoverAcceptsABareString() throws {
    #expect(LSPHoverContent.plainText(from: try json(#"{"contents":"just text"}"#)) == "just text")
}

@Test func hoverAcceptsAMarkedStringObject() throws {
    let value = try json(#"{"contents":{"language":"rust","value":"fn main()"}}"#)
    #expect(LSPHoverContent.plainText(from: value) == "fn main()")
}

@Test func hoverJoinsAnArrayOfMarkedStrings() throws {
    let value = try json(#"{"contents":["one",{"language":"rust","value":"two"}]}"#)
    #expect(LSPHoverContent.plainText(from: value) == "one\n\ntwo")
}

@Test func hoverOfNullIsNil() throws {
    #expect(LSPHoverContent.plainText(from: .null) == nil)
    #expect(LSPHoverContent.plainText(from: try json(#"{"contents":null}"#)) == nil)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LSPTypesTests`
Expected: FAIL — types not defined.

- [ ] **Step 3: Write the types**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LSPTypes.swift
import Foundation
import TillerRPC

/// A position in a document. Both fields are **0-indexed**, and `character`
/// counts **UTF-16 code units** within the line — not bytes, not Characters.
public struct LSPPosition: Sendable, Equatable {
    public let line: Int
    public let character: Int

    public init(line: Int, character: Int) {
        self.line = line
        self.character = character
    }

    public init?(json: JSONValue?) {
        guard let line = json?["line"]?.intValue,
              let character = json?["character"]?.intValue else { return nil }
        self.init(line: line, character: character)
    }

    public var json: JSONValue {
        .object(["line": .number(Double(line)), "character": .number(Double(character))])
    }
}

public struct LSPRange: Sendable, Equatable {
    public let start: LSPPosition
    public let end: LSPPosition

    public init(start: LSPPosition, end: LSPPosition) {
        self.start = start
        self.end = end
    }

    public init?(json: JSONValue?) {
        guard let start = LSPPosition(json: json?["start"]),
              let end = LSPPosition(json: json?["end"]) else { return nil }
        self.init(start: start, end: end)
    }
}

public struct LSPLocation: Sendable, Equatable {
    public let uri: String
    public let range: LSPRange

    public init(uri: String, range: LSPRange) {
        self.uri = uri
        self.range = range
    }

    /// `textDocument/definition` answers with `Location | Location[] |
    /// LocationLink[] | null`. All four shapes collapse to a list here.
    public static func list(from value: JSONValue?) -> [LSPLocation] {
        guard let value, value != .null else { return [] }
        if case .array(let elements) = value { return elements.compactMap(single) }
        return [single(value)].compactMap { $0 }
    }

    private static func single(_ value: JSONValue) -> LSPLocation? {
        if let uri = value["uri"]?.stringValue, let range = LSPRange(json: value["range"]) {
            return LSPLocation(uri: uri, range: range)
        }
        // LocationLink: prefer targetSelectionRange (the symbol) over
        // targetRange (its whole body, which would scroll to the wrong line).
        if let uri = value["targetUri"]?.stringValue {
            let range = LSPRange(json: value["targetSelectionRange"])
                ?? LSPRange(json: value["targetRange"])
            if let range { return LSPLocation(uri: uri, range: range) }
        }
        return nil
    }
}

public enum LSPDiagnosticSeverity: Int, Sendable, Equatable {
    case error = 1, warning = 2, information = 3, hint = 4
}

public struct LSPDiagnostic: Sendable, Equatable {
    public let range: LSPRange
    public let severity: LSPDiagnosticSeverity
    public let message: String
    public let source: String?

    public init(range: LSPRange, severity: LSPDiagnosticSeverity, message: String, source: String?) {
        self.range = range
        self.severity = severity
        self.message = message
        self.source = source
    }

    public init?(json: JSONValue?) {
        guard let range = LSPRange(json: json?["range"]),
              let message = json?["message"]?.stringValue else { return nil }
        // The field is optional in the spec; the spec's own guidance is that a
        // client should then treat the severity as decided by the client. An
        // unlabelled problem is worth showing at full strength.
        let severity = (json?["severity"]?.intValue).flatMap(LSPDiagnosticSeverity.init(rawValue:)) ?? .error
        self.init(range: range, severity: severity, message: message,
                  source: json?["source"]?.stringValue)
    }
}

public struct LSPCompletionItem: Sendable, Equatable {
    public let label: String
    public let detail: String?
    public let documentation: String?
    public let insertText: String?
    public let kind: Int?
    public let sortText: String?
    public let deprecated: Bool

    /// `textDocument/completion` answers with `CompletionItem[] |
    /// CompletionList | null`.
    public static func list(from value: JSONValue?) -> [LSPCompletionItem] {
        guard let value, value != .null else { return [] }
        if case .array(let elements) = value { return elements.compactMap(Self.init(json:)) }
        if case .array(let elements)? = value["items"] { return elements.compactMap(Self.init(json:)) }
        return []
    }

    public init?(json: JSONValue?) {
        guard let label = json?["label"]?.stringValue else { return nil }
        self.label = label
        self.detail = json?["detail"]?.stringValue
        self.documentation = LSPHoverContent.text(from: json?["documentation"])
        self.insertText = json?["insertText"]?.stringValue ?? json?["textEdit"]?["newText"]?.stringValue
        self.kind = json?["kind"]?.intValue
        self.sortText = json?["sortText"]?.stringValue
        if case .bool(true)? = json?["deprecated"] { self.deprecated = true }
        else if case .array(let tags)? = json?["tags"] { self.deprecated = tags.contains(.number(1)) }
        else { self.deprecated = false }
    }
}

/// Flattens the several shapes LSP uses for human-readable text.
public enum LSPHoverContent {
    /// `Hover.contents` is `MarkedString | MarkedString[] | MarkupContent`,
    /// where `MarkedString` is itself `String | {language, value}`.
    public static func plainText(from value: JSONValue?) -> String? {
        guard let value, value != .null else { return nil }
        return text(from: value["contents"])
    }

    static func text(from value: JSONValue?) -> String? {
        guard let value, value != .null else { return nil }
        switch value {
        case .string(let text):
            return text.isEmpty ? nil : text
        case .array(let elements):
            let parts = elements.compactMap { text(from: $0) }
            return parts.isEmpty ? nil : parts.joined(separator: "\n\n")
        case .object:
            // MarkupContent uses `value`; MarkedString uses `value` too.
            return value["value"]?.stringValue
        default:
            return nil
        }
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test --filter LSPTypesTests`
Expected: 15 tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add LSP protocol types with union decoders"
```

---

### Task 6: `LineIndex` — the only place positions are converted

Three coordinate systems meet here: LSP (0-indexed line, UTF-16 character), `NSRange` (UTF-16 offset from document start), and `CursorPosition` (1-indexed line and column). Every conversion in the codebase goes through this file, so an off-by-one has one place to be wrong and one place to be fixed.

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LineIndex.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LineIndexTests.swift`

**Interfaces:**
- Consumes: `LSPPosition`, `LSPRange` (Task 5).
- Produces:
  - `struct LineIndex: Sendable { init(text: String) }`
  - `func utf16Offset(of position: LSPPosition) -> Int?`
  - `func position(atUTF16Offset offset: Int) -> LSPPosition?`
  - `func nsRange(of range: LSPRange) -> NSRange?`
  - `var lineCount: Int`

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LineIndexTests.swift
import Foundation
import Testing
@testable import TillerLSP

@Test func offsetOfTheDocumentStartIsZero() {
    let index = LineIndex(text: "let a = 1\nlet b = 2\n")
    #expect(index.utf16Offset(of: LSPPosition(line: 0, character: 0)) == 0)
}

@Test func offsetOfTheSecondLineSkipsTheNewline() {
    let index = LineIndex(text: "abc\ndef")
    #expect(index.utf16Offset(of: LSPPosition(line: 1, character: 0)) == 4)
    #expect(index.utf16Offset(of: LSPPosition(line: 1, character: 2)) == 6)
}

@Test func characterCountsUTF16UnitsNotCharacters() {
    // "🇮🇹" is 1 Character, 2 Unicode scalars, 4 UTF-16 code units.
    let index = LineIndex(text: "🇮🇹x")
    #expect(index.utf16Offset(of: LSPPosition(line: 0, character: 4)) == 4)
    #expect(index.position(atUTF16Offset: 4) == LSPPosition(line: 0, character: 4))
}

@Test func accentedCharactersOccupyOneUTF16Unit() {
    let index = LineIndex(text: "café\nx")
    #expect(index.utf16Offset(of: LSPPosition(line: 1, character: 0)) == 5)
}

@Test func handlesCRLFLineEndings() {
    let index = LineIndex(text: "abc\r\ndef")
    #expect(index.lineCount == 2)
    #expect(index.utf16Offset(of: LSPPosition(line: 1, character: 0)) == 5)
}

@Test func positionRoundTripsThroughOffset() {
    let index = LineIndex(text: "one\ntwo\nthree")
    for offset in 0...13 {
        let position = index.position(atUTF16Offset: offset)
        #expect(position != nil)
        #expect(index.utf16Offset(of: position!) == offset)
    }
}

@Test func nsRangeSpansFromStartToEnd() {
    let index = LineIndex(text: "let foo = 1\n")
    let range = index.nsRange(of: LSPRange(
        start: LSPPosition(line: 0, character: 4),
        end: LSPPosition(line: 0, character: 7)))
    #expect(range == NSRange(location: 4, length: 3))
}

@Test func nsRangeSpanningTwoLinesIncludesTheNewline() {
    let index = LineIndex(text: "ab\ncd")
    let range = index.nsRange(of: LSPRange(
        start: LSPPosition(line: 0, character: 1),
        end: LSPPosition(line: 1, character: 1)))
    #expect(range == NSRange(location: 1, length: 3))
}

@Test func returnsNilForALineBeyondTheDocument() {
    let index = LineIndex(text: "only one line")
    #expect(index.utf16Offset(of: LSPPosition(line: 7, character: 0)) == nil)
}

@Test func clampsACharacterBeyondTheLineEndToTheLineEnd() {
    // Servers legitimately report a column past the last character to mean
    // "end of line" — most often on a diagnostic covering a trailing token.
    let index = LineIndex(text: "abc\ndef")
    #expect(index.utf16Offset(of: LSPPosition(line: 0, character: 99)) == 3)
}

@Test func emptyDocumentHasOneLineAndOffsetZero() {
    let index = LineIndex(text: "")
    #expect(index.lineCount == 1)
    #expect(index.utf16Offset(of: LSPPosition(line: 0, character: 0)) == 0)
}

@Test func trailingNewlineCreatesAFinalEmptyLine() {
    let index = LineIndex(text: "a\n")
    #expect(index.lineCount == 2)
    #expect(index.utf16Offset(of: LSPPosition(line: 1, character: 0)) == 2)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LineIndexTests`
Expected: FAIL — `LineIndex` not defined.

- [ ] **Step 3: Write the index**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LineIndex.swift
import Foundation

/// Converts between LSP positions and UTF-16 offsets for one snapshot of a
/// document's text.
///
/// Everything here is in UTF-16 code units, which is what LSP counts and what
/// `NSString`/`NSRange` count. Swift's `String.Index` deliberately never
/// appears: mixing grapheme-cluster indices into this arithmetic is the bug
/// this type exists to make impossible.
///
/// An instance describes one immutable snapshot. Rebuild it when the text
/// changes rather than mutating it.
public struct LineIndex: Sendable {
    /// UTF-16 offset at which each line starts.
    private let lineStarts: [Int]
    /// UTF-16 offset at which each line's content ends, excluding the newline.
    private let lineEnds: [Int]
    private let totalLength: Int

    public var lineCount: Int { lineStarts.count }

    public init(text: String) {
        let utf16 = Array(text.utf16)
        var starts: [Int] = [0]
        var ends: [Int] = []
        var offset = 0
        while offset < utf16.count {
            let unit = utf16[offset]
            if unit == 0x0A {                                  // \n
                ends.append(offset)
                starts.append(offset + 1)
            } else if unit == 0x0D {                           // \r or \r\n
                ends.append(offset)
                if offset + 1 < utf16.count, utf16[offset + 1] == 0x0A {
                    offset += 1
                }
                starts.append(offset + 1)
            }
            offset += 1
        }
        ends.append(utf16.count)
        self.lineStarts = starts
        self.lineEnds = ends
        self.totalLength = utf16.count
    }

    /// `nil` when the line does not exist. A character past the end of a line
    /// is clamped: servers use that to mean "end of line".
    public func utf16Offset(of position: LSPPosition) -> Int? {
        guard position.line >= 0, position.line < lineStarts.count else { return nil }
        let start = lineStarts[position.line]
        let end = lineEnds[position.line]
        return min(start + max(0, position.character), end)
    }

    public func position(atUTF16Offset offset: Int) -> LSPPosition? {
        guard offset >= 0, offset <= totalLength else { return nil }
        // Binary search for the last line whose start is <= offset.
        var low = 0, high = lineStarts.count - 1, line = 0
        while low <= high {
            let mid = (low + high) / 2
            if lineStarts[mid] <= offset { line = mid; low = mid + 1 } else { high = mid - 1 }
        }
        return LSPPosition(line: line, character: offset - lineStarts[line])
    }

    public func nsRange(of range: LSPRange) -> NSRange? {
        guard let start = utf16Offset(of: range.start),
              let end = utf16Offset(of: range.end),
              end >= start else { return nil }
        return NSRange(location: start, length: end - start)
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test --filter LineIndexTests`
Expected: 12 tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: add LineIndex for UTF-16 position conversion"
```

---

### Task 7: Server catalog and login-shell binary detection

Decision 3 says language coverage is a table, not code. Decision 4 says Tiller never installs anything. This task is both: rows of data, plus the one mechanism that finds a binary the user installed with their own package manager.

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LanguageServerDefinition.swift`
- Create: `Packages/TillerLSP/Sources/TillerLSP/LanguageServerCatalog.swift`
- Create: `Packages/TillerLSP/Sources/TillerLSP/ExecutableLocator.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LanguageServerCatalogTests.swift`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `struct LanguageServerDefinition: Sendable, Equatable, Identifiable` with
    `id`, `displayName`, `executable`, `arguments`, `languageID`, `fileExtensions`, `rootMarkers`, `installCommand`
  - `enum LanguageServerCatalog { static let all: [LanguageServerDefinition] }`
  - `static func definition(forFileExtension ext: String) -> LanguageServerDefinition?`
  - `protocol ExecutableLocating: Sendable { func locate(_ executable: String) async -> String? }`
  - `actor LoginShellExecutableLocator: ExecutableLocating` with `init(shell: String = "/bin/zsh")` and an internal result cache
  - `func workspaceRoot(for fileURL: URL, boundedBy bound: URL, markers: [String]) -> URL?`

> **Why a login shell.** An app launched from Finder inherits `PATH=/usr/bin:/bin:/usr/sbin:/sbin`. `rust-analyzer` lives in `~/.cargo/bin`, `gopls` in `~/go/bin`, `typescript-language-server` in an npm prefix — all invisible. `TillerACP/AgentInstaller.swift` already solves this with `zsh -lc` for agent CLIs; this is the same mechanism for the same reason.

> **On untested rows.** Only Swift, Rust and C/C++ can be verified on the author's machine today. Every other row ships unverified, which is acceptable **because they are data**: a wrong row is a wrong row, not a wrong code path. Keep it that way — a single `if definition.id == …` special case anywhere in `TillerLSP` breaks decision 3 and makes coverage cost scale with language count again.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LanguageServerCatalogTests.swift
import Foundation
import Testing
@testable import TillerLSP

@Test func everyDefinitionHasAUniqueID() {
    let ids = LanguageServerCatalog.all.map(\.id)
    #expect(Set(ids).count == ids.count)
}

@Test func noFileExtensionIsClaimedByTwoDefinitions() {
    var seen: [String: String] = [:]
    for definition in LanguageServerCatalog.all {
        for ext in definition.fileExtensions {
            #expect(seen[ext] == nil, "\(ext) claimed by both \(seen[ext] ?? "") and \(definition.id)")
            seen[ext] = definition.id
        }
    }
}

@Test func everyDefinitionCarriesAnInstallCommandAndAtLeastOneRootMarker() {
    for definition in LanguageServerCatalog.all {
        #expect(!definition.installCommand.isEmpty, "\(definition.id) has no install command")
        #expect(!definition.rootMarkers.isEmpty, "\(definition.id) has no root marker")
        #expect(!definition.languageID.isEmpty, "\(definition.id) has no LSP languageId")
    }
}

@Test func resolvesRustFilesToRustAnalyzer() {
    #expect(LanguageServerCatalog.definition(forFileExtension: "rs")?.id == "rust-analyzer")
}

@Test func resolvesSwiftFilesToSourcekitLSP() {
    #expect(LanguageServerCatalog.definition(forFileExtension: "swift")?.id == "sourcekit-lsp")
}

@Test func extensionMatchIsCaseInsensitive() {
    #expect(LanguageServerCatalog.definition(forFileExtension: "RS")?.id == "rust-analyzer")
}

@Test func unknownExtensionResolvesToNothing() {
    #expect(LanguageServerCatalog.definition(forFileExtension: "wat") == nil)
}

@Test func workspaceRootIsTheNearestAncestorHoldingAMarker() throws {
    let base = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("lsp-root-\(UUID().uuidString)")
    let nested = base.appendingPathComponent("crate/src/inner")
    try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: base) }
    let marker = base.appendingPathComponent("crate/Cargo.toml")
    try Data().write(to: marker)

    let root = workspaceRoot(
        for: nested.appendingPathComponent("lib.rs"),
        boundedBy: base, markers: ["Cargo.toml"])
    #expect(root?.standardizedFileURL == base.appendingPathComponent("crate").standardizedFileURL)
}

@Test func workspaceRootFallsBackToTheBoundWhenNoMarkerExists() throws {
    let base = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("lsp-root-\(UUID().uuidString)")
    let nested = base.appendingPathComponent("src")
    try FileManager.default.createDirectory(at: nested, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: base) }

    let root = workspaceRoot(
        for: nested.appendingPathComponent("main.rs"),
        boundedBy: base, markers: ["Cargo.toml"])
    #expect(root?.standardizedFileURL == base.standardizedFileURL)
}

@Test func workspaceRootNeverEscapesTheBound() throws {
    let outside = URL(fileURLWithPath: "/tmp/definitely-not-under-the-bound/main.rs")
    let bound = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("bound-\(UUID().uuidString)")
    #expect(workspaceRoot(for: outside, boundedBy: bound, markers: ["Cargo.toml"]) == nil)
}

@Test func locatorReturnsAPathForABinaryThatCertainlyExists() async {
    // `ls` is in the default PATH on every macOS install, so this asserts the
    // login-shell plumbing works without depending on a language server.
    let path = await LoginShellExecutableLocator().locate("ls")
    #expect(path?.hasSuffix("/ls") == true)
}

@Test func locatorReturnsNilForABinaryThatDoesNotExist() async {
    #expect(await LoginShellExecutableLocator().locate("tiller-no-such-binary-xyz") == nil)
}

@Test func locatorCachesSoASecondLookupDoesNotRespawnAShell() async {
    let locator = LoginShellExecutableLocator()
    let first = await locator.locate("ls")
    let second = await locator.locate("ls")
    #expect(first == second)
    #expect(await locator.spawnCount == 1)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LanguageServerCatalogTests`
Expected: FAIL — nothing defined.

- [ ] **Step 3: Write the definition type**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LanguageServerDefinition.swift
import Foundation

/// One row of the server table. Pure data by design: adding a language must
/// never mean adding a code path (decision 3).
public struct LanguageServerDefinition: Sendable, Equatable, Identifiable {
    /// Stable identifier, also the conventional binary name.
    public let id: String
    public let displayName: String
    /// Binary to look up on the user's login-shell PATH.
    public let executable: String
    public let arguments: [String]
    /// The `languageId` sent in `textDocument/didOpen`, from the LSP spec's list.
    public let languageID: String
    /// Lower-cased, no leading dot.
    public let fileExtensions: [String]
    /// Files or directories whose presence marks a workspace root, nearest first.
    public let rootMarkers: [String]
    /// Shown, never executed (decision 4).
    public let installCommand: String

    public init(id: String, displayName: String, executable: String, arguments: [String] = [],
                languageID: String, fileExtensions: [String], rootMarkers: [String],
                installCommand: String) {
        self.id = id
        self.displayName = displayName
        self.executable = executable
        self.arguments = arguments
        self.languageID = languageID
        self.fileExtensions = fileExtensions
        self.rootMarkers = rootMarkers
        self.installCommand = installCommand
    }
}
```

- [ ] **Step 4: Write the catalog**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LanguageServerCatalog.swift
import Foundation

/// The static server table.
///
/// Rows for Swift, Rust and C/C++ are verified against the real binaries. The
/// rest are written from each server's documentation and ship unverified —
/// acceptable precisely because they are data. Adding a row must never require
/// touching any other file in this package.
public enum LanguageServerCatalog {
    public static let all: [LanguageServerDefinition] = [
        .init(id: "sourcekit-lsp", displayName: "Swift", executable: "sourcekit-lsp",
              languageID: "swift", fileExtensions: ["swift"],
              rootMarkers: ["Package.swift", "buildServer.json", "compile_commands.json"],
              installCommand: "xcode-select --install   # ships with Xcode"),
        .init(id: "rust-analyzer", displayName: "Rust", executable: "rust-analyzer",
              languageID: "rust", fileExtensions: ["rs"],
              rootMarkers: ["Cargo.toml"],
              installCommand: "rustup component add rust-analyzer"),
        .init(id: "clangd", displayName: "C / C++", executable: "clangd",
              languageID: "cpp", fileExtensions: ["c", "h", "cc", "cpp", "cxx", "hpp", "hh", "m", "mm"],
              rootMarkers: ["compile_commands.json", "CMakeLists.txt", "Makefile"],
              installCommand: "brew install llvm   # or xcode-select --install"),
        .init(id: "gopls", displayName: "Go", executable: "gopls",
              languageID: "go", fileExtensions: ["go"],
              rootMarkers: ["go.mod", "go.work"],
              installCommand: "go install golang.org/x/tools/gopls@latest"),
        .init(id: "typescript-language-server", displayName: "TypeScript / JavaScript",
              executable: "typescript-language-server", arguments: ["--stdio"],
              languageID: "typescript",
              fileExtensions: ["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"],
              rootMarkers: ["tsconfig.json", "jsconfig.json", "package.json"],
              installCommand: "npm install -g typescript-language-server typescript"),
        .init(id: "pyright", displayName: "Python", executable: "pyright-langserver",
              arguments: ["--stdio"], languageID: "python", fileExtensions: ["py", "pyi"],
              rootMarkers: ["pyproject.toml", "setup.py", "requirements.txt", "Pipfile"],
              installCommand: "npm install -g pyright"),
        .init(id: "ruby-lsp", displayName: "Ruby", executable: "ruby-lsp",
              languageID: "ruby", fileExtensions: ["rb", "rake"],
              rootMarkers: ["Gemfile", ".ruby-version"],
              installCommand: "gem install ruby-lsp"),
        .init(id: "jdtls", displayName: "Java", executable: "jdtls",
              languageID: "java", fileExtensions: ["java"],
              rootMarkers: ["pom.xml", "build.gradle", "build.gradle.kts", ".project"],
              installCommand: "brew install jdtls"),
        .init(id: "kotlin-language-server", displayName: "Kotlin",
              executable: "kotlin-language-server", languageID: "kotlin",
              fileExtensions: ["kt", "kts"],
              rootMarkers: ["build.gradle.kts", "build.gradle", "settings.gradle.kts"],
              installCommand: "brew install kotlin-language-server"),
        .init(id: "zls", displayName: "Zig", executable: "zls",
              languageID: "zig", fileExtensions: ["zig"],
              rootMarkers: ["build.zig"],
              installCommand: "brew install zls"),
        .init(id: "lua-language-server", displayName: "Lua",
              executable: "lua-language-server", languageID: "lua", fileExtensions: ["lua"],
              rootMarkers: [".luarc.json", "stylua.toml", ".git"],
              installCommand: "brew install lua-language-server"),
        .init(id: "phpactor", displayName: "PHP", executable: "phpactor",
              arguments: ["language-server"], languageID: "php", fileExtensions: ["php"],
              rootMarkers: ["composer.json"],
              installCommand: "brew install phpactor"),
        .init(id: "elixir-ls", displayName: "Elixir", executable: "elixir-ls",
              languageID: "elixir", fileExtensions: ["ex", "exs"],
              rootMarkers: ["mix.exs"],
              installCommand: "brew install elixir-ls"),
        .init(id: "haskell-language-server", displayName: "Haskell",
              executable: "haskell-language-server-wrapper", arguments: ["--lsp"],
              languageID: "haskell", fileExtensions: ["hs", "lhs"],
              rootMarkers: ["stack.yaml", "cabal.project", "*.cabal"],
              installCommand: "ghcup install hls"),
        .init(id: "ocaml-lsp", displayName: "OCaml", executable: "ocamllsp",
              languageID: "ocaml", fileExtensions: ["ml", "mli"],
              rootMarkers: ["dune-project"],
              installCommand: "opam install ocaml-lsp-server"),
        .init(id: "solargraph-erb", displayName: "ERB", executable: "solargraph",
              arguments: ["stdio"], languageID: "eruby", fileExtensions: ["erb"],
              rootMarkers: ["Gemfile"],
              installCommand: "gem install solargraph"),
        .init(id: "vscode-json-languageserver", displayName: "JSON",
              executable: "vscode-json-language-server", arguments: ["--stdio"],
              languageID: "json", fileExtensions: ["json", "jsonc"],
              rootMarkers: ["package.json", ".git"],
              installCommand: "npm install -g vscode-langservers-extracted"),
        .init(id: "yaml-language-server", displayName: "YAML",
              executable: "yaml-language-server", arguments: ["--stdio"],
              languageID: "yaml", fileExtensions: ["yml", "yaml"],
              rootMarkers: [".git"],
              installCommand: "npm install -g yaml-language-server"),
        .init(id: "bash-language-server", displayName: "Shell",
              executable: "bash-language-server", arguments: ["start"],
              languageID: "shellscript", fileExtensions: ["sh", "bash", "zsh"],
              rootMarkers: [".git"],
              installCommand: "npm install -g bash-language-server"),
        .init(id: "taplo", displayName: "TOML", executable: "taplo",
              arguments: ["lsp", "stdio"], languageID: "toml", fileExtensions: ["toml"],
              rootMarkers: [".git"],
              installCommand: "brew install taplo")
    ]

    public static func definition(forFileExtension ext: String) -> LanguageServerDefinition? {
        let needle = ext.lowercased()
        return all.first { $0.fileExtensions.contains(needle) }
    }

    public static func definition(for fileURL: URL) -> LanguageServerDefinition? {
        definition(forFileExtension: fileURL.pathExtension)
    }
}

/// Walks up from `fileURL` looking for the nearest directory holding one of
/// `markers`, stopping at `bound` — the worktree. Falls back to `bound` itself,
/// which is what most servers do with an unmarked directory anyway.
/// Returns `nil` when `fileURL` is not under `bound`.
public func workspaceRoot(for fileURL: URL, boundedBy bound: URL, markers: [String]) -> URL? {
    let boundPath = bound.standardizedFileURL.path
    var directory = fileURL.standardizedFileURL.deletingLastPathComponent()
    guard directory.path == boundPath || directory.path.hasPrefix(boundPath + "/") else { return nil }

    while directory.path.hasPrefix(boundPath) {
        for marker in markers {
            if FileManager.default.fileExists(atPath: directory.appendingPathComponent(marker).path) {
                return directory
            }
        }
        if directory.path == boundPath { break }
        directory = directory.deletingLastPathComponent()
    }
    return bound.standardizedFileURL
}
```

- [ ] **Step 5: Write the locator**

```swift
// Packages/TillerLSP/Sources/TillerLSP/ExecutableLocator.swift
import Foundation

public protocol ExecutableLocating: Sendable {
    /// Absolute path, or `nil` when the binary is not on the user's PATH.
    func locate(_ executable: String) async -> String?
}

/// Resolves binaries through a **login** shell.
///
/// A GUI app launched from Finder inherits `/usr/bin:/bin:/usr/sbin:/sbin` and
/// nothing else, so `~/.cargo/bin/rust-analyzer` and `~/go/bin/gopls` are
/// invisible to a plain `FileManager` PATH walk. `TillerACP/AgentInstaller`
/// already goes through `zsh -lc` for agent CLIs for the same reason.
///
/// Results are cached: a login shell sources the user's whole `.zprofile` and
/// costs 50-100 ms, and the Settings panel asks about twenty binaries at once.
public actor LoginShellExecutableLocator: ExecutableLocating {
    private let shell: String
    private var cache: [String: String?] = [:]
    private(set) var spawnCount = 0

    public init(shell: String = "/bin/zsh") { self.shell = shell }

    public func locate(_ executable: String) async -> String? {
        if let cached = cache[executable] { return cached }
        let resolved = run(executable)
        cache[executable] = resolved
        return resolved
    }

    /// Drops every cached answer, so a server installed while Tiller was
    /// running is picked up on the next look.
    public func invalidate() { cache.removeAll() }

    private func run(_ executable: String) -> String? {
        spawnCount += 1
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: shell)
        // `command -v` is a POSIX builtin: no dependency on `which` being present.
        process.arguments = ["-lc", "command -v \(shellQuoted(executable))"]
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0 else { return nil }
        let path = String(decoding: data, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return path.isEmpty ? nil : path
    }

    private func shellQuoted(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: #"'\''"#) + "'"
    }
}
```

- [ ] **Step 6: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test --filter LanguageServerCatalogTests`
Expected: 13 tests pass. If `noFileExtensionIsClaimedByTwoDefinitions` fails, two rows overlap — fix the table, not the test.

- [ ] **Step 7: Run the gate**

Run: `bash Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: add language server catalog and login-shell locator"
```

---

## Phase 3 — Lifecycle and document sync

### Task 8: `LanguageServerHost` — lazy start, cap, restart budget, document sync

The single stateful heart of the package. Everything above it is pure; everything below it is UI.

**Files:**
- Create: `Packages/TillerLSP/Sources/TillerLSP/LanguageServerStatus.swift`
- Create: `Packages/TillerLSP/Sources/TillerLSP/LanguageServerHost.swift`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/LanguageServerHostTests.swift`

**Interfaces:**
- Consumes: `LSPClient` (Task 4), `LSPTransport`/`FakeLSPTransport` (Task 3), `LSPTypes` (Task 5), `LineIndex` (Task 6), catalog + locator (Task 7).
- Produces:
  - `enum LanguageServerStatus: Sendable, Equatable { case notInstalled, starting, indexing(String), ready, stopped(reason: String) }`
  - `struct ServerKey: Sendable, Hashable { let definitionID: String; let rootPath: String }`
  - `actor LanguageServerHost`
  - `init(catalog: [LanguageServerDefinition] = LanguageServerCatalog.all, locator: ExecutableLocating = LoginShellExecutableLocator(), transportFactory: @Sendable @escaping (LanguageServerDefinition, String, String) -> LSPTransport = LanguageServerHost.processTransport, maxConcurrentServers: Int = 4, restartBudget: Int = 2)`
  - `func openDocument(fileURL: URL, worktreeRoot: URL, text: String) async -> ServerKey?`
  - `func changeDocument(fileURL: URL, text: String) async`
  - `func closeDocument(fileURL: URL) async`
  - `func completion(fileURL: URL, position: LSPPosition) async -> [LSPCompletionItem]`
  - `func definition(fileURL: URL, position: LSPPosition) async -> [LSPLocation]`
  - `func hover(fileURL: URL, position: LSPPosition) async -> String?`
  - `func status(for key: ServerKey) -> LanguageServerStatus`
  - `func statusUpdates() -> AsyncStream<(key: ServerKey, status: LanguageServerStatus)>`
  - `func diagnosticUpdates() -> AsyncStream<(uri: String, diagnostics: [LSPDiagnostic])>`
  - `func shutdownAll() async`

> **What the cap protects.** Tiller opens several worktrees at once, and a worktree is a separate checkout, so each is a separate LSP root indexed independently. rust-analyzer sits at 1-3 GB on a medium project; four of them is already a lot of a laptop. LRU eviction means the cap degrades the least-recently-used editor rather than the machine.

> **Debounce placement.** The 150 ms debounce lives here, not in the view. A view-level debounce would be bypassed by any other caller, and decision 2 says this package must stay usable from the control socket later.

- [ ] **Step 1: Write the failing tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/LanguageServerHostTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

// MARK: - Support

/// A locator with a fixed answer table, so tests never touch the real PATH.
private struct StubLocator: ExecutableLocating {
    let installed: Set<String>
    func locate(_ executable: String) async -> String? {
        installed.contains(executable) ? "/usr/local/bin/\(executable)" : nil
    }
}

/// Hands out fakes and remembers them, so a test can drive the server side.
private final class TransportRegistry: @unchecked Sendable {
    private let lock = NSLock()
    private var transports: [FakeLSPTransport] = []

    var all: [FakeLSPTransport] { lock.lock(); defer { lock.unlock() }; return transports }

    func make() -> FakeLSPTransport {
        let transport = FakeLSPTransport()
        lock.lock(); transports.append(transport); lock.unlock()
        return transport
    }

    /// Answers the `initialize` request every server sends first, then the
    /// `initialized` notification, so the host reaches `.ready`.
    func completeHandshake(at index: Int) async throws {
        let transport = all[index]
        try await waitUntil { !transport.sent.isEmpty }
        guard case .request(let id, let method, _) = transport.sent[0], method == "initialize" else {
            Issue.record("first message was not initialize"); return
        }
        transport.emit(.response(id: id, result: .object(["capabilities": .object([:])]), error: nil))
    }
}

private let fixtureRoot = URL(fileURLWithPath: "/tmp/tiller-lsp-tests/crate")
private let fixtureFile = fixtureRoot.appendingPathComponent("src/lib.rs")

private func makeHost(_ registry: TransportRegistry,
                      installed: Set<String> = ["rust-analyzer"],
                      maxConcurrentServers: Int = 4,
                      restartBudget: Int = 2) -> LanguageServerHost {
    LanguageServerHost(
        catalog: LanguageServerCatalog.all,
        locator: StubLocator(installed: installed),
        transportFactory: { _, _, _ in registry.make() },
        maxConcurrentServers: maxConcurrentServers,
        restartBudget: restartBudget)
}

private func waitUntil(_ condition: () async -> Bool) async throws {
    for _ in 0..<400 {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(5))
    }
    Issue.record("condition never became true")
}

// MARK: - Tests

@Test func openingADocumentStartsExactlyOneServerAndSendsInitializeThenDidOpen() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)

    let key = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "fn main() {}")
    #expect(key?.definitionID == "rust-analyzer")
    try await registry.completeHandshake(at: 0)

    try await waitUntil {
        registry.all[0].sent.contains {
            if case .notification(let method, _) = $0 { return method == "textDocument/didOpen" }
            return false
        }
    }
    #expect(registry.all.count == 1)
}

@Test func didOpenCarriesTheCatalogLanguageIDAndTheFileURI() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "fn main() {}")
    try await registry.completeHandshake(at: 0)

    try await waitUntil { registry.all[0].sent.count >= 3 }
    let didOpen = registry.all[0].sent.compactMap { message -> JSONValue? in
        if case .notification("textDocument/didOpen", let params) = message { return params }
        return nil
    }.first
    let item = try #require(didOpen?["textDocument"])
    #expect(item["languageId"]?.stringValue == "rust")
    #expect(item["uri"]?.stringValue == fixtureFile.absoluteString)
    #expect(item["version"]?.intValue == 1)
    #expect(item["text"]?.stringValue == "fn main() {}")
}

@Test func aSecondDocumentInTheSameRootReusesTheSameServer() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)
    _ = await host.openDocument(
        fileURL: fixtureRoot.appendingPathComponent("src/other.rs"),
        worktreeRoot: fixtureRoot, text: "b")

    #expect(registry.all.count == 1)
}

@Test func aDocumentInADifferentWorktreeStartsASecondServer() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    let otherRoot = URL(fileURLWithPath: "/tmp/tiller-lsp-tests-2/crate")
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    _ = await host.openDocument(
        fileURL: otherRoot.appendingPathComponent("src/lib.rs"), worktreeRoot: otherRoot, text: "b")

    #expect(registry.all.count == 2)
}

@Test func openingAFileWhoseServerIsNotInstalledStartsNothingAndReportsNotInstalled() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry, installed: [])
    let key = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")

    #expect(registry.all.isEmpty)
    #expect(await host.status(for: try #require(key)) == .notInstalled)
}

@Test func openingAFileWithNoCatalogRowReturnsNoKey() async {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    let key = await host.openDocument(
        fileURL: fixtureRoot.appendingPathComponent("notes.wat"),
        worktreeRoot: fixtureRoot, text: "")
    #expect(key == nil)
    #expect(registry.all.isEmpty)
}

@Test func statusReachesReadyAfterTheInitializeResponse() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    let key = try #require(await host.openDocument(
        fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a"))
    #expect(await host.status(for: key) == .starting)

    try await registry.completeHandshake(at: 0)
    try await waitUntil { await host.status(for: key) == .ready }
}

@Test func progressNotificationsMoveStatusToIndexingAndBackToReady() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    let key = try #require(await host.openDocument(
        fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a"))
    try await registry.completeHandshake(at: 0)
    try await waitUntil { await host.status(for: key) == .ready }

    registry.all[0].emit(.notification(method: "$/progress", params: .object([
        "token": .string("indexing"),
        "value": .object(["kind": .string("begin"), "title": .string("Indexing")])])))
    try await waitUntil { await host.status(for: key) == .indexing("Indexing") }

    registry.all[0].emit(.notification(method: "$/progress", params: .object([
        "token": .string("indexing"), "value": .object(["kind": .string("end")])])))
    try await waitUntil { await host.status(for: key) == .ready }
}

@Test func changeIsDebouncedIntoASingleDidChangeWithAnIncrementedVersion() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)
    try await waitUntil {
        registry.all[0].sent.contains {
            if case .notification("textDocument/didOpen", _) = $0 { return true }; return false
        }
    }

    for text in ["ab", "abc", "abcd"] {
        await host.changeDocument(fileURL: fixtureFile, text: text)
    }
    try await Task.sleep(for: .milliseconds(400))

    let changes = registry.all[0].sent.compactMap { message -> JSONValue? in
        if case .notification("textDocument/didChange", let params) = message { return params }
        return nil
    }
    #expect(changes.count == 1)
    #expect(changes[0]["textDocument"]?["version"]?.intValue == 2)
    // Full-document sync: one content change with no range (decision 10).
    #expect(changes[0]["contentChanges"]?[0]?["text"]?.stringValue == "abcd")
    #expect(changes[0]["contentChanges"]?[0]?["range"] == nil)
}

@Test func closingTheLastDocumentOfARootShutsTheServerDown() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)
    await host.closeDocument(fileURL: fixtureFile)

    try await waitUntil { registry.all[0].isTerminated }
}

@Test func closingOneOfTwoDocumentsKeepsTheServerAlive() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    let second = fixtureRoot.appendingPathComponent("src/other.rs")
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)
    _ = await host.openDocument(fileURL: second, worktreeRoot: fixtureRoot, text: "b")

    await host.closeDocument(fileURL: fixtureFile)
    try await Task.sleep(for: .milliseconds(50))
    #expect(registry.all[0].isTerminated == false)
}

@Test func exceedingTheCapEvictsTheLeastRecentlyUsedServer() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry, maxConcurrentServers: 2)
    for index in 0..<3 {
        let root = URL(fileURLWithPath: "/tmp/tiller-lsp-cap-\(index)")
        _ = await host.openDocument(
            fileURL: root.appendingPathComponent("lib.rs"), worktreeRoot: root, text: "x")
        try await registry.completeHandshake(at: index)
    }
    // The first root was least recently used when the third arrived.
    try await waitUntil { registry.all[0].isTerminated }
    #expect(registry.all[1].isTerminated == false)
    #expect(registry.all[2].isTerminated == false)
}

@Test func publishDiagnosticsReachTheDiagnosticStream() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)

    var iterator = await host.diagnosticUpdates().makeAsyncIterator()
    registry.all[0].emit(.notification(
        method: "textDocument/publishDiagnostics",
        params: .object([
            "uri": .string("file:///tmp/tiller-lsp-tests/crate/src/lib.rs"),
            "diagnostics": .array([.object([
                "range": .object([
                    "start": .object(["line": .number(0), "character": .number(0)]),
                    "end": .object(["line": .number(0), "character": .number(1)])]),
                "severity": .number(1),
                "message": .string("boom")])])])))

    let update = await iterator.next()
    #expect(update?.diagnostics.count == 1)
    #expect(update?.diagnostics[0].message == "boom")
    #expect(update?.diagnostics[0].severity == .error)
}

@Test func diagnosticsArrivingForAFileWithNoOpenTabAreStillDelivered() async throws {
    // Decision 12: the panel shows everything the server publishes.
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)

    var iterator = await host.diagnosticUpdates().makeAsyncIterator()
    registry.all[0].emit(.notification(
        method: "textDocument/publishDiagnostics",
        params: .object([
            "uri": .string("file:///tmp/tiller-lsp-tests/crate/src/never-opened.rs"),
            "diagnostics": .array([.object([
                "range": .object([
                    "start": .object(["line": .number(3), "character": .number(0)]),
                    "end": .object(["line": .number(3), "character": .number(2)])]),
                "message": .string("unused import")])])])))

    let update = await iterator.next()
    #expect(update?.uri.hasSuffix("never-opened.rs") == true)
    #expect(update?.diagnostics.count == 1)
}

@Test func anUnexpectedExitRestartsTheServerUpToTheBudgetThenStopsTrying() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry, restartBudget: 2)
    let key = try #require(await host.openDocument(
        fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a"))
    try await registry.completeHandshake(at: 0)
    try await waitUntil { await host.status(for: key) == .ready }

    registry.all[0].finish()                        // crash 1
    try await waitUntil { registry.all.count == 2 }
    try await registry.completeHandshake(at: 1)

    registry.all[1].finish()                        // crash 2
    try await waitUntil { registry.all.count == 3 }
    try await registry.completeHandshake(at: 2)

    registry.all[2].finish()                        // crash 3 — budget spent
    try await waitUntil {
        if case .stopped = await host.status(for: key) { return true }; return false
    }
    try await Task.sleep(for: .milliseconds(100))
    #expect(registry.all.count == 3)
}

@Test func completionReturnsItemsFromTheServer() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry)
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    try await registry.completeHandshake(at: 0)

    async let items = host.completion(fileURL: fixtureFile, position: LSPPosition(line: 0, character: 1))
    try await waitUntil {
        registry.all[0].sent.contains {
            if case .request(_, "textDocument/completion", _) = $0 { return true }; return false
        }
    }
    let request = registry.all[0].sent.last!
    guard case .request(let id, _, let params) = request else { Issue.record("no request"); return }
    #expect(params?["position"]?["line"]?.intValue == 0)
    #expect(params?["textDocument"]?["uri"]?.stringValue == fixtureFile.absoluteString)
    registry.all[0].emit(.response(
        id: id, result: .array([.object(["label": .string("push")])]), error: nil))

    #expect(await items.map(\.label) == ["push"])
}

@Test func completionOnAStoppedServerReturnsEmptyRatherThanThrowing() async throws {
    let registry = TransportRegistry()
    let host = makeHost(registry, installed: [])
    _ = await host.openDocument(fileURL: fixtureFile, worktreeRoot: fixtureRoot, text: "a")
    let items = await host.completion(fileURL: fixtureFile, position: LSPPosition(line: 0, character: 0))
    #expect(items.isEmpty)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd Packages/TillerLSP && swift test --filter LanguageServerHostTests`
Expected: FAIL — `LanguageServerHost` not defined.

- [ ] **Step 3: Write the status type**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LanguageServerStatus.swift
import Foundation

/// What the editor header shows (decision 16).
///
/// The distinction that matters: an empty completion popup looks identical
/// whether the server is missing, still indexing, dead, or simply has nothing
/// to suggest. Only this value can tell the user which.
public enum LanguageServerStatus: Sendable, Equatable {
    case notInstalled
    case starting
    /// Carries the server's own progress title, e.g. "Indexing".
    case indexing(String)
    case ready
    case stopped(reason: String)
}
```

- [ ] **Step 4: Write the host**

```swift
// Packages/TillerLSP/Sources/TillerLSP/LanguageServerHost.swift
import Foundation
import TillerRPC

/// Identifies one running server: one definition, one workspace root.
public struct ServerKey: Sendable, Hashable {
    public let definitionID: String
    public let rootPath: String
}

/// Owns every language server process, one per (definition × workspace root).
///
/// Servers start lazily on first document open and stop when their last
/// document closes (decision 9). A worktree is a separate checkout, so two
/// worktrees of one project are two roots and two indexes — which is why the
/// concurrent cap exists and evicts least-recently-used rather than letting
/// the machine decide.
public actor LanguageServerHost {
    public static let processTransport: @Sendable (LanguageServerDefinition, String, String) -> LSPTransport = {
        definition, executablePath, root in
        LSPProcessTransport(
            executable: executablePath, arguments: definition.arguments, cwd: root,
            environment: nil, onStderrLine: nil)
    }

    private struct Server {
        let definition: LanguageServerDefinition
        let client: LSPClient
        var status: LanguageServerStatus
        var openDocuments: Set<String> = []
        var restartsUsed = 0
        var lastUsed = Date()
        var pumpTask: Task<Void, Never>?
    }

    private struct Document {
        let key: ServerKey
        let uri: String
        let languageID: String
        var version: Int
        var debounceTask: Task<Void, Never>?
    }

    private let catalog: [LanguageServerDefinition]
    private let locator: ExecutableLocating
    private let transportFactory: @Sendable (LanguageServerDefinition, String, String) -> LSPTransport
    private let maxConcurrentServers: Int
    private let restartBudget: Int
    private let changeDebounce: Duration

    private var servers: [ServerKey: Server] = [:]
    private var documents: [String: Document] = [:]           // keyed by file path
    private var statusContinuations: [UUID: AsyncStream<(key: ServerKey, status: LanguageServerStatus)>.Continuation] = [:]
    private var diagnosticContinuations: [UUID: AsyncStream<(uri: String, diagnostics: [LSPDiagnostic])>.Continuation] = [:]

    public init(catalog: [LanguageServerDefinition] = LanguageServerCatalog.all,
                locator: ExecutableLocating = LoginShellExecutableLocator(),
                transportFactory: @Sendable @escaping (LanguageServerDefinition, String, String) -> LSPTransport
                    = LanguageServerHost.processTransport,
                maxConcurrentServers: Int = 4,
                restartBudget: Int = 2,
                changeDebounce: Duration = .milliseconds(150)) {
        self.catalog = catalog
        self.locator = locator
        self.transportFactory = transportFactory
        self.maxConcurrentServers = maxConcurrentServers
        self.restartBudget = restartBudget
        self.changeDebounce = changeDebounce
    }

    // MARK: - Documents

    /// Returns the key of the server that will serve this file, or `nil` when
    /// no catalog row claims its extension. A key is returned even when the
    /// binary is missing, so the caller can show `.notInstalled`.
    @discardableResult
    public func openDocument(fileURL: URL, worktreeRoot: URL, text: String) async -> ServerKey? {
        guard let definition = catalog.first(where: {
            $0.fileExtensions.contains(fileURL.pathExtension.lowercased())
        }) else { return nil }
        guard let root = workspaceRoot(for: fileURL, boundedBy: worktreeRoot,
                                       markers: definition.rootMarkers) else { return nil }
        let key = ServerKey(definitionID: definition.id, rootPath: root.path)

        documents[fileURL.path] = Document(
            key: key, uri: fileURL.absoluteString, languageID: definition.languageID, version: 1)

        await ensureServer(key: key, definition: definition, root: root)
        guard var server = servers[key], server.status != .notInstalled else { return key }

        server.openDocuments.insert(fileURL.path)
        server.lastUsed = Date()
        servers[key] = server

        try? await server.client.notify("textDocument/didOpen", params: .object([
            "textDocument": .object([
                "uri": .string(fileURL.absoluteString),
                "languageId": .string(definition.languageID),
                "version": .number(1),
                "text": .string(text)])]))
        return key
    }

    public func changeDocument(fileURL: URL, text: String) async {
        guard var document = documents[fileURL.path] else { return }
        document.debounceTask?.cancel()
        // Full-document sync (decision 10): every notification carries the whole
        // file, so a dropped intermediate state can never desynchronise the
        // server. The debounce exists because typing produces ~10 edits/second
        // and every intermediate state is one nobody will ever read.
        let debounce = changeDebounce
        document.debounceTask = Task { [weak self] in
            try? await Task.sleep(for: debounce)
            guard !Task.isCancelled else { return }
            await self?.flushChange(path: fileURL.path, uri: fileURL.absoluteString, text: text)
        }
        documents[fileURL.path] = document
    }

    private func flushChange(path: String, uri: String, text: String) async {
        guard var document = documents[path], let server = servers[document.key],
              server.status != .notInstalled else { return }
        document.version += 1
        documents[path] = document
        try? await server.client.notify("textDocument/didChange", params: .object([
            "textDocument": .object(["uri": .string(uri), "version": .number(Double(document.version))]),
            "contentChanges": .array([.object(["text": .string(text)])])]))
    }

    public func closeDocument(fileURL: URL) async {
        guard let document = documents.removeValue(forKey: fileURL.path) else { return }
        document.debounceTask?.cancel()
        guard var server = servers[document.key] else { return }
        try? await server.client.notify("textDocument/didClose", params: .object([
            "textDocument": .object(["uri": .string(document.uri)])]))
        server.openDocuments.remove(fileURL.path)
        servers[document.key] = server
        if server.openDocuments.isEmpty { await stop(key: document.key, reason: "no open documents") }
    }

    // MARK: - Requests

    public func completion(fileURL: URL, position: LSPPosition) async -> [LSPCompletionItem] {
        let value = try? await request("textDocument/completion", fileURL: fileURL, position: position)
        return LSPCompletionItem.list(from: value)
    }

    public func definition(fileURL: URL, position: LSPPosition) async -> [LSPLocation] {
        let value = try? await request("textDocument/definition", fileURL: fileURL, position: position)
        return LSPLocation.list(from: value)
    }

    public func hover(fileURL: URL, position: LSPPosition) async -> String? {
        let value = try? await request("textDocument/hover", fileURL: fileURL, position: position)
        return LSPHoverContent.plainText(from: value)
    }

    /// Every failure — no server, missing binary, timeout, dead process —
    /// collapses to "no result". A completion popup is not the place to
    /// surface an error; the header status is (decision 16).
    private func request(_ method: String, fileURL: URL, position: LSPPosition) async throws -> JSONValue? {
        guard let document = documents[fileURL.path],
              var server = servers[document.key],
              server.status == .ready || isIndexing(server.status) else { return nil }
        server.lastUsed = Date()
        servers[document.key] = server
        return try await server.client.request(method, params: .object([
            "textDocument": .object(["uri": .string(document.uri)]),
            "position": position.json]))
    }

    private func isIndexing(_ status: LanguageServerStatus) -> Bool {
        if case .indexing = status { return true }
        return false
    }

    // MARK: - Status and diagnostics

    public func status(for key: ServerKey) -> LanguageServerStatus {
        servers[key]?.status ?? .stopped(reason: "not started")
    }

    public func statusUpdates() -> AsyncStream<(key: ServerKey, status: LanguageServerStatus)> {
        AsyncStream { continuation in
            let id = UUID()
            statusContinuations[id] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.dropStatusContinuation(id) }
            }
        }
    }

    public func diagnosticUpdates() -> AsyncStream<(uri: String, diagnostics: [LSPDiagnostic])> {
        AsyncStream { continuation in
            let id = UUID()
            diagnosticContinuations[id] = continuation
            continuation.onTermination = { [weak self] _ in
                Task { await self?.dropDiagnosticContinuation(id) }
            }
        }
    }

    private func dropStatusContinuation(_ id: UUID) { statusContinuations[id] = nil }
    private func dropDiagnosticContinuation(_ id: UUID) { diagnosticContinuations[id] = nil }

    private func setStatus(_ status: LanguageServerStatus, for key: ServerKey) {
        guard var server = servers[key] else { return }
        guard server.status != status else { return }
        server.status = status
        servers[key] = server
        for continuation in statusContinuations.values { continuation.yield((key: key, status: status)) }
    }

    // MARK: - Lifecycle

    private func ensureServer(key: ServerKey, definition: LanguageServerDefinition, root: URL) async {
        if servers[key] != nil { return }
        guard let executablePath = await locator.locate(definition.executable) else {
            servers[key] = Server(definition: definition,
                                  client: LSPClient(transport: NullTransport()),
                                  status: .notInstalled)
            for continuation in statusContinuations.values {
                continuation.yield((key: key, status: .notInstalled))
            }
            return
        }
        await evictIfOverCap()
        await launch(key: key, definition: definition, root: root, executablePath: executablePath,
                     restartsUsed: 0)
    }

    private func launch(key: ServerKey, definition: LanguageServerDefinition, root: URL,
                        executablePath: String, restartsUsed: Int) async {
        let transport = transportFactory(definition, executablePath, root.path)
        let client = LSPClient(transport: transport)
        servers[key] = Server(definition: definition, client: client, status: .starting,
                              restartsUsed: restartsUsed)

        do { try await client.start() } catch {
            setStatus(.stopped(reason: "failed to launch: \(error)"), for: key)
            return
        }

        let notifications = await client.notifications()
        servers[key]?.pumpTask = Task { [weak self] in
            for await notification in notifications {
                await self?.handle(notification: notification, key: key)
            }
            // The stream finishing means the process exited.
            await self?.handleExit(key: key, definition: definition, root: root,
                                   executablePath: executablePath)
        }

        Task { [weak self] in
            await self?.performHandshake(key: key, root: root)
        }
    }

    private func performHandshake(key: ServerKey, root: URL) async {
        guard let server = servers[key] else { return }
        let params = JSONValue.object([
            "processId": .number(Double(ProcessInfo.processInfo.processIdentifier)),
            "rootUri": .string(root.absoluteString),
            "workspaceFolders": .array([.object([
                "uri": .string(root.absoluteString),
                "name": .string(root.lastPathComponent)])]),
            "capabilities": .object([
                "textDocument": .object([
                    "synchronization": .object(["dynamicRegistration": .bool(false)]),
                    "completion": .object([
                        "completionItem": .object([
                            "snippetSupport": .bool(false),
                            "documentationFormat": .array([.string("markdown"), .string("plaintext")])])]),
                    "definition": .object(["linkSupport": .bool(true)]),
                    "hover": .object([
                        "contentFormat": .array([.string("markdown"), .string("plaintext")])]),
                    "publishDiagnostics": .object(["relatedInformation": .bool(false)])]),
                "window": .object(["workDoneProgress": .bool(true)])]),
            // Declared explicitly: the default is utf-16, which is what
            // LineIndex assumes and what NSRange counts. Never negotiate away
            // from it without changing LineIndex first.
            "general": .object(["positionEncodings": .array([.string("utf-16")])])])

        do {
            _ = try await server.client.request("initialize", params: params, timeout: .seconds(10))
            try await server.client.notify("initialized", params: .object([:]))
            setStatus(.ready, for: key)
        } catch {
            setStatus(.stopped(reason: "initialize failed: \(error)"), for: key)
        }
    }

    private func handle(notification: (method: String, params: JSONValue?), key: ServerKey) {
        switch notification.method {
        case "textDocument/publishDiagnostics":
            guard let uri = notification.params?["uri"]?.stringValue else { return }
            var diagnostics: [LSPDiagnostic] = []
            if case .array(let raw)? = notification.params?["diagnostics"] {
                diagnostics = raw.compactMap { LSPDiagnostic(json: $0) }
            }
            for continuation in diagnosticContinuations.values {
                continuation.yield((uri: uri, diagnostics: diagnostics))
            }
        case "$/progress":
            let kind = notification.params?["value"]?["kind"]?.stringValue
            if kind == "begin" {
                let title = notification.params?["value"]?["title"]?.stringValue ?? "Working"
                setStatus(.indexing(title), for: key)
            } else if kind == "end" {
                setStatus(.ready, for: key)
            }
        default:
            break
        }
    }

    private func handleExit(key: ServerKey, definition: LanguageServerDefinition, root: URL,
                            executablePath: String) async {
        guard let server = servers[key] else { return }
        if case .stopped = server.status { return }          // deliberate shutdown
        // A server that dies repeatedly does so for a structural reason — a
        // malformed project, an incompatible version. Restarting forever would
        // burn CPU and log noise without ever converging (decision 16).
        guard server.restartsUsed < restartBudget else {
            setStatus(.stopped(reason: "server exited \(restartBudget + 1) times"), for: key)
            return
        }
        let used = server.restartsUsed + 1
        let documentsToReopen = server.openDocuments
        servers[key] = nil
        await launch(key: key, definition: definition, root: root,
                     executablePath: executablePath, restartsUsed: used)
        servers[key]?.openDocuments = documentsToReopen
    }

    private func evictIfOverCap() async {
        let live = servers.filter { $0.value.status != .notInstalled }
        guard live.count >= maxConcurrentServers else { return }
        guard let oldest = live.min(by: { $0.value.lastUsed < $1.value.lastUsed })?.key else { return }
        await stop(key: oldest, reason: "evicted: more than \(maxConcurrentServers) servers")
    }

    private func stop(key: ServerKey, reason: String) async {
        guard let server = servers[key] else { return }
        setStatus(.stopped(reason: reason), for: key)
        server.pumpTask?.cancel()
        await server.client.shutdown()
        servers[key] = nil
        for (path, document) in documents where document.key == key {
            documents[path]?.debounceTask?.cancel()
            _ = path
        }
    }

    public func shutdownAll() async {
        for key in servers.keys { await stop(key: key, reason: "app shutting down") }
    }
}

/// Stands in for a server that was never launched because its binary is
/// missing. Every call is a no-op, so `notInstalled` needs no special-casing
/// at each call site.
private struct NullTransport: LSPTransport {
    func start() async throws {}
    func send(_ message: JSONRPCMessage) async throws {}
    func messages() -> AsyncThrowingStream<JSONRPCMessage, Error> {
        AsyncThrowingStream { $0.finish() }
    }
    func terminate() async {}
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test --filter LanguageServerHostTests`
Expected: 17 tests pass.

- [ ] **Step 6: Run the whole package and the gate**

Run: `cd Packages/TillerLSP && swift test && cd ../.. && bash Scripts/ci.sh`
Expected: all `TillerLSP` tests pass; `CI OK`.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: add LanguageServerHost with lazy lifecycle, cap and restart budget"
```

---

### Task 9: Record a real `rust-analyzer` session as a fixture

Everything so far was tested against hand-written JSON. This task checks that assumption against a server that actually exists — once, offline, with the result committed.

**Files:**
- Create: `Scripts/record-lsp-fixture.sh`
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/Fixtures/rust-analyzer-session.ndjson` (generated, committed)
- Create: `Packages/TillerLSP/Tests/TillerLSPTests/FixtureReplayTests.swift`
- Modify: `Packages/TillerLSP/Package.swift` (declare the fixture resource)

**Interfaces:**
- Consumes: `LSPTypes` (Task 5), `JSONRPCMessage` (Task 1).
- Produces: a committed `.ndjson` where each line is one server-to-client JSON-RPC message, and tests that decode every line without loss.

> **Why the fixture is not a live test.** `rust-analyzer` indexes for tens of seconds before it answers usefully, and `ci.sh` must stay fast and deterministic. Recording once and replaying forever gives the decoding coverage without the process. What a fixture cannot cover — debounce, cancellation, a server dying mid-request — is already covered by `FakeLSPTransport`.

- [ ] **Step 1: Write the recording script**

```bash
#!/bin/bash
# Scripts/record-lsp-fixture.sh
# Records one rust-analyzer session against a throwaway crate.
# Requires: rust-analyzer on PATH. Run from the repo root. Offline-safe.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
out="$repo_root/Packages/TillerLSP/Tests/TillerLSPTests/Fixtures/rust-analyzer-session.ndjson"
mkdir -p "$(dirname "$out")"

server="$(command -v rust-analyzer || true)"
[ -n "$server" ] || { echo "rust-analyzer not on PATH"; exit 1; }

# A disposable crate: the recording must not depend on this repo's contents,
# and rust-analyzer must not be pointed at anything the author cares about.
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/src"
cat > "$scratch/Cargo.toml" <<'TOML'
[package]
name = "fixture"
version = "0.1.0"
edition = "2021"
TOML
cat > "$scratch/src/lib.rs" <<'RS'
pub struct Counter { pub value: i64 }

impl Counter {
    pub fn increment(&mut self) { self.value += 1; }
}

pub fn broken() -> i64 { undefined_symbol() }
RS

python3 - "$server" "$scratch" "$out" <<'PY'
import json, subprocess, sys, threading, time

server, cwd, out_path = sys.argv[1:]
root_uri = "file://" + cwd
doc_uri = root_uri + "/src/lib.rs"
with open(cwd + "/src/lib.rs") as handle:
    text = handle.read()

proc = subprocess.Popen([server], cwd=cwd, stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)

def send(payload):
    body = json.dumps(payload).encode()
    proc.stdin.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
    proc.stdin.flush()

captured = []

def read_loop():
    buffer = b""
    while True:
        chunk = proc.stdout.read(1)
        if not chunk:
            return
        buffer += chunk
        if b"\r\n\r\n" not in buffer:
            continue
        header, rest = buffer.split(b"\r\n\r\n", 1)
        length = next(int(line.split(b":")[1]) for line in header.split(b"\r\n")
                      if line.lower().startswith(b"content-length"))
        while len(rest) < length:
            rest += proc.stdout.read(length - len(rest))
        captured.append(rest[:length].decode())
        buffer = rest[length:]

threading.Thread(target=read_loop, daemon=True).start()

send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "processId": None, "rootUri": root_uri,
    "capabilities": {"textDocument": {
        "completion": {"completionItem": {"documentationFormat": ["markdown", "plaintext"]}},
        "definition": {"linkSupport": True},
        "hover": {"contentFormat": ["markdown", "plaintext"]},
        "publishDiagnostics": {}}},
    "workspaceFolders": [{"uri": root_uri, "name": "fixture"}]}})
time.sleep(3)
send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
send({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {
    "textDocument": {"uri": doc_uri, "languageId": "rust", "version": 1, "text": text}}})

# Let the crate be indexed so completion and diagnostics are real.
time.sleep(12)

# `.` after `self` on the increment line — a completion with real items.
send({"jsonrpc": "2.0", "id": 2, "method": "textDocument/completion", "params": {
    "textDocument": {"uri": doc_uri}, "position": {"line": 3, "character": 39}}})
# The `Counter` in `impl Counter` — a definition that resolves in-file.
send({"jsonrpc": "2.0", "id": 3, "method": "textDocument/definition", "params": {
    "textDocument": {"uri": doc_uri}, "position": {"line": 2, "character": 6}}})
send({"jsonrpc": "2.0", "id": 4, "method": "textDocument/hover", "params": {
    "textDocument": {"uri": doc_uri}, "position": {"line": 0, "character": 12}}})
time.sleep(6)

proc.terminate()
with open(out_path, "w") as handle:
    for line in captured:
        handle.write(line + "\n")
print(f"captured {len(captured)} messages -> {out_path}")
PY
```

- [ ] **Step 2: Make it executable and record**

```bash
chmod +x Scripts/record-lsp-fixture.sh
bash Scripts/record-lsp-fixture.sh
```

Expected: `captured N messages -> …/rust-analyzer-session.ndjson` with N ≥ 6.

If `rust-analyzer` is not installed, install it (`rustup component add rust-analyzer`) — this step is the one place in the plan where a real server is required, and it happens on the author's machine, never in CI.

- [ ] **Step 3: Declare the fixture as a test resource**

```swift
// Packages/TillerLSP/Package.swift — the test target only
        .testTarget(
            name: "TillerLSPTests",
            dependencies: ["TillerLSP", "TillerRPC"],
            resources: [.copy("Fixtures")])
```

- [ ] **Step 4: Write the replay tests**

```swift
// Packages/TillerLSP/Tests/TillerLSPTests/FixtureReplayTests.swift
import Foundation
import Testing
import TillerRPC
@testable import TillerLSP

private func fixtureMessages() throws -> [JSONRPCMessage] {
    let url = try #require(Bundle.module.url(
        forResource: "rust-analyzer-session", withExtension: "ndjson",
        subdirectory: "Fixtures"))
    return try String(contentsOf: url, encoding: .utf8)
        .split(separator: "\n", omittingEmptySubsequences: true)
        .map { try JSONRPCMessage.decode(Data($0.utf8)) }
}

@Test func everyRecordedMessageDecodes() throws {
    let messages = try fixtureMessages()
    #expect(messages.count >= 6)
}

@Test func theRecordedInitializeResponseAdvertisesCompletionAndHover() throws {
    let capabilities = try fixtureMessages().compactMap { message -> JSONValue? in
        if case .response(.number(1), let result, _) = message { return result?["capabilities"] }
        return nil
    }.first
    let value = try #require(capabilities)
    #expect(value["completionProvider"] != nil)
    #expect(value["hoverProvider"] != nil)
    #expect(value["definitionProvider"] != nil)
}

@Test func theRecordedCompletionResponseParsesIntoItems() throws {
    let result = try fixtureMessages().compactMap { message -> JSONValue? in
        if case .response(.number(2), let result, _) = message { return result }
        return nil
    }.first
    let items = LSPCompletionItem.list(from: try #require(result))
    #expect(!items.isEmpty)
    #expect(items.allSatisfy { !$0.label.isEmpty })
}

@Test func theRecordedDefinitionResponseParsesIntoALocation() throws {
    let result = try fixtureMessages().compactMap { message -> JSONValue? in
        if case .response(.number(3), let result, _) = message { return result }
        return nil
    }.first
    let locations = LSPLocation.list(from: try #require(result))
    #expect(locations.count >= 1)
    #expect(locations[0].uri.hasSuffix(".rs"))
}

@Test func theRecordedHoverResponseParsesIntoText() throws {
    let result = try fixtureMessages().compactMap { message -> JSONValue? in
        if case .response(.number(4), let result, _) = message { return result }
        return nil
    }.first
    #expect(LSPHoverContent.plainText(from: try #require(result))?.isEmpty == false)
}

@Test func theRecordedDiagnosticsParseAndIncludeTheDeliberateError() throws {
    // The fixture crate calls `undefined_symbol()`, so rustc must complain.
    let published = try fixtureMessages().compactMap { message -> JSONValue? in
        if case .notification("textDocument/publishDiagnostics", let params) = message { return params }
        return nil
    }
    #expect(!published.isEmpty, "no publishDiagnostics recorded — re-record with a longer wait")
    let all = published.flatMap { params -> [LSPDiagnostic] in
        guard case .array(let raw)? = params["diagnostics"] else { return [] }
        return raw.compactMap { LSPDiagnostic(json: $0) }
    }
    #expect(all.contains { $0.message.contains("undefined_symbol") })
}

@Test func everyRecordedProgressNotificationHasAReadableKind() throws {
    let kinds = try fixtureMessages().compactMap { message -> String? in
        if case .notification("$/progress", let params) = message {
            return params?["value"]?["kind"]?.stringValue
        }
        return nil
    }
    #expect(kinds.allSatisfy { ["begin", "report", "end"].contains($0) })
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cd Packages/TillerLSP && swift test --filter FixtureReplayTests`
Expected: 7 tests pass. A failure here means a real server disagrees with the decoders written in Task 5 — fix the decoder, never the fixture.

- [ ] **Step 6: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "test: record and replay a real rust-analyzer session"
```

---

## Phase 4 — App bridge and completion

### Task 10: `LanguageServerCenter` — the App-side entry point

One `@MainActor @Observable` object owns the host, mirrors its two streams into observable state, and is the only thing App code talks to. Views never touch `LanguageServerHost` directly.

**Files:**
- Create: `App/LanguageServers/LanguageServerCenter.swift`
- Create: `AppTests/LanguageServerCenterTests.swift`
- Modify: `App/AppModel.swift` (own one instance, close documents on tab close)

**Interfaces:**
- Consumes: `LanguageServerHost`, `LanguageServerStatus`, `ServerKey`, `LSPDiagnostic` (Task 8).
- Produces:
  - `@MainActor @Observable final class LanguageServerCenter`
  - `init(host: LanguageServerHost = LanguageServerHost())`
  - `func open(fileURL: URL, worktreeRoot: URL, text: String) async`
  - `func change(fileURL: URL, text: String) async`
  - `func close(fileURL: URL) async`
  - `func status(for fileURL: URL) -> LanguageServerStatus?`
  - `func diagnostics(for fileURL: URL) -> [LSPDiagnostic]`
  - `func diagnosticsByPath(under root: URL) -> [(url: URL, diagnostics: [LSPDiagnostic])]`
  - `var host: LanguageServerHost { get }`
  - `nonisolated static func isReadOnlyLocation(_ url: URL, worktreeRoot: URL?) -> Bool`

> **Why the read-only rule lives here.** Decision 13 makes it a property of a URL relative to a worktree, not of the editor view. Keeping it a `nonisolated static` function makes it directly testable and impossible to forget at a second call site.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LanguageServerCenterTests.swift
import Foundation
import Testing
import TillerLSP
@testable import Tiller

@MainActor
@Test func diagnosticsAreStoredPerURLAndReplacedNotAppended() async {
    let center = LanguageServerCenter()
    let url = URL(fileURLWithPath: "/tmp/a.rs")
    let range = LSPRange(start: LSPPosition(line: 0, character: 0),
                         end: LSPPosition(line: 0, character: 1))

    center.applyDiagnostics(uri: url.absoluteString, diagnostics: [
        LSPDiagnostic(range: range, severity: .error, message: "first", source: nil)])
    #expect(center.diagnostics(for: url).map(\.message) == ["first"])

    // publishDiagnostics always carries the complete list for a file, so the
    // previous set must be replaced — appending would leave fixed errors on screen.
    center.applyDiagnostics(uri: url.absoluteString, diagnostics: [
        LSPDiagnostic(range: range, severity: .warning, message: "second", source: nil)])
    #expect(center.diagnostics(for: url).map(\.message) == ["second"])
}

@MainActor
@Test func anEmptyDiagnosticListClearsTheFile() {
    let center = LanguageServerCenter()
    let url = URL(fileURLWithPath: "/tmp/a.rs")
    center.applyDiagnostics(uri: url.absoluteString, diagnostics: [
        LSPDiagnostic(range: LSPRange(start: LSPPosition(line: 0, character: 0),
                                      end: LSPPosition(line: 0, character: 1)),
                      severity: .error, message: "x", source: nil)])
    center.applyDiagnostics(uri: url.absoluteString, diagnostics: [])
    #expect(center.diagnostics(for: url).isEmpty)
}

@MainActor
@Test func diagnosticsUnderARootIncludeFilesThatWereNeverOpened() {
    // Decision 12.
    let center = LanguageServerCenter()
    let root = URL(fileURLWithPath: "/tmp/crate")
    let range = LSPRange(start: LSPPosition(line: 0, character: 0),
                         end: LSPPosition(line: 0, character: 1))
    center.applyDiagnostics(uri: root.appendingPathComponent("src/never.rs").absoluteString,
                            diagnostics: [LSPDiagnostic(range: range, severity: .warning,
                                                        message: "unused", source: nil)])
    center.applyDiagnostics(uri: URL(fileURLWithPath: "/tmp/elsewhere/x.rs").absoluteString,
                            diagnostics: [LSPDiagnostic(range: range, severity: .error,
                                                        message: "other root", source: nil)])

    let grouped = center.diagnosticsByPath(under: root)
    #expect(grouped.count == 1)
    #expect(grouped[0].url.lastPathComponent == "never.rs")
}

@MainActor
@Test func groupedDiagnosticsAreSortedByPathThenByLine() {
    let center = LanguageServerCenter()
    let root = URL(fileURLWithPath: "/tmp/crate")
    func diagnostic(_ line: Int, _ message: String) -> LSPDiagnostic {
        LSPDiagnostic(range: LSPRange(start: LSPPosition(line: line, character: 0),
                                      end: LSPPosition(line: line, character: 1)),
                      severity: .error, message: message, source: nil)
    }
    center.applyDiagnostics(uri: root.appendingPathComponent("z.rs").absoluteString,
                            diagnostics: [diagnostic(1, "z1")])
    center.applyDiagnostics(uri: root.appendingPathComponent("a.rs").absoluteString,
                            diagnostics: [diagnostic(5, "a5"), diagnostic(2, "a2")])

    let grouped = center.diagnosticsByPath(under: root)
    #expect(grouped.map { $0.url.lastPathComponent } == ["a.rs", "z.rs"])
    #expect(grouped[0].diagnostics.map(\.message) == ["a2", "a5"])
}

@Test func aFileOutsideTheWorktreeIsReadOnly() {
    #expect(LanguageServerCenter.isReadOnlyLocation(
        URL(fileURLWithPath: "/Users/me/.cargo/registry/src/serde/lib.rs"),
        worktreeRoot: URL(fileURLWithPath: "/Users/me/project")))
}

@Test func aFileInsideTheWorktreeIsWritable() {
    #expect(!LanguageServerCenter.isReadOnlyLocation(
        URL(fileURLWithPath: "/Users/me/project/src/main.rs"),
        worktreeRoot: URL(fileURLWithPath: "/Users/me/project")))
}

@Test func buildDirectoriesInsideTheWorktreeAreStillReadOnly() {
    let root = URL(fileURLWithPath: "/Users/me/project")
    for path in ["/Users/me/project/target/debug/build/x.rs",
                 "/Users/me/project/.build/checkouts/dep/Sources/a.swift",
                 "/Users/me/project/node_modules/pkg/index.js",
                 "/Users/me/project/Pods/Lib/Lib.m",
                 "/Users/me/project/DerivedData/x.swift"] {
        #expect(LanguageServerCenter.isReadOnlyLocation(URL(fileURLWithPath: path), worktreeRoot: root),
                "\(path) should be read-only")
    }
}

@Test func aPathContainingTheWordTargetAsAnOrdinaryDirectoryStaysWritable() {
    // "targets" is not "target": matching must be per path component.
    #expect(!LanguageServerCenter.isReadOnlyLocation(
        URL(fileURLWithPath: "/Users/me/project/targets/main.rs"),
        worktreeRoot: URL(fileURLWithPath: "/Users/me/project")))
}

@Test func withoutAWorktreeRootEverythingIsReadOnly() {
    #expect(LanguageServerCenter.isReadOnlyLocation(
        URL(fileURLWithPath: "/tmp/x.rs"), worktreeRoot: nil))
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServerCenterTests 2>&1 | tail -20`
Expected: FAIL — `LanguageServerCenter` not defined.

Do **not** add `CODE_SIGNING_ALLOWED=NO` or `-derivedDataPath` to that command: with either, the test host hangs in dyld before discovery on managed Macs.

- [ ] **Step 3: Write the center**

```swift
// App/LanguageServers/LanguageServerCenter.swift
import Foundation
import Observation
import TillerLSP

/// The only App-side entry point to language servers.
///
/// Mirrors the host's diagnostic and status streams into observable state so
/// SwiftUI can read them, and owns the two policies that are App concerns
/// rather than protocol concerns: which locations are read-only, and how
/// diagnostics are grouped for the Problems panel.
@MainActor
@Observable
final class LanguageServerCenter {
    let host: LanguageServerHost

    /// Keyed by file path. Replaced wholesale per publish, never appended to.
    private(set) var diagnosticsByURI: [String: [LSPDiagnostic]] = [:]
    private(set) var statusByKey: [ServerKey: LanguageServerStatus] = [:]
    private var keyByPath: [String: ServerKey] = [:]

    /// Directories whose contents are generated or vendored. Editing them is
    /// almost always a mistake, and go-to-definition lands in them routinely.
    private static let readOnlyComponents: Set<String> = [
        ".build", "build", "target", "node_modules", "Pods", "DerivedData",
        ".swiftpm", "vendor", "Carthage", ".venv", "site-packages"
    ]

    init(host: LanguageServerHost = LanguageServerHost()) {
        self.host = host
        observeDiagnostics()
        observeStatus()
    }

    // MARK: - Documents

    func open(fileURL: URL, worktreeRoot: URL, text: String) async {
        guard let key = await host.openDocument(
            fileURL: fileURL, worktreeRoot: worktreeRoot, text: text) else { return }
        keyByPath[fileURL.standardizedFileURL.path] = key
        statusByKey[key] = await host.status(for: key)
    }

    func change(fileURL: URL, text: String) async {
        await host.changeDocument(fileURL: fileURL, text: text)
    }

    func close(fileURL: URL) async {
        await host.closeDocument(fileURL: fileURL)
        keyByPath[fileURL.standardizedFileURL.path] = nil
    }

    // MARK: - Reads

    func status(for fileURL: URL) -> LanguageServerStatus? {
        guard let key = keyByPath[fileURL.standardizedFileURL.path] else { return nil }
        return statusByKey[key]
    }

    func diagnostics(for fileURL: URL) -> [LSPDiagnostic] {
        diagnosticsByURI[fileURL.absoluteString] ?? []
    }

    /// Every file the servers have reported a problem for under `root`,
    /// including files with no open tab (decision 12). Sorted by path, then
    /// by line, so the panel does not reshuffle on every publish.
    func diagnosticsByPath(under root: URL) -> [(url: URL, diagnostics: [LSPDiagnostic])] {
        let rootPath = root.standardizedFileURL.path
        return diagnosticsByURI.compactMap { uri, diagnostics -> (URL, [LSPDiagnostic])? in
            guard !diagnostics.isEmpty, let url = URL(string: uri), url.isFileURL else { return nil }
            let path = url.standardizedFileURL.path
            guard path == rootPath || path.hasPrefix(rootPath + "/") else { return nil }
            let sorted = diagnostics.sorted {
                ($0.range.start.line, $0.range.start.character)
                    < ($1.range.start.line, $1.range.start.character)
            }
            return (url, sorted)
        }
        .sorted { $0.0.path < $1.0.path }
        .map { (url: $0.0, diagnostics: $0.1) }
    }

    // MARK: - Policy

    /// Decision 13: anything outside the worktree, and anything inside a build
    /// or dependency directory, opens read-only. Go-to-definition lands in
    /// `~/.cargo/registry` routinely, and that cache is shared across every
    /// project on the machine — a stray ⌘S there is not a local mistake.
    nonisolated static func isReadOnlyLocation(_ url: URL, worktreeRoot: URL?) -> Bool {
        guard let worktreeRoot else { return true }
        let rootPath = worktreeRoot.standardizedFileURL.path
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(rootPath + "/") else { return true }
        let relative = path.dropFirst(rootPath.count + 1)
        return relative.split(separator: "/").contains { readOnlyComponents.contains(String($0)) }
    }

    // MARK: - Internal, exercised directly by tests

    func applyDiagnostics(uri: String, diagnostics: [LSPDiagnostic]) {
        if diagnostics.isEmpty { diagnosticsByURI[uri] = nil }
        else { diagnosticsByURI[uri] = diagnostics }
    }

    private func observeDiagnostics() {
        Task { [weak self] in
            guard let host = self?.host else { return }
            for await update in await host.diagnosticUpdates() {
                self?.applyDiagnostics(uri: update.uri, diagnostics: update.diagnostics)
            }
        }
    }

    private func observeStatus() {
        Task { [weak self] in
            guard let host = self?.host else { return }
            for await update in await host.statusUpdates() {
                self?.statusByKey[update.key] = update.status
            }
        }
    }
}
```

- [ ] **Step 4: Own one instance in `AppModel`**

Add next to the other long-lived models in `App/AppModel.swift`:

```swift
    /// Language servers for the code editor. One per app, keyed internally by
    /// (server × worktree root).
    let languageServers = LanguageServerCenter()
```

In the code path that discards a code tab's document — the same place `codeDocuments[tab.id]` is removed — close the document so its server can stop:

```swift
        if let url = tab.codeFileURL {
            Task { await languageServers.close(fileURL: url) }
        }
```

- [ ] **Step 5: Run to verify they pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServerCenterTests 2>&1 | tail -20`
Expected: 9 tests pass.

- [ ] **Step 6: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: add LanguageServerCenter bridging LSP into the app"
```

---

### Task 11: Completion in the editor

The first user-visible payoff, and the binary verification for everything below it: if the popup shows real symbols from `rust-analyzer`, the client works.

**Files:**
- Create: `App/CodeEditor/LSPCompletionDelegate.swift`
- Create: `AppTests/LSPCompletionDelegateTests.swift`
- Modify: `App/CodeEditor/CodeEditorTabView.swift`

**Interfaces:**
- Consumes: `LanguageServerCenter` (Task 10), `LSPCompletionItem`, `LineIndex`, `LSPPosition`.
- Produces:
  - `@MainActor final class LSPCompletionDelegate: CodeSuggestionDelegate`
  - `init(fileURL: URL, center: LanguageServerCenter)`
  - `struct LSPSuggestion: CodeSuggestionEntry` — the adapter from `LSPCompletionItem` to what the editor's popup renders
  - `nonisolated static func symbolImage(forKind kind: Int?) -> (Image, Color)`

> **`CursorPosition` is 1-indexed, LSP is 0-indexed.** The conversion happens once, in `lspPosition(from:)`, and nowhere else. An off-by-one here does not crash — it silently completes against the wrong column, which is far harder to notice than a crash.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LSPCompletionDelegateTests.swift
import CodeEditSourceEditor
import Foundation
import Testing
import TillerLSP
@testable import Tiller

@Test func cursorPositionConvertsToAZeroIndexedLSPPosition() {
    // CursorPosition line 1, column 1 is the very start of the document.
    let position = LSPCompletionDelegate.lspPosition(
        from: CursorPosition(line: 1, column: 1))
    #expect(position == LSPPosition(line: 0, character: 0))
}

@Test func cursorPositionOnTheThirdLineFifthColumnConverts() {
    let position = LSPCompletionDelegate.lspPosition(
        from: CursorPosition(line: 3, column: 5))
    #expect(position == LSPPosition(line: 2, character: 4))
}

@Test func aCursorPositionWithUnfilledLineAndColumnYieldsNil() {
    // CursorPosition(range:) leaves line and column at -1 until the controller
    // fills them; asking a server about (-1, -1) would be nonsense.
    #expect(LSPCompletionDelegate.lspPosition(from: CursorPosition(range: NSRange(location: 0, length: 0))) == nil)
}

@Test func suggestionUsesTheLabelAndCarriesDetailAndDocumentation() {
    let item = LSPCompletionItem(
        label: "increment", detail: "fn(&mut self)", documentation: "Adds one.",
        insertText: nil, kind: 2, sortText: nil, deprecated: false)
    let suggestion = LSPSuggestion(item: item)
    #expect(suggestion.label == "increment")
    #expect(suggestion.detail == "fn(&mut self)")
    #expect(suggestion.documentation == "Adds one.")
    #expect(suggestion.deprecated == false)
}

@Test func suggestionMarksDeprecatedItems() {
    let item = LSPCompletionItem(
        label: "old", detail: nil, documentation: nil, insertText: nil,
        kind: nil, sortText: nil, deprecated: true)
    #expect(LSPSuggestion(item: item).deprecated)
}

@Test func insertionTextPrefersInsertTextOverLabel() {
    let item = LSPCompletionItem(
        label: "increment()", detail: nil, documentation: nil, insertText: "increment",
        kind: nil, sortText: nil, deprecated: false)
    #expect(LSPSuggestion(item: item).insertionText == "increment")
}

@Test func insertionTextFallsBackToTheLabel() {
    let item = LSPCompletionItem(
        label: "increment", detail: nil, documentation: nil, insertText: nil,
        kind: nil, sortText: nil, deprecated: false)
    #expect(LSPSuggestion(item: item).insertionText == "increment")
}

@Test func itemsAreOrderedBySortTextWhenPresentAndByLabelOtherwise() {
    let items = [
        LSPCompletionItem(label: "zebra", detail: nil, documentation: nil, insertText: nil,
                          kind: nil, sortText: "0001", deprecated: false),
        LSPCompletionItem(label: "apple", detail: nil, documentation: nil, insertText: nil,
                          kind: nil, sortText: "0002", deprecated: false)
    ]
    // The server ranks by relevance in sortText; re-sorting alphabetically
    // would throw away the only ranking that knows about the code.
    #expect(LSPCompletionDelegate.ordered(items).map(\.label) == ["zebra", "apple"])

    let unranked = [
        LSPCompletionItem(label: "zebra", detail: nil, documentation: nil, insertText: nil,
                          kind: nil, sortText: nil, deprecated: false),
        LSPCompletionItem(label: "apple", detail: nil, documentation: nil, insertText: nil,
                          kind: nil, sortText: nil, deprecated: false)
    ]
    #expect(LSPCompletionDelegate.ordered(unranked).map(\.label) == ["apple", "zebra"])
}

@Test func triggerCharactersIncludeDotAndColonColon() {
    let delegate = LSPCompletionDelegate(
        fileURL: URL(fileURLWithPath: "/tmp/a.rs"), center: LanguageServerCenter())
    let characters = delegate.completionTriggerCharacters()
    #expect(characters.contains("."))
    #expect(characters.contains(":"))
}

@Test func everyCompletionKindMapsToASymbolWithoutCrashing() {
    for kind in 1...25 {
        _ = LSPCompletionDelegate.symbolImage(forKind: kind)
    }
    _ = LSPCompletionDelegate.symbolImage(forKind: nil)
    _ = LSPCompletionDelegate.symbolImage(forKind: 9999)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPCompletionDelegateTests 2>&1 | tail -20`
Expected: FAIL — `LSPCompletionDelegate` not defined.

- [ ] **Step 3: Write the delegate**

```swift
// App/CodeEditor/LSPCompletionDelegate.swift
import CodeEditSourceEditor
import Foundation
import SwiftUI
import TillerLSP

/// One completion item as the editor's suggestion window wants it.
struct LSPSuggestion: CodeSuggestionEntry {
    let item: LSPCompletionItem

    var label: String { item.label }
    var detail: String? { item.detail }
    var documentation: String? { item.documentation }
    var pathComponents: [String]? { nil }
    var targetPosition: CursorPosition? { nil }
    var sourcePreview: String? { nil }
    var deprecated: Bool { item.deprecated }

    /// What actually gets typed. Servers use `label` for display ("increment()")
    /// and `insertText` for the text ("increment"); inserting the label would
    /// duplicate parentheses the editor already balances.
    var insertionText: String { item.insertText ?? item.label }

    var image: Image { LSPCompletionDelegate.symbolImage(forKind: item.kind).0 }
    var imageColor: Color { LSPCompletionDelegate.symbolImage(forKind: item.kind).1 }
}

/// Serves the editor's completion popup from a language server.
@MainActor
final class LSPCompletionDelegate: CodeSuggestionDelegate {
    private let fileURL: URL
    private let center: LanguageServerCenter
    private var lastItems: [LSPCompletionItem] = []

    init(fileURL: URL, center: LanguageServerCenter) {
        self.fileURL = fileURL
        self.center = center
    }

    func completionTriggerCharacters() -> Set<String> {
        // The union of what the common servers declare. Asking a server that
        // does not care about ":" simply returns nothing, which costs one
        // round-trip and no correctness.
        [".", ":", "-", ">", "@", "#", "$", "/", "\\", "<", "\"", "'"]
    }

    func completionSuggestionsRequested(
        textView: TextViewController,
        cursorPosition: CursorPosition
    ) async -> (windowPosition: CursorPosition, items: [CodeSuggestionEntry])? {
        guard let position = Self.lspPosition(from: cursorPosition) else { return nil }
        let items = Self.ordered(await center.host.completion(fileURL: fileURL, position: position))
        lastItems = items
        guard !items.isEmpty else { return nil }
        return (windowPosition: cursorPosition, items: items.map(LSPSuggestion.init))
    }

    /// Must stay synchronous and cheap: it runs on every cursor move while the
    /// window is open. Filtering the last response is the whole job — a new
    /// request here would make the window flicker on every arrow key.
    func completionOnCursorMove(
        textView: TextViewController,
        cursorPosition: CursorPosition
    ) -> [CodeSuggestionEntry]? {
        lastItems.isEmpty ? nil : lastItems.map(LSPSuggestion.init)
    }

    func completionWindowApplyCompletion(
        item: CodeSuggestionEntry,
        textView: TextViewController,
        cursorPosition: CursorPosition?
    ) {
        guard let suggestion = item as? LSPSuggestion,
              let position = cursorPosition else { return }
        textView.textView.insertText(suggestion.insertionText, replacementRange: position.range)
    }

    func completionWindowDidClose() { lastItems = [] }

    // MARK: - Conversions

    /// `CursorPosition` is 1-indexed in both line and column; LSP is 0-indexed
    /// in both. This is the only place the two meet.
    ///
    /// Returns `nil` for an unfilled position — `CursorPosition(range:)` leaves
    /// line and column at `-1` until the text controller fills them in.
    nonisolated static func lspPosition(from cursorPosition: CursorPosition) -> LSPPosition? {
        let start = cursorPosition.start
        guard start.line > 0, start.column > 0 else { return nil }
        return LSPPosition(line: start.line - 1, character: start.column - 1)
    }

    /// Servers rank by relevance in `sortText`, which knows about scope and
    /// type in a way an alphabetical sort cannot. Only fall back to the label
    /// when no item is ranked.
    nonisolated static func ordered(_ items: [LSPCompletionItem]) -> [LSPCompletionItem] {
        if items.contains(where: { $0.sortText != nil }) {
            return items.sorted { ($0.sortText ?? $0.label) < ($1.sortText ?? $1.label) }
        }
        return items.sorted { $0.label < $1.label }
    }

    /// LSP `CompletionItemKind` values, 1...25 per the spec.
    nonisolated static func symbolImage(forKind kind: Int?) -> (Image, Color) {
        switch kind {
        case 2, 3, 4:  (Image(systemName: "function"), .purple)          // method, function, constructor
        case 5:        (Image(systemName: "f.square"), .teal)            // field
        case 6:        (Image(systemName: "v.square"), .blue)            // variable
        case 7, 22:    (Image(systemName: "c.square"), .orange)          // class, struct
        case 8:        (Image(systemName: "i.square"), .indigo)          // interface
        case 9:        (Image(systemName: "shippingbox"), .brown)        // module
        case 10:       (Image(systemName: "p.square"), .cyan)            // property
        case 13, 20:   (Image(systemName: "e.square"), .pink)            // enum, enum member
        case 14:       (Image(systemName: "k.square"), .red)             // keyword
        case 15:       (Image(systemName: "chevron.left.forwardslash.chevron.right"), .green)
        case 21:       (Image(systemName: "number.square"), .mint)       // constant
        default:       (Image(systemName: "dot.square.fill"), .secondary)
        }
    }
}
```

- [ ] **Step 4: Wire it into the editor**

In `App/CodeEditor/CodeEditorTabView.swift`, add the worktree root and center, hold the delegate in state, and pass it to `SourceEditor`:

```swift
struct CodeEditorTabView: View {
    @Bindable var document: CodeDocument
    var worktreeRoot: URL
    var center: LanguageServerCenter
    @Environment(\.colorScheme) private var colorScheme
    @State private var editorState = SourceEditorState()
    @State private var completionDelegate: LSPCompletionDelegate?
    private static let highlightByteLimit = 2_000_000
```

Inside `body`, replace the bare `SourceEditor(...)` call's argument list with one that carries the delegate, and drive the document lifecycle:

```swift
            SourceEditor(
                $document.text,
                language: detectedLanguage,
                configuration: SourceEditorConfiguration(
                    appearance: .init(
                        theme: .tiller(isDark: colorScheme == .dark),
                        font: .monospacedSystemFont(ofSize: 12, weight: .regular),
                        wrapLines: false),
                    behavior: .init(isEditable: true, isSelectable: true,
                                    indentOption: .spaces(count: 4)),
                    layout: .init(contentInsets: NSEdgeInsets())),
                state: $editorState,
                completionDelegate: completionDelegate)
            .clipped()
            .pointerStyle(.horizontalText)
            .task(id: document.fileURL) {
                completionDelegate = LSPCompletionDelegate(
                    fileURL: document.fileURL, center: center)
                await center.open(fileURL: document.fileURL,
                                  worktreeRoot: worktreeRoot, text: document.text)
            }
            .onChange(of: document.text) { _, text in
                Task { await center.change(fileURL: document.fileURL, text: text) }
            }
```

- [ ] **Step 5: Pass the new arguments at the call site**

In `App/ContentView.swift` around line 325:

```swift
                            case .code:
                                if let document = model.codeDocument(for: tab) {
                                    CodeEditorTabView(
                                        document: document,
                                        worktreeRoot: URL(fileURLWithPath: worktree.path),
                                        center: model.languageServers)
                                } else {
```

- [ ] **Step 6: Run to verify the tests pass**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPCompletionDelegateTests 2>&1 | tail -20`
Expected: 10 tests pass.

- [ ] **Step 7: Verify by hand — this is the milestone**

Build and run. Open a Rust file inside a Cargo project, wait for indexing, type `.` after a value. A popup with real symbols means the whole client works; everything after this task is presentation.

If the popup is empty, check in this order: `command -v rust-analyzer` in a login shell, then the `Cargo.toml` above the file, then whether ~30 s of indexing have passed.

- [ ] **Step 8: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: serve editor completion from language servers"
```

---

## Phase 5 — Navigation

### Task 12: Go-to-definition, with read-only landings

**Files:**
- Create: `App/CodeEditor/LSPJumpToDefinitionDelegate.swift`
- Create: `AppTests/LSPJumpToDefinitionDelegateTests.swift`
- Modify: `App/CodeEditor/CodeEditorTabView.swift`, `App/AppModel.swift`

**Interfaces:**
- Consumes: `LanguageServerCenter` (Task 10), `LSPLocation`, `LineIndex`.
- Produces:
  - `@MainActor final class LSPJumpToDefinitionDelegate: JumpToDefinitionDelegate`
  - `init(fileURL: URL, worktreeRoot: URL, center: LanguageServerCenter, open: @escaping (URL, CursorPosition, Bool) -> Void)` — the last parameter of `open` is `readOnly`
  - `nonisolated static func openableFileURL(from uri: String) -> URL?`
  - `nonisolated static func cursorPosition(from location: LSPLocation) -> CursorPosition`
- Also produces on `AppModel`: `func openCodeFile(_ url: URL, at position: CursorPosition?, readOnly: Bool, in worktree: Worktree)`
- Also produces on `CodeDocument`: `var isReadOnly: Bool` (set at init, honoured by `save()`)

> **Not every LSP target is a file.** sourcekit-lsp answers with generated-interface URIs and rust-analyzer with `rust-analyzer://` for macro expansions. Those schemes are the server's own; without implementing them the target cannot be opened, and handing the string to `CodeDocument` would make it try to read a path that does not exist and fail confusingly.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LSPJumpToDefinitionDelegateTests.swift
import CodeEditSourceEditor
import Foundation
import Testing
import TillerLSP
@testable import Tiller

@Test func aFileURIBecomesAURL() {
    let url = LSPJumpToDefinitionDelegate.openableFileURL(from: "file:///tmp/a.rs")
    #expect(url?.path == "/tmp/a.rs")
}

@Test func aPercentEncodedFileURIDecodes() {
    let url = LSPJumpToDefinitionDelegate.openableFileURL(from: "file:///tmp/my%20crate/a.rs")
    #expect(url?.path == "/tmp/my crate/a.rs")
}

@Test func aNonFileSchemeIsRejectedRatherThanGuessedAt() {
    #expect(LSPJumpToDefinitionDelegate.openableFileURL(from: "rust-analyzer://expand/1") == nil)
    #expect(LSPJumpToDefinitionDelegate.openableFileURL(from: "sourcekit-lsp://generated/X.swift") == nil)
    #expect(LSPJumpToDefinitionDelegate.openableFileURL(from: "not a uri at all") == nil)
}

@Test func locationConvertsToAOneIndexedCursorPosition() {
    let location = LSPLocation(
        uri: "file:///tmp/a.rs",
        range: LSPRange(start: LSPPosition(line: 4, character: 7),
                        end: LSPPosition(line: 4, character: 12)))
    let position = LSPJumpToDefinitionDelegate.cursorPosition(from: location)
    #expect(position.start.line == 5)
    #expect(position.start.column == 8)
}

@MainActor
@Test func openingALocationInsideTheWorktreeIsWritable() async {
    var opened: (URL, Bool)?
    let delegate = LSPJumpToDefinitionDelegate(
        fileURL: URL(fileURLWithPath: "/tmp/project/src/main.rs"),
        worktreeRoot: URL(fileURLWithPath: "/tmp/project"),
        center: LanguageServerCenter(),
        open: { url, _, readOnly in opened = (url, readOnly) })

    delegate.openLink(link: JumpToDefinitionLink(
        url: URL(fileURLWithPath: "/tmp/project/src/lib.rs"),
        targetRange: CursorPosition(line: 3, column: 1),
        typeName: "Counter", sourcePreview: "", documentation: nil))

    #expect(opened?.0.lastPathComponent == "lib.rs")
    #expect(opened?.1 == false)
}

@MainActor
@Test func openingALocationOutsideTheWorktreeIsReadOnly() async {
    var opened: (URL, Bool)?
    let delegate = LSPJumpToDefinitionDelegate(
        fileURL: URL(fileURLWithPath: "/tmp/project/src/main.rs"),
        worktreeRoot: URL(fileURLWithPath: "/tmp/project"),
        center: LanguageServerCenter(),
        open: { url, _, readOnly in opened = (url, readOnly) })

    delegate.openLink(link: JumpToDefinitionLink(
        url: URL(fileURLWithPath: "/Users/me/.cargo/registry/src/serde/lib.rs"),
        targetRange: CursorPosition(line: 1, column: 1),
        typeName: "Serialize", sourcePreview: "", documentation: nil))

    #expect(opened?.1 == true)
}

@MainActor
@Test func openingALocationInsideABuildDirectoryIsReadOnly() async {
    var opened: (URL, Bool)?
    let delegate = LSPJumpToDefinitionDelegate(
        fileURL: URL(fileURLWithPath: "/tmp/project/src/main.rs"),
        worktreeRoot: URL(fileURLWithPath: "/tmp/project"),
        center: LanguageServerCenter(),
        open: { url, _, readOnly in opened = (url, readOnly) })

    delegate.openLink(link: JumpToDefinitionLink(
        url: URL(fileURLWithPath: "/tmp/project/target/debug/build/generated.rs"),
        targetRange: CursorPosition(line: 1, column: 1),
        typeName: "Generated", sourcePreview: "", documentation: nil))

    #expect(opened?.1 == true)
}

@Test func aReadOnlyDocumentRefusesToSave() throws {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("readonly-\(UUID().uuidString).swift")
    try "let a = 1".write(to: url, atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: url) }

    let document = try CodeDocument(fileURL: url, isReadOnly: true)
    document.text = "let a = 2"
    #expect(throws: CodeDocument.ReadOnlyError.self) { try document.save() }
    #expect(try String(contentsOf: url, encoding: .utf8) == "let a = 1")
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPJumpToDefinitionDelegateTests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Add read-only to `CodeDocument`**

```swift
// Packages/TillerCode/Sources/TillerCode/CodeDocument.swift
// Add the stored property and the error, and guard save().
    public struct ReadOnlyError: Error {
        public let fileURL: URL
    }

    /// Set when the document was opened by go-to-definition outside the
    /// worktree, or inside a build/dependency directory. The editor also
    /// renders non-editable, but the guard lives here so no second call site
    /// can bypass it.
    public let isReadOnly: Bool

    public init(fileURL: URL, isReadOnly: Bool = false) throws {
        self.isReadOnly = isReadOnly
        // ...existing body unchanged...
    }

    public func save() throws {
        guard !isReadOnly else { throw ReadOnlyError(fileURL: fileURL) }
        // ...existing body unchanged...
    }
```

Add a matching test in `Packages/TillerCode/Tests/TillerCodeTests/CodeDocumentTests.swift`:

```swift
@Test func readOnlyDocumentsDoNotWriteToDisk() throws {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("ro-\(UUID().uuidString).txt")
    try "original".write(to: url, atomically: true, encoding: .utf8)
    defer { try? FileManager.default.removeItem(at: url) }

    let document = try CodeDocument(fileURL: url, isReadOnly: true)
    document.text = "changed"
    #expect(throws: CodeDocument.ReadOnlyError.self) { try document.save() }
    #expect(try String(contentsOf: url, encoding: .utf8) == "original")
}
```

- [ ] **Step 4: Write the delegate**

```swift
// App/CodeEditor/LSPJumpToDefinitionDelegate.swift
import CodeEditSourceEditor
import Foundation
import SwiftUI
import TillerLSP

/// Answers ⌘-click and "Jump to Definition" from a language server.
@MainActor
final class LSPJumpToDefinitionDelegate: JumpToDefinitionDelegate {
    private let fileURL: URL
    private let worktreeRoot: URL
    private let center: LanguageServerCenter
    private let open: (URL, CursorPosition, Bool) -> Void

    init(fileURL: URL, worktreeRoot: URL, center: LanguageServerCenter,
         open: @escaping (URL, CursorPosition, Bool) -> Void) {
        self.fileURL = fileURL
        self.worktreeRoot = worktreeRoot
        self.center = center
        self.open = open
    }

    func queryLinks(forRange range: NSRange, textView: TextViewController) async -> [JumpToDefinitionLink]? {
        let index = LineIndex(text: textView.text)
        guard let position = index.position(atUTF16Offset: range.location) else { return nil }
        let locations = await center.host.definition(fileURL: fileURL, position: position)
        let links = locations.compactMap { location -> JumpToDefinitionLink? in
            guard let url = Self.openableFileURL(from: location.uri) else { return nil }
            return JumpToDefinitionLink(
                url: url,
                targetRange: Self.cursorPosition(from: location),
                typeName: url.lastPathComponent,
                sourcePreview: Self.preview(of: url, at: location) ?? "",
                documentation: nil)
        }
        return links.isEmpty ? nil : links
    }

    func openLink(link: JumpToDefinitionLink) {
        guard let url = link.url else { return }
        let readOnly = LanguageServerCenter.isReadOnlyLocation(url, worktreeRoot: worktreeRoot)
        open(url, link.targetRange, readOnly)
    }

    // MARK: - Conversions

    /// Only `file:` URIs can be opened. sourcekit-lsp answers with generated
    /// interface URIs and rust-analyzer with `rust-analyzer://` for expanded
    /// macros; those are server-private schemes, and passing one to
    /// `CodeDocument` would make it read a path that does not exist.
    nonisolated static func openableFileURL(from uri: String) -> URL? {
        guard let url = URL(string: uri), url.isFileURL, !url.path.isEmpty else { return nil }
        return url.standardizedFileURL
    }

    /// LSP is 0-indexed, `CursorPosition` is 1-indexed.
    nonisolated static func cursorPosition(from location: LSPLocation) -> CursorPosition {
        CursorPosition(line: location.range.start.line + 1,
                       column: location.range.start.character + 1)
    }

    /// One line of context for the disambiguation list the editor shows when a
    /// symbol has several definitions.
    private nonisolated static func preview(of url: URL, at location: LSPLocation) -> String? {
        guard let contents = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        let lines = contents.components(separatedBy: .newlines)
        guard location.range.start.line < lines.count else { return nil }
        return lines[location.range.start.line].trimmingCharacters(in: .whitespaces)
    }
}
```

- [ ] **Step 5: Add the opener to `AppModel`**

```swift
    /// Opens a code file in a tab of `worktree`, optionally read-only and
    /// optionally scrolled to a position. Reuses an existing tab for the same
    /// file rather than stacking duplicates.
    func openCodeFile(_ url: URL, at position: CursorPosition?, readOnly: Bool, in worktree: Worktree) {
        if let existing = worktree.tabs.first(where: {
            $0.codeFileURL?.standardizedFileURL == url.standardizedFileURL
        }) {
            focusTab(tabId: existing.id, in: worktree)
            return
        }
        guard let document = try? CodeDocument(fileURL: url, isReadOnly: readOnly) else { return }
        let tab = makeCodeTab(fileURL: url, in: worktree)
        codeDocuments[tab.id] = document
        focusTab(tabId: tab.id, in: worktree)
    }
```

`makeCodeTab` is the existing helper that already backs "open file from the Files panel"; reuse it rather than constructing a tab inline.

- [ ] **Step 6: Wire the delegate into the view**

In `CodeEditorTabView`, hold it alongside the completion delegate and pass it:

```swift
    @State private var jumpDelegate: LSPJumpToDefinitionDelegate?
```

```swift
                state: $editorState,
                completionDelegate: completionDelegate,
                jumpToDefinitionDelegate: jumpDelegate)
```

and build it in the same `.task(id:)`:

```swift
                jumpDelegate = LSPJumpToDefinitionDelegate(
                    fileURL: document.fileURL, worktreeRoot: worktreeRoot, center: center,
                    open: onOpenDefinition)
```

where `onOpenDefinition: (URL, CursorPosition, Bool) -> Void` is a new parameter of `CodeEditorTabView`, supplied in `ContentView` as:

```swift
                                        onOpenDefinition: { url, position, readOnly in
                                            model.openCodeFile(url, at: position,
                                                               readOnly: readOnly, in: worktree)
                                        })
```

Finally, make the editor itself non-editable for a read-only document:

```swift
                    behavior: .init(isEditable: !document.isReadOnly, isSelectable: true,
                                    indentOption: .spaces(count: 4)),
```

- [ ] **Step 7: Run the tests**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPJumpToDefinitionDelegateTests 2>&1 | tail -20`
Expected: 8 tests pass.

Run: `cd Packages/TillerCode && swift test`
Expected: the new `readOnlyDocumentsDoNotWriteToDisk` passes with the existing suite.

- [ ] **Step 8: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: add go-to-definition with read-only external landings"
```

---

## Phase 6 — Diagnostics

### Task 13: Underline diagnostics with `EmphasisManager`

**Files:**
- Create: `App/CodeEditor/LSPDiagnosticsPainter.swift`
- Create: `AppTests/LSPDiagnosticsPainterTests.swift`
- Modify: `App/CodeEditor/CodeEditorTabView.swift`

**Interfaces:**
- Consumes: `LSPDiagnostic`, `LineIndex` (Tasks 5-6), `LanguageServerCenter` (Task 10).
- Produces:
  - `enum LSPDiagnosticsPainter`
  - `static let emphasisID = "lsp.diagnostics"`
  - `static func emphases(for diagnostics: [LSPDiagnostic], in text: String) -> [Emphasis]`
  - `static func color(for severity: LSPDiagnosticSeverity) -> NSColor`

> **Why `EmphasisManager` and not a custom overlay.** `CodeEditTextView` already exposes `EmphasisStyle.underline(color:)` and `replaceEmphases(_:for:)`. The `id` namespace keeps Tiller's diagnostics from colliding with find-highlighting and bracket matching, and `replace` is exactly `publishDiagnostics` semantics: the server always sends the complete list for a file, so the previous set must go.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LSPDiagnosticsPainterTests.swift
import AppKit
import CodeEditTextView
import Foundation
import Testing
import TillerLSP
@testable import Tiller

private func diagnostic(line: Int, from: Int, to: Int,
                        severity: LSPDiagnosticSeverity = .error,
                        message: String = "boom") -> LSPDiagnostic {
    LSPDiagnostic(
        range: LSPRange(start: LSPPosition(line: line, character: from),
                        end: LSPPosition(line: line, character: to)),
        severity: severity, message: message, source: nil)
}

@Test func aDiagnosticBecomesAnUnderlineOverItsRange() {
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 0, from: 4, to: 7)], in: "let foo = 1\n")
    #expect(emphases.count == 1)
    #expect(emphases[0].range == NSRange(location: 4, length: 3))
    guard case .underline = emphases[0].style else {
        Issue.record("expected an underline style"); return
    }
}

@Test func severityDecidesTheColour() {
    #expect(LSPDiagnosticsPainter.color(for: .error) != LSPDiagnosticsPainter.color(for: .warning))
    #expect(LSPDiagnosticsPainter.color(for: .warning) != LSPDiagnosticsPainter.color(for: .hint))
}

@Test func aDiagnosticOnTheSecondLineOffsetsPastTheNewline() {
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 1, from: 0, to: 3)], in: "abc\ndef\n")
    #expect(emphases[0].range == NSRange(location: 4, length: 3))
}

@Test func anEmptyRangeIsWidenedToOneCharacterSoItStaysVisible() {
    // Servers report zero-width ranges for "something is missing here".
    // A zero-length underline draws nothing at all.
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 0, from: 3, to: 3)], in: "let x\n")
    #expect(emphases[0].range.length == 1)
}

@Test func anEmptyRangeAtTheVeryEndOfTheDocumentStillProducesAVisibleRange() {
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 0, from: 3, to: 3)], in: "abc")
    #expect(emphases.count == 1)
    #expect(emphases[0].range.upperBound <= 3)
    #expect(emphases[0].range.length == 1)
}

@Test func aDiagnosticPointingBeyondTheDocumentIsDroppedRatherThanClamped() {
    // Stale diagnostics arrive routinely: the server is a debounce behind the
    // buffer. Drawing them at a guessed location is worse than not drawing.
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 40, from: 0, to: 2)], in: "one line only")
    #expect(emphases.isEmpty)
}

@Test func diagnosticsAreNotFlashingAndDoNotMoveTheSelection() {
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 0, from: 0, to: 2)], in: "abc")
    #expect(emphases[0].flash == false)
    #expect(emphases[0].selectInDocument == false)
}

@Test func severalDiagnosticsProduceSeveralEmphases() {
    let emphases = LSPDiagnosticsPainter.emphases(
        for: [diagnostic(line: 0, from: 0, to: 1), diagnostic(line: 1, from: 0, to: 1)],
        in: "ab\ncd")
    #expect(emphases.count == 2)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPDiagnosticsPainterTests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Write the painter**

```swift
// App/CodeEditor/LSPDiagnosticsPainter.swift
import AppKit
import CodeEditTextView
import Foundation
import TillerLSP

/// Turns diagnostics into text-view emphases.
///
/// `EmphasisManager` namespaces emphases by id, so Tiller's diagnostics never
/// collide with find highlighting or bracket matching, and `replaceEmphases`
/// matches `publishDiagnostics` exactly: the server always sends the complete
/// current list for a file, so whatever was drawn before must go.
enum LSPDiagnosticsPainter {
    static let emphasisID = "lsp.diagnostics"

    static func emphases(for diagnostics: [LSPDiagnostic], in text: String) -> [Emphasis] {
        let index = LineIndex(text: text)
        let length = (text as NSString).length
        return diagnostics.compactMap { diagnostic in
            guard var range = index.nsRange(of: diagnostic.range) else { return nil }
            // A zero-width range means "something is missing here". Underlining
            // nothing draws nothing, so borrow one character — the one before,
            // if there is no character after.
            if range.length == 0 {
                if range.location < length {
                    range = NSRange(location: range.location, length: 1)
                } else if range.location > 0 {
                    range = NSRange(location: range.location - 1, length: 1)
                } else {
                    return nil
                }
            }
            guard range.upperBound <= length else { return nil }
            return Emphasis(
                range: range,
                style: .underline(color: color(for: diagnostic.severity)),
                flash: false, inactive: false, selectInDocument: false)
        }
    }

    static func color(for severity: LSPDiagnosticSeverity) -> NSColor {
        switch severity {
        case .error: .systemRed
        case .warning: .systemOrange
        case .information: .systemBlue
        case .hint: .systemGray
        }
    }
}
```

- [ ] **Step 4: Repaint whenever diagnostics or text change**

In `CodeEditorTabView`, add the repaint. `editorState` gives access to the controller, whose `textView` owns the emphasis manager:

```swift
            .onChange(of: center.diagnostics(for: document.fileURL).count) { _, _ in repaint() }
            .onChange(of: document.text) { _, _ in repaint() }
```

```swift
    /// Diagnostics are always redrawn as a complete set: `publishDiagnostics`
    /// is not incremental, and neither is this.
    private func repaint() {
        guard let textView = editorState.textViewController?.textView else { return }
        let emphases = LSPDiagnosticsPainter.emphases(
            for: center.diagnostics(for: document.fileURL), in: document.text)
        textView.emphasisManager?.replaceEmphases(emphases, for: LSPDiagnosticsPainter.emphasisID)
    }
```

If `SourceEditorState` does not vend the controller under that name, read the property list in `SourceEditorState/` and use the accessor it does expose. Do not reach into the view hierarchy to find the text view.

- [ ] **Step 5: Run the tests**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPDiagnosticsPainterTests 2>&1 | tail -20`
Expected: 8 tests pass.

- [ ] **Step 6: Verify by hand**

Open a Rust file with a deliberate error (`undefined_symbol()`). A red underline appears under the offending token within a second or two of the server settling. Fix the error and the underline goes.

- [ ] **Step 7: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: underline diagnostics in the code editor"
```

---

### Task 14: The Problems panel

**Files:**
- Create: `App/RightPanel/ProblemsView.swift`
- Create: `AppTests/RightPanelModeTests.swift`
- Modify: `App/RightPanel/RightPanelMode.swift`, and the right-panel view that switches on it

**Interfaces:**
- Consumes: `LanguageServerCenter.diagnosticsByPath(under:)` (Task 10), `AppModel.openCodeFile` (Task 12).
- Produces:
  - `RightPanelMode.problems` with `title == "Problems"`, `systemImage == "exclamationmark.triangle"`, `requiresGit == false`
  - `struct ProblemsView: View`

> **The panel's honest scope.** A server publishes diagnostics only for files it knows about. rust-analyzer runs `cargo check` and reports the whole workspace; sourcekit-lsp reports the files you have opened. The same panel is therefore full on Rust and sparse on Swift, and that is the server's behaviour, not a bug to paper over. The title is "Problems", never "All Problems".

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/RightPanelModeTests.swift
import Foundation
import Testing
@testable import Tiller

@Test func problemsIsAvailableOnNonGitProjects() {
    // Unlike diff and status, diagnostics have nothing to do with git.
    #expect(RightPanelMode.problems.requiresGit == false)
    #expect(RightPanelMode.effective(rawValue: "problems", isGitRepository: false) == .problems)
}

@Test func problemsSurvivesAPersistenceRoundTrip() {
    #expect(RightPanelMode(rawValue: RightPanelMode.problems.rawValue) == .problems)
}

@Test func problemsHasATitleAndASymbol() {
    #expect(RightPanelMode.problems.title == "Problems")
    #expect(!RightPanelMode.problems.systemImage.isEmpty)
}

@Test func gitOnlyModesStillFallBackToFilesOnNonGitProjects() {
    #expect(RightPanelMode.effective(rawValue: "diff", isGitRepository: false) == .files)
    #expect(RightPanelMode.effective(rawValue: "status", isGitRepository: false) == .files)
}

@Test func everyModeIsEnumerated() {
    #expect(Set(RightPanelMode.allCases.map(\.rawValue))
        == ["files", "diff", "status", "problems"])
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/RightPanelModeTests 2>&1 | tail -20`
Expected: FAIL — no `problems` case.

- [ ] **Step 3: Add the route**

```swift
// App/RightPanel/RightPanelMode.swift
enum RightPanelMode: String, CaseIterable, Identifiable {
    case files
    case diff
    case status
    case problems

    var id: String { rawValue }

    var title: String {
        switch self {
        case .files: "Files"
        case .diff: "Diff"
        case .status: "Status"
        case .problems: "Problems"
        }
    }

    var systemImage: String {
        switch self {
        case .files: "folder"
        case .diff: "plus.forwardslash.minus"
        case .status: "arrow.triangle.branch"
        case .problems: "exclamationmark.triangle"
        }
    }

    /// Diagnostics come from a language server, not from git, so this route
    /// stays available on non-git projects.
    var requiresGit: Bool { self != .files && self != .problems }

    static func effective(rawValue: String, isGitRepository: Bool) -> RightPanelMode {
        let saved = RightPanelMode(rawValue: rawValue) ?? .files
        return saved.requiresGit && !isGitRepository ? .files : saved
    }
}
```

- [ ] **Step 4: Write the view**

```swift
// App/RightPanel/ProblemsView.swift
import SwiftUI
import TillerLSP

/// Every problem a language server has reported under this worktree.
///
/// Deliberately not limited to open tabs (decision 12): where a server
/// publishes workspace-wide — rust-analyzer does, via `cargo check` — that
/// breadth is free and worth having.
struct ProblemsView: View {
    var model: AppModel
    var worktree: Worktree

    private var groups: [(url: URL, diagnostics: [LSPDiagnostic])] {
        model.languageServers.diagnosticsByPath(
            under: URL(fileURLWithPath: worktree.path))
    }

    var body: some View {
        Group {
            if groups.isEmpty { empty } else { list }
        }
        .background(AppTheme.background)
    }

    private var empty: some View {
        VStack(spacing: 6) {
            Image(systemName: "checkmark.circle")
                .font(.system(size: 22))
                .foregroundStyle(AppTheme.subtitle)
            Text("No problems reported")
                .font(.system(size: 12))
                .foregroundStyle(AppTheme.subtitle)
            Text("Only languages with a running server report problems.")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.subtitle)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, 16)
    }

    private var list: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ForEach(groups, id: \.url) { group in
                    fileHeader(group.url, count: group.diagnostics.count)
                    ForEach(Array(group.diagnostics.enumerated()), id: \.offset) { _, diagnostic in
                        row(diagnostic, in: group.url)
                    }
                }
            }
            .padding(.vertical, 6)
        }
    }

    private func fileHeader(_ url: URL, count: Int) -> some View {
        HStack(spacing: 6) {
            Text(relativePath(of: url))
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(AppTheme.title)
                .lineLimit(1)
                .truncationMode(.head)
            Text("\(count)")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.subtitle)
            Spacer()
        }
        .padding(.horizontal, 10)
        .padding(.top, 8)
        .padding(.bottom, 2)
    }

    private func row(_ diagnostic: LSPDiagnostic, in url: URL) -> some View {
        Button {
            model.openCodeFile(
                url,
                at: CursorPosition(line: diagnostic.range.start.line + 1,
                                   column: diagnostic.range.start.character + 1),
                readOnly: LanguageServerCenter.isReadOnlyLocation(
                    url, worktreeRoot: URL(fileURLWithPath: worktree.path)),
                in: worktree)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Image(systemName: symbol(for: diagnostic.severity))
                    .font(.system(size: 10))
                    .foregroundStyle(tint(for: diagnostic.severity))
                Text("\(diagnostic.range.start.line + 1)")
                    .font(.system(size: 10).monospacedDigit())
                    .foregroundStyle(AppTheme.subtitle)
                Text(diagnostic.message)
                    .font(.system(size: 11))
                    .foregroundStyle(AppTheme.title)
                    .lineLimit(2)
                    .multilineTextAlignment(.leading)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func relativePath(of url: URL) -> String {
        let root = URL(fileURLWithPath: worktree.path).standardizedFileURL.path
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(root + "/") else { return url.lastPathComponent }
        return String(path.dropFirst(root.count + 1))
    }

    private func symbol(for severity: LSPDiagnosticSeverity) -> String {
        switch severity {
        case .error: "xmark.circle.fill"
        case .warning: "exclamationmark.triangle.fill"
        case .information: "info.circle"
        case .hint: "lightbulb"
        }
    }

    private func tint(for severity: LSPDiagnosticSeverity) -> Color {
        switch severity {
        case .error: .red
        case .warning: .orange
        case .information: .blue
        case .hint: .secondary
        }
    }
}
```

- [ ] **Step 5: Route to it**

In the right-panel view that switches on `RightPanelMode`, add:

```swift
        case .problems: ProblemsView(model: model, worktree: worktree)
```

- [ ] **Step 6: Run the tests, the gate, and commit**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/RightPanelModeTests 2>&1 | tail -20`
Expected: 5 tests pass.

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: add a Problems panel to the worktree tools sidebar"
```

---

## Phase 7 — Making the state legible

### Task 15: Language Servers settings

**Files:**
- Create: `App/LanguageServersSettingsView.swift`
- Create: `AppTests/LanguageServersSettingsTests.swift`
- Modify: `App/AppRoute.swift`, `App/SettingsSurface.swift`

**Interfaces:**
- Consumes: `LanguageServerCatalog`, `ExecutableLocating` (Task 7).
- Produces:
  - `SettingsCategory.languageServers` with `title == "Language Servers"`, `symbol == "chevron.left.forwardslash.chevron.right"`
  - `@MainActor @Observable final class LanguageServersSettingsModel`
  - `init(catalog: [LanguageServerDefinition] = LanguageServerCatalog.all, locator: ExecutableLocating = LoginShellExecutableLocator())`
  - `func refresh() async`
  - `struct Row: Identifiable { let definition: LanguageServerDefinition; let resolvedPath: String? }`
  - `var rows: [Row]`, `var isRefreshing: Bool`
  - `struct LanguageServersSettingsView: View`

> **Why a login shell, again.** The lookup goes through `zsh -lc "command -v …"`, not `FileManager`. Tiller launched from Finder sees `/usr/bin:/bin:/usr/sbin:/sbin`, so a `PATH` walk would report "not installed" for every server the user put in `~/.cargo/bin` or `~/go/bin` — which is most of them.

> **The install command is shown, never run** (decisions 4 and 15). The user sees the exact command, copies it, and runs it in a terminal Tiller is already showing. Tiller downloads nothing and executes nothing, so none of the supply-chain questions of a built-in installer arise.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LanguageServersSettingsTests.swift
import Foundation
import Testing
import TillerLSP
@testable import Tiller

private struct StubLocator: ExecutableLocating {
    let installed: [String: String]
    func locate(_ executable: String) async -> String? { installed[executable] }
}

@MainActor
@Test func refreshMarksInstalledServersWithTheirResolvedPath() async {
    let model = LanguageServersSettingsModel(
        catalog: [
            .init(id: "rust-analyzer", displayName: "Rust", executable: "rust-analyzer",
                  languageID: "rust", fileExtensions: ["rs"], rootMarkers: ["Cargo.toml"],
                  installCommand: "rustup component add rust-analyzer"),
            .init(id: "gopls", displayName: "Go", executable: "gopls",
                  languageID: "go", fileExtensions: ["go"], rootMarkers: ["go.mod"],
                  installCommand: "go install golang.org/x/tools/gopls@latest")
        ],
        locator: StubLocator(installed: ["rust-analyzer": "/Users/me/.cargo/bin/rust-analyzer"]))

    await model.refresh()

    #expect(model.rows.count == 2)
    #expect(model.rows[0].resolvedPath == "/Users/me/.cargo/bin/rust-analyzer")
    #expect(model.rows[1].resolvedPath == nil)
}

@MainActor
@Test func rowsAreSortedInstalledFirstThenAlphabetically() async {
    let model = LanguageServersSettingsModel(
        catalog: [
            .init(id: "zls", displayName: "Zig", executable: "zls", languageID: "zig",
                  fileExtensions: ["zig"], rootMarkers: ["build.zig"], installCommand: "brew install zls"),
            .init(id: "gopls", displayName: "Go", executable: "gopls", languageID: "go",
                  fileExtensions: ["go"], rootMarkers: ["go.mod"], installCommand: "go install …"),
            .init(id: "rust-analyzer", displayName: "Rust", executable: "rust-analyzer",
                  languageID: "rust", fileExtensions: ["rs"], rootMarkers: ["Cargo.toml"],
                  installCommand: "rustup component add rust-analyzer")
        ],
        locator: StubLocator(installed: ["zls": "/opt/zls"]))

    await model.refresh()
    #expect(model.rows.map(\.definition.displayName) == ["Zig", "Go", "Rust"])
}

@MainActor
@Test func isRefreshingIsFalseOnceRefreshCompletes() async {
    let model = LanguageServersSettingsModel(catalog: [], locator: StubLocator(installed: [:]))
    await model.refresh()
    #expect(model.isRefreshing == false)
}

@Test func theSettingsCategoryExistsAndIsEnumerated() {
    #expect(SettingsCategory.allCases.contains(.languageServers))
    #expect(SettingsCategory.languageServers.title == "Language Servers")
    #expect(!SettingsCategory.languageServers.symbol.isEmpty)
}

@Test func everyCatalogRowCanBeRenderedWithoutAnEmptyInstallHint() {
    // The panel's whole point is telling the user what to do next.
    for definition in LanguageServerCatalog.all {
        #expect(!definition.installCommand.isEmpty)
        #expect(!definition.displayName.isEmpty)
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServersSettingsTests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Add the settings category**

```swift
// App/AppRoute.swift
enum SettingsCategory: String, CaseIterable, Identifiable {
    case aiProviders
    case agents
    case languageServers
    case general
    case permissions
    case appearance

    var id: String { rawValue }

    var title: String {
        switch self {
        case .aiProviders: "AI Providers"
        case .agents: "Agents"
        case .languageServers: "Language Servers"
        case .general: "General"
        case .permissions: "Permissions"
        case .appearance: "Appearance"
        }
    }

    var symbol: String {
        switch self {
        case .aiProviders: "sparkles"
        case .agents: "cpu"
        case .languageServers: "chevron.left.forwardslash.chevron.right"
        case .general: "gearshape"
        case .permissions: "lock.shield"
        case .appearance: "paintbrush"
        }
    }
}
```

- [ ] **Step 4: Write the model and view**

```swift
// App/LanguageServersSettingsView.swift
import Observation
import SwiftUI
import TillerLSP

@MainActor
@Observable
final class LanguageServersSettingsModel {
    struct Row: Identifiable {
        let definition: LanguageServerDefinition
        let resolvedPath: String?
        var id: String { definition.id }
        var isInstalled: Bool { resolvedPath != nil }
    }

    private let catalog: [LanguageServerDefinition]
    private let locator: ExecutableLocating

    private(set) var rows: [Row] = []
    private(set) var isRefreshing = false

    init(catalog: [LanguageServerDefinition] = LanguageServerCatalog.all,
         locator: ExecutableLocating = LoginShellExecutableLocator()) {
        self.catalog = catalog
        self.locator = locator
    }

    /// Looks every binary up concurrently. Each lookup is a login shell that
    /// sources the user's whole profile — serially, twenty of them would block
    /// the panel for around two seconds.
    func refresh() async {
        isRefreshing = true
        defer { isRefreshing = false }

        let definitions = catalog
        let locator = self.locator
        let resolved = await withTaskGroup(of: (String, String?).self) { group in
            for definition in definitions {
                group.addTask { (definition.id, await locator.locate(definition.executable)) }
            }
            var found: [String: String?] = [:]
            for await (id, path) in group { found[id] = path }
            return found
        }

        rows = definitions
            .map { Row(definition: $0, resolvedPath: resolved[$0.id] ?? nil) }
            .sorted {
                if $0.isInstalled != $1.isInstalled { return $0.isInstalled }
                return $0.definition.displayName < $1.definition.displayName
            }
    }
}

/// Shows which language servers are present and, for the ones that are not,
/// the exact command that would install them. Tiller runs nothing here: the
/// user copies the command and decides (decisions 4 and 15).
struct LanguageServersSettingsView: View {
    @State private var model = LanguageServersSettingsModel()

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                header
                ForEach(model.rows) { row in
                    self.row(row)
                    Divider().overlay(AppTheme.hairline)
                }
            }
            .padding(16)
        }
        .task { await model.refresh() }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text("Language Servers")
                    .font(.headline)
                    .foregroundStyle(AppTheme.title)
                Spacer()
                Button("Refresh") { Task { await model.refresh() } }
                    .disabled(model.isRefreshing)
            }
            Text("Tiller uses language servers you install yourself. It never downloads or runs an installer.")
                .font(.system(size: 11))
                .foregroundStyle(AppTheme.subtitle)
        }
    }

    private func row(_ row: LanguageServersSettingsModel.Row) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Circle()
                .fill(row.isInstalled ? Color.green : AppTheme.hairline)
                .frame(width: 8, height: 8)
                .padding(.top, 5)
            VStack(alignment: .leading, spacing: 3) {
                Text(row.definition.displayName)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.title)
                if let path = row.resolvedPath {
                    Text(path)
                        .font(.system(size: 10).monospaced())
                        .foregroundStyle(AppTheme.subtitle)
                        .lineLimit(1)
                        .truncationMode(.middle)
                } else {
                    HStack(spacing: 6) {
                        Text(row.definition.installCommand)
                            .font(.system(size: 10).monospaced())
                            .foregroundStyle(AppTheme.subtitle)
                            .textSelection(.enabled)
                        Button {
                            NSPasteboard.general.clearContents()
                            NSPasteboard.general.setString(
                                row.definition.installCommand, forType: .string)
                        } label: {
                            Image(systemName: "doc.on.doc")
                        }
                        .buttonStyle(.plain)
                        .help("Copy install command")
                    }
                }
            }
            Spacer()
            Text(row.definition.fileExtensions.prefix(4).map { ".\($0)" }.joined(separator: " "))
                .font(.system(size: 10).monospaced())
                .foregroundStyle(AppTheme.subtitle)
        }
    }
}
```

- [ ] **Step 5: Route to it**

```swift
// App/SettingsSurface.swift — detailPane
        case .languageServers: LanguageServersSettingsView()
```

- [ ] **Step 6: Run the tests, the gate, and commit**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServersSettingsTests 2>&1 | tail -20`
Expected: 5 tests pass.

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: add a Language Servers settings panel"
```

---

### Task 16: Status indicator in the editor header

The smallest piece of UI in the plan and, per decision 16, the one with the best ratio of value to cost: it is what turns "this feature is broken" into "wait thirty seconds" or "install the binary".

**Files:**
- Create: `App/CodeEditor/LanguageServerStatusBadge.swift`
- Create: `AppTests/LanguageServerStatusBadgeTests.swift`
- Modify: `App/CodeEditor/CodeEditorTabView.swift`

**Interfaces:**
- Consumes: `LanguageServerStatus` (Task 8), `LanguageServerCenter.status(for:)` (Task 10).
- Produces:
  - `struct LanguageServerStatusBadge: View { let status: LanguageServerStatus? }`
  - `nonisolated static func label(for status: LanguageServerStatus?) -> String?`
  - `nonisolated static func tint(for status: LanguageServerStatus?) -> Color`

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LanguageServerStatusBadgeTests.swift
import Foundation
import SwiftUI
import Testing
import TillerLSP
@testable import Tiller

@Test func eachStatusHasItsOwnLabel() {
    #expect(LanguageServerStatusBadge.label(for: .notInstalled) == "No language server")
    #expect(LanguageServerStatusBadge.label(for: .starting) == "Starting…")
    #expect(LanguageServerStatusBadge.label(for: .ready) == "Ready")
    #expect(LanguageServerStatusBadge.label(for: .indexing("Indexing")) == "Indexing…")
    #expect(LanguageServerStatusBadge.label(for: .stopped(reason: "crashed")) == "Stopped")
}

@Test func theProgressTitleFromTheServerIsUsedVerbatim() {
    // rust-analyzer reports several phases; showing its own word is more
    // informative than a generic "Working".
    #expect(LanguageServerStatusBadge.label(for: .indexing("Loading Cargo workspace"))
        == "Loading Cargo workspace…")
}

@Test func anUnknownStatusShowsNothingRatherThanAnEmptyBadge() {
    // A file with no catalog row has no server and needs no badge.
    #expect(LanguageServerStatusBadge.label(for: nil) == nil)
}

@Test func readyAndFailureStatesAreTintedDifferently() {
    #expect(LanguageServerStatusBadge.tint(for: .ready)
        != LanguageServerStatusBadge.tint(for: .stopped(reason: "x")))
    #expect(LanguageServerStatusBadge.tint(for: .notInstalled)
        != LanguageServerStatusBadge.tint(for: .ready))
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServerStatusBadgeTests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Write the badge**

```swift
// App/CodeEditor/LanguageServerStatusBadge.swift
import SwiftUI
import TillerLSP

/// Tells the user why the editor is quiet.
///
/// An empty completion popup looks identical whether the server is missing,
/// still indexing, dead, or simply has nothing to suggest here. Without this,
/// every one of those reads as "the feature is broken".
struct LanguageServerStatusBadge: View {
    let status: LanguageServerStatus?

    var body: some View {
        if let text = Self.label(for: status) {
            HStack(spacing: 4) {
                Circle()
                    .fill(Self.tint(for: status))
                    .frame(width: 5, height: 5)
                Text(text)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            .help(Self.help(for: status) ?? text)
        }
    }

    nonisolated static func label(for status: LanguageServerStatus?) -> String? {
        switch status {
        case .none: nil
        case .notInstalled: "No language server"
        case .starting: "Starting…"
        case .indexing(let title): "\(title)…"
        case .ready: "Ready"
        case .stopped: "Stopped"
        }
    }

    nonisolated static func tint(for status: LanguageServerStatus?) -> Color {
        switch status {
        case .none: .clear
        case .notInstalled: .secondary
        case .starting, .indexing: .orange
        case .ready: .green
        case .stopped: .red
        }
    }

    private nonisolated static func help(for status: LanguageServerStatus?) -> String? {
        switch status {
        case .notInstalled: "Install a language server for this file type in Settings › Language Servers."
        case .indexing: "The server is still building its index. Results will be incomplete until it finishes."
        case .stopped(let reason): "Language server stopped: \(reason)"
        default: nil
        }
    }
}
```

- [ ] **Step 4: Put it in the header**

```swift
// App/CodeEditor/CodeEditorTabView.swift — private var header
    private var header: some View {
        HStack(spacing: 8) {
            Text(document.fileURL.path)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            Text(detectedLanguage.tsName)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.secondary)
            if document.isReadOnly {
                Text("Read-only")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            Spacer()
            LanguageServerStatusBadge(status: center.status(for: document.fileURL))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }
```

- [ ] **Step 5: Run the tests, the gate, and commit**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LanguageServerStatusBadgeTests 2>&1 | tail -20`
Expected: 4 tests pass.

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: show language server status in the editor header"
```

---

## Phase 8 — Hover

### Task 17: Hover popover

The only from-scratch UI in the plan. By decision 11 it serves hover alone — diagnostics go to the Problems panel — so it carries its cost without a second use to amortise it.

**Files:**
- Create: `App/CodeEditor/LSPHoverPopover.swift`
- Create: `AppTests/LSPHoverPopoverTests.swift`
- Modify: `App/CodeEditor/CodeEditorTabView.swift`

**Interfaces:**
- Consumes: `LanguageServerCenter` (Task 10), `LineIndex` (Task 6).
- Produces:
  - `@MainActor final class LSPHoverController`
  - `init(fileURL: URL, center: LanguageServerCenter, textView: TextView)`
  - `func attach()` / `func detach()`
  - `nonisolated static func trimmedMarkdown(_ text: String) -> String`
  - `struct LSPHoverPopover: View { let markdown: String }`

> **Hover has no API in the editor library, so this is an `NSTrackingArea` on the text view plus an `NSPopover`.** Two rules keep it from becoming annoying: a dwell delay, so moving the pointer across the file does not fire a request per character; and cancellation of the previous request, so a fast pointer does not queue work the user has already moved past.

- [ ] **Step 1: Write the failing tests**

```swift
// AppTests/LSPHoverPopoverTests.swift
import Foundation
import Testing
@testable import Tiller

@Test func fencedCodeBlocksSurviveTrimming() {
    let text = "```rust\nfn main() {}\n```"
    #expect(LSPHoverController.trimmedMarkdown(text).contains("fn main()"))
}

@Test func trailingAndLeadingWhitespaceIsRemoved() {
    #expect(LSPHoverController.trimmedMarkdown("\n\n  text  \n\n") == "text")
}

@Test func veryLongHoverTextIsTruncatedSoThePopoverStaysReadable() {
    let long = String(repeating: "line\n", count: 400)
    let trimmed = LSPHoverController.trimmedMarkdown(long)
    #expect(trimmed.components(separatedBy: "\n").count <= 41)
    #expect(trimmed.hasSuffix("…"))
}

@Test func textShorterThanTheLimitIsNotTruncated() {
    let short = "one\ntwo\nthree"
    #expect(LSPHoverController.trimmedMarkdown(short) == short)
    #expect(!LSPHoverController.trimmedMarkdown(short).hasSuffix("…"))
}

@Test func emptyHoverTextTrimsToEmpty() {
    #expect(LSPHoverController.trimmedMarkdown("   \n  ").isEmpty)
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPHoverPopoverTests 2>&1 | tail -20`
Expected: FAIL.

- [ ] **Step 3: Write the controller and popover**

```swift
// App/CodeEditor/LSPHoverPopover.swift
import AppKit
import CodeEditTextView
import MarkdownUI
import SwiftUI
import TillerLSP

/// Shows a language server's `textDocument/hover` text near the pointer.
///
/// The editor library has no hover API, so this is a tracking area plus an
/// `NSPopover`. The dwell delay is what keeps it from firing a request per
/// character as the pointer crosses the file, and cancelling the previous
/// request is what keeps a fast pointer from queueing work already moved past.
@MainActor
final class LSPHoverController: NSObject {
    private static let dwell = Duration.milliseconds(400)
    private static let maximumLines = 40

    private let fileURL: URL
    private let center: LanguageServerCenter
    private weak var textView: TextView?
    private var trackingArea: NSTrackingArea?
    private var pendingTask: Task<Void, Never>?
    private var popover: NSPopover?

    init(fileURL: URL, center: LanguageServerCenter, textView: TextView) {
        self.fileURL = fileURL
        self.center = center
        self.textView = textView
        super.init()
    }

    func attach() {
        guard let textView else { return }
        detach()
        let area = NSTrackingArea(
            rect: textView.bounds,
            options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: self, userInfo: nil)
        textView.addTrackingArea(area)
        trackingArea = area
    }

    func detach() {
        pendingTask?.cancel()
        pendingTask = nil
        dismiss()
        if let trackingArea, let textView { textView.removeTrackingArea(trackingArea) }
        trackingArea = nil
    }

    override func mouseMoved(with event: NSEvent) {
        pendingTask?.cancel()
        dismiss()
        guard let textView else { return }
        let point = textView.convert(event.locationInWindow, from: nil)
        guard let offset = textView.layoutManager.textOffsetAtPoint(point) else { return }

        pendingTask = Task { [weak self] in
            try? await Task.sleep(for: Self.dwell)
            guard !Task.isCancelled, let self else { return }
            await self.show(atOffset: offset, point: point)
        }
    }

    override func mouseExited(with event: NSEvent) {
        pendingTask?.cancel()
        dismiss()
    }

    private func show(atOffset offset: Int, point: NSPoint) async {
        guard let textView else { return }
        let index = LineIndex(text: textView.string)
        guard let position = index.position(atUTF16Offset: offset) else { return }
        guard let raw = await center.host.hover(fileURL: fileURL, position: position) else { return }
        let markdown = Self.trimmedMarkdown(raw)
        guard !markdown.isEmpty, !Task.isCancelled else { return }

        let popover = NSPopover()
        popover.behavior = .transient
        popover.animates = false
        popover.contentViewController = NSHostingController(
            rootView: LSPHoverPopover(markdown: markdown))
        popover.show(relativeTo: NSRect(origin: point, size: CGSize(width: 1, height: 1)),
                     of: textView, preferredEdge: .maxY)
        self.popover = popover
    }

    private func dismiss() {
        popover?.close()
        popover = nil
    }

    /// Servers happily return whole doc comments. Past about forty lines the
    /// popover stops being a hint and becomes a document that covers the code
    /// the user was reading.
    nonisolated static func trimmedMarkdown(_ text: String) -> String {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let lines = trimmed.components(separatedBy: "\n")
        guard lines.count > maximumLines else { return trimmed }
        return lines.prefix(maximumLines).joined(separator: "\n") + "\n…"
    }
}

struct LSPHoverPopover: View {
    let markdown: String

    var body: some View {
        ScrollView {
            Markdown(markdown)
                .markdownTextStyle { FontSize(11) }
                .padding(10)
                .textSelection(.enabled)
        }
        .frame(maxWidth: 460, maxHeight: 320)
    }
}
```

If `textView.layoutManager` does not expose `textOffsetAtPoint(_:)` under that name, use the accessor `TextLayoutManager` does provide for point-to-offset conversion — the mouse-handling code in `TextView+Mouse.swift` already performs this conversion and is the reference to follow.

- [ ] **Step 4: Attach it from the view**

```swift
    @State private var hoverController: LSPHoverController?
```

In the same `.task(id: document.fileURL)`, after the delegates:

```swift
                if let textView = editorState.textViewController?.textView {
                    let controller = LSPHoverController(
                        fileURL: document.fileURL, center: center, textView: textView)
                    controller.attach()
                    hoverController = controller
                }
```

and tear down on disappear, so a tracking area never outlives its text view:

```swift
            .onDisappear {
                hoverController?.detach()
                hoverController = nil
                Task { await center.close(fileURL: document.fileURL) }
            }
```

- [ ] **Step 5: Run the tests**

Run: `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -configuration Debug -only-testing:TillerTests/LSPHoverPopoverTests 2>&1 | tail -20`
Expected: 5 tests pass.

- [ ] **Step 6: Verify by hand**

Hover a symbol in a Rust file. After roughly half a second a popover shows its signature and doc comment. Moving the pointer away closes it; moving quickly across the file opens nothing.

- [ ] **Step 7: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "feat: show hover documentation in the code editor"
```

---

## Phase 9 — Swift

### Task 18: Make `App/**.swift` resolvable, and document the limit

Everything until now was verified on Rust, where the server self-configures from `Cargo.toml`. Swift is the one language in the catalog whose build system Tiller cannot hand the server for free.

**Files:**
- Create: `docs/language-servers.md`
- Modify: `CLAUDE.md`
- Modify: `.gitignore`

**Interfaces:**
- Consumes: everything above.
- Produces: no new code. A documented, reproducible setup and an honest statement of what does not work.

> **The shape of the problem.** sourcekit-lsp understands SwiftPM packages and `compile_commands.json`. It does not read `.xcodeproj`. So `Packages/*/Sources/**.swift` works well, and `App/**.swift` — the largest and most-edited directory in this repo — resolves nothing across modules: no `import TillerCore`, reduced completion, unreliable diagnostics. Nothing in the code fixes this; a build-server description does.

- [ ] **Step 1: Confirm the split by hand**

Open `Packages/TillerCore/Sources/TillerCore/AgentActivityModel.swift` in Tiller. Completion should offer real symbols. Then open `App/AppModel.swift`: completion collapses to local scope, and `import TillerCore` resolves nothing. That contrast is the thing being documented.

- [ ] **Step 2: Generate a build server description**

```bash
brew install xcode-build-server
xcode-build-server config -project Tiller.xcodeproj -scheme Tiller
```

This writes `buildServer.json` at the repo root, which is already listed as a root marker for `sourcekit-lsp` in the catalog (Task 7).

- [ ] **Step 3: Check whether it actually helped**

Reopen `App/AppModel.swift` in Tiller after the badge reports Ready. If cross-module symbols now resolve, the setup works and gets documented as recommended. If it does not — a common outcome with generated projects, since `xcodegen generate` rewrites the project and invalidates the description — document it as attempted and not working, and stop. Do not spend the phase fighting it.

- [ ] **Step 4: Ignore the generated file**

```
# .gitignore
buildServer.json
```

It describes one machine's derived-data paths, and `xcodegen generate` invalidates it. Committing it would hand every other checkout a stale, wrong description — worse than none.

- [ ] **Step 5: Write the documentation**

`docs/language-servers.md` covers: which servers Tiller knows about and how to install each; that Tiller never installs anything; that a worktree is an LSP root, so several open worktrees mean several indexes; the four-server cap and what eviction looks like; and — stated plainly — that `App/**.swift` is degraded without `buildServer.json`, with the regeneration command and the note that it must be re-run after `xcodegen generate`.

- [ ] **Step 6: Add a paragraph to `CLAUDE.md`**

Under "Architecture", after the agent-adapters section:

```markdown
### Language servers (`TillerLSP`)

The code editor's completion, go-to-definition, diagnostics and hover come from
real language servers, one process per (server × workspace root), started lazily
on first document open and capped at four with LRU eviction. The server table in
`LanguageServerCatalog` is **pure data**: adding a language is a row, never a
code path — a single per-language branch in `TillerLSP` would make coverage cost
scale with language count again, which is the thing the design avoids.

Tiller never installs a server. Settings › Language Servers reports what is on
the user's login-shell `PATH` (a GUI app's inherited `PATH` cannot see
`~/.cargo/bin`, so lookups go through `zsh -lc`, exactly as `AgentInstaller`
does for agent CLIs) and shows the install command to copy.

Known limit: sourcekit-lsp does not read `.xcodeproj`, so `App/**.swift`
resolves nothing across modules unless a `buildServer.json` is generated. The
`Packages/*` SwiftPM targets work without any setup. See
`docs/language-servers.md`.
```

- [ ] **Step 7: Run the gate and commit**

```bash
bash Scripts/ci.sh
git add -A
git commit -m "docs: document language server setup and the sourcekit-lsp limit"
```

---

## Self-review

Run against the sixteen decisions, per the writing-plans checklist.

**Decision coverage.** 1 → the whole plan targets the editor. 2 → `TillerLSP` imports no UI framework, verified by its manifest; no `tillerctl` surface is added. 3 → Task 7 is a data table, and `LanguageServerCatalogTests` asserts uniqueness and completeness across all rows. 4 → nothing in the plan downloads or executes an installer; Task 15 displays commands only. 6 → completion (Task 11), go-to-definition (Task 12), diagnostics (Task 13), hover (Task 17). 7 → Tasks 2-5 are the hand-written client. 8 → Task 1 extracts types only; the ACP transport is untouched. 9 → Task 8 covers lazy start, cap, eviction, restart budget. 10 → Task 8's `changeDocument` is full-text and debounced, asserted by `changeIsDebouncedIntoASingleDidChange…`. 11 → Task 13 underlines, Task 14 lists; the hover popover carries no diagnostics. 12 → `diagnosticsUnderARootIncludeFilesThatWereNeverOpened` and `diagnosticsArrivingForAFileWithNoOpenTabAreStillDelivered`. 13 → Task 12, plus read-only enforced in `CodeDocument.save()`. 14 → Task 9 records fixtures; `FakeLSPTransport` covers timing; no task spawns a server inside `ci.sh`. 15 → Task 15. 16 → Task 16 for the badge, Task 8 for the restart budget.

**Placeholder scan.** No `TBD`, no "add error handling", no "similar to Task N". Two steps say "if the accessor is not named X, use the one the library provides" (Tasks 13 and 17) — those are named, bounded lookups against a specific file, not deferred decisions.

**Type consistency.** `LSPPosition`/`LSPRange`/`LSPLocation`/`LSPDiagnostic`/`LSPCompletionItem` are defined in Task 5 and used unchanged after. `ServerKey` and `LanguageServerStatus` are defined in Task 8 and used in Tasks 10 and 16. `LanguageServerCenter.isReadOnlyLocation` is defined in Task 10 and consumed in Tasks 12 and 14. `LineIndex` is defined in Task 6 and consumed in Tasks 8, 12, 13 and 17. `CodeDocument.init(fileURL:isReadOnly:)` is introduced in Task 12 and used by `AppModel.openCodeFile` in the same task.

---

## Risks

**1. `App/**.swift` will be the weakest experience, and it is where the author works.** sourcekit-lsp cannot read `.xcodeproj`. Task 18 attempts a fix and is explicitly allowed to conclude that it did not work. Nothing earlier in the plan depends on it.

**2. The hover popover is the only from-scratch UI, and it serves the least valuable of the four features.** Decision 11 sent diagnostics to a panel instead, so the popover has no second use to amortise its cost. It is deliberately last: if the plan is cut short, Task 17 is the cheapest thing to drop.

**3. Seventeen of twenty catalog rows ship unverified.** Acceptable while the table stays pure data. The failure mode is a language that silently does nothing, which the Settings panel and the header badge both make visible. The guard is `LanguageServerCatalogTests` plus the review rule that no per-language branch may enter `TillerLSP`.

**4. Memory.** Four concurrent servers at 1-3 GB each is real pressure on a machine already noted for high `IOSurface` usage. The cap and LRU eviction bound it; if it still bites, the lever is lowering `maxConcurrentServers` from 4, which is one number in one initialiser.

**5. `EmphasisManager` draws a straight underline, not a wavy one.** No squiggle is available. If a wavy underline turns out to matter, it is a change inside `LSPDiagnosticsPainter` plus custom drawing, not a change to anything upstream of it.

---

## Execution order

Tasks are numbered in dependency order and each ends green. The single most informative checkpoint is **Task 11, step 7**: if the completion popup shows real `rust-analyzer` symbols, the entire client is proven and everything after it is presentation.

Suggested review points: after Task 1 (the refactor is the only task that can break existing behaviour), after Task 8 (the whole package is testable in isolation at that point), and after Task 11 (first user-visible result).
