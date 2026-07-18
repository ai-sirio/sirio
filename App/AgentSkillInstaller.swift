import Foundation
import TillerCore

/// Opens Terminal.app and immediately runs the tiller skill install
/// command. Not unit-tested because this is a real Terminal.app side effect;
/// the command/script it runs is
/// covered by AgentSkillInstallTests in TillerCore.
/// Opens Terminal to install the public canonical `tiller` skill. Launched worktrees provision it automatically.
enum AgentSkillInstaller {
    static func openTerminalAndInstall() {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", AgentSkillInstall.appleScript()]
        try? process.run()
    }
}
