import Foundation
import TillerGit

extension Notification.Name {
    static let tillerChangesDidRefresh = Notification.Name(
        "Tiller.changesDidRefresh")
}

enum DiffUnavailableReason: Equatable {
    case clean
    case binary
    case submodule
    case outputLimit
    case error(String)

    var message: String {
        switch self {
        case .clean: "The file is no longer modified."
        case .binary: "Binary files do not have a text diff."
        case .submodule: "Submodule changes cannot be rendered as text."
        case .outputLimit: "The diff is too large to display."
        case .error(let message): message
        }
    }
}

enum DiffTabAvailability {
    static func reason(for diff: GitFileDiff?) -> DiffUnavailableReason? {
        guard let diff else { return .clean }
        if diff.isBinary { return .binary }
        if diff.isSubmodule { return .submodule }
        return nil
    }

    static func reason(for error: Error) -> DiffUnavailableReason {
        if case .outputTooLarge = error as? GitError {
            return .outputLimit
        }
        return .error(error.localizedDescription)
    }
}
