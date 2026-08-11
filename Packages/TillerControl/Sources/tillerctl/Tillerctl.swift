import ArgumentParser
import Foundation
import TillerControl
import TillerCore

extension TerminalKey: @retroactive ExpressibleByArgument {}

@main
struct Tillerctl: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "tillerctl",
        abstract: "Control a running Tiller.app over its unix socket.",
        subcommands: [
            Panel.self, Notify.self, SessionRef.self, Worktree.self,
            // cmux-parity flat commands
            Ping.self, Capabilities.self, Identify.self,
            ListWorkspaces.self, NewWorkspace.self, SelectWorkspace.self,
            CurrentWorkspace.self, CloseWorkspace.self,
            ListNotifications.self, ClearNotifications.self,
            RestoreSession.self,
            Browser.self,
        ]
    )
}

struct SocketOptions: ParsableArguments {
    @Option(help: "Socket path (default: $TILLER_SOCKET or app support).")
    var socket: String = ControlSocket.defaultPath()
}

enum PanelDirection: String, CaseIterable, ExpressibleByArgument {
    case left, right, up, down

    init?(argument: String) {
        self.init(rawValue: argument)
    }
}

func requiredPanelTarget(
    explicit: String?,
    environment: [String: String],
    key: String,
    option: String
) throws -> String {
    if let explicit, !explicit.isEmpty { return explicit }
    if let value = environment[key], !value.isEmpty { return value }
    throw ValidationError("Missing \(option) and $\(key) is not set")
}

func createdPaneOutput(id: String, json: Bool) throws -> String {
    guard json else { return id }
    let data = try JSONSerialization.data(
        withJSONObject: ["id": id],
        options: [.sortedKeys]
    )
    return String(decoding: data, as: UTF8.self)
}

func requireCreatedPaneID(_ response: ControlResponse) throws -> String {
    guard response.ok, let id = response.result?["id"], !id.isEmpty else {
        throw ValidationError(response.error ?? "panel creation returned no id")
    }
    return id
}

func panelWaitTimeoutSeconds(timeoutMs: Int?) -> Int? {
    guard let timeoutMs else { return nil }
    return max(1, (timeoutMs + 999) / 1_000 + 30)
}

struct Panel: ParsableCommand {
    static let configuration = CommandConfiguration(
        subcommands: [
            Create.self, Split.self, List.self, Write.self, Key.self,
            Read.self, Wait.self, Focus.self, Close.self,
        ]
    )

    struct Create: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option(name: .customLong("worktree")) var worktree: String?
        @Option(name: .customLong("cmd")) var cmd: String?
        @Flag(name: .customLong("json")) var json = false

        func run() throws {
            let worktree = try requiredPanelTarget(
                explicit: worktree,
                environment: ProcessInfo.processInfo.environment,
                key: "TILLER_WORKTREE_ID",
                option: "--worktree"
            )
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.panelCreate(worktree: worktree, cmd: cmd),
                socket: socketOptions.socket
            )
            let id = try requireCreatedPaneID(response)
            print(try createdPaneOutput(id: id, json: json))
        }
    }

    struct Split: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Argument(help: "left | right | up | down") var direction: PanelDirection
        @Option(name: .customLong("from")) var from: String?
        @Option(name: .customLong("cmd")) var cmd: String?
        @Flag(name: .customLong("json")) var json = false

        func run() throws {
            let source = try requiredPanelTarget(
                explicit: from,
                environment: ProcessInfo.processInfo.environment,
                key: "TILLER_PANE_ID",
                option: "--from"
            )
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.panelSplit(
                    from: source,
                    direction: direction.rawValue,
                    cmd: cmd
                ),
                socket: socketOptions.socket
            )
            print(try createdPaneOutput(
                id: requireCreatedPaneID(response),
                json: json
            ))
        }
    }

    struct List: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option(name: .customLong("worktree")) var worktree: String?
        @Flag(name: .customLong("json")) var json = false

        func run() throws {
            let worktree = try requiredPanelTarget(
                explicit: worktree,
                environment: ProcessInfo.processInfo.environment,
                key: "TILLER_WORKTREE_ID",
                option: "--worktree"
            )
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.panelList(worktree: worktree),
                socket: socketOptions.socket
            )
            printRows(
                response,
                key: "panels",
                columns: ["id", "tab", "title", "agent", "active"],
                asJSON: json
            )
        }
    }

    struct Write: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        @Option var input: String
        @Flag(help: "Append Enter.") var enter = false

        func run() throws {
            let payload = input + (enter ? "\r" : "")
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.panelWrite(id: id, input: payload),
                socket: socketOptions.socket
            )
        }
    }

    struct Key: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        @Argument(help: "enter | tab | escape | backspace | delete | up | down | left | right")
        var key: TerminalKey

        func run() throws {
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.panelKey(id: id, key: key.rawValue),
                socket: socketOptions.socket
            )
        }
    }

    struct Read: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String

        func run() throws {
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.panelRead(id: id),
                socket: socketOptions.socket
            )
            guard let encoded = response.result?["output"],
                  let data = Data(base64Encoded: encoded) else {
                throw ValidationError("panel.read returned invalid output")
            }
            FileHandle.standardOutput.write(data)
        }
    }

    struct Wait: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        @Option(name: .customLong("timeout-ms")) var timeoutMs: Int?

        func run() throws {
            if let timeoutMs, timeoutMs < 0 {
                throw ValidationError("--timeout-ms must be non-negative")
            }
            let response = try roundTripOrDie(
                TillerctlRequestBuilder.panelWait(id: id, timeoutMs: timeoutMs),
                socket: socketOptions.socket,
                timeoutSeconds: panelWaitTimeoutSeconds(timeoutMs: timeoutMs)
            )
            guard let codeString = response.result?["exitCode"],
                  let code = Int32(codeString) else {
                throw ValidationError("panel.wait returned no exit code")
            }
            throw ExitCode(code)
        }
    }

    struct Focus: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String

        func run() throws {
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.panelFocus(id: id),
                socket: socketOptions.socket
            )
        }
    }

    struct Close: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String?

        func run() throws {
            let target = try requiredPanelTarget(
                explicit: id,
                environment: ProcessInfo.processInfo.environment,
                key: "TILLER_PANE_ID",
                option: "--id"
            )
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.panelClose(id: target),
                socket: socketOptions.socket
            )
        }
    }
}

