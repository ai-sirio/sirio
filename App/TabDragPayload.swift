import CoreTransferable
import UniformTypeIdentifiers

extension UTType {
    /// Tipo interno per il drag delle tab: mai esportato fuori dall'app.
    static let tillerTabDrag = UTType(exportedAs: "it.tiller.tab-drag")
}

/// Payload del drag di una tab: porta anche il worktree di origine così i
/// drop target rifiutano il cross-worktree.
struct TabDragPayload: Codable, Transferable {
    let tabId: UUID
    let worktreeId: UUID

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .tillerTabDrag)
    }
}
