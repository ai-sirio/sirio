import ArgumentParser
import Foundation
import TillerControl

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
            NewSplit.self, ListPanels.self, ListPaneSurfaces.self, FocusPanel.self,
            Send.self, SendKey.self, ClosePanel.self,
            ListNotifications.self, ClearNotifications.self,
            RestoreSession.self,
        ]
    )
}

struct SocketOptions: ParsableArguments {
    @Option(help: "Socket path (default: $TILLER_SOCKET or app support).")
    var socket: String = ControlSocket.defaultPath()
}

struct Panel: ParsableCommand {
    static let configuration = CommandConfiguration(
        subcommands: [Create.self, Write.self, Read.self, Wait.self]
    )

    struct Create: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option(name: .customLong("worktree")) var worktree: String
        @Option(name: .customLong("cmd")) var cmd: String?
        func run() throws {
            let response = try ControlClient.roundTrip(
                socketPath: socketOptions.socket,
                request: TillerctlRequestBuilder.panelCreate(worktree: worktree, cmd: cmd)
            )
            guard response.ok, let panelId = response.result?["panelId"] else {
                throw ValidationError(response.error ?? "panel.create failed")
            }
            print(panelId)
        }
    }

    struct Write: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        @Option var input: String
        @Flag(help: "Append newline (send Enter).") var enter = false
        func run() throws {
            let payload = enter ? input + "\r" : input
            let response = try ControlClient.roundTrip(
                socketPath: socketOptions.socket,
                request: TillerctlRequestBuilder.panelWrite(id: id, input: payload)
            )
            guard response.ok else { throw ValidationError(response.error ?? "panel.write failed") }
        }
    }

    struct Read: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        func run() throws {
            let response = try ControlClient.roundTrip(
                socketPath: socketOptions.socket,
                request: TillerctlRequestBuilder.panelRead(id: id)
            )
            guard response.ok, let b64 = response.result?["output"],
                  let data = Data(base64Encoded: b64) else {
                throw ValidationError(response.error ?? "panel.read failed")
            }
            FileHandle.standardOutput.write(data)
        }
    }

    struct Wait: ParsableCommand {
        @OptionGroup var socketOptions: SocketOptions
        @Option var id: String
        @Option(name: .customLong("timeout-ms")) var timeoutMs: Int?
        func run() throws {
            let response = try ControlClient.roundTrip(
                socketPath: socketOptions.socket,
                request: TillerctlRequestBuilder.panelWait(id: id, timeoutMs: timeoutMs),
                timeoutSeconds: timeoutMs.map { $0 / 1000 + 30 } ?? 86_400
            )
            guard response.ok, let codeString = response.result?["exitCode"],
                  let code = Int32(codeString) else {
                throw ValidationError(response.error ?? "panel.wait failed/timeout")
            }
            throw ExitCode(code)
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
            let data = FileHandle.standardInput.readDataToEndOfFile()
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
