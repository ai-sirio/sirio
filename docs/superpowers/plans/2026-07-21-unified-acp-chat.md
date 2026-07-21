# Unified ACP Chat + Agent Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every chat agent-agnostic (agent picked/switched from a selector in the chat, context carried over on switch) and add a Settings "Agents" tab that installs ACP agents from the official ACP registry.

**Architecture:** All non-UI logic lands in `Packages/TillerACP` (registry models, installer, handoff serializer, session-store changes); `App/` gets the selector UI, the Settings tab, and an `@Observable` install center. `AgentLaunchSpec.forAgent` hardcoding is replaced by resolution from installed manifests + a built-in omp entry.

**Tech Stack:** Swift 6, swift-testing (`@Test`/`#expect`), GRDB (existing `AppDatabase`), SwiftUI. Registry: `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`.

**Spec:** `docs/superpowers/specs/2026-07-21-unified-acp-chat-design.md`

## Global Constraints

- UI strings always English.
- Value types for models; classes only for real identity, actor-isolated.
- Tests first (swift-testing, not XCTest). No network and no real npm in tests.
- `TillerACP` must not import SwiftUI/AppKit.
- Install root: `~/Library/Application Support/Tiller/acp-agents/<id>/`. Registry cache: `~/Library/Application Support/Tiller/acp-registry.json`. Never touch the user's global environment (no `npm install -g`).
- Registry ids are canonical (`claude-acp`, `codex-acp`, `opencode`, `pi-acp`); legacy Tiller ids (`claude`, `codex`, `pi`) must keep working via mapping. `omp` is a built-in (not in the registry), launched as `zsh -lc "exec omp acp"`.
- Handoff preamble size cap: 100 KB (drop oldest first, truncation note).
- Conventional Commits, lower-case imperative. `Scripts/ci.sh` must print `CI OK` before the branch is done.
- After adding files to the App target: `xcodegen generate`. Package-only files need no xcodegen.

---

### Task 1: Registry models + decoding

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentRegistryModels.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentRegistryModelsTests.swift`

**Interfaces:**
- Produces: `ACPRegistry` (`version: String`, `agents: [RegistryAgent]`), `RegistryAgent` (`id/name/version: String`, `description: String?`, `icon: URL?`, `distribution: AgentDistribution`), `AgentDistribution` (`npx: Npx?`, `binary: [String: BinaryPlatform]?`, `hasUvx: Bool`) with `Npx` (`package: String`, `args: [String]?`, `env: [String: String]?`) and `BinaryPlatform` (`archive: URL`, `cmd: String`). All `Sendable, Equatable, Decodable`.

- [ ] **Step 1: Write the failing test**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentRegistryModelsTests {
    static let fixture = Data("""
    {
      "version": "1.0.0",
      "agents": [
        {"id": "claude-acp", "name": "Claude Agent", "version": "0.60.0",
         "description": "ACP wrapper for Anthropic's Claude",
         "icon": "https://cdn.agentclientprotocol.com/registry/v1/latest/claude-acp.svg",
         "distribution": {"npx": {"package": "@agentclientprotocol/claude-agent-acp@0.60.0"}}},
        {"id": "auggie", "name": "Auggie CLI", "version": "0.33.0",
         "distribution": {"npx": {"package": "@augmentcode/auggie@0.33.0",
                                  "args": ["--acp"],
                                  "env": {"AUGMENT_DISABLE_AUTO_UPDATE": "1"}}}},
        {"id": "amp-acp", "name": "Amp", "version": "0.8.1",
         "distribution": {"binary": {
            "darwin-aarch64": {"archive": "https://example.com/amp-arm.tar.gz", "cmd": "./amp-acp"},
            "darwin-x86_64": {"archive": "https://example.com/amp-x64.tar.gz", "cmd": "./amp-acp"}}}},
        {"id": "fast-agent", "name": "fast-agent", "version": "0.9.19",
         "distribution": {"uvx": {"package": "fast-agent-mcp@0.9.19"}}}
      ]
    }
    """.utf8)

    @Test func decodesRealRegistryShape() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        #expect(registry.agents.count == 4)
        let claude = registry.agents[0]
        #expect(claude.id == "claude-acp")
        #expect(claude.distribution.npx?.package == "@agentclientprotocol/claude-agent-acp@0.60.0")
        #expect(claude.distribution.npx?.args == nil)
        #expect(claude.icon != nil)
    }

    @Test func decodesNpxArgsAndEnv() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let auggie = registry.agents[1]
        #expect(auggie.distribution.npx?.args == ["--acp"])
        #expect(auggie.distribution.npx?.env == ["AUGMENT_DISABLE_AUTO_UPDATE": "1"])
    }

    @Test func decodesBinaryPlatformMap() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let amp = registry.agents[2]
        #expect(amp.distribution.binary?["darwin-aarch64"]?.cmd == "./amp-acp")
        #expect(amp.distribution.binary?["darwin-aarch64"]?.archive
                == URL(string: "https://example.com/amp-arm.tar.gz"))
    }

    @Test func uvxOnlyAgentIsFlagged() throws {
        let registry = try JSONDecoder().decode(ACPRegistry.self, from: Self.fixture)
        let fast = registry.agents[3]
        #expect(fast.distribution.hasUvx)
        #expect(fast.distribution.npx == nil)
        #expect(fast.distribution.binary == nil)
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter AgentRegistryModelsTests`
Expected: FAIL — `cannot find 'ACPRegistry'`.

- [ ] **Step 3: Implement the models**

```swift
import Foundation

/// The official ACP agent registry document
/// (https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json).
public struct ACPRegistry: Sendable, Equatable, Decodable {
    public var version: String
    public var agents: [RegistryAgent]
}

public struct RegistryAgent: Sendable, Equatable, Decodable, Identifiable {
    public var id: String
    public var name: String
    public var version: String
    public var description: String?
    public var icon: URL?
    public var distribution: AgentDistribution
}

/// The `distribution` object of a registry entry. `uvx` is parsed only as a
/// presence flag: it is unsupported in v1 and shown as such in Settings.
public struct AgentDistribution: Sendable, Equatable, Decodable {
    public struct Npx: Sendable, Equatable, Decodable {
        public var package: String
        public var args: [String]?
        public var env: [String: String]?
    }

    public struct BinaryPlatform: Sendable, Equatable, Decodable {
        public var archive: URL
        public var cmd: String
    }

    public var npx: Npx?
    public var binary: [String: BinaryPlatform]?
    public var hasUvx: Bool

    private enum CodingKeys: String, CodingKey { case npx, binary, uvx }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        npx = try container.decodeIfPresent(Npx.self, forKey: .npx)
        binary = try container.decodeIfPresent(
            [String: BinaryPlatform].self, forKey: .binary)
        hasUvx = container.contains(.uvx)
    }

    public init(npx: Npx? = nil, binary: [String: BinaryPlatform]? = nil,
                hasUvx: Bool = false) {
        self.npx = npx
        self.binary = binary
        self.hasUvx = hasUvx
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test --filter AgentRegistryModelsTests`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: decode ACP agent registry models"
```

---

### Task 2: Install-method resolution, manifest, install store, status

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentInstallStore.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentInstallStoreTests.swift`

**Interfaces:**
- Consumes: `RegistryAgent`, `AgentDistribution` (Task 1).
- Produces:
  - `HostPlatform` enum (`darwinArm64 = "darwin-aarch64"`, `darwinX86 = "darwin-x86_64"`, `static var current`).
  - `InstallMethod` enum: `.npx(package: String, args: [String], env: [String: String])`, `.binary(archive: URL, cmd: String)`.
  - `RegistryAgent.installMethod(platform:) -> InstallMethod?`.
  - `InstalledAgentManifest` (`id/version/executable: String`, `arguments: [String]`, `environment: [String: String]`, `Codable`).
  - `AgentInstallStore` struct: `init(rootDirectory: URL)`, `agentDirectory(id:) -> URL`, `manifest(id:) -> InstalledAgentManifest?`, `installedManifests() -> [InstalledAgentManifest]`, `write(_:) throws`.
  - `AgentInstallStatus` enum: `.notInstalled`, `.installed(version: String)`, `.updateAvailable(installed: String, latest: String)`, `.unsupported`; `static func resolve(manifest: InstalledAgentManifest?, latestVersion: String?, isSupported: Bool) -> AgentInstallStatus`.

