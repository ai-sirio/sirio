# Canonical `tillerctl panel` API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Tiller's overlapping pane commands with one deterministic `tillerctl panel` API and provision one canonical Tiller skill to all five supported harnesses.

**Architecture:** `tillerctl panel` maps one-to-one onto canonical `panel.*` control requests. `AppModel` resolves only explicit or environment-derived targets, allocates pane UUIDs before tab/split mutation, waits for `PaneRegistry`, and rolls back failed mounts. The public `skills/tiller/SKILL.md` is bundled into the app and installed worktree-locally through a marker-guarded `TillerSkillProvisioner`.

**Tech Stack:** Swift 6, Swift Argument Parser, Swift Testing, SwiftUI/AppKit, XcodeGen, line-delimited JSON over Tiller's Unix control socket.

## Global Constraints

- Work only in `/Users/enzopiopalmisano/Desktop/Progetti/tiller-feat-tillerctl-panel-api` on `feat-tillerctl-panel-api`.
- Preserve macOS 15+ and Swift 6 support; add no dependencies.
- Use Swift Testing (`@Test`, `#expect`, `#require`), not XCTest.
- Never use `selectedWorktree`, `activeTabId`, or the active pane as an implicit control target.
- `panel create` and `panel split` must not activate Tiller or steal macOS focus; only `panel focus` may do so.
- Preserve `\r` as the terminal Enter byte.
- Keep workspace, notification, system, and session-restore commands unchanged.
- Remove old flat pane commands and redundant `surface.*` pane methods without aliases or shims.
- Provision skills only inside the worktree; never edit user-global harness configuration.
- `Scripts/ci.sh` must print `CI OK` before completion.

---

### Task 1: Canonical control request builders and socket wait semantics

**Files:**
- Modify: `Packages/TillerControl/Sources/TillerControl/TillerctlRequestBuilder.swift`
- Modify: `Packages/TillerControl/Sources/TillerControl/ControlClient.swift`
- Modify: `Packages/TillerControl/Tests/TillerControlTests/TillerctlRequestTests.swift`

**Interfaces:**
- Consumes: existing `ControlRequest`, `ControlResponse`, and `ControlClient` transport.
- Produces: `panelCreate(worktree:cmd:)`, `panelSplit(from:direction:cmd:)`, `panelList(worktree:)`, `panelWrite(id:input:)`, `panelKey(id:key:)`, `panelRead(id:)`, `panelWait(id:timeoutMs:)`, `panelFocus(id:)`, `panelClose(id:)`.
- Produces: `ControlClient.roundTrip(socketPath:request:timeoutSeconds: Int? = 3600)`; `nil` means no receive deadline.

- [ ] **Step 1: Replace surface-builder coverage with failing canonical panel tests**

```swift
import Testing
@testable import TillerControl

@Test func canonicalPanelBuildersUseStableMethodsAndParameters() {
    let create = TillerctlRequestBuilder.panelCreate(worktree: "worktree", cmd: "codex")
    #expect(create.method == "panel.create")
    #expect(create.params == ["worktree": "worktree", "cmd": "codex"])

    let split = TillerctlRequestBuilder.panelSplit(
        from: "source", direction: "right", cmd: "pi"
    )
    #expect(split.method == "panel.split")
    #expect(split.params == ["from": "source", "direction": "right", "cmd": "pi"])

    let list = TillerctlRequestBuilder.panelList(worktree: "/repo/worktree")
    #expect(list.method == "panel.list")
    #expect(list.params == ["worktree": "/repo/worktree"])

    #expect(TillerctlRequestBuilder.panelWrite(id: "pane", input: "hello\r").params
        == ["id": "pane", "input": "hello\r"])
    #expect(TillerctlRequestBuilder.panelKey(id: "pane", key: "enter").params
        == ["id": "pane", "key": "enter"])
    #expect(TillerctlRequestBuilder.panelRead(id: "pane").method == "panel.read")
    #expect(TillerctlRequestBuilder.panelWait(id: "pane", timeoutMs: 250).params
        == ["id": "pane", "timeoutMs": "250"])
    #expect(TillerctlRequestBuilder.panelFocus(id: "pane").method == "panel.focus")
    #expect(TillerctlRequestBuilder.panelClose(id: "pane").method == "panel.close")
}

@Test func optionalPanelParametersAreOmitted() {
    #expect(TillerctlRequestBuilder.panelCreate(worktree: "worktree", cmd: nil).params
        == ["worktree": "worktree"])
    #expect(TillerctlRequestBuilder.panelSplit(from: "pane", direction: "down", cmd: nil).params
        == ["from": "pane", "direction": "down"])
    #expect(TillerctlRequestBuilder.panelWait(id: "pane", timeoutMs: nil).params
        == ["id": "pane"])
}
```

- [ ] **Step 2: Run the focused tests and confirm the new builders are missing**

Run:

```bash
swift test --package-path Packages/TillerControl --filter canonicalPanelBuilders
```

Expected: compilation fails because `panelSplit`, `panelList`, `panelKey`, `panelFocus`, and `panelClose` do not exist.

- [ ] **Step 3: Implement the canonical builder family and delete redundant pane builders**

