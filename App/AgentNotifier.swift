import Foundation
import AppKit
import UserNotifications
import TillerCore

/// Thin wrapper attorno a UNUserNotificationCenter + delegate per il click.
/// La decisione (notificare o no) è in NotificationPolicy (TillerCore), chiamata
/// da AppModel prima di postare. Questo tipo fa solo I/O di sistema.
final class AgentNotifier: NSObject, UNUserNotificationCenterDelegate, @unchecked Sendable {

    /// Chiamato quando l'utente clicka una notifica. Riceve il paneId estratto
    /// da userInfo. Wired da AppModel in bootstrap → seleziona il worktree.
    var onActivatePane: ((UUID) -> Void)?

    private let keyRequested = "hasRequestedNotifyAuth"

    /// Richiede autorizzazione (.alert + .sound) una sola volta, in modo
    /// idempotente, persistendo il flag su UserDefaults. Non re-prompta su
    /// denial. Chiamato da AppModel.spawnAgent (contestuale, non a launch).
    func ensureAuthorization() async {
        let defaults = UserDefaults.standard
        guard !defaults.bool(forKey: keyRequested) else { return }
        defaults.set(true, forKey: keyRequested)
        _ = try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound])
    }

    /// Posta una notifica se l'autorizzazione è concessa; silenzioso altrimenti.
    /// Identificatore per-pane: l'ultima notifica sostituisce la precedente
    /// dello stesso pane nel Notification Center.
    func post(_ payload: NotificationPayload) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            guard settings.authorizationStatus == .authorized ||
                  settings.authorizationStatus == .provisional else { return }
            let content = UNMutableNotificationContent()
            content.title = payload.title
            content.body = payload.body
            content.sound = .default
            content.userInfo = [
                "paneId": payload.paneId.uuidString,
                "worktreeId": payload.worktreeId.uuidString,
            ]
            let request = UNNotificationRequest(
                identifier: "tiller.pane.\(payload.paneId.uuidString)",
                content: content,
                trigger: nil
            )
            center.add(request) { _ in }
        }
    }

    /// User-visible notification requested over the control socket
    /// (tillerctl notify --title …). Unlike agent-status notifications,
    /// each gets a unique identifier — they don't replace each other.
    func postUser(title: String, subtitle: String?, body: String) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            guard settings.authorizationStatus == .authorized ||
                  settings.authorizationStatus == .provisional else { return }
            let content = UNMutableNotificationContent()
            content.title = title
            if let subtitle { content.subtitle = subtitle }
            content.body = body
            content.sound = .default
            let request = UNNotificationRequest(
                identifier: "tiller.user.\(UUID().uuidString)",
                content: content, trigger: nil
            )
            center.add(request) { _ in }
        }
    }

    /// Delivered Tiller notifications still in Notification Center.
    func deliveredNotifications() async -> [[String: String]] {
        let delivered = await UNUserNotificationCenter.current().deliveredNotifications()
        let iso = ISO8601DateFormatter()
        return delivered.map { n in
            [
                "title": n.request.content.title,
                "subtitle": n.request.content.subtitle,
                "body": n.request.content.body,
                "date": iso.string(from: n.date),
            ]
        }
    }

    func clearDelivered() {
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
    }

    // MARK: - UNUserNotificationCenterDelegate

    /// Click sulla notifica (azione default) → estrae il paneId e lo passa a
    /// onActivatePane, che focusa l'app sul worktree contenitore.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        completionHandler: @escaping () -> Void
    ) {
        if let raw = response.notification.request.content.userInfo["paneId"] as? String,
           let paneId = UUID(uuidString: raw) {
            onActivatePane?(paneId)
        }
        completionHandler()
    }

    /// Quando l'app è attiva, mostra comunque banner+sound per le notifiche
    /// postate (la soppressione delle transizioni visibili è già gestita dalla
    /// policy prima del post).
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }
}
