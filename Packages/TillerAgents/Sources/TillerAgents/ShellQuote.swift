import Foundation

/// POSIX single-quote escaping: wraps in ' and replaces every embedded
/// ' with '\'' so the result is one shell word under sh/zsh/bash.
func shellQuote(_ s: String) -> String {
    "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
}

/// JSON string literal (also a valid TOML basic string literal) for
/// embedding arbitrary text in generated code/config. `\/` is not a TOML
/// escape sequence, so slashes must stay unescaped or a TOML parser (e.g.
/// Codex's `-c key=value` override) rejects the whole value.
func jsonStringLiteral(_ s: String) -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.withoutEscapingSlashes]
    let data = try! encoder.encode([s])
    let arr = String(decoding: data, as: UTF8.self)
    return String(arr.dropFirst().dropLast()) // strip [ ]
}
