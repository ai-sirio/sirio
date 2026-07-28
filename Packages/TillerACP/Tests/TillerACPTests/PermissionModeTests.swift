import Testing
@testable import TillerACP

struct PermissionModeTests {
    @Test func claudeMapping() {
        #expect(PermissionMode.ask.claudeValue == "default")
        #expect(PermissionMode.acceptEdits.claudeValue == "acceptEdits")
        #expect(PermissionMode.plan.claudeValue == "plan")
        #expect(PermissionMode.fullAuto.claudeValue == "acceptEdits")
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
        // Native, but its RPC has no mode call and its CLI no mode flag.
        #expect(PermissionMode.supported(byDriverFor: "pi").isEmpty)
    }

    @Test func thePillIsHiddenForDriversWithoutModes() {
        #expect(PermissionMode.pillSelection(forAgent: "pi", requested: .ask) == nil)
        #expect(PermissionMode.pillSelection(forAgent: "omp", requested: .ask) == nil)
        #expect(PermissionMode.pillSelection(forAgent: "claude-acp", requested: .plan)
            == .plan)
        // A persisted mode the driver dropped falls back instead of vanishing.
        #expect(PermissionMode.pillSelection(forAgent: "codex-acp", requested: .plan)
            == .ask)
    }

    @Test func bridgesToSessionMode() {
        let mode = PermissionMode.acceptEdits.sessionMode
        #expect(mode.id == "acceptEdits")
        #expect(!mode.name.isEmpty)
    }

    @Test func displayNamesAreHumanReadable() {
        #expect(PermissionMode.ask.displayName == "Ask")
        #expect(PermissionMode.acceptEdits.displayName == "Accept edits")
        #expect(PermissionMode.plan.displayName == "Plan")
        #expect(PermissionMode.fullAuto.displayName == "Full auto")
    }
}