```swift
public static func panelSplit(from: String, direction: String, cmd: String?) -> ControlRequest {
    var params = ["from": from, "direction": direction]
    if let cmd { params["cmd"] = cmd }
    return request("panel.split", params)
}

public static func panelList(worktree: String) -> ControlRequest {
    request("panel.list", ["worktree": worktree])
}

public static func panelKey(id: String, key: String) -> ControlRequest {
    request("panel.key", ["id": id, "key": key])
}

public static func panelFocus(id: String) -> ControlRequest {
    request("panel.focus", ["id": id])
}

public static func panelClose(id: String) -> ControlRequest {
    request("panel.close", ["id": id])
}
```

Keep `panelCreate`, `panelWrite`, `panelRead`, and `panelWait`, updating names and optional-parameter omission to match the tests. Delete `surfaceList`, `paneSurfaces`, `surfaceFocus`, `surfaceSplit`, `surfaceSendText`, `surfaceSendKey`, and `surfaceClose` only after their CLI/server callers are migrated in Tasks 2 and 4; until then, keep the branch compiling and remove them in the same commit as the last caller.

- [ ] **Step 4: Make the receive deadline optional**

```swift
public static func roundTrip(
    socketPath: String,
    request: ControlRequest,
    timeoutSeconds: Int? = 3600
) throws -> ControlResponse {
    let sunPathCapacity = MemoryLayout<sockaddr_un>.size
        - MemoryLayout.offset(of: \sockaddr_un.sun_path)!
    guard socketPath.utf8CString.count <= sunPathCapacity else {
        throw ControlClientError.connectFailed("socket path too long: \(socketPath)")
    }
    let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else {
        throw ControlClientError.connectFailed("socket: \(errnoDescription)")
    }
    defer { close(fd) }

    if let timeoutSeconds {
        var tv = timeval(tv_sec: timeoutSeconds, tv_usec: 0)
        let rcvOptRet = withUnsafePointer(to: &tv) {
            $0.withMemoryRebound(to: timeval.self, capacity: 1) { tvp in
                setsockopt(
                    fd,
                    SOL_SOCKET,
                    SO_RCVTIMEO,
                    tvp,
                    socklen_t(MemoryLayout<timeval>.size)
                )
            }
        }
        guard rcvOptRet == 0 else {
            throw ControlClientError.connectFailed(
                "setsockopt SO_RCVTIMEO: \(errnoDescription)"
            )
        }
    }

    try connectSocket(fd: fd, socketPath: socketPath)
    try writeRequest(fd: fd, request: request)
    return try readResponse(fd: fd)
}
```

The only transport semantic change is the optional deadline; connection, framing, and decoding stay unchanged.

- [ ] **Step 5: Run the package suite**

Run:

```bash
swift test --package-path Packages/TillerControl
```

Expected: all TillerControl tests pass.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerControl/Sources/TillerControl/TillerctlRequestBuilder.swift \
  Packages/TillerControl/Sources/TillerControl/ControlClient.swift \
  Packages/TillerControl/Tests/TillerControlTests/TillerctlRequestTests.swift
git commit -m "refactor(control): canonicalize panel requests"
```

---

### Task 2: Replace flat pane commands with `tillerctl panel`

**Files:**
- Modify: `Packages/TillerControl/Package.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/Tillerctl.swift`
- Modify: `Packages/TillerControl/Sources/tillerctl/CmuxCommands.swift`
- Create: `Packages/TillerControl/Tests/TillerControlTests/TillerctlCommandTests.swift`

**Interfaces:**
- Consumes: canonical builders from Task 1 and `TerminalKey` from TillerCore.
- Produces: `PanelDirection`, `requiredPanelTarget(explicit:environment:key:option:)`, `createdPaneOutput(id:json:)`, and nested `Panel.Create|Split|List|Write|Key|Read|Wait|Focus|Close` commands.
- Removes: `NewSplit`, `ListPanels`, `ListPaneSurfaces`, `FocusPanel`, `Send`, `SendKey`, `ClosePanel`, and `defaultSurface`.

- [ ] **Step 1: Add executable-target test access and failing command-contract tests**

Add `"tillerctl"` to the `TillerControlTests` target dependencies, then create:

```swift
import ArgumentParser
import Testing
@testable import tillerctl

@Test func rootExposesOnlyCanonicalPanelCommands() throws {
    let rootNames = Set(Tillerctl.configuration.subcommands.compactMap(\.configuration.commandName))
    #expect(rootNames.isDisjoint(with: [
        "new-split", "list-panels", "list-pane-surfaces",
        "focus-panel", "send", "send-key", "close-panel",
    ]))

    let panelNames = Set(Panel.configuration.subcommands.compactMap(\.configuration.commandName))
    #expect(panelNames == [
        "create", "split", "list", "write", "key",
        "read", "wait", "focus", "close",
    ])
}

@Test func explicitPanelTargetWinsThenEnvironmentIsUsed() throws {
    let environment = ["TILLER_PANE_ID": "environment-pane"]
    #expect(try requiredPanelTarget(
        explicit: "explicit-pane",
        environment: environment,
        key: "TILLER_PANE_ID",
        option: "--id"
    ) == "explicit-pane")
    #expect(try requiredPanelTarget(
        explicit: nil,
        environment: environment,
        key: "TILLER_PANE_ID",
        option: "--id"
    ) == "environment-pane")
    #expect(throws: ValidationError.self) {
        try requiredPanelTarget(
            explicit: nil,
            environment: [:],
            key: "TILLER_PANE_ID",
            option: "--id"
        )
    }
}

