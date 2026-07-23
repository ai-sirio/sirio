import Foundation

/// A pending permission request surfaced in the composer's approval panel.
public struct ComposerPermission: Sendable, Equatable, Identifiable {
    public var requestId: JSONRPCID
    public var toolCallId: String
    public var title: String
    public var kind: ToolKind
    public var options: [PermissionOption]

    public var id: String { toolCallId }

    public init(requestId: JSONRPCID, toolCallId: String, title: String,
                kind: ToolKind, options: [PermissionOption]) {
        self.requestId = requestId
        self.toolCallId = toolCallId
        self.title = title
        self.kind = kind
        self.options = options
    }
}

public enum ComposerPermissions {
    /// Pending permissions in transcript order. Plan-mode exits
    /// (kind .switchMode) are excluded: they render inside the
    /// proposed-plan card instead of the composer panel.
    public static func extract(from items: [TranscriptItem]) -> [ComposerPermission] {
        items.compactMap { item in
            guard case .toolCall(let call) = item,
                  call.kind != .switchMode,
                  let permission = call.permission, permission.isPending else {
                return nil
            }
            return ComposerPermission(
                requestId: permission.requestId, toolCallId: call.toolCallId,
                title: call.title, kind: call.kind, options: permission.options)
        }
    }
}
