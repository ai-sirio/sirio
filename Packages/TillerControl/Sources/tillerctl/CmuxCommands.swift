import ArgumentParser
import Foundation
import TillerControl

// MARK: - Shared CLI plumbing for the flat cmux-parity commands

struct JSONFlag: ParsableArguments {
    @Flag(name: .customLong("json"), help: "Output raw JSON.") var json = false
}

func roundTripOrDie(_ request: ControlRequest, socket: String) throws -> ControlResponse {
    let response: ControlResponse
    do {
        response = try ControlClient.roundTrip(socketPath: socket, request: request)
    } catch {
        // Connection-level failure (socket file missing / connection refused):
        // the spec's canonical message for a disabled socket or dead app.
        throw ValidationError(
            "Tiller control socket is disabled or Tiller is not running (\(error))")
    }
    guard response.ok else {
        throw ValidationError(response.error ?? "\(request.method) failed")
    }
    return response
}

/// Print a rows result: `--json` prints the embedded JSON array verbatim;
/// otherwise one line per row, tab-separated in `columns` order.
func printRows(_ response: ControlResponse, key: String, columns: [String], asJSON: Bool) {
    let raw = response.result?[key] ?? "[]"
    if asJSON { print(raw); return }
    for row in ControlRows.decode(raw) ?? [] {
        print(columns.map { row[$0] ?? "" }.joined(separator: "\t"))
    }
}

/// Print a single-object result: `--json` re-encodes result as JSON.
func printResult(_ response: ControlResponse, columns: [String], asJSON: Bool) {
    let result = response.result ?? [:]
    if asJSON {
        print(ControlRows.encode([result]))
        return
    }
    print(columns.compactMap { result[$0] }.joined(separator: "\t"))
}

// MARK: - Utility commands

struct Ping: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "ping", abstract: "Check that Tiller is running and responding.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        _ = try roundTripOrDie(TillerctlRequestBuilder.systemPing(),
                               socket: socketOptions.socket)
        print("pong")
    }
}

struct Capabilities: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "capabilities", abstract: "List available socket methods.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.systemCapabilities(),
                                          socket: socketOptions.socket)
        printRows(response, key: "methods", columns: ["method"], asJSON: jsonFlag.json)
    }
}

struct Identify: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "identify", abstract: "Show the current workspace/surface context.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let env = ProcessInfo.processInfo.environment
        let response = try roundTripOrDie(
            TillerctlRequestBuilder.systemIdentify(
                worktree: env["TILLER_WORKTREE_ID"], pane: env["TILLER_PANE_ID"]),
            socket: socketOptions.socket)
        printResult(response,
                    columns: ["project", "branch", "path", "workspaceId", "surfaceId"],
                    asJSON: jsonFlag.json)
    }
}
