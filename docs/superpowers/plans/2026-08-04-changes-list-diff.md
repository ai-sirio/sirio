# Lista modifiche espandibile — piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sostituire le due viste git del pannello destro (Status e Diff) con una lista sola, dove ogni file modificato si espande e mostra sotto di sé il proprio diff.

**Architecture:** Le sezioni Staged/Changes/Untracked di `GitStatusView` diventano righe espandibili. Un `DiffLoadStore` (`@Observable`, di proprietà di `RightPanelModel`) tiene per ogni path uno stato `idle/loading/loaded/failed` e carica il patch solo all'espansione, deduplicando le richieste concorrenti. I conteggi `+N −N` arrivano da un solo `git diff --numstat -z HEAD` per refresh, non dai patch. Il rendering delle righe di codice è `UnifiedDiffPane` spostato in un file suo.

**Tech Stack:** Swift 6, SwiftUI, macOS 15+, `swift-testing` (`@Test`/`#expect`), TillerGit (shell-out a git), Observation.

**Spec:** `docs/superpowers/specs/2026-08-04-changes-list-diff-design.md`

## Global Constraints

- Test prima dell'implementazione, `swift-testing` (`@Test` / `#expect`), mai XCTest.
- Ogni file nuovo richiede `xcodegen generate` prima di compilare: `Tiller.xcodeproj` è generato da `project.yml` e non va mai editato a mano.
- Stringhe UI in inglese, sempre, anche quando la conversazione è in italiano.
- Commit message in [Conventional Commits](https://www.conventionalcommits.org/), soggetto imperativo minuscolo.
- Domain logic senza SwiftUI/AppKit va nei package, non in `App/`.
- File sotto le 400 righe; funzioni sotto le 50.
- Niente mutazione in place: si costruiscono valori nuovi.
- Verifica per pacchetto: `cd Packages/TillerGit && swift test`. Verifica dell'app: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -skipPackagePluginValidation build`. Gate completo: `Scripts/ci.sh` deve stampare `CI OK`, e va eseguito **una volta sola a fine piano** (Task 8), non a ogni task.
- `Scripts/ci.sh` include gli AppTests. I test `swift-testing` vanno dichiarati con il selector sulla **struct**, non sul `@Suite`, altrimenti la run passa con 0 test eseguiti.
- I test PTY in `TillerTerminal` sono notoriamente instabili: se `ci.sh` fallisce solo lì, rilanciare (fino a 5-6 volte) prima di cercare una causa nel proprio lavoro.

---

### Task 1: `GitDiff.numstat` — conteggi di tutti i file in una chiamata

I conteggi `+26 / −7` stanno in `GitFileDiff.additions/deletions`, che però esiste solo dopo aver caricato il patch di quel file. Le righe partono collassate, quindi serve una fonte che dia i conteggi di tutti i file senza caricare nulla.

**Files:**
- Create: `Packages/TillerGit/Sources/TillerGit/GitDiffStats.swift`
- Test: `Packages/TillerGit/Tests/TillerGitTests/GitDiffStatsTests.swift`

**Interfaces:**
- Consumes: `GitPath`, `GitStatusEntry`, `GitRunner.runCaptured`, `GitRepository.hasHead`, `GitDiff.outputLimits` (già esistenti in `TillerGit`).
- Produces:
  - `public struct GitDiffStat: Equatable, Sendable { public let additions: Int; public let deletions: Int; public let isBinary: Bool }`
  - `public static func GitDiff.parseNumstat(_ output: String) -> [GitPath: GitDiffStat]`
  - `public static func GitDiff.stats(entries: [GitStatusEntry], in repoPath: String) async throws -> [GitPath: GitDiffStat]`

Nota sul formato: si usa `--numstat -z` invece di `--numstat` perché senza `-z` git *cita* i path con caratteri speciali (`"src/f\303\250.txt"`) e serializza i rename come `src/{old => new}.txt`, due formati ambigui da riparsare. Con `-z` ogni record è `<add>\t<del>\t<path>\0`, e per i rename diventa `<add>\t<del>\t\0<from>\0<to>\0`: nessuna citazione, nessuna ambiguità.

- [ ] **Step 1: Scrivi il test del parser che fallisce**

Crea `Packages/TillerGit/Tests/TillerGitTests/GitDiffStatsTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerGit

@Test func numstatParsesPlainRecords() throws {
    let output = "26\t0\tsrc/main/index.ts\0" + "7\t24\tsrc/App.svelte\0"
    let stats = GitDiff.parseNumstat(output)

    #expect(stats[try GitPath("src/main/index.ts")]
        == GitDiffStat(additions: 26, deletions: 0, isBinary: false))
    #expect(stats[try GitPath("src/App.svelte")]
        == GitDiffStat(additions: 7, deletions: 24, isBinary: false))
}

@Test func numstatParsesPathWithSpaces() throws {
    let stats = GitDiff.parseNumstat("3\t1\tdocs/my notes.md\0")

    #expect(stats[try GitPath("docs/my notes.md")]
        == GitDiffStat(additions: 3, deletions: 1, isBinary: false))
}

@Test func numstatAttributesRenameToDestinationPath() throws {
    // Rename record: the path field is empty, then <from>\0<to>\0 follow.
    let output = "1\t1\t\0old/name.swift\0new/name.swift\0"
    let stats = GitDiff.parseNumstat(output)

    #expect(stats[try GitPath("new/name.swift")]
        == GitDiffStat(additions: 1, deletions: 1, isBinary: false))
    #expect(stats[try GitPath("old/name.swift")] == nil)
}

@Test func numstatMarksBinaryFiles() throws {
    let stats = GitDiff.parseNumstat("-\t-\tassets/logo.png\0")

    #expect(stats[try GitPath("assets/logo.png")]
        == GitDiffStat(additions: 0, deletions: 0, isBinary: true))
}

@Test func numstatIgnoresEmptyAndMalformedOutput() {
    #expect(GitDiff.parseNumstat("").isEmpty)
    #expect(GitDiff.parseNumstat("garbage\0").isEmpty)
}
```

- [ ] **Step 2: Lancia i test e verifica che falliscano**

```bash
cd Packages/TillerGit && swift test --filter numstat
```

Atteso: FAIL alla compilazione, `cannot find 'GitDiffStat' in scope`.

- [ ] **Step 3: Implementa il tipo e il parser**

Crea `Packages/TillerGit/Sources/TillerGit/GitDiffStats.swift`:

```swift
import Foundation

/// Per-file line counts, cheap enough to fetch for every changed file at once.
/// Binary files report zero counts with `isBinary` set: git emits `-` instead
/// of a number for them.
public struct GitDiffStat: Equatable, Sendable {
    public let additions: Int
    public let deletions: Int
    public let isBinary: Bool

    public init(additions: Int, deletions: Int, isBinary: Bool) {
        self.additions = additions
        self.deletions = deletions
        self.isBinary = isBinary
    }
}

extension GitDiff {
    /// Parses `git diff --numstat -z` output. NUL separation is what makes this
    /// unambiguous: without `-z`, git quotes non-ASCII paths and folds renames
    /// into a `{old => new}` brace form that cannot be split reliably.
    public static func parseNumstat(_ output: String) -> [GitPath: GitDiffStat] {
        let parts = output.split(separator: "\0", omittingEmptySubsequences: false).map(String.init)
        var stats: [GitPath: GitDiffStat] = [:]
        var index = 0

        while index < parts.count {
            let record = parts[index]
            guard !record.isEmpty else {
                index += 1
                continue
            }
            let fields = record.split(separator: "\t", omittingEmptySubsequences: false)
            guard fields.count >= 3 else {
                index += 1
                continue
            }
            let isBinary = fields[0] == "-"
            let additions = Int(fields[0]) ?? 0
            let deletions = Int(fields[1]) ?? 0
            var pathValue = String(fields[2])

            if pathValue.isEmpty {
                // Rename: the destination is two records further on.
                guard index + 2 < parts.count else { break }
                pathValue = parts[index + 2]
                index += 3
            } else {
                index += 1
            }
            guard let path = try? GitPath(pathValue) else { continue }
            stats[path] = GitDiffStat(
                additions: additions, deletions: deletions, isBinary: isBinary)
        }
        return stats
    }
}
```

- [ ] **Step 4: Lancia i test e verifica che passino**

```bash
cd Packages/TillerGit && swift test --filter numstat
```

Atteso: PASS, 5 test.

- [ ] **Step 5: Scrivi il test d'integrazione che fallisce**

Aggiungi in fondo a `GitDiffStatsTests.swift`:

```swift
@Test func statsCoversTrackedAndUntrackedFiles() async throws {
    let repo = try makeGitTestRepository()
    defer { try? FileManager.default.removeItem(at: repo) }

    try "one\ntwo\n".write(
        to: repo.appendingPathComponent("file.txt"), atomically: true, encoding: .utf8)
    try "a\nb\nc\n".write(
        to: repo.appendingPathComponent("new.txt"), atomically: true, encoding: .utf8)

    let tracked = GitStatusEntry(
        path: try GitPath("file.txt"), originalPath: nil,
        indexState: nil, worktreeState: .modified)
    let untracked = GitStatusEntry(
        path: try GitPath("new.txt"), originalPath: nil,
        indexState: nil, worktreeState: .untracked)

    let stats = try await GitDiff.stats(entries: [tracked, untracked], in: repo.path)

    #expect(stats[try GitPath("file.txt")]?.additions == 1)
    #expect(stats[try GitPath("file.txt")]?.deletions == 0)
    #expect(stats[try GitPath("new.txt")]
        == GitDiffStat(additions: 3, deletions: 0, isBinary: false))
}

@Test func statsOnRepositoryWithoutHeadCountsUntrackedOnly() async throws {
    let repo = try makeGitTestRepository(withCommit: false)
    defer { try? FileManager.default.removeItem(at: repo) }
    try "x\ny\n".write(
        to: repo.appendingPathComponent("fresh.txt"), atomically: true, encoding: .utf8)

    let entry = GitStatusEntry(
        path: try GitPath("fresh.txt"), originalPath: nil,
        indexState: nil, worktreeState: .untracked)
    let stats = try await GitDiff.stats(entries: [entry], in: repo.path)

    #expect(stats[try GitPath("fresh.txt")]?.additions == 2)
}
```

- [ ] **Step 6: Lancia i test e verifica che falliscano**

```bash
cd Packages/TillerGit && swift test --filter stats
```

Atteso: FAIL, `type 'GitDiff' has no member 'stats'`.

- [ ] **Step 7: Implementa `stats`**

Aggiungi in `GitDiffStats.swift`, dentro l'`extension GitDiff` già creata:

```swift
    /// Line counts for every changed file, in one git invocation plus a cheap
    /// read per untracked file. Untracked files do not appear in `--numstat`
    /// against HEAD at all, so their additions are counted from disk.
    public static func stats(
        entries: [GitStatusEntry], in repoPath: String
    ) async throws -> [GitPath: GitDiffStat] {
        var stats: [GitPath: GitDiffStat] = [:]

        if try await GitRepository.hasHead(in: repoPath) {
            let result = try await GitRunner.runCaptured(
                ["diff", "--numstat", "-z", "--no-color", "--no-ext-diff", "HEAD"],
                in: repoPath,
                limits: outputLimits)
            stats = parseNumstat(result.stdoutString)
        }

        let root = URL(fileURLWithPath: repoPath, isDirectory: true)
        for entry in entries where entry.isUntracked {
            let url = root.appendingPathComponent(entry.path.value)
            guard let lines = lineCount(at: url) else { continue }
            stats[entry.path] = GitDiffStat(
                additions: lines, deletions: 0, isBinary: false)
        }
        return stats
    }

    /// Nil past the snapshot size limit, or for anything that is not UTF-8:
    /// a count is a nicety, never worth loading an arbitrarily large file for.
    private static func lineCount(at url: URL) -> Int? {
        guard let data = try? Data(contentsOf: url, options: .mappedIfSafe),
              data.count <= 500_000,
              let text = String(data: data, encoding: .utf8) else { return nil }
        if text.isEmpty { return 0 }
        return text.hasSuffix("\n")
            ? text.split(separator: "\n", omittingEmptySubsequences: false).count - 1
            : text.split(separator: "\n", omittingEmptySubsequences: false).count
    }
```

- [ ] **Step 8: Lancia tutti i test del package**

```bash
cd Packages/TillerGit && swift test
```

Atteso: PASS, inclusi i 7 test nuovi e tutti i preesistenti.

- [ ] **Step 9: Commit**

```bash
git add Packages/TillerGit/Sources/TillerGit/GitDiffStats.swift \
        Packages/TillerGit/Tests/TillerGitTests/GitDiffStatsTests.swift
git commit -m "feat: add per-file diff stats via git numstat"
```

---

### Task 2: conteggi nel model

`RightPanelModel` deve esporre i conteggi alla vista. Il caricamento non deve mai bloccare la lista: se numstat fallisce, i file si vedono lo stesso, senza numeri.

**Files:**
- Modify: `App/RightPanel/RightPanelModel.swift` (`RightPanelLoaders` righe 45-64, proprietà righe 73-85, `loadInitial` righe 213-250, `performRefresh` righe 435-477, `deactivate` righe 141-174)
- Test: `AppTests/ChangesListTests.swift` (nuovo)

**Interfaces:**
- Consumes: `GitDiff.stats(entries:in:)`, `GitDiffStat` (Task 1).
- Produces:
  - `RightPanelLoaders.StatsLoader = @Sendable ([GitStatusEntry], String) async throws -> [GitPath: GitDiffStat]`
  - `RightPanelLoaders.stats` (nuovo campo, `.live` = `GitDiff.stats`)
  - `RightPanelModel.diffStats: [GitPath: GitDiffStat]` (private(set))

- [ ] **Step 1: Scrivi il test che fallisce**

Crea `AppTests/ChangesListTests.swift`:

```swift
import Foundation
import Testing
import TillerCore

@testable import Tiller
@testable import TillerGit

@Suite("ChangesList")
@MainActor
struct ChangesListTests {
    @Test func refreshPopulatesDiffStats() async throws {
        let entry = try makeEntry("a.swift")
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in GitStatusSnapshot(entries: [entry]) },
                stats: { _, _ in
                    [try GitPath("a.swift"):
                        GitDiffStat(additions: 5, deletions: 2, isBinary: false)]
                }),
            monitoringEnabled: false)

        await model.activate(worktree: makeWorktree(), isGitRepository: true)

        #expect(model.diffStats[try GitPath("a.swift")]?.additions == 5)
    }

    @Test func statsFailureLeavesListUsable() async throws {
        let entry = try makeEntry("a.swift")
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in GitStatusSnapshot(entries: [entry]) },
                stats: { _, _ in throw TestFailure.boom }),
            monitoringEnabled: false)

        await model.activate(worktree: makeWorktree(), isGitRepository: true)

        #expect(model.status.entries.count == 1)
        #expect(model.diffStats.isEmpty)
        #expect(model.gitError == nil)
    }
}

enum TestFailure: Error { case boom }

@MainActor
func makeLoaders(
    status: @escaping RightPanelLoaders.StatusLoader,
    stats: @escaping RightPanelLoaders.StatsLoader = { _, _ in [:] },
    diff: @escaping RightPanelLoaders.DiffLoader = { entry, _ in
        GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                    isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
    }
) -> RightPanelLoaders {
    RightPanelLoaders(
        directory: { _, _ in [] }, status: status, diff: diff, stats: stats)
}

func makeEntry(
    _ path: String, state: GitFileState = .modified
) throws -> GitStatusEntry {
    GitStatusEntry(
        path: try GitPath(path), originalPath: nil,
        indexState: nil, worktreeState: state)
}

func makeWorktree() -> Worktree {
    Worktree(
        id: UUID(), projectId: UUID(), name: "wt",
        path: FileManager.default.temporaryDirectory.path, branch: "main")
}
```

Se `makeEntry` o l'inizializzatore di `Worktree` collidono con helper già presenti in `AppTests` (in `RightPanelDirectoryStatusTests.swift` `makeEntry` è `private` dentro la struct, quindi non collide), rinomina i tuoi con prefisso `changesList`. Verifica la firma reale di `Worktree.init` in `Packages/TillerCore/Sources/TillerCore/` e adattala: il campo che conta per questi test è solo `path`.

- [ ] **Step 2: Lancia il test e verifica che fallisca**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build-for-testing
```

Atteso: FAIL alla compilazione, `extra argument 'stats' in call` su `RightPanelLoaders`.

- [ ] **Step 3: Aggiungi il loader e la proprietà**

In `App/RightPanel/RightPanelModel.swift`, dentro `RightPanelLoaders` (riga 45):

```swift
    typealias StatsLoader =
        @Sendable ([GitStatusEntry], String) async throws -> [GitPath: GitDiffStat]

    let directory: DirectoryLoader
    let status: StatusLoader
    let diff: DiffLoader
    let stats: StatsLoader
```

e nel `.live`, dopo il campo `diff`:

```swift
        stats: { entries, path in
            try await GitDiff.stats(entries: entries, in: path)
        })
