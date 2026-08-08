import AppKit
import Foundation
import Observation
import TillerBrowser

@MainActor
@Observable
final class BrowserPermissionStore {
    static let defaultsKey = "browser.originGrants"
    private let defaults: UserDefaults
    /// Injectable so a test can state "there is no window" instead of depending
    /// on whether the test host happens to have one — a sheet nobody answers
    /// never returns.
    private let hostWindow: @MainActor () -> NSWindow?
    private(set) var grants: Set<OriginGrant>

    init(
        defaults: UserDefaults = .standard,
        hostWindow: @MainActor @escaping () -> NSWindow? = {
            MainWindowRef.shared.window ?? NSApp.windows.first(where: \.isVisible)
        }
    ) {
        self.defaults = defaults
        self.hostWindow = hostWindow
        if let data = defaults.data(forKey: Self.defaultsKey) {
            do {
                grants = try Set(JSONDecoder().decode([OriginGrant].self, from: data))
            } catch {
                NSLog("Tiller could not load browser origin grants: %@", error.localizedDescription)
                grants = []
            }
        } else {
            grants = []
        }
    }

    func requestAccess(worktreeID: UUID, url: URL) async -> Bool {
        guard let origin = OriginPolicy.origin(for: url) else { return false }
        let grant = OriginGrant(worktreeID: worktreeID, origin: origin)
        if grants.contains(grant) { return true }
        guard await confirm(origin: origin) else { return false }

        grants.insert(grant)
        persist()
        return true
    }

    /// Asks the human, and grants only on an actual answer.
    ///
    /// This used to be `NSAlert.runModal()`, which returns the default button's
    /// response **immediately when the app is not active** — so with Tiller in
    /// the background an agent was granted access to any origin in about two
    /// seconds, and the grant was persisted as though somebody had consented.
    /// That is precisely the situation the gate exists for: an agent running
    /// while nobody watches. A sheet resolves only when it is answered, and with
    /// no window to attach it to there is nobody to ask, so the answer is no.
    private func confirm(origin: String) async -> Bool {
        guard let window = hostWindow() else {
            NSLog("Tiller denied browser access to %@: no window available to ask in", origin)
            return false
        }
        let alert = NSAlert()
        alert.messageText = "Allow agent browser access?"
        alert.informativeText =
            "An agent wants to read and script \(origin). Allowing this lets it inspect and modify this site in the selected worktree."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Allow")
        alert.addButton(withTitle: "Deny")
        NSApp.activate(ignoringOtherApps: true)
        return await withCheckedContinuation { continuation in
            alert.beginSheetModal(for: window) { response in
                continuation.resume(returning: response == .alertFirstButtonReturn)
            }
        }
    }

    func revoke(_ grant: OriginGrant) {
        grants.remove(grant)
        persist()
    }

    func revokeAll(for worktreeID: UUID) {
        grants = grants.filter { $0.worktreeID != worktreeID }
        persist()
    }

    private func persist() {
        do {
            defaults.set(try JSONEncoder().encode(Array(grants)), forKey: Self.defaultsKey)
        } catch {
            NSLog("Tiller could not save browser origin grants: %@", error.localizedDescription)
        }
    }
}