- [ ] **Step 1: Write the failing tests**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentInstallStoreTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("acp-agents-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    @Test func npxDistributionResolvesRegardlessOfPlatform() {
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(
                npx: .init(package: "pkg@1.0.0", args: ["--acp"], env: ["K": "V"])))
        #expect(agent.installMethod(platform: .darwinArm64)
                == .npx(package: "pkg@1.0.0", args: ["--acp"], env: ["K": "V"]))
    }

    @Test func binaryDistributionPicksCurrentPlatform() {
        let arm = URL(string: "https://example.com/arm.tar.gz")!
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(binary: [
                "darwin-aarch64": .init(archive: arm, cmd: "./x")]))
        #expect(agent.installMethod(platform: .darwinArm64)
                == .binary(archive: arm, cmd: "./x"))
        #expect(agent.installMethod(platform: .darwinX86) == nil)
    }

    @Test func uvxOnlyAgentHasNoInstallMethod() {
        let agent = RegistryAgent(
            id: "x", name: "X", version: "1.0.0", description: nil, icon: nil,
            distribution: AgentDistribution(hasUvx: true))
        #expect(agent.installMethod(platform: .darwinArm64) == nil)
    }

    @Test func manifestRoundTripsThroughStore() throws {
        let store = try tempStore()
        let manifest = InstalledAgentManifest(
            id: "claude-acp", version: "0.60.0",
            executable: "/tmp/bin/claude-agent-acp", arguments: [],
            environment: [:])
        try store.write(manifest)
        #expect(store.manifest(id: "claude-acp") == manifest)
        #expect(store.installedManifests() == [manifest])
    }

    @Test func missingManifestReadsAsNil() throws {
        let store = try tempStore()
        #expect(store.manifest(id: "nope") == nil)
        #expect(store.installedManifests().isEmpty)
    }

    @Test func statusResolution() {
        let installed = InstalledAgentManifest(
            id: "a", version: "1.0.0", executable: "/x", arguments: [], environment: [:])
        #expect(AgentInstallStatus.resolve(manifest: nil, latestVersion: "1.0.0",
                                           isSupported: false) == .unsupported)
        #expect(AgentInstallStatus.resolve(manifest: nil, latestVersion: "1.0.0",
                                           isSupported: true) == .notInstalled)
        #expect(AgentInstallStatus.resolve(manifest: installed, latestVersion: "1.0.0",
                                           isSupported: true) == .installed(version: "1.0.0"))
        #expect(AgentInstallStatus.resolve(manifest: installed, latestVersion: "2.0.0",
                                           isSupported: true)
                == .updateAvailable(installed: "1.0.0", latest: "2.0.0"))
    }
}
```

Note: this test constructs `RegistryAgent`/`AgentDistribution` directly — add a public memberwise `init` to `RegistryAgent` in this task (Decodable synthesis alone doesn't give one publicly).

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter AgentInstallStoreTests`
Expected: FAIL — `cannot find 'AgentInstallStore'`.

- [ ] **Step 3: Implement**

```swift
import Foundation

/// Registry `binary` distribution platform keys for the current host.
public enum HostPlatform: String, Sendable {
    case darwinArm64 = "darwin-aarch64"
    case darwinX86 = "darwin-x86_64"

    public static var current: HostPlatform {
        #if arch(arm64)
        .darwinArm64
        #else
        .darwinX86
        #endif
    }
}

/// Concrete way to install one registry agent on this machine.
public enum InstallMethod: Sendable, Equatable {
    case npx(package: String, args: [String], env: [String: String])
    case binary(archive: URL, cmd: String)
}

extension RegistryAgent {
    /// npx wins when both exist (no archive download needed). Nil when the
    /// agent is uvx-only or has no archive for this platform.
    public func installMethod(platform: HostPlatform) -> InstallMethod? {
        if let npx = distribution.npx {
            return .npx(package: npx.package, args: npx.args ?? [],
                        env: npx.env ?? [:])
        }
        if let entry = distribution.binary?[platform.rawValue] {
            return .binary(archive: entry.archive, cmd: entry.cmd)
        }
        return nil
    }
}

/// `installed.json` written after a successful install; single source of
/// truth for "installed" state and for launching.
public struct InstalledAgentManifest: Sendable, Equatable, Codable {
    public var id: String
    public var version: String
    public var executable: String
    public var arguments: [String]
    public var environment: [String: String]

    public init(id: String, version: String, executable: String,
                arguments: [String], environment: [String: String]) {
        self.id = id
        self.version = version
        self.executable = executable
        self.arguments = arguments
        self.environment = environment
    }
}

/// Filesystem layout of Tiller-managed agent installs:
/// `<root>/<id>/installed.json` + the install payload next to it.
public struct AgentInstallStore: Sendable {
    public let rootDirectory: URL

    public init(rootDirectory: URL) {
        self.rootDirectory = rootDirectory
    }

    public func agentDirectory(id: String) -> URL {
        rootDirectory.appendingPathComponent(id, isDirectory: true)
    }

    public func manifestURL(id: String) -> URL {
        agentDirectory(id: id).appendingPathComponent("installed.json")
    }

    public func manifest(id: String) -> InstalledAgentManifest? {
        guard let data = try? Data(contentsOf: manifestURL(id: id)) else { return nil }
        return try? JSONDecoder().decode(InstalledAgentManifest.self, from: data)
    }

    public func installedManifests() -> [InstalledAgentManifest] {
        let contents = (try? FileManager.default.contentsOfDirectory(
            at: rootDirectory, includingPropertiesForKeys: nil)) ?? []
        return contents
            .compactMap { manifest(id: $0.lastPathComponent) }
            .sorted { $0.id < $1.id }
    }

    public func write(_ manifest: InstalledAgentManifest) throws {
        let dir = agentDirectory(id: manifest.id)
        try FileManager.default.createDirectory(
            at: dir, withIntermediateDirectories: true)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(manifest).write(to: manifestURL(id: manifest.id),
                                           options: .atomic)
    }
}

/// Install state shown in Settings, derived — never stored.
public enum AgentInstallStatus: Sendable, Equatable {
    case notInstalled
    case installed(version: String)
    case updateAvailable(installed: String, latest: String)
    case unsupported

    public static func resolve(manifest: InstalledAgentManifest?,
                               latestVersion: String?,
                               isSupported: Bool) -> AgentInstallStatus {
        guard let manifest else {
            return isSupported ? .notInstalled : .unsupported
        }
        if let latestVersion, latestVersion != manifest.version {
            return .updateAvailable(installed: manifest.version, latest: latestVersion)
        }
        return .installed(version: manifest.version)
    }
}
```

Also add to `RegistryAgent` (Task 1 file):

```swift
    public init(id: String, name: String, version: String,
                description: String?, icon: URL?,
                distribution: AgentDistribution) {
        self.id = id
        self.name = name
        self.version = version
        self.description = description
        self.icon = icon
        self.distribution = distribution
    }
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test --filter AgentInstallStoreTests`
Expected: 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: agent install store, manifest, and install-method resolution"
```

---

### Task 3: Registry client with disk cache

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentRegistryClient.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentRegistryClientTests.swift`

**Interfaces:**
- Consumes: `ACPRegistry` (Task 1).
- Produces: `actor AgentRegistryClient` — `static let defaultURL: URL`; `init(cacheURL: URL, fetch: @escaping @Sendable () async throws -> Data, now: @escaping @Sendable () -> Date = { Date() })`; `func registry(maxAge: TimeInterval = 86_400, forceRefresh: Bool = false) async throws -> ACPRegistry`; `func lastFetchedAt() -> Date?`.

- [ ] **Step 1: Write the failing tests**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentRegistryClientTests {
    static let registryJSON = Data("""
    {"version": "1.0.0", "agents": [
      {"id": "claude-acp", "name": "Claude Agent", "version": "0.60.0",
       "distribution": {"npx": {"package": "p@0.60.0"}}}]}
    """.utf8)

    func tempCacheURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("registry-\(UUID().uuidString).json")
    }

    @Test func fetchesAndCachesOnFirstCall() async throws {
        let cache = tempCacheURL()
        let client = AgentRegistryClient(cacheURL: cache,
                                         fetch: { Self.registryJSON })
        let registry = try await client.registry()
        #expect(registry.agents.first?.id == "claude-acp")
        #expect(FileManager.default.fileExists(atPath: cache.path))
    }

    @Test func freshCacheSkipsNetwork() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { throw URLError(.notConnectedToInternet) })
        let registry = try await client.registry()  // must not hit fetch
        #expect(registry.agents.count == 1)
    }

    @Test func forceRefreshBypassesFreshCache() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { Data("""
                {"version": "1.0.1", "agents": []}
                """.utf8) })
        let registry = try await client.registry(forceRefresh: true)
        #expect(registry.version == "1.0.1")
    }

    @Test func fetchFailureFallsBackToStaleCache() async throws {
        let cache = tempCacheURL()
        try Self.registryJSON.write(to: cache)
        let past = Date(timeIntervalSinceNow: -200_000)  // cache older than TTL
        try FileManager.default.setAttributes(
            [.modificationDate: past], ofItemAtPath: cache.path)
        let client = AgentRegistryClient(
            cacheURL: cache,
            fetch: { throw URLError(.notConnectedToInternet) })
        let registry = try await client.registry()
        #expect(registry.agents.count == 1)
    }

    @Test func fetchFailureWithoutCacheThrows() async {
        let client = AgentRegistryClient(
            cacheURL: tempCacheURL(),
            fetch: { throw URLError(.notConnectedToInternet) })
        await #expect(throws: (any Error).self) {
            _ = try await client.registry()
        }
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter AgentRegistryClientTests`
Expected: FAIL — `cannot find 'AgentRegistryClient'`.

- [ ] **Step 3: Implement**

```swift
import Foundation