```

Aggiungi la proprietà accanto a `statusByPath` (riga 74):

```swift
    private(set) var diffStats: [GitPath: GitDiffStat] = [:]
```

e azzerala in `deactivate()`, accanto a `statusByPath = [:]`:

```swift
        diffStats = [:]
```

- [ ] **Step 4: Carica i conteggi dopo lo status**

In `loadInitial`, subito dopo `apply(snapshot)` e `gitError = nil` (righe 243-244):

```swift
            await loadStats(token: token)
```

In `performRefresh`, subito dopo `apply(snapshot)` e `gitError = nil` (righe 461-462):

```swift
            await loadStats(token: token)
```

E aggiungi il metodo nell'`extension RightPanelModel` che contiene `loadDiff` (dopo riga 561):

```swift
    /// Counts are a nicety: a failure hides the numbers and leaves the list
    /// working, so it must never surface as `gitError`.
    private func loadStats(token: Int) async {
        guard let worktree else { return }
        let entries = status.entries
        guard !entries.isEmpty else {
            diffStats = [:]
            return
        }
        do {
            let loaded = try await loaders.stats(entries, worktree.path)
            guard token == generation else { return }
            diffStats = loaded
        } catch {
            guard token == generation else { return }
            diffStats = [:]
        }
    }
```

- [ ] **Step 5: Lancia i test e verifica che passino**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: PASS, 2 test. Se la run riporta 0 test eseguiti, il selector sta puntando al `@Suite` invece che alla struct: usa il nome della struct (`ChangesListTests`).

- [ ] **Step 6: Commit**

```bash
git add App/RightPanel/RightPanelModel.swift AppTests/ChangesListTests.swift
git commit -m "feat: load per-file diff stats in the right panel model"
```

---

### Task 3: `DiffLoadStore` — espansione e caricamento pigro

**Files:**
- Create: `App/RightPanel/DiffLoadStore.swift`
- Modify: `App/RightPanel/RightPanelModel.swift` (init righe 94-100)
- Test: `AppTests/ChangesListTests.swift`

**Interfaces:**
- Consumes: `RightPanelLoaders.DiffLoader`, `GitStatusEntry`, `GitFileDiff`, `GitPath`.
- Produces:
  - `enum DiffLoadState: Equatable { case idle, loading, loaded(GitFileDiff), failed(String) }`
  - `final class DiffLoadStore` con: `expanded: Set<GitPath>`, `state(for:) -> DiffLoadState`, `isExpanded(_:) -> Bool`, `toggle(_:repoPath:) async`, `expand(_:repoPath:) async`, `collapse(_:)`, `retry(_:repoPath:) async`, `prune(to:)`, `reloadExpanded(entries:repoPath:) async`, `reset()`
  - `RightPanelModel.diffStore: DiffLoadStore`

Perché lo store è un oggetto a parte e non altre proprietà di `RightPanelModel`: il model è `@Observable` e ogni sua proprietà osservata invalida chi la legge. La riga file legge lo stato del **proprio** path e passa il `GitFileDiff` per valore a `FileDiffBody`; siccome `GitFileDiff` è `Equatable`, SwiftUI salta il re-render dei corpi diff non toccati.

- [ ] **Step 1: Scrivi i test che falliscono**

Aggiungi dentro `struct ChangesListTests` in `AppTests/ChangesListTests.swift`:

```swift
    @Test func expandingIdlePathLoadsItsDiff() async throws {
        let entry = try makeEntry("a.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 1, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        #expect(store.state(for: entry.path) == .idle)
        await store.expand(entry, repoPath: "/tmp")

        #expect(store.isExpanded(entry.path))
        guard case .loaded(let diff) = store.state(for: entry.path) else {
            Issue.record("expected a loaded diff")
            return
        }
        #expect(diff.additions == 1)
    }

    @Test func concurrentExpansionsOfSamePathLoadOnce() async throws {
        let entry = try makeEntry("a.swift")
        let counter = LoadCounter()
        let store = DiffLoadStore(loader: { entry, _ in
            await counter.increment()
            try? await Task.sleep(for: .milliseconds(20))
            return GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        async let first: Void = store.expand(entry, repoPath: "/tmp")
        async let second: Void = store.expand(entry, repoPath: "/tmp")
        _ = await (first, second)

        #expect(await counter.value == 1)
    }

    @Test func collapsingKeepsTheLoadedDiff() async throws {
        let entry = try makeEntry("a.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 3, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        await store.expand(entry, repoPath: "/tmp")
        store.collapse(entry.path)

        #expect(!store.isExpanded(entry.path))
        guard case .loaded = store.state(for: entry.path) else {
            Issue.record("collapsing must not discard the loaded diff")
            return
        }
    }

    @Test func failedLoadIsReportedAndRetryable() async throws {
        let entry = try makeEntry("a.swift")
        let attempts = LoadCounter()
        let store = DiffLoadStore(loader: { entry, _ in
            let count = await attempts.increment()
            if count == 1 { throw TestFailure.boom }
            return GitFileDiff(path: entry.path, lines: [], additions: 9, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })

        await store.expand(entry, repoPath: "/tmp")
        guard case .failed = store.state(for: entry.path) else {
            Issue.record("expected a failed state")
            return
        }

        await store.retry(entry, repoPath: "/tmp")
        guard case .loaded(let diff) = store.state(for: entry.path) else {
            Issue.record("expected retry to succeed")
            return
        }
        #expect(diff.additions == 9)
    }

    @Test func pruneDropsPathsThatLeftTheStatus() async throws {
        let kept = try makeEntry("kept.swift")
        let gone = try makeEntry("gone.swift")
        let store = DiffLoadStore(loader: { entry, _ in
            GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })
        await store.expand(kept, repoPath: "/tmp")
        await store.expand(gone, repoPath: "/tmp")

        store.prune(to: [kept.path])

        #expect(store.isExpanded(kept.path))
        #expect(!store.isExpanded(gone.path))
        #expect(store.state(for: gone.path) == .idle)
    }

    @Test func reloadExpandedRefreshesOnlyExpandedPaths() async throws {
        let expanded = try makeEntry("open.swift")
        let collapsed = try makeEntry("closed.swift")
        let loadedPaths = LoadedPaths()
        let store = DiffLoadStore(loader: { entry, _ in
            await loadedPaths.record(entry.path.value)
            return GitFileDiff(path: entry.path, lines: [], additions: 0, deletions: 0,
                               isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
        })
        await store.expand(expanded, repoPath: "/tmp")
        await loadedPaths.clear()

        await store.reloadExpanded(entries: [expanded, collapsed], repoPath: "/tmp")

        #expect(await loadedPaths.values == ["open.swift"])
    }
```

E in fondo al file, fuori dalla struct:

```swift
actor LoadCounter {
    private(set) var value = 0

    @discardableResult
    func increment() -> Int {
        value += 1
        return value
    }
}

actor LoadedPaths {
    private(set) var values: [String] = []

    func record(_ path: String) { values.append(path) }
    func clear() { values.removeAll() }
}
```

- [ ] **Step 2: Lancia i test e verifica che falliscano**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build-for-testing
```

Atteso: FAIL, `cannot find 'DiffLoadStore' in scope`.

- [ ] **Step 3: Implementa lo store**

Crea `App/RightPanel/DiffLoadStore.swift`:

```swift
import Foundation
import Observation
import SwiftUI
import TillerGit

enum DiffLoadState: Equatable {
    case idle
    case loading
    case loaded(GitFileDiff)
    case failed(String)
}

/// Per-file expansion and diff loading for the changes list.
///
/// Kept out of `RightPanelModel` on purpose: the model is `@Observable`, so a
/// diff map living there would invalidate every row in the list each time one
/// file finished loading.
@MainActor @Observable
final class DiffLoadStore {
    typealias Loader = @Sendable (GitStatusEntry, String) async throws -> GitFileDiff

    private(set) var expanded: Set<GitPath> = []
    private(set) var states: [GitPath: DiffLoadState] = [:]

    @ObservationIgnored private var inFlight: Set<GitPath> = []
    @ObservationIgnored private let loader: Loader

    init(loader: @escaping Loader) {
        self.loader = loader
    }

    func isExpanded(_ path: GitPath) -> Bool { expanded.contains(path) }

    func state(for path: GitPath) -> DiffLoadState { states[path] ?? .idle }

    func toggle(_ entry: GitStatusEntry, repoPath: String) async {
        if expanded.contains(entry.path) {
            collapse(entry.path)
        } else {
            await expand(entry, repoPath: repoPath)
        }
    }

    func expand(_ entry: GitStatusEntry, repoPath: String) async {
        withAnimation(.easeOut(duration: 0.18)) {
            _ = expanded.insert(entry.path)
        }
        guard state(for: entry.path) == .idle else { return }
        await load(entry, repoPath: repoPath)
    }

    /// Collapsing keeps whatever was loaded: re-expanding is then instant, and
    /// a stale diff is impossible because every refresh reloads what is open.
    func collapse(_ path: GitPath) {
        withAnimation(.easeOut(duration: 0.18)) {
            expanded.remove(path)
        }
    }

    func retry(_ entry: GitStatusEntry, repoPath: String) async {
        states[entry.path] = .idle
        await load(entry, repoPath: repoPath)
    }

    func prune(to paths: Set<GitPath>) {
        expanded = expanded.intersection(paths)
        states = states.filter { paths.contains($0.key) }
    }

    func reloadExpanded(entries: [GitStatusEntry], repoPath: String) async {
        for entry in entries where expanded.contains(entry.path) {
            states[entry.path] = .idle
            await load(entry, repoPath: repoPath)
        }
    }

    func reset() {
        expanded = []
        states = [:]
        inFlight = []
    }

    private func load(_ entry: GitStatusEntry, repoPath: String) async {
        guard inFlight.insert(entry.path).inserted else { return }
        defer { inFlight.remove(entry.path) }
        states[entry.path] = .loading
        do {
            let diff = try await loader(entry, repoPath)
            guard states[entry.path] == .loading else { return }
            states[entry.path] = .loaded(diff)
        } catch {
            guard states[entry.path] == .loading else { return }
            states[entry.path] = .failed(error.localizedDescription)
        }
    }
}
```

- [ ] **Step 4: Dai lo store al model**

In `App/RightPanel/RightPanelModel.swift`, sostituisci l'`init` (righe 94-100) con:

```swift
    /// Not observed: the reference never changes, and views observe the store
    /// itself. Observing it here would invalidate the whole panel per load.
    @ObservationIgnored let diffStore: DiffLoadStore

    init(
        loaders: RightPanelLoaders = .live,
        monitoringEnabled: Bool = true
    ) {
        self.loaders = loaders
        self.monitoringEnabled = monitoringEnabled
        let diffLoader = loaders.diff
        self.diffStore = DiffLoadStore(loader: diffLoader)
    }
```

e in `deactivate()`, accanto a `diffStats = [:]`:

```swift
        diffStore.reset()
```

- [ ] **Step 5: Lancia i test e verifica che passino**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: PASS, 8 test.

- [ ] **Step 6: Commit**

```bash
git add App/RightPanel/DiffLoadStore.swift App/RightPanel/RightPanelModel.swift \
        AppTests/ChangesListTests.swift Tiller.xcodeproj
git commit -m "feat: add per-path diff load store with lazy expansion"
```

---

### Task 4: estrarre icona file e stile di stato

`iconView(for:)` è un metodo privato dentro `FileExplorerView` e `statusSymbol(_:)` è duplicato identico in `FileExplorerView.swift:205` e `GitStatusView.swift:182`. La lista nuova ha bisogno di entrambi: o li copio una terza volta, o li estraggo.

**Files:**
- Create: `App/RightPanel/FileTypeIcon.swift`
- Create: `App/RightPanel/GitPanelTypes.swift`
- Modify: `App/RightPanel/FileExplorerView.swift` (righe 183-203 e `statusSymbol`/`statusColor`)
- Modify: `App/RightPanel/GitStatusView.swift` (rimuovi `PendingGitDiscard` righe 4-22, `symbol` righe 182-189, `color` righe 191-196)

**Interfaces:**
- Consumes: `FileIconKey`, `FileIconTheme` (`TillerCore`), `GitStatusEntry`, `AppTheme`.
- Produces:
  - `struct FileTypeIcon: View { init(key: FileIconKey, theme: FileIconTheme, tint: Color) }`
  - `enum GitStatusStyle { static func symbol(_ entry: GitStatusEntry) -> String; static func color(_ entry: GitStatusEntry) -> Color }`
  - `struct PendingGitDiscard` (spostato invariato da `GitStatusView.swift`)

- [ ] **Step 1: Crea `FileTypeIcon`**

Crea `App/RightPanel/FileTypeIcon.swift`:

```swift
import SwiftUI
import TillerCore

/// File-type glyph shared by the file explorer and the changes list. The theme
/// resolves to either an SF Symbol name or an asset-catalog image.
struct FileTypeIcon: View {
    let key: FileIconKey
    let theme: FileIconTheme
    var tint: Color = AppTheme.subtitle

    var body: some View {
        switch theme.iconRef(for: key) {
        case .system(let name):
            Image(systemName: name)
                .foregroundStyle(tint)
        case .asset(let name):
            Image(name)
                .resizable()
                .scaledToFit()
                .frame(width: 14, height: 14)
        }
    }
}
```

- [ ] **Step 2: Crea `GitPanelTypes`**

Crea `App/RightPanel/GitPanelTypes.swift`, spostandoci `PendingGitDiscard` **invariato** da `GitStatusView.swift` (righe 4-22) e aggiungendo lo stile:

```swift
import SwiftUI
import TillerGit

struct PendingGitDiscard: Identifiable {
    enum Kind: Equatable { case changes, untracked }

    let id = UUID()
    let kind: Kind
    let entries: [GitStatusEntry]

    var title: String {
        entries.count == 1
            ? "Discard \(entries[0].path.value)?"
            : "Discard \(entries.count) files?"
    }

    var message: String {
        kind == .untracked
            ? "Untracked files will be permanently deleted."
            : "Unstaged changes will be restored from the Git index."
    }
}

/// Status letter and colour for a changed file. Extracted because the file
/// explorer and the changes list both need it and had drifted into two copies.
enum GitStatusStyle {
    static func symbol(_ entry: GitStatusEntry) -> String {
        if entry.isConflicted { return "U" }
        if entry.isUntracked { return "?" }
        if entry.indexState == .added { return "A" }
        if entry.indexState == .deleted || entry.worktreeState == .deleted { return "D" }
        if entry.indexState == .renamed || entry.worktreeState == .renamed { return "R" }
        return "M"
    }

    static func color(_ entry: GitStatusEntry) -> Color {
        if entry.isConflicted { return AppTheme.gitConflict }
        if entry.isUntracked { return AppTheme.gitUntracked }
        if entry.isStaged { return AppTheme.gitStaged }
        return AppTheme.gitModified
    }
}
```

- [ ] **Step 3: Fai usare i nuovi tipi a `FileExplorerView`**

In `App/RightPanel/FileExplorerView.swift`: sostituisci il corpo di `iconView(for:)` (righe 183-195) con

```swift
    @ViewBuilder
    private func iconView(for node: FileTreeNode) -> some View {
        FileTypeIcon(
            key: iconKey(for: node),
            theme: iconTheme,
            tint: node.kind.isDirectory ? AppTheme.meta : AppTheme.subtitle)
    }
```

Poi cancella i metodi `statusSymbol(_:)` e il corrispondente metodo colore, e sostituisci le loro chiamate con `GitStatusStyle.symbol(entry)` / `GitStatusStyle.color(entry)`. Trovale con:

```bash
TOKENSAVE_DISABLE_GREP_HOOK=1 grep -n "statusSymbol\|statusColor" App/RightPanel/FileExplorerView.swift
```

- [ ] **Step 4: Togli i duplicati da `GitStatusView`**

In `App/RightPanel/GitStatusView.swift` cancella `PendingGitDiscard` (righe 4-22), `symbol(_:)` (righe 182-189) e `color(_:)` (righe 191-196), e sostituisci le due chiamate nel corpo di `statusRow` con `GitStatusStyle.symbol(entry)` e `GitStatusStyle.color(entry)`. La vista verrà cancellata del tutto al Task 7, ma deve compilare adesso.

- [ ] **Step 5: Compila**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build
```

Atteso: `** BUILD SUCCEEDED **`. Due fallimenti SwiftLint sui package di terze parti `CodeEditTextView`/`CodeEditSourceEditor` compaiono in coda: sono preesistenti e non bloccano il link.

- [ ] **Step 6: Commit**

```bash
git add App/RightPanel/FileTypeIcon.swift App/RightPanel/GitPanelTypes.swift \
        App/RightPanel/FileExplorerView.swift App/RightPanel/GitStatusView.swift \
        Tiller.xcodeproj
git commit -m "refactor: extract shared file icon and git status style"
```

---

### Task 5: `FileDiffBody` — il corpo espanso

Il rendering delle righe esiste già come `UnifiedDiffPane`, privato dentro `GitDiffView.swift`. Va estratto così com'è, con due differenze: niente `ScrollView` propria (scorre la lista che lo contiene) e un `VStack` al posto del `LazyVStack`, perché annidare due contenitori pigri non aggiunge nulla.

**Files:**
- Create: `App/RightPanel/FileDiffBody.swift`
- Test: `AppTests/ChangesListTests.swift`

**Interfaces:**
- Consumes: `GitFileDiff`, `GitDiffLine`, `DiffHighlightCache`, `AttributedCodeRenderer`, `SyntaxHighlightRange`, `AppTheme`.
- Produces: `struct FileDiffBody: View { init(diff: GitFileDiff, fileURL: URL?) }`

- [ ] **Step 1: Scrivi il test che fallisce**

Aggiungi dentro `struct ChangesListTests`:

```swift
    @Test func diffBodySkipsMetadataLines() throws {
        let patch = """
        diff --git a/file.txt b/file.txt
        --- a/file.txt
        +++ b/file.txt
        @@ -1,2 +1,3 @@
         one
        -two
        +second
        """
        let diff = try GitDiff.parse(patch, path: try GitPath("file.txt"))

        #expect(FileDiffBody.renderableLines(of: diff).allSatisfy { $0.kind != .metadata })
        #expect(FileDiffBody.renderableLines(of: diff).contains { $0.kind == .hunk })
    }
```

- [ ] **Step 2: Lancia il test e verifica che fallisca**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build-for-testing
```

Atteso: FAIL, `cannot find 'FileDiffBody' in scope`.

- [ ] **Step 3: Implementa**

Crea `App/RightPanel/FileDiffBody.swift`:

```swift
import AppKit
import SwiftUI
import TillerCode
import TillerGit

/// Rendered body of one expanded file: hunk headers plus numbered, syntax
/// highlighted lines. Scrolls with its parent — the changes list owns the
/// ScrollView so that expanding a file does not create a nested scroll area.
struct FileDiffBody: View {
    let diff: GitFileDiff
    let fileURL: URL?

    @Environment(\.colorScheme) private var colorScheme
    @State private var highlights: DiffHighlights?

    /// `diff --git` and `+++`/`---` headers carry nothing the row does not
    /// already show, so they never reach the screen.
    static func renderableLines(of diff: GitFileDiff) -> [GitDiffLine] {
        diff.lines.filter { $0.kind != .metadata }
    }

    private struct HighlightRequest: Hashable {
        let path: String?
        let oldHash: Int?
        let newHash: Int?
    }

    private var requestID: HighlightRequest {
        HighlightRequest(
            path: fileURL?.standardizedFileURL.path,
            oldHash: diff.oldText?.hashValue,
            newHash: diff.newText?.hashValue)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Self.renderableLines(of: diff)) { line in
                if line.kind == .hunk {
                    Text(line.text)
                        .font(.system(size: 11, design: .monospaced))
                        .foregroundStyle(AppTheme.subtitle)
                        .padding(.vertical, 1)
                        .padding(.leading, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(AppTheme.diffHunkBackground)
                } else {
                    lineRow(line)
                }
            }
        }
        .padding(.vertical, 4)
        .task(id: requestID) {
            guard let fileURL else { highlights = nil; return }
            highlights = await DiffHighlightCache.shared.highlights(
                path: fileURL,
                oldText: diff.oldText,
                newText: diff.newText)
        }
    }

    private func lineRow(_ line: GitDiffLine) -> some View {
        HStack(alignment: .top, spacing: 0) {
            Text(line.oldLineNumber.map(String.init) ?? "")
                .frame(width: 32, alignment: .trailing)
                .foregroundStyle(AppTheme.meta)
            Text(line.newLineNumber.map(String.init) ?? "")
                .frame(width: 32, alignment: .trailing)
                .foregroundStyle(AppTheme.meta)
            codeText(for: line)
                .textSelection(.enabled)
                .padding(.leading, 6)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.system(size: 11, design: .monospaced))
        .padding(.vertical, 1)
        .background(background(for: line))
    }

    private func ranges(for line: GitDiffLine) -> [SyntaxHighlightRange]? {
        switch line.kind {
        case .deletion:
            guard let number = line.oldLineNumber,
                  let map = highlights?.old else { return nil }
            return map.ranges(forLine: number)
        case .addition, .context:
            guard let number = line.newLineNumber,
                  let map = highlights?.new else { return nil }
            return map.ranges(forLine: number)
        case .hunk, .metadata:
            return nil
        }
    }

    @ViewBuilder
    private func codeText(for line: GitDiffLine) -> some View {
        if let ranges = ranges(for: line) {
            Text(AttributedCodeRenderer.renderLine(
                line.text,
                ranges: ranges,
                theme: .tiller(isDark: colorScheme == .dark),
                font: .monospacedSystemFont(ofSize: 11, weight: .regular)))
        } else {
            Text(line.text)
                .font(.system(size: 11, design: .monospaced))
                .foregroundStyle(foreground(for: line))
        }
    }

    private func foreground(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAddition
        case .deletion: AppTheme.diffDeletion
        default: AppTheme.subtitle
        }
    }

    private func background(for line: GitDiffLine) -> Color {
        switch line.kind {
        case .addition: AppTheme.diffAdditionBackground
        case .deletion: AppTheme.diffDeletionBackground
        default: .clear
        }
    }
}
```

- [ ] **Step 4: Lancia i test e verifica che passino**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: PASS, 9 test.

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel/FileDiffBody.swift AppTests/ChangesListTests.swift Tiller.xcodeproj
git commit -m "feat: extract expanded file diff body view"
```

---

### Task 6: `ChangedFileRow` — la riga file

A riposo mostra i conteggi; in hover al loro posto compaiono le azioni. Il menu contestuale offre le stesse azioni, per chi non passa dall'hover.

**Files:**
- Create: `App/RightPanel/ChangedFileRow.swift`
- Test: `AppTests/ChangesListTests.swift`

**Interfaces:**
- Consumes: `GitStatusEntry`, `GitDiffStat`, `FileTypeIcon`, `GitStatusStyle`, `FileIconKey`, `FileIconTheme`, `AppTheme`.
- Produces:
  - `struct ChangedFileRow: View` con init:
    `init(entry: GitStatusEntry, stat: GitDiffStat?, isExpanded: Bool, iconTheme: FileIconTheme, isStagedSection: Bool, onToggle: @escaping () -> Void, onStage: @escaping () -> Void, onUnstage: @escaping () -> Void, onDiscard: @escaping () -> Void, onOpenFile: @escaping () -> Void)`
  - `enum ChangedFileCounts { static func label(for stat: GitDiffStat?) -> String? }`

- [ ] **Step 1: Scrivi i test che falliscono**

Aggiungi dentro `struct ChangesListTests`:

```swift
    @Test func countLabelShowsSignedAdditionsAndDeletions() {
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 24, deletions: 7, isBinary: false)) == "−7 +24")
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 26, deletions: 0, isBinary: false)) == "+26")
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 3, isBinary: false)) == "−3")
    }

    @Test func countLabelReportsBinaryAndMissingStats() {
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 0, isBinary: true)) == "bin")
        #expect(ChangedFileCounts.label(for: nil) == nil)
        #expect(ChangedFileCounts.label(
            for: GitDiffStat(additions: 0, deletions: 0, isBinary: false)) == nil)
    }
