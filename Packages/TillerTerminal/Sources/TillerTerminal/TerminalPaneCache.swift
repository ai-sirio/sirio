import AppKit

/// Cache condivisa (una per worktree) dei controller di pane terminale.
/// Passata a ogni TerminalSplitHost del worktree, permette a un pane
/// spostato tra tab di riagganciare lo stesso NSHostingController: il PTY
/// sopravvive allo spostamento. Senza cache condivisa ogni host tiene la
/// propria (comportamento precedente, ancora il default).
@MainActor
public final class TerminalPaneCache {
    public var controllers: [UUID: NSViewController] = [:]

    public init() {}

    /// Scarta i controller dei pane non più presenti in alcun tab del
    /// worktree; il deinit del controller fa il teardown del PTY.
    public func prune(keeping: Set<UUID>) {
        for key in controllers.keys where !keeping.contains(key) {
            controllers.removeValue(forKey: key)
        }
    }
}
