import AppKit
import SwiftUI
import Testing
import TillerCore
@testable import TillerTerminal

@MainActor
struct TerminalSurfaceHostTests {
    private let contentID = TerminalContentID()
    private let configuration = TerminalSurfaceConfiguration()

    @Test func oneContentIdProducesOneStableController() {
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        let first = host.viewController
        let second = host.viewController

        #expect(ObjectIdentifier(first) == ObjectIdentifier(second))
        #expect(host.contentID == contentID)
    }

    @Test func relaunchMintsANewGenerationUnderTheSameContentId() {
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)
        let firstGeneration = host.generationID
        let controller = host.viewController

        let secondGeneration = host.relaunch()

        #expect(secondGeneration != firstGeneration)
        #expect(host.generationID == secondGeneration)
        #expect(host.contentID == contentID)
        #expect(ObjectIdentifier(host.viewController) == ObjectIdentifier(controller))
    }

    @Test func teardownIsIdempotent() async {
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)
        let controller = host.viewController
        let generation = host.generationID

        await host.teardown()
        await host.teardown()

        #expect(ObjectIdentifier(host.viewController) == ObjectIdentifier(controller))
        #expect(host.generationID == generation)
        #expect(host.contentID == contentID)
    }

    /// An agent resume command has to embed the pane id it will report status
    /// under, but that id is this host's generation — minted here, after the
    /// caller would have had to build the string. The provider closes the gap
    /// by receiving the real pane id.
    @Test func theCommandProviderReceivesThePaneIdTheSurfacePublishes() {
        let seen = Box()
        let configuration = TerminalSurfaceConfiguration(
            commandProvider: { paneId in
                seen.value.append(paneId)
                return "claude --resume abc"
            })

        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        #expect(seen.value == [host.generationID.rawValue])
    }

    /// Relaunch mints a new pane id, so the command must be rebuilt for it —
    /// reusing the first one would point the agent's hooks at a dead pane.
    @Test func relaunchAsksTheCommandProviderAgainWithTheNewPaneId() {
        let seen = Box()
        let configuration = TerminalSurfaceConfiguration(
            commandProvider: { paneId in
                seen.value.append(paneId)
                return nil
            })
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        let second = host.relaunch()

        #expect(seen.value.count == 2)
        #expect(seen.value.last == second.rawValue)
    }

    /// An explicit command wins: the provider exists only to fill in a command
    /// that could not be built before the pane id existed.
    @Test func anExplicitCommandIsNotOverriddenByTheProvider() {
        let seen = Box()
        let configuration = TerminalSurfaceConfiguration(
            command: "zsh",
            commandProvider: { paneId in
                seen.value.append(paneId)
                return "claude --resume abc"
            })

        _ = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        #expect(seen.value.isEmpty)
    }

    private final class Box: @unchecked Sendable {
        var value: [UUID] = []
    }

    @Test func theSurfaceHostExposesNoSplitTreeApi() {
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        // The public seam is a single controller. There is no container child
        // through which a recursive SplitTree topology could be exposed.
        #expect(host.viewController.children.isEmpty)
    }

    @Test func terminalViewAccessibilityExposureIsUnchanged() {
        let host = TerminalSurfaceHost(contentID: contentID, configuration: configuration)

        // Keep Ghostty's TerminalSurfaceView on the direct NSHostingView path;
        // the host must not insert an AppKit wrapper that hides its AX tree.
        #expect(host.viewController.view is NSHostingView<AnyView>)
    }
}
