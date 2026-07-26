import Foundation
import Testing
import TillerCore
import TillerTerminal
@testable import Tiller

@Suite(.serialized)
@MainActor
struct ProcessScanCoordinatorTests {
    @Test func firstRequestStartsScan() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        #expect(coordinator.requestScan(paneId: paneId) == .start(generation: 1))
    }

    @Test func concurrentRequestsRecordOnlyOneFollowUp() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        #expect(coordinator.requestScan(paneId: paneId) == .start(generation: 1))
        for _ in 0..<10 {
            #expect(coordinator.requestScan(paneId: paneId) == .coalesce)
        }

        let followUp = coordinator.finishScan(paneId: paneId, generation: 1)
        #expect(followUp)
        #expect(coordinator.requestScan(paneId: paneId) == .start(generation: 2))
        let secondFollowUp = coordinator.finishScan(paneId: paneId, generation: 2)
        #expect(secondFollowUp == false)
    }

    @Test func finishingWithFollowUpStartsItOnce() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        _ = coordinator.requestScan(paneId: paneId)
        _ = coordinator.requestScan(paneId: paneId)

        let followUp = coordinator.finishScan(paneId: paneId, generation: 1)
        #expect(followUp)
        let repeatedFinish = coordinator.finishScan(paneId: paneId, generation: 1)
        #expect(repeatedFinish == false)
        #expect(coordinator.requestScan(paneId: paneId) == .start(generation: 2))
    }

    @Test func finishingWithoutFollowUpReturnsFalse() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        _ = coordinator.requestScan(paneId: paneId)

        let followUp = coordinator.finishScan(paneId: paneId, generation: 1)
        #expect(followUp == false)
    }

    @Test func olderGenerationIsStale() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        _ = coordinator.requestScan(paneId: paneId)
        _ = coordinator.requestScan(paneId: paneId)
        _ = coordinator.finishScan(paneId: paneId, generation: 1)
        _ = coordinator.requestScan(paneId: paneId)

        #expect(coordinator.isResultStale(paneId: paneId, generation: 1))
        #expect(coordinator.isResultStale(paneId: paneId, generation: 2) == false)
    }

    @Test func closedPaneDropsStateAndRejectsResults() {
        var coordinator = ProcessScanCoordinator()
        let paneId = UUID()

        _ = coordinator.requestScan(paneId: paneId)
        coordinator.paneClosed(paneId: paneId)

        #expect(coordinator.isResultStale(paneId: paneId, generation: 1))
        #expect(coordinator.requestScan(paneId: paneId) == .ignore)
    }

    @Test func panesHaveIndependentInFlightScans() {
        var coordinator = ProcessScanCoordinator()
        let firstPane = UUID()
        let secondPane = UUID()

        #expect(coordinator.requestScan(paneId: firstPane) == .start(generation: 1))
        #expect(coordinator.requestScan(paneId: secondPane) == .start(generation: 1))
        #expect(coordinator.requestScan(paneId: firstPane) == .coalesce)
        let secondPaneFollowUp = coordinator.finishScan(paneId: secondPane, generation: 1)
        #expect(secondPaneFollowUp == false)
        #expect(coordinator.isResultStale(paneId: firstPane, generation: 1) == false)
    }

    @Test func processOwnedContentSignalStartsExactlyOneScan() async {
        let paneId = UUID()
        let result = ForegroundProcessScanResult(
            agentId: "codex",
            processTree: [ProcessNode(pid: 10, name: "codex", children: [])]
        )
        let probe = ProcessScanProbe(result: result, blocksFirstCall: true)
        let model = makeModel(scanner: { _ in await probe.scan() })
        model.agentActivity.processIdentified(paneId: paneId, agentId: "codex", now: Date())

        model.handleContentSignal(paneId: paneId, tailText: "")
        await probe.waitForFirstCall()
        await probe.releaseFirstCall()
        await waitUntil { await probe.callCount >= 1 }
        try? await Task.sleep(for: .milliseconds(50))

        #expect(await probe.callCount == 1)
    }

    @Test func unchangedProcessScanDoesNotInvalidateObservedState() async {
        let paneId = UUID()
        let result = ForegroundProcessScanResult(
            agentId: "codex",
            processTree: [ProcessNode(pid: 10, name: "codex", children: [])]
        )
        let probe = ProcessScanProbe(result: result, blocksFirstCall: false)
        let model = makeModel(scanner: { _ in await probe.scan() })
        model.agentActivity.processIdentified(paneId: paneId, agentId: "codex", now: Date())
        model.paneProcessTrees[paneId] = result.processTree

        let invalidationCounter = ObservationInvalidationCounter()
        withObservationTracking({
            _ = model.agentActivity
            _ = model.paneProcessTrees
        }, onChange: {
            Task { @MainActor in
                invalidationCounter.value += 1
            }
        })

        model.handleContentSignal(paneId: paneId, tailText: "")
        await waitUntil { await probe.callCount >= 1 }
        try? await Task.sleep(for: .milliseconds(50))

        #expect(invalidationCounter.value == 0)
        #expect(model.agentActivity.agentId(paneId: paneId) == "codex")
        #expect(model.paneProcessTrees[paneId] == result.processTree)
    }

    @Test func scanResultAfterPaneCloseIsIgnored() async {
        let paneId = UUID()
        let result = ForegroundProcessScanResult(
            agentId: "codex",
            processTree: [ProcessNode(pid: 10, name: "codex", children: [])]
        )
        let probe = ProcessScanProbe(result: result, blocksFirstCall: true)
        let model = makeModel(scanner: { _ in await probe.scan() })

        model.handleContentSignal(paneId: paneId, tailText: "")
        await probe.waitForFirstCall()
        model.paneClosed(paneId: paneId)
        await probe.releaseFirstCall()
        await waitUntil { await probe.callCount >= 1 }
        try? await Task.sleep(for: .milliseconds(50))

        #expect(model.agentActivity.agentId(paneId: paneId) == nil)
        #expect(model.agentActivity.processOwnedPanes.contains(paneId) == false)
        #expect(model.paneProcessTrees[paneId] == nil)
    }

    private func makeModel(
        scanner: @escaping AppModel.ForegroundProcessScanner
    ) -> AppModel {
        AppModel(foregroundProcessScanner: scanner)
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async {
        for _ in 0..<100 {
            if await condition() { return }
            try? await Task.sleep(for: .milliseconds(5))
        }
    }
}

private actor ProcessScanProbe {
    let result: ForegroundProcessScanResult?
    let blocksFirstCall: Bool
    private(set) var callCount = 0
    private var firstCallContinuation: CheckedContinuation<Void, Never>?
    private var firstCallStartedContinuation: CheckedContinuation<Void, Never>?

    init(result: ForegroundProcessScanResult?, blocksFirstCall: Bool) {
        self.result = result
        self.blocksFirstCall = blocksFirstCall
    }

    func scan() async -> ForegroundProcessScanResult? {
        callCount += 1
        firstCallStartedContinuation?.resume()
        firstCallStartedContinuation = nil
        if blocksFirstCall && callCount == 1 {
            await withCheckedContinuation { continuation in
                firstCallContinuation = continuation
            }
        }
        return result
    }

    func waitForFirstCall() async {
        guard callCount == 0 else { return }
        await withCheckedContinuation { continuation in
            firstCallStartedContinuation = continuation
        }
    }

    func releaseFirstCall() {
        firstCallContinuation?.resume()
        firstCallContinuation = nil
    }
}
@MainActor
private final class ObservationInvalidationCounter {
    var value = 0
}
