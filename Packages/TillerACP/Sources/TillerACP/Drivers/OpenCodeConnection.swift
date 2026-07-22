import Foundation

/// HTTP+SSE surface of one `opencode serve` process. Protocol-ized so the
/// driver is testable without sockets (t3code's OpenCodeServerConnection idea).
public protocol OpenCodeConnection: Sendable {
    func request(method: String, path: String, body: Data?) async throws -> Data
    /// Server-sent events from GET /event, one JSON object per event.
    func events() -> AsyncThrowingStream<Data, Error>
    func close() async
}

/// Incrementally parses the data fields of server-sent event frames.
///
/// A frame is emitted when a blank line is received. Comment-only frames,
/// including OpenCode's `:heartbeat` frames, are ignored.
public struct SSEParser: Sendable {
    private var buffer = Data()

    public init() {}

    /// Feeds one network chunk and returns the complete JSON data fields found
    /// in it. Incomplete frames remain buffered for the next call.
    public mutating func feed(_ data: Data) -> [Data] {
        buffer.append(data)

        var events: [Data] = []
        while let delimiter = frameDelimiter(in: buffer) {
            let frame = buffer.prefix(delimiter.start)
            buffer.removeFirst(delimiter.start + delimiter.length)
            if let event = parse(frame: Data(frame)) {
                events.append(event)
            }
        }
        return events
    }

    private func parse(frame: Data) -> Data? {
        var dataLines: [Data] = []
        var lineStart = frame.startIndex

        while lineStart < frame.endIndex {
            let lineEnd = frame[lineStart...].firstIndex(of: UInt8(ascii: "\n"))
                ?? frame.endIndex
            var line = frame.subdata(in: lineStart..<lineEnd)
            if line.last == UInt8(ascii: "\r") {
                line.removeLast()
            }

            if line.first != UInt8(ascii: ":"), line.starts(with: Data("data:".utf8)) {
                var value = line.dropFirst(5)
                if value.first == UInt8(ascii: " ") {
                    value = value.dropFirst()
                }
                dataLines.append(Data(value))
            }

            guard lineEnd < frame.endIndex else { break }
            lineStart = frame.index(after: lineEnd)
        }

        guard !dataLines.isEmpty else { return nil }
        return dataLines.dropFirst().reduce(dataLines[0]) { result, line in
            var combined = result
            combined.append(UInt8(ascii: "\n"))
            combined.append(line)
            return combined
        }
    }

    private struct Delimiter {
        let start: Int
        let length: Int
    }

    private func frameDelimiter(in data: Data) -> Delimiter? {
        let bytes = Array(data)
        guard bytes.count >= 2 else { return nil }

        for index in 0..<(bytes.count - 1) {
            if bytes[index] == 0x0A, bytes[index + 1] == 0x0A {
                return Delimiter(start: index, length: 2)
            }
            if index + 3 < bytes.count,
               bytes[index] == 0x0D, bytes[index + 1] == 0x0A,
               bytes[index + 2] == 0x0D, bytes[index + 3] == 0x0A {
                return Delimiter(start: index, length: 4)
            }
        }
        return nil
    }
}

/// URLSession-backed connection to a child `opencode serve` process.
public final class OpenCodeProcessConnection: OpenCodeConnection, @unchecked Sendable {
    private enum ConnectionError: Error, Sendable {
        case serverURLNotFound
        case invalidResponse
        case httpFailure(statusCode: Int)
    }

    private let baseURL: URL
    private let session: URLSession
    private let processTransport: ProcessTransport

    /// Starts `opencode serve` in the supplied working directory and waits for
    /// its stdout announcement before returning a usable connection.
    public init(cwd: String, session: URLSession = .shared) async throws {
        let transport = ProcessTransport(
            executable: "/bin/zsh",
            arguments: ["-lc", "exec opencode serve --port 0 --hostname 127.0.0.1"],
            cwd: cwd)

        do {
            try await transport.start()
            var serverURL: URL?
            for try await line in transport.lines() {
                if let url = Self.parseServerURL(
                    fromStdoutLine: String(decoding: line, as: UTF8.self)) {
                    serverURL = url
                    break
                }
            }
            guard let serverURL else { throw ConnectionError.serverURLNotFound }

            self.baseURL = serverURL
            self.session = session
            self.processTransport = transport
        } catch {
            await transport.terminate()
            throw error
        }
    }

    /// Convenience overload for callers that already have a directory URL.
    public convenience init(cwd: URL, session: URLSession = .shared) async throws {
        try await self.init(cwd: cwd.path, session: session)
    }

    /// Extracts the first `http://127.0.0.1:<port>` occurrence from a server
    /// startup line, regardless of the surrounding CLI wording.
    public static func parseServerURL(fromStdoutLine line: String) -> URL? {
        let prefix = "http://127.0.0.1:"
        guard let start = line.range(of: prefix) else { return nil }
        let portStart = start.upperBound
        let port = line[portStart...].prefix { character in
            character >= "0" && character <= "9"
        }
        guard !port.isEmpty else { return nil }

        let value = String(line[start.lowerBound..<portStart]) + port
        guard let url = URL(string: value),
              url.scheme == "http",
              url.host == "127.0.0.1",
              url.port != nil else { return nil }
        return url
    }

    public func request(method: String, path: String, body: Data?) async throws -> Data {
        var request = URLRequest(url: endpoint(path: path))
        request.httpMethod = method
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if body != nil {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }

        let (data, response) = try await session.data(for: request)
        try validate(response)
        return data
    }

    public func events() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    var request = URLRequest(url: endpoint(path: "/event"))
                    request.httpMethod = "GET"
                    request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                    let (bytes, response) = try await session.bytes(for: request)
                    try validate(response)

                    var parser = SSEParser()
                    var chunk = Data()
                    for try await byte in bytes {
                        chunk.append(byte)
                        if chunk.count >= 4_096 {
                            for event in parser.feed(chunk) {
                                continuation.yield(event)
                            }
                            chunk.removeAll(keepingCapacity: true)
                        }
                    }
                    for event in parser.feed(chunk) {
                        continuation.yield(event)
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    public func close() async {
        await processTransport.terminate()
    }

    private func endpoint(path: String) -> URL {
        baseURL.appendingPathComponent(path.trimmingCharacters(in: CharacterSet(charactersIn: "/")))
    }

    private func validate(_ response: URLResponse) throws {
        guard let http = response as? HTTPURLResponse else {
            throw ConnectionError.invalidResponse
        }
        guard (200..<300).contains(http.statusCode) else {
            throw ConnectionError.httpFailure(statusCode: http.statusCode)
        }
    }
}
