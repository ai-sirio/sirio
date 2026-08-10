# Piano prestazioni e fluidità di Tiller

## Context
Tiller (Swift 6, macOS 15+, SwiftUI/AppKit e libghostty) deve risultare più fluido e reattivo durante l’uso simultaneo di più worktree, tab, pane terminale e agenti, senza cambiare il comportamento funzionale. Il piano parte da hotspot confermati nel codice e usa misure Release ripetibili invece di micro-ottimizzazioni speculative. L’esito atteso è una UI regolare sotto carico, con meno lavoro ridondante sul main actor e nessuna regressione nella persistenza o nel rilevamento attività.

## Approach

### 1. Preparare l’harness, catturare la baseline e fissare i gate

- Estendere `Scripts/bench-workspace.sh` invece di introdurre un secondo runner: aggiungere `--self-test`, `--scenario workspace|terminal|resources|all`, `--samples`, `--output` e pairing begin/end per signpost ID. `--self-test` deve terminare prima di build, `defaults` o process launch; usa log sintetici con intervalli sovrapposti, outcome superseded/cancelled, begin senza end, driver unsupported e sample insufficienti. Solo il self-test viene aggiunto a `Scripts/ci.sh`, mai le soglie hardware.
- Il runner richiede `BENCH_DEDICATED=1` e `BENCH_WORKTREES` con dieci ID già esistenti. Prima di rilanciare Tiller valida gli ID con `tillerctl list-workspaces --json` e rifiuta il run se `panel list --json` trova un pane preesistente in qualunque worktree, così non termina agenti dell’utente. Poi costruisce Release una volta, abilita i signpost, termina l’eventuale processo senza pane, lancia `DerivedData/Build/Products/Release/Tiller.app` e attende `tillerctl ping`. Crea soltanto i pane di carico (due per worktree) e ne registra gli ID; il `trap` chiude quei pane e ripristina il valore precedente di `dev.tiller.Tiller debug.signpostMetrics`, senza creare/eliminare worktree. Log e trace restano in `--output` (default `DerivedData/Performance/<timestamp>/`) insieme a commit, hardware, macOS, Xcode, refresh display, configurazione e workload. Calcolare p50/p95 con almeno 30 campioni e p99 solo con almeno 100; altrimenti stampare `INSUFFICIENT DATA`.
- Scenario `workspace`: 10 worktree montati, 20 terminali, sidebar e Activity espanse, pannello destro aperto/chiuso, cinque warm-up, 100 switch worktree e 100 switch tab nel gruppo attivo. Scenario `terminal`: quattro pane, ciascuno emette 8 MiB deterministici più marker finale; ripeterlo visibile, nascosto su altro worktree e in Settings. Scenario `resources`: 60 s idle con 12 campioni CPU/RSS, poi cinque snapshot `vmmap` ogni 20 switch dopo warm-up.
- Eseguire Time Profiler, SwiftUI e Core Animation/Metal dopo `xcrun xctrace list templates`; se un template non esiste, fermare il runner con istruzione esplicita per aprire Instruments, senza sostituirlo silenziosamente. Le trace grezze restano fuori git; il report Markdown/JSON resta accanto alle trace.
- Gate: `worktreeSwitchToLayout` p95 ≤ 8 ms; HID-to-present p95 entro un frame e p99 entro due frame al refresh registrato; con output nascosto p95 entro due frame e nessun evento >100 ms; `workspaceReconcile` p95 ≤8 ms e zero reconcile su revisioni invariate; zero draw/display link delle surface nascoste dopo settle; throughput byte-perfect con marker finale 4/4 e CPU/MiB ridotta almeno del 15%; write-to-echo control p95 non peggiora oltre il 10%; idle CPU medio <1%; footprint finale entro il 10% del post-warm-up e senza crescita monotona.
- Eseguire `terminal` e `resources` sul checkout iniziale prima di qualsiasi modifica sorgente. Dopo la fase 2, quando esiste `worktreeSwitchToLayout` ma nessuna ottimizzazione di rendering è ancora applicata, acquisire la baseline `workspace` confrontabile; dopo ogni fase successiva eseguire lo scenario focalizzato e al termine rieseguire `all` sullo stesso dataset senza cambiare hardware, refresh o carico.

