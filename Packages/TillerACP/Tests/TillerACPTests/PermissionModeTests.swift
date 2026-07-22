import Testing
@testable import TillerACP

struct PermissionModeTests {
    @Test func claudeMapping() {
        #expect(PermissionMode.ask.claudeValue == "default")
        #expect(PermissionMode.acceptEdits.claudeValue == "acceptEdits")
        #expect(PermissionMode.plan.claudeValue == "plan")
        #expect(PermissionMode.fullAuto.claudeValue == "bypassPermissions")
    }

    @Test func codexMapping() {
        #expect(PermissionMode.ask.codexApprovalPolicy == "untrusted")
        #expect(PermissionMode.acceptEdits.codexApprovalPolicy == "on-request")
        #expect(PermissionMode.fullAuto.codexApprovalPolicy == "never")
        // Security invariant from the spec: full auto never escapes the workspace.
        #expect(PermissionMode.fullAuto.codexSandbox == "workspace-write")
    }

    @Test func supportedModesPerDriver() {
        #expect(PermissionMode.supported(byDriverFor: "claude-acp")
            == [.ask, .acceptEdits, .plan, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "codex-acp")
            == [.ask, .acceptEdits, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "opencode")
            == [.ask, .acceptEdits, .fullAuto])
        #expect(PermissionMode.supported(byDriverFor: "omp").isEmpty) // ACP: no dropdown
    }

    @Test func bridgesToSessionMode() {
        let mode = PermissionMode.acceptEdits.sessionMode
        #expect(mode.id == "acceptEdits")
        #expect(!mode.name.isEmpty)
    }
}
