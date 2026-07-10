import Foundation

/// I permessi macOS (TCC) rilevanti per Tiller. Servono soprattutto ai CLI
/// agent nei terminali: i processi figli ereditano il TCC envelope dell'app
/// host, quindi i permessi vanno concessi al bundle di Tiller.
public enum PermissionKind: String, CaseIterable, Identifiable, Sendable {
    case notifications
    case screenRecording
    case accessibility
    case fullDiskAccess
    case automation
    case localNetwork

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .notifications: "Notifications"
        case .screenRecording: "Screen Recording"
        case .accessibility: "Accessibility"
        case .fullDiskAccess: "Full Disk Access"
        case .automation: "Automation"
        case .localNetwork: "Local Network"
        }
    }

    /// SF Symbol per la riga.
    public var symbol: String {
        switch self {
        case .notifications: "bell.badge"
        case .screenRecording: "record.circle"
        case .accessibility: "accessibility"
        case .fullDiskAccess: "internaldrive"
        case .automation: "gearshape.2"
        case .localNetwork: "network"
        }
    }

    public var detail: String {
        switch self {
        case .notifications: "Alerts when agents finish or need input."
        case .screenRecording: "Screenshot, visual automation, and UI inspection tools."
        case .accessibility: "Keystroke injection, window control, and UI automation tools."
        case .fullDiskAccess: "Recommended when projects or worktrees touch macOS-protected folders."
        case .automation: "Apple Events for scripts that control other local apps."
        case .localNetwork: "Discovery and access for development servers on your network."
        }
    }
}

/// Stato TCC osservato. `checkManually` copre i permessi senza API di check
/// affidabile (Full Disk Access, Local Network) o stati non determinabili.
public enum PermissionStatus: Sendable, Equatable {
    case granted
    case denied
    case notRequested
    case checkManually

    public var badgeLabel: String {
        switch self {
        case .granted: "GRANTED"
        case .denied: "DENIED"
        case .notRequested: "NOT REQUESTED"
        case .checkManually: "CHECK MANUALLY"
        }
    }
}

/// L'unica azione mostrata per riga, decisa da PermissionPresentation.
public enum PermissionAction: Sendable, Equatable {
    case request
    case triggerPrompt
    case openSettings

    public var label: String {
        switch self {
        case .request: "Request"
        case .triggerPrompt: "Trigger Prompt"
        case .openSettings: "Open Settings"
        }
    }
}

/// Mapping puro (kind, status) → azione. Regole:
/// - il prompt di sistema si può triggerare solo finché non è mai stato
///   mostrato (macOS non lo ripropone dopo una risposta);
/// - Full Disk Access non ha alcuna API di richiesta: solo System Settings;
/// - Local Network non ha API di check: il trigger Bonjour resta l'unica leva.
public enum PermissionPresentation {
    public static func action(for kind: PermissionKind, status: PermissionStatus) -> PermissionAction {
        switch kind {
        case .fullDiskAccess:
            .openSettings
        case .localNetwork:
            .triggerPrompt
        case .automation:
            switch status {
            case .notRequested, .checkManually: .triggerPrompt
            case .granted, .denied: .openSettings
            }
        case .notifications, .screenRecording, .accessibility:
            status == .notRequested ? .request : .openSettings
        }
    }
}