| Scenario | Driver | Inizio → fine | Sorgente / evaluator |
|---|---|---|---|
| Worktree switch | `tillerctl select-workspace` alternato A/B | `worktreeSwitchToLayout begin` → end `outcome=completed` | `log show --signpost`, parser per ID |
| Tab/pane focus | `tillerctl panel focus --id <pane>` | invio comando → risposta control; `workspaceReconcile` riportato separatamente | clock monotono runner + signpost parser |
| Terminal throughput | `panel create --cmd <8 MiB payload>` + `panel wait` | primo `ptyIngest` del payload → processo terminato con marker finale | signpost bytes/duration + campioni CPU del PID |
| Terminal write-to-echo | pane dedicato `while read; do printf 'ECHO:%s\n'; done`, 100 `panel write` con nonce | clock prima del write → nonce osservato da `panel read` | clock monotono runner; metrica control, non HID |
| Hidden worktree | `select-workspace` verso altro ID | `workspaceVisibility` begin → `setVisible(false)` completato; poi assenza draw | signpost + Core Animation/Metal trace |
| Settings | azione manuale menu durante pausa dichiarata | route `.workspace` → `.settings` → `workspaceVisibility` completato | signpost + trace; nessuna automazione prodotto |
| Right panel | apertura/chiusura manuale, poi 100 switch automatici | un `rightPanelActivation` per activate → fine initial load | signpost parser + overlap trace |
| HID-to-present | 100 click manuali worktree durante trace | evento HID di sistema → frame presentato | Instruments Hangs/Core Animation; se la tabella non è esportabile: `UNSUPPORTED`, mai PASS |
| Risorse | loop runner | inizio/fine finestra fissa | `ps`, `vmmap`, Allocations |

Ogni scenario senza driver, endpoint o evaluator disponibile produce `UNSUPPORTED` e fa fallire `--scenario all`; non sostituire `layoutSubtreeIfNeeded()` a un frame presentato.

### 2. Rendere misurabile la navigazione completa e unificare la selezione

