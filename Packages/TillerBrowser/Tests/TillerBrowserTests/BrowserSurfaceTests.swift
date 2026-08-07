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

    @Test func targetBlankNavigationStaysOnTheSameSurface() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-browser-target-blank-\(UUID().uuidString)",
                                    isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let destination = directory.appendingPathComponent("destination.html")
        let source = directory.appendingPathComponent("source.html")
        try "<html><head><title>Destination</title></head><body>destination</body></html>"
            .write(to: destination, atomically: true, encoding: .utf8)
        try "<a href=\"\(destination.absoluteString)\" target=\"_blank\">Open</a>"
            .write(to: source, atomically: true, encoding: .utf8)

        let surface = BrowserSurface(dataStore: .nonPersistent())
        let destinationLoaded = Task { @MainActor in
            await withCheckedContinuation { continuation in
                surface.onPageChange = { page in
                    if page.url == destination {
                        continuation.resume()
                    }
                }
            }
        }
        _ = try await surface.open(source)
        _ = try await surface.webView.evaluateJavaScript("document.querySelector('a').click()")
        await destinationLoaded.value

        #expect(try await surface.get(.url) == destination.absoluteString)
        #expect(surface.webView.uiDelegate === surface)
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
