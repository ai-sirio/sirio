import Foundation
import Testing
@preconcurrency import WebKit
import TillerBrowser

@MainActor
@Suite struct BrowserSurfaceTests {
    @Test func opensFixtureAndReadsPageValues() async throws {
        let fixtureURL = try makeFixture()
        let surface = makeSurface()

        let page = try await surface.open(fixtureURL)

        #expect(page.url == fixtureURL)
        #expect(page.title == "Browser fixture")
        #expect(try await surface.get(.url) == fixtureURL.absoluteString)
        #expect(try await surface.get(.text).contains("Hello from the browser fixture"))
        #expect(try await surface.get(.html).contains("<h1>Hello from the browser fixture</h1>"))
    }

    @Test func reloadPreservesTheCurrentURL() async throws {
        let fixtureURL = try makeFixture()
        let surface = makeSurface()
        _ = try await surface.open(fixtureURL)

        let page = try await surface.navigate(.reload)

        #expect(page.url == fixtureURL)
    }

    @Test func backWithoutHistoryReturnsAnError() async throws {
        let surface = makeSurface()
        do {
            _ = try await surface.navigate(.back)
            Issue.record("back without history should fail")
        } catch let error as BrowserError {
            #expect(error.code == .navigationUnavailable)
        }
    }

    @Test func malformedOpenReturnsInvalidURL() async {
        let surface = makeSurface()
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
        let defaultSurface = makeSurface()
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

        let surface = makeSurface()
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

    @Test func snapshotActAndWaitSupportTheAgentLoop() async throws {
        let fixtureURL = try makeFixture(contents: """
        <!doctype html>
        <html><head><title>Loop</title></head><body>
          <form onsubmit="event.preventDefault(); document.querySelector('#result').textContent = document.querySelector('#name').value">
            <input id="name" name="name" aria-label="Name">
            <button id="submit" type="submit">Submit</button>
          </form>
          <p id="result">Waiting</p>
        </body></html>
        """)
        let surface = makeSurface()

        _ = try await surface.open(fixtureURL)
        let snapshot = try await surface.snapshot()
        let input = try #require(snapshot.nodes.first { $0.name == "Name" })
        let button = try #require(snapshot.nodes.first { $0.name == "Submit" })

        _ = try await surface.act(.fill(ref: input.ref, value: "Ada"), generation: snapshot.generation)
        _ = try await surface.act(.click(ref: button.ref), generation: snapshot.generation)
        _ = try await surface.wait(.text("Ada"), timeoutMs: 1_000)

        #expect(try await surface.get(.text).contains("Ada"))
    }

    @Test func actingWithARefFromAnOlderSnapshotFailsAsStale() async throws {
        let fixtureURL = try makeFixture(contents: "<html><body><button>One</button></body></html>")
        let surface = makeSurface()

        _ = try await surface.open(fixtureURL)
        let snapshot = try await surface.snapshot()
        _ = try await surface.snapshot()
        let button = try #require(snapshot.nodes.first)

        await #expect(throws: BrowserError.staleRef) {
            _ = try await surface.act(.click(ref: button.ref), generation: snapshot.generation)
        }
    }

    /// Navigating is a human act, and reading the title to fill in the tab is
    /// Tiller rendering its own window. Prompting there tells the user an agent
    /// wants their page when none does, and teaches them to click Allow on
    /// every site they visit — which grants the access `eval` really needs.
    @Test func humanNavigationDoesNotAskForAgentOriginPermission() async throws {
        let fixtureURL = try makeFixture()
        var authorizationCalls = 0
        let surface = BrowserSurface(
            dataStore: .nonPersistent(),
            worktreeID: UUID(),
            originAuthorization: { _, _ in
                authorizationCalls += 1
                return true
            })

        let page = try await surface.open(fixtureURL)

        #expect(authorizationCalls == 0)
        #expect(page.title == "Browser fixture")

        _ = try await surface.get(.text)
        #expect(authorizationCalls == 1)
    }