```

Il carattere del meno è `−` (U+2212, minus sign), non il trattino ASCII: allineato in colonna con le cifre nei font di sistema.

- [ ] **Step 2: Lancia i test e verifica che falliscano**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation build-for-testing
```

Atteso: FAIL, `cannot find 'ChangedFileCounts' in scope`.

- [ ] **Step 3: Implementa**

Crea `App/RightPanel/ChangedFileRow.swift`:

```swift
import SwiftUI
import TillerCore
import TillerGit

enum ChangedFileCounts {
    /// Nil when there is nothing worth showing, so the row can fall back to
    /// blank space instead of a misleading "+0".
    static func label(for stat: GitDiffStat?) -> String? {
        guard let stat else { return nil }
        if stat.isBinary { return "bin" }
        var parts: [String] = []
        if stat.deletions > 0 { parts.append("−\(stat.deletions)") }
        if stat.additions > 0 { parts.append("+\(stat.additions)") }
        return parts.isEmpty ? nil : parts.joined(separator: " ")
    }
}

struct ChangedFileRow: View {
    let entry: GitStatusEntry
    let stat: GitDiffStat?
    let isExpanded: Bool
    let iconTheme: FileIconTheme
    let isStagedSection: Bool
    let onToggle: () -> Void
    let onStage: () -> Void
    let onUnstage: () -> Void
    let onDiscard: () -> Void
    let onOpenFile: () -> Void

    @State private var isHovering = false

    private var fileName: String {
        entry.path.value.split(separator: "/").last.map(String.init) ?? entry.path.value
    }

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "chevron.right")
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(AppTheme.meta)
                .rotationEffect(.degrees(isExpanded ? 90 : 0))
                .frame(width: 12)

            FileTypeIcon(
                key: FileIconKey.key(forFileName: fileName),
                theme: iconTheme,
                tint: GitStatusStyle.color(entry))
                .frame(width: 14)

            Text(entry.path.value)
                .font(.system(size: 12))
                .foregroundStyle(GitStatusStyle.color(entry))
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer(minLength: 6)

            if entry.isConflicted {
                Text("Resolve in terminal")
                    .font(.caption2)
                    .foregroundStyle(AppTheme.gitConflict)
            } else if isHovering {
                actions
            } else if let counts = ChangedFileCounts.label(for: stat) {
                Text(counts)
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(AppTheme.meta)
            }
        }
        .padding(.horizontal, 7)
        .padding(.vertical, 5)
        .contentShape(Rectangle())
        .background(
            isHovering ? AppTheme.rowHover : Color.clear,
            in: RoundedRectangle(cornerRadius: 6))
        .onHover { isHovering = $0 }
        .onTapGesture(perform: onToggle)
        .contextMenu {
            if !entry.isConflicted {
                if isStagedSection {
                    Button("Unstage", action: onUnstage)
                } else {
                    Button("Stage", action: onStage)
                    Button("Discard", role: .destructive, action: onDiscard)
                }
            }
            Button("Open in editor", action: onOpenFile)
        }
        .help(entry.isConflicted ? "Conflicted" : entry.path.value)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(
            "\(GitStatusStyle.symbol(entry)) \(entry.path.value)")
        .accessibilityHint(isExpanded ? "Collapse diff" : "Expand diff")
    }

    private var actions: some View {
        HStack(spacing: 6) {
            if isStagedSection {
                Button("Unstage", action: onUnstage)
                    .buttonStyle(.plain)
            } else {
                Button("Discard", role: .destructive, action: onDiscard)
                    .buttonStyle(.plain)
                Button("Stage", action: onStage)
                    .buttonStyle(.plain)
            }
            Button(action: onOpenFile) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
            }
            .buttonStyle(.plain)
            .help("Open in editor")
        }
        .font(.caption)
    }
}
```