@Test func splitParsesOnlySupportedDirections() throws {
    let parsed = try Tillerctl.parseAsRoot([
        "panel", "split", "right", "--from", "source", "--cmd", "pi",
    ])
    let split = try #require(parsed as? Panel.Split)
    #expect(split.direction == .right)
    #expect(split.from == "source")
    #expect(split.cmd == "pi")

    #expect(throws: (any Error).self) {
        try Tillerctl.parseAsRoot(["panel", "split", "diagonal"])
    }
}

@Test func createdPaneOutputIsStable() throws {
    #expect(try createdPaneOutput(id: "pane-id", json: false) == "pane-id")
    #expect(try createdPaneOutput(id: "pane-id", json: true) == #"{"id":"pane-id"}"#)
}
```

- [ ] **Step 2: Run the focused tests and confirm the old command set fails the contract**

Run:

```bash
swift test --package-path Packages/TillerControl --filter rootExposesOnlyCanonicalPanelCommands
```

Expected: failure because the flat pane commands remain and the nested panel commands are incomplete.

- [ ] **Step 3: Add pure parsing/output helpers**

```swift
enum PanelDirection: String, CaseIterable, ExpressibleByArgument {
    case left, right, up, down
}

func requiredPanelTarget(
    explicit: String?,
    environment: [String: String],
    key: String,
    option: String
) throws -> String {
    if let explicit, !explicit.isEmpty { return explicit }
    if let value = environment[key], !value.isEmpty { return value }
    throw ValidationError("Missing \(option) and $\(key) is not set")
}

func createdPaneOutput(id: String, json: Bool) throws -> String {
    guard json else { return id }
    let data = try JSONSerialization.data(
        withJSONObject: ["id": id],
        options: [.sortedKeys]
    )
    return String(decoding: data, as: UTF8.self)
}
```

- [ ] **Step 4: Implement the complete nested command namespace**

Register exactly these subcommands:

```swift
struct Panel: ParsableCommand {
    static let configuration = CommandConfiguration(
        subcommands: [
            Create.self, Split.self, List.self, Write.self, Key.self,
            Read.self, Wait.self, Focus.self, Close.self,
        ]
    )
}
```

Each `run()` must make the direct builder call below:

```swift
// create
let worktree = try requiredPanelTarget(
    explicit: worktree,
    environment: ProcessInfo.processInfo.environment,
    key: "TILLER_WORKTREE_ID",
    option: "--worktree"
)
let response = try roundTripOrDie(
    TillerctlRequestBuilder.panelCreate(worktree: worktree, cmd: cmd),
    socket: socketOptions.socket
)
let id = try requireCreatedPaneID(response)
print(try createdPaneOutput(id: id, json: json))

// split
let source = try requiredPanelTarget(
    explicit: from,
    environment: ProcessInfo.processInfo.environment,
    key: "TILLER_PANE_ID",
    option: "--from"
)
let response = try roundTripOrDie(
    TillerctlRequestBuilder.panelSplit(
        from: source,
        direction: direction.rawValue,
        cmd: cmd
    ),
    socket: socketOptions.socket
)
print(try createdPaneOutput(id: requireCreatedPaneID(response), json: json))

// list
let worktree = try requiredPanelTarget(
    explicit: worktree,
    environment: ProcessInfo.processInfo.environment,
    key: "TILLER_WORKTREE_ID",
    option: "--worktree"
)
let response = try roundTripOrDie(
    TillerctlRequestBuilder.panelList(worktree: worktree),
    socket: socketOptions.socket
)
printRows(
    response,
    key: "panels",
    columns: ["id", "tab", "title", "agent", "active"],
    asJSON: json
)

// write
let payload = input + (enter ? "\r" : "")
_ = try roundTripOrDie(
    TillerctlRequestBuilder.panelWrite(id: id, input: payload),
    socket: socketOptions.socket
)

// key
_ = try roundTripOrDie(
    TillerctlRequestBuilder.panelKey(id: id, key: key),
    socket: socketOptions.socket
)

// read: preserve the existing raw stdout Data write
// wait: nil timeout uses timeoutSeconds: nil; a finite timeout uses its existing grace period
// focus
_ = try roundTripOrDie(
    TillerctlRequestBuilder.panelFocus(id: id),
    socket: socketOptions.socket
)

// close: resolve --id, then TILLER_PANE_ID
_ = try roundTripOrDie(
    TillerctlRequestBuilder.panelClose(id: target),
    socket: socketOptions.socket
)
```

Add:

```swift
func requireCreatedPaneID(_ response: ControlResponse) throws -> String {
    guard response.ok, let id = response.result?["id"], !id.isEmpty else {
        throw ValidationError(response.error ?? "panel creation returned no id")
    }
    return id
}
```

For `Panel.Wait`, call `roundTripOrDie(..., timeoutSeconds: nil)` when `--timeout-ms` is absent. When present, use `max(1, (timeoutMs + 999) / 1_000 + 30)` so the server timeout wins before the client receive deadline.

- [ ] **Step 5: Remove the flat pane structs and request helpers**

Delete the seven flat pane command registrations from `Tillerctl.configuration`, delete their structs and `defaultSurface` from `CmuxCommands.swift`, then delete the now-unreferenced `surface.*`/`pane.surfaces` builders from `TillerctlRequestBuilder.swift`. Keep `roundTripOrDie`, `printRows`, and all non-pane cmux-parity commands.

- [ ] **Step 6: Run tests and inspect generated help**

Run:

```bash
swift test --package-path Packages/TillerControl
swift build --package-path Packages/TillerControl --product tillerctl
Packages/TillerControl/.build/debug/tillerctl panel --help
Packages/TillerControl/.build/debug/tillerctl --help
```

Expected: tests pass; panel help lists nine canonical subcommands; root help has none of the removed seven flat commands.

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerControl
git commit -m "feat(cli): unify terminal panel commands"
```

