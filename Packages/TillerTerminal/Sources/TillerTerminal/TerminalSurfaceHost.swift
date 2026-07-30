import AppKit
import Foundation
import SwiftUI
import TillerCore

/// Configuration for one terminal surface. The callbacks use the same
/// content signals as `PtyTerminalPane`; the surface host adds no workspace
/// or split-tree concerns around them.
public struct TerminalSurfaceConfiguration: Sendable {
    public let workingDirectory: String?
    public let command: String?
    public let extraEnvironment: [String: String]
    public let initialScrollback: Data?
    public let onScrollback: (@Sendable (UUID, Data) async -> Void)?
    public let onTitleChange: (@Sendable (UUID, String) -> Void)?
    public let onContentSignal: (@Sendable (UUID, String) -> Void)?
    public let onOpenURL: (@Sendable (UUID, String) -> Void)?

    public init(
        workingDirectory: String? = nil,
        command: String? = nil,
        extraEnvironment: [String: String] = [:],
        initialScrollback: Data? = nil,
        onScrollback: (@Sendable (UUID, Data) async -> Void)? = nil,
        onTitleChange: (@Sendable (UUID, String) -> Void)? = nil,
        onContentSignal: (@Sendable (UUID, String) -> Void)? = nil,
        onOpenURL: (@Sendable (UUID, String) -> Void)? = nil
    ) {
        self.workingDirectory = workingDirectory
        self.command = command
        self.extraEnvironment = extraEnvironment
        self.initialScrollback = initialScrollback
        self.onScrollback = onScrollback
        self.onTitleChange = onTitleChange
        self.onContentSignal = onContentSignal
        self.onOpenURL = onOpenURL
    }
}

/// Owns exactly one terminal surface for one persistent content identity.
///
/// The AppKit controller is stable for the lifetime of this host. A relaunch
/// replaces only the SwiftUI surface content and mints a new runtime
/// generation, so the PTY generation cannot become the content identity.
@MainActor
public final class TerminalSurfaceHost {
    public let contentID: TerminalContentID
    public private(set) var generationID: ResourceGenerationID

    private let configuration: TerminalSurfaceConfiguration
    private let controller: NSHostingController<AnyView>
    private var didTeardown = false

    public var viewController: NSViewController { controller }

    public init(contentID: TerminalContentID, configuration: TerminalSurfaceConfiguration) {
        self.contentID = contentID
        self.configuration = configuration
        let generationID = ResourceGenerationID()
        self.generationID = generationID
        self.controller = NSHostingController(
            rootView: Self.rootView(configuration: configuration, generationID: generationID)
        )
    }

    /// Replaces the PTY-backed surface while preserving this content's host
    /// controller and content identity.
    @discardableResult
    public func relaunch() -> ResourceGenerationID {
        let generationID = ResourceGenerationID()
        self.generationID = generationID
        didTeardown = false
        controller.rootView = Self.rootView(
            configuration: configuration,
            generationID: generationID
        )
        return generationID
    }

    /// Focuses Ghostty's deepest focusable terminal view, if the host is
    /// attached to a window. The host has no focus or topology wrapper in the
    /// view hierarchy.
    @discardableResult
    public func focusTerminal() -> Bool {
        guard !didTeardown,
              let window = controller.view.window,
              let candidate = Self.focusCandidate(in: controller.view) else {
            return false
        }
        return window.makeFirstResponder(candidate)
    }

    /// Removes the active SwiftUI surface and its PTY lifecycle from the
    /// hierarchy. Repeated teardown calls are intentionally no-ops.
    public func teardown() async {
        guard !didTeardown else { return }
        didTeardown = true
        controller.rootView = AnyView(EmptyView())
        controller.viewIfLoaded?.removeFromSuperview()
        await Task.yield()
    }

    private static func rootView(
        configuration: TerminalSurfaceConfiguration,
        generationID: ResourceGenerationID
    ) -> AnyView {
        AnyView(
            PtyTerminalPane(
                workingDirectory: configuration.workingDirectory,
                command: configuration.command,
                paneId: generationID.rawValue,
                initialScrollback: configuration.initialScrollback,
                extraEnvironment: configuration.extraEnvironment,
                onScrollback: configuration.onScrollback,
                onTitleChange: configuration.onTitleChange,
                onContentSignal: configuration.onContentSignal,
                onOpenURL: configuration.onOpenURL
            )
            .id(generationID.rawValue)
        )
    }

    private static func focusCandidate(in view: NSView) -> NSView? {
        for subview in view.subviews.reversed() {
            if let candidate = focusCandidate(in: subview) { return candidate }
        }
        return view.acceptsFirstResponder ? view : nil
    }
}
