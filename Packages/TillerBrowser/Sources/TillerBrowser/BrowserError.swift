import Foundation

public enum BrowserError: Error, Equatable, Sendable, LocalizedError {
    public enum Code: String, Equatable, Sendable {
        case surfaceNotFound = "surface_not_found"
        case staleRef = "stale_ref"
        case jsError = "js_error"
        case timeout
        case notSupported = "not_supported"
        case originDenied = "origin_denied"
        case invalidURL = "invalid_url"
        case invalidArgument = "invalid_argument"
        case navigationUnavailable = "navigation_unavailable"
        case navigationFailed = "navigation_failed"
    }

    case surfaceNotFound
    case staleRef
    case jsError(hint: String)
    case timeout
    case notSupported
    case originDenied
        case invalidURL
    case invalidArgument(hint: String)
    case navigationUnavailable
    case navigationFailed(hint: String)

    public var code: Code {
        switch self {
        case .surfaceNotFound: .surfaceNotFound
        case .staleRef: .staleRef
        case .jsError: .jsError
        case .timeout: .timeout
        case .notSupported: .notSupported
        case .originDenied: .originDenied
        case .invalidURL: .invalidURL
        case .invalidArgument: .invalidArgument
        case .navigationUnavailable: .navigationUnavailable
        case .navigationFailed: .navigationFailed
        }
    }

    public var hint: String? {
        switch self {
        case .jsError(let hint), .navigationFailed(let hint), .invalidArgument(let hint): hint
        default: nil
        }
    }

    public var errorDescription: String? {
        if let hint { return code.rawValue + ": " + hint }
        return code.rawValue
    }

    /// Text for the chrome bar. `errorDescription` stays the machine code the
    /// socket API reports to agents: an agent acts on the code, a human reads
    /// the sentence, and collapsing the two registers serves neither.
    public var userMessage: String {
        switch self {
        case .surfaceNotFound: "This browser surface is no longer available."
        case .staleRef: "The page changed. Take a new snapshot before acting."
        case .jsError(let hint): "The page rejected the script. " + hint
        case .timeout: "The page took too long to respond."
        case .notSupported: "That action is not supported here."
        case .originDenied: "Permission for this site was denied."
        case .invalidURL: "That address is not valid."
        case .invalidArgument(let hint): "That argument is not valid. " + hint
        case .navigationUnavailable: "There is nowhere to go in that direction."
        case .navigationFailed(let hint): hint
        }
    }
}