---

### Task 3: Canonical skill source and worktree-local provisioning

**Files:**
- Move: `skills/tillerctl-cli/SKILL.md` to `skills/tiller/SKILL.md`
- Create: `Packages/TillerAgents/Sources/TillerAgents/TillerSkillProvisioner.swift`
- Delete: `Packages/TillerAgents/Sources/TillerAgents/TillerSkillDocument.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/AgentAdapter.swift`
- Modify: `Packages/TillerAgents/Sources/TillerAgents/ClaudeCodeAdapter.swift`
- Replace: `Packages/TillerAgents/Tests/TillerAgentsTests/TillerSkillDocumentTests.swift` with `Packages/TillerAgents/Tests/TillerAgentsTests/TillerSkillProvisionerTests.swift`
- Modify: `project.yml`
- Create: `App/TillerSkillResource.swift`
- Modify: `App/AppModel.swift` at `restoreAgentSessions` and `spawnAgent`

**Interfaces:**
- Consumes: existing three-argument `AgentAdapter.prepare` implementations for hook setup.
- Produces: `TillerSkillProvisioner.install(markdown:agentID:worktreePath:)` and a four-argument `AgentAdapter.prepare(..., skillMarkdown:)` overload.
- Produces: `TillerSkillResource.markdown`, loaded from the app-bundled canonical file.

- [ ] **Step 1: Write failing provisioner tests around the canonical repository file**

```swift
import Foundation
import Testing
@testable import TillerAgents

private func repositorySkill() throws -> String {
    var url = URL(fileURLWithPath: #filePath)
    for _ in 0..<5 { url.deleteLastPathComponent() }
    return try String(
        contentsOf: url.appendingPathComponent("skills/tiller/SKILL.md"),
        encoding: .utf8
    )
}

@Test func everyAdapterReceivesTheCanonicalSkill() throws {
    let markdown = try repositorySkill()
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    for adapter in AgentCatalog.all {
        let worktree = root.appendingPathComponent(adapter.id, isDirectory: true)
        try FileManager.default.createDirectory(
            at: worktree,
            withIntermediateDirectories: true
        )
        try TillerSkillProvisioner.install(
            markdown: markdown,
            agentID: adapter.id,
            worktreePath: worktree.path
        )
        let installed = try String(
            contentsOf: TillerSkillProvisioner.destination(
                agentID: adapter.id,
                worktreePath: worktree.path
            ),
            encoding: .utf8
        )
        #expect(installed == markdown)
    }
}

@Test func unmanagedSkillIsNeverOverwritten() throws {
    let root = FileManager.default.temporaryDirectory
        .appendingPathComponent(UUID().uuidString, isDirectory: true)
    let destination = root.appendingPathComponent(
        ".agents/skills/tiller/SKILL.md"
    )
    try FileManager.default.createDirectory(
        at: destination.deletingLastPathComponent(),
        withIntermediateDirectories: true
    )
    try "user content".write(to: destination, atomically: true, encoding: .utf8)

    #expect(throws: TillerSkillProvisioner.Error.unmanagedFile(destination.path)) {
        try TillerSkillProvisioner.install(
            markdown: try repositorySkill(),
            agentID: "codex",
            worktreePath: root.path
        )
    }
    #expect(try String(contentsOf: destination, encoding: .utf8) == "user content")
}

@Test func unmarkedInputIsRejected() {
    #expect(throws: TillerSkillProvisioner.Error.missingMarker) {
        try TillerSkillProvisioner.install(
            markdown: "unsafe",
            agentID: "pi",
            worktreePath: "/tmp/worktree"
        )
    }
}
```

Add a second managed write in the first test and assert the destination equals the updated marked content; this proves idempotent replacement.

- [ ] **Step 2: Run the focused tests and confirm the provisioner/canonical path are missing**

Run:

```bash
swift test --package-path Packages/TillerAgents --filter TillerSkillProvisioner
```

Expected: compilation or fixture loading fails because the provisioner and `skills/tiller/SKILL.md` do not exist.

- [ ] **Step 3: Move and rewrite the canonical skill**

The document must begin exactly with:

```markdown
---
name: tiller
description: Use when running inside a Tiller pane to create and manage terminal panels, dispatch worker agents, wait for completion, read output, report status, or leave worktree progress comments through tillerctl.
---
<!-- Machine-managed by Tiller. Do not edit this installed copy. -->
```

Its executable recipes must use only the canonical API:

```bash
[ "$TILLER_ENV" = "1" ] || exit 1
tillerctl ping
tillerctl identify --json

TAB_ID=$(tillerctl panel create --cmd 'codex')
SPLIT_ID=$(tillerctl panel split right --from "$TILLER_PANE_ID" --cmd 'pi')

tillerctl panel write --id "$TAB_ID" --input 'Run the assigned task' --enter
tillerctl panel key --id "$TAB_ID" enter
tillerctl panel read --id "$TAB_ID"
tillerctl panel wait --id "$TAB_ID"
tillerctl panel focus --id "$TAB_ID"
tillerctl panel close --id "$TAB_ID"

tillerctl worktree set --workspace "$TILLER_WORKTREE_ID" --comment 'Implemented and verified'
tillerctl notify --title 'Done' --body 'Worker completed'
```

Explain explicit UUID capture, child exit-code propagation, cleanup on success/failure, and the prohibition on visual-selection inference. Remove every flat pane command example.

- [ ] **Step 4: Implement marker-guarded atomic provisioning**

```swift
import Foundation

public enum TillerSkillProvisioner {
    public static let marker = "<!-- Machine-managed by Tiller. Do not edit this installed copy. -->"

    public enum Error: Swift.Error, Equatable, LocalizedError {
        case missingMarker
        case unsupportedAgent(String)
        case unmanagedFile(String)

        public var errorDescription: String? {
            switch self {
            case .missingMarker:
                "Tiller skill content is missing its managed-file marker"
            case let .unsupportedAgent(id):
                "Unsupported Tiller agent: \(id)"
            case let .unmanagedFile(path):
                "Refusing to overwrite unmanaged skill at \(path)"
            }
        }
    }

    public static func destination(agentID: String, worktreePath: String) throws -> URL {
        let relativePath = switch agentID {
        case "claude": ".claude/skills/tiller/SKILL.md"
        case "codex", "opencode", "pi", "omp": ".agents/skills/tiller/SKILL.md"
        default: throw Error.unsupportedAgent(agentID)
        }
        return URL(fileURLWithPath: worktreePath, isDirectory: true)
            .appendingPathComponent(relativePath)
    }

    public static func install(
        markdown: String,
        agentID: String,
        worktreePath: String
    ) throws {
        guard markdown.contains(marker) else { throw Error.missingMarker }
        let destination = try destination(agentID: agentID, worktreePath: worktreePath)
        let fileManager = FileManager.default
        if fileManager.fileExists(atPath: destination.path) {
            let existing = try String(contentsOf: destination, encoding: .utf8)
            guard existing.contains(marker) else {
                throw Error.unmanagedFile(destination.path)
            }
        }
        try fileManager.createDirectory(
            at: destination.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try markdown.write(to: destination, atomically: true, encoding: .utf8)
    }
}
```

- [ ] **Step 5: Add the preserving adapter overload and remove Claude's duplicate writer**

```swift
public extension AgentAdapter {
    func prepare(
        worktreePath: String,
        paneId: UUID,
        tillerctlPath: String,
        skillMarkdown: String
    ) throws {
        try TillerSkillProvisioner.install(
            markdown: skillMarkdown,
            agentID: id,
            worktreePath: worktreePath
        )
        try prepare(
            worktreePath: worktreePath,
            paneId: paneId,
            tillerctlPath: tillerctlPath
        )
    }
}
```

Keep every adapter's existing three-argument hook preparation unchanged. Delete only ClaudeCodeAdapter's direct `.claude/skills/tiller/SKILL.md` write and delete `TillerSkillDocument.swift`.

- [ ] **Step 6: Bundle and load the one canonical Markdown resource**

Change `project.yml`:

```yaml
sources:
  - App
  - path: skills/tiller/SKILL.md
    buildPhase: resources
```

Create:

```swift
import Foundation

enum TillerSkillResourceError: Error, LocalizedError {
    case missing
    case unreadable(String)

    var errorDescription: String? {
        switch self {
        case .missing: "Bundled Tiller skill is missing"
        case let .unreadable(message): "Bundled Tiller skill is unreadable: \(message)"
        }
    }
}

enum TillerSkillResource {
    static let markdown: Result<String, TillerSkillResourceError> = {
        guard let url = Bundle.main.url(forResource: "SKILL", withExtension: "md") else {
            return .failure(.missing)
        }
        do {
            return .success(try String(contentsOf: url, encoding: .utf8))
        } catch {
            return .failure(.unreadable(error.localizedDescription))
        }
    }()
}
```

At both `restoreAgentSessions` and `spawnAgent`, replace the three-argument preparation call with:

```swift
try adapter.prepare(
    worktreePath: worktree.path,
    paneId: paneId,
    tillerctlPath: hc,
    skillMarkdown: try TillerSkillResource.markdown.get()
)
```

Use each callsite's existing pane/tillerctl local names.

- [ ] **Step 7: Run package tests and verify app resource packaging**

Run:

```bash
swift test --package-path Packages/TillerAgents
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build
test -f DerivedData/Build/Products/Debug/Tiller.app/Contents/Resources/SKILL.md
```

Expected: all TillerAgents tests pass; app build succeeds; the bundled `SKILL.md` exists.

- [ ] **Step 8: Commit**

```bash
git add skills/tiller project.yml App/TillerSkillResource.swift App/AppModel.swift \
  Packages/TillerAgents
git add -u skills/tillerctl-cli
git commit -m "feat(agents): provision canonical tiller skill"
```

---

### Task 4: Canonical server dispatch, deterministic pane creation, and rollback