/// Fetches the official ACP registry with a disk cache: fresh cache (mtime
/// within maxAge) is served without touching the network; a failed fetch
/// falls back to any cached copy; decoding is validated before the cache is
/// overwritten (a bad payload never clobbers a good cache).
public actor AgentRegistryClient {
    public static let defaultURL = URL(
        string: "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json")!

    private let cacheURL: URL
    private let fetch: @Sendable () async throws -> Data
    private let now: @Sendable () -> Date

    public init(cacheURL: URL,
                fetch: @escaping @Sendable () async throws -> Data,
                now: @escaping @Sendable () -> Date = { Date() }) {
        self.cacheURL = cacheURL
        self.fetch = fetch
        self.now = now
    }

    /// Convenience production initializer.
    public init(cacheURL: URL, session: URLSession = .shared) {
        self.init(cacheURL: cacheURL, fetch: {
            let (data, _) = try await session.data(from: Self.defaultURL)
            return data
        })
    }

    public func registry(maxAge: TimeInterval = 86_400,
                         forceRefresh: Bool = false) async throws -> ACPRegistry {
        if !forceRefresh, let cached = cachedRegistry(),
           let fetchedAt = lastFetchedAt(),
           now().timeIntervalSince(fetchedAt) < maxAge {
            return cached
        }
        do {
            let data = try await fetch()
            let registry = try JSONDecoder().decode(ACPRegistry.self, from: data)
            try? data.write(to: cacheURL, options: .atomic)
            return registry
        } catch {
            if let cached = cachedRegistry() { return cached }
            throw error
        }
    }

    public func lastFetchedAt() -> Date? {
        (try? FileManager.default.attributesOfItem(
            atPath: cacheURL.path))?[.modificationDate] as? Date
    }

    private func cachedRegistry() -> ACPRegistry? {
        guard let data = try? Data(contentsOf: cacheURL) else { return nil }
        return try? JSONDecoder().decode(ACPRegistry.self, from: data)
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test --filter AgentRegistryClientTests`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: ACP registry client with disk cache and offline fallback"
```

---

### Task 4: Agent installer (npx + binary, atomic update)

**Files:**
- Create: `Packages/TillerACP/Sources/TillerACP/AgentInstaller.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentInstallerTests.swift`

**Interfaces:**
- Consumes: `RegistryAgent.installMethod(platform:)`, `AgentInstallStore`, `InstalledAgentManifest` (Task 2).
- Produces:
  - `protocol ShellRunning: Sendable { func run(_ command: String, cwd: URL) async throws -> ShellResult }` with `ShellResult` (`exitCode: Int32`, `output: String`).
  - `ZshRunner: ShellRunning` (production, `zsh -lc` via `Process`).
  - `actor AgentInstaller`: `init(store: AgentInstallStore, shell: ShellRunning, download: @escaping @Sendable (URL, URL) async throws -> Void)`; `func install(_ agent: RegistryAgent, platform: HostPlatform = .current) async throws -> InstalledAgentManifest`.
  - `enum AgentInstallError: Error, Equatable`: `.unsupported`, `.commandFailed(output: String)`, `.noExecutableFound`.

- [ ] **Step 1: Write the failing tests**

Tests fake the shell (records commands, creates the files npm/tar would create) and the downloader (writes a fixture archive path marker). No network, no npm.

```swift
import Foundation
import Testing
@testable import TillerACP

/// Shell fake: records commands and simulates their filesystem effects.
final class FakeShell: ShellRunning, @unchecked Sendable {
    var commands: [String] = []
    var effect: (@Sendable (String, URL) throws -> Void)?
    var exitCode: Int32 = 0

    func run(_ command: String, cwd: URL) async throws -> ShellResult {
        commands.append(command)
        try effect?(command, cwd)
        return ShellResult(exitCode: exitCode, output: "fake")
    }
}

@Suite struct AgentInstallerTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("installer-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    func npxAgent() -> RegistryAgent {
        RegistryAgent(id: "claude-acp", name: "Claude Agent", version: "0.60.0",
                      description: nil, icon: nil,
                      distribution: AgentDistribution(
                        npx: .init(package: "@agentclientprotocol/claude-agent-acp@0.60.0",
                                   args: nil, env: ["K": "V"])))
    }

    @Test func npxInstallWritesManifestPointingIntoBin() async throws {
        let store = try tempStore()
        let shell = FakeShell()
        shell.effect = { command, _ in
            // Simulate `npm install --prefix <staging> pkg` creating .bin/.
            guard command.hasPrefix("npm install --prefix ") else { return }
            let staging = URL(fileURLWithPath: String(
                command.dropFirst("npm install --prefix ".count)
                    .split(separator: " ", maxSplits: 1)[0]))
            let bin = staging.appendingPathComponent("node_modules/.bin")
            try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
            FileManager.default.createFile(
                atPath: bin.appendingPathComponent("claude-agent-acp").path, contents: Data())
        }
        let installer = AgentInstaller(store: store, shell: shell,
                                       download: { _, _ in Issue.record("no download for npx") })
        let manifest = try await installer.install(npxAgent(), platform: .darwinArm64)
        #expect(manifest.version == "0.60.0")
        #expect(manifest.executable
                == store.agentDirectory(id: "claude-acp")
                    .appendingPathComponent("node_modules/.bin/claude-agent-acp").path)
        #expect(manifest.environment == ["K": "V"])
        #expect(store.manifest(id: "claude-acp") == manifest)
        #expect(FileManager.default.fileExists(atPath: manifest.executable))
    }

    @Test func npmFailureThrowsAndLeavesNothingInstalled() async throws {
        let store = try tempStore()
        let shell = FakeShell()
        shell.exitCode = 1
        let installer = AgentInstaller(store: store, shell: shell, download: { _, _ in })
        await #expect(throws: (any Error).self) {
            _ = try await installer.install(npxAgent(), platform: .darwinArm64)
        }
        #expect(store.manifest(id: "claude-acp") == nil)
    }

    @Test func binaryInstallExtractsAndChmods() async throws {
        let store = try tempStore()
        let agent = RegistryAgent(
            id: "amp-acp", name: "Amp", version: "0.8.1", description: nil, icon: nil,
            distribution: AgentDistribution(binary: [
                "darwin-aarch64": .init(
                    archive: URL(string: "https://example.com/amp.tar.gz")!,
                    cmd: "./amp-acp")]))
        let shell = FakeShell()
        shell.effect = { command, _ in
            // Simulate `tar -xzf <archive> -C <staging>` producing the binary.
            guard command.hasPrefix("tar ") || command.hasPrefix("chmod ") else { return }
            if command.hasPrefix("tar ") {
                let staging = URL(fileURLWithPath:
                    String(command.split(separator: " ").last!))
                FileManager.default.createFile(
                    atPath: staging.appendingPathComponent("amp-acp").path, contents: Data())
            }
        }
        let installer = AgentInstaller(store: store, shell: shell,
                                       download: { _, destination in
            FileManager.default.createFile(atPath: destination.path, contents: Data())
        })
        let manifest = try await installer.install(agent, platform: .darwinArm64)
        #expect(manifest.executable
                == store.agentDirectory(id: "amp-acp").appendingPathComponent("amp-acp").path)
        #expect(shell.commands.contains { $0.hasPrefix("chmod +x ") })
    }

    @Test func updateReplacesPreviousInstallAtomically() async throws {
        let store = try tempStore()
        // Pre-existing install with a sentinel file that must disappear.
        let oldDir = store.agentDirectory(id: "claude-acp")
        try FileManager.default.createDirectory(at: oldDir, withIntermediateDirectories: true)
        let sentinel = oldDir.appendingPathComponent("old-version-file")
        FileManager.default.createFile(atPath: sentinel.path, contents: Data())
        let shell = FakeShell()
        shell.effect = { command, _ in
            guard command.hasPrefix("npm install --prefix ") else { return }
            let staging = URL(fileURLWithPath: String(
                command.dropFirst("npm install --prefix ".count)
                    .split(separator: " ", maxSplits: 1)[0]))
            let bin = staging.appendingPathComponent("node_modules/.bin")
            try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
            FileManager.default.createFile(
                atPath: bin.appendingPathComponent("claude-agent-acp").path, contents: Data())
        }
        let installer = AgentInstaller(store: store, shell: shell, download: { _, _ in })
        _ = try await installer.install(npxAgent(), platform: .darwinArm64)
        #expect(!FileManager.default.fileExists(atPath: sentinel.path))
        #expect(store.manifest(id: "claude-acp")?.version == "0.60.0")
    }

    @Test func uvxOnlyAgentThrowsUnsupported() async throws {
        let store = try tempStore()
        let agent = RegistryAgent(id: "fast-agent", name: "fast-agent", version: "1",
                                  description: nil, icon: nil,
                                  distribution: AgentDistribution(hasUvx: true))
        let installer = AgentInstaller(store: store, shell: FakeShell(), download: { _, _ in })
        await #expect(throws: AgentInstallError.unsupported) {
            _ = try await installer.install(agent, platform: .darwinArm64)
        }
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter AgentInstallerTests`
Expected: FAIL — `cannot find 'AgentInstaller'`.

- [ ] **Step 3: Implement**

```swift
import Foundation

public struct ShellResult: Sendable, Equatable {
    public var exitCode: Int32
    public var output: String
    public init(exitCode: Int32, output: String) {
        self.exitCode = exitCode
        self.output = output
    }
}

/// Runs a command line through `zsh -lc` so the user's PATH (npm via
/// nvm/homebrew) resolves as in their terminal. Injectable for tests.
public protocol ShellRunning: Sendable {
    func run(_ command: String, cwd: URL) async throws -> ShellResult
}

public struct ZshRunner: ShellRunning {
    public init() {}

    public func run(_ command: String, cwd: URL) async throws -> ShellResult {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.currentDirectoryURL = cwd
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return ShellResult(exitCode: process.terminationStatus,
                           output: String(decoding: data, as: UTF8.self))
    }
}

public enum AgentInstallError: Error, Equatable {
    case unsupported
    case commandFailed(output: String)
    case noExecutableFound
}

/// Installs registry agents into the Tiller-local store. Staging-and-swap:
/// everything happens in `<root>/<id>.staging`, the final directory is
/// replaced only on success, so a failed install/update never breaks a
/// working one.
public actor AgentInstaller {
    private let store: AgentInstallStore
    private let shell: ShellRunning
    private let download: @Sendable (URL, URL) async throws -> Void

    public init(store: AgentInstallStore, shell: ShellRunning = ZshRunner(),
                download: @escaping @Sendable (URL, URL) async throws -> Void) {
        self.store = store
        self.shell = shell
        self.download = download
    }

    /// Production downloader convenience.
    public init(store: AgentInstallStore, shell: ShellRunning = ZshRunner(),
                session: URLSession = .shared) {
        self.init(store: store, shell: shell, download: { remote, destination in
            let (temp, _) = try await session.download(from: remote)
            try? FileManager.default.removeItem(at: destination)
            try FileManager.default.moveItem(at: temp, to: destination)
        })
    }

    public func install(_ agent: RegistryAgent,
                        platform: HostPlatform = .current) async throws
        -> InstalledAgentManifest {
        guard let method = agent.installMethod(platform: platform) else {
            throw AgentInstallError.unsupported
        }
        let fm = FileManager.default
        let staging = store.rootDirectory
            .appendingPathComponent("\(agent.id).staging", isDirectory: true)
        try? fm.removeItem(at: staging)
        try fm.createDirectory(at: staging, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: staging) }

        let final = store.agentDirectory(id: agent.id)
        let manifest: InstalledAgentManifest
        switch method {
        case .npx(let package, let args, let env):
            let result = try await shell.run(
                "npm install --prefix \(staging.path) \(package)",
                cwd: store.rootDirectory)
            guard result.exitCode == 0 else {
                throw AgentInstallError.commandFailed(output: result.output)
            }
            let binDir = staging.appendingPathComponent("node_modules/.bin")
            guard let binName = try resolveBinName(in: binDir, package: package) else {
                throw AgentInstallError.noExecutableFound
            }
            manifest = InstalledAgentManifest(
                id: agent.id, version: agent.version,
                executable: final.appendingPathComponent("node_modules/.bin/\(binName)").path,
                arguments: args, environment: env)
        case .binary(let archive, let cmd):
            let archiveFile = staging.appendingPathComponent(archive.lastPathComponent)
            try await download(archive, archiveFile)
            let extract = archive.lastPathComponent.hasSuffix(".zip")
                ? "ditto -x -k \(archiveFile.path) \(staging.path)"
                : "tar -xzf \(archiveFile.path) -C \(staging.path)"
            let result = try await shell.run(extract, cwd: staging)
            guard result.exitCode == 0 else {
                throw AgentInstallError.commandFailed(output: result.output)
            }
            try? fm.removeItem(at: archiveFile)
            let relative = cmd.hasPrefix("./") ? String(cmd.dropFirst(2)) : cmd
            guard fm.fileExists(atPath: staging.appendingPathComponent(relative).path) else {
                throw AgentInstallError.noExecutableFound
            }
            _ = try await shell.run(
                "chmod +x \(staging.appendingPathComponent(relative).path)", cwd: staging)
            manifest = InstalledAgentManifest(
                id: agent.id, version: agent.version,
                executable: final.appendingPathComponent(relative).path,
                arguments: [], environment: [:])
        }

        // Atomic-enough swap: remove old, move staging into place, write manifest.
        try? fm.removeItem(at: final)
        try fm.moveItem(at: staging, to: final)
        try store.write(manifest)
        return manifest
    }

    /// Picks the launchable entry in node_modules/.bin: single entry wins;
    /// with several, prefer the one whose name appears in the package name.
    private func resolveBinName(in binDir: URL, package: String) throws -> String? {
        let entries = ((try? FileManager.default.contentsOfDirectory(
            at: binDir, includingPropertiesForKeys: nil)) ?? [])
            .map(\.lastPathComponent)
            .filter { !$0.hasPrefix(".") }
            .sorted()
        if entries.count == 1 { return entries[0] }
        return entries.first { package.contains($0) } ?? entries.first
    }
}
```

Note for the implementer: `chmod +x` goes through the shell (not `fm.setAttributes`) so the FakeShell in tests can observe it; the binary test's `tar` simulation keys off the command prefix.

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test --filter AgentInstallerTests`
Expected: 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: agent installer for npx and binary distributions"
```

---

### Task 5: Launch resolution from manifests + legacy id mapping

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/AgentLaunchSpecTests.swift` (extend if exists, else create)

**Interfaces:**
- Consumes: `AgentInstallStore` (Task 2).
- Produces:
  - `AgentLaunchSpec.environment: [String: String]` (new field, default `[:]` — existing call sites keep compiling).
  - `enum AgentIdMigration { static func canonical(_ id: String) -> String }` — `"claude"→"claude-acp"`, `"codex"→"codex-acp"`, `"pi"→"pi-acp"`, everything else unchanged.
  - `AgentLaunchSpec.resolved(id:installStore:) -> AgentLaunchSpec?` — canonicalizes the id, returns the built-in omp spec for `"omp"`, otherwise builds from the installed manifest, nil when not installed.
  - Existing `forAgent(id:)` is left in place for now (still referenced by `AppModel.acpAgents()` / `openChatTab`); it is deleted in Task 12.

- [ ] **Step 1: Write the failing tests**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct AgentLaunchResolutionTests {
    func tempStore() throws -> AgentInstallStore {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("launch-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return AgentInstallStore(rootDirectory: dir)
    }

    @Test func legacyIdsCanonicalize() {
        #expect(AgentIdMigration.canonical("claude") == "claude-acp")
        #expect(AgentIdMigration.canonical("codex") == "codex-acp")
        #expect(AgentIdMigration.canonical("pi") == "pi-acp")
        #expect(AgentIdMigration.canonical("opencode") == "opencode")
        #expect(AgentIdMigration.canonical("claude-acp") == "claude-acp")
        #expect(AgentIdMigration.canonical("omp") == "omp")
    }

    @Test func ompIsBuiltIn() throws {
        let spec = AgentLaunchSpec.resolved(id: "omp", installStore: try tempStore())
        #expect(spec == AgentLaunchSpec(executable: "/bin/zsh",
                                        arguments: ["-lc", "exec omp acp"]))
    }

    @Test func installedManifestResolves() throws {
        let store = try tempStore()
        try store.write(InstalledAgentManifest(
            id: "claude-acp", version: "0.60.0",
            executable: "/x/claude-agent-acp", arguments: ["--acp"],
            environment: ["K": "V"]))
        let spec = AgentLaunchSpec.resolved(id: "claude", installStore: store)  // legacy id
        #expect(spec?.executable == "/x/claude-agent-acp")
        #expect(spec?.arguments == ["--acp"])
        #expect(spec?.environment == ["K": "V"])
    }

    @Test func notInstalledResolvesNil() throws {
        #expect(AgentLaunchSpec.resolved(id: "codex-acp",
                                         installStore: try tempStore()) == nil)
    }

    @Test func launchEnvironmentMergesManifestEnv() {
        let env = AgentLaunchSpec.launchEnvironment(
            base: ["PATH": "/usr/bin", "CLAUDECODE": "1"],
            extra: ["AUGMENT_DISABLE_AUTO_UPDATE": "1"])
        #expect(env["CLAUDECODE"] == nil)
        #expect(env["AUGMENT_DISABLE_AUTO_UPDATE"] == "1")
        #expect(env["PATH"] == "/usr/bin")
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter AgentLaunchResolutionTests`
Expected: FAIL — `cannot find 'AgentIdMigration'`.

- [ ] **Step 3: Implement in `AgentLaunchSpec.swift`**

Add `environment` to the struct (default keeps existing call sites source-compatible):

```swift
public struct AgentLaunchSpec: Sendable, Equatable {
    public var executable: String
    public var arguments: [String]
    /// Extra environment from the install manifest, merged over the base
    /// environment at launch.
    public var environment: [String: String]

    public init(executable: String, arguments: [String],
                environment: [String: String] = [:]) {
        self.executable = executable
        self.arguments = arguments
        self.environment = environment
    }
```

Extend `launchEnvironment` with an `extra` parameter (existing single-argument call sites keep working):

```swift
    public static func launchEnvironment(
        base: [String: String] = ProcessInfo.processInfo.environment,
        extra: [String: String] = [:]
    ) -> [String: String] {
        var environment = base.merging(extra) { _, new in new }
        environment.removeValue(forKey: "CLAUDECODE")
        environment.removeValue(forKey: "CLAUDE_CODE_ENTRYPOINT")
        return environment
    }
```

Add id migration + resolution (same file):

```swift
/// Registry ids are canonical (`claude-acp`); Tiller's historical short ids
/// (persisted in sessions and tabs from earlier versions) map onto them.
public enum AgentIdMigration {
    public static func canonical(_ id: String) -> String {
        switch id {
        case "claude": "claude-acp"
        case "codex": "codex-acp"
        case "pi": "pi-acp"
        default: id
        }
    }
}

extension AgentLaunchSpec {
    /// Launch spec resolved from the install store + built-ins. Replaces the
    /// hardcoded `forAgent(id:)` (removed once all callers migrate).
    public static func resolved(id: String,
                                installStore: AgentInstallStore) -> AgentLaunchSpec? {
        let canonical = AgentIdMigration.canonical(id)
        if canonical == "omp" {
            return AgentLaunchSpec(executable: "/bin/zsh",
                                   arguments: ["-lc", "exec omp acp"])
        }
        guard let manifest = installStore.manifest(id: canonical) else { return nil }
        return AgentLaunchSpec(executable: manifest.executable,
                               arguments: manifest.arguments,
                               environment: manifest.environment)
    }
}
```

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test`
Expected: all TillerACP tests PASS (new + pre-existing — the `AgentLaunchSpec` field addition must not break existing tests).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: resolve agent launch specs from install manifests"
```

---

### Task 6: `.systemNotice` transcript item + handoff serializer

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift` (add case)
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift` (`kindLabel` switch)
- Create: `Packages/TillerACP/Sources/TillerACP/TranscriptHandoff.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptHandoffTests.swift`

**Interfaces:**
- Produces:
  - `TranscriptItem.systemNotice(id: String, text: String)` (Codable like the others; `kindLabel` = `"systemNotice"`).
  - `enum TranscriptHandoff { static let maxBytes = 100_000; static func preamble(items: [TranscriptItem]) -> String? }` — nil for empty/no-content transcripts.
- Warning: adding an enum case breaks every exhaustive `switch` over `TranscriptItem`. Fix them all in this task so the whole workspace still builds: `kindLabel` in `ChatSessionStore.swift`, plus any in `TranscriptReducer.swift`, `SubagentTasks.swift`, and (App target, compiled by ci.sh) `App/Chat/TranscriptView.swift` — for now render nothing in TranscriptView (`EmptyView()`); Task 9 adds the real rendering. Grep: `grep -rn "case .editSummary" Packages/TillerACP App/` to find every exhaustive switch (editSummary is the most recently added case, so its handlers are exactly the switches to extend).

- [ ] **Step 1: Write the failing tests**

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct TranscriptHandoffTests {
    @Test func emptyTranscriptProducesNoPreamble() {
        #expect(TranscriptHandoff.preamble(items: []) == nil)
        #expect(TranscriptHandoff.preamble(items: [
            .turnDivider(id: "d", at: Date())]) == nil)
    }

    @Test func formatsMessagesAndCompressesToolCalls() throws {
        let items: [TranscriptItem] = [
            .userMessage(id: "u1", blocks: [.text("Fix the bug")]),
            .toolCall(ToolCallItem(toolCallId: "t1", title: "Read main.swift",
                                   kind: .read, status: .completed)),
            .agentMessage(id: "a1", text: "Done, the bug was X.", isComplete: true),
            .thought(id: "th1", text: "hidden reasoning"),
            .systemNotice(id: "s1", text: "Agent changed"),
        ]
        let preamble = try #require(TranscriptHandoff.preamble(items: items))
        #expect(preamble.contains("User: Fix the bug"))
        #expect(preamble.contains("- [tool] Read main.swift → completed"))
        #expect(preamble.contains("Agent: Done, the bug was X."))
        #expect(!preamble.contains("hidden reasoning"))   // thoughts excluded
        #expect(!preamble.contains("Agent changed"))      // notices excluded
        #expect(preamble.hasPrefix("Previous conversation with another agent"))
    }

    @Test func capDropsOldestFirstWithTruncationNote() throws {
        let big = String(repeating: "x", count: 60_000)
        let items: [TranscriptItem] = [
            .agentMessage(id: "old", text: "OLDEST-" + big, isComplete: true),
            .agentMessage(id: "mid", text: "MIDDLE-" + big, isComplete: true),
            .agentMessage(id: "new", text: "NEWEST-" + big, isComplete: true),
        ]
        let preamble = try #require(TranscriptHandoff.preamble(items: items))
        #expect(preamble.utf8.count < TranscriptHandoff.maxBytes + 1_000)
        #expect(preamble.contains("NEWEST-"))
        #expect(!preamble.contains("OLDEST-"))
        #expect(preamble.contains("(older messages truncated)"))
    }

    @Test func systemNoticeRoundTripsThroughCodable() throws {
        let item = TranscriptItem.systemNotice(id: "s1", text: "Agent changed: A → B.")
        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(TranscriptItem.self, from: data)
        #expect(decoded == item)
        #expect(item.kindLabel == "systemNotice")
    }
}
```

Check `ToolCallItem`'s `kind:` argument against `ToolKind`'s real cases (`.read` here — use whatever exists, e.g. `.other`, if `.read` doesn't).

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd Packages/TillerACP && swift test --filter TranscriptHandoffTests`
Expected: FAIL — `type 'TranscriptItem' has no member 'systemNotice'`.

