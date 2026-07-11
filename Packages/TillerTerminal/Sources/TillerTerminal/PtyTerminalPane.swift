import SwiftUI
import AppKit
import Foundation
import os
import TillerCore
import GhosttyTerminal
import Inject

/// Proxy for operations on the terminal surface from the context menu.
/// Holds a weak reference to the attached `TerminalSurface` once the
/// representable bridge creates it; until then copy/paste fall back to
/// no-ops because `TerminalSurface` binding actions are internal to
/// `GhosttyTerminal`. `copyContextToPasteboard` always works via the
/// public `InMemoryTerminalSession.readViewportText()` API.
@MainActor
public final class TerminalSurfaceProxy: @unchecked Sendable {
    weak var surface: TerminalSurface?
    weak var view: TerminalView?
    let session: InMemoryTerminalSession

    nonisolated init(surface: TerminalSurface? = nil, session: InMemoryTerminalSession) {
        self.surface = surface
        self.session = session
    }

    public var canCopy: Bool {
        view != nil
    }

    public var canPaste: Bool {
        view != nil
    }

    @discardableResult
    public func copySelectionToPasteboard() -> Bool {
        guard let view else { return false }
        return view.copySelectedTextToPasteboard()
    }

    @discardableResult
    public func pasteFromPasteboard() -> Bool {
        guard let view else { return false }
        return view.performBindingAction("paste_from_clipboard")
    }

    @discardableResult
    public func clearScreen() -> Bool {
        guard let view else { return false }
        return view.performBindingAction("clear_screen")
    }

    public func copyContextToPasteboard() {
        guard let text = session.readViewportText(), !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }
}

/// PTY-backed terminal surface: we own the process (PtyProcess), libghostty
/// only renders. `write:` (user keystrokes from the surface) goes to the PTY;
/// PTY output is fed back via `session.receive`. This is the architecture
/// every Tiller terminal uses from Fase 1 on.
public struct PtyTerminalPane: View {
    @ObserveInjection var inject
    @StateObject private var state = TerminalViewState(theme: TillerTerminalTheme.current())
    @AppStorage(AppSettings.terminalFontSizeKey)
    private var terminalFontSize = AppSettings.defaultTerminalFontSize
    private let workingDirectory: String?
    private let command: String?
    private let paneId: UUID
    private let initialScrollback: Data?
    private let extraEnvironment: [String: String]
    private let onScrollback: (@Sendable (UUID, Data) async -> Void)?
    private let onTitleChange: ((UUID, String) -> Void)?
    private let onContentSignal: ((UUID, String) -> Void)?
    private let onOpenURL: ((UUID, String) -> Void)?
    private let onContextMenu: ((UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem])?
    @State private var runtime: PtyRuntime?

    public init(
        workingDirectory: String? = nil,
        command: String? = nil,
        paneId: UUID = UUID(),
        initialScrollback: Data? = nil,
        extraEnvironment: [String: String] = [:],
        onScrollback: (@Sendable (UUID, Data) async -> Void)? = nil,
        onTitleChange: ((UUID, String) -> Void)? = nil,
        onContentSignal: ((UUID, String) -> Void)? = nil,
        onOpenURL: ((UUID, String) -> Void)? = nil,
        onContextMenu: ((UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem])? = nil
    ) {
        self.workingDirectory = workingDirectory
        self.command = command
        self.paneId = paneId
        self.initialScrollback = initialScrollback
        self.extraEnvironment = extraEnvironment
        self.onScrollback = onScrollback
        self.onTitleChange = onTitleChange
        self.onContentSignal = onContentSignal
        self.onOpenURL = onOpenURL
        self.onContextMenu = onContextMenu
    }

    public var body: some View {
        TerminalSurfaceView(context: state)
            .onAppear {
                guard runtime == nil else { return }
                let rt = PtyRuntime(
                    workingDirectory: workingDirectory,
                    command: command,
                    paneId: paneId,
                    initialScrollback: initialScrollback,
                    extraEnvironment: extraEnvironment
                )
                rt.onContentSignal = onContentSignal
                runtime = rt
                PaneProxyRegistry.shared.register(paneId: paneId, proxy: rt.proxy)
                state.configuration = TerminalSurfaceOptions(backend: .inMemory(rt.session))
                rt.start()
                TerminalOpenURLRouter.register(state) { [paneId, onOpenURL] url in
                    onOpenURL?(paneId, url)
                }
            }
            .onDisappear {
                TerminalOpenURLRouter.unregister(state)
                PaneProxyRegistry.shared.unregister(paneId: paneId)
                runtime?.stop()
                if let onScrollback, let rt = runtime {
                    Task { await onScrollback(paneId, await rt.scrollback.snapshot()) }
                }
            }
            .onChange(of: state.surfaceSize) { _, _ in
                runtime?.updateSurface(state.surface)
            }
            .onChange(of: state.title) { _, newTitle in
                onTitleChange?(paneId, newTitle)
            }
            .onAppear { applyFontSize() }
            .onChange(of: terminalFontSize) { _, _ in applyFontSize() }
            .enableInjection()
    }

