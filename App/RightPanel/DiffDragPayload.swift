import AppKit
import SwiftUI
import TillerWorkspace
import UniformTypeIdentifiers

extension UTType {
    static let tillerDiffDrag = UTType(exportedAs: "it.tiller.diff-drag")
}

struct DiffDragPayload: Codable, Hashable, Sendable, Transferable {
    let path: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .tillerDiffDrag)
    }

    static func path(from pasteboard: NSPasteboard) -> String? {
        guard let data = pasteboard.data(forType: WorkspaceExternalDrop.diffPasteboardType),
              let payload = try? JSONDecoder().decode(Self.self, from: data) else {
            return nil
        }
        return payload.path
    }
}
