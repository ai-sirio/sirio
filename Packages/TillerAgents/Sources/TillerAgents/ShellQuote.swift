import Foundation

/// POSIX single-quote escaping: wraps in ' and replaces every embedded
/// ' with '\'' so the result is one shell word under sh/zsh/bash.
func shellQuote(_ s: String) -> String {
    "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
}

/// JSON string literal (also a valid JS/TS string literal) for embedding
/// arbitrary text in generated code/config.
func jsonStringLiteral(_ s: String) -> String {
    let data = try! JSONEncoder().encode([s])
    let arr = String(decoding: data, as: UTF8.self)
    return String(arr.dropFirst().dropLast()) // strip [ ]
}
