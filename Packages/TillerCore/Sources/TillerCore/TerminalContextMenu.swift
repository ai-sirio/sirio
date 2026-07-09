import Foundation

public struct TerminalContextMenuItem: Sendable, Equatable {
    public let title: String
    public let systemImage: String
    public let action: TerminalContextMenuAction

    public init(title: String, systemImage: String, action: TerminalContextMenuAction) {
        self.title = title
        self.systemImage = systemImage
        self.action = action
    }
}

public enum TerminalContextMenuAction: Sendable, Equatable {
    case copy, paste, copyContext, setTitle, copyPaneId, copyTerminalId, splitRight, splitDown, clear, close
}
