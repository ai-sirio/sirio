import SwiftUI
import UniformTypeIdentifiers
import TillerCore

extension UTType {
    /// Internal drag type for reorderable sidebar and tab bar rows: never
    /// exported outside the app, declared in project.yml so the drop side can
    /// match it.
    static let tillerRowDrag = UTType(exportedAs: "it.tiller.row-drag")
}

/// Which list a dragged row belongs to. Drops across two different scopes are
/// rejected, so a tab can never land among projects and a worktree can never
/// jump to another project.
enum ReorderScope: Equatable {
    case projects
    case worktrees(projectId: UUID)
    case tabs(worktreeId: UUID)
}

struct DraggedRow: Equatable {
    let id: UUID
    let scope: ReorderScope
}

/// Makes a row draggable and a drop target for its own list, reordering the
/// model **while the drag is in flight** so the neighbours shift under the
/// pointer and the drop only commits what is already on screen.
///
/// The dragged identity lives in `AppModel.draggingRow` rather than in the
/// drag item: `onDrop`'s targeting callback fires before the item is ever
/// delivered, and a live preview needs to know the source on hover.
private struct ReorderableRow: ViewModifier {
    let model: AppModel
    let id: UUID
    let scope: ReorderScope

    func body(content: Content) -> some View {
        content
            .onDrag {
                // A drag released outside any row never reaches a drop, so the
                // previewed order would stay unsaved. Flush the previous one
                // here, the earliest point where we know it ended.
                if model.draggingRow != nil { model.endRowDrag() }
                model.draggingRow = DraggedRow(id: id, scope: scope)
                // The row id travels in `draggingRow`, not in the item, but the
                // item still has to carry real data: an empty provider never
                // starts a drag session at all.
                let provider = NSItemProvider()
                provider.registerDataRepresentation(
                    for: .tillerRowDrag, visibility: .ownProcess
                ) { completion in
                    completion(Data(id.uuidString.utf8), nil)
                    return nil
                }
                return provider
            }
            .onDrop(
                of: [.tillerRowDrag],
                isTargeted: Binding(
                    get: { false },
                    set: { targeted in if targeted { model.previewDrag(before: id, in: scope) } }
                )
            ) { _ in
                model.endRowDrag()
                return true
            }
    }
}

extension View {
    /// See `ReorderableRow`.
    func reorderable(model: AppModel, id: UUID, scope: ReorderScope) -> some View {
        modifier(ReorderableRow(model: model, id: id, scope: scope))
    }
}