- [ ] **Step 3: Implement**

In `TranscriptItem.swift`, add the case and its `id`:

```swift
    /// Local UI notice (e.g. agent switch); never sent by the protocol,
    /// excluded from handoff preambles.
    case systemNotice(id: String, text: String)
```

```swift
        case .systemNotice(let id, _): id
```

In `ChatSessionStore.swift` `kindLabel`:

```swift
        case .systemNotice: "systemNotice"
```

Fix every other exhaustive switch found by the grep (render/skip the new case: `TranscriptReducer` and `SubagentTasks` treat it like `.turnDivider`; `TranscriptView` renders `EmptyView()` for now).

New file `TranscriptHandoff.swift`:

```swift
import Foundation

/// Serializes a transcript into the "previous conversation" preamble sent to
/// a newly selected agent. Pure — trivially testable.
public enum TranscriptHandoff {
    public static let maxBytes = 100_000

    public static func preamble(items: [TranscriptItem]) -> String? {
        let lines = items.compactMap(line(for:))
        guard !lines.isEmpty else { return nil }

        var included: [String] = []
        var total = 0
        var truncated = false
        for line in lines.reversed() {           // newest kept first
            let cost = line.utf8.count + 1
            if total + cost > maxBytes { truncated = true; break }
            included.append(line)
            total += cost
        }
        var body = Array(included.reversed()).joined(separator: "\n")
        if truncated { body = "(older messages truncated)\n" + body }
        return """
        Previous conversation with another agent (for context — continue from here):

        \(body)

        ---
        """
    }

    private static func line(for item: TranscriptItem) -> String? {
        switch item {
        case .userMessage(_, let blocks):
            let text = blocks.compactMap { block -> String? in
                if case .text(let value) = block { return value }
                return nil
            }.joined(separator: "\n")
            return text.isEmpty ? nil : "User: \(text)"
        case .agentMessage(_, let text, _):
            return text.isEmpty ? nil : "Agent: \(text)"
        case .toolCall(let call):
            return "- [tool] \(call.title) → \(call.status)"
        case .thought, .plan, .turnDivider, .editSummary, .systemNotice:
            return nil
        }
    }
}
```

