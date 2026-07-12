import Foundation
import TillerCore

/// Opens Terminal.app and immediately runs the tillerctl-cli skill install
/// command. Not unit-tested: App has no test target, and this is a real
/// side effect (launches an external app) — the command/script it runs is
/// covered by AgentSkillInstallTests in TillerCore.
enum AgentSkillInstaller {
    static func openTerminalAndInstall() {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
        process.arguments = ["-e", AgentSkillInstall.appleScript()]
        try? process.run()
    }
}
