# Terminal Split Panes in Sidebar Tree — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show terminal split panes as sidebar tree nodes with a context menu (targeted split, "affianca al terminale corrente" that moves a live pane next to the current terminal without killing its PTY, close with confirmation).

**Architecture:** Pane views stay alive across tab moves via a shared per-worktree `TerminalPaneCache` used by every `TerminalSplitHost` of that worktree. `adoptPane` is a pure `SplitTree` mutation in AppModel (remove leaf from source tab, graft into active tab). Sidebar gains a 4th-level `PaneRow` shown only when a tab has >1 pane, labeled with the last PTY title (`paneTitles`, in-memory).

**Tech Stack:** SwiftUI + AppKit (NSViewControllerRepresentable), Swift Testing (`import Testing`, `#expect`), packages TillerCore / TillerTerminal.

**Spec:** `docs/superpowers/specs/2026-07-10-terminal-splits-sidebar-design.md`

## Global Constraints

- Deployment target macOS 15.0; no new dependencies.
- NON fare push. NON committare `.tokensave/`, `.serena/` o `DerivedData/`.
- Verifica build: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3` (run `xcodegen generate` first if project file is missing).
- Package test noti flaky sotto carico: se fallisce SOLO TillerTerminal in `./Scripts/ci.sh`, rilanciare `cd Packages/TillerTerminal && swift test` da solo (seriale) — verde = ok.
- Testo UI in italiano, coerente con l'esistente ("Nuovo Terminale", "Rinomina", "Chiudi").
- `AppModel` è `@MainActor @Observable`; già importa `TillerTerminal`.

---

### Task 1: SplitTree graft semantics tests (TillerCore)

Characterization tests: nessun codice di produzione nuovo — fissano la semantica di `splitting(newLeaf:)`/`removing(leaf:)` su cui `adoptPane` (Task 3) si appoggia. Se uno di questi fallisce, la semantica è diversa dal previsto: STOP e segnala, non "aggiustare" SplitTree.

**Files:**
- Test (create): `Packages/TillerCore/Tests/TillerCoreTests/SplitTreeGraftTests.swift`

**Interfaces:**
- Consumes: `SplitTree.splitting(leaf:axis:newLeaf:)`, `SplitTree.removing(leaf:)`, `SplitTree.leafIds` (già esistenti in `Packages/TillerCore/Sources/TillerCore/SplitTree.swift`).
- Produces: garanzia che `splitting(…, newLeaf: idEsistente)` innesta un leaf con id arbitrario e che `removing` dell'unico leaf torna `nil`.

- [ ] **Step 1: Write the tests**

```swift
// Packages/TillerCore/Tests/TillerCoreTests/SplitTreeGraftTests.swift
import Testing
import Foundation
@testable import TillerCore

/// Semantica di SplitTree usata da adoptPane (spostamento pane tra tab):
/// splitting(newLeaf:) accetta un id già esistente altrove (graft) e
/// removing(leaf:) su albero mono-leaf torna nil (tab svuotato).
struct SplitTreeGraftTests {
    @Test func splittingGraftsExistingLeafIdIntoTree() {
        // Arrange
        let anchor = UUID()
        let moved = UUID()
        let tree = SplitTree.leaf(id: anchor)

        // Act
        let grafted = tree.splitting(leaf: anchor, axis: .horizontal, newLeaf: moved)

        // Assert
        #expect(grafted == .split(axis: .horizontal, first: .leaf(id: anchor), second: .leaf(id: moved)))
        #expect(grafted.leafIds == [anchor, moved])
    }

    @Test func removingOnlyLeafReturnsNil() {
        let only = UUID()
        #expect(SplitTree.leaf(id: only).removing(leaf: only) == nil)
    }

