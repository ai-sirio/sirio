# Markdown Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Editor/lettore markdown integrato in Tiller come tipo di tab accanto ai terminali: preview MarkdownUI (default) + modalità codice, salvataggio manuale ⌘S, watch del file, apertura via cmd+click nel terminale / drag & drop / ⌘O.

**Architecture:** `TerminalTab` diventa `WorkspaceTab` con contenuto polimorfico (`.terminal(SplitTree)` | `.markdown(fileURL:)`). Logica (documento, watcher, link resolution, trasformazioni sintassi) in TillerCore (testabile); view in `App/MarkdownEditor/`; persistenza via migration GRDB additiva v7.

**Tech Stack:** Swift 6 (strict concurrency), SwiftUI macOS 15, GRDB, MarkdownUI (gonzalezreal/swift-markdown-ui), Swift Testing (`@Test`/`#expect`), xcodegen.

**Spec:** `docs/superpowers/specs/2026-07-10-markdown-editor-design.md`

## Global Constraints

- macOS deployment target **15.0**, Swift **6.0** strict concurrency (da `project.yml`).
- Test con **Swift Testing** (`import Testing`, `@Test`, `#expect`), NON XCTest — stile di `Packages/TillerCore/Tests/TillerCoreTests/SplitTreeTests.swift`.
- Commenti nel codice: stesso stile del file che modifichi (il codebase mescola italiano e inglese; i commenti spiegano il *perché*).
- Il progetto Xcode si genera con `xcodegen generate` da `project.yml` (rigenerare dopo ogni modifica a `project.yml`).
- Build app: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build` (dalla root del repo). Se `xcodebuild` fallisce per firma, appendere `CODE_SIGNING_ALLOWED=NO`.
- Test package: `swift test --package-path Packages/TillerCore` (e analogo per gli altri package).
- Commit format: `<type>: <description>` (feat, fix, refactor, docs, test, chore).
- La tabella DB resta `terminalTab` e il record resta `TerminalTabRecord`: rinominare solo il tipo dominio (`TerminalTab` → `WorkspaceTab`), non lo schema.
- Estensioni markdown riconosciute: `.md`, `.markdown` (case-insensitive), unica fonte: `MarkdownFileLink.extensions`.

---

### Task 1: WorkspaceTab con contenuto polimorfico (TillerCore + call sites)

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/TerminalTab.swift` (rinomina file in `WorkspaceTab.swift` con `git mv`)
- Modify: `Packages/TillerCore/Sources/TillerCore/ScrollbackFlush.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` (solo rinomina tipo, la persistenza markdown arriva nel Task 2)
- Modify: `App/AppModel.swift`, `App/TabBarView.swift`, `App/ContentView.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift` (nuovo)
- Modify: eventuali test esistenti che citano `TerminalTab` (trovarli con grep, step 6)

**Interfaces:**
- Produces (i task successivi si affidano a queste firme esatte):
  ```swift
  public enum TabContent: Equatable, Sendable {
      case terminal(SplitTree)
      case markdown(fileURL: URL)
  }
  public struct WorkspaceTab: Identifiable, Equatable, Sendable {
      public let id: UUID
      public var title: String
      public var content: TabContent
      public init(id: UUID, title: String, content: TabContent)
      public init(id: UUID, title: String, tree: SplitTree)   // convenience: .terminal
      public var leafIds: [UUID]           // tree.leafIds per .terminal, [] per .markdown
      public var terminalTree: SplitTree?  // nil per .markdown
      public var markdownFileURL: URL?     // nil per .terminal
      public static func nextShellTitle(existing: [WorkspaceTab]) -> String
  }
  ```

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/WorkspaceTabTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

@Test func terminalTabExposesLeafIdsAndTree() {
    let a = UUID(), b = UUID()
    let tree = SplitTree.leaf(id: a).splitting(leaf: a, axis: .horizontal, newLeaf: b)
    let tab = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: tree)
    #expect(tab.leafIds == [a, b])
    #expect(tab.terminalTree == tree)
    #expect(tab.markdownFileURL == nil)
}

@Test func markdownTabHasNoLeavesAndExposesFileURL() {
    let url = URL(fileURLWithPath: "/tmp/README.md")
    let tab = WorkspaceTab(id: UUID(), title: "README.md", content: .markdown(fileURL: url))
    #expect(tab.leafIds == [])
    #expect(tab.terminalTree == nil)
    #expect(tab.markdownFileURL == url)
}

@Test func nextShellTitleIgnoresMarkdownTabs() {
    let existing = [
        WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID())),
        WorkspaceTab(id: UUID(), title: "README.md",
                     content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/README.md")))
    ]
    #expect(WorkspaceTab.nextShellTitle(existing: existing) == "Terminale 2")
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore --filter WorkspaceTabTests`
Expected: FAIL / errore di compilazione "cannot find 'WorkspaceTab' in scope"

- [ ] **Step 3: Implementa WorkspaceTab**

`git mv Packages/TillerCore/Sources/TillerCore/TerminalTab.swift Packages/TillerCore/Sources/TillerCore/WorkspaceTab.swift`, poi sostituisci l'intero contenuto:

```swift
import Foundation

/// Contenuto di una tab di workspace: un albero di split terminale oppure
/// un editor markdown puntato a un file su disco.
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
}

/// Una tab dentro un worktree. L'identità della tab è stabile; title e
/// content sono mutabili (rinomina, split).
public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String
    public var content: TabContent

    public init(id: UUID, title: String, content: TabContent) {
        self.id = id; self.title = title; self.content = content
    }

    /// Convenience per il caso terminale, il più comune nei call site.
    public init(id: UUID, title: String, tree: SplitTree) {
        self.init(id: id, title: title, content: .terminal(tree))
    }

    /// Leaf del tree terminale; una tab markdown non ha pane.
    public var leafIds: [UUID] {
        if case .terminal(let tree) = content { return tree.leafIds }
        return []
    }

    public var terminalTree: SplitTree? {
        if case .terminal(let tree) = content { return tree }
        return nil
    }

    public var markdownFileURL: URL? {
        if case .markdown(let url) = content { return url }
        return nil
    }

    /// Titolo default per una nuova shell manuale: "Terminale N", dove N
    /// conta le tab shell esistenti (prefisso "Terminale") + 1. Le tab
    /// agente (titolo = displayName) e markdown non incrementano il contatore.
    public static func nextShellTitle(existing: [WorkspaceTab]) -> String {
        let count = existing.filter { $0.title.hasPrefix("Terminale") }.count
        return "Terminale \(count + 1)"
    }
}
```

- [ ] **Step 4: Aggiorna tutti i call sites**

Trova tutti i punti: `grep -rn "TerminalTab\b" App Packages --include="*.swift" | grep -v TerminalTabRecord`

Modifiche meccaniche (`TerminalTab` → `WorkspaceTab`) più questi punti NON meccanici:

`Packages/TillerCore/Sources/TillerCore/ScrollbackFlush.swift` — firma e accesso leaf:
```swift
public func scrollbackFlushTargets(
    tabs: [UUID: [WorkspaceTab]]
) -> [(worktreeId: UUID, paneId: UUID)] {
    var out: [(worktreeId: UUID, paneId: UUID)] = []
    for (worktreeId, tabList) in tabs {
        for tab in tabList {
            for paneId in tab.leafIds {
                out.append((worktreeId, paneId))
            }
        }
    }
    return out
}
```

`Packages/TillerCore/Sources/TillerCore/ProjectStore.swift` — in questo task solo i tipi (`saveTabs(worktreeId:tabs: [WorkspaceTab], activeTabId:)`, `loadTabs` ritorna `(tabs: [WorkspaceTab], activeTabId: UUID?)`); dentro `saveTabs` l'encode del tree diventa:
```swift
// Task 1: le tab markdown non esistono ancora a runtime; guardia temporanea
// sostituita dalla persistenza completa nel Task 2.
guard case .terminal(let tree) = tab.content else { continue }
let treeJSON = String(decoding: try encoder.encode(tree), as: UTF8.self)
```
e in `loadTabs` la costruzione diventa `WorkspaceTab(id: id, title: record.title, tree: tree)`.

`App/AppModel.swift` — ogni `$0.tree.leafIds` / `tab.tree.leafIds` → `.leafIds`. I punti con mutazione del tree:

```swift
// tabs var
var tabs: [UUID: [WorkspaceTab]] = [:]