**Files:**
- Modify: `App/AppModel.swift` at `handleControl`, `split`, and pane helpers
- Modify: `App/AppModel+Control.swift` at `cmuxMethods`, `handleCmuxControl`, and pane-target helpers
- Modify: `Packages/TillerCore/Sources/TillerCore/SplitTree.swift`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/SplitTreeTests.swift`

**Interfaces:**
- Consumes: canonical `panel.*` requests from Tasks 1-2 and `PaneRegistry.shared.isRegistered(paneId:)`.
- Produces: `SplitPlacement`, `split(paneId:axis:placement:newPaneId:) -> Bool`, `waitForPaneRegistration(_:timeout:)`, and canonical `panel.create|split|list|write|key|read|wait|focus|close` dispatch.
- Removes: server dispatch for `surface.list`, `pane.surfaces`, `surface.focus`, `surface.split`, `surface.send_text`, `surface.send_key`, and `surface.close`.

- [ ] **Step 1: Inspect all callers before changing the split APIs**

Run LSP references queries for `AppModel.split(paneId:axis:)` and `SplitTree.splitting(leaf:axis:newLeaf:)`. Preserve every existing caller through default `.after` placement and UUID arguments.

- [ ] **Step 2: Add a failing split-placement contract**

```swift
@Test func splittingCanInsertBeforeOrAfterTheTarget() {
    let target = UUID()
    let inserted = UUID()
    let tree = SplitTree.leaf(id: target)

    #expect(
        tree.splitting(
            leaf: target,
            axis: .horizontal,
            newLeaf: inserted,
            placement: .before
        ) == .split(
            axis: .horizontal,
            first: .leaf(id: inserted),
            second: .leaf(id: target)
        )
    )
    #expect(
        tree.splitting(
            leaf: target,
            axis: .vertical,
            newLeaf: inserted,
            placement: .after
        ) == .split(
            axis: .vertical,
            first: .leaf(id: target),
            second: .leaf(id: inserted)
        )
    )
}
```

Run:

```bash
swift test --package-path Packages/TillerCore --filter splittingCanInsertBeforeOrAfterTheTarget
```

Expected: compilation fails because `SplitPlacement` and the `placement` parameter do not exist.

- [ ] **Step 3: Implement directional placement and server-assigned UUIDs**

```swift
public enum SplitPlacement: Sendable {
    case before
    case after
}

public func splitting(
    leaf target: UUID,
    axis: SplitAxis,
    newLeaf: UUID,
    placement: SplitPlacement = .after
) -> SplitTree {
    switch self {
    case .leaf(let id) where id == target:
        switch placement {
        case .before:
            return .split(axis: axis, first: .leaf(id: newLeaf), second: self)
        case .after:
            return .split(axis: axis, first: self, second: .leaf(id: newLeaf))
        }
    case .leaf:
        return self
    case .split(let currentAxis, let first, let second):
        return .split(
            axis: currentAxis,
            first: first.splitting(
                leaf: target,
                axis: axis,
                newLeaf: newLeaf,
                placement: placement
            ),
            second: second.splitting(
                leaf: target,
                axis: axis,
                newLeaf: newLeaf,
                placement: placement
            )
        )
    }
}

@discardableResult
func split(
    paneId: UUID,
    axis: SplitAxis,
    placement: SplitPlacement = .after,
    newPaneId: UUID = UUID()
) -> Bool {
    guard let tuple = tabContaining(paneId: paneId),
          let tree = tuple.tab.terminalTree else { return false }
    tabs[tuple.worktree.id]?[tuple.index].content = .terminal(
        tree.splitting(
            leaf: paneId,
            axis: axis,
            newLeaf: newPaneId,
            placement: placement
        )
    )
    persistTabs(for: tuple.worktree.id)
    return true
}
```

- [ ] **Step 4: Add bounded registration and mount helpers**

```swift
private func waitForPaneRegistration(
    _ paneId: UUID,
    timeout: Duration = .seconds(5)
) async -> Bool {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: timeout)
    while clock.now < deadline {
        if await PaneRegistry.shared.isRegistered(paneId: paneId) { return true }
        try? await Task.sleep(for: .milliseconds(25))
    }
    return await PaneRegistry.shared.isRegistered(paneId: paneId)
}

private func mountForControl(_ worktree: Worktree) -> Bool {
    let wasOpen = openWorktreeIds.contains(worktree.id)
    if !wasOpen { openWorktreeIds.append(worktree.id) }
    return wasOpen
}

private func restoreControlMount(_ worktree: Worktree, wasOpen: Bool) {
    guard !wasOpen else { return }
    openWorktreeIds.removeAll { $0 == worktree.id }
}
```

- [ ] **Step 5: Replace `panel.create` with registered, rollback-safe creation**

```swift
case "panel.create":
    guard let selector = request.params["worktree"],
          let worktree = resolveWorktree(selector) else {
        return .failure(id: request.id, error: "unknown worktree")
    }
    let paneId = UUID()
    if let cmd = request.params["cmd"] { paneCommands[paneId] = cmd }
    let wasOpen = mountForControl(worktree)
    let tab = openTab(
        paneId: paneId,
        title: request.params["cmd"]?
            .split(separator: " ").first.map(String.init) ?? "Panel",
        in: worktree
    )
    guard await waitForPaneRegistration(paneId) else {
        paneCommands.removeValue(forKey: paneId)
        closeTab(tab.id, in: worktree)
        restoreControlMount(worktree, wasOpen: wasOpen)
        return .failure(id: request.id, error: "panel registration timed out")
    }
    return .success(id: request.id, result: ["id": paneId.uuidString])
