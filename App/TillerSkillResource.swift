import Foundation

enum TillerSkillResourceError: Error, LocalizedError {
    case missing
    case unreadable(String)

    var errorDescription: String? {
        switch self {
        case .missing: "Bundled Tiller skill is missing"
        case let .unreadable(message): "Bundled Tiller skill is unreadable: \(message)"
        }
    }
}

enum TillerSkillResource {
    static let markdown: Result<String, TillerSkillResourceError> = {
        guard let url = Bundle.main.url(forResource: "SKILL", withExtension: "md") else {
            return .failure(.missing)
        }
        do {
            return .success(try String(contentsOf: url, encoding: .utf8))
        } catch {
            return .failure(.unreadable(error.localizedDescription))
        }
    }()
}
