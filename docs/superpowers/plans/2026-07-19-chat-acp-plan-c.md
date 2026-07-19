# Chat ACP Piano C — following + edit review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chiudere le sezioni 5 e 6 della spec `docs/superpowers/specs/2026-07-19-chat-acp-feature-complete-design.md`: toggle "Segui l'agente" che apre il diff del file toccato nel right panel, e card di fine turno "N file modificati" con revert git per file.

**Architecture:** Il reducer (TillerACP, puro) accumula i path dei tool call `kind == .edit` completati e a fine turno emette un nuovo item `.editSummary`. Il following vive nel ChatController (estrazione locations dagli update, throttle 500ms) e arriva al right panel via una richiesta observable su AppModel che ContentView traduce in `selectDiff` sul RightPanelModel. Il revert usa `GitActions.discardChanges`/`discardUntracked` esistenti in TillerGit.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`@Test`/`#expect`), GRDB (nessuna migrazione: payload JSON).

## Global Constraints

- Test SEMPRE swift-testing (`import Testing`, `@Test`, `#expect`), MAI XCTest.
- Conventional commits, soggetto imperativo minuscolo.
- Working tree PULITO all'avvio: non stashare, non resettare, non toccare i commit esistenti.
- Dopo aver creato nuovi file in `App/`: eseguire `xcodegen generate` e includere `Tiller.xcodeproj/project.pbxproj` nel commit.
- NON eseguire `Scripts/ci.sh` né rilanciare l'app (Task 5 è fuori da questo mandato).
- Stringhe UI in italiano, coerenti con quelle esistenti ("Nuova conversazione", "Configurazione MCP").
- Decisioni chiuse della spec: following default OFF, non persistito; nessun buffering delle scritture; revert nascosto nei progetti non-git; scroll-to-line NON supportato (la preview attuale è il diff del right panel) — una location senza modifiche git è un no-op per il following.

---

### Task 1: TillerACP — `.editSummary` nel reducer

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptItem.swift`
- Modify: `Packages/TillerACP/Sources/TillerACP/TranscriptReducer.swift`
- Modify: `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift` (solo `kindLabel`)
- Test: `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerEditSummaryTests.swift` (nuovo)

**Interfaces:**
- Produces: `TranscriptItem.editSummary(id: String, paths: [String])`; il reducer emette l'item subito PRIMA del `turnDivider` in `turnEnded` quando nel turno è stato completato almeno un tool call `kind == .edit`. I path sono quelli grezzi dell'agente (tipicamente assoluti), deduplicati preservando l'ordine.

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `Packages/TillerACP/Tests/TillerACPTests/TranscriptReducerEditSummaryTests.swift`:

```swift
import Foundation
import Testing
@testable import TillerACP

@Suite struct TranscriptReducerEditSummaryTests {
    private func editCall(_ id: String, path: String,
                          status: ToolCallStatus = .completed) -> SessionUpdate {
        .toolCall(ToolCall(
            toolCallId: id, title: "Edit \(path)", kind: .edit, status: .pending,
            locations: [ToolCallLocation(path: path, line: nil)]))
    }

