import Foundation

public enum BrowserNavigationOrigin: Equatable, Sendable {
    case humanGesture
    case agentAction
}

public struct BrowserBox: Codable, Equatable, Sendable {
    public let x: Double
    public let y: Double
    public let width: Double
    public let height: Double
}

public struct BrowserSnapshotNode: Codable, Equatable, Sendable {
    public let ref: String
    public let role: String
    public let name: String
    public let value: String?
    public let box: BrowserBox
}

public struct BrowserSnapshot: Codable, Equatable, Sendable {
    public let generation: Int
    public let nodes: [BrowserSnapshotNode]
}

public enum BrowserAct: Equatable, Sendable {
    case click(ref: String? = nil, selector: String? = nil)
    case fill(ref: String? = nil, selector: String? = nil, value: String)
    case type(ref: String? = nil, selector: String? = nil, value: String)
    case press(ref: String? = nil, selector: String? = nil, key: String)
    case scroll(ref: String? = nil, selector: String? = nil, deltaX: Double, deltaY: Double)
}

public enum BrowserWaitCondition: Equatable, Sendable {
    case selector(String)
    case text(String)
    case urlContains(String)
    case loadState(String)
    case function(String)
}

public struct BrowserActResult: Equatable, Sendable {
    public init() {}
}

public struct BrowserConsoleEntry: Codable, Equatable, Sendable {
    public let level: String
    public let text: String
    public let at: Double
}

public enum BrowserUnsupportedVerb: String, CaseIterable, Equatable, Sendable {
    case offlineEmulation = "offline"
    case trace
    case screencast
    case record
    case networkRoute = "network-route"
    case networkMock = "network-mock"
    case networkIntercept = "network-intercept"
    case rawInput = "raw-input"
    case download
    case upload
    case pdf
    case viewport
    case devtools
    case extensions
    case bookmarks
    case history

    public var error: BrowserError { .notSupported }
}
