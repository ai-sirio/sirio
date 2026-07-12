import Testing
@testable import TillerCore

@Test func installCommandIsExactExpectedString() {
    #expect(AgentSkillInstall.command ==
        "npx skills add e-palmisano/tiller --skill tillerctl-cli -a claude-code,codex,opencode -y")
}

@Test func appleScriptWrapsCommandInDoScript() {
    let script = AgentSkillInstall.appleScript(command: "echo hi")
    #expect(script.contains("tell application \"Terminal\""))
    #expect(script.contains("activate"))
    #expect(script.contains("do script \"echo hi\""))
}

@Test func appleScriptDefaultsToInstallCommand() {
    let script = AgentSkillInstall.appleScript()
    #expect(script.contains(AgentSkillInstall.command))
}
