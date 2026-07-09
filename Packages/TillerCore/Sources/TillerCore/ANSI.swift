import Foundation

/// Strips terminal ANSI escape sequences (CSI + OSC) from raw PTY text.
/// Shared by `ProviderUsage`'s `/usage` scraping and `ScreenManifest`'s
/// content-signal detection — one implementation, two consumers.
public func stripANSI(_ s: String) -> String {
    var out = s
    // CSI sequences: ESC [ ... final-byte
    out = out.replacingOccurrences(
        of: "\u{1B}\\[[0-9;?]*[ -/]*[@-~]", with: "", options: .regularExpression)
    // OSC sequences: ESC ] ... BEL
    out = out.replacingOccurrences(
        of: "\u{1B}\\][^\u{07}]*\u{07}", with: "", options: .regularExpression)
    return out
}
