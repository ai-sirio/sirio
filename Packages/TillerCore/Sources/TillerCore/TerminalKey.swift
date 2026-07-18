import Foundation

/// Symbolic key names accepted by `tillerctl panel key`, resolved to the byte
/// sequences a PTY expects. Names travel over the socket; bytes are
/// resolved app-side at write time.
public enum TerminalKey: String, CaseIterable, Sendable {
    case enter, tab, escape, backspace, delete, up, down, left, right

    public var bytes: Data {
        switch self {
        case .enter: Data([0x0D])
        case .tab: Data([0x09])
        case .escape: Data([0x1B])
        case .backspace: Data([0x7F])
        case .delete: Data("\u{1B}[3~".utf8)
        case .up: Data("\u{1B}[A".utf8)
        case .down: Data("\u{1B}[B".utf8)
        case .right: Data("\u{1B}[C".utf8)
        case .left: Data("\u{1B}[D".utf8)
        }
    }
}
