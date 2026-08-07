import AppKit
import Foundation
import Observation
import TillerBrowser

@MainActor
@Observable
final class BrowserPermissionStore {
    static let defaultsKey = "browser.originGrants"
    private let defaults: UserDefaults
    private(set) var grants: Set<OriginGrant>

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
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

        let alert = NSAlert()
        alert.messageText = "Allow agent browser access?"
        alert.informativeText =
            "An agent wants to run JavaScript on \(origin). Allowing this lets it inspect and modify this site in the selected worktree."
        alert.alertStyle = .warning
        alert.addButton(withTitle: "Allow")
        alert.addButton(withTitle: "Deny")
        guard alert.runModal() == .alertFirstButtonReturn else { return false }

        grants.insert(grant)
        persist()
        return true
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
