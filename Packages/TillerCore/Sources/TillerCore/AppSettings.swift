import Foundation

/// App-preference bounds and defaults. Foundation-only so the timer can never
/// be driven by a corrupt or out-of-range stored interval.
public enum AppSettings {
    public static let defaultRefreshSeconds = 300
    public static let refreshRange: ClosedRange<Int> = 60...3600

    /// Clamp a stored refresh interval (seconds) into the allowed range.
    public static func clampRefresh(_ seconds: Int) -> Int {
        min(max(seconds, refreshRange.lowerBound), refreshRange.upperBound)
    }
    
    /// UserDefaults key for the "resume agent sessions on launch" toggle.
    /// Missing value means enabled (default true).
    public static let resumeAgentSessionsKey = "resumeAgentSessions"

    /// UserDefaults key for the control-socket toggle. Missing value means
    /// enabled (default true) — disabling it also disables agent hooks.
    public static let controlSocketEnabledKey = "controlSocket.enabled"
 
    /// UserDefaults key per il toggle di auto-naming di tab/agenti. Missing
    /// value significa disabilitato (default false) — a differenza degli altri
    /// toggle qui, opt-in perché genera chiamate CLI extra a carico dell'utente.
    public static let autoNamingEnabledKey = "autoNaming.enabled"
    public static let translucencyEnabledKey = "appearance.translucencyEnabled"

    /// Resolve whether the control socket should start.
    /// TILLER_SOCKET_ENABLE (1/0, true/false, on/off — case-insensitive)
    /// overrides the stored preference; anything else falls through.
    public static func controlSocketEnabled(defaultsValue: Bool?, env: [String: String]) -> Bool {
        if let raw = env["TILLER_SOCKET_ENABLE"] {
            switch raw.lowercased() {
            case "1", "true", "on": return true
            case "0", "false", "off": return false
            default: break
            }
        }
        return defaultsValue ?? true
    }

    /// Resolve whether auto-naming is enabled. Missing value means disabled.
    public static func autoNamingEnabled(defaultsValue: Bool?) -> Bool {
        defaultsValue ?? false
    }

    public static func translucencyEnabled(defaultsValue: Bool?) -> Bool {
        defaultsValue ?? false
    }

    /// UserDefaults key for the auto-naming summarizer agent (an AgentCatalog
    /// short id). Missing or empty value means the default, "claude".
    /// Validation against the actual adapter list happens at the App layer
    /// (TillerCore does not know the catalog).
    public static let summarizerAgentIdKey = "autoNaming.summarizerAgentId"
    public static let defaultSummarizerAgentId = "claude"

    /// Resolve the stored summarizer agent id, falling back to the default
    /// for missing or empty values.
    public static func summarizerAgentId(defaultsValue: String?) -> String {
        guard let id = defaultsValue, !id.isEmpty else {
            return defaultSummarizerAgentId
        }
        return id
    }

    /// UserDefaults key for the app appearance (AppAppearance rawValue).
    /// Missing value means `.system`.
    public static let appearanceThemeKey = "appearance.theme"

    /// UserDefaults key for the worktree ids whose terminal hosts were
    /// mounted at last quit, restored on next launch. Stored as an array
    /// of UUID strings.
    public static let openWorktreeIdsKey = "session.openWorktreeIds"

    /// UserDefaults key for the previously selected worktree id, restored
    /// alongside openWorktreeIdsKey on launch.
    public static let selectedWorktreeIdKey = "session.selectedWorktreeId"

    /// UserDefaults key for the max number of worktree terminal hosts kept
    /// mounted (PTYs alive) at once, oldest-idle evicted first via
    /// WorktreeMountPolicy. Missing/0 means unlimited — current behavior,
    /// opt-in only since eviction terminates a worktree's PTYs.
    public static let maxMountedWorktreesKey = "session.maxMountedWorktrees"

    /// UserDefaults key for how many chat conversations to keep per worktree.
    /// 0 means unlimited. Pruning runs once at bootstrap.
    public static let chatHistoryRetentionKey = "chat.history.retentionCount"
    public static let defaultChatHistoryRetention = 50

    /// UserDefaults key for the os_signpost metrics gate. Default: off.
    /// Enable via `defaults write dev.tiller debug.signpostMetrics -bool YES`.
    public static let signpostMetricsKey = "debug.signpostMetrics"

    /// UserDefaults key for the terminal font size in points.
    public static let terminalFontSizeKey = "appearance.terminalFontSize"
    public static let defaultTerminalFontSize = 13
    public static let terminalFontSizeRange: ClosedRange<Int> = 9...24

    /// Icon theme for the Files explorer in the right panel. Stores a
    /// FileIconTheme raw value; default is sfSymbols.
    public static let fileIconThemeKey = "appearance.fileIconTheme"

    /// Clamp a stored terminal font size into the allowed range.
    public static func clampTerminalFontSize(_ size: Int) -> Int {
        min(max(size, terminalFontSizeRange.lowerBound), terminalFontSizeRange.upperBound)
    }
    public static let rightPanelVisibleKey = "rightPanel.visible"
    public static let rightPanelWidthKey = "rightPanel.width"
    public static let rightPanelModeKey = "rightPanel.mode"

    public static let defaultRightPanelVisible = false
    public static let defaultRightPanelWidth = 320.0
    public static let rightPanelWidthRange: ClosedRange<Double> = 240...400

    public static func clampRightPanelWidth(_ width: Double) -> Double {
        min(max(width, rightPanelWidthRange.lowerBound), rightPanelWidthRange.upperBound)
    }

    public static let defaultSidebarWidth = 260.0
    public static let sidebarWidthRange: ClosedRange<Double> = 240...320
    public static func clampSidebarWidth(_ width: Double) -> Double {
        min(max(width, sidebarWidthRange.lowerBound), sidebarWidthRange.upperBound)
    }
}