- Aggiungere `App/Navigation/WorktreeSwitchTracker.swift` come `@MainActor final class` con `begin(worktreeID:) -> UInt64`, `complete(generation:worktreeID:)` e `cancel()`, più un initializer di test `init(onCompletion: @escaping (UInt64, UUID) -> Void)`. `begin` chiude l’eventuale intervallo precedente con outcome `superseded`, incrementa la generazione e apre `worktreeSwitchToLayout`; `cancel` chiude con `cancelled`; `complete` chiude con `completed` e invoca la closure solo quando generazione e worktree coincidono con il pending corrente. Il parser benchmark include soltanto outcome `completed`.
- In `App/AppModel.swift` sostituire il `didSet` con `private(set) var selectedWorktree`, `private(set) var worktreeSelectionGeneration: UInt64 = 0`, un tracker privato, `selectWorktree(_ worktree: Worktree?)` e `completeWorktreePresentation(generation:worktreeID:)`. Per un ID nuovo aprire il tracker e pubblicare la generation prima di assegnare `selectedWorktree` soltanto quando `route == .workspace`; in Settings aggiornare la selezione senza intervallo. `nil`, `openSettings()` e `openAgentsSettings()` cancellano il pending. Un valore con lo stesso ID aggiorna metadata e `selectedProjectId` senza nuova generation. Conservare persistenza, mount ed eviction; rinominare il vecchio intervallo sincrono `worktreeSelectionMutation`.
- Nella stessa fase aggiungere in `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceView.swift` `public struct WorkspaceRenderState: Equatable, Sendable` con campi esatti `layout: WorkspaceLayout`, `revision: Int`, `delta: WorkspaceLayoutDelta?`, `isVisible: Bool` e `presentationGeneration: UInt64?`, più `WorkspaceView.onPresentationComplete: @MainActor (UInt64) -> Void`. Prima della fase 3, `ContentView` passa la generation solo quando route workspace e ID selezionato coincidono; `WorkspaceViewController` completa solo con `state.isVisible`, mantiene reconcile incondizionato, chiama `view.layoutSubtreeIfNeeded()` e deduplica la generation. Questa è instrumentation, non ancora l’ottimizzazione.
- Migrare tutte le assegnazioni trovate dalla regex `selectedWorktree\s*=` sotto `App/` e `AppTests/` a `selectWorktree(_:)`; i callsite di test correnti sono `ActivityPanelTests.swift` e `AppModelControlTests.swift`. Dopo il cutover la ricerca repository-wide deve restituire soltanto l’assegnazione interna a `selectWorktree`; non lasciare setter, alias o percorsi compatibili.
- Con `WorkspaceEngineGate.isEnabled`, `activateTab`, selezione numerata e ciclo tab leggono `layout.group(layout.activeGroupID)?.tabs` e chiamano `workspaceCoordinator.activateTabDirectly`; il percorso legacy conserva store/persist. Nel verb control `panel.focus`, selezionare worktree e attivare direttamente la tab senza suspension point tra le due pubblicazioni; ritornare prima quando la tab è già attiva.
- Aggiungere test Swift Testing in `AppTests/Navigation/WorktreeSwitchTrackerTests.swift` e ampliare `AppTests/Workspace/SidebarEngineParityTests.swift` per: completamento solo della generazione corrente, outcome superseded/cancelled esclusi, riselezione idempotente, aggiornamento metadata a ID invariato e navigazione tab coerente con il motore universale.
- Subito dopo i test della fase, eseguire `--scenario workspace --samples 100` e conservare il report come baseline navigazione “instrumented”; è il riferimento per le fasi 3, 5, 7 e 8, perché il checkout precedente non possiede l’intervallo end-to-end.

### 3. Fermare il rendering nascosto senza fermare runtime, PTY o monitor corretti

- In `Packages/TillerTerminal/Sources/TillerTerminal/TerminalSurfaceHost.swift` rendere `SurfaceVisibility.apply(_:in:)` `@discardableResult` e farle restituire il numero di surface trovate. Sostituire l’attuale `NSHostingController<AnyView>` con un controller privato che conserva `surfacesVisible` e `needsVisibilityApplication`: un cambio flag o `relaunch()` marca dirty; `viewDidLayout()` prova l’applicazione soltanto finché trova almeno una `AppTerminalView`, poi azzera il flag. Esporre `public func setVisible(_ visible: Bool)`; `teardown()`, lettura PTY, scrollback e callback non cambiano.
- In `App/Workspace/TerminalContentAdapter.makeHost` passare `visibility: { [weak surface] visible in surface?.setVisible(visible) }` a `WorkspaceContentHostAdapter`, mantenendo le closure esistenti di focus e release che trattengono il `TerminalSurfaceHost`. L’attuale default no-op non deve più essere usato per host terminali.
- Estendere `WorkspaceMountPlan` con `visibleWorktreeID` e l’inizializzatore esatto `init(openWorktreeIDs:selectedWorktreeID:workspaceVisible:)`: i worktree aperti restano tutti in `mountedWorktreeIDs`, mentre `visibleWorktreeID` vale il selezionato solo quando `workspaceVisible` è `true`. `ContentView` passa `model.route == .workspace` e usa il valore sia nel renderer universale sia nel `TerminalSplitHost` legacy.
- In `ContentView.rightPanelContext` impostare `visible` a `rightPanelVisible && model.route == .workspace`; nel `.task(id:)` sostituire il guard corrente con `guard rightPanelContext.visible else { rightPanelModel.deactivate(); return }` e chiamare `activate` solo dopo quel guard.
- Usare il `WorkspaceRenderState` introdotto nella fase 2 per valorizzare revision/delta/visibility di ogni worktree. Aggiungere l’intervallo `workspaceVisibility` attorno alla propagazione sincrona del cambio visibility, con payload booleano e numero host; in questa fase il reconcile resta incondizionato per mantenere confrontabile la baseline.
- Estendere `SurfaceVisibilityTests`, `TerminalSurfaceHostTests`, `WorkspaceMountTests` e `WorkspaceContentAdapterTests`, e creare `WorkspaceVisibilityTests`: due layout consecutivi dopo l’applicazione non ripercorrono il subtree, passare workspace→Settings→workspace non aumenta il conteggio host né invoca release/teardown, conserva controller e generation ID, propaga `[true, false, true]` una volta per transizione e mantiene esatti marker finale, ordine e byte count dello scrollback prodotto mentre la surface è nascosta.

