import Testing
@testable import TillerACP

@Suite struct AgentLaunchSpecTests {
    @Test func claudeCodeRunsPinnedAdapterThroughLoginShell() {
        let spec = AgentLaunchSpec.claudeCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments.count == 2)
        #expect(spec.arguments[0] == "-lc")
        #expect(spec.arguments[1] ==
            "exec npx -y @zed-industries/claude-code-acp@\(AgentLaunchSpec.claudeCodeACPVersion)")
    }

    @Test func pinnedVersionLooksLikeSemver() {
        let parts = AgentLaunchSpec.claudeCodeACPVersion.split(separator: ".")
        #expect(parts.count == 3)
        #expect(parts.allSatisfy { Int($0) != nil })
    }

    @Test func openCodeRunsNativeACPThroughLoginShell() {
        let spec = AgentLaunchSpec.openCode()
        #expect(spec.executable == "/bin/zsh")
        #expect(spec.arguments == ["-lc", "exec opencode acp"])
    }
}