    @Test func completedEditEmitsSummaryAtTurnEnd() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("fix it")])
        reducer.apply(editCall("t1", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        let summaries = reducer.items.compactMap { item -> [String]? in
            if case .editSummary(_, let paths) = item { return paths }
            return nil
        }
        #expect(summaries == [["/repo/a.swift"]])
        // Emesso prima del divider di fine turno.
        if case .turnDivider = reducer.items.last {} else {
            Issue.record("expected turnDivider as last item")
        }
    }

    @Test func duplicatePathsAreDeduplicatedPreservingOrder() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("go")])
        reducer.apply(editCall("t1", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.apply(editCall("t2", path: "/repo/b.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t2",
                                                     status: .completed)))
        reducer.apply(editCall("t3", path: "/repo/a.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t3",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/a.swift", "/repo/b.swift"])
    }

    @Test func nonEditOrFailedCallsEmitNoSummary() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("read")])
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "r1", title: "Read", kind: .read, status: .pending,
            locations: [ToolCallLocation(path: "/repo/a.swift", line: nil)])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "r1",
                                                     status: .completed)))
        reducer.apply(editCall("e1", path: "/repo/b.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "e1",
                                                     status: .failed)))
        reducer.turnEnded(.endTurn)
        #expect(!reducer.items.contains {
            if case .editSummary = $0 { return true } else { return false }
        })
    }

    @Test func diffContentPathCountsAsEditedPath() {
        var reducer = TranscriptReducer()
        reducer.userPrompted([.text("go")])
        reducer.apply(.toolCall(ToolCall(
            toolCallId: "t1", title: "Edit", kind: .edit, status: .pending,
            content: [.diff(path: "/repo/c.swift", oldText: "a", newText: "b")])))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/c.swift"])
    }

    @Test func newPromptResetsAccumulatedPaths() {
        var reducer = TranscriptReducer()
        // Replay di session/load: edit completati arrivano SENZA turnEnded.
        reducer.apply(editCall("old", path: "/repo/stale.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "old",
                                                     status: .completed)))
        // Il turno live che parte dopo il replay non deve ereditarli.
        reducer.userPrompted([.text("new turn")])
        reducer.apply(editCall("t1", path: "/repo/fresh.swift"))
        reducer.apply(.toolCallUpdate(ToolCallUpdate(toolCallId: "t1",
                                                     status: .completed)))
        reducer.turnEnded(.endTurn)
        guard case .editSummary(_, let paths)? = reducer.items.first(where: {
            if case .editSummary = $0 { return true } else { return false }
        }) else { Issue.record("missing editSummary"); return }
        #expect(paths == ["/repo/fresh.swift"])
    }

    @Test func editSummaryRoundTripsThroughCodable() throws {
        let item = TranscriptItem.editSummary(id: "s-1",
                                              paths: ["/repo/a.swift"])
        let data = try JSONEncoder().encode(item)
        let decoded = try JSONDecoder().decode(TranscriptItem.self, from: data)
        #expect(decoded == item)
        #expect(item.id == "s-1")
    }
}
```

- [ ] **Step 2: Verificare che falliscano**

Run: `cd Packages/TillerACP && swift test --filter TranscriptReducerEditSummaryTests`
Expected: FAIL di compilazione (`editSummary` non esiste).

- [ ] **Step 3: Implementazione**

In `TranscriptItem.swift`, aggiungere il case all'enum `TranscriptItem` e il ramo in `id`:

```swift
    case turnDivider(id: String, at: Date)
    /// Riepilogo di fine turno dei file modificati dall'agente (kind .edit
    /// completati); sintetizzato dal reducer, non arriva dal protocollo.
    case editSummary(id: String, paths: [String])
```

```swift
        case .turnDivider(let id, _): id
        case .editSummary(let id, _): id
```

In `ChatSessionStore.swift`, estendere `kindLabel`:

```swift
        case .editSummary: "editSummary"
```

In `TranscriptReducer.swift`:

1. Nuovo stato privato accanto a `nextOrdinal`:

```swift
    /// Path (grezzi, come inviati dall'agente) dei tool call kind == .edit
    /// completati nel turno corrente; svuotato a inizio e fine turno.
    private var turnEditPaths: [String] = []
```

2. In `userPrompted`, come prima riga:

```swift
        turnEditPaths = []
```

3. In `turnEnded`, subito PRIMA del blocco `if !items.isEmpty { items.append(.turnDivider(...)) }`:

```swift
        if !turnEditPaths.isEmpty {
            items.append(.editSummary(id: makeId("edits"), paths: turnEditPaths))
            turnEditPaths = []
        }
```

4. Helper privato:

```swift
    private mutating func recordEditPaths(of item: ToolCallItem) {
        guard item.kind == .edit, item.status == .completed else { return }
        var paths = item.locations.map(\.path)
        for content in item.content {
            if case .diff(let path, _, _) = content { paths.append(path) }
        }
        for path in paths where !turnEditPaths.contains(path) {
            turnEditPaths.append(path)
        }
    }
```

5. Chiamare l'helper nei due punti dove un tool call cambia stato: in `upsert`, dopo `items[index] = .toolCall(merged)` → `recordEditPaths(of: merged)`, e dopo `items.append(.toolCall(item))` → `recordEditPaths(of: item)`; in `applyToolCallUpdate`, dopo `items[index] = .toolCall(item)` → `recordEditPaths(of: item)`, e nel ramo materialize dopo `items.append(.toolCall(item))` → `recordEditPaths(of: item)`. La guardia su `.completed` dentro l'helper rende idempotenti le chiamate sui call ancora in corso; il dedupe dentro l'helper copre update ripetuti sullo stesso call.

- [ ] **Step 4: Verificare che passino tutti**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS (tutte le suite, incluse le esistenti — nessuna regressione su `TranscriptReducerToolTests`).

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: emit end-of-turn edit summary item from transcript reducer"
```