struct Notify: ParsableCommand {
    static let configuration = CommandConfiguration(
        abstract: "Agent-status update (--session/--status) or user notification (--title/--body)."
    )
    @OptionGroup var socketOptions: SocketOptions
    // Mode 1: agent status (used by agent hooks — do not change semantics).
    @Option var session: String?
    @Option var status: String?   // running | needs-input | done | error
    @Option(name: .customLong("agent-session"),
            help: "Agent-native session reference for restore.")
    var agentSession: String?
    @Flag(name: .customLong("stdin-json"),
          help: "Read a hook JSON payload from stdin and extract the agent session id.")
    var stdinJSON = false
    // Mode 2: user-visible notification (cmux parity).
    @Option var title: String?
    @Option var subtitle: String?
    @Option var body: String?
    // Agent hooks may append extra positional payload (e.g. Codex notify JSON) —
    // scanned for a session reference, otherwise ignored.
    @Argument(parsing: .allUnrecognized) var extra: [String] = []

    func validate() throws {
        do {
            _ = try NotifyMode.resolve(session: session, status: status,
                                       title: title, body: body)
        } catch let error as NotifyModeError {
            throw ValidationError(error.usageMessage)
        }
    }

    func run() throws {
        if let title {
            _ = try roundTripOrDie(
                TillerctlRequestBuilder.notificationCreate(
                    title: title, subtitle: subtitle, body: body ?? ""),
                socket: socketOptions.socket)
            return
        }
        // Agent-status mode: unchanged behavior.
        var ref = agentSession
        if ref == nil, stdinJSON {
            let data = readBoundedStdin()
            ref = AgentSessionExtractor.sessionRef(fromJSON: data)
        }
        if ref == nil {
            ref = AgentSessionExtractor.sessionRef(fromPayloadArguments: extra)
        }
        let response = try ControlClient.roundTrip(
            socketPath: socketOptions.socket,
            request: TillerctlRequestBuilder.notify(
                session: session!, status: status!, agentSession: ref)
        )
        guard response.ok else { throw ValidationError(response.error ?? "notify failed") }
    }
}

struct SessionRef: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "session-ref",
        abstract: "Report an agent-native session reference for a pane."
    )
    @OptionGroup var socketOptions: SocketOptions
    @Option var session: String
    @Option var ref: String
    func run() throws {
        let response = try ControlClient.roundTrip(
            socketPath: socketOptions.socket,
            request: TillerctlRequestBuilder.sessionRef(session: session, ref: ref)
        )
        guard response.ok else { throw ValidationError(response.error ?? "session-ref failed") }
    }
}

struct Worktree: ParsableCommand {
    static let configuration = CommandConfiguration(
        commandName: "worktree",
        subcommands: [Set.self]
    )

    struct Set: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option(name: .customLong("worktree"), help: "Worktree UUID or absolute path (e.g. $PWD).")
        var worktree: String
        @Option var comment: String
        func run() throws {
            let response = try ControlClient.roundTrip(
                socketPath: socketOptions.socket,
                request: TillerctlRequestBuilder.worktreeSet(worktree: worktree, comment: comment)
            )
            guard response.ok else { throw ValidationError(response.error ?? "worktree.set failed") }
        }
    }
}