- [ ] **Step 4: Lancia i test e verifica che passino**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: PASS, 11 test.

- [ ] **Step 5: Commit**

```bash
git add App/RightPanel/ChangedFileRow.swift AppTests/ChangesListTests.swift Tiller.xcodeproj
git commit -m "feat: add expandable changed file row"
```

---

### Task 7: `ChangesListView` e sostituzione delle due viste

**Files:**
- Create: `App/RightPanel/ChangesListView.swift`
- Delete: `App/RightPanel/GitDiffView.swift`, `App/RightPanel/GitStatusView.swift`
- Modify: `App/RightPanel/RightPanelMode.swift` (tutto), `App/RightPanel/RightPanelView.swift` (righe 40-42 e 93-128), `App/ContentView.swift` (righe 97-99)
- Test: `AppTests/ChangesListTests.swift`

**Interfaces:**
- Consumes: `ChangedFileRow`, `FileDiffBody`, `DiffLoadStore`, `DiffLoadState`, `RightPanelModel.diffStats`, `PendingGitDiscard`.
- Produces: `struct ChangesListView: View { init(panelModel: RightPanelModel, worktree: Worktree, onOpenFile: @escaping (GitStatusEntry) -> Void, requestDiscard: @escaping (PendingGitDiscard) -> Void) }`