---

### Task 2: TillerACP — estrazione locations per il following

**Files:**
- Modify: `Packages/TillerACP/Sources/TillerACP/SessionUpdate.swift`
- Test: `Packages/TillerACP/Tests/TillerACPTests/SessionUpdateTests.swift` (append)

**Interfaces:**
- Produces: `SessionUpdate.toolCallLocations: [ToolCallLocation]` — le locations trasportate da un update (`tool_call` → `call.locations`, `tool_call_update` → `update.locations ?? []`, ogni altro case → `[]`). Il ChatController (Task 3) la usa per il following.

- [ ] **Step 1: Test che fallisce**

Append in `SessionUpdateTests.swift` (dentro la suite esistente):

```swift
    @Test func toolCallLocationsExtractsFromCallAndUpdate() {
        let call = SessionUpdate.toolCall(ToolCall(
            toolCallId: "t1", title: "Edit", kind: .edit, status: .inProgress,
            locations: [ToolCallLocation(path: "/repo/a.swift", line: 3)]))
        #expect(call.toolCallLocations.map(\.path) == ["/repo/a.swift"])

        let update = SessionUpdate.toolCallUpdate(ToolCallUpdate(
            toolCallId: "t1",
            locations: [ToolCallLocation(path: "/repo/b.swift", line: nil)]))
        #expect(update.toolCallLocations.map(\.path) == ["/repo/b.swift"])

        let bare = SessionUpdate.toolCallUpdate(ToolCallUpdate(toolCallId: "t1"))
        #expect(bare.toolCallLocations.isEmpty)

        #expect(SessionUpdate.agentMessageChunk(.text("hi")).toolCallLocations.isEmpty)
    }
```

- [ ] **Step 2: Verificare che fallisca**

Run: `cd Packages/TillerACP && swift test --filter toolCallLocationsExtractsFromCallAndUpdate`
Expected: FAIL di compilazione (`toolCallLocations` non esiste).

- [ ] **Step 3: Implementazione**

In fondo a `SessionUpdate.swift`:

```swift
public extension SessionUpdate {
    /// Locations trasportate dall'update; usate dal following per aprire il
    /// file toccato dall'agente nel right panel.
    var toolCallLocations: [ToolCallLocation] {
        switch self {
        case .toolCall(let call): call.locations
        case .toolCallUpdate(let update): update.locations ?? []
        default: []
        }
    }
}
```

- [ ] **Step 4: Verificare che passi**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerACP
git commit -m "feat: expose tool call locations from session updates"
```

---

### Task 3: App — following (toggle header, bridge AppModel, apertura diff)

**Files:**
- Modify: `App/Chat/ChatController.swift`
- Modify: `App/Chat/ChatPaneView.swift`
- Modify: `App/AppModel.swift`
- Modify: `App/ContentView.swift`

**Interfaces:**
- Consumes: `SessionUpdate.toolCallLocations` (Task 2).
- Produces: `ChatController.isFollowing: Bool` e `ChatController.onFollowLocation: ((String) -> Void)?`; `AppModel.ChatFollowRequest {path, worktreeId, ordinal}`, `AppModel.chatFollowRequest: ChatFollowRequest?`, `AppModel.requestChatFollow(path:worktreeId:)`. Il Task 4 riusa `requestChatFollow` per il click sui file della card.

- [ ] **Step 1: ChatController — stato following + throttle**

In `ChatController.swift`, accanto a `var onStatusChange`:

```swift
    /// Following ("Segui l'agente"): default off, non persistito.
    var isFollowing = false
    var onFollowLocation: ((String) -> Void)?
    @ObservationIgnored private var lastFollowAt = Date.distantPast
    private static let followThrottle: TimeInterval = 0.5
```

In `handle(_ event:)`, nel case `.update(let update)` dopo `reducer.apply(update)`:

```swift
            if isFollowing, let location = update.toolCallLocations.last,
               Date().timeIntervalSince(lastFollowAt) >= Self.followThrottle {
                lastFollowAt = Date()
                onFollowLocation?(location.path)
            }