// ensureTabs / openTab / closeTab: TerminalTab(id:title:tree:) → WorkspaceTab(id:title:tree:)

// worstStatusTab
func worstStatusTab(in worktree: Worktree) -> WorkspaceTab? {
    AttentionSort.sorted(tabs[worktree.id] ?? []) { tab in
        agentActivity.statusForWorktree(paneIds: tab.leafIds)
    }.first
}

// activeTab
func activeTab(for worktreeId: UUID) -> WorkspaceTab? { ... }  // corpo invariato

// openTab: ritorno WorkspaceTab

// tabContaining
func tabContaining(paneId: UUID) -> (worktree: Worktree, tab: WorkspaceTab, index: Int)? {
    for worktree in worktrees.values.flatMap({ $0 }) {
        if let idx = tabs[worktree.id]?.firstIndex(where: { $0.leafIds.contains(paneId) }) {
            return (worktree, tabs[worktree.id]![idx], idx)
        }
    }
    return nil
}

// split(paneId:axis:) — il tree ora è dentro l'enum
func split(paneId: UUID, axis: SplitAxis) {
    guard let tuple = tabContaining(paneId: paneId),
          let tree = tuple.tab.terminalTree else { return }
    tabs[tuple.worktree.id]?[tuple.index].content =
        .terminal(tree.splitting(leaf: paneId, axis: axis, newLeaf: UUID()))
    persistTabs(for: tuple.worktree.id)
}

// closePane — idem
func closePane(paneId: UUID) {
    guard let tuple = tabContaining(paneId: paneId),
          let tree = tuple.tab.terminalTree,
          let newTree = tree.removing(leaf: paneId) else { return }
    deleteAgentSessionRefs(paneIds: [paneId])
    tabs[tuple.worktree.id]?[tuple.index].content = .terminal(newTree)
    persistTabs(for: tuple.worktree.id)
}

