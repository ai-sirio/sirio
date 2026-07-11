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


// MARK: - Workspace commands

struct ListWorkspaces: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-workspaces", abstract: "List all worktrees.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.workspaceList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "workspaces",
                  columns: ["id", "project", "branch", "path", "selected"],
                  asJSON: jsonFlag.json)
    }
}

struct NewWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "new-workspace", abstract: "Create a worktree in a project.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Project UUID or name.") var project: String
    @Option(help: "Branch name (default: generated).") var branch: String?
    func run() throws {
        let response = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceCreate(project: project, branch: branch),
            socket: socketOptions.socket)
        print(response.result?["id"] ?? "")
    }
}

struct SelectWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "select-workspace", abstract: "Select a worktree in the sidebar.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Worktree UUID or absolute path.") var workspace: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceSelect(workspace: workspace),
            socket: socketOptions.socket)
    }
}

struct CurrentWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "current-workspace", abstract: "Show the selected worktree.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.workspaceCurrent(),
                                          socket: socketOptions.socket)
        printResult(response, columns: ["project", "branch", "path", "id"],
                    asJSON: jsonFlag.json)
    }
}

struct CloseWorkspace: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "close-workspace",
        abstract: "Unmount a worktree's terminals (worktree stays in sidebar).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Worktree UUID or absolute path.") var workspace: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.workspaceClose(workspace: workspace),
            socket: socketOptions.socket)
    }
}
