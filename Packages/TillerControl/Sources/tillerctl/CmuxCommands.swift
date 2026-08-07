import ArgumentParser
import Foundation
import TillerControl

// MARK: - Shared CLI plumbing for the flat cmux-parity commands

struct JSONFlag: ParsableArguments {
    @Flag(name: .customLong("json"), help: "Output raw JSON.") var json = false
}

func roundTripOrDie(
    _ request: ControlRequest, socket: String, timeoutSeconds: Int? = 3600
) throws -> ControlResponse {
    let response: ControlResponse
    do {
        response = try ControlClient.roundTrip(
            socketPath: socket, request: request, timeoutSeconds: timeoutSeconds)
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

enum BrowserNavigationAction: String, CaseIterable, ExpressibleByArgument {
    case back, forward, reload
}

enum BrowserGetValue: String, CaseIterable, ExpressibleByArgument {
    case url, text, html
}

enum BrowserIDFormat: String, CaseIterable, ExpressibleByArgument {
    case uuids, both
}

struct Browser: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "browser",
        abstract: "Open and read browser surfaces.",
        subcommands: [Open.self, Navigate.self, Get.self, Screenshot.self,
                      Snapshot.self, Act.self, Wait.self, Eval.self, Console.self])

    struct Open: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument(help: "URL to open.") var url: String
        @Option(name: .customLong("workspace")) var workspace: String?
        @Option(name: .customLong("window")) var window: String?
        @Option(name: .customLong("id-format")) var idFormat: BrowserIDFormat = .uuids
        @OptionGroup var jsonFlag: JSONFlag

        func run() throws {
            let implicitWorkspace = workspace
                ?? ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserOpen(
                    url: url, workspace: implicitWorkspace, window: window,
                    idFormat: idFormat == .uuids ? nil : idFormat.rawValue),
                socket: socketOptions.socket)
            printResult(
                response, columns: ["surface", "surfaceRef", "url", "title"],
                asJSON: jsonFlag.json)
        }
    }

    struct Navigate: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Argument var action: BrowserNavigationAction

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserNavigate(
                    surface: surface, action: action.rawValue, workspace: workspace),
                socket: socketOptions.socket)
            printResult(response, columns: ["url", "title"], asJSON: false)
        }
    }

    struct Get: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Argument var what: BrowserGetValue
        @Option var selector: String?
        @OptionGroup var jsonFlag: JSONFlag

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserGet(
                    surface: surface, what: what.rawValue, selector: selector,
                    workspace: workspace),
                socket: socketOptions.socket)
            printResult(response, columns: ["value"], asJSON: jsonFlag.json)
        }
    }

    struct Screenshot: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Option var path: String?

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserScreenshot(
                    surface: surface, path: path, workspace: workspace),
                socket: socketOptions.socket)
            print(response.result?["path"] ?? "")
        }
    }

    struct Snapshot: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @OptionGroup var jsonFlag: JSONFlag

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserSnapshot(surface: surface, workspace: workspace),
                socket: socketOptions.socket)
            if jsonFlag.json {
                printResult(response, columns: ["generation", "nodes"], asJSON: true)
            } else {
                print(response.result?["generation"] ?? "")
                print(response.result?["nodes"] ?? "[]")
            }
        }
    }

    enum ActVerb: String, CaseIterable, ExpressibleByArgument {
        case click, fill, type, press, scroll
    }

    struct Act: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Argument var verb: ActVerb
        @Option(name: .customLong("ref")) var ref: String?
        @Option var selector: String?
        @Option var value: String?
        @Option var key: String?
        @Option var generation: Int?
        @Flag(name: .customLong("snapshot-after")) var snapshotAfter = false
        @Option(name: .customLong("delta-x")) var deltaX: Double?
        @Option(name: .customLong("delta-y")) var deltaY: Double?

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserAct(
                    surface: surface, verb: verb.rawValue, ref: ref, selector: selector,
                    value: value, key: key, generation: generation,
                    snapshotAfter: snapshotAfter, deltaX: deltaX, deltaY: deltaY,
                    workspace: workspace), socket: socketOptions.socket)
            printResult(response, columns: ["ok", "generation", "nodes"], asJSON: true)
        }
    }

    struct Wait: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Option var selector: String?
        @Option var text: String?
        @Option(name: .customLong("url-contains")) var urlContains: String?
        @Option(name: .customLong("load-state")) var loadState: String?
        @Option var function: String?
        @Option var timeoutMs: Int

        func run() throws {
            let values: [(String, String?)] = [
                ("selector", selector), ("text", text), ("urlContains", urlContains),
                ("loadState", loadState), ("function", function),
            ]
            guard let (key, value) = values.first(where: { $0.1 != nil }),
                  values.filter({ $0.1 != nil }).count == 1,
                  let value else {
                throw ValidationError("pass exactly one wait condition")
            }
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserWait(
                    surface: surface, conditionKey: key, conditionValue: value,
                    timeoutMs: timeoutMs, workspace: workspace), socket: socketOptions.socket)
            printResult(response, columns: ["ok", "elapsedMs"], asJSON: true)
        }
    }

    struct Eval: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Argument var script: String
        @OptionGroup var jsonFlag: JSONFlag

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserEval(
                    surface: surface, script: script, workspace: workspace),
                socket: socketOptions.socket)
            printResult(response, columns: ["value"], asJSON: jsonFlag.json)
        }
    }

    struct Console: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument var surface: String
        @Option var since: Double?

        func run() throws {
            let workspace = ProcessInfo.processInfo.environment["TILLER_WORKTREE_ID"]
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.browserConsole(
                    surface: surface, since: since, workspace: workspace),
                socket: socketOptions.socket)
            print(response.result?["entries"] ?? "[]")
        }
    }
}