- [ ] **Step 1: Scrivi il test di migrazione che fallisce**

Aggiungi dentro `struct ChangesListTests`:

```swift
    @Test func savedDiffModeMigratesToStatus() {
        #expect(RightPanelMode.effective(rawValue: "diff", isGitRepository: true) == .status)
        #expect(RightPanelMode.effective(rawValue: "diff", isGitRepository: false) == .files)
        #expect(RightPanelMode.allCases.count == 2)
    }
```

- [ ] **Step 2: Lancia il test e verifica che fallisca**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: FAIL, `allCases.count` è 3 e `"diff"` risolve a `.diff`.

- [ ] **Step 3: Togli la modalità Diff**

Riscrivi `App/RightPanel/RightPanelMode.swift`:

```swift
import Foundation

/// UI route for the worktree tools panel. Persistence stores rawValue only.
enum RightPanelMode: String, CaseIterable, Identifiable {
    case files
    case status

    var id: String { rawValue }

    var title: String {
        switch self {
        case .files: "Files"
        case .status: "Changes"
        }
    }

    var systemImage: String {
        switch self {
        case .files: "folder"
        case .status: "arrow.triangle.branch"
        }
    }

    var requiresGit: Bool { self != .files }

    static func effective(rawValue: String, isGitRepository: Bool) -> RightPanelMode {
        // "diff" was a separate mode until the changes list absorbed it; anyone
        // upgrading has it persisted and must land on the list, not on Files.
        let saved = rawValue == "diff" ? .status : RightPanelMode(rawValue: rawValue) ?? .files
        return saved.requiresGit && !isGitRepository ? .files : saved
    }
}
```

