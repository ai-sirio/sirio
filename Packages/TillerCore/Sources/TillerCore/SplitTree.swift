import Foundation

public enum SplitAxis: Equatable, Sendable {
    case horizontal
    case vertical
}

/// Immutable binary tree of terminal panes. Every mutation returns a new
/// tree — pane state (PTY, scrollback) is keyed by leaf UUID elsewhere, so
/// structural edits never touch running terminals.
public indirect enum SplitTree: Equatable, Sendable {
    case leaf(id: UUID)
    case split(axis: SplitAxis, first: SplitTree, second: SplitTree)

    public func splitting(leaf target: UUID, axis: SplitAxis, newLeaf: UUID) -> SplitTree {
        switch self {
        case .leaf(let id) where id == target:
            return .split(axis: axis, first: self, second: .leaf(id: newLeaf))
        case .leaf:
            return self
        case .split(let a, let first, let second):
            return .split(
                axis: a,
                first: first.splitting(leaf: target, axis: axis, newLeaf: newLeaf),
                second: second.splitting(leaf: target, axis: axis, newLeaf: newLeaf)
            )
        }
    }

    public func removing(leaf target: UUID) -> SplitTree? {
        switch self {
        case .leaf(let id):
            return id == target ? nil : self
        case .split(let axis, let first, let second):
            switch (first.removing(leaf: target), second.removing(leaf: target)) {
            case (nil, let survivor?), (let survivor?, nil):
                return survivor
            case (let f?, let s?):
                return .split(axis: axis, first: f, second: s)
            case (nil, nil):
                return nil
            }
        }
    }

    public var leafIds: [UUID] {
        switch self {
        case .leaf(let id): return [id]
        case .split(_, let first, let second): return first.leafIds + second.leafIds
        }
    }
}

// Conformance sintetizzata (SE-0295): serializza tab layout in GRDB.
extension SplitAxis: Codable {}
extension SplitTree: Codable {}
