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
        Task { await self.refresh() }
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

        let ompFound: Bool
        if let result = try? await shell.run("command -v omp",
                                             cwd: FileManager.default.temporaryDirectory) {
            ompFound = result.exitCode == 0
        } else {
            ompFound = false
        }
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