```

Do not call `NSApp.activate`, set `selectedWorktree`, or use active UI state in this branch.

- [ ] **Step 6: Add registered, rollback-safe split dispatch**

```swift
case "panel.split":
    guard let rawSource = request.params["from"],
          let source = UUID(uuidString: rawSource),
          let owner = tabContaining(paneId: source) else {
        return .failure(id: request.id, error: "unknown source pane")
    }
    let splitSpec: (axis: SplitAxis, placement: SplitPlacement)
    switch request.params["direction"] {
    case "left": splitSpec = (.horizontal, .before)
    case "right": splitSpec = (.horizontal, .after)
    case "up": splitSpec = (.vertical, .before)
    case "down": splitSpec = (.vertical, .after)
    default: return .failure(id: request.id, error: "invalid split direction")
    }
    let paneId = UUID()
    if let cmd = request.params["cmd"] { paneCommands[paneId] = cmd }
    let wasOpen = mountForControl(owner.worktree)
    guard split(
        paneId: source,
        axis: splitSpec.axis,
        placement: splitSpec.placement,
        newPaneId: paneId
    ) else {
        paneCommands.removeValue(forKey: paneId)
        restoreControlMount(owner.worktree, wasOpen: wasOpen)
        return .failure(id: request.id, error: "split failed")
    }
    guard await waitForPaneRegistration(paneId) else {
        paneCommands.removeValue(forKey: paneId)
        closeTerminal(paneId: paneId)
        restoreControlMount(owner.worktree, wasOpen: wasOpen)
        return .failure(id: request.id, error: "panel registration timed out")
    }
    return .success(id: request.id, result: ["id": paneId.uuidString])
```


- [ ] **Step 7: Add the remaining canonical server branches**

```swift
case "panel.list":
    guard let selector = request.params["worktree"],
          let worktree = resolveWorktree(selector) else {
        return .failure(id: request.id, error: "unknown worktree")
    }
    let rows = ControlListing.paneRows(
        tabs: tabs[worktree.id] ?? [],
        activeTabId: activeTabId[worktree.id],
        agentIdForPane: { self.agentActivity.paneAgents[$0] },
        titleForPane: { self.paneTitles[$0] }
    )
    return .success(id: request.id, result: ["panels": ControlRows.encode(rows)])

case "panel.key":
    guard let rawId = request.params["id"],
          let id = UUID(uuidString: rawId),
          let rawKey = request.params["key"],
          let key = TerminalKey(rawValue: rawKey),
          await PaneRegistry.shared.write(paneId: id, data: key.bytes) else {
        return .failure(id: request.id, error: "unknown pane or key")
    }
    return .success(id: request.id)

case "panel.focus":
    guard let rawId = request.params["id"],
          let id = UUID(uuidString: rawId),
          let tuple = tabContaining(paneId: id) else {
        return .failure(id: request.id, error: "unknown pane")
    }
    selectedWorktree = tuple.worktree
    activateTab(tuple.tab.id, in: tuple.worktree.id)
    NSApp.activate(ignoringOtherApps: true)
    NSApp.windows.first?.makeKeyAndOrderFront(nil)
    return .success(id: request.id)

case "panel.close":
    guard let rawId = request.params["id"],
          let id = UUID(uuidString: rawId),
          tabContaining(paneId: id) != nil else {
        return .failure(id: request.id, error: "unknown pane")
    }
    closeTerminal(paneId: id)
    return .success(id: request.id)
```

Retain the existing `panel.write`, `panel.read`, and `panel.wait` behavior, changing only result keys/error consistency required by the canonical contract.

- [ ] **Step 8: Remove redundant cmux pane dispatch and update capabilities**

`cmuxMethods` must contain:

```swift
"panel.create", "panel.split", "panel.list", "panel.write", "panel.key",
"panel.read", "panel.wait", "panel.focus", "panel.close"
```

Remove all seven `surface.*`/`pane.surfaces` cases and delete `resolveTargetPane`/`activePaneId` only after LSP references show no callers. Keep workspace, notification, system, and session restore cases byte-for-byte unless compilation requires a moved brace.

- [ ] **Step 9: Build the app and run split-tree tests**

Run:

```bash
swift test --package-path Packages/TillerCore --filter SplitTree
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build
```

Expected: `** BUILD SUCCEEDED **`.

- [ ] **Step 10: Commit**

```bash
git add App/AppModel.swift App/AppModel+Control.swift \
  Packages/TillerCore/Sources/TillerCore/SplitTree.swift \
  Packages/TillerCore/Tests/TillerCoreTests/SplitTreeTests.swift