### 4. Togliere task, hop actor e settle inutili dal percorso PTY

- Convertire `ScrollbackBuffer` da actor a `public final class ScrollbackBuffer: @unchecked Sendable` con un solo `OSAllocatedUnfairLock<State>`; `State` contiene `storage`, `capacity`, `head` e `count`. Mantenere invariati capacità predefinita (256 KiB), ring bulk-copy e semantica suffix; `append(_:)`, `snapshot()` e `tail(_:)` diventano sincroni e tengono il lock per una sola operazione atomica.
- Estendere il holder di bootstrap usato da `PtyRuntime`: il runtime conserva fortemente `private let handle: PtyHandle`, mentre `PtyHandle` espone `weak var pty: PtyProcess?` e `weak var runtime: PtyRuntime?`. Le closure session/PTY catturano il holder; `handle.pty = pty` e `handle.runtime = self` avvengono solo dopo l’inizializzazione di tutti gli stored property, evitando cattura anticipata e cicli forti.
- Introdurre `PtyRuntime.ingest(_ data: Data)`, che esegue in ordine `session.receive(data)`, `scrollback.append(data)` e `outputSettle?.push()`; il callback PTY chiama `handle.runtime?.ingest(data)`. I byte sono quindi nello scrollback quando `ingest` ritorna, senza un `Task` per chunk.
- Cambiare l’inizializzatore interno in `PtyRuntime.init(workingDirectory:command:paneId:initialScrollback:extraEnvironment:onContentSignal:)`, con callback opzionale finale predefinita a `nil`. Conservare `outputSettle` come proprietà internal opzionale per il test target; `PtyTerminalPane` passa il callback durante la costruzione invece di assegnarlo dopo.
- Rendere `OutputSettleDebouncer.cancel()` terminale e sincrono sul suo stato: `cancelled = true`, annullamento del pending e ogni `push()` futuro no-op. Introdurre l’internal protocol `OutputSettleScheduler.schedule(after:_:) -> (@Sendable () -> Void)`, con implementazione GCD che restituisce la closure di cancellazione e scheduler manuale nei test; proteggere pending/cancelled con `OSAllocatedUnfairLock`. `PtyRuntime.stop()` chiama cancel; byte già letti restano consegnati allo scrollback ma non programmano Layer C.
- Rendere lazy il parametro `message` di `SignpostMetrics.endInterval` con `@autoclosure`, così le stringhe `bytes: …` non vengono allocate quando il gate `debug.signpostMetrics` è disabilitato; mantenere nomi, subsystem e payload esistenti.
- Estendere `ScrollbackBufferTests`, `PtyRuntimeContentSignalTests`, `OutputSettleDebouncerTests` e `SignpostMetricsTests`: ingest sincrono immediatamente leggibile, holder/runtime rilasciabili dopo stop, snapshot concorrenti con record atomici completi, callback nil con `outputSettle == nil`, un evento per burst, cancel/push guidati dallo scheduler manuale senza sleep-gate, e messaggio signpost con side effect non valutato quando state/gate sono nil.

### 5. Evitare reconcile e layout invariati