// splitCurrent: tab.tree.leafIds.first → tab.leafIds.first
// isSelectedWorktreeContaining / worktreeContaining: $0.tree.leafIds → $0.leafIds
// bootstrap restoreAgentSessions: loaded.tabs.flatMap { $0.tree.leafIds } → { $0.leafIds }
// statusForWorktree / agentIdForWorktree / runningAgentIds / paneIds in
// deregister (righe 73, 109, 117): { $0.tree.leafIds } → { $0.leafIds }
```

`App/TabBarView.swift` — riga 64: `tab.tree.leafIds` → `tab.leafIds`; firma `private func tabItem(_ tab: WorkspaceTab)`.

`App/ContentView.swift` — il body del ForEach tab: `TerminalSplitHost(tree: tab.tree, ...)` diventa (rendering markdown arriva nel Task 6, qui solo compilazione):
```swift
if let tree = tab.terminalTree {
    TerminalSplitHost(
        tree: tree,
        ...  // resto invariato
    )
    .opacity(isVisible ? 1 : 0)
    .allowsHitTesting(isVisible)
    .accessibilityHidden(!isVisible)
}
```

- [ ] **Step 5: Aggiorna i test esistenti che citano TerminalTab**

`grep -rln "TerminalTab\b" Packages/*/Tests --include="*.swift" | grep -v TerminalTabRecord` — in ogni match sostituisci `TerminalTab(` → `WorkspaceTab(` e i tipi nelle firme. Il costruttore convenience `(id:title:tree:)` mantiene compilabili le chiamate esistenti.

- [ ] **Step 6: Verifica package e app**

Run: `swift test --package-path Packages/TillerCore`
Expected: PASS (inclusi i 3 nuovi test)

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build CODE_SIGNING_ALLOWED=NO`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "refactor: TerminalTab diventa WorkspaceTab con contenuto polimorfico"
```

---

### Task 2: Persistenza tab markdown (migration v7 + save/load)

**Files:**
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/AppDatabase.swift`
- Modify: `Packages/TillerPersistence/Sources/TillerPersistence/Records.swift`
- Modify: `Packages/TillerCore/Sources/TillerCore/ProjectStore.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift` (nuovo)

**Interfaces:**
- Consumes: `WorkspaceTab`, `TabContent` (Task 1).
- Produces: `TerminalTabRecord` con `kind: String` (`"terminal"`/`"markdown"`) e `filePath: String?`; `ProjectStore.saveTabs`/`loadTabs` round-trip di entrambi i kind.

- [ ] **Step 1: Scrivi il test che fallisce**

Crea `Packages/TillerCore/Tests/TillerCoreTests/MarkdownTabPersistenceTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore
import TillerPersistence

@Test func saveAndLoadRoundTripsTerminalAndMarkdownTabs() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")

    let terminal = WorkspaceTab(id: UUID(), title: "Terminale 1", tree: .leaf(id: UUID()))
    let markdown = WorkspaceTab(
        id: UUID(), title: "README.md",
        content: .markdown(fileURL: URL(fileURLWithPath: "/tmp/p/README.md"))
    )
    try await store.saveTabs(worktreeId: worktree.id, tabs: [terminal, markdown],
                             activeTabId: markdown.id)

    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs == [terminal, markdown])
    #expect(loaded.activeTabId == markdown.id)
}

@Test func markdownRecordWithoutFilePathIsSkipped() async throws {
    let db = try AppDatabase.inMemory()
    let store = ProjectStore(database: db)
    let project = try await store.addProject(name: "p", rootPath: "/tmp/p")
    let worktree = try await store.addWorktree(projectId: project.id, branch: "main", path: "/tmp/p")
    // Riga corrotta scritta a mano: kind markdown ma filePath NULL.
    try db.write { dbConn in
        try dbConn.execute(
            sql: """
            INSERT INTO terminalTab (id, worktreeId, title, orderIdx, isActive, treeJSON, updatedAt, kind, filePath)
            VALUES (?, ?, 'x.md', 0, 1, '', ?, 'markdown', NULL)
            """,
            arguments: [UUID().uuidString, worktree.id.uuidString, Date()]
        )
    }
    let loaded = try await store.loadTabs(of: worktree.id)
    #expect(loaded.tabs.isEmpty)
}
```

- [ ] **Step 2: Esegui il test e verifica che fallisca**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownTabPersistenceTests`
Expected: FAIL — errore di compilazione (colonne/parametri `kind`/`filePath` inesistenti) o round-trip che perde la tab markdown (guardia `continue` del Task 1)

- [ ] **Step 3: Implementa migration, record e store**

`AppDatabase.swift` — nuova migration DOPO `v6`, prima del `return migrator`:

```swift
migrator.registerMigration("v7") { db in
    try db.alter(table: "terminalTab") { t in
        t.add(column: "kind", .text).notNull().defaults(to: "terminal")
        t.add(column: "filePath", .text)
    }
}
```

`Records.swift` — `TerminalTabRecord` con i due campi nuovi in coda (default nel memberwise init così i call site esistenti non cambiano):

```swift
public struct TerminalTabRecord: Codable, FetchableRecord, PersistableRecord, Sendable, Equatable {
    public static let databaseTableName = "terminalTab"
    public var id: String
    public var worktreeId: String
    public var title: String
    public var orderIdx: Int
    public var isActive: Bool
    public var treeJSON: String
    public var updatedAt: Date
    public var kind: String
    public var filePath: String?

    public init(id: String, worktreeId: String, title: String, orderIdx: Int,
                isActive: Bool, treeJSON: String, updatedAt: Date,
                kind: String = "terminal", filePath: String? = nil) {
        self.id = id; self.worktreeId = worktreeId; self.title = title
        self.orderIdx = orderIdx; self.isActive = isActive
        self.treeJSON = treeJSON; self.updatedAt = updatedAt
        self.kind = kind; self.filePath = filePath
    }
}
```

`ProjectStore.swift` — `saveTabs` sostituisce la guardia temporanea del Task 1:

```swift
public func saveTabs(worktreeId: UUID, tabs: [WorkspaceTab], activeTabId: UUID?) throws {
    let encoder = JSONEncoder()
    try database.write { db in
        try db.execute(
            sql: "DELETE FROM terminalTab WHERE worktreeId = ?",
            arguments: [worktreeId.uuidString]
        )
        for (idx, tab) in tabs.enumerated() {
            let record: TerminalTabRecord
            switch tab.content {
            case .terminal(let tree):
                let treeJSON = String(decoding: try encoder.encode(tree), as: UTF8.self)
                record = TerminalTabRecord(
                    id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                    title: tab.title, orderIdx: idx,
                    isActive: tab.id == activeTabId,
                    treeJSON: treeJSON, updatedAt: Date()
                )
            case .markdown(let fileURL):
                // treeJSON "" e non NULL: la colonna è notNull dalla v3.
                record = TerminalTabRecord(
                    id: tab.id.uuidString, worktreeId: worktreeId.uuidString,
                    title: tab.title, orderIdx: idx,
                    isActive: tab.id == activeTabId,
                    treeJSON: "", updatedAt: Date(),
                    kind: "markdown", filePath: fileURL.path
                )
            }
            try record.insert(db)
        }
    }
}
```

`loadTabs` — il corpo del for diventa:

```swift
for record in records {
    guard let id = UUID(uuidString: record.id) else {
        logger.warning("loadTabs: skipping corrupt TerminalTabRecord '\(record.id)'")
        continue
    }
    switch record.kind {
    case "markdown":
        guard let path = record.filePath else {
            logger.warning("loadTabs: markdown tab '\(record.id)' senza filePath — skip")
            continue
        }
        tabs.append(WorkspaceTab(id: id, title: record.title,
                                 content: .markdown(fileURL: URL(fileURLWithPath: path))))
    default:
        guard let tree = try? decoder.decode(SplitTree.self, from: Data(record.treeJSON.utf8)) else {
            logger.warning("loadTabs: skipping corrupt TerminalTabRecord '\(record.id)'")
            continue
        }
        tabs.append(WorkspaceTab(id: id, title: record.title, tree: tree))
    }
    if record.isActive { active = id }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore && swift test --package-path Packages/TillerPersistence`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: persistenza tab markdown (migration v7, kind+filePath)"
```

---

### Task 3: MarkdownFileLink — risoluzione URL/path → file markdown

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/MarkdownFileLink.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownFileLinkTests.swift`

**Interfaces:**
- Produces:
  ```swift
  public enum MarkdownFileLink {
      public static let extensions: Set<String>  // ["md", "markdown"]
      /// nil se il raw non è un file markdown; URL file standardizzato altrimenti.
      public static func resolve(_ raw: String, worktreePath: String) -> URL?
      public static func isMarkdown(_ url: URL) -> Bool
  }
  ```

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/MarkdownFileLinkTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

@Test func resolvesFileURLToMarkdown() {
    let url = MarkdownFileLink.resolve("file:///tmp/wt/README.md", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/README.md"))
}

@Test func resolvesAbsolutePath() {
    let url = MarkdownFileLink.resolve("/tmp/wt/docs/piano.markdown", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/docs/piano.markdown"))
}

@Test func resolvesRelativePathAgainstWorktree() {
    let url = MarkdownFileLink.resolve("docs/piano.md", worktreePath: "/tmp/wt")
    #expect(url == URL(fileURLWithPath: "/tmp/wt/docs/piano.md"))
}

@Test func extensionMatchIsCaseInsensitive() {
    #expect(MarkdownFileLink.resolve("/tmp/wt/A.MD", worktreePath: "/tmp/wt") != nil)
}

@Test func rejectsNonMarkdownAndWebURLs() {
    #expect(MarkdownFileLink.resolve("https://example.com/x.md", worktreePath: "/tmp/wt") == nil)
    #expect(MarkdownFileLink.resolve("/tmp/wt/main.swift", worktreePath: "/tmp/wt") == nil)
    #expect(MarkdownFileLink.resolve("file:///tmp/wt/script.sh", worktreePath: "/tmp/wt") == nil)
}

@Test func isMarkdownChecksExtension() {
    #expect(MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.md")))
    #expect(MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.markdown")))
    #expect(!MarkdownFileLink.isMarkdown(URL(fileURLWithPath: "/a/b.txt")))
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownFileLinkTests`
Expected: FAIL "cannot find 'MarkdownFileLink' in scope"

- [ ] **Step 3: Implementa**

Crea `Packages/TillerCore/Sources/TillerCore/MarkdownFileLink.swift`:

```swift
import Foundation

/// Traduce la stringa grezza di un link attivato nel terminale (o un path
/// da drag & drop) in un file URL markdown. Unica fonte di verità per le
/// estensioni riconosciute. Pura: non tocca il filesystem — l'esistenza
/// del file la verifica chi apre il documento.
public enum MarkdownFileLink {
    public static let extensions: Set<String> = ["md", "markdown"]

    public static func isMarkdown(_ url: URL) -> Bool {
        extensions.contains(url.pathExtension.lowercased())
    }

    public static func resolve(_ raw: String, worktreePath: String) -> URL? {
        let fileURL: URL
        if let parsed = URL(string: raw), parsed.scheme == "file" {
            fileURL = URL(fileURLWithPath: parsed.path)
        } else if raw.hasPrefix("/") {
            fileURL = URL(fileURLWithPath: raw)
        } else if !raw.contains("://") {
            // Path relativo (es. stampato da un agente): risolto sul worktree.
            fileURL = URL(fileURLWithPath: worktreePath).appendingPathComponent(raw)
        } else {
            return nil   // http(s) e altri schemi non sono file locali
        }
        guard isMarkdown(fileURL) else { return nil }
        return fileURL.standardizedFileURL
    }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownFileLinkTests`
Expected: PASS (6 test)

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/MarkdownFileLink.swift Packages/TillerCore/Tests/TillerCoreTests/MarkdownFileLinkTests.swift
git commit -m "feat: MarkdownFileLink risolve link terminale/path in file markdown"
```

---

### Task 4: MarkdownSyntax — trasformazioni pure per la toolbar

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/MarkdownSyntax.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownSyntaxTests.swift`

**Interfaces:**
- Produces:
  ```swift
  public enum MarkdownSyntax {
      /// Avvolge la selezione con prefix/suffix (bold, italic, link).
      /// Selezione vuota → inserisce i marker e mette la selezione in mezzo.
      public static func wrap(_ text: String, selection: Range<String.Index>,
                              prefix: String, suffix: String)
          -> (text: String, selection: Range<String.Index>)
      /// Prepone linePrefix a ogni riga toccata dalla selezione (heading, liste).
      public static func prefixLines(_ text: String, selection: Range<String.Index>,
                                     linePrefix: String)
          -> (text: String, selection: Range<String.Index>)
  }
  ```

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/MarkdownSyntaxTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

private func range(of substring: String, in text: String) -> Range<String.Index> {
    text.range(of: substring)!
}

@Test func wrapBoldsSelectedText() {
    let text = "ciao mondo"
    let (out, sel) = MarkdownSyntax.wrap(text, selection: range(of: "mondo", in: text),
                                         prefix: "**", suffix: "**")
    #expect(out == "ciao **mondo**")
    #expect(String(out[sel]) == "mondo")
}

@Test func wrapWithEmptySelectionInsertsMarkersAndCursorInside() {
    let text = "ciao "
    let (out, sel) = MarkdownSyntax.wrap(text, selection: text.endIndex..<text.endIndex,
                                         prefix: "**", suffix: "**")
    #expect(out == "ciao ****")
    #expect(sel.isEmpty)
    #expect(out.distance(from: out.startIndex, to: sel.lowerBound) == 7) // tra i due **
}

@Test func prefixLinesAddsHeadingToCurrentLine() {
    let text = "titolo\ncorpo"
    let (out, _) = MarkdownSyntax.prefixLines(text, selection: range(of: "tit", in: text),
                                              linePrefix: "# ")
    #expect(out == "# titolo\ncorpo")
}

@Test func prefixLinesCoversEveryLineTouchedBySelection() {
    let text = "uno\ndue\ntre"
    let sel = range(of: "no\ndu", in: text)   // attraversa riga 1 e 2
    let (out, _) = MarkdownSyntax.prefixLines(text, selection: sel, linePrefix: "- ")
    #expect(out == "- uno\n- due\ntre")
}

@Test func wrapAsLinkPlacesSelectionOnText() {
    let text = "vedi qui"
    let (out, sel) = MarkdownSyntax.wrap(text, selection: range(of: "qui", in: text),
                                         prefix: "[", suffix: "](url)")
    #expect(out == "vedi [qui](url)")
    #expect(String(out[sel]) == "qui")
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownSyntaxTests`
Expected: FAIL "cannot find 'MarkdownSyntax' in scope"

- [ ] **Step 3: Implementa**

Crea `Packages/TillerCore/Sources/TillerCore/MarkdownSyntax.swift`:

```swift
import Foundation

/// Trasformazioni testuali pure usate dalla toolbar dell'editor markdown.
/// Gli indici della selezione risultante sono ricalcolati via offset:
/// dopo replaceSubrange gli indici della stringa originale non sono
/// garantiti validi sulla nuova.
public enum MarkdownSyntax {
    public static func wrap(_ text: String, selection: Range<String.Index>,
                            prefix: String, suffix: String)
        -> (text: String, selection: Range<String.Index>) {
        let startOffset = text.distance(from: text.startIndex, to: selection.lowerBound)
        let selected = String(text[selection])
        var out = text
        out.replaceSubrange(selection, with: prefix + selected + suffix)
        let newStart = out.index(out.startIndex, offsetBy: startOffset + prefix.count)
        let newEnd = out.index(newStart, offsetBy: selected.count)
        return (out, newStart..<newEnd)
    }

    public static func prefixLines(_ text: String, selection: Range<String.Index>,
                                   linePrefix: String)
        -> (text: String, selection: Range<String.Index>) {
        let lineRange = text.lineRange(for: selection)
        let block = String(text[lineRange])
        let endsWithNewline = block.hasSuffix("\n")
        let core = endsWithNewline ? String(block.dropLast()) : block
        let prefixed = core
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { linePrefix + $0 }
            .joined(separator: "\n") + (endsWithNewline ? "\n" : "")
        let startOffset = text.distance(from: text.startIndex, to: lineRange.lowerBound)
        var out = text
        out.replaceSubrange(lineRange, with: prefixed)
        let newStart = out.index(out.startIndex, offsetBy: startOffset)
        let newEnd = out.index(newStart, offsetBy: prefixed.count)
        return (out, newStart..<newEnd)
    }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownSyntaxTests`
Expected: PASS (5 test)

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/MarkdownSyntax.swift Packages/TillerCore/Tests/TillerCoreTests/MarkdownSyntaxTests.swift
git commit -m "feat: MarkdownSyntax, trasformazioni pure per la toolbar editor"
```

---

### Task 5: MarkdownDocument + FileWatcher (buffer, save, watch, conflitti)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/MarkdownDocument.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/MarkdownDocumentTests.swift`

**Interfaces:**
- Consumes: niente dai task precedenti (Foundation + Observation).
- Produces:
  ```swift
  @Observable @MainActor
  public final class MarkdownDocument {
      public let fileURL: URL
      public var text: String
      public private(set) var savedText: String
      public private(set) var externalChangeConflict: Bool
      public private(set) var fileDeleted: Bool
      public var isDirty: Bool { text != savedText }
      public init(fileURL: URL) throws            // legge il file, avvia il watcher
      public func save() throws                   // write atomico
      public func reloadFromDisk()                // risolve il conflitto ricaricando
      public func keepLocalBuffer()               // risolve il conflitto tenendo il buffer
      public func stopWatching()
  }
  ```

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/MarkdownDocumentTests.swift`:

```swift
import Testing
import Foundation
@testable import TillerCore

private func makeTempFile(_ content: String) throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("md-doc-test-\(UUID().uuidString).md")
    try content.write(to: url, atomically: true, encoding: .utf8)
    return url
}

@MainActor
@Test func loadsContentAndStartsClean() throws {
    let url = try makeTempFile("# Titolo\n")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    #expect(doc.text == "# Titolo\n")
    #expect(!doc.isDirty)
}

@MainActor
@Test func editingMakesDirtyAndSaveCleans() throws {
    let url = try makeTempFile("a")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "ab"
    #expect(doc.isDirty)
    try doc.save()
    #expect(!doc.isDirty)
    #expect(try String(contentsOf: url, encoding: .utf8) == "ab")
}

@MainActor
@Test func externalChangeOnCleanBufferReloads() throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    doc.handleExternalChange()   // internal: la logica, senza dipendere dal timing del watcher
    #expect(doc.text == "v2")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func externalChangeOnDirtyBufferFlagsConflict() throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "mio"
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    doc.handleExternalChange()
    #expect(doc.text == "mio")            // il buffer non viene toccato
    #expect(doc.externalChangeConflict)
    doc.reloadFromDisk()
    #expect(doc.text == "v2")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func echoOfOwnSaveIsIgnored() throws {
    let url = try makeTempFile("a")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    doc.text = "b"
    try doc.save()
    doc.handleExternalChange()   // il watcher scatta sul nostro write
    #expect(doc.text == "b")
    #expect(!doc.externalChangeConflict)
}

@MainActor
@Test func fileDeletedFlagsAndSaveRecreates() throws {
    let url = try makeTempFile("a")
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try FileManager.default.removeItem(at: url)
    doc.handleFileGone()
    #expect(doc.fileDeleted)
    try doc.save()
    #expect(!doc.fileDeleted)
    #expect(FileManager.default.fileExists(atPath: url.path))
    try? FileManager.default.removeItem(at: url)
}

@MainActor
@Test func watcherDeliversRealExternalWrite() async throws {
    let url = try makeTempFile("v1")
    defer { try? FileManager.default.removeItem(at: url) }
    let doc = try MarkdownDocument(fileURL: url)
    defer { doc.stopWatching() }
    try "v2".write(to: url, atomically: true, encoding: .utf8)
    // Il write atomico sostituisce l'inode: il watcher deve riagganciarsi.
    for _ in 0..<40 where doc.text != "v2" {
        try await Task.sleep(for: .milliseconds(50))
    }
    #expect(doc.text == "v2")
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownDocumentTests`
Expected: FAIL "cannot find 'MarkdownDocument' in scope"

- [ ] **Step 3: Implementa**

Crea `Packages/TillerCore/Sources/TillerCore/MarkdownDocument.swift`:

```swift
import Foundation
import Observation

/// Buffer di editing di un file markdown aperto in una tab. Salvataggio
/// manuale (⌘S); un watcher sul file rileva modifiche esterne (agenti,
/// git): buffer pulito → reload automatico, buffer sporco → conflitto
/// che l'utente risolve dal banner (Ricarica / Mantieni).
@Observable @MainActor
public final class MarkdownDocument {
    public let fileURL: URL
    public var text: String
    public private(set) var savedText: String
    public private(set) var externalChangeConflict = false
    public private(set) var fileDeleted = false

    public var isDirty: Bool { text != savedText }

    @ObservationIgnored private var watcher: FileWatcher?

    public init(fileURL: URL) throws {
        self.fileURL = fileURL
        let content = try String(contentsOf: fileURL, encoding: .utf8)
        self.text = content
        self.savedText = content
        startWatching()
    }

    public func save() throws {
        try text.write(to: fileURL, atomically: true, encoding: .utf8)
        savedText = text
        fileDeleted = false
        externalChangeConflict = false
        // Il write atomico sostituisce l'inode: il fd osservato è orfano.
        startWatching()
    }

    public func reloadFromDisk() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        text = disk
        savedText = disk
        externalChangeConflict = false
    }

    /// "Mantieni": il buffer resta; savedText si allinea al disco così il
    /// prossimo ⌘S sovrascrive consapevolmente la versione esterna.
    public func keepLocalBuffer() {
        if let disk = try? String(contentsOf: fileURL, encoding: .utf8) {
            savedText = disk
        }
        externalChangeConflict = false
    }

    public func stopWatching() {
        watcher?.cancel()
        watcher = nil
    }

    // MARK: - Watcher

    private func startWatching() {
        watcher?.cancel()
        watcher = FileWatcher(path: fileURL.path) { [weak self] event in
            Task { @MainActor [weak self] in self?.handleEvent(event) }
        }
    }

    private func handleEvent(_ event: DispatchSource.FileSystemEvent) {
        if event.contains(.delete) || event.contains(.rename) {
            // Write atomico o git checkout: il file è stato sostituito
            // (esiste un nuovo inode) oppure davvero cancellato.
            if FileManager.default.fileExists(atPath: fileURL.path) {
                startWatching()
                handleExternalChange()
            } else {
                handleFileGone()
            }
        } else if event.contains(.write) {
            handleExternalChange()
        }
    }

    /// Internal (non private) per testare la logica senza dipendere dal
    /// timing degli eventi DispatchSource.
    func handleExternalChange() {
        guard let disk = try? String(contentsOf: fileURL, encoding: .utf8) else { return }
        guard disk != savedText else { return }   // eco del nostro save
        if isDirty {
            externalChangeConflict = true
        } else {
            text = disk
            savedText = disk
        }
    }

    func handleFileGone() {
        stopWatching()
        fileDeleted = true
    }
}

/// Wrapper non-actor attorno a DispatchSourceFileSystemObject: il cancel
/// in deinit non può vivere su un tipo @MainActor (deinit non isolato).
final class FileWatcher: @unchecked Sendable {
    private let source: DispatchSourceFileSystemObject

    init?(path: String, onEvent: @escaping @Sendable (DispatchSource.FileSystemEvent) -> Void) {
        let fd = open(path, O_EVTONLY)
        guard fd >= 0 else { return nil }
        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd, eventMask: [.write, .delete, .rename], queue: .main
        )
        source.setEventHandler { onEvent(source.data) }
        source.setCancelHandler { close(fd) }
        source.resume()
        self.source = source
    }

    func cancel() { source.cancel() }

    deinit {
        // cancel è idempotente: sicuro anche se già chiamato esplicitamente.
        source.cancel()
    }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore --filter MarkdownDocumentTests`
Expected: PASS (7 test; `watcherDeliversRealExternalWrite` può richiedere fino a 2s)

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/MarkdownDocument.swift Packages/TillerCore/Tests/TillerCoreTests/MarkdownDocumentTests.swift
git commit -m "feat: MarkdownDocument con save atomico, watcher e gestione conflitti"
```

---

### Task 6: AppModel — apertura tab markdown, dedup, chiusura con dirty check

**Files:**
- Modify: `App/AppModel.swift`
- Modify: `App/TillerApp.swift` (⌘S)

**Interfaces:**
- Consumes: `WorkspaceTab`, `TabContent` (Task 1), `MarkdownDocument` (Task 5), `MarkdownFileLink` (Task 3).
- Produces (usate dal Task 7 e 8):
  ```swift
  // su AppModel:
  var markdownDocuments: [UUID: MarkdownDocument]   // keyed su tab.id
  @discardableResult
  func openMarkdownTab(fileURL: URL, in worktree: Worktree) -> WorkspaceTab?
  func markdownDocument(for tab: WorkspaceTab) -> MarkdownDocument?
  func saveActiveMarkdownDocument()
  func handleTerminalOpenURL(_ raw: String, in worktree: Worktree)
  func openMarkdownFilePanel()
  ```

- [ ] **Step 1: Aggiungi stato e API markdown ad AppModel**

In `App/AppModel.swift`, dopo `var paneCommands: [UUID: String] = [:]` (sezione "Pane commands & agent spawning") aggiungi una nuova sezione:

```swift
// MARK: - Markdown editor

/// Documenti aperti, keyed su tab.id. Vivono qui (non nella view) perché
/// le view SwiftUI muoiono al cambio tab e perderebbero il buffer.
var markdownDocuments: [UUID: MarkdownDocument] = [:]

/// Funnel unico per tutti i canali di apertura (cmd+click, drop, ⌘O).
/// Dedup per fileURL: se il file è già aperto nel worktree attiva quella tab.
@discardableResult
func openMarkdownTab(fileURL: URL, in worktree: Worktree) -> WorkspaceTab? {
    let url = fileURL.standardizedFileURL
    if let existing = tabs[worktree.id]?.first(where: {
        $0.markdownFileURL?.standardizedFileURL == url
    }) {
        selectedWorktree = worktree
        activeTabId[worktree.id] = existing.id
        persistTabs(for: worktree.id)
        return existing
    }
    do {
        let doc = try MarkdownDocument(fileURL: url)
        let tab = WorkspaceTab(id: UUID(), title: url.lastPathComponent,
                               content: .markdown(fileURL: url))
        markdownDocuments[tab.id] = doc
        selectedWorktree = worktree
        tabs[worktree.id, default: []].append(tab)
        activeTabId[worktree.id] = tab.id
        persistTabs(for: worktree.id)
        return tab
    } catch {
        lastError = "Apertura \(url.lastPathComponent) fallita: \(error.localizedDescription)"
        return nil
    }
}

/// Documento della tab; lo crea al volo per le tab ripristinate da sessione.
func markdownDocument(for tab: WorkspaceTab) -> MarkdownDocument? {
    guard let url = tab.markdownFileURL else { return nil }
    if let doc = markdownDocuments[tab.id] { return doc }
    guard let doc = try? MarkdownDocument(fileURL: url) else { return nil }
    markdownDocuments[tab.id] = doc
    return doc
}

/// ⌘S: salva il documento della tab attiva, se è markdown. No-op altrimenti.
func saveActiveMarkdownDocument() {
    guard let worktree = selectedWorktree,
          let tab = activeTab(for: worktree.id),
          let doc = markdownDocuments[tab.id] else { return }
    do { try doc.save() }
    catch { lastError = "Salvataggio \(doc.fileURL.lastPathComponent) fallito: \(error.localizedDescription)" }
}

/// Link attivato (cmd+click) in un pane del terminale: file markdown →
/// tab editor nel worktree del pane; tutto il resto → apertura di sistema.
func handleTerminalOpenURL(_ raw: String, in worktree: Worktree) {
    if let fileURL = MarkdownFileLink.resolve(raw, worktreePath: worktree.path) {
        openMarkdownTab(fileURL: fileURL, in: worktree)
    } else if let url = URL(string: raw) {
        NSWorkspace.shared.open(url)
    }
}

/// File > Apri file… (⌘O): NSOpenPanel filtrato su markdown, nel worktree selezionato.
func openMarkdownFilePanel() {
    guard let worktree = selectedWorktree else { return }
    let panel = NSOpenPanel()
    panel.allowedContentTypes = MarkdownFileLink.extensions
        .compactMap { UTType(filenameExtension: $0) }
    panel.directoryURL = URL(fileURLWithPath: worktree.path)
    panel.canChooseDirectories = false
    panel.allowsMultipleSelection = false
    guard panel.runModal() == .OK, let url = panel.url else { return }
    openMarkdownTab(fileURL: url, in: worktree)
}

/// Alert modale per chiusura con modifiche non salvate.
/// true = procedere con la chiusura.
private func resolveDirtyClose(_ doc: MarkdownDocument) -> Bool {
    let alert = NSAlert()
    alert.messageText = "Salvare le modifiche a \(doc.fileURL.lastPathComponent)?"
    alert.informativeText = "Le modifiche andranno perse se non le salvi."
    alert.addButton(withTitle: "Salva")
    alert.addButton(withTitle: "Non salvare")
    alert.addButton(withTitle: "Annulla")
    switch alert.runModal() {
    case .alertFirstButtonReturn:
        do { try doc.save(); return true }
        catch {
            lastError = "Salvataggio fallito: \(error.localizedDescription)"
            return false
        }
    case .alertSecondButtonReturn:
        return true
    default:
        return false
    }
}

private func teardownMarkdownDocument(tabId: UUID) {
    markdownDocuments[tabId]?.stopWatching()
    markdownDocuments[tabId] = nil
}
```

Aggiungi `import UniformTypeIdentifiers` in testa al file (per `UTType`).

- [ ] **Step 2: Integra dirty check e teardown nella chiusura tab/worktree/progetto**

`closeTab(_:in:)` — all'inizio, prima di `guard var list`:

```swift
if let doc = markdownDocuments[tabId], doc.isDirty {
    guard resolveDirtyClose(doc) else { return }
}
```

e dopo `list.removeAll { $0.id == tabId }` aggiungi:

```swift
teardownMarkdownDocument(tabId: tabId)
```

`removeWorktree(_:)` — dopo `tabs[worktree.id] = nil` aggiungi:

```swift
for tab in tabsBeingRemoved { teardownMarkdownDocument(tabId: tab.id) }
```

dove `let tabsBeingRemoved = tabs[worktree.id] ?? []` va catturato PRIMA di `tabs[worktree.id] = nil`. Stessa cosa in `removeProject(_:)` dentro il loop `for worktree in projectWorktrees`.

- [ ] **Step 3: Scarta al restore le tab markdown con file mancante**

In `bootstrap()`, dopo `let loaded = try await store.loadTabs(of: worktree.id)` sostituisci `guard !loaded.tabs.isEmpty` con:

```swift
// Tab markdown il cui file è sparito tra le sessioni: scartate in silenzio.
let restoredTabs = loaded.tabs.filter { tab in
    guard let url = tab.markdownFileURL else { return true }
    return FileManager.default.fileExists(atPath: url.path)
}
guard !restoredTabs.isEmpty else { continue }
tabs[worktree.id] = restoredTabs
activeTabId[worktree.id] = loaded.activeTabId.flatMap { active in
    restoredTabs.contains { $0.id == active } ? active : nil
} ?? restoredTabs.first?.id
await restoreAgentSessions(
    for: worktree,
    paneIds: Set(restoredTabs.flatMap { $0.leafIds })
)
```

- [ ] **Step 4: ⌘S nel menu File**

In `App/TillerApp.swift`, dentro `CommandGroup(after: .newItem)`, dopo il bottone "Chiudi tab":

```swift
Button("Salva") { model.saveActiveMarkdownDocument() }
    .keyboardShortcut("s", modifiers: .command)
Button("Apri file…") { model.openMarkdownFilePanel() }
    .keyboardShortcut("o", modifiers: .command)
```

- [ ] **Step 5: Verifica build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build CODE_SIGNING_ALLOWED=NO`
Expected: BUILD SUCCEEDED

- [ ] **Step 6: Commit**

```bash
git add App/AppModel.swift App/TillerApp.swift
git commit -m "feat: AppModel apre tab markdown con dedup, dirty check e restore"
```

---

### Task 7: UI editor — MarkdownUI, vista tab, toolbar, integrazione ContentView/TabBar

**Files:**
- Modify: `project.yml` (dipendenza MarkdownUI)
- Create: `App/MarkdownEditor/MarkdownEditorTabView.swift`
- Create: `App/MarkdownEditor/MarkdownToolbar.swift`
- Modify: `App/ContentView.swift` (switch di rendering + drag & drop)
- Modify: `App/TabBarView.swift` (icona documento + pallino dirty)

**Interfaces:**
- Consumes: `MarkdownDocument` (Task 5), `MarkdownSyntax` (Task 4), `AppModel.markdownDocument(for:)` e `openMarkdownTab` (Task 6), `MarkdownFileLink.isMarkdown` (Task 3).
- Produces: `MarkdownEditorTabView(document:)` — vista completa della tab markdown.

- [ ] **Step 1: Aggiungi MarkdownUI a project.yml**

In `project.yml`, sezione `packages:` aggiungi:

```yaml
  MarkdownUI:
    url: https://github.com/gonzalezreal/swift-markdown-ui
    from: 2.4.0
```

e nelle `dependencies` del target `Tiller` aggiungi:

```yaml
      - package: MarkdownUI
```

Run: `xcodegen generate`
Expected: progetto rigenerato senza errori (la prima resolve del package scarica swift-markdown-ui e le sue dipendenze, incluso swift-cmark).

- [ ] **Step 2: Crea MarkdownToolbar**

Crea `App/MarkdownEditor/MarkdownToolbar.swift`:

```swift
import SwiftUI
import TillerCore

/// Toolbar essenziale della modalità codice: ogni bottone applica una
/// trasformazione pura (MarkdownSyntax) sulla selezione corrente.
struct MarkdownToolbar: View {
    @Bindable var document: MarkdownDocument
    @Binding var selection: TextSelection?

    var body: some View {
        HStack(spacing: 2) {
            button("bold", help: "Grassetto") { wrap("**", "**") }
            button("italic", help: "Corsivo") { wrap("*", "*") }
            Divider().frame(height: 14)
            button("1.square", help: "Titolo 1") { prefixLines("# ") }
            button("2.square", help: "Titolo 2") { prefixLines("## ") }
            button("3.square", help: "Titolo 3") { prefixLines("### ") }
            Divider().frame(height: 14)
            button("list.bullet", help: "Lista") { prefixLines("- ") }
            button("link", help: "Link") { wrap("[", "](url)") }
            Spacer()
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
    }

    private func button(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 11))
                .frame(width: 22, height: 20)
        }
        .buttonStyle(.plain)
        .help(help)
    }

    /// Range della selezione corrente; cursore (o niente selezione) →
    /// range vuoto a fine testo, così i marker vengono comunque inseriti.
    private func currentRange() -> Range<String.Index> {
        if case .selection(let range) = selection?.indices {
            return range
        }
        return document.text.endIndex..<document.text.endIndex
    }

    private func wrap(_ prefix: String, _ suffix: String) {
        let (newText, newSelection) = MarkdownSyntax.wrap(
            document.text, selection: currentRange(), prefix: prefix, suffix: suffix
        )
        document.text = newText
        selection = TextSelection(range: newSelection)
    }

    private func prefixLines(_ linePrefix: String) {
        let (newText, newSelection) = MarkdownSyntax.prefixLines(
            document.text, selection: currentRange(), linePrefix: linePrefix
        )
        document.text = newText
        selection = TextSelection(range: newSelection)
    }
}
```

- [ ] **Step 3: Crea MarkdownEditorTabView**

Crea `App/MarkdownEditor/MarkdownEditorTabView.swift`:

```swift
import SwiftUI
import MarkdownUI
import TillerCore

/// Tab editor markdown: preview MarkdownUI (default) + modalità codice,
/// toggle stile Orca in alto a destra. Banner per conflitti esterni e
/// file cancellato.
struct MarkdownEditorTabView: View {
    @Bindable var document: MarkdownDocument
    @State private var mode: EditorMode
    @State private var selection: TextSelection?

    /// Oltre questa soglia la preview non si apre da sola: MarkdownUI
    /// rallenta sui documenti enormi. ponytail: soglia fissa, config se servirà.
    private static let previewByteLimit = 2_000_000
    private let isHuge: Bool

    enum EditorMode { case preview, code }

    init(document: MarkdownDocument) {
        self.document = document
        let huge = document.text.utf8.count > Self.previewByteLimit
        self.isHuge = huge
        self._mode = State(initialValue: huge ? .code : .preview)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            if document.externalChangeConflict { conflictBanner }
            if document.fileDeleted { deletedBanner }
            if mode == .code {
                MarkdownToolbar(document: document, selection: $selection)
                Divider()
                TextEditor(text: $document.text, selection: $selection)
                    .font(.system(.body, design: .monospaced))
                    .scrollContentBackground(.hidden)
                    .padding(8)
            } else {
                ScrollView {
                    Markdown(document.text)
                        .markdownTheme(.gitHub)
                        .textSelection(.enabled)
                        .padding(16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .background(AppTheme.background)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(document.fileURL.path)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)
                .lineLimit(1)
                .truncationMode(.middle)
            if isHuge {
                Text("File grande — preview manuale")
                    .font(.system(size: 10))
                    .foregroundStyle(.orange)
            }
            Spacer()
            Picker("", selection: $mode) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .tag(EditorMode.code)
                    .help("Codice")
                Image(systemName: "doc.richtext")
                    .tag(EditorMode.preview)
                    .help("Preview")
            }
            .pickerStyle(.segmented)
            .frame(width: 76)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }

    private var conflictBanner: some View {
        HStack {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text("File modificato su disco.")
            Spacer()
            Button("Ricarica") { document.reloadFromDisk() }
            Button("Mantieni") { document.keepLocalBuffer() }
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.orange.opacity(0.15))
    }

    private var deletedBanner: some View {
        HStack {
            Image(systemName: "trash").foregroundStyle(.red)
            Text("File eliminato su disco. ⌘S lo ricrea.")
            Spacer()
        }
        .font(.system(size: 12))
        .padding(8)
        .background(.red.opacity(0.15))
    }
}
```

- [ ] **Step 4: Integra il rendering in ContentView + drag & drop**

In `App/ContentView.swift`, dentro il `ForEach(model.tabs[worktreeId] ?? []) { tab in ... }` del `terminalStack`, sostituisci il blocco introdotto nel Task 1 con uno switch completo (i modifier `.opacity/.allowsHitTesting/.accessibilityHidden` si spostano sul `Group`):

```swift
ForEach(model.tabs[worktreeId] ?? []) { tab in
    let isVisible = isSelected
        && model.activeTab(for: worktreeId)?.id == tab.id
    Group {
        switch tab.content {
        case .terminal(let tree):
            TerminalSplitHost(
                tree: tree,
                workingDirectory: worktree.path,
                extraEnvironment: [
                    "TILLER_ENV": "1",
                    "TILLER_SOCKET": ControlSocket.defaultPath(),
                    "TILLER_WORKTREE_ID": worktreeId.uuidString
                ],
                paneContext: (
                    initial: { model.loadScrollback(paneId: $0) },
                    onClose: { id, data in
                        await model.saveScrollback(worktreeId: worktreeId, paneId: id, data: data)
                        await MainActor.run {
                            model.agentActivity.paneClosed(paneId: id)
                            model.paneCommands[id] = nil
                        }
                    },
                    command: { model.paneCommand(paneId: $0) },
                    onTitleChange: { id, title in model.handleTitleChange(paneId: id, title: title) },
                    onContentSignal: { id, tail in model.handleContentSignal(paneId: id, tailText: tail) }
                ),
                menuProvider: { paneId, proxy in
                    menuProvider.items(for: paneId, proxy: proxy)
                },
                onMenuAction: { action, paneId, proxy in
                    menuProvider.handle(action, paneId: paneId, proxy: proxy)
                }
            )
        case .markdown:
            if let doc = model.markdownDocument(for: tab) {
                MarkdownEditorTabView(document: doc)
            } else {
                ContentUnavailableView(
                    "File non trovato",
                    systemImage: "doc.questionmark",
                    description: Text(tab.markdownFileURL?.path ?? "")
                )
            }
        }
    }
    .opacity(isVisible ? 1 : 0)
    .allowsHitTesting(isVisible)
    .accessibilityHidden(!isVisible)
}
```

Drag & drop: sul `VStack` del `detail:` (quello con TabBarView + terminalStack) aggiungi:

```swift
.dropDestination(for: URL.self) { urls, _ in
    guard let worktree = model.selectedWorktree,
          let url = urls.first(where: { MarkdownFileLink.isMarkdown($0) }) else { return false }
    model.openMarkdownTab(fileURL: url, in: worktree)
    return true
}
```

- [ ] **Step 5: TabBarView — icona documento e pallino dirty**

In `App/TabBarView.swift`, dentro `tabItem(_:)`, sostituisci il blocco dell'icona agente con:

```swift
if tab.markdownFileURL != nil {
    Image(systemName: "doc.text")
        .font(.system(size: 10))
        .foregroundStyle(.secondary)
} else if let agentId = tab.leafIds.compactMap({ model.agentActivity.paneAgents[$0] }).first {
    AgentIcon(agentId: agentId, size: 12)
}
```

e subito dopo il `Text(tab.title)` (ramo non-rinomina) aggiungi il pallino dirty:

```swift
if model.markdownDocuments[tab.id]?.isDirty == true {
    Circle().fill(.secondary).frame(width: 5, height: 5)
}
```

- [ ] **Step 6: Verifica build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build CODE_SIGNING_ALLOWED=NO`
Expected: BUILD SUCCEEDED

- [ ] **Step 7: Commit**

```bash
git add project.yml App/MarkdownEditor App/ContentView.swift App/TabBarView.swift Tiller.xcodeproj
git commit -m "feat: vista editor markdown con preview MarkdownUI, toolbar e drag&drop"
```

(Se `Tiller.xcodeproj` è in `.gitignore`, ometterlo dall'add.)

---

### Task 8: Cmd+click nel terminale — delegate open-url

**Files:**
- Create: `Packages/TillerTerminal/Sources/TillerTerminal/TerminalOpenURLRouter.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`
- Modify: `App/ContentView.swift`

**Interfaces:**
- Consumes: `AppModel.handleTerminalOpenURL(_:in:)` (Task 6); `TerminalSurfaceOpenURLDelegate` e `TerminalViewState` (libghostty-spm).
- Produces: elemento `onOpenURL: (UUID, String) -> Void` in coda alla tupla `paneContext` di `TerminalSplitHost`.

**Contesto per l'implementatore:** libghostty-spm invoca `(delegate as? any TerminalSurfaceOpenURLDelegate)?.terminalDidRequestOpenURL(url, kind:)` quando l'utente cmd+clicca un link. Il delegate della surface è il `TerminalViewState` (assegnato dal representable interno: `view.delegate = context`), che nel package conforma molti protocolli delegate ma NON quello open-url → oggi il cmd+click non fa nulla in Tiller, nemmeno per i link http. `TerminalViewState` è `final`: si aggiunge la conformance via `@retroactive` extension + un router MainActor keyed su `ObjectIdentifier`.

- [ ] **Step 1: Crea il router con conformance retroattiva**

Crea `Packages/TillerTerminal/Sources/TillerTerminal/TerminalOpenURLRouter.swift`:

```swift
import Foundation
import GhosttyTerminal

/// Instrada i cmd+click sui link del terminale. TerminalViewState (final,
/// del package GhosttyTerminal) è il delegate della surface ma non
/// conforma TerminalSurfaceOpenURLDelegate: la conformance retroattiva
/// qui sotto smista al pane registrato via ObjectIdentifier dello state.
@MainActor
enum TerminalOpenURLRouter {
    private static var handlers: [ObjectIdentifier: (String) -> Void] = [:]

    static func register(_ state: TerminalViewState, handler: @escaping (String) -> Void) {
        handlers[ObjectIdentifier(state)] = handler
    }

    static func unregister(_ state: TerminalViewState) {
        handlers[ObjectIdentifier(state)] = nil
    }

    static func route(from state: TerminalViewState, url: String) {
        handlers[ObjectIdentifier(state)]?(url)
    }
}

// ponytail: conformance retroattiva a un protocollo di un altro package —
// se un futuro libghostty-spm aggiunge questa conformance il compilatore
// segnala il conflitto e questo file va rimosso a favore dell'upstream.
extension TerminalViewState: @retroactive TerminalSurfaceOpenURLDelegate {
    public func terminalDidRequestOpenURL(_ url: String, kind: TerminalOpenURLKind) {
        TerminalOpenURLRouter.route(from: self, url: url)
    }
}
```

- [ ] **Step 2: Propaga onOpenURL in PtyTerminalPane**

In `PtyTerminalPane.swift`:
- aggiungi la property `private let onOpenURL: ((UUID, String) -> Void)?` accanto a `onContentSignal`;
- aggiungi il parametro `onOpenURL: ((UUID, String) -> Void)? = nil` all'`init` (dopo `onContentSignal`) e assegnalo;
- in `body`, dentro `.onAppear` (dopo `rt.start()`):

```swift
TerminalOpenURLRouter.register(state) { [paneId, onOpenURL] url in
    onOpenURL?(paneId, url)
}
```

- dentro `.onDisappear` (prima di `runtime?.stop()`):

```swift
TerminalOpenURLRouter.unregister(state)
```

- [ ] **Step 3: Estendi la tupla paneContext di TerminalSplitHost**

In `SplitViewRenderer.swift` aggiungi `onOpenURL: (UUID, String) -> Void` in coda alla tupla `paneContext` (sia nella property che nell'`init`):

```swift
let paneContext: (
    initial: (UUID) -> Data?,
    onClose: @Sendable (UUID, Data) async -> Void,
    command: (UUID) -> String?,
    onTitleChange: (UUID, String) -> Void,
    onContentSignal: (UUID, String) -> Void,
    onOpenURL: (UUID, String) -> Void
)?
```

e nel `node(_:coordinator:)` passa il campo al pane:

```swift
PtyTerminalPane(
    workingDirectory: workingDirectory,
    command: paneContext?.command(id),
    paneId: id,
    initialScrollback: paneContext?.initial(id),
    extraEnvironment: extraEnvironment,
    onScrollback: paneContext?.onClose,
    onTitleChange: paneContext?.onTitleChange,
    onContentSignal: paneContext?.onContentSignal,
    onOpenURL: paneContext?.onOpenURL,
    onContextMenu: menuProvider
)
```

(attenzione all'ordine dei parametri: `onOpenURL` prima di `onContextMenu`, coerente con l'init del pane).

- [ ] **Step 4: Collega in ContentView**

In `App/ContentView.swift`, nella tupla `paneContext` del ramo `.terminal`, aggiungi in coda:

```swift
onOpenURL: { _, url in model.handleTerminalOpenURL(url, in: worktree) }
```

- [ ] **Step 5: Verifica build completa**

Run: `swift build --package-path Packages/TillerTerminal && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build CODE_SIGNING_ALLOWED=NO`
Expected: BUILD SUCCEEDED (l'eventuale warning sulla conformance retroattiva è atteso e documentato nel commento)

- [ ] **Step 6: Commit**

```bash
git add Packages/TillerTerminal App/ContentView.swift
git commit -m "feat: cmd+click su link markdown nel terminale apre l'editor"
```

---

### Task 9: Verifica finale end-to-end

**Files:**
- Modify: `README.md` (riga feature)

- [ ] **Step 1: Tutti i test package**

Run:
```bash
swift test --package-path Packages/TillerCore
swift test --package-path Packages/TillerPersistence
```
Expected: PASS ovunque

- [ ] **Step 2: Build release-like**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build CODE_SIGNING_ALLOWED=NO`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Checklist manuale (lanciare l'app)**

Da spuntare manualmente con l'app in esecuzione (`open` sul .app buildato o run da Xcode):

1. Seleziona un worktree → `⌘O` → scegli un `.md` → si apre tab con preview renderizzata, titolo = nome file.
2. Toggle codice → editing monospace; digita → pallino dirty sul tab; `⌘S` → pallino sparisce.
3. Toolbar: seleziona testo → bold → `**testo**` inserito, selezione mantenuta.
4. Trascina un `.md` dal Finder sulla finestra → si apre tab editor.
5. Riapri lo stesso file via `⌘O` → NON si crea una seconda tab, si attiva quella esistente.
6. In un terminale: `printf '\e]8;;file://%s/README.md\e\\README\e]8;;\e\\\n' "$PWD"` → cmd+click sul link → si apre l'editor. Cmd+click su un link https → si apre il browser.
7. Con tab markdown aperta e pulita: `echo nuovo >> file.md` da terminale → il contenuto si aggiorna da solo. Con buffer sporco → appare il banner Ricarica/Mantieni.
8. `⌘W` su tab dirty → alert Salva/Non salvare/Annulla.
9. Riavvia l'app → la tab markdown è ripristinata in preview.
10. Le tab terminale funzionano come prima (split, agenti, badge, scrollback).

- [ ] **Step 4: Aggiorna README**

In `README.md`, nella lista Features, dopo la riga "Native terminal" aggiungi:

```markdown
- 📝 **Markdown editor** — click a `.md` link in the terminal (or drag & drop / ⌘O) to open it in a tab: rendered preview + code mode, live reload while agents write
```

e nella tabella Keyboard Shortcuts aggiungi:

```markdown
| Open markdown file in selected worktree | `⌘O` |
| Save markdown file | `⌘S` |
```

- [ ] **Step 5: Commit finale**

```bash
git add README.md
git commit -m "docs: markdown editor nel README"
```