If `ToolCallStatus` doesn't interpolate readably, use its `rawValue`.

- [ ] **Step 4: Run all package tests + workspace build**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (including pre-existing `ChatSessionStoreTests` — the new case must not break stored-transcript decoding).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP App/Chat/TranscriptView.swift
git commit -m "feat: system notice transcript item and handoff serializer"
```

---

### Task 7: ChatSessionStore — worktree-scoped lookup, agent switch support

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/ChatSessionStoreTests.swift` (extend)

**Interfaces:**
- Produces:
  - `latestSession(worktreeId:) -> ChatSessionRecord?` — replaces `latestSession(worktreeId:agentId:)`; same "has at least one persisted item" filter, no agent filter. Update existing tests/callers.
  - `setAgentId(_ agentId: String, sessionId: String) throws` — records the last-used agent.
  - `clearACPSessionId(sessionId: String) throws` — severs same-agent resume after a switch.

- [ ] **Step 1: Write the failing tests** (append to the existing suite, reusing its in-memory database helper — mirror the setup used by `createThenLatestFindsSession`)

```swift
    @Test func latestSessionIgnoresAgent() throws {
        let store = try makeStore()  // existing helper name in this file; adapt
        let a = try store.createSession(worktreeId: "w1", agentId: "claude-acp")
        try store.saveTranscript(sessionId: a.id, items: [
            .agentMessage(id: "m1", text: "hi", isComplete: true)])
        let b = try store.createSession(worktreeId: "w1", agentId: "codex-acp",
                                        now: Date().addingTimeInterval(10))
        try store.saveTranscript(sessionId: b.id, items: [
            .agentMessage(id: "m2", text: "yo", isComplete: true)],
            now: Date().addingTimeInterval(20))
        #expect(try store.latestSession(worktreeId: "w1")?.id == b.id)
        #expect(try store.latestSession(worktreeId: "w1")?.agentId == "codex-acp")
    }

    @Test func setAgentIdUpdatesRecord() throws {
        let store = try makeStore()
        let session = try store.createSession(worktreeId: "w1", agentId: "claude-acp")
        try store.saveTranscript(sessionId: session.id, items: [
            .agentMessage(id: "m", text: "x", isComplete: true)])
        try store.setAgentId("codex-acp", sessionId: session.id)
        #expect(try store.latestSession(worktreeId: "w1")?.agentId == "codex-acp")
    }

    @Test func clearACPSessionIdRemovesResumeHandle() throws {
        let store = try makeStore()
        let session = try store.createSession(worktreeId: "w1", agentId: "claude-acp")
        try store.setACPSessionId("acp-123", sessionId: session.id)
        try store.saveTranscript(sessionId: session.id, items: [
            .agentMessage(id: "m", text: "x", isComplete: true)])
        try store.clearACPSessionId(sessionId: session.id)
        #expect(try store.latestSession(worktreeId: "w1")?.acpSessionId == nil)
    }
```

