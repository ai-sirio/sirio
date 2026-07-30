import Foundation
import TillerCore
import TillerWorkspace

@MainActor
final class WorkspaceContentRegistry: WorkspaceHostProvider {
    private var hosts: [WorkspaceTabID: WorkspaceContentHost] = [:]
    private var generations: [WorkspaceTabID: ResourceGenerationID] = [:]
    private var phases: [WorkspaceTabID: ContentPhase] = [:]

    init() {}

    func host(for tabID: WorkspaceTabID) -> WorkspaceContentHost? { hosts[tabID] }

    func adopt(_ host: WorkspaceContentHost, tab: WorkspaceTab,
               generation: ResourceGenerationID) {
        hosts[tab.id] = host
        generations[tab.id] = generation
        phases[tab.id] = .dormant
    }

    func release(tabID: WorkspaceTabID) async {
        guard let host = hosts.removeValue(forKey: tabID) else { return }
        generations.removeValue(forKey: tabID)
        phases.removeValue(forKey: tabID)
        if let releaser = host as? any WorkspaceRuntimeReleaser {
            await releaser.releaseRuntime()
        }
    }

    func generation(for tabID: WorkspaceTabID) -> ResourceGenerationID? {
        generations[tabID]
    }

    var liveHostCount: Int { hosts.count }

    @discardableResult
    func publish(tabID: WorkspaceTabID, generation: ResourceGenerationID,
                 phase: ContentPhase) -> Bool {
        guard generations[tabID] == generation else { return false }
        phases[tabID] = phase
        return true
    }

    func phase(for tabID: WorkspaceTabID) -> ContentPhase? { phases[tabID] }

    @discardableResult
    func replaceGeneration(tabID: WorkspaceTabID, generation: ResourceGenerationID) -> Bool {
        guard hosts[tabID] != nil else { return false }
        generations[tabID] = generation
        phases[tabID] = .dormant
        return true
    }
}