- In `App/Workspace/WorkspaceCoordinator.swift` sostituire `lastSemanticDelta` con `semanticDeltas: [UUID: WorkspaceLayoutDelta]` e `semanticDelta(for worktreeID: UUID) -> WorkspaceLayoutDelta?`; `publish(_:worktreeID:)` incrementa la revisione e registra il delta solo per quel worktree.
- Consumare revisione e delta per-worktree già trasportati da `WorkspaceRenderState`; mantenere invariata la closure `onPresentationComplete` introdotta nella fase 2.
- In `WorkspaceViewController` inizializzare `appliedRevision: Int? = nil` e conservare `lastCompletedPresentationGeneration` anche quando lo state successivo porta `nil`. Esporre soltanto `update(state:onPresentationComplete:)`: applicare prima visibility; consentire il fast path solo dopo il primo reconcile; su generation nuova chiamare `view.layoutSubtreeIfNeeded()` e completare una volta. Rimuovere `update(layout:delta:isVisible:)` senza overload compatibile.
- In `WorkspaceReconciler` memorizzare la visibilità con guardia di uguaglianza e applicarla a ogni `PaneGroupController` esistente e appena creato. In `PaneGroupController.update`, assegnare `stripModel.entries` e `stripModel.isFocusedGroup` solo se il valore cambia; il fast path di host invariato deve comunque applicare la visibilità corrente.
- Ampliare `ReconcilerPerformanceTests` con contatori: il primo state revision 0 esegue un reconcile; i successivi 100 invariati zero; visibility-only zero traversate; revisione nuova uno; generation uguale completa una volta e una stantia non chiude il tracker. Migrare allo state API `WorkspaceView`, `DividerHitTestDiagnosticTests`, `WorkspaceGeometryTests` e `AppTests/Workspace/EmptyPaneClickReproductionTests`.

### 6. Ridurre scansioni processo e invalidazioni activity

- In `App/ForegroundProcessAgent.swift` sostituire `identify` + `processTree` con `scan(shellPid:maxDepth:maxCount:) -> ForegroundProcessScanResult` e l’overload testabile `scan(shellPid:maxDepth:maxCount:childrenOf:nameOf:)`. Il wrapper production passa libproc; l’overload enumera `childrenOf(shellPid)` una volta, memoizza anche i nomi mancanti, identifica solo i figli diretti nell’ordine catalogo, costruisce l’albero con cap 5/50 e racchiude l’intera operazione in `processScan`. `AppModel.makeForegroundProcessScanner` usa solo questa API; rimuovere gli entry point obsoleti.
- In `AgentActivityModel.notify` assegnare `agentStatus[paneId]` solo se il valore cambia, ma aggiornare sempre `lastHookUpdateAt[paneId]`; in `handleTitleChange` restituire `nil` senza scrivere quando lo status riconosciuto è già quello corrente. In `AppModel.notifyTransition` fare return immediato quando `old == new`, prima di auto-rename, policy e notifiche. Non modificare la finestra Layer A/B di 1,5 s né le ownership spawn/title/process.
- Ampliare `ForegroundProcessTreeTests`, `ProcessScanCoordinatorTests` e `AgentActivityModelTests`: una sola enumerazione della root e una sola lettura nome per PID, identità solo direct-child, cap 5/50 invariati, dieci richieste durante una scan bloccata uguali a una scan più un follow-up, notify duplicato che aggiorna il timestamp senza invalidare `agentStatus` e titolo duplicato senza transition.

### 7. Costruire sidebar e Activity in un solo passaggio