git commit -m "feat(app): implement canonical panel lifecycle"
```

---

### Task 5: Settings installer, user documentation, and migration cleanup

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AgentSkillInstall.swift`
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AgentSkillInstallTests.swift`
- Modify: `App/Settings/GeneralSettingsView.swift`
- Modify: `App/AgentSkillInstaller.swift`
- Modify: `README.md`
- Verify deletion: `skills/tillerctl-cli/`
- Verify deletion: `Packages/TillerAgents/Sources/TillerAgents/TillerSkillDocument.swift`

**Interfaces:**
- Consumes: canonical `skills/tiller` package name and command surface.
- Produces: Settings installer command for Claude Code, Codex, OpenCode, and Pi; OMP uses the same `.agents/skills` worktree provisioning.

- [ ] **Step 1: Update the failing installer expectation first**

```swift
@Test func installCommandTargetsCanonicalTillerSkill() {
    #expect(
        AgentSkillInstall.command
            == "npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y"
    )
}
```

- [ ] **Step 2: Run the focused test and confirm it sees the old skill name**

Run:

```bash
swift test --package-path Packages/TillerCore --filter installCommandTargetsCanonicalTillerSkill
```

Expected: failure showing `tillerctl-cli` and the old agent list.

- [ ] **Step 3: Update installer command and UI copy**

```swift
public enum AgentSkillInstall {
    public static let command =
        "npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y"
}
```

Settings copy must say the skill is installed automatically inside launched worktrees for all five Tiller harnesses; the button installs the public package for supported Skills CLI agents. Update `AgentSkillInstaller` comments to match the canonical name without changing its Terminal-launch mechanism.

- [ ] **Step 4: Replace README orchestration examples**

Use these canonical recipes:

```bash
WORKER=$(tillerctl panel create --cmd 'claude')
PEER=$(tillerctl panel split right --from "$TILLER_PANE_ID" --cmd 'codex')

tillerctl panel write --id "$WORKER" --input 'Implement the parser change' --enter
tillerctl panel wait --id "$WORKER"
tillerctl panel read --id "$WORKER"
tillerctl panel close --id "$WORKER"
```

Document `skills/tiller/SKILL.md`, worktree-local automatic provisioning, explicit UUID addressing, and the nine panel subcommands. Remove every flat pane example and old `tillerctl-cli` skill name.

- [ ] **Step 5: Run focused package tests and migration searches**

Run:

```bash
swift test --package-path Packages/TillerCore
swift test --package-path Packages/TillerAgents
```

Then search the repository for these removed names and require zero production/documentation matches:

```text
tillerctl-cli
new-split
list-panels
list-pane-surfaces
focus-panel
send-key
close-panel
surface.list
surface.split
surface.send_text
surface.send_key
surface.close
pane.surfaces
TillerSkillDocument
```

Test names that explicitly assert removal may mention old command strings; no executable registration, request builder, server dispatch, README recipe, or skill recipe may remain.

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerCore App/Settings/GeneralSettingsView.swift \
  App/AgentSkillInstaller.swift README.md
git add -u
git commit -m "docs: migrate orchestration to panel api"
```

---

### Task 6: Live control-socket smoke and final repository gate

**Files:**
- Modify only files required by defects exposed in this task.

**Interfaces:**
- Consumes: built app, bundled CLI, canonical skill, and live Tiller socket.
- Produces: observed end-to-end proof for tab creation, split creation, explicit targeting, child exit propagation, cleanup, and focus behavior.

- [ ] **Step 1: Build and launch the changed app in the isolated worktree**

Run the app from `DerivedData/Build/Products/Debug/Tiller.app`, keep a different worktree selected, and use the CLI embedded in that same app bundle so client and server versions match.

- [ ] **Step 2: Exercise new-tab creation without UI selection dependence**

```bash
CTL="DerivedData/Build/Products/Debug/Tiller.app/Contents/MacOS/tillerctl"
WORKTREE_ID="EE254107-CBA9-475C-9FE0-F09EFB1BD545"
TAB_ID=$($CTL panel create --worktree "$WORKTREE_ID" --cmd "printf tab-ready; exit 0")
$CTL panel read --id "$TAB_ID"
$CTL panel wait --id "$TAB_ID"
$CTL panel close --id "$TAB_ID"
```

Expected: `TAB_ID` is a UUID; read contains `tab-ready`; wait exits 0; the temporary tab closes; the previously selected worktree remains selected.

- [ ] **Step 3: Exercise split creation and explicit management**

Use an existing pane UUID from `panel list`, then run:

```bash
SOURCE_ID=$($CTL panel list --worktree "$WORKTREE_ID" --json \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["id"])')
SPLIT_ID=$($CTL panel split right --from "$SOURCE_ID" --cmd "printf split-ready; exit 0")
$CTL panel read --id "$SPLIT_ID"
$CTL panel wait --id "$SPLIT_ID"
$CTL panel close --id "$SPLIT_ID"
```

Expected: `SPLIT_ID` differs from `SOURCE_ID`; read contains `split-ready`; wait exits 0; only the created split closes.

- [ ] **Step 4: Prove failure and focus semantics**

```bash
$CTL panel read --id 00000000-0000-0000-0000-000000000000
$CTL panel split diagonal --from "$SOURCE_ID"
env -u TILLER_WORKTREE_ID $CTL panel create
```

Expected: each command exits nonzero with a specific diagnostic. Create a live pane while Tiller is not foreground and confirm it stays backgrounded; invoke `panel focus --id <uuid>` and confirm only that command activates Tiller and selects the owning tab/worktree.

- [ ] **Step 5: Run the final gate**

Run:

```bash
Scripts/ci.sh
```

Expected final line: `CI OK`.

- [ ] **Step 6: Commit smoke-discovered fixes, if any**

Use one Conventional Commit per coherent defect, then rerun the focused reproduction and `Scripts/ci.sh`. If the smoke reveals no defect, create no empty commit.