- [ ] **Step 2: Run tests, verify the new ones fail**

Run: `cd Packages/TillerACP && swift test --filter ChatSessionStoreTests`
Expected: new tests FAIL (missing methods / extra-argument error).

- [ ] **Step 3: Implement**

Replace `latestSession(worktreeId:agentId:)` (drop the `agentId` filter line and parameter, keep the doc comment updated — `agentId` on the record is now the *last-used* agent, not an owner):

```swift
    public func latestSession(worktreeId: String) throws -> ChatSessionRecord? {
        try database.read { db in
            try ChatSessionRecord
                .filter(Column("worktreeId") == worktreeId)
                .filter(sql: "EXISTS (SELECT 1 FROM chatItem WHERE chatItem.sessionId = chatSession.id)")
                .order(Column("lastActivityAt").desc)
                .fetchOne(db)
        }
    }

    /// Records the (new) last-used agent for a session after a switch.
    public func setAgentId(_ agentId: String, sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.agentId = agentId
            try record.update(db)
        }
    }

    /// Severs same-agent resume: after an agent switch the stored ACP session
    /// belongs to the previous agent and must never be replayed into the new one.
    public func clearACPSessionId(sessionId: String) throws {
        try database.write { db in
            guard var record = try ChatSessionRecord.fetchOne(db, key: sessionId) else { return }
            record.acpSessionId = nil
            try record.update(db)
        }
    }
```

Update the existing tests that call `latestSession(worktreeId:agentId:)` to the new signature (drop the argument; where a test relied on per-agent filtering, its assertion becomes "latest across agents"). `ChatController.start()` also calls the old signature — leave it broken only if the package builds independently; otherwise apply the minimal call-site fix there (`latestSession(worktreeId:)`) now, since Task 8 rewrites that code anyway.

- [ ] **Step 4: Run tests, verify they pass**

Run: `cd Packages/TillerACP && swift test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP App/Chat/ChatController.swift
git commit -m "feat: worktree-scoped session lookup with last-used agent semantics"
```

---

### Task 8: ChatController — mutable agent, switch flow, handoff injection

**Files:**
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/AppModel.swift:1352-1367` (`chatController(for:in:)` — pass the install store)

**Interfaces:**
- Consumes: `AgentLaunchSpec.resolved(id:installStore:)`, `AgentIdMigration` (Task 5), `TranscriptHandoff`, `.systemNotice` (Task 6), `setAgentId`/`clearACPSessionId`/`latestSession(worktreeId:)` (Task 7).
- Produces (used by Tasks 9-11):
  - `ChatController.agentId` becomes `private(set) var` (canonicalized in `init` via `AgentIdMigration.canonical`).
  - `ChatController.init(tabId:agentId:worktreeId:worktreePath:store:installStore:)` — new `installStore: AgentInstallStore` parameter.
  - `func switchAgent(to newAgentId: String, displayName: String) async`.
  - `AppModel.agentInstallStore: AgentInstallStore` property.

- [ ] **Step 1: Apply the ChatController changes**

No good unit seam exists for the controller (it owns a live process); the logic-heavy pieces were made pure and tested in Tasks 5-7. Changes:

1. Declarations:

```swift
    private(set) var agentId: String
    private let installStore: AgentInstallStore
    /// Set on agent switch; the next send() prepends the handoff preamble.
    private var pendingHandoff = false
