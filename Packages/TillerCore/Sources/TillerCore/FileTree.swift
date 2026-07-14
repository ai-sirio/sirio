import Foundation

public enum FileTreeNodeKind: Equatable, Sendable {
    case directory
    case file
    case symbolicLink

    public var isDirectory: Bool { self == .directory }
}

public struct FileTreeNode: Identifiable, Equatable, Sendable {
    public let relativePath: String
    public let name: String
    public let kind: FileTreeNodeKind

    public var id: String { relativePath }

    public init(relativePath: String, name: String, kind: FileTreeNodeKind) {
        self.relativePath = relativePath
        self.name = name
        self.kind = kind
    }

    public func url(relativeTo rootURL: URL) -> URL {
        rootURL.appendingPathComponent(relativePath)
    }
}

public enum FileTreeError: Error, Equatable {
    case pathOutsideRoot(String)
    case notDirectory(String)
}

extension FileTreeError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .pathOutsideRoot(let path): "Path escapes the worktree root: \(path)"
        case .notDirectory(let path): "Path is not a directory: \(path)"
        }
    }
}

public enum FileTreeLoader {
    public static func children(at relativePath: String, rootURL: URL) throws -> [FileTreeNode] {
        let root = rootURL.standardizedFileURL
        let directory = try validatedURL(relativePath: relativePath, root: root)
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey])
        guard values.isDirectory == true else { throw FileTreeError.notDirectory(relativePath) }

        return try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
            options: []
        )
        .filter { $0.lastPathComponent != ".git" }
        .map { child in
            let resource = try child.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
            let kind: FileTreeNodeKind
            if resource.isSymbolicLink == true {
                kind = .symbolicLink
            } else if resource.isDirectory == true {
                kind = .directory
            } else {
                kind = .file
            }
            let childRelative = relativePath.isEmpty
                ? child.lastPathComponent
                : relativePath + "/" + child.lastPathComponent
            return FileTreeNode(relativePath: childRelative, name: child.lastPathComponent, kind: kind)
        }
        .sorted { lhs, rhs in
            if lhs.kind.isDirectory != rhs.kind.isDirectory { return lhs.kind.isDirectory }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }

    private static func validatedURL(relativePath: String, root: URL) throws -> URL {
        guard !relativePath.hasPrefix("/") else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        let components = relativePath.split(separator: "/", omittingEmptySubsequences: false)
        guard !components.contains("..") else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        let rootValues = try root.resourceValues(forKeys: [.isSymbolicLinkKey])
        if rootValues.isSymbolicLink == true {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }

        var traversed = root
        for component in components where !component.isEmpty && component != "." {
            traversed.appendPathComponent(String(component), isDirectory: true)
            let values = try traversed.resourceValues(forKeys: [.isSymbolicLinkKey])
            if values.isSymbolicLink == true {
                throw FileTreeError.pathOutsideRoot(relativePath)
            }
        }

        let candidate = relativePath.isEmpty
            ? root
            : root.appendingPathComponent(relativePath, isDirectory: true).standardizedFileURL
        let rootComponents = root.pathComponents
        let candidateComponents = candidate.pathComponents
        guard candidateComponents.starts(with: rootComponents) else {
            throw FileTreeError.pathOutsideRoot(relativePath)
        }
        return candidate
    }
}
