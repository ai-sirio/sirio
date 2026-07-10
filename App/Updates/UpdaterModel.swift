import AppKit
import Observation
import Sparkle
import TillerCore

/// Possiede SPUUpdater e fa da SPUUserDriver: nessuna finestra Sparkle,
/// ogni callback diventa una transizione di UpdateState resa dal toast.
@MainActor
@Observable
final class UpdaterModel: NSObject {
    private(set) var state: UpdateState = .idle

    private var updater: SPUUpdater?
    private var updateChoice: ((SPUUserUpdateChoice) -> Void)?
    private var installReply: (() -> Void)?
    private var expectedLength: UInt64 = 0
    private var receivedLength: UInt64 = 0

    func start() {
        guard updater == nil else { return }
        let updater = SPUUpdater(
            hostBundle: .main,
            applicationBundle: .main,
            userDriver: self,
            delegate: nil
        )
        do {
            try updater.start()
            self.updater = updater
        } catch {
            state = state.failed(message: error.localizedDescription)
        }
    }

    func checkForUpdates() {
        state = state.manualCheckStarted()
        updater?.checkForUpdates()
    }

    func download() {
        updateChoice?(.install)
        updateChoice = nil
    }

    func installAndRelaunch() {
        installReply?()
        installReply = nil
    }

    func dismiss() {
        updateChoice?(.dismiss)
        updateChoice = nil
        state = state.dismissed()
    }
}

extension UpdaterModel: SPUUserDriver {
    func show(
        _ request: SPUUpdatePermissionRequest,
        reply: @escaping (SUUpdatePermissionResponse) -> Void
    ) {
        reply(SUUpdatePermissionResponse(automaticUpdateChecks: true, sendSystemProfile: false))
    }

    func showUserInitiatedUpdateCheck(cancellation: @escaping () -> Void) {}

    func showUpdateFound(
        with appcastItem: SUAppcastItem,
        state: SPUUserUpdateState,
        reply: @escaping (SPUUserUpdateChoice) -> Void
    ) {
        updateChoice = reply
        self.state = self.state.found(version: appcastItem.displayVersionString)
    }

    func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {}

    func showUpdateReleaseNotesFailedToDownloadWithError(_ error: Error) {}

    func showUpdateNotFoundWithError(_ error: Error, acknowledgement: @escaping () -> Void) {
        state = state.notFound()
        acknowledgement()
    }

    func showUpdaterError(_ error: Error, acknowledgement: @escaping () -> Void) {
        state = state.failed(message: error.localizedDescription)
        acknowledgement()
    }

    func showDownloadInitiated(cancellation: @escaping () -> Void) {
        expectedLength = 0
        receivedLength = 0
        state = state.downloadStarted()
    }

    func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {
        expectedLength = expectedContentLength
    }

    func showDownloadDidReceiveData(ofLength length: UInt64) {
        receivedLength += length
        guard expectedLength > 0 else { return }
        state = state.downloadProgressed(Double(receivedLength) / Double(expectedLength))
    }

    func showDownloadDidStartExtractingUpdate() {}

    func showExtractionReceivedProgress(_ progress: Double) {}

    func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
        installReply = { reply(.install) }
        state = state.downloadCompleted()
    }

    func showInstallingUpdate(
        withApplicationTerminated applicationTerminated: Bool,
        retryTerminatingApplication: @escaping () -> Void
    ) {}

    func showUpdateInstalledAndRelaunched(_ relaunched: Bool, acknowledgement: @escaping () -> Void) {
        acknowledgement()
    }

    func showUpdateInProgress() {}

    func dismissUpdateInstallation() {
        state = state.dismissed()
    }
}