    private func applyFontSize() {
        let size = Float(AppSettings.clampTerminalFontSize(terminalFontSize))
        state.setTheme(TillerTerminalTheme.theme(fontSize: size))
    }
}

/// `@unchecked Sendable` justification: `session`, `pty`, `proxy`, and
/// `debouncer` are immutable `let` references assigned in `init`; each has
/// its own synchronization (PtyProcess serializes on its private queue,
/// InMemoryTerminalSession is libghostty's thread-safe ingest API, the
/// debouncer confines state to its serial queue, and the proxy is confined
/// to the main actor). The spawn/stop state machine fields below are guarded
/// by `stateLock` because settle callbacks arrive on the debouncer queue while
/// start()/stop() run on the main thread.
final class PtyRuntime: @unchecked Sendable {
    let session: InMemoryTerminalSession
    let scrollback: ScrollbackBuffer
    let proxy: TerminalSurfaceProxy
    /// Layer-C callback: fires with the pane id and an ANSI-stripped tail
    /// snapshot each time PTY output settles. Set by `PtyTerminalPane` right
    /// after construction, same pattern as `onTitleChange`'s SwiftUI side.
    var onContentSignal: ((UUID, String) -> Void)?
    private weak var surface: TerminalSurface?
    private let pty: PtyProcess
    private let debouncer: ResizeDebouncer
    private let outputSettle: OutputSettleDebouncer
    private let workingDirectory: String?
    private let command: String?
    private let paneId: UUID
    private let initialScrollback: Data?
    private let extraEnvironment: [String: String]

    private let stateLock = NSLock()
    private var startRequested = false
    private var spawned = false
    private var stopped = false
    private var settledSize: (cols: UInt16, rows: UInt16)?

    init(workingDirectory: String? = nil, command: String? = nil, paneId: UUID = UUID(),
         initialScrollback: Data? = nil, extraEnvironment: [String: String] = [:]) {
        self.workingDirectory = workingDirectory
        self.command = command
        self.paneId = paneId
        self.initialScrollback = initialScrollback
        self.extraEnvironment = extraEnvironment
        let scrollback = ScrollbackBuffer()
        self.scrollback = scrollback
        let handle = PtyHandle()
        let debouncer = ResizeDebouncer()
        self.debouncer = debouncer
        let outputSettle = OutputSettleDebouncer()
        self.outputSettle = outputSettle
        // Session write: bytes the user types on the surface → PTY stdin.
        // Session resize: grid changes → debounced → spawn or TIOCSWINSZ.
        // Note: actual API field is `columns` (not `cols` as in brief pseudocode).
        let session = InMemoryTerminalSession(
            write: { data in handle.pty?.write(data) },
            resize: { viewport in
                debouncer.push(cols: viewport.columns, rows: viewport.rows)
            }
        )
        self.session = session
        self.proxy = TerminalSurfaceProxy(session: session)
        // PTY output → surface + scrollback + Layer-C settle trigger.
        let pty = PtyProcess { data in
            session.receive(data)
            Task { await scrollback.append(data) }
            outputSettle.push()
        }
        handle.pty = pty
        self.pty = pty
        debouncer.onSettle = { [weak self] cols, rows in
            self?.handleSettledResize(cols: cols, rows: rows)
        }
        outputSettle.onSettle = { [weak self] in
            Task { await self?.emitContentSignal() }
        }
    }

    private func emitContentSignal() async {
        let data = await scrollback.snapshot()
        let text = stripANSI(String(decoding: data, as: UTF8.self))
        let lines = text.split(separator: "\n", omittingEmptySubsequences: false)
        let tail = lines.suffix(40).joined(separator: "\n")
        guard !tail.isEmpty else { return }
        onContentSignal?(paneId, tail)
    }