- [ ] **Step 4: Lancia il test di migrazione**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: il test `savedDiffModeMigratesToStatus` PASSA; la build dell'app fallisce altrove perché `GitDiffView`/`RightPanelView` citano ancora `.diff`. Si sistema al passo dopo.

- [ ] **Step 5: Implementa `ChangesListView`**

Crea `App/RightPanel/ChangesListView.swift`:

```swift
import SwiftUI
import TillerCore
import TillerGit

/// The worktree's changed files, each row expanding into its own diff.
/// Replaces the former Status and Diff modes, which forced a dropdown round
/// trip to look at a second file.
struct ChangesListView: View {
    private enum SectionKind: Equatable { case staged, changes, untracked }

    @Bindable var panelModel: RightPanelModel
    let worktree: Worktree
    let onOpenFile: (GitStatusEntry) -> Void
    let requestDiscard: (PendingGitDiscard) -> Void

    @AppStorage(AppSettings.fileIconThemeKey) private var fileIconThemeRaw =
        FileIconTheme.sfSymbols.rawValue

    private var iconTheme: FileIconTheme {
        FileIconTheme(rawValue: fileIconThemeRaw) ?? .sfSymbols
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            content
        }
        .disabled(panelModel.mutationInProgress)
    }

    private var header: some View {
        HStack {
            Text(panelModel.status.isClean ? "Working tree clean" : "Local changes")
                .font(.system(size: 12, weight: .semibold))
            Spacer()
            Button { Task { await panelModel.refresh() } } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.plain)
            .help("Refresh Status")
        }
        .padding(8)
    }

    @ViewBuilder
    private var content: some View {
        if panelModel.gitLoading && panelModel.status.isClean {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        } else if let error = panelModel.gitError {
            ContentUnavailableView {
                Label("Git status unavailable", systemImage: "exclamationmark.triangle")
            } description: {
                Text(error)
            } actions: {
                Button("Retry") { Task { await panelModel.refresh() } }
            }
        } else if panelModel.status.isClean {
            ContentUnavailableView("Working tree clean", systemImage: "checkmark.circle")
        } else {
            ScrollView {
                LazyVStack(spacing: 10) {
                    section("Staged", kind: .staged, entries: panelModel.status.staged,
                            actionTitle: "Unstage all") { entries in
                        Task { await panelModel.unstage(entries) }
                    }
                    section("Changes", kind: .changes, entries: panelModel.status.changes,
                            actionTitle: "Stage all") { entries in
                        Task { await panelModel.stage(entries) }
                    }
                    section("Untracked", kind: .untracked, entries: panelModel.status.untracked,
                            actionTitle: "Stage all") { entries in
                        Task { await panelModel.stage(entries) }
                    }
                }
                .padding(8)
            }
        }
    }

    @ViewBuilder
    private func section(
        _ title: String,
        kind: SectionKind,
        entries: [GitStatusEntry],
        actionTitle: String,
        action: @escaping ([GitStatusEntry]) -> Void
    ) -> some View {
        if !entries.isEmpty {
            let actionable = entries.filter { !$0.isConflicted }
            VStack(spacing: 2) {
                HStack {
                    Text("\(title) (\(entries.count))")
                        .font(.system(size: 11, weight: .semibold))
                    Spacer()
                    Button(actionTitle) { action(actionable) }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    if kind != .staged {
                        Button("Discard all", role: .destructive) {
                            requestDiscard(PendingGitDiscard(
                                kind: kind == .untracked ? .untracked : .changes,
                                entries: actionable))
                        }
                        .buttonStyle(.plain)
                        .font(.caption)
                        .disabled(actionable.isEmpty)
                    }
                }
                ForEach(entries, id: \.path) { entry in
                    fileBlock(entry, kind: kind)
                }
            }
        }
    }

    @ViewBuilder
    private func fileBlock(_ entry: GitStatusEntry, kind: SectionKind) -> some View {
        let store = panelModel.diffStore
        VStack(spacing: 0) {
            ChangedFileRow(
                entry: entry,
                stat: panelModel.diffStats[entry.path],
                isExpanded: store.isExpanded(entry.path),
                iconTheme: iconTheme,
                isStagedSection: kind == .staged,
                onToggle: {
                    Task { await store.toggle(entry, repoPath: worktree.path) }
                },
                onStage: { Task { await panelModel.stage([entry]) } },
                onUnstage: { Task { await panelModel.unstage([entry]) } },
                onDiscard: {
                    requestDiscard(PendingGitDiscard(
                        kind: entry.isUntracked ? .untracked : .changes, entries: [entry]))
                },
                onOpenFile: { onOpenFile(entry) })

            if store.isExpanded(entry.path) {
                expandedBody(entry, state: store.state(for: entry.path))
            }
        }
    }

    @ViewBuilder
    private func expandedBody(_ entry: GitStatusEntry, state: DiffLoadState) -> some View {
        switch state {
        case .idle, .loading:
            ProgressView()
                .controlSize(.small)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 26)
                .padding(.vertical, 6)
        case .loaded(let diff):
            if diff.isBinary {
                Text("Binary diff unavailable")
                    .font(.caption)
                    .foregroundStyle(AppTheme.meta)
                    .padding(.leading, 26)
                    .padding(.vertical, 6)
            } else {
                FileDiffBody(diff: diff, fileURL: fileURL(for: entry))
                    .padding(.leading, 12)
            }
        case .failed(let message):
            VStack(alignment: .leading, spacing: 4) {
                Label(message, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(AppTheme.gitConflict)
                HStack(spacing: 8) {
                    Button("Retry") {
                        Task {
                            await panelModel.diffStore.retry(entry, repoPath: worktree.path)
                        }
                    }
                    Button("Open file") { onOpenFile(entry) }
                }
                .font(.caption)
                .buttonStyle(.plain)
            }
            .padding(.leading, 26)
            .padding(.vertical, 6)
        }
    }

    private func fileURL(for entry: GitStatusEntry) -> URL? {
        panelModel.rootURL?.appendingPathComponent(entry.path.value)
    }
}
```

