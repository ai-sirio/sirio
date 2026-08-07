import Foundation
import Testing
import TillerBrowser

@MainActor
@Suite struct BrowserSurfaceTests {
    @Test func opensFixtureAndReadsPageValues() async throws {
        let fixtureURL = try makeFixture()
        let surface = BrowserSurface(dataStore: .nonPersistent())

        let page = try await surface.open(fixtureURL)

        #expect(page.url == fixtureURL)
        #expect(page.title == "Browser fixture")
        #expect(try await surface.get(.url) == fixtureURL.absoluteString)
        #expect(try await surface.get(.text).contains("Hello from the browser fixture"))
        #expect(try await surface.get(.html).contains("<h1>Hello from the browser fixture</h1>"))
    }

    @Test func reloadPreservesTheCurrentURL() async throws {
        let fixtureURL = try makeFixture()
        let surface = BrowserSurface(dataStore: .nonPersistent())
        _ = try await surface.open(fixtureURL)

        let page = try await surface.navigate(.reload)

        #expect(page.url == fixtureURL)
    }

    @Test func backWithoutHistoryReturnsAnError() async throws {
        let surface = BrowserSurface(dataStore: .nonPersistent())
        do {
            _ = try await surface.navigate(.back)
            Issue.record("back without history should fail")
        } catch let error as BrowserError {
            #expect(error.code == .navigationUnavailable)
        }
    }

    @Test func malformedOpenReturnsInvalidURL() async {
        let surface = BrowserSurface(dataStore: .nonPersistent())
        do {
            _ = try await surface.open("not a URL")
            Issue.record("malformed URL should fail")
        } catch let error as BrowserError {
            #expect(error.code == .invalidURL)
        } catch {
            Issue.record("unexpected error: \(error)")
        }
    }

    @Test func usesFixedSafariUserAgentWithAnExplicitOverridePoint() {
        let defaultSurface = BrowserSurface(dataStore: .nonPersistent())
        let overrideSurface = BrowserSurface(
            dataStore: .nonPersistent(),
            userAgentPolicy: UserAgentPolicy(overrideUserAgent: "Tiller test agent"))

        #expect(defaultSurface.webView.customUserAgent == UserAgentPolicy.safariDesktop)
        #expect(overrideSurface.webView.customUserAgent == "Tiller test agent")
    }

    private func makeFixture() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-browser-\(UUID().uuidString).html")
        try """
        <!doctype html>
        <html><head><title>Browser fixture</title></head>
        <body><h1>Hello from the browser fixture</h1></body></html>
        """.write(to: url, atomically: true, encoding: .utf8)
        return url
    }
}