- Modificare `AttentionSort.sorted` e `urgentFirst` per decorare ogni elemento una volta con il rango e poi ordinare stabilmente; aggiungere a `AttentionSortTests` l’asserzione che `statusOf` venga invocato esattamente una volta per elemento.
- In `ChatContentAdapter` aggiungere `resolveAgentID: (ChatContentID) -> String?`, una mappa `agentIDs: [WorkspaceTabID: String]`, `agentID(for:)` e `setAgentID(_:for:)`. `.newChat` memorizza l’ID ricevuto; resume lo risolve una sola volta; `AppModel.registerRestoredChatAgents` semina la stessa mappa mentre registra `AgentActivityModel`; close/dispose rimuovono la voce. `AppModel` collega il resolver a `chatStore.session` fuori dal body SwiftUI, preservando l’icona immediata di chat nuove, ripristinate e detached senza query ripetute.
- In `App/Workspace/SidebarTabProjection.swift` aggiungere `WorkspacePresentationIndex` con `workspaceTabs: [WorkspaceTab]`, `sidebarTabs: [LegacyWorkspaceTab]`, `activeTabID: UUID?`, `livePaneIDs: [WorkspaceTabID: UUID]` e `activityPaneIDs: [UUID]`. `build(layout:livePaneID:chatAgentID:)` legge `layout.allTabs` una volta e risolve ogni tab O(1); `buildLegacy(tabs:activeTabID:)` conserva il gate-off usando le `LegacyWorkspaceTab`, `workspaceTabs = []` e gli activity rows oggi assenti. Aggiungere `WorkspaceCoordinator.liveControlPaneId(tabID:)` e `chatAgentID(tabID:)`, delegati agli adapter; mantenere il lookup per content ID solo al control plane.
- Aggiungere `App/Navigation/SidebarPresentation.swift`; tutti i valori sono `Equatable` e quelli iterati sono `Identifiable`. Definire `SidebarProjectPresentation(project:isExpanded:isGitProject:collapsedStatus:worktrees:)`, `SidebarWorktreePresentation(worktree:isSelected:isGitProject:status:agentID:runningAgentIDs:tabs:)`, `SidebarTabPresentation(tab:isActive:isDirty:agentID:panes:)` e `SidebarPanePresentation(id:title:agentID:)`. `SidebarPresentation.build(appModel: AppModel, query: String)` produce un solo indice e aggregato activity per worktree.
- `SidebarView.body` costruisce uno snapshot per passaggio; i row ricevono valori e non eseguono query di presentazione su `AppModel`. Rendere `WorkspaceTabIcon` value-based con un `WorkspaceTabIconKind: Equatable` derivato da `tab` + `agentID`, e migrare sia Sidebar sia `TabBarView`; action/menu possono catturare il modello ma non richiamano proiezioni/status/SQLite durante il body chiuso.
- In `ActivitySectionView.body` calcolare `let rows = ActivityPanelModel.rows(appModel:)` una volta e passarlo a header e content insieme al conteggio running. `ActivityPanelModel` costruisce e riusa un indice per worktree e non chiama `chatSession(id:)` né `liveControlPaneId(contentID:in:)` dentro un loop sulle tab.
- Aggiungere `SidebarPresentationTests` e `AppTests/Workspace/WorkspaceTabIconTests.swift`; ampliare `ChatContentAdapterTests` e `ActivityPanelTests`. Verificare resolver chat singolo, restore senza seconda query, una generation per terminale, un lookup status per worktree, highlight worktree/tab, mapping icone e titoli/dirty state equivalenti.

### 8. Tenere il pannello destro fuori dal percorso critico solo se i dati lo richiedono

- Aggiungere esattamente un intervallo `rightPanelActivation` per `activate`, un distinto `rightPanelInitialLoad` attorno a root+status+stats e `rightPanelPrefetch` per il lookahead, con soli conteggi/booleani e nessun path o nome repository.
- Acquisire 100 switch con pannello aperto e 100 chiuso. Il ramo cache scatta solo se la trace mostra che `rightPanelActivation` sovrappone il percorso pre-layout e contribuisce almeno il 10% del suo p95, oppure se `rightPanelInitialLoad` supera 8 ms p95 durante quella sovrapposizione. Allora introdurre `RightPanelSnapshot` con `isGitRepository`, directory/expanded/status map/diffStats e `RightPanelSnapshotCache(capacity: 3)` LRU per worktree; un hit richiede lo stesso flag git.
- Nel ramo cache aggiungere `storeSnapshotIfStable(token:)`, che inserisce soltanto con generation corrente, `filesLoading == false`, `gitLoading == false` e nessun refresh in-flight: alla fine di `loadInitial`, dopo applicazioni riuscite di directory/espansioni e alla fine di `performRefresh`; mai durante loading. Un hit pubblica subito, azzera errori, marca loading e aggiorna in background; prefetch utility parte dopo root rows + `Task.yield()`. Diff store ed errori non sono cacheati.
- Solo se la soglia scatta, aggiungere test cache a `RightPanelDirectoryStatusTests`/`RightPanelPrefetchTests`: publish immediato, eviction LRU A/B/C→touch A→D elimina B, nessun put parziale/stale e nessuna contaminazione diff/errori. Sotto soglia non creare cache né test relativi: restano signpost, misure e test esistenti.