```

`init` gains `installStore: AgentInstallStore` and sets `self.agentId = AgentIdMigration.canonical(agentId)`.

2. `start()` — replace the spec guard and session-record logic:

```swift
    func start() async {
        guard state == .idle || isDisconnected else { return }

        var record = try? store?.latestSession(worktreeId: worktreeId.uuidString)
        if forceNewSession { record = nil }
        // First start of a reopened chat: adopt the session's last-used agent.
        if let record, sessionRecordId == nil, restored.isEmpty, reducer.items.isEmpty {
            agentId = AgentIdMigration.canonical(record.agentId)
        }
        guard let spec = AgentLaunchSpec.resolved(id: agentId, installStore: installStore) else {
            state = .disconnected(message: "Agent not installed. Install it from Settings → Agents.")
            return
        }
        state = .connecting

        if let record, sessionRecordId == nil,
           let stored = try? store?.loadTranscript(sessionId: record.id) {
            restored = stored
        }
        if let record, let used = record.contextUsageUsed, let size = record.contextUsageSize {
            reducer.restoreContextUsage(ContextUsage(used: used, size: size))
        }

        let transport = ProcessTransport(
            executable: spec.executable, arguments: spec.arguments,
            cwd: worktreePath,
            environment: AgentLaunchSpec.launchEnvironment(extra: spec.environment),
            onStderrLine: { line in
                NSLog("[chat:\(spec.arguments.last ?? "?")] %@", line)
            })
        // ... ACPSession construction unchanged ...

        // Resume only a session created by this same agent.
        let resumeId = (AgentIdMigration.canonical(record?.agentId ?? "") == agentId)
            ? record?.acpSessionId : nil
        let handle = try await session.connect(
            cwd: worktreePath, resumeSessionId: resumeId,
            mcpServers: mcpServers)
        // ... modes/models/effort handling unchanged ...
        if handle.didResume, let record {
            restored = []
            sessionRecordId = record.id
            try? store?.setACPSessionId(handle.sessionId, sessionId: record.id)
        } else if let sessionRecordId {
            // Agent switch reuses the existing record for transcript continuity.
            try? store?.setACPSessionId(handle.sessionId, sessionId: sessionRecordId)
        } else {
            let created = try store?.createSession(
                worktreeId: worktreeId.uuidString, agentId: agentId)
            sessionRecordId = created?.id
            if let sessionRecordId {
                try? store?.setACPSessionId(handle.sessionId, sessionId: sessionRecordId)
            }
        }
```

(Keep the surrounding error handling exactly as it is.)

3. New method:

```swift
    /// Switches the conversation to another agent: same transcript, new
    /// process, new ACP session; the next prompt carries the handoff preamble.
    func switchAgent(to newAgentId: String, displayName: String) async {
        let canonical = AgentIdMigration.canonical(newAgentId)
        guard canonical != agentId else { return }
        if state == .prompting { await cancelTurn() }
        await stop()

        let snapshot = items
        reducer = TranscriptReducer()
        restored = snapshot + [.systemNotice(
            id: UUID().uuidString,
            text: "Agent changed to \(displayName). The previous conversation will be resent to the new agent; long conversations may exceed its context window.")]
        agentId = canonical
        pendingHandoff = snapshot.contains { item in
            if case .turnDivider = item { return false }
            if case .systemNotice = item { return false }
            return true
        }
        modes = nil
        models = nil
        effortOption = nil
        if let sessionRecordId {
            try? store?.setAgentId(canonical, sessionId: sessionRecordId)
            try? store?.clearACPSessionId(sessionId: sessionRecordId)
        }
        state = .idle
        await start()
        persist()
    }
```

4. `send(...)` — inject the preamble after `reducer.userPrompted(blocks)` (the preamble must reach the agent but not render as the user's message):

```swift
        reducer.userPrompted(blocks)
        if pendingHandoff {
            pendingHandoff = false
            if let preamble = TranscriptHandoff.preamble(items: restored) {
                blocks.insert(.text(preamble), at: 0)
            }
        }
```

(`blocks` becomes `var`.)

- [ ] **Step 2: Wire the install store in AppModel**

In `App/AppModel.swift`, add near the other stored properties:

```swift
    /// Tiller-managed ACP agent installs (Settings → Agents).
    let agentInstallStore = AgentInstallStore(
        rootDirectory: FileManager.default.urls(
            for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("Tiller/acp-agents", isDirectory: true))
```

and pass it in `chatController(for:in:)`:

```swift
        let controller = ChatController(
            tabId: tab.id, agentId: agentId, worktreeId: worktree.id,
            worktreePath: worktree.path, store: chatStore,
            installStore: agentInstallStore)
```

Add `import TillerACP` where missing.

- [ ] **Step 3: Build**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller build -quiet 2>&1 | tail -5` (or `Scripts/ci.sh` if faster locally)
Expected: build succeeds. Note: `openChatTab`'s `AgentLaunchSpec.forAgent` guard still compiles (Task 5 kept it); it is removed in Task 10.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatController.swift App/AppModel.swift
git commit -m "feat: agent switching with transcript handoff in chat controller"
```

---

### Task 9: Chat UI — agent selector menu + notice rendering

**Files:**
- Modify: `App/Chat/ChatPaneView.swift` (header)
- Modify: `App/Chat/TranscriptView.swift` (replace Task 6's `EmptyView()` for `.systemNotice`)

**Interfaces:**
- Consumes: `controller.switchAgent(to:displayName:)` (Task 8), `AcpAgentCenter.installedAgents` (Task 11 — see note below).
- Note on ordering: this task references `AcpAgentCenter`. To keep every task independently buildable, implement Task 11 (`AcpAgentCenter` + Settings) **before** this one if executing strictly in order becomes an issue — or implement here the minimal source the menu needs and let Task 11 extend it. Recommended execution order: 10 → 11 → 9.

- [ ] **Step 1: Replace the static `AgentIcon` in the header with a selector menu**

In `ChatPaneView.header`, replace `AgentIcon(agentId: controller.agentId, size: 14)` with:

```swift
            Menu {
                ForEach(appModel.agentCenter.installedAgents) { agent in
                    Button {
                        Task {
                            await controller.switchAgent(to: agent.id,
                                                         displayName: agent.name)
                        }
                    } label: {
                        if let image = AgentMenuIconCache.image(for: agent.id) {
                            Label { Text(agent.name) } icon: { Image(nsImage: image) }
                        } else {
                            Text(agent.name)
                        }
                    }
                    .disabled(agent.id == controller.agentId)
                }
                Divider()
                Button("Other agents…") { appModel.openAgentsSettings() }
            } label: {
                HStack(spacing: 4) {
                    AgentIcon(agentId: controller.agentId, size: 14)
                    Image(systemName: "chevron.down")
                        .font(.system(size: 8, weight: .semibold))
                        .foregroundStyle(.secondary)
                }
            }
            .menuStyle(.borderlessButton)
            .fixedSize()
            .help("Switch agent for this conversation")
```

`installedAgents` is `[InstalledAgentSummary]` (`id`, `name`, `Identifiable`) from `AcpAgentCenter` (Task 11).

- [ ] **Step 1b: Keep brand icons for canonical ids**

`AgentIcon` switches on the legacy short ids (`"claude"`, `"codex"`, `"pi"`); canonical registry ids (`claude-acp`, `codex-acp`, `pi-acp`) would fall through to the monogram. In `App/AgentIcon.swift`, normalize at the top of `body` (and in `color(for:)`):

```swift
    private var normalizedId: String {
        agentId.hasSuffix("-acp") ? String(agentId.dropLast(4)) : agentId
    }
```

and switch on `normalizedId` instead of `agentId`.

- [ ] **Step 2: Render the system notice in `TranscriptView`**

Where Task 6 left `EmptyView()` for `.systemNotice`:

```swift
        case .systemNotice(_, let text):
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity, alignment: .center)
                .padding(.vertical, 6)
```

- [ ] **Step 3: Build and run visually**

Run: `xcodegen generate && open Tiller.xcodeproj` — build with ⌘R; open a chat, switch agent, verify: menu lists installed agents, notice appears centered, new agent connects.

- [ ] **Step 4: Commit**

```bash
git add App/Chat/ChatPaneView.swift App/Chat/TranscriptView.swift
git commit -m "feat: in-chat agent selector with switch notice"
```

---

### Task 10: Agent-agnostic chat tab creation

**Files:**
- Modify: `App/AppModel.swift:1322-1348` (`acpAgents()`, `openChatTab`)
- Modify: `App/NewTabMenuItems.swift:35-38`
- Modify: `App/SidebarView.swift:44-47`

**Interfaces:**
- Consumes: `agentInstallStore` (Task 8), `AgentLaunchSpec.resolved` (Task 5).
- Produces: `AppModel.openChatTab(in:) -> WorkspaceTab?` (agent-less; picks the default agent), `AppModel.defaultChatAgentId: String?`, UserDefaults key `"chat.lastAgentId"`.

- [ ] **Step 1: Rework `openChatTab`**

Replace `static func acpAgents()` and `openChatTab(agentId:in:)` with:

```swift
    /// Most recently used chat agent app-wide, falling back to the first
    /// installed one. Nil when nothing is installed (the chat then shows its
    /// "not installed" banner pointing at Settings).
    var defaultChatAgentId: String? {
        if let last = defaults.string(forKey: "chat.lastAgentId"),
           AgentLaunchSpec.resolved(id: last, installStore: agentInstallStore) != nil {
            return last
        }
        return agentCenter.installedAgents.first?.id
    }

    @discardableResult
    func openChatTab(in worktree: Worktree) -> WorkspaceTab? {
        let agentId = defaultChatAgentId ?? "claude-acp"
        let tab = WorkspaceTab(id: UUID(), title: "Chat",
                               content: .chat(agentId: agentId))
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        agentActivity.agentSpawned(paneId: tab.id, agentId: agentId, now: Date())
        persistTabs(for: worktree.id)
        return tab
    }

    /// Called by ChatController wiring whenever a chat connects or switches.
    func rememberChatAgent(_ agentId: String) {
        defaults.set(agentId, forKey: "chat.lastAgentId")
    }
```

Notes: `defaults` is the injectable `UserDefaults` already used by AppModel (auto-rename fix convention — never `UserDefaults.standard` directly in tests). `agentCenter` comes from Task 11; if executing before Task 11, temporarily use `agentInstallStore.installedManifests().first?.id`. Wire `rememberChatAgent` in `chatController(for:in:)` by wrapping the existing `onStatusChange` setup with an additional callback, or simply call it from `ChatPaneView`'s switch action and from `openChatTab`.

- [ ] **Step 2: Update the two menus**

`App/NewTabMenuItems.swift:35-38` — replace the `ForEach(AppModel.acpAgents())` block with:

```swift
            Button("New Chat") {
                model.openChatTab(in: worktree)
            }
```

`App/SidebarView.swift:44-47` — same replacement (keep the surrounding menu structure/labels consistent with what's there).

- [ ] **Step 3: Build**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: builds; existing AppTests pass. If any test references `openChatTab(agentId:in:)` or `acpAgents()`, update it to the new API.

- [ ] **Step 4: Commit**

```bash
git add App/AppModel.swift App/NewTabMenuItems.swift App/SidebarView.swift
git commit -m "feat: agent-agnostic chat tab creation"
```

---

### Task 11: Settings "Agents" tab

**Files:**
- Create: `App/AcpAgentCenter.swift`
- Create: `App/AgentsSettingsView.swift`
- Modify: `App/AppRoute.swift` (add `SettingsCategory.agents`)
- Modify: `App/SettingsSurface.swift:65-69` (switch)
- Modify: `App/AppModel.swift` (own the center; `openAgentsSettings()`)

**Interfaces:**
- Consumes: `AgentRegistryClient` (Task 3), `AgentInstaller` (Task 4), `AgentInstallStore`/`AgentInstallStatus` (Task 2).
- Produces: `AcpAgentCenter` (`@MainActor @Observable`):
  - `struct InstalledAgentSummary: Identifiable, Equatable { let id: String; let name: String }`
  - `struct AgentRow: Identifiable { let id: String; let name: String; let description: String?; let latestVersion: String? }`
  - `enum RowStatus: Equatable { case notInstalled, installing, installed(String), updateAvailable(installed: String, latest: String), failed(String), unsupported, builtin(available: Bool) }`
  - `rows: [AgentRow]`, `statuses: [String: RowStatus]`, `installedAgents: [InstalledAgentSummary]`, `registryError: String?`, `lastFetchedAt: Date?`
  - `func refresh(force: Bool = false) async`, `func install(_ id: String) async`
  - `AppModel.agentCenter: AcpAgentCenter`, `AppModel.openAgentsSettings()`.

- [ ] **Step 1: Implement `AcpAgentCenter`**

```swift
import Foundation
import Observation
import TillerACP

/// Observable façade over registry + installer for the Settings tab and the
/// in-chat selector. All state is @MainActor; installs run on the actor-
/// isolated installer and report back here.
@MainActor @Observable
final class AcpAgentCenter {
    struct InstalledAgentSummary: Identifiable, Equatable {
        let id: String
        let name: String
    }

    struct AgentRow: Identifiable, Equatable {
        let id: String
        let name: String
        let description: String?
        let latestVersion: String?
    }

    enum RowStatus: Equatable {
        case notInstalled
        case installing
        case installed(String)
        case updateAvailable(installed: String, latest: String)
        case failed(String)
        case unsupported
        case builtin(available: Bool)
    }

    private(set) var rows: [AgentRow] = []
    private(set) var statuses: [String: RowStatus] = [:]
    private(set) var registryError: String?
    private(set) var lastFetchedAt: Date?

    let installStore: AgentInstallStore
    private let registryClient: AgentRegistryClient
    private let installer: AgentInstaller
    private let shell: ShellRunning
    private var registryAgents: [RegistryAgent] = []

    init(installStore: AgentInstallStore,
         registryClient: AgentRegistryClient? = nil,
         installer: AgentInstaller? = nil,
         shell: ShellRunning = ZshRunner()) {
        self.installStore = installStore
        let cacheURL = installStore.rootDirectory
            .deletingLastPathComponent()
            .appendingPathComponent("acp-registry.json")
        self.registryClient = registryClient
            ?? AgentRegistryClient(cacheURL: cacheURL)
        self.installer = installer ?? AgentInstaller(store: installStore)
        self.shell = shell
    }

    /// Installed agents for the chat selector: manifests + omp when its
    /// binary is present. Names come from the registry when known.
    var installedAgents: [InstalledAgentSummary] {
        var result = installStore.installedManifests().map { manifest in
            InstalledAgentSummary(id: manifest.id,
                                  name: displayName(for: manifest.id))
        }
        if case .builtin(true) = statuses["omp"] ?? .builtin(available: false) {
            result.append(InstalledAgentSummary(id: "omp", name: "omp"))
        }
        return result.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    func displayName(for id: String) -> String {
        registryAgents.first { $0.id == id }?.name ?? id
    }

    func refresh(force: Bool = false) async {
        do {
            let registry = try await registryClient.registry(forceRefresh: force)
            registryAgents = registry.agents
            registryError = nil
        } catch {
            registryError = "Could not load the agent registry: \(error.localizedDescription)"
        }
        lastFetchedAt = await registryClient.lastFetchedAt()

        var newRows: [AgentRow] = [
            AgentRow(id: "omp", name: "omp (Oh My Pi)",
                     description: "Built-in: uses the omp binary on your PATH.",
                     latestVersion: nil)
        ]
        newRows += registryAgents.map {
            AgentRow(id: $0.id, name: $0.name, description: $0.description,
                     latestVersion: $0.version)
        }.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        rows = newRows

        let ompFound = (try? await shell.run("command -v omp",
                                             cwd: FileManager.default.temporaryDirectory))
            .map { $0.exitCode == 0 } ?? false
        var newStatuses: [String: RowStatus] = ["omp": .builtin(available: ompFound)]
        for agent in registryAgents {
            newStatuses[agent.id] = Self.rowStatus(
                AgentInstallStatus.resolve(
                    manifest: installStore.manifest(id: agent.id),
                    latestVersion: agent.version,
                    isSupported: agent.installMethod(platform: .current) != nil))
        }
        // Installed agents that vanished from the registry stay usable.
        for manifest in installStore.installedManifests()
        where newStatuses[manifest.id] == nil {
            newStatuses[manifest.id] = .installed(manifest.version)
        }
        statuses = newStatuses
    }

    func install(_ id: String) async {
        guard let agent = registryAgents.first(where: { $0.id == id }) else { return }
        statuses[id] = .installing
        do {
            let manifest = try await installer.install(agent)
            statuses[id] = .installed(manifest.version)
        } catch AgentInstallError.commandFailed(let output) {
            statuses[id] = .failed(String(output.suffix(300)))
        } catch {
            statuses[id] = .failed("\(error)")
        }
    }

    private static func rowStatus(_ status: AgentInstallStatus) -> RowStatus {
        switch status {
        case .notInstalled: .notInstalled
        case .installed(let version): .installed(version)
        case .updateAvailable(let installed, let latest):
            .updateAvailable(installed: installed, latest: latest)
        case .unsupported: .unsupported
        }
    }
}
```

- [ ] **Step 2: Implement `AgentsSettingsView`**

```swift
import SwiftUI
import TillerACP

/// Settings → Agents: install/update ACP agents from the official registry.
struct AgentsSettingsView: View {
    let center: AcpAgentCenter
    @State private var search = ""

    private var filteredRows: [AcpAgentCenter.AgentRow] {
        guard !search.isEmpty else { return center.rows }
        return center.rows.filter {
            $0.name.localizedCaseInsensitiveContains(search)
                || $0.id.localizedCaseInsensitiveContains(search)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                TextField("Search agents", text: $search)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 260)
                Spacer()
                if let fetched = center.lastFetchedAt {
                    Text("Updated \(fetched.formatted(.relative(presentation: .named)))")
                        .font(.caption).foregroundStyle(.secondary)
                }
                Button {
                    Task { await center.refresh(force: true) }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .help("Refresh the agent registry")
            }
            if let error = center.registryError {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(.caption).foregroundStyle(.orange)
            }
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(filteredRows) { row in
                        agentRow(row)
                        Divider()
                    }
                }
            }
        }
        .padding(16)
        .task { await center.refresh() }
    }

    @ViewBuilder
    private func agentRow(_ row: AcpAgentCenter.AgentRow) -> some View {
        HStack(spacing: 10) {
            AgentIcon(agentId: row.id, size: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name).font(.body)
                if let description = row.description {
                    Text(description).font(.caption)
                        .foregroundStyle(.secondary).lineLimit(2)
                }
            }
            Spacer()
            statusControl(row)
        }
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private func statusControl(_ row: AcpAgentCenter.AgentRow) -> some View {
        switch center.statuses[row.id] ?? .notInstalled {
        case .notInstalled:
            Button("Install") { Task { await center.install(row.id) } }
                .controlSize(.small)
        case .installing:
            ProgressView().controlSize(.small)
        case .installed(let version):
            Label("Installed \(version)", systemImage: "checkmark.circle.fill")
                .font(.caption).foregroundStyle(.green)
        case .updateAvailable(let installed, let latest):
            HStack(spacing: 6) {
                Text("v\(installed)").font(.caption).foregroundStyle(.secondary)
                Button("Update to \(latest)") { Task { await center.install(row.id) } }
                    .controlSize(.small)
            }
        case .failed(let message):
            HStack(spacing: 6) {
                Text(message).font(.caption).foregroundStyle(.red)
                    .lineLimit(1).help(message)
                Button("Retry") { Task { await center.install(row.id) } }
                    .controlSize(.small)
            }
        case .unsupported:
            Text("Not supported yet").font(.caption).foregroundStyle(.secondary)
        case .builtin(let available):
            Label(available ? "Available" : "omp binary not found on PATH",
                  systemImage: available ? "checkmark.circle.fill" : "questionmark.circle")
                .font(.caption)
                .foregroundStyle(available ? .green : .secondary)
        }
    }
}
```

- [ ] **Step 3: Wire category, surface, AppModel**

`App/AppRoute.swift` — add to `SettingsCategory`:

```swift
    case agents
