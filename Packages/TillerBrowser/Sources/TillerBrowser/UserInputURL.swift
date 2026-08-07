import Foundation

/// Normalises what a human types in the address bar. The engine's `open` stays
/// strict on purpose: the socket API is driven by agents, and answering
/// `invalid_url` to a malformed URL shows an agent its own mistake instead of
/// guessing. Humans type `www.google.it`, so the chrome bar normalises before
/// handing the string to the engine.
///
/// Deliberately not a search engine: free-text queries are a non-goal of the
/// browser surface spec, so input that cannot be read as a host stays invalid.
public enum UserInputURL {
    /// Mirrors what `BrowserSurface` will actually load.
    private static let supportedSchemes: Set<String> = ["http", "https", "file", "about"]

    /// Returns an absolute URL for `raw`, defaulting a missing scheme to https.
    /// Returns nil when the input cannot be read as a host or file path.
    public static func normalize(_ raw: String) -> URL? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        // Only a scheme the surface can actually load counts as a scheme.
        // `URL(string:)` reads "localhost:5173/index.html" as scheme
        // "localhost", and "example.com:8080/x" as scheme "example.com", since
        // any letter-led token before a colon is a syntactically valid scheme —
        // so trusting `url.scheme` here would leave host:port input unusable.
        if let url = URL(string: trimmed),
           let scheme = url.scheme?.lowercased(),
           supportedSchemes.contains(scheme) {
            return url
        }
        // A bare path is a local file, not a host: "/tmp/x.html" must not become
        // "https:///tmp/x.html".
        if trimmed.hasPrefix("/") {
            return URL(fileURLWithPath: trimmed)
        }
        guard isPlausibleHost(trimmed) else { return nil }
        return URL(string: "https://" + trimmed)
    }

    /// A host must have no whitespace and either contain a dot or be localhost,
    /// so free text like "how to cook rice" is rejected rather than turned into
    /// a doomed navigation.
    private static func isPlausibleHost(_ candidate: String) -> Bool {
        guard !candidate.contains(" ") else { return false }
        let host = candidate.split(separator: "/", maxSplits: 1).first.map(String.init) ?? candidate
        let bare = host.split(separator: ":", maxSplits: 1).first.map(String.init) ?? host
        if bare == "localhost" { return true }
        guard bare.contains("."), !bare.hasPrefix("."), !bare.hasSuffix(".") else { return false }
        return true
    }
}
