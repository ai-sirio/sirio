import Testing
@testable import TillerCore

@Test func foundShowsAvailableWithVersion() {
    #expect(UpdateState.idle.found(version: "0.3.0") == .available(version: "0.3.0"))
}

@Test func manualCheckStartsOnlyFromRestingStates() {
    #expect(UpdateState.idle.manualCheckStarted() == .checking)
    #expect(UpdateState.upToDate.manualCheckStarted() == .checking)
    #expect(UpdateState.error(message: "x").manualCheckStarted() == .checking)
    let downloading = UpdateState.downloading(version: "0.3.0", progress: 0.5)
    #expect(downloading.manualCheckStarted() == downloading)
}

@Test func notFoundAfterManualCheckShowsUpToDate() {
    #expect(UpdateState.checking.notFound() == .upToDate)
}

@Test func notFoundAfterBackgroundCheckStaysSilent() {
    #expect(UpdateState.idle.notFound() == .idle)
    #expect(UpdateState.upToDate.notFound() == .idle)
}

@Test func downloadStartsOnlyFromAvailable() {
    let available = UpdateState.available(version: "0.3.0")
    #expect(available.downloadStarted() == .downloading(version: "0.3.0", progress: 0))
    #expect(UpdateState.idle.downloadStarted() == .idle)
}

@Test func progressIsMonotoneAndClamped() {
    let state = UpdateState.downloading(version: "0.3.0", progress: 0.5)
    #expect(state.downloadProgressed(0.7) == .downloading(version: "0.3.0", progress: 0.7))
    #expect(state.downloadProgressed(0.3) == state)
    #expect(state.downloadProgressed(1.5) == .downloading(version: "0.3.0", progress: 1.0))
    #expect(state.downloadProgressed(-1) == state)
}

@Test func progressIgnoredOutsideDownloading() {
    #expect(UpdateState.idle.downloadProgressed(0.5) == .idle)
}

@Test func downloadCompletedFromDownloadingOrAvailable() {
    // .available copre il caso Sparkle "update già scaricato in un run precedente":
    // showReady arriva senza callback di download.
    #expect(UpdateState.downloading(version: "0.3.0", progress: 1).downloadCompleted()
        == .readyToInstall(version: "0.3.0"))
    #expect(UpdateState.available(version: "0.3.0").downloadCompleted()
        == .readyToInstall(version: "0.3.0"))
    #expect(UpdateState.idle.downloadCompleted() == .idle)
}

@Test func failedAlwaysShowsError() {
    #expect(UpdateState.downloading(version: "0.3.0", progress: 0.5).failed(message: "rete")
        == .error(message: "rete"))
}

@Test func dismissedAlwaysReturnsToIdle() {
    #expect(UpdateState.available(version: "0.3.0").dismissed() == .idle)
    #expect(UpdateState.error(message: "x").dismissed() == .idle)
    #expect(UpdateState.upToDate.dismissed() == .idle)
}
