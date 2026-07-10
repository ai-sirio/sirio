import AppKit
import UserNotifications
import ApplicationServices
import CoreGraphics
import Network
import TillerCore

/// Probe TCC reale. Solo I/O di sistema, zero logica di presentazione
/// (che vive in PermissionPresentation, testata in TillerCore).
///
/// Nota su screenRecording/accessibility: macOS non distingue "mai chiesto"
/// da "negato" via API, quindi il probe persiste un flag "già richiesto" su
/// UserDefaults: preflight negativo + flag ⇒ denied, senza flag ⇒ notRequested.
struct SystemPermissionProbe: PermissionProbe {

    // MARK: - Status

    func status(for kind: PermissionKind) async -> PermissionStatus {
        switch kind {
        case .notifications:
            let settings = await UNUserNotificationCenter.current().notificationSettings()
            return switch settings.authorizationStatus {
            case .authorized, .provisional: .granted
            case .denied: .denied
            default: .notRequested
            }

        case .screenRecording:
            if CGPreflightScreenCaptureAccess() { return .granted }
            return hasRequested(kind) ? .denied : .notRequested

        case .accessibility:
            if AXIsProcessTrusted() { return .granted }
            return hasRequested(kind) ? .denied : .notRequested

        case .fullDiskAccess:
            // Nessuna API: il canary più affidabile è il database TCC stesso,
            // leggibile solo con Full Disk Access concesso.
            let tccPath = ("~/Library/Application Support/com.apple.TCC/TCC.db" as NSString)
                .expandingTildeInPath
            return FileHandle(forReadingAtPath: tccPath) != nil ? .granted : .checkManually

        case .automation:
            return automationStatus()

        case .localNetwork:
            // Nessuna API pubblica di check su macOS.
            return .checkManually
        }
    }

    // MARK: - Perform

    func perform(_ action: PermissionAction, for kind: PermissionKind) async {
        switch action {
        case .openSettings:
            openSystemSettings(for: kind)
        case .request, .triggerPrompt:
            await requestOrTrigger(kind)
        }
    }

    private func requestOrTrigger(_ kind: PermissionKind) async {
        switch kind {
        case .notifications:
            _ = try? await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound])

        case .screenRecording:
            markRequested(kind)
            _ = CGRequestScreenCaptureAccess()

        case .accessibility:
            markRequested(kind)
            let options = ["AXTrustedCheckOptionPrompt": true] as CFDictionary
            _ = AXIsProcessTrustedWithOptions(options)

        case .automation:
            // Apple Event innocuo a System Events: forza il prompt di consenso.
            // NSAppleScript è bloccante → fuori dal main actor.
            await Task.detached {
                let script = NSAppleScript(
                    source: "tell application \"System Events\" to count processes")
                script?.executeAndReturnError(nil)
            }.value

        case .localNetwork:
            // Breve browse Bonjour: tocca la rete locale e triggera il prompt.
            let browser = NWBrowser(
                for: .bonjour(type: "_http._tcp", domain: nil),
                using: NWParameters())
            browser.start(queue: .global())
            try? await Task.sleep(for: .seconds(2))
            browser.cancel()

        case .fullDiskAccess:
            // Mai raggiunto: PermissionPresentation mappa FDA solo su openSettings.
            openSystemSettings(for: kind)
        }
    }

    // MARK: - Automation (Apple Events)

    private func automationStatus() -> PermissionStatus {
        // Il consenso Automation è per-target: si sonda System Events, il
        // bersaglio più comune degli script degli agenti.
        var addr = AEAddressDesc()
        let bundleID = "com.apple.systemevents"
        let created = bundleID.utf8CString.withUnsafeBufferPointer { buf in
            AECreateDesc(typeApplicationBundleID, buf.baseAddress, buf.count - 1, &addr)
        }
        guard created == noErr else { return .checkManually }
        defer { AEDisposeDesc(&addr) }

        return switch AEDeterminePermissionToAutomateTarget(
            &addr, typeWildCard, typeWildCard, false) {
        case noErr: .granted
        case -1743: .denied        // errAEEventNotPermitted
        case -1744: .notRequested  // errAEEventWouldRequireUserConsent
        default: .checkManually    // es. procNotFound: System Events non attivo
        }
    }

    // MARK: - System Settings deep link

    private func openSystemSettings(for kind: PermissionKind) {
        let url: String = switch kind {
        case .notifications: "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
        case .screenRecording: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        case .accessibility: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        case .fullDiskAccess: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        case .automation: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
        case .localNetwork: "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"
        }
        if let url = URL(string: url) { NSWorkspace.shared.open(url) }
    }

    // MARK: - Flag "già richiesto" (per distinguere denied da notRequested)

    private func requestedKey(_ kind: PermissionKind) -> String {
        "permissions.requested.\(kind.rawValue)"
    }

    private func hasRequested(_ kind: PermissionKind) -> Bool {
        UserDefaults.standard.bool(forKey: requestedKey(kind))
    }

    private func markRequested(_ kind: PermissionKind) {
        UserDefaults.standard.set(true, forKey: requestedKey(kind))
    }
}
