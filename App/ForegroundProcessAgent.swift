import Foundation
import TillerAgents
import TillerCore
import TillerTerminal

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
        let sid = SignpostMetrics.makeSignpostID()
        let state = SignpostMetrics.beginInterval("processScan", id: sid)
        let names = childProcessNames(of: shellPid)
        defer {
            SignpostMetrics.endInterval(
                "processScan", state, message: "processes: \(names.count)")
        }
        guard !names.isEmpty else { return nil }
        return AgentCatalog.all.map(\.id).first { names.contains($0) }
    }

    // MARK: - Recursive tree (Agents panel)

    /// Recursive snapshot of the shell's process subtree. Caps guard against
    /// runaway trees; excess is silently truncated.
    static func processTree(shellPid: pid_t,
                            maxDepth: Int = 5, maxCount: Int = 50) -> [ProcessNode] {
        buildTree(roots: childPids(of: shellPid),
                  childrenOf: childPids(of:),
                  nameOf: processName(of:),
                  maxDepth: maxDepth, maxCount: maxCount)
    }

    /// Generic core, injectable for tests. Depth-first; `maxCount` counts
    /// emitted nodes across the whole forest.
    static func buildTree(roots: [pid_t],
                          childrenOf: (pid_t) -> [pid_t],
                          nameOf: (pid_t) -> String?,
                          maxDepth: Int, maxCount: Int) -> [ProcessNode] {
        var budget = maxCount

        func walk(_ pids: [pid_t], depth: Int) -> [ProcessNode] {
            guard depth <= maxDepth else { return [] }
            var nodes: [ProcessNode] = []
            for pid in pids where pid > 0 {
                guard budget > 0 else { break }
                guard let name = nameOf(pid) else { continue }
                budget -= 1
                let children = walk(childrenOf(pid), depth: depth + 1)
                nodes.append(ProcessNode(pid: pid, name: name, children: children))
            }
            return nodes
        }
        return walk(roots, depth: 1)
    }

    private static func childPids(of pid: pid_t) -> [pid_t] {
        var pids = [pid_t](repeating: 0, count: 64)
        let byteCount = proc_listchildpids(pid, &pids, Int32(pids.count * MemoryLayout<pid_t>.size))
        guard byteCount > 0 else { return [] }
        let count = min(Int(byteCount) / MemoryLayout<pid_t>.size, pids.count)
        return Array(pids.prefix(count)).filter { $0 > 0 }
    }

    private static func processName(of pid: pid_t) -> String? {
        var buffer = [CChar](repeating: 0, count: Int(MAXCOMLEN) * 2 + 1)
        guard proc_name(pid, &buffer, UInt32(buffer.count)) > 0 else { return nil }
        return String(cString: buffer)
    }

    private static func childProcessNames(of pid: pid_t) -> Set<String> {
        Set(childPids(of: pid).compactMap(processName(of:)))
    }
}
