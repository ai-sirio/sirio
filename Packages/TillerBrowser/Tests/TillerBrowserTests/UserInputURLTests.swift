import Foundation
import Testing
@testable import TillerBrowser

@Suite struct UserInputURLTests {
    @Test func schemelessHostGetsHTTPS() throws {
        let url = try #require(UserInputURL.normalize("www.google.it"))
        #expect(url.absoluteString == "https://www.google.it")
    }

    @Test func existingSchemeIsLeftAlone() throws {
        let http = try #require(UserInputURL.normalize("http://127.0.0.1:8765/x"))
        #expect(http.absoluteString == "http://127.0.0.1:8765/x")
        let file = try #require(UserInputURL.normalize("file:///tmp/page.html"))
        #expect(file.isFileURL)
    }

    @Test func localhostWithPortAndPathIsAHost() throws {
        let url = try #require(UserInputURL.normalize("localhost:5173/index.html"))
        #expect(url.absoluteString == "https://localhost:5173/index.html")
    }

    @Test func barePathIsAFileNotAHost() throws {
        let url = try #require(UserInputURL.normalize("/tmp/page.html"))
        #expect(url.isFileURL)
        #expect(url.path == "/tmp/page.html")
    }

    @Test func freeTextIsRejectedRatherThanTurnedIntoADoomedNavigation() {
        #expect(UserInputURL.normalize("how to cook rice") == nil)
        #expect(UserInputURL.normalize("google") == nil)
        #expect(UserInputURL.normalize("   ") == nil)
        #expect(UserInputURL.normalize(".leadingdot") == nil)
    }
}
