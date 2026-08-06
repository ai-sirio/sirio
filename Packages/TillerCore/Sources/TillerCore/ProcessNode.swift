import Foundation

/// One process in a pane's shell subtree (captured by the App-layer libproc
/// walk; modelled here so the scan logic stays testable in TillerCore).
public struct ProcessNode: Equatable, Sendable {
    public let pid: Int32
    public let name: String
    public let children: [ProcessNode]

    public init(pid: Int32, name: String, children: [ProcessNode]) {
        self.pid = pid
        self.name = name
        self.children = children
    }
}
