import Foundation
import Testing
@testable import TillerCore

@Test func resolvesProvidersIntoURLsInDragOrder() async throws {
    let directory = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent(UUID().uuidString)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(at: directory) }

    let first = directory.appendingPathComponent("first.txt")
    let second = directory.appendingPathComponent("second.txt")
    try Data("one".utf8).write(to: first)
    try Data("two".utf8).write(to: second)

    let providers = [NSItemProvider(contentsOf: first), NSItemProvider(contentsOf: second)]
        .compactMap { $0 }
    let urls = await DroppedFileLoader.urls(from: providers)

    #expect(urls.map(\.lastPathComponent) == ["first.txt", "second.txt"])
}

@Test func readsTheByteCountOfARealFile() throws {
    let url = URL(fileURLWithPath: NSTemporaryDirectory())
        .appendingPathComponent("\(UUID().uuidString).bin")
    try Data(count: 128).write(to: url)
    defer { try? FileManager.default.removeItem(at: url) }

    #expect(DroppedFileLoader.byteCount(of: url) == 128)
}

@Test func reportsZeroBytesForSomethingThatIsNotAReadableFile() {
    let missing = URL(fileURLWithPath: "/nonexistent/\(UUID().uuidString)")
    #expect(DroppedFileLoader.byteCount(of: missing) == 0)
}