```

- [ ] **Step 2: AppModel — richiesta di follow observable**

In `App/AppModel.swift`, dentro la classe `AppModel` (vicino a `gitProjectIds`, riga ~202):

```swift
    /// Richiesta della chat di mostrare un file nel right panel (following /
    /// click su un file della card riepilogo). ContentView la osserva.
    struct ChatFollowRequest: Equatable {
        let path: String
        let worktreeId: UUID
        let ordinal: Int
    }
    var chatFollowRequest: ChatFollowRequest?
    private var chatFollowOrdinal = 0

    func requestChatFollow(path: String, worktreeId: UUID) {
        chatFollowOrdinal += 1
        chatFollowRequest = ChatFollowRequest(
            path: path, worktreeId: worktreeId, ordinal: chatFollowOrdinal)
    }
```

(`ordinal` distingue richieste ripetute sullo stesso path, altrimenti `onChange` non scatterebbe.)

- [ ] **Step 3: ContentView — reagire alla richiesta**

In `App/ContentView.swift`, dopo il blocco `.task(id: rightPanelContext) { ... }` e prima di `.onDisappear { rightPanelModel.deactivate() }`:

```swift
        .onChange(of: model.chatFollowRequest) {
            guard let request = model.chatFollowRequest,
                  let worktree = model.selectedWorktree,
                  worktree.id == request.worktreeId else { return }
            rightPanelVisible = true
            let root = URL(fileURLWithPath: worktree.path).standardizedFileURL.path
            let relative = request.path.hasPrefix(root + "/")
                ? String(request.path.dropFirst(root.count + 1))
                : request.path
            guard let entry = rightPanelModel.statusByPath[relative] else { return }
            rightPanelModeRaw = RightPanelMode.diff.rawValue
            Task { await rightPanelModel.selectDiff(entry) }
        }
```

Nota: se il panel era chiuso, `rightPanelVisible = true` fa ripartire `.task(id: rightPanelContext)` che attiva il modello; `statusByPath` può essere vuoto alla prima richiesta — accettabile, la richiesta successiva (throttle 500ms) aggancia. Nessun retry.

- [ ] **Step 4: ChatPaneView — toggle nell'header + wiring callback**

In `ChatPaneView.swift`, nell'`header` prima del bottone "Nuova conversazione":

```swift
            Button {
                controller.isFollowing.toggle()
            } label: {
                Label("Segui l'agente",
                      systemImage: controller.isFollowing ? "eye.fill" : "eye")
                    .font(.caption)
            }
            .buttonStyle(.borderless)
            .foregroundStyle(controller.isFollowing ? Color.accentColor : .secondary)
            .help("Apre nel pannello di destra i file che l'agente sta modificando")
```

Nel `body`, sostituire `.task { await controller.start() }` con:

```swift
        .task {
            controller.onFollowLocation = { [weak appModel] path in
                appModel?.requestChatFollow(path: path, worktreeId: worktree.id)
            }
            await controller.start()
        }
```

(`AppModel` è una classe `@MainActor @Observable`: la capture list `[weak appModel]` evita un ciclo se il controller sopravvive al pane.)

- [ ] **Step 5: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet; echo exit=$?`
Expected: `exit=0`.

- [ ] **Step 6: Commit**

```bash
git add App
git commit -m "feat: follow agent edits into right panel diff with header toggle"
```

---

### Task 4: App — card riepilogo modifiche con revert

**Files:**
- Create: `App/Chat/EditSummaryCardView.swift`
- Modify: `App/Chat/TranscriptView.swift`
- Modify: `Tiller.xcodeproj/project.pbxproj` (via `xcodegen generate`)

**Interfaces:**
- Consumes: `TranscriptItem.editSummary(id:paths:)` (Task 1), `AppModel.requestChatFollow(path:worktreeId:)` (Task 3), `GitStatus.load(in:)` / `GitActions.discardChanges` / `GitActions.discardUntracked` (TillerGit, esistenti), `appModel.isGitProject(id:)` (esistente).

- [ ] **Step 1: Creare la view**

`App/Chat/EditSummaryCardView.swift`:

