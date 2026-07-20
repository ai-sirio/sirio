import Foundation
import TillerAgents

/// Orchestra il pass di auto-naming: costruisce il prompt, lancia il
/// comando di riassunto dell'adapter come processo detached, parsa
/// l'output. Ogni fallimento (adapter non supportato, binario assente,
/// timeout, output vuoto) restituisce nil — mai un errore visibile.
enum AutoNamer {
    static let maxTitleLength = 60

    static func summarize(
        transcript: String, worktreePath: String,
        adapter: any AgentAdapter, timeout: TimeInterval = 10
    ) async -> String? {
        let prompt = """
        Summarize this coding-agent conversation into a short title, \
        2-5 words, in the conversation's own language, no quotes, no punctuation \
        at the end. Reply with only the title.

        \(transcript)
        """
        guard let command = adapter.summarizerCommand(prompt: prompt) else { return nil }

        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.currentDirectoryURL = URL(fileURLWithPath: worktreePath)
        let outputPipe = Pipe()
        process.standardOutput = outputPipe
        process.standardError = Pipe() // scartato: mai propagare stderr nella UI

        do {
            try process.run()
        } catch {
            return nil // binario assente o non eseguibile
        }

        let output: Data? = await withTaskGroup(of: Data?.self) { group in
            group.addTask {
                outputPipe.fileHandleForReading.readDataToEndOfFile()
            }
            group.addTask {
                try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                if process.isRunning { process.terminate() }
                return nil
            }
            let first = await group.next() ?? nil
            group.cancelAll()
            return first
        }
        process.waitUntilExit()

        guard let output else { return nil }
        let title = String(decoding: output, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return nil }
        return String(title.prefix(maxTitleLength))
    }
}
