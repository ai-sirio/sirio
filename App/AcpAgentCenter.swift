import Foundation
import Observation
import TillerACP

/// Observable façade over registry + installer for the Settings tab and the
/// in-chat selector. All state is @MainActor; installs run on the actor-
/// isolated installer and report back here.
@MainActor @Observable
final class AcpAgentCenter {
    private static let nativeAgentIDs = ["claude-acp", "codex-acp", "opencode", "pi"]

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
    private let pathProbe: @Sendable (String) -> Bool
    private var registryAgents: [RegistryAgent] = []

    init(installStore: AgentInstallStore,
         registryClient: AgentRegistryClient? = nil,
         installer: AgentInstaller? = nil,
         shell: ShellRunning = ZshRunner(),
         pathProbe: @escaping @Sendable (String) -> Bool = defaultPathProbe) {
        self.installStore = installStore
        let cacheURL = installStore.rootDirectory
            .deletingLastPathComponent()
            .appendingPathComponent("acp-registry.json")
        self.registryClient = registryClient
            ?? AgentRegistryClient(cacheURL: cacheURL)
        self.installer = installer ?? AgentInstaller(store: installStore)
        self.shell = shell
        self.pathProbe = pathProbe
        Task { await self.refresh() }
    }

    /// Installed agents for the chat selector: ACP manifests plus built-in
    /// agents whose native CLI is present. Names come from the registry when
    /// known.
    var installedAgents: [InstalledAgentSummary] {
        var result = installStore.installedManifests().compactMap { manifest -> InstalledAgentSummary? in
            let canonical = AgentIdMigration.canonical(manifest.id)
            guard !Self.isNative(canonical) else { return nil }
            return InstalledAgentSummary(id: manifest.id,
                                         name: displayName(for: manifest.id))
        }
        for id in Self.nativeAgentIDs {
            if case .builtin(true) = statuses[id] {
                result.append(InstalledAgentSummary(id: id, name: displayName(for: id)))
            }
        }
        if case .builtin(true) = statuses["omp"] ?? .builtin(available: false) {
            result.append(InstalledAgentSummary(id: "omp", name: "omp"))
        }
        return result.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
    }

    func displayName(for id: String) -> String {
        let canonical = AgentIdMigration.canonical(id)
        if let name = registryAgents.first(where: {
            AgentIdMigration.canonical($0.id) == canonical
        })?.name {
            return name
        }
        switch canonical {
        case "claude-acp": return "Claude Code"
        case "codex-acp": return "Codex"
        case "opencode": return "OpenCode"
        case "pi": return "Pi"
        default: return id
        }
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

        var nativeAvailability: [String: Bool] = [:]
        for id in Self.nativeAgentIDs {
            nativeAvailability[id] = pathProbe(AgentDriverFactory.nativeBinary(for: id))
        }

        var newRows: [AgentRow] = [
            AgentRow(id: "omp", name: "omp (Oh My Pi)",
                     description: "Built-in: uses the omp binary on your PATH.",
                     latestVersion: nil)
        ]

        for id in Self.nativeAgentIDs {
            let registryAgent = registryAgents.first {
                AgentIdMigration.canonical($0.id) == id
            }
            let binary = AgentDriverFactory.nativeBinary(for: id)
            let available = nativeAvailability[id] == true
            newRows.append(AgentRow(
                id: id,
                name: registryAgent?.name ?? displayName(for: id),
                description: available
                    ? (registryAgent?.description
                       ?? "Built-in: uses the \(binary) binary on your PATH.")
                    : "Requires \(binary) on PATH",
                latestVersion: registryAgent?.version))
        }

        newRows += registryAgents.filter {
            !Self.isNative($0.id)
        }.map {
            AgentRow(id: $0.id, name: $0.name, description: $0.description,
                     latestVersion: $0.version)
        }
        .sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        rows = newRows

        let ompFound: Bool
        if let result = try? await shell.run("command -v omp",
                                             cwd: FileManager.default.temporaryDirectory) {
            ompFound = result.exitCode == 0
        } else {
            ompFound = false
        }
        var newStatuses: [String: RowStatus] = ["omp": .builtin(available: ompFound)]
        for id in Self.nativeAgentIDs {
            newStatuses[id] = .builtin(available: nativeAvailability[id] == true)
        }
        for agent in registryAgents where !Self.isNative(agent.id) {
            newStatuses[agent.id] = Self.rowStatus(
                AgentInstallStatus.resolve(
                    manifest: installStore.manifest(id: agent.id),
                    latestVersion: agent.version,
                    isSupported: agent.installMethod(platform: .current) != nil))
        }
        // Installed agents that vanished from the registry stay usable.
        for manifest in installStore.installedManifests()
        where !Self.isNative(manifest.id) && newStatuses[manifest.id] == nil {
            newStatuses[manifest.id] = .installed(manifest.version)
        }
        statuses = newStatuses
    }

    func install(_ id: String) async {
        guard !Self.isNative(id) else { return }
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

    private static func isNative(_ id: String) -> Bool {
        AgentDriverFactory.transportKind(for: id) == .native
    }

    nonisolated private static func defaultPathProbe(_ binary: String) -> Bool {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", "command -v \(binary)"]
        process.standardOutput = Pipe()
        process.standardError = Pipe()
        do {
            try process.run()
            process.waitUntilExit()
            return process.terminationStatus == 0
        } catch {
            return false
        }
    }
}
