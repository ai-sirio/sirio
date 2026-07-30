import Foundation
import TillerAgents

/// Orchestra il pass di auto-naming: costruisce il prompt, lancia il
/// comando di riassunto dell'adapter come processo detached, parsa
/// l'output. Ogni fallimento (adapter non supportato, binario assente,
/// timeout, output vuoto) restituisce nil — mai un errore visibile.
enum AutoNamer {
    static let maxTitleLength = 60

    /// Timeout largo: `claude -p` a freddo impiega ~20s anche su prompt
    /// minimi (misurato 2026-07-24); 10s troncava ogni pass di claude/codex.
    static func summarize(
        transcript: String, worktreePath: String,
        adapter: any AgentAdapter, timeout: TimeInterval = 60
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

        // Lettura via readabilityHandler, guidata dalla coda interna di
        // Foundation, mai da una chiamata sincrona su questa funzione async.
        // La versione precedente usava `readDataToEndOfFile()` e
        // `waitUntilExit()`, entrambe bloccanti per l'intera durata del
        // processo figlio — e il thread chiamante appartiene al pool a
        // dimensione FISSA condiviso da ogni Task concorrente del processo.
        // Con abbastanza `summarize()` in volo insieme (come nella suite di
        // test in parallelo), il pool si esaurisce: i Task successivi
        // restano in coda per un thread che non si libera mai, il che si
        // manifesta come un hang, non come una chiamata lenta. Non serve
        // chiamare `waitUntilExit()`: Foundation riscatta il processo figlio
        // internamente a prescindere da questa chiamata.
        let collector = OutputCollector()
        outputPipe.fileHandleForReading.readabilityHandler = { handle in
            let chunk = handle.availableData
            if chunk.isEmpty {
                handle.readabilityHandler = nil
                collector.finish()
            } else {
                collector.append(chunk)
            }
        }

        let timedOut = await withTaskGroup(of: Bool.self) { group in
            group.addTask { await collector.waitUntilFinished(); return false }
            group.addTask {
                try? await Task.sleep(nanoseconds: UInt64(timeout * 1_000_000_000))
                return true
            }
            let first = await group.next() ?? true
            group.cancelAll()
            return first
        }
        if timedOut {
            outputPipe.fileHandleForReading.readabilityHandler = nil
            if process.isRunning { process.terminate() }
        }

        guard !timedOut else { return nil }
        let output = collector.result
        let title = String(decoding: output, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty else { return nil }
        return String(title.prefix(maxTitleLength))
    }
}

/// Accumula i chunk consegnati da `readabilityHandler` (che li invoca in
/// serie sulla propria coda) e sveglia l'unico attendente async alla EOF.
/// Lock semplice invece di un actor: la mutazione è sincrona e banale, e
/// `readabilityHandler` non è una closure `async` — farla `await`are
/// un attore avrebbe solo introdotto un giro di scheduling in più senza
/// bisogno.
private final class OutputCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var data = Data()
    private var isFinished = false
    private var continuation: CheckedContinuation<Void, Never>?

    func append(_ chunk: Data) {
        lock.lock()
        data.append(chunk)
        lock.unlock()
    }

    func finish() {
        lock.lock()
        isFinished = true
        let pending = continuation
        continuation = nil
        lock.unlock()
        pending?.resume()
    }

    /// No direct `lock()`/`unlock()` calls here: Swift 6 marks `NSLock`'s
    /// locking methods `noasync`, flagged textually inside any `async`
    /// function even when the call happens before the first suspension. The
    /// actual locking lives in the two synchronous helpers below.
    func waitUntilFinished() async {
        if alreadyFinished() { return }
        await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
            registerIfStillPending(c)
        }
    }

    private func alreadyFinished() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return isFinished
    }

    /// Re-checks `isFinished` under the same lock acquisition rather than
    /// trusting `alreadyFinished()`'s earlier read: `finish()` can run
    /// between that check and this registration, and a continuation stored
    /// after finish() already looked for one would never resume.
    private func registerIfStillPending(_ continuation: CheckedContinuation<Void, Never>) {
        lock.lock()
        if isFinished {
            lock.unlock()
            continuation.resume()
            return
        }
        self.continuation = continuation
        lock.unlock()
    }

    var result: Data {
        lock.lock()
        defer { lock.unlock() }
        return data
    }
}