Verifica il nome reale della chiave del tema icone con:

```bash
TOKENSAVE_DISABLE_GREP_HOOK=1 grep -n "fileIconTheme" App/RightPanel/FileExplorerView.swift
```

e usa la stessa forma esatta che usa `FileExplorerView` (righe 12-16).

- [ ] **Step 6: Instrada e cancella le viste vecchie**

In `App/RightPanel/RightPanelView.swift` togli il `.task(id: effectiveMode)` (righe 40-42) — non c'è più niente da precaricare — e sostituisci lo `switch effectiveMode` (righe 95-120) con:

```swift
                    switch effectiveMode {
                    case .files:
                        FileExplorerView(
                            appModel: appModel,
                            panelModel: panelModel,
                            worktree: worktree)
                    case .status:
                        ChangesListView(
                            panelModel: panelModel,
                            worktree: worktree,
                            onOpenFile: { entry in
                                guard let root = panelModel.rootURL else { return }
                                appModel.openDocument(
                                    fileURL: root.appendingPathComponent(entry.path.value),
                                    in: worktree)
                            },
                            requestDiscard: { pendingDiscard = $0 })
                    }
```

Poi cancella i due file:

```bash
git rm App/RightPanel/GitDiffView.swift App/RightPanel/GitStatusView.swift
```

- [ ] **Step 7: Aggiorna il link "mostra diff" di ContentView**

In `App/ContentView.swift`, righe 97-99, sostituisci

```swift
            rightPanelModeRaw = RightPanelMode.diff.rawValue
            Task { await rightPanelModel.selectDiff(entry) }
```

con

```swift
            rightPanelModeRaw = RightPanelMode.status.rawValue
            Task {
                await rightPanelModel.diffStore.expand(
                    entry, repoPath: worktree.path)
            }
```

- [ ] **Step 8: Compila e lancia i test**