    @Test func externalScriptAccessWithoutAGrantReturnsOriginDenied() async throws {
        let fixtureURL = try makeFixture()
        let surface = BrowserSurface(
            dataStore: .nonPersistent(),
            worktreeID: UUID(),
            originAuthorization: { _, _ in false })

        _ = try await surface.open(fixtureURL)

        var receivedCode: BrowserError.Code?
        do {
            _ = try await surface.eval("document.title")
        } catch {
            receivedCode = (error as? BrowserError)?.code
        }
        #expect(receivedCode == .originDenied)
    }

    /// A screenshot of an authenticated page leaks at least as much as its text,
    /// so the pixel path may not be the cheap way around the origin gate.
    @Test func screenshotWithoutAGrantReturnsOriginDenied() async throws {
        let fixtureURL = try makeFixture()
        let surface = BrowserSurface(
            dataStore: .nonPersistent(),
            worktreeID: UUID(),
            originAuthorization: { _, _ in false })
        _ = try await surface.open(fixtureURL)

        var receivedCode: BrowserError.Code?
        do {
            _ = try await surface.screenshot()
        } catch {
            receivedCode = (error as? BrowserError)?.code
        }
        #expect(receivedCode == .originDenied)
    }

    @Test func unsupportedBrowserVerbsHaveAnExplicitError() {
        for verb in BrowserUnsupportedVerb.allCases {
            #expect(BrowserUnsupportedVerb(rawValue: verb.rawValue) == verb)
            #expect(verb.error == .notSupported)
        }
    }

    @Test func synthesizedClickAndLocationAssignmentExposeNavigationTypes() async throws {
        let fixtureURL = try makeFixture(contents: """
        <html><body><a id="mail" href="mailto:synthetic@example.test">Mail</a></body></html>
        """)
        let surface = makeSurface()
        var type: WKNavigationType?
        var origin: BrowserNavigationOrigin?
        surface.onNavigationType = { type = $0 }
        surface.onExternalURL = { _, value in origin = value }

        _ = try await surface.open(fixtureURL)
        _ = try await surface.webView.evaluateJavaScript("document.querySelector('#mail').click()")
        for _ in 0..<20 where origin == nil { try await Task.sleep(for: .milliseconds(10)) }
        print("syntheticClickNavigationType=\(type?.rawValue ?? -1)")
        print("syntheticClickOrigin=\(String(describing: origin))")

        let agentSurface = makeSurface()
        var agentOrigin: BrowserNavigationOrigin?
        agentSurface.onExternalURL = { _, value in agentOrigin = value }
        agentSurface.onNavigationType = { type = $0 }
        _ = try await agentSurface.open(fixtureURL)
        origin = nil
        let snapshot = try await agentSurface.snapshot()
        let mailRef = try #require(snapshot.nodes.first { $0.name == "Mail" })
        _ = try await agentSurface.act(.click(ref: mailRef.ref), generation: snapshot.generation)
        for _ in 0..<20 where agentOrigin == nil { try await Task.sleep(for: .milliseconds(10)) }
        #expect(agentOrigin == .agentAction)

        agentOrigin = nil
        type = nil
        _ = try await agentSurface.eval("location.href = 'mailto:location@example.test'")
        for _ in 0..<20 where agentOrigin == nil { try await Task.sleep(for: .milliseconds(10)) }
        print("locationAssignmentNavigationType=\(type?.rawValue ?? -1)")
        print("locationAssignmentOrigin=\(String(describing: agentOrigin))")

        #expect(agentOrigin == .agentAction)
    }

    @MainActor
    private func makeSurface() -> BrowserSurface {
        BrowserSurface(
            dataStore: .nonPersistent(),
            worktreeID: UUID(),
            originAuthorization: { _, _ in true })
    }

    private func makeFixture(contents: String = """
    <!doctype html>
    <html><head><title>Browser fixture</title></head>
    <body><h1>Hello from the browser fixture</h1></body></html>
    """) throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("tiller-browser-\(UUID().uuidString).html")
        try contents.write(to: url, atomically: true, encoding: .utf8)
        return url
    }
}
