import Foundation
import TillerAgents

/// Layer D: identifies which catalog agent (if any) is running inside a
/// pane's shell by inspecting the shell's child processes — the only signal
/// that catches agents which never emit an OSC title (Codex). Agents that
/// run as interpreter processes ("node", "bun") are invisible here and rely
/// on title identification instead (pi, omp).
enum ForegroundProcessAgent {
    /// Catalog ids whose CLI runs as a native binary of the same name.
    private static let matchableIds = Set(AgentCatalog.all.map(\.id))

    /// First catalog agent found among the direct children of `shellPid`,
    /// in catalog order. Called off the main thread; libproc calls are
    /// cheap (microseconds) but this still runs only on content signals.
    static func identify(shellPid: pid_t) -> String? {
        let names = childProcessNames(of: shellPid)
        guard !names.isEmpty else { return nil }
        return AgentCatalog.all.map(\.id).first { names.contains($0) }
    }

    private static func childProcessNames(of pid: pid_t) -> Set<String> {
        var pids = [pid_t](repeating: 0, count: 64)
        let byteCount = proc_listchildpids(pid, &pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        guard byteCount > 0 else { return [] }
        let count = min(Int(byteCount) / MemoryLayout<pid_t>.size, pids.count)
        var names: Set<String> = []
        var buffer = [CChar](repeating: 0, count: Int(MAXCOMLEN) * 2 + 1)
        for child in pids.prefix(count) where child > 0 {
            buffer[0] = 0
            guard proc_name(child, &buffer, UInt32(buffer.count)) > 0 else { continue }
            names.insert(String(cString: buffer))
        }
        return names
    }
}
