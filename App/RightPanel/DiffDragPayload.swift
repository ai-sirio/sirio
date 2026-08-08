import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    static let tillerDiffDrag = UTType(exportedAs: "it.tiller.diff-drag")
}

struct DiffDragPayload: Codable, Hashable, Sendable, Transferable {
    let path: String

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .tillerDiffDrag)
    }
}
