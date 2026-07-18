import ArgumentParser
import Testing
@testable import tillerctl

@Test func rootExposesOnlyCanonicalPanelCommands() throws {
    let rootNames = Set(
        Tillerctl.configuration.subcommands.compactMap { $0.configuration.commandName }
    )
    #expect(rootNames.isDisjoint(with: [
        "new-split", "list-panels", "list-pane-surfaces",
        "focus-panel", "send", "send-key", "close-panel",
    ]))

    let panelHelp = Panel.helpMessage()
    for command in [
        "create", "split", "list", "write", "key",
        "read", "wait", "focus", "close",
    ] {
        #expect(panelHelp.contains("\n  \(command)"))
    }
}

@Test func explicitPanelTargetWinsThenEnvironmentIsUsed() throws {
    let environment = ["TILLER_PANE_ID": "environment-pane"]

    #expect(try requiredPanelTarget(
        explicit: "explicit-pane",
        environment: environment,
        key: "TILLER_PANE_ID",
        option: "--id"
    ) == "explicit-pane")
    #expect(try requiredPanelTarget(
        explicit: nil,
        environment: environment,
        key: "TILLER_PANE_ID",
        option: "--id"
    ) == "environment-pane")
    #expect(throws: ValidationError.self) {
        try requiredPanelTarget(
            explicit: nil,
            environment: [:],
            key: "TILLER_PANE_ID",
            option: "--id"
        )
    }
}

@Test func splitParsesOnlySupportedDirections() throws {
    let parsed = try Tillerctl.parseAsRoot([
        "panel", "split", "right", "--from", "source", "--cmd", "pi",
    ])
    let split = try #require(parsed as? Panel.Split)
    #expect(split.direction == .right)
    #expect(split.from == "source")
    #expect(split.cmd == "pi")

    #expect(throws: (any Error).self) {
        try Tillerctl.parseAsRoot(["panel", "split", "diagonal"])
    }
}

@Test func createdPaneOutputIsStable() throws {
    #expect(try createdPaneOutput(id: "pane-id", json: false) == "pane-id")
    #expect(try createdPaneOutput(id: "pane-id", json: true) == #"{"id":"pane-id"}"#)
}

@Test func panelWaitUsesNoDeadlineOrRoundedGracePeriod() {
    #expect(panelWaitTimeoutSeconds(timeoutMs: nil) == nil)
    #expect(panelWaitTimeoutSeconds(timeoutMs: 1_001) == 32)
    #expect(panelWaitTimeoutSeconds(timeoutMs: 5_000) == 35)
}
