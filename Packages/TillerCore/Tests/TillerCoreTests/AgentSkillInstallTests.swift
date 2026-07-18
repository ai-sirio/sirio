import Testing
@testable import TillerCore

@Test func installCommandTargetsCanonicalTillerSkill() {
    #expect(
        AgentSkillInstall.command
            == "npx skills add e-palmisano/tiller --skill tiller -a claude-code,codex,opencode,pi -y"
    )
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
