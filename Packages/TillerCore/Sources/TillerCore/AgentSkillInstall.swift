import Foundation

/// The command that installs the tillerctl-cli agent skill via
/// https://github.com/vercel-labs/skills, and the AppleScript that runs it
/// in a new Terminal.app window. Foundation-only (no AppKit) so it stays
/// unit-testable; App/AgentSkillInstaller.swift owns the actual Process
/// launch.
public enum AgentSkillInstall {
    public static let command =
        "npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y"

    /// AppleScript source that opens/activates Terminal.app and runs
    /// `command` in it. `command` has no user input interpolated by any
    /// caller in this codebase — always a static string — so no escaping
    /// is performed here.
    public static func appleScript(command: String = AgentSkillInstall.command) -> String {
        """
        tell application "Terminal"
            activate
            do script "\(command)"
        end tell
        """
    }
}