## Critical files & anchors

- `App/AppModel.swift` — `selectedWorktree`, `workspaceTabs(for:)`, `makeForegroundProcessScanner`, `notifyTransition`: unifica navigazione e impedisce che status/proiezioni duplicate invalidino l’intera UI.
- `App/ContentView.swift` — `rightPanelContext`, `terminalStack`, `workspaceStack`: compone route, mount e visibility senza cambiare il lifetime dei runtime.
- `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift` — `PtyRuntime.init`, nuovo `ingest`, `emitContentSignal`, `stop`: percorso byte PTY → ghostty/scrollback/Layer C.
- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceViewController.swift` — `update`: confine AppKit dove separare visibility, reconcile strutturale, layout e completamento della misura end-to-end.
- `App/SidebarView.swift` — `SidebarView.body`, `ProjectRow`, `WorktreeRow`, `TabRow`, `PaneRow`: principale gerarchia SwiftUI ad alta cardinalità da alimentare con valori immutabili.

## Verification

### Contratti deterministici

Eseguire dalla root del repository, senza variabili d’ambiente:

```bash
swift test --package-path Packages/TillerCore --filter AttentionSortTests
swift test --package-path Packages/TillerCore --filter AgentActivityModelTests
swift test --package-path Packages/TillerCore --filter SignpostMetricsTests

swift test --package-path Packages/TillerTerminal --filter ScrollbackBufferTests
swift test --package-path Packages/TillerTerminal --filter PtyRuntimeContentSignalTests
swift test --package-path Packages/TillerTerminal --filter OutputSettleDebouncerTests
swift test --package-path Packages/TillerTerminal --filter TerminalSurfaceHostTests
swift test --package-path Packages/TillerTerminal --filter SurfaceVisibilityTests

swift test --package-path Packages/TillerWorkspace --filter WorkspaceVisibilityTests
swift test --package-path Packages/TillerWorkspace --filter ReconcilerPerformanceTests
swift test --package-path Packages/TillerWorkspace --filter DividerHitTestDiagnosticTests
swift test --package-path Packages/TillerWorkspace --filter WorkspaceGeometryTests
```

Poi rigenerare il progetto e coprire l’integrazione App:

```bash
xcodegen generate
xcodebuild test -project Tiller.xcodeproj -scheme Tiller \
  -destination 'platform=macOS' \
  -only-testing:TillerTests/WorktreeSwitchTrackerTests \
  -only-testing:TillerTests/AppModelControlTests \
  -only-testing:TillerTests/SidebarPresentationTests \
  -only-testing:TillerTests/SidebarEngineParityTests \
  -only-testing:TillerTests/WorkspaceMountTests \
  -only-testing:TillerTests/WorkspaceContentAdapterTests \
  -only-testing:TillerTests/ChatContentAdapterTests \
  -only-testing:TillerTests/WorkspaceTabIconTests \
  -only-testing:TillerTests/EmptyPaneClickReproductionTests \
  -only-testing:TillerTests/ActivityPanelTests \
  -only-testing:TillerTests/ForegroundProcessTreeTests \
  -only-testing:TillerTests/ProcessScanCoordinatorTests \
  -only-testing:TillerTests/RightPanelDirectoryStatusTests \
  -only-testing:TillerTests/RightPanelPrefetchTests