    func updateSurface(_ surface: TerminalSurface?) {
        MainActor.assumeIsolated {
            self.surface = surface
            proxy.surface = surface
        }
    }

    func start() {
        // Replay saved scrollback before spawning so the user sees previous
        // output above the fresh shell prompt.
        if let initialScrollback, !initialScrollback.isEmpty {
            session.receive(initialScrollback)
            session.receive(Data("\r\n".utf8))
        }
        stateLock.lock()
        startRequested = true
        let size = settledSize
        stateLock.unlock()
        if let size {
            spawnIfNeeded(cols: size.cols, rows: size.rows)
        }
        // Belt: a pane whose surface never reports a size must not stay a
        // dead shell-less pane forever. Every mounted host gets a layout
        // pass, so in practice the settled resize wins this race.
        DispatchQueue.global().asyncAfter(deadline: .now() + 1.0) { [weak self] in
            self?.spawnFallback()
        }
    }

    /// Debounced viewport settle. The first one spawns the shell at the
    /// real grid size (so zsh never sees a startup SIGWINCH); later ones
    /// forward TIOCSWINSZ once per gesture instead of once per layout tick.
    func handleSettledResize(cols: UInt16, rows: UInt16) {
        stateLock.lock()
        settledSize = (cols, rows)
        let ready = startRequested && !spawned && !stopped
        let alive = spawned && !stopped
        stateLock.unlock()
        if ready {
            spawnIfNeeded(cols: cols, rows: rows)
        } else if alive {
            pty.resize(cols: cols, rows: rows)
        }
    }

    private func spawnFallback() {
        stateLock.lock()
        let size = settledSize ?? (cols: 80, rows: 24)
        stateLock.unlock()
        spawnIfNeeded(cols: size.cols, rows: size.rows)
    }

    private func spawnIfNeeded(cols: UInt16, rows: UInt16) {
        stateLock.lock()
        guard startRequested, !spawned, !stopped else {
            stateLock.unlock()
            return
        }
        spawned = true
        stateLock.unlock()
        let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        // xterm-ghostty terminfo ships with Ghostty.app, not with libghostty:
        // on machines without it, zsh gets no erase/backspace capabilities
        // (backspace prints spaces). Fall back to the universally installed
        // xterm-256color unless the ghostty entry actually exists.
        let term = Self.hasGhosttyTerminfo ? "xterm-ghostty" : "xterm-256color"
        let env = PtyEnvironment.compose(
            base: ProcessInfo.processInfo.environment,
            term: term,
            extra: extraEnvironment,
            paneId: paneId
        )
        let args: [String]
        if let command {
            args = ["-l", "-c", command]
        } else {
            args = ["-l"]
        }
        // Notify PaneRegistry when the process exits.
        pty.onExit = { [self] code in
            Task { await PaneRegistry.shared.markExited(paneId: paneId, code: code) }
        }
        do {
            try pty.spawn(
                executable: shell,
                arguments: args,
                environment: env,
                workingDirectory: workingDirectory,
                initialCols: cols,
                initialRows: rows
            )
        } catch {
            Self.logger.error("pty spawn failed for pane \(self.paneId): \(error)")
            stateLock.lock(); stopped = true; stateLock.unlock()
            return
        }
        Task { [self] in await PaneRegistry.shared.register(paneId: paneId, pty: pty, scrollback: scrollback) }
    }

    func stop() {
        stateLock.lock()
        stopped = true
        stateLock.unlock()
        debouncer.cancel()
        pty.terminate()
        Task { [self] in await PaneRegistry.shared.unregister(paneId: paneId) }
    }

    private static let logger = Logger(subsystem: "dev.tiller", category: "pane")

    private static let hasGhosttyTerminfo: Bool = {
        ["/usr/share/terminfo", "\(NSHomeDirectory())/.terminfo"].contains {
            FileManager.default.fileExists(atPath: "\($0)/x/xterm-ghostty")
                || FileManager.default.fileExists(atPath: "\($0)/78/xterm-ghostty")
        }
    }()
}

/// Break the PtyRuntime → PtyProcess → session circular init dependency.
/// Swift concurrency checking rejects capturing an uninitialized `var` in
/// a `@Sendable` closure; this holder lets us set the PTY after construction.
private final class PtyHandle: @unchecked Sendable {
    var pty: PtyProcess?
}
