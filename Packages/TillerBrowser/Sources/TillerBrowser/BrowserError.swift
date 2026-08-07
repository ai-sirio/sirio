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
        case .navigationUnavailable: .navigationUnavailable
        case .navigationFailed: .navigationFailed
        }
    }

    public var hint: String? {
        switch self {
        case .jsError(let hint), .navigationFailed(let hint): hint
        default: nil
        }
    }

    public var errorDescription: String? {
        if let hint { return code.rawValue + ": " + hint }
        return code.rawValue
    }
}
