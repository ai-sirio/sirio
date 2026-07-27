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
    private var nextGeneration = 0
    var trackedPaneCount: Int { states.count }

    mutating func requestScan(paneId: UUID) -> Decision {
        if var state = states[paneId] {
            guard state.inFlightGeneration != nil else {
                nextGeneration += 1
                state.generation = nextGeneration
                state.inFlightGeneration = nextGeneration
                states[paneId] = state
                return .start(generation: nextGeneration)
            }
            state.hasPendingFollowUp = true
            states[paneId] = state
            return .coalesce
        }

        nextGeneration += 1
        let state = PaneState(
            generation: nextGeneration,
            inFlightGeneration: nextGeneration
        )
        states[paneId] = state
        return .start(generation: nextGeneration)
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
    }

    func isResultStale(paneId: UUID, generation: Int) -> Bool {
        guard let state = states[paneId] else {
            return true
        }
        return generation != state.generation
    }
}

struct ForegroundProcessScanResult: Sendable {
    let agentId: String?
    let processTree: [ProcessNode]
}