```swift
import SwiftUI
import TillerACP
import TillerCore
import TillerGit

/// Card di fine turno "N file modificati": ogni riga apre il diff nel right
/// panel; "Ripristina" scarta le modifiche del file via git (con conferma).
/// Visibilità post-hoc + undo — il permission gate resta la difesa preventiva.
struct EditSummaryCardView: View {
    let paths: [String]
    let worktree: Worktree
    let appModel: AppModel

    @State private var confirmingPath: String?
    @State private var revertError: String?
    @State private var revertedPaths: Set<String> = []
    @State private var revertInProgress = false

    private var isGitProject: Bool {
        appModel.isGitProject(id: worktree.projectId)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label(paths.count == 1 ? "1 file modificato"
                                   : "\(paths.count) file modificati",
                  systemImage: "pencil.line")
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            ForEach(paths, id: \.self) { path in
                row(path)
            }
            if let revertError {
                Text(revertError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.quaternary.opacity(0.4), in: RoundedRectangle(cornerRadius: 8))
        .confirmationDialog(
            "Ripristinare \((confirmingPath as NSString?)?.lastPathComponent ?? "")?",
            isPresented: Binding(
                get: { confirmingPath != nil },
                set: { if !$0 { confirmingPath = nil } })) {
            Button("Ripristina", role: .destructive) {
                if let path = confirmingPath { revert(path) }
                confirmingPath = nil
            }
            Button("Annulla", role: .cancel) { confirmingPath = nil }
        } message: {
            Text("Le modifiche del file andranno perse (git restore / clean).")
        }
    }

    private func row(_ path: String) -> some View {
        HStack(spacing: 8) {
            Button {
                appModel.requestChatFollow(path: path, worktreeId: worktree.id)
            } label: {
                Label((path as NSString).lastPathComponent,
                      systemImage: "arrow.up.forward.square")
                    .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tint)
            Spacer()
            if revertedPaths.contains(path) {
                Text("ripristinato")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else if isGitProject {
                Button("Ripristina") { confirmingPath = path }
                    .controlSize(.small)
                    .disabled(revertInProgress)
            }
        }
    }

    private func revert(_ path: String) {
        revertInProgress = true
        revertError = nil
        Task {
            defer { revertInProgress = false }
            let root = URL(fileURLWithPath: worktree.path).standardizedFileURL.path
            let relative = path.hasPrefix(root + "/")
                ? String(path.dropFirst(root.count + 1))
                : path
            do {
                let status = try await GitStatus.load(in: worktree.path)
                guard let entry = status.entries.first(where: {
                    $0.path.value == relative
                }) else {
                    revertError = "Nessuna modifica da ripristinare per \(relative)."
                    return
                }
                if entry.isUntracked {
                    try await GitActions.discardUntracked([entry], in: worktree.path)
                } else {
                    try await GitActions.discardChanges([entry], in: worktree.path)
                }
                revertedPaths.insert(path)
            } catch {
                revertError = error.localizedDescription
            }
        }
    }
}
```

- [ ] **Step 2: Renderizzare il case in TranscriptView**

In `TranscriptView.swift`, in `itemView(_:)` aggiungere il ramo:

```swift
        case .editSummary(_, let paths):
            EditSummaryCardView(paths: paths, worktree: worktree,
                                appModel: appModel)
```

- [ ] **Step 3: Rigenerare il progetto e buildare**

Run:
```bash
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData build -quiet; echo exit=$?
```
Expected: `exit=0`.

- [ ] **Step 4: Test di regressione package**

Run: `cd Packages/TillerACP && swift test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add App/Chat/EditSummaryCardView.swift App/Chat/TranscriptView.swift Tiller.xcodeproj/project.pbxproj
git commit -m "feat: render end-of-turn edit summary card with per-file git revert"
```

---

### Task 5: Gate finale (NON delegato)

- [ ] `Scripts/ci.sh` → deve stampare `CI OK` (retry sul test PTY flaky se serve).
- [ ] Rebuild + rilancio: `pkill -x Tiller; open DerivedData/Build/Products/Debug/Tiller.app`.
- [ ] Checklist manuale: (a) toggle occhio ON → chiedere a Claude un edit → il right panel si apre sul diff del file; (b) a fine turno appare "N file modificati"; click su un file apre il diff; (c) "Ripristina" con conferma annulla la modifica e la riga diventa "ripristinato"; (d) progetto non-git → niente bottone Ripristina.
