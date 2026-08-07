import Foundation
import Testing
@testable import TillerBrowser

struct OriginPolicyTests {
    @Test(arguments: [
        "http://localhost:3000",
        "https://localhost:443",
        "http://127.0.0.1:5173",
        "http://[::1]:8080",
        "http://app.local:1234",
    ])
    func localOriginsAreAllowedWithoutAGrant(_ rawURL: String) throws {
        let url = try #require(URL(string: rawURL))

        #expect(OriginPolicy.isLocal(url))
    }

    @Test(arguments: [
        "https://example.test:8443/path",
        "https://notlocalhost.example",
        "http://127.0.0.2:5173",
        "file:///tmp/page.html",
    ])
    func everythingElseNeedsAGrant(_ rawURL: String) throws {
        let url = try #require(URL(string: rawURL))

        #expect(!OriginPolicy.isLocal(url))
    }

    /// The grant is keyed on this string, so two URLs that share an origin must
    /// produce the same one and a different port must not.
    @Test func originCanonicalizationIgnoresPathAndNormalizesHost() throws {
        let url = try #require(URL(string: "HTTPS://Example.TEST:443/a"))
        let sameOrigin = try #require(URL(string: "https://example.test:443/b?q=1"))
        let otherPort = try #require(URL(string: "https://example.test:8443/a"))

        #expect(OriginPolicy.origin(for: url) == "https://example.test:443")
        #expect(OriginPolicy.origin(for: sameOrigin) == OriginPolicy.origin(for: url))
        #expect(OriginPolicy.origin(for: otherPort) != OriginPolicy.origin(for: url))
    }
}
