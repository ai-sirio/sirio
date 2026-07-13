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


// MARK: - Surface commands

/// Explicit --surface wins; otherwise TILLER_PANE_ID (set inside every
/// Tiller pane) is forwarded so a pane targets itself; otherwise the app
/// resolves its active pane.
func defaultSurface(_ explicit: String?) -> String? {
    explicit ?? ProcessInfo.processInfo.environment["TILLER_PANE_ID"]
}

struct NewSplit: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "new-split", abstract: "Split the active pane.")
    @OptionGroup var socketOptions: SocketOptions
    @Argument(help: "left | right | up | down") var direction: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSplit(direction: direction),
            socket: socketOptions.socket)
    }
}

struct ListPanels: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-panels", abstract: "List panes of the current worktree.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.surfaceList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "surfaces",
                  columns: ["id", "tab", "title", "agent", "active"],
                  asJSON: jsonFlag.json)
    }
}

struct ListPaneSurfaces: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-pane-surfaces", abstract: "List panes of the active tab.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.paneSurfaces(),
                                          socket: socketOptions.socket)
        printRows(response, key: "surfaces",
                  columns: ["id", "tab", "title", "agent", "active"],
                  asJSON: jsonFlag.json)
    }
}

struct FocusPanel: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "focus-panel", abstract: "Focus a pane (switches worktree/tab).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Pane UUID.") var panel: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceFocus(surface: panel),
            socket: socketOptions.socket)
    }
}

struct ClosePanel: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "close-panel",
        abstract: "Close a pane (default: your own).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Target pane UUID (default: this pane, else active pane).")
    var surface: String?
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceClose(surface: defaultSurface(surface)),
            socket: socketOptions.socket)
    }
}

struct Send: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "send", abstract: "Send text to a pane (default: active pane).")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Target pane UUID (default: this pane, else active pane).")
    var surface: String?
    @Argument(help: "Text to send.") var text: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSendText(text: text,
                                                    surface: defaultSurface(surface)),
            socket: socketOptions.socket)
    }
}

struct SendKey: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "send-key", abstract: "Send a key press to a pane.")
    @OptionGroup var socketOptions: SocketOptions
    @Option(help: "Target pane UUID (default: this pane, else active pane).")
    var surface: String?
    @Argument(help: "enter | tab | escape | backspace | delete | up | down | left | right")
    var key: String
    func run() throws {
        _ = try roundTripOrDie(
            TillerctlRequestBuilder.surfaceSendKey(key: key,
                                                   surface: defaultSurface(surface)),
            socket: socketOptions.socket)
    }
}


// MARK: - Notification commands

struct ListNotifications: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "list-notifications", abstract: "List delivered notifications.")
    @OptionGroup var socketOptions: SocketOptions
    @OptionGroup var jsonFlag: JSONFlag
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.notificationList(),
                                          socket: socketOptions.socket)
        printRows(response, key: "notifications",
                  columns: ["date", "title", "subtitle", "body"], asJSON: jsonFlag.json)
    }
}

struct ClearNotifications: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "clear-notifications", abstract: "Clear delivered notifications.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        _ = try roundTripOrDie(TillerctlRequestBuilder.notificationClear(),
                               socket: socketOptions.socket)
    }
}
// MARK: - Session restore

struct RestoreSession: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "restore-session",
        abstract: "Re-apply the layout Tiller loaded at launch.")
    @OptionGroup var socketOptions: SocketOptions
    func run() throws {
        let response = try roundTripOrDie(TillerctlRequestBuilder.sessionRestore(),
                                          socket: socketOptions.socket)
        print("restored \(response.result?["restored"] ?? "0") item(s)")
    }
}