    @Test func moveSequencePreservesRemainingTreeAndLeafOrder() {
        // Arrange: sorgente con due pane, destinazione mono-pane.
        let a = UUID()
        let b = UUID()
        let dest = UUID()
        let source = SplitTree.leaf(id: a).splitting(leaf: a, axis: .vertical, newLeaf: b)

        // Act: rimuovo b dal sorgente e lo innesto nella destinazione.
        let sourceAfter = source.removing(leaf: b)
        let destination = SplitTree.leaf(id: dest)
            .splitting(leaf: dest, axis: .horizontal, newLeaf: b)

        // Assert
        #expect(sourceAfter == .leaf(id: a))
        #expect(destination.leafIds == [dest, b])
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cd Packages/TillerCore && swift test --filter SplitTreeGraftTests`
Expected: 3 test PASS (characterization: il codice esiste già). Se FAIL → STOP, la semantica non è quella assunta dal design; riportarlo senza modificare SplitTree.

- [ ] **Step 3: Commit**

```bash
git add Packages/TillerCore/Tests/TillerCoreTests/SplitTreeGraftTests.swift
git commit -m "test: characterize SplitTree graft semantics for pane adoption"
```

---

### Task 2: TerminalPaneCache + shared cache in TerminalSplitHost (TillerTerminal)

**Files:**
- Create: `Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift`
- Modify: `Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`
- Test (create): `Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalPaneCacheTests.swift`

**Interfaces:**
- Consumes: `TerminalSplitHost` esistente (struct `NSViewControllerRepresentable` con `Coordinator.leafControllers`, `pruneCache`, `node`).
- Produces (usati da Task 3):
  - `public final class TerminalPaneCache` (`@MainActor`) con `public var controllers: [UUID: NSViewController]`, `public init()`, `public func prune(keeping: Set<UUID>)`.
  - `TerminalSplitHost.init(…, paneCache: TerminalPaneCache? = nil, liveLeafIds: (() -> Set<UUID>)? = nil)` — due parametri nuovi IN CODA alla lista parametri (dopo `onMenuAction`), default nil = comportamento attuale invariato.

- [ ] **Step 1: Write the failing test**

```swift
// Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalPaneCacheTests.swift
import Testing
import AppKit
@testable import TillerTerminal

@MainActor
struct TerminalPaneCacheTests {
    @Test func pruneDropsControllersOutsideKeepSet() {
        // Arrange
        let cache = TerminalPaneCache()
        let keep = UUID()
        let drop = UUID()
        cache.controllers[keep] = NSViewController()
        cache.controllers[drop] = NSViewController()

        // Act
        cache.prune(keeping: [keep])

        // Assert
        #expect(cache.controllers[keep] != nil)
        #expect(cache.controllers[drop] == nil)
    }

    @Test func pruneWithEmptyKeepSetClearsCache() {
        let cache = TerminalPaneCache()
        cache.controllers[UUID()] = NSViewController()
        cache.prune(keeping: [])
        #expect(cache.controllers.isEmpty)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd Packages/TillerTerminal && swift test --filter TerminalPaneCacheTests`
Expected: FAIL — "cannot find 'TerminalPaneCache' in scope".

- [ ] **Step 3: Create TerminalPaneCache**

```swift
// Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd Packages/TillerTerminal && swift test --filter TerminalPaneCacheTests`
Expected: 2 test PASS.

- [ ] **Step 5: Wire the shared cache into TerminalSplitHost**

In `Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift`:

5a. Aggiungi due stored property dopo `let onMenuAction: …`:

```swift
    let paneCache: TerminalPaneCache?
    let liveLeafIds: (() -> Set<UUID>)?
```

5b. Nell'`init`, aggiungi i due parametri IN CODA (dopo `onMenuAction`) e le assegnazioni:

```swift
    public init(
        tree: SplitTree,
        workingDirectory: String? = nil,
        extraEnvironment: [String: String] = [:],
        paneContext: (
            initial: (UUID) -> Data?,
            onClose: @Sendable (UUID, Data) async -> Void,
            command: (UUID) -> String?,
            onTitleChange: (UUID, String) -> Void,
            onContentSignal: (UUID, String) -> Void,
            onOpenURL: (UUID, String) -> Void
        )? = nil,
        menuProvider: ((UUID, TerminalSurfaceProxy) -> [TerminalContextMenuItem])? = nil,
        onMenuAction: ((TerminalContextMenuAction, UUID, TerminalSurfaceProxy) -> Void)? = nil,
        paneCache: TerminalPaneCache? = nil,
        liveLeafIds: (() -> Set<UUID>)? = nil
    ) {
        self.tree = tree
        self.workingDirectory = workingDirectory
        self.extraEnvironment = extraEnvironment
        self.paneContext = paneContext
        self.menuProvider = menuProvider
        self.onMenuAction = onMenuAction
        self.paneCache = paneCache
        self.liveLeafIds = liveLeafIds
    }
```

5c. Sostituisci `pruneCache` e aggiungi i due helper di accesso cache:

```swift
    /// Con cache condivisa il set "keeping" è il set dei pane vivi in TUTTI
    /// i tab del worktree (liveLeafIds), non solo quelli di questo host: un
    /// pane appena spostato in un altro tab non va scartato.
    private func pruneCache(_ coordinator: Coordinator, keeping treeLeaves: Set<UUID>) {
        let keeping = liveLeafIds.map { $0() } ?? treeLeaves
        if let paneCache {
            paneCache.prune(keeping: keeping)
        } else {
            for key in coordinator.leafControllers.keys where !keeping.contains(key) {
                coordinator.leafControllers.removeValue(forKey: key)
            }
        }
    }

    private func cachedController(for id: UUID, _ coordinator: Coordinator) -> NSViewController? {
        if let paneCache { return paneCache.controllers[id] }
        return coordinator.leafControllers[id]
    }

    private func storeController(_ controller: NSViewController, for id: UUID, _ coordinator: Coordinator) {
        if let paneCache {
            paneCache.controllers[id] = controller
        } else {
            coordinator.leafControllers[id] = controller
        }
    }
```

5d. In `node(_:coordinator:)`, sostituisci il case `.leaf`:

```swift
        case .leaf(let id):
            if let cached = cachedController(for: id, coordinator) {
                // Adozione cross-host: stacca il controller dalla gerarchia
                // del tab precedente prima di inserirlo qui (un VC ha un
                // solo parent). No-op nei rebuild interni allo stesso host.
                cached.removeFromParent()
                cached.view.removeFromSuperview()
                return cached
            }
            let hosting = NSHostingController(
                rootView: PtyTerminalPane(
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
            )
            storeController(hosting, for: id, coordinator)
            return hosting
```

- [ ] **Step 6: Run the package tests**

Run: `cd Packages/TillerTerminal && swift test`
Expected: tutti PASS (incluse le 2 nuove; se falliscono solo i PtyProcessTests sotto carico, rilancia una volta il comando da solo).

- [ ] **Step 7: Commit**

```bash
git add Packages/TillerTerminal/Sources/TillerTerminal/TerminalPaneCache.swift \
        Packages/TillerTerminal/Sources/TillerTerminal/SplitViewRenderer.swift \
        Packages/TillerTerminal/Tests/TillerTerminalTests/TerminalPaneCacheTests.swift
git commit -m "feat: shared per-worktree pane controller cache in TerminalSplitHost"
```

---

### Task 3: AppModel — paneTitles, adoptPane, closeTerminal, cache plumbing

**Files:**
- Modify: `App/AppModel.swift` (punti indicati sotto)
- Modify: `App/ContentView.swift` (call site `TerminalSplitHost`, closure `onClose`)

**Interfaces:**
- Consumes: `TerminalPaneCache` e i parametri `paneCache:`/`liveLeafIds:` di `TerminalSplitHost` (Task 2); `SplitTree.splitting/removing` (Task 1); esistenti `tabContaining(paneId:)`, `activeTab(for:)`, `closeTab(_:in:)`, `closePane(paneId:)`, `persistTabs(for:)`.
- Produces (usati da Task 4):
  - `var paneTitles: [UUID: String]`
  - `func paneCache(for worktreeId: UUID) -> TerminalPaneCache`
  - `func liveLeafIds(for worktreeId: UUID) -> Set<UUID>`
  - `func canAdoptPane(_ paneId: UUID) -> Bool`
  - `func adoptPane(_ paneId: UUID)`
  - `func closeTerminal(paneId: UUID)`

- [ ] **Step 1: paneTitles + handleTitleChange**

In `App/AppModel.swift`, accanto a `var paneCommands: [UUID: String] = [:]` (riga ~668) aggiungi:

```swift
    /// Ultimo titolo PTY riportato da ogni pane (solo in memoria: al riavvio
    /// la shell lo rigenera). Alimenta le etichette dei nodi pane in sidebar.
    var paneTitles: [UUID: String] = [:]
```

Sostituisci `handleTitleChange` (riga ~891):

```swift
    func handleTitleChange(paneId: UUID, title: String) {
        paneTitles[paneId] = title
        guard let t = agentActivity.handleTitleChange(paneId: paneId, title: title, now: Date()) else { return }
        notifyTransition(paneId: paneId, from: t.old, to: t.new)
    }
```

- [ ] **Step 2: cache per worktree + live set**

Sempre in `App/AppModel.swift`, dopo `func closePane(paneId:)` (riga ~598) aggiungi:

```swift
    // MARK: - Pane controller caches (una per worktree)

    private var paneCaches: [UUID: TerminalPaneCache] = [:]

    func paneCache(for worktreeId: UUID) -> TerminalPaneCache {
        if let cache = paneCaches[worktreeId] { return cache }
        let cache = TerminalPaneCache()
        paneCaches[worktreeId] = cache
        return cache
    }

    /// Tutti i pane vivi nei tab del worktree — set di pruning per gli host.
    func liveLeafIds(for worktreeId: UUID) -> Set<UUID> {
        Set((tabs[worktreeId] ?? []).flatMap(\.leafIds))
    }
```

- [ ] **Step 3: pruning centralizzato in persistTabs + cleanup removeWorktree**

Sostituisci l'inizio di `persistTabs(for:)` (riga ~602) aggiungendo la prima riga:

```swift
    private func persistTabs(for worktreeId: UUID) {
        // Ogni mutazione tab passa di qui: il prune tiene la cache condivisa
        // allineata (un pane chiuso viene scartato → deinit → stop PTY).
        paneCaches[worktreeId]?.prune(keeping: liveLeafIds(for: worktreeId))
        guard let store else { return }
        let list = tabs[worktreeId] ?? []
        let active = activeTabId[worktreeId]
        Task { try? await store.saveTabs(worktreeId: worktreeId, tabs: list, activeTabId: active) }
    }
```

In `removeWorktree(_:)` (riga ~436), subito dopo `tabs[worktree.id] = nil` aggiungi:

```swift
            paneCaches[worktree.id] = nil
```

- [ ] **Step 4: canAdoptPane / adoptPane / closeTerminal**

Dopo il blocco cache del Step 2 aggiungi:

```swift
    /// True se il pane può essere affiancato al terminale corrente: stesso
    /// worktree del tab attivo, non già nel tab attivo, tab attivo terminale.
    func canAdoptPane(_ paneId: UUID) -> Bool {
        guard let worktree = selectedWorktree,
              let source = tabContaining(paneId: paneId),
              source.worktree.id == worktree.id,
              let active = activeTab(for: worktree.id),
              active.terminalTree != nil,
              !active.leafIds.contains(paneId) else { return false }
        return true
    }

    /// Sposta il pane in split orizzontale accanto al primo leaf del tab
    /// attivo. Il tab sorgente svuotato viene rimosso direttamente (niente
    /// closeTab: il pane è vivo altrove, i suoi session ref non vanno toccati).
    func adoptPane(_ paneId: UUID) {
        guard canAdoptPane(paneId),
              let worktree = selectedWorktree,
              let source = tabContaining(paneId: paneId),
              let active = activeTab(for: worktree.id),
              let destTree = active.terminalTree,
              let anchor = destTree.leafIds.first,
              let sourceTree = source.tab.terminalTree else { return }

        if let remaining = sourceTree.removing(leaf: paneId) {
            tabs[worktree.id]?[source.index].content = .terminal(remaining)
        } else {
            tabs[worktree.id]?.remove(at: source.index)
        }
        guard let destIndex = tabs[worktree.id]?.firstIndex(where: { $0.id == active.id }) else { return }
        tabs[worktree.id]?[destIndex].content =
            .terminal(destTree.splitting(leaf: anchor, axis: .horizontal, newLeaf: paneId))
        persistTabs(for: worktree.id)
    }

    /// Chiude un terminale: se è l'unico pane del tab chiude l'intera tab.
    func closeTerminal(paneId: UUID) {
        guard let target = tabContaining(paneId: paneId) else { return }
        if target.tab.leafIds.count <= 1 {
            closeTab(target.tab.id, in: target.worktree)
        } else {
            closePane(paneId: paneId)
        }
    }
```

- [ ] **Step 5: plumbing in ContentView**

In `App/ContentView.swift`, nel call site `TerminalSplitHost(…)` dentro `terminalStack`, aggiungi DOPO `onMenuAction: { … }` i due argomenti:

```swift
                                    onMenuAction: { action, paneId, proxy in
                                        menuProvider.handle(action, paneId: paneId, proxy: proxy)
                                    },
                                    paneCache: model.paneCache(for: worktreeId),
                                    liveLeafIds: { model.liveLeafIds(for: worktreeId) }
```

Nella closure `onClose` dello stesso call site, dopo `model.paneCommands[id] = nil` aggiungi:

```swift
                                                model.paneTitles[id] = nil
```

- [ ] **Step 6: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 7: Commit**

```bash
git add App/AppModel.swift App/ContentView.swift
git commit -m "feat: pane titles, adoptPane and shared pane cache plumbing in AppModel"
```

---

### Task 4: Sidebar — PaneRow, menu contestuale terminale, conferma chiusura

**Files:**
- Modify: `App/SidebarView.swift`

**Interfaces:**
- Consumes (Task 3): `model.paneTitles`, `model.canAdoptPane(_:)`, `model.adoptPane(_:)`, `model.closeTerminal(paneId:)`, `model.split(paneId:axis:)`; esistenti `TreeGuideLines(throughLines:elbowAt:branchLength:isLast:)`, `AgentIcon`, `AppTheme`.
- Produces: `PaneRow` (privata), `TerminalPaneMenu` (privata); `TabRow` acquisisce menu terminale con conferma.

Nota geometrie: worktree leading 10 (elbow x 3), tab leading 44 (through [3], elbow 26, branch 14). I pane: leading 60, through [3] (+26 solo se il tab NON è l'ultimo), elbow 42, branch 14.

- [ ] **Step 1: PaneRow e TerminalPaneMenu**

In `App/SidebarView.swift`, dopo la struct `TabRow` aggiungi:

```swift
/// Nodo pane (4° livello): mostrato solo quando il tab terminale ha più di
/// un pane. Etichetta = ultimo titolo PTY, fallback posizionale "Pane N".
private struct PaneRow: View {
    @Bindable var model: AppModel
    let worktree: Worktree
    let tab: WorkspaceTab
    let paneId: UUID
    let index: Int
    /// Ultimo pane del tab: chiude la guida col raccordo curvo.
    let isLast: Bool
    /// Il tab padre è l'ultimo del worktree: la verticale di livello tab
    /// (x 26) non prosegue oltre.
    let tabIsLast: Bool
    @State private var hovering = false
    @State private var confirmingClose = false

    var body: some View {
        HStack(spacing: 7) {
            icon
                .frame(width: 14)
            Text(model.paneTitles[paneId] ?? "Pane \(index + 1)")
                .font(.system(size: 11.5))
                .foregroundStyle(AppTheme.subtitle)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 4)
        }
        .padding(.vertical, 3)
        .padding(.horizontal, 9)
        .contentShape(Rectangle())
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(hovering ? AppTheme.rowHover : Color.clear)
        )
        .padding(.leading, 60)
        .padding(.vertical, 1)
        .overlay(TreeGuideLines(
            throughLines: tabIsLast ? [3] : [3, 26],
            elbowAt: 42, branchLength: 14, isLast: isLast
        ))
        .focusEffectDisabled()
        .onHover { hovering = $0 }
        .onTapGesture {
            model.selectedWorktree = worktree
            model.activateTab(tab.id, in: worktree.id)
        }
        .contextMenu {
            TerminalPaneMenu(model: model, paneId: paneId, confirmingClose: $confirmingClose)
        }
        .alert("Chiudere il terminale?", isPresented: $confirmingClose) {
            Button("Annulla", role: .cancel) {}
            Button("Chiudi", role: .destructive) { model.closeTerminal(paneId: paneId) }
        } message: {
            Text("Il processo in esecuzione verrà terminato.")
        }
    }

    @ViewBuilder private var icon: some View {
        if let agentId = model.agentActivity.paneAgents[paneId] {
            AgentIcon(agentId: agentId, size: 12)
        } else {
            Image(systemName: "terminal")
                .font(.system(size: 10))
                .foregroundStyle(AppTheme.meta)
        }
    }
}

/// Voci di menu condivise per un pane terminale (PaneRow e TabRow mono-pane):
/// split mirato, affianca (solo stesso worktree e fuori dal tab attivo),
/// chiusura con conferma (il chiamante mostra l'alert).
private struct TerminalPaneMenu: View {
    @Bindable var model: AppModel
    let paneId: UUID
    @Binding var confirmingClose: Bool

    var body: some View {
        Button("Split orizzontale") { model.split(paneId: paneId, axis: .horizontal) }
        Button("Split verticale") { model.split(paneId: paneId, axis: .vertical) }
        if model.canAdoptPane(paneId) {
            Button("Affianca al terminale corrente") { model.adoptPane(paneId) }
        }
        Divider()
        Button("Chiudi terminale…", role: .destructive) { confirmingClose = true }
    }
}
```

- [ ] **Step 2: menu contestuale di TabRow con conferma**

In `TabRow` aggiungi lo state accanto a `@State private var renaming = false`:

```swift
    @State private var confirmingClose = false
```

Sostituisci l'intero blocco `.contextMenu { … }` di `TabRow` e aggiungi l'alert subito dopo:

```swift
        .contextMenu {
            Button("Rinomina") {
                draftTitle = tab.title
                renaming = true
                renameFieldFocused = true
            }
            if tab.terminalTree != nil {
                if tab.leafIds.count == 1, let paneId = tab.leafIds.first {
                    TerminalPaneMenu(model: model, paneId: paneId, confirmingClose: $confirmingClose)
                } else {
                    Divider()
                    Button("Chiudi tab…", role: .destructive) { confirmingClose = true }
                }
            } else {
                Button("Chiudi") {
                    model.closeTab(tab.id, in: worktree)
                }
            }
        }
        .alert("Chiudere il terminale?", isPresented: $confirmingClose) {
            Button("Annulla", role: .cancel) {}
            Button("Chiudi", role: .destructive) { model.closeTab(tab.id, in: worktree) }
        } message: {
            Text("Il processo in esecuzione verrà terminato.")
        }
```

(La conferma di TabRow chiude sempre l'intera tab: per il mono-pane equivale a `closeTerminal`; per il multi-pane "Chiudi tab…" termina tutti i pane. La × in hover resta senza conferma: fuori scope.)

- [ ] **Step 3: righe pane nel body di SidebarView**

Nel `body` di `SidebarView`, sostituisci il blocco `ForEach(tabs)`:

```swift
                                let tabs = model.tabs[worktree.id] ?? []
                                ForEach(tabs) { tab in
                                    let tabIsLast = tab.id == tabs.last?.id
                                    TabRow(model: model, worktree: worktree, tab: tab,
                                           isLast: tabIsLast)
                                    if tab.terminalTree != nil, tab.leafIds.count > 1 {
                                        let leafIds = tab.leafIds
                                        ForEach(Array(leafIds.enumerated()), id: \.element) { index, paneId in
                                            PaneRow(model: model, worktree: worktree, tab: tab,
                                                    paneId: paneId, index: index,
                                                    isLast: paneId == leafIds.last,
                                                    tabIsLast: tabIsLast)
                                        }
                                    }
                                }
```

- [ ] **Step 4: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -3`
Expected: `** BUILD SUCCEEDED **`

- [ ] **Step 5: Commit**

```bash
git add App/SidebarView.swift
git commit -m "feat: pane nodes in sidebar tree with split/adopt/close context menu"
```

---

### Task 5: Verifica completa

**Files:** nessuna modifica prevista (solo fix eventuali).

- [ ] **Step 1: CI**

Run: `./Scripts/ci.sh`
Expected: `CI OK`. Se fallisce SOLO TillerTerminal: `cd Packages/TillerTerminal && swift test` da solo; verde = ok (flakiness nota sotto carico).

- [ ] **Step 2: Lancio app**

```bash
pkill -x Tiller 2>/dev/null; sleep 1
open DerivedData/Build/Products/Debug/Tiller.app
```

- [ ] **Step 3: Checklist manuale (utente)**

1. Split da titlebar → sotto il tab compaiono 2 nodi pane; con 1 solo pane nessun nodo.
2. Etichette pane = titolo shell (es. "zsh"); prima del primo titolo "Pane 1"/"Pane 2".
3. Tasto destro su nodo pane: Split orizzontale/verticale mirati; "Affianca al terminale corrente" visibile solo per pane fuori dal tab attivo, stesso worktree; "Chiudi terminale…" mostra conferma.
4. **Sopravvivenza PTY**: `top` in un pane di un altro tab → "Affianca al terminale corrente" → `top` ancora in esecuzione nel pane spostato. Se il processo muore, riportarlo (fallback respawn accettato dal design ma da segnalare).
5. Tab sorgente svuotato dall'affianca → sparisce dalla sidebar.
6. Tasto destro su tab terminale mono-pane: stesso menu del pane. Multi-pane: "Chiudi tab…" con conferma. Markdown: menu invariato.
7. Guide dell'albero: verticali continue, raccordo curvo su ultimo pane/tab/riga New Worktree.

---

## Self-review (fatto)

- **Spec coverage**: Sezione 1 → Task 3 (paneTitles) + Task 4 (PaneRow); Sezione 2 → Task 4 (menu + conferma) + Task 3 (canAdoptPane/adoptPane/closeTerminal); Sezione 3 → Task 2 (cache) + Task 3 (plumbing); test SplitTree → Task 1; rischio PTY → Task 5 checklist punto 4.
- **Placeholder**: nessuno; ogni step con codice completo.
- **Type consistency**: `TerminalPaneCache.controllers/prune(keeping:)` coerenti tra Task 2 e 3; firme `paneCache(for:)`/`liveLeafIds(for:)`/`canAdoptPane`/`adoptPane`/`closeTerminal(paneId:)` coerenti tra Task 3 e 4; `TreeGuideLines(throughLines:elbowAt:branchLength:isLast:)` coerente con l'esistente.