```

with `title` `"Agents"` and `symbol` `"cpu"` in the two switches (place after `aiProviders`).

`App/SettingsSurface.swift:65` — add to the switch:

```swift
        case .agents: AgentsSettingsView(center: model.agentCenter)
```

`App/AppModel.swift` — own the center and add the route helper:

```swift
    let agentCenter: AcpAgentCenter
```

initialize in `init` (after `agentInstallStore` is available): `agentCenter = AcpAgentCenter(installStore: ...)` — note `agentInstallStore` can move its literal construction into `init` so both share the same instance; and:

```swift
    func openAgentsSettings() {
        settingsCategory = .agents
        route = .settings
    }
```

- [ ] **Step 4: Build + regenerate project (new App files)**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK`.

- [ ] **Step 5: Commit**

```bash
git add App/AcpAgentCenter.swift App/AgentsSettingsView.swift App/AppRoute.swift App/SettingsSurface.swift App/AppModel.swift project.yml
git commit -m "feat: agents settings tab installing ACP agents from the registry"
```

---

### Task 12: Remove hardcoded launch list, final gate

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/AgentLaunchSpec.swift` (delete `claudeCode()`, `openCode()`, `forAgent(id:)`, `claudeCodeACPVersion`)
- Modify: any remaining callers (grep `forAgent`)

- [ ] **Step 1: Delete the hardcoded factories**

Remove `claudeCode()`, `openCode()`, `forAgent(id:)` and `claudeCodeACPVersion` from `AgentLaunchSpec.swift`. Grep for stragglers:

Run: `grep -rn "forAgent\|claudeCodeACPVersion\|acpAgents" Packages App --include="*.swift"`
Expected: no hits outside tests; fix any that remain (they should all have been migrated in Tasks 8-11). Update/delete tests that covered the removed factories.

- [ ] **Step 2: Full verification gate**

Run: `xcodegen generate && Scripts/ci.sh`
Expected: `CI OK` (retry per the known flaky PTY test if `spawnCapturesOutput` fails — up to 5-6 runs).

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor: drop hardcoded agent launch specs in favor of install manifests"
```

---

## Manual smoke checklist (post-implementation)

1. Settings → Agents: registry loads (38+ rows), search filters, omp row shows binary state.
2. Install `claude-acp` → status becomes Installed; new chat connects with it.
3. Install a binary-distribution agent (e.g. `opencode`) → connects.
4. Chat: send messages with agent A, switch to agent B → notice appears, first prompt to B answers with knowledge of the prior conversation.
5. Kill network → Settings still lists agents from cache; error banner only on cold start without cache.
6. Reopen Tiller → existing chat restores transcript and last-used agent.
7. Legacy check: a chat tab persisted with `agentId: "claude"` before this feature resolves to `claude-acp` (after installing it).
