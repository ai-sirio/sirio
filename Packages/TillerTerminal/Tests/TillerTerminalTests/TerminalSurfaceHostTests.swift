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