```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: `** BUILD SUCCEEDED **` e PASS, 12 test.

- [ ] **Step 9: Commit**

```bash
git add -A App/RightPanel App/ContentView.swift AppTests/ChangesListTests.swift Tiller.xcodeproj
git commit -m "feat: replace status and diff panes with expandable changes list"
```

---

### Task 8: pulizia del model e gate finale

Il model porta ancora l'API della vecchia vista a file singolo: `diff`, `diffLoading`, `diffError`, `selectedDiffPath`, `selectDiff`, `ensureDiffLoaded`, `loadDiff`. Nessuno la chiama più. Va via, e al suo posto il refresh deve potare l'espansione e ricaricare i file aperti.

**Files:**
- Modify: `App/RightPanel/RightPanelModel.swift` (proprietà righe 76-84, `deactivate` righe 164-172, `performRefresh` righe 444-472, `apply` righe 530-543, `selectedEntry`/`selectDiff`/`ensureDiffLoaded`/`loadDiff` righe 496-561)
- Test: `AppTests/ChangesListTests.swift`

**Interfaces:**
- Consumes: `DiffLoadStore.prune(to:)`, `DiffLoadStore.reloadExpanded(entries:repoPath:)`.
- Produces: niente di nuovo; rimuove `RightPanelModel.diff`, `.diffLoading`, `.diffError`, `.selectedDiffPath`, `.selectedEntry`, `.selectDiff(_:)`, `.ensureDiffLoaded()`.

- [ ] **Step 1: Scrivi i test che falliscono**

Aggiungi dentro `struct ChangesListTests`:

```swift
    @Test func refreshDropsExpansionForFilesThatLeftTheStatus() async throws {
        let gone = try makeEntry("gone.swift")
        let snapshots = SnapshotSequence(values: [
            GitStatusSnapshot(entries: [gone]),
            GitStatusSnapshot(entries: [])
        ])
        let model = RightPanelModel(
            loaders: makeLoaders(status: { _ in await snapshots.next() }),
            monitoringEnabled: false)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.diffStore.expand(gone, repoPath: "/tmp")
        #expect(model.diffStore.isExpanded(gone.path))

        await model.refresh()

        #expect(!model.diffStore.isExpanded(gone.path))
    }

    @Test func refreshReloadsOnlyExpandedDiffs() async throws {
        let open = try makeEntry("open.swift")
        let closed = try makeEntry("closed.swift")
        let loadedPaths = LoadedPaths()
        let snapshot = GitStatusSnapshot(entries: [open, closed])
        let model = RightPanelModel(
            loaders: makeLoaders(
                status: { _ in snapshot },
                diff: { entry, _ in
                    await loadedPaths.record(entry.path.value)
                    return GitFileDiff(
                        path: entry.path, lines: [], additions: 0, deletions: 0,
                        isBinary: false, isSubmodule: false, oldText: nil, newText: nil)
                }),
            monitoringEnabled: false)
        await model.activate(worktree: makeWorktree(), isGitRepository: true)
        await model.diffStore.expand(open, repoPath: "/tmp")
        await loadedPaths.clear()

        await model.refresh()

        #expect(await loadedPaths.values == ["open.swift"])
    }
```

E in fondo al file, fuori dalla struct:

```swift
actor SnapshotSequence {
    private var values: [GitStatusSnapshot]
    private var index = 0

    init(values: [GitStatusSnapshot]) { self.values = values }

    func next() -> GitStatusSnapshot {
        defer { index = min(index + 1, values.count - 1) }
        return values[min(index, values.count - 1)]
    }
}
```

- [ ] **Step 2: Lancia i test e verifica che falliscano**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: FAIL — l'espansione resta viva dopo il refresh e nessun diff viene ricaricato.

- [ ] **Step 3: Pota l'espansione in `apply`**

In `App/RightPanel/RightPanelModel.swift`, sostituisci la coda di `apply(_:)` (righe 536-542, il blocco `if let selectedDiffPath … diffError = nil`) con:

```swift
        diffStore.prune(to: Set(snapshot.entries.map(\.path)))
```

- [ ] **Step 4: Ricarica i diff espansi nel refresh**

In `performRefresh`, sostituisci il blocco che ricaricava il diff selezionato (righe 464-472, da `let selectedChanged` fino alla chiusura dell'`if`) con:

```swift
            await diffStore.reloadExpanded(
                entries: status.entries, repoPath: worktree.path)
            guard token == generation else { return }
```

Cancella anche `let selectedPathBefore = selectedDiffPath` e `let selectedEntryBefore = selectedEntry` (righe 445-446) e il metodo privato `changedPathsAffecting(_:changedPaths:rootURL:)` (righe 479-490), che restano senza chiamanti.

- [ ] **Step 5: Rimuovi l'API a file singolo**

Cancella da `RightPanelModel`:

- le proprietà `selectedDiffPath`, `diff`, `diffLoading`, `diffError` (righe 76-84) e le loro tre righe in `deactivate()`
- `var selectedEntry`, `func selectDiff(_:)`, `func ensureDiffLoaded()`, `private func loadDiff(_:)` (righe 496-512 e 545-561)

`RightPanelLoaders.diff` **resta**: è il loader che `DiffLoadStore` riceve nell'init.

Verifica che non sia rimasto nessun chiamante:

```bash
TOKENSAVE_DISABLE_GREP_HOOK=1 grep -rn "selectDiff\|ensureDiffLoaded\|selectedDiffPath\|panelModel.diff\b" App/ AppTests/
```

Atteso: nessun risultato. Se `AppModelControlTests` o altri test citano questi simboli, aggiornali per usare `diffStore.expand(_:repoPath:)`.

- [ ] **Step 6: Lancia i test e verifica che passino**

```bash
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug \
  -skipPackagePluginValidation test -only-testing:AppTests/ChangesListTests
```

Atteso: PASS, 14 test.

- [ ] **Step 7: Gate completo**

```bash
Scripts/ci.sh
```

Atteso: `CI OK`. Se falliscono solo i test PTY di `TillerTerminal`, rilancia: sono instabili e servono fino a 5-6 tentativi. Qualsiasi altro fallimento è da attribuire a questo lavoro.

- [ ] **Step 8: Commit**

```bash
git add App/RightPanel/RightPanelModel.swift AppTests/ChangesListTests.swift
git commit -m "refactor: drop single-file diff state from right panel model"
```

---

## Checklist manuale

Da fare a mano dopo il Task 8, con l'app costruita in Release e aperta su un worktree con modifiche vere:

- [ ] Il pannello destro mostra due modalità, `Files` e `Changes`; `Diff` non esiste più.
- [ ] Chi aveva il pannello su Diff prima dell'aggiornamento riapre l'app su `Changes`, non su `Files`.
- [ ] All'apertura tutte le righe sono chiuse e nessun diff è caricato.
- [ ] Cliccare una riga la espande e mostra il codice; ricliccare la chiude.
- [ ] Espandere un file già aperto in precedenza è immediato, senza spinner.
- [ ] I conteggi `+N −N` compaiono a destra su ogni riga e spariscono in hover, sostituiti da Discard/Stage/apri.
- [ ] Un file untracked mostra `+N` pari alle sue righe.
- [ ] Un file binario mostra `bin` sulla riga e `Binary diff unavailable` una volta espanso.
- [ ] Mettere in stage un file espanso lo sposta in `Staged` restando espanso.
- [ ] Modificare un file espanso da terminale ne aggiorna il diff entro un secondo.
- [ ] Cancellare un file espanso lo toglie dalla lista senza lasciare residui.
- [ ] Il menu tasto destro offre le stesse azioni dell'hover.
- [ ] Cliccare un file link da una chat apre il pannello su `Changes` con quel file già espanso.
- [ ] Un file in conflitto è espandibile, mostra i marker e ha le azioni disabilitate.

## Note per chi implementa

- `App/RightPanel/RightPanelView.swift` righe 79-80 ha due stringhe UI in italiano (`"Nascondi pannello destro (⌃⌘I)"`). È preesistente e fuori dallo scope: segnalato, non toccato.
- `Packages/TillerGit/Sources/TillerGit/GitDiffSideBySide.swift` non è referenziato da nessun file in `App/`; vive solo per i propri test. Codice morto preesistente, non va cancellato in questo lavoro.
- `GitCommandResult` non ha un flag di troncamento: un diff oltre `GitDiff.outputLimits` (5MB / 20k righe) viene tagliato in silenzio. Comportamento preesistente, fuori scope.
