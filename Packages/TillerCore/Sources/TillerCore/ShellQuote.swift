import Foundation

/// POSIX single-quote escaping: wraps in ' and replaces every embedded
/// ' with '\'' so the result is one shell word under sh/zsh/bash.
///
/// Lives here rather than in TillerAgents because `FileDrop` needs it too
/// and TillerCore sits below TillerAgents in the dependency graph.
public func shellQuote(_ s: String) -> String {
    "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
}