```

Risultati richiesti: zero failure; stesso ID assegna zero nuove generation; nil/Settings cancellano il tracker; hide/show conserva controller/runtime; `ingest` rende i byte leggibili prima del ritorno; callback nil crea zero settle; il primo state revision 0 riconcilia e i 100 successivi invariati no; ogni terminale e status viene risolto una volta; notify/titolo duplicati non pubblicano uno status nuovo.

### Harness e gate repository

```bash
Scripts/bench-workspace.sh --self-test
Scripts/ci.sh
```

Il self-test deve rifiutare pairing ambiguo e sample insufficienti; `Scripts/ci.sh` deve terminare con `CI OK`. Nessun valore di latenza hardware entra nel CI.

### Prova Release end-to-end

Prerequisito: dieci worktree reali già registrati in Tiller. Ricavarne gli ID con il `tillerctl` Release e impostare esattamente:

```bash
export BENCH_DEDICATED=1
export BENCH_WORKTREES='<id1>,<id2>,<id3>,<id4>,<id5>,<id6>,<id7>,<id8>,<id9>,<id10>'
export WORKSPACE_A='<id1>'
export WORKSPACE_B='<id2>'
Scripts/bench-workspace.sh \
  --scenario all \
  --samples 100 \
  --output DerivedData/Performance/current-head
```

Il runner verifica che non esistano pane preesistenti prima di rilanciare l’app, valida i dieci ID con `list-workspaces --json`, crea e chiude soltanto i pane necessari, aspetta `tillerctl ping` e conserva ogni log/trace nella directory indicata. La fase Settings resta una pausa manuale esplicita: aprire Settings dal menu durante la finestra indicata, senza aggiungere permessi Accessibility o automazione UI al prodotto.

Input terminale concreto per ciascuno dei quattro pane di carico: `printf 'TILLER_START\n'; /usr/bin/yes '0123456789abcdef0123456789abcdef' | /usr/bin/head -c 8388608; printf '\nTILLER_END\n'`. Output atteso: `panel wait` termina, `panel read` contiene `TILLER_END` in 4/4 pane (il marker iniziale può essere espulso dal ring da 256 KiB), i PID/generation ID restano invariati durante visible→hidden→Settings→visible e Instruments mostra zero draw/display link per le surface nascoste dopo settle.

Confrontare `terminal`/`resources` con la baseline del checkout iniziale, `workspace` con la baseline instrumented della fase 2 e l’eventuale cache right-panel con il run acquisito nella fase 8 subito dopo i signpost ma prima della cache. Usare sempre lo stesso Mac, dataset e refresh display; un gate della fase 1 fallito impedisce di dichiarare l’ottimizzazione completata.

## Assumptions & contingencies

- Il motore universale resta attivo per default (`WorkspaceEngineGate` default `true`), ma i test coprono il percorso legacy. Nascondere non equivale mai a smontare: `openWorktreeIds`, registry, PTY, agent detection, scrollback e notifiche restano vivi.
- Il benchmark di accettazione richiede dieci worktree esistenti. Con meno worktree il runner può produrre un report `REDUCED DATASET`, ma deve uscire non-zero per `--scenario all` e non può soddisfare i gate finali.
- Se il passaggio actor→`OSAllocatedUnfairLock` non riduce CPU/MiB di almeno il 15% oppure peggiora il write-to-echo control p95 oltre il 10%, ripristinare soltanto `ScrollbackBuffer` come actor e il relativo `Task` per append; mantenere optional settle, cancel in `stop`, lazy signpost payload e tutti i test di ordine/correttezza.
- La cache del pannello destro viene implementata soltanto con la soglia della fase 8; sotto soglia restano esclusivamente signpost e misure. Nessun implementatore sceglie il ramo a intuito.
- Se `xcrun xctrace list templates` non espone SwiftUI o Core Animation, usare Instruments GUI con View Body, Hangs e Core Animation sullo stesso workload e salvare trace più riepilogo numerico. Se né export né GUI espongono l’evento frame-present richiesto, marcare HID-to-present `UNSUPPORTED`: non sostituire metriche diverse né abbassare i gate.
- Non cambiare schema SQLite, formato transcript, policy di mount, API interne di libghostty o lifecycle del control socket in questa esecuzione: i profili letti non dimostrano che tali cambi siano necessari per i gate scelti.
