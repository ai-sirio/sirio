import Foundation
import TillerCore

struct ProcessScanCoordinator {
    enum Decision: Equatable {
        case start(generation: Int)
        case coalesce
        case ignore
    }

    private struct PaneState {
        var generation: Int
        var inFlightGeneration: Int?
        var hasPendingFollowUp = false
    }

    private var states: [UUID: PaneState] = [:]
    private var closedPanes: Set<UUID> = []

    mutating func requestScan(paneId: UUID) -> Decision {
        guard !closedPanes.contains(paneId) else { return .ignore }

        if var state = states[paneId] {
            guard state.inFlightGeneration != nil else {
                state.generation += 1
                state.inFlightGeneration = state.generation
                states[paneId] = state
                return .start(generation: state.generation)
            }
            state.hasPendingFollowUp = true
            states[paneId] = state
            return .coalesce
        }

        let state = PaneState(generation: 1, inFlightGeneration: 1)
        states[paneId] = state
        return .start(generation: 1)
    }

    /// Finishes the current generation. The caller starts the next generation
    /// by requesting another scan when this returns true.
    mutating func finishScan(paneId: UUID, generation: Int) -> Bool {
        guard var state = states[paneId], state.inFlightGeneration == generation else {
            return false
        }
        guard state.hasPendingFollowUp else {
            state.inFlightGeneration = nil
            states[paneId] = state
            return false
        }
        state.hasPendingFollowUp = false
        state.inFlightGeneration = nil
        states[paneId] = state
        return true
    }

    mutating func paneClosed(paneId: UUID) {
        states[paneId] = nil
        closedPanes.insert(paneId)
    }

    func isResultStale(paneId: UUID, generation: Int) -> Bool {
        guard !closedPanes.contains(paneId), let state = states[paneId] else {
            return true
        }
        return generation != state.generation
    }
}

struct ForegroundProcessScanResult: Sendable {
    let agentId: String?
    let processTree: [ProcessNode]
}
