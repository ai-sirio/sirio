# Audit bidirezionale dell'inventario (FABLE-12)

Due domande, due direzioni. **Direzione 1**: ogni riga d'inventario ha ancora un referente
nel sorgente macOS? (Righe senza referente ⇒ il denominatore 389 è gonfio.) **Direzione 2**:
ogni file sorgente macOS ha almeno una riga che lo copre? (Referenti senza riga ⇒ il
denominatore è bucato: 389 righe tutte verdi NON sarebbero parità.)

Riferimento: `/home/enzopalmisano/Scrivania/Progetti/tiller` (App/ + Packages/), letto oggi.
Nessun verdetto cambiato qui; `INVENTORY-LEDGER.md` non toccato. Le righe proposte in §Direzione 2
NON sono state aggiunte all'inventario: aggiungerle cambia il denominatore, decisione dell'utente.

## Metodo e controlli

- Popolazione Direzione 1: **149 righe** = 127 FAILED-absent + 22 N/A-platform (queste ultime in
  senso inverso: fattibilità su Linux). Fonte claim: `01-inventory-app.md` + `02-inventory-packages.md`
  (colonna SRC), incrociate col testo ledger.
- Sonda: risoluzione SRC (path diretto, altrimenti mappa basename→path sui sorgenti non-test,
  disambiguazione multi-candidato con le parole `Tiller*` della claim), poi punteggio di
  sovrapposizione token claim↔file; ogni punteggio basso è stato verificato a mano con una
  **seconda formulazione** (lettura del file, vocabolario alternativo) prima di classificare.
  "Un grep negativo non è una prova, è un indizio" — ed è stato decisivo due volte (PRJ-03, SID-16/17).
- Controlli: positivi F-TAB-25 (trovato: `SidebarView.swift:717`), F-SID-16 (4/4 token),
  F-SID-17 (5/5); negativo fabbricato F-FAKE-99 → correttamente NO-INV-ROW. I positivi
  provano che il rilevatore è vivo, non che il suo vocabolario sia completo — da qui la regola
  delle due formulazioni.
- Direzione 2: censimento meccanico — file di produzione Swift (esclusi Tests) **meno** file
  citati da almeno un SRC risolto. "Cerca file, non feature."

## Direzione 1 — righe senza referente

**Risultato: zero.** Tutte le 149 righe citano file che esistono oggi e claim che il sorgente
sostiene. Un solo referente sbagliato (file citato errato, feature reale). Dettaglio dei casi
che hanno richiesto la seconda formulazione:

| Riga | Claim | Prima formulazione (fallita) | Seconda formulazione | Classificazione |
|---|---|---|---|---|
| F-PRJ-03 | prompt Initialize-git / Add-without-git / Cancel su cartella non-git | `grep 'Initialize\|Without Git'` su AddProjectSheet.swift: **0 hit** | lettura del file: l'alert esiste in `browseFolder()` (~:110) con etichette **in italiano** — `"Inizializza git"` / `"Aggiungi senza git"` / `"Annulla"`, rami su `initGitAndAddProject` | **confermata** |
| F-TAB-14 | rename di un tab | `grep -i 'ename'` su `PaneTabStripBar.swift` (SRC citato): **0 hit** | 'Rename' vive in `TabBarView.swift` (+ SidebarView, AppModel): la barra tab del workspace, non la strip per-pane citata | **referente diverso** — feature reale, SRC da correggere in TabBarView.swift |
| F-SID-16 | drag-reorder progetti nella sidebar | vocabolario ortodosso (`onMove`, `.draggable`, `dropDestination`): scarso | `App/RowReorder.swift`: macchinario completo — `UTType(exportedAs: "it.tiller.row-drag")`, `ReorderScope.projects/.worktrees/.tabs`, `ReorderableRow` con `.onDrag`/`.onDrop`; **cablato** a `SidebarView.swift:50` (projects) | **confermata** |
| F-SID-17 | drag-reorder worktree | come sopra | stesso macchinario, cablato a `SidebarView.swift:55` (worktrees) + `AppModel.swift:1618/:1632/:1645` (previewDrag/endRowDrag) | **confermata** |

⚠ **Contraddice il ruling dell'orchestratore del 2026-08-13** ("L'app macOS non ha il
drag-reorder della sidebar → righe d'inventario sbagliate → N/A, non nel riferimento").
La ricerca che ha fondato quel ruling usava il vocabolario SwiftUI standard; l'implementazione
usa un UTType custom con `.onDrag`/`.onDrop`. Se il cambio di verdetto verso "N/A — non nel
riferimento" arriva a pireview così com'è, il ledger diventa **sbagliato**: il riferimento CE
L'HA. Da ritirare. (Collaterale: lo stesso `ReorderScope.tabs` cablato a `SidebarView.swift:600`
e `TabBarView.swift:221` fonda anche F-TAB-18.)

Nota su PRJ-03: `initializeGitRepository` (`AppModel.swift:1187`) raggiunta dal menu contestuale
(`SidebarView.swift:288`) è una **seconda** superficie — quella è F-SID-08; il prompt dell'add-flow
è distinto e chiama `initGitAndAddProject`. Le due righe non si sovrappongono.

Spot-check confermati senza storia (campione): EmptyWorktreeView ⌘T, PaneEmptyStateView
"New Terminal", BrowserPaneView errorMessage, PlanCardView option buttons, ActivitySectionView
runningCount, ChangedFileRow `.draggable`, WindowChromeConfigurator titlebarDoubleClick,
`.alert("Clone failed")` + `startClone()`/`cloneProject` (PRJ-07) e `CreateNewProjectView`
(PRJ-08/09/10) in AddProjectSheet.swift.

### Le 22 N/A in senso inverso — fattibilità su Linux

| Classe | Righe | Perché |
|---|---|---|
| **Fattibili ora — famiglia browser (10)** | F-AUTO-09, F-CTRL-BROWSER-01..06, F-PER-08, F-SET-24, F-TAB-06 | il ruling utente 2026-08-13 su F-WIN-06 mette il browser IN scope ("niente webview" vieta la shell Electron, non un motore dietro la surface). L'N/A di queste dieci era assorbimento dello stesso assunto caduto. Rientrano nel costruibile (dipendenza: spike P72). |
| **Fattibili con adattamento (7)** | F-CORE-AUTH-03 (Keychain → Secret Service/libsecret), F-USE-04/05 (menu-bar roster → StatusNotifierItem/tray o surface in-app), F-WIN-09 (GNOME `action-double-click-titlebar`), F-WIN-08 (hide-on-close → minimize/tray), F-CORE-FILE-03A (NSItemProvider → `text/uri-list`, che è multi-file ordinato per costruzione), F-WIN-11 (Sparkle → meccanismo update Linux, decisione di prodotto) | il meccanismo macOS è Apple-only ma l'equivalente Linux esiste; "archiviata per pigrizia" è la descrizione giusta per FILE-03A e WIN-09 almeno. |
| **Genuinamente platform (5)** | F-CORE-PLAT-01 (manifest Package.swift macos-15), F-CORE-SET-02, F-SET-23, F-SET-25, F-WIN-12 (tutte TCC: modello permessi, pagina, refresh-on-activate, onboarding) | TCC non ha equivalente Linux; i portal xdg sono un altro modello. Restano N/A. |

**Se pireview concorda: 17 delle 22 N/A rientrano nel costruibile** (target costruzione
102 → 119 righe), il denominatore non si muove (erano già righe).

## Direzione 2 — referenti senza riga

Censimento: **380 file di produzione**, di cui **216 non citati da alcuna riga (24.436 righe
di sorgente)**. Non tutti richiedono una riga — molti sono helper semanticamente coperti o
infrastruttura — ma i buchi veri sono grossi, e il più grosso è un intero package.

### Il buco più caro: TillerACP — 7.016 righe, zero righe d'inventario

`grep -c 'F-ACP' INVENTORY-LEDGER.md` → **0**. `grep -c 'TillerACP' 01-… 02-…` → **0**.
Il package che fa parlare l'app con gli agenti non è MAI entrato nell'inventario.
(12.882 righe col test suite; 7.016 di Sources, di cui 3.247 in `Drivers/`.)

Cosa contiene e cosa c'è dal lato Rust:

- **Cinque driver dedicati** (`Drivers/`): PiRPCDriver 755 (+PiWire 72), ClaudeStreamJSONDriver
  666 (+ClaudeWire 401), OpenCodeHTTPDriver 530 (+OpenCodeConnection 271), CodexAppServerDriver
  490; selezione via AgentDriverFactory 262 (con la policy a commento `AgentInstaller.swift:140`:
  il `codex-acp` di @openai/codex "must never be preferred"). Lato Rust `tiller_acp/src/` contiene
  **solo `chat.rs` + `lib.rs`**: nessun driver dedicato — Pi e OpenCode di là hanno un driver,
  di qua niente.
- **Installer + registry con versioni pinnate**: AgentRegistryClient scarica
  `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json` (`AgentRegistryClient.swift:8-9`)
  e lo cachea; AgentInstaller installa `agent.id@agent.version` **una volta** (`:104`, `:125`) e
  poi esegue il binario locale in `node_modules/.bin` (AgentInstallStore, AgentLaunchSpec,
  `resolveBinName`). I pin correnti (claude-agent-acp@0.60.0, codex-acp@1.1.5) sono nei fixture
  dei test del package. Lato Rust: `npx -y @agentclientprotocol/claude-agent-acp@latest`
  (`tiller_agents/src/claude.rs:128`) — **non pinnato, rete a ogni lancio dell'agente**, e
  duplicato: misurati **8 occorrenze di 'claude-agent-acp' in 7 file su 4 crate** (tiller_agents,
  tiller, tiller_ui, tiller_acp) e 6 di 'codex-acp' in 5 file; zero esecuzioni da `node_modules`.
  (Il "14 volte" del brief non riproduce con nessun letterale singolo; l'ordine di grandezza e la
  sostanza — scorciatoia duplicata al posto di un sottosistema — sì.)
- **Trasporto e modello**: ACPClient/ACPSession/ProcessTransport, JSONRPC/JSONValue/ACPTypes/
  SessionUpdate, McpConfig; TranscriptReducer/TranscriptItem/ToolCall (con l'estensione terminale
  `_meta` di claude-agent-acp), ChatPromptBuilder/FileMentionIndex/FileSystemMessages/DiffStats,
  ChatSessionStore/ChatQuestion/Timeline.

### Righe proposte ("referenti senza riga") — NON aggiunte al ledger

**Blocco A — TillerACP, copertura zero, 14 righe:**

| Proposta | File (righe) | Cosa fa |
|---|---|---|
| F-ACP-01 | ACPClient 113, ACPSession 227, ProcessTransport 113 | ciclo di vita client/sessione ACP su processo figlio |
| F-ACP-02 | JSONRPC 90, JSONValue 85, ACPTypes 285, SessionUpdate 113 | framing JSON-RPC e tipi del protocollo |
| F-ACP-03 | McpConfig 90 | passthrough config MCP all'agente |
| F-ACP-04 | AgentDriverFactory 262 | selezione driver per agente, policy anti-`codex-acp`-di-openai |
| F-ACP-05 | ClaudeStreamJSONDriver 666, ClaudeWire 401 | driver Claude stream-JSON |
| F-ACP-06 | PiRPCDriver 755, PiWire 72 | driver Pi RPC dedicato |
| F-ACP-07 | OpenCodeHTTPDriver 530, OpenCodeConnection 271 | driver OpenCode HTTP |
| F-ACP-08 | CodexAppServerDriver 490 | driver Codex app-server |
| F-ACP-09 | AgentRegistryClient, AgentRegistryModels | fetch+cache del registry CDN, versioni pinnate |
| F-ACP-10 | AgentInstaller 156, AgentInstallStore 117, AgentLaunchSpec 87 | install-once in node_modules, esecuzione del bin locale |
| F-ACP-11 | TranscriptReducer 270, TranscriptItem 160, ContentBlock | riduzione transcript da SessionUpdate |
| F-ACP-12 | ToolCall 232 | ciclo di vita tool-call + estensione terminale `_meta` |
| F-ACP-13 | ChatPromptBuilder, FileMentionIndex, FileSystemMessages, DiffStats | costruzione prompt con file mention |
| F-ACP-14 | ChatSessionStore 260, ChatQuestion 149, TimelineBuilder 145 | store sessioni chat, domande permesso, timeline |

**Blocco B — motore workspace (App/Workspace 3.316 + TillerWorkspace 2.505 + TillerCore/Workspace 631 ≈ 6.452 righe), 8 righe.**
Caveat: è il sottosistema feature-flagged "universal workspace" (lo stesso di F-AUTO-09); split e
tab generici hanno già righe. Qui manca il motore:

| Proposta | File chiave | Cosa fa |
|---|---|---|
| F-WORK-01 | WorkspaceLayoutEngine 547, WorkspaceIDs 84 | motore layout ad albero |
| F-WORK-02 | WorkspaceCoordinator 794, WorkspaceReconciler 367 | coordinamento e riconciliazione stato↔UI |
| F-WORK-03 | SQLiteWorkspacePersistence 582, WorkspaceCheckpointQueue 176, WorkspaceLayoutPersistence 78, WorkspaceMigrationV15 268 | persistenza layout + migrazione V15 |
| F-WORK-04 | DiffContentAdapter 393, BrowserContentAdapter 354, TerminalContentAdapter 180, DocumentContentAdapter 168, ChatContentAdapter 141, WorkspaceContentAdapter 92 | adapter di contenuto per tipo di pane |
| F-WORK-05 | PaneGroupController 247, WorkspaceSplitController 243, PaneTabStripView 147+Model 77, OverflowCanvas 97 | gruppi pane, split, strip |
| F-WORK-06 | DragSession 90, WorkspaceDragCoordinator 102, WorkspaceDragOverlay 165, DropTargetResolver 79, DragPreviewGeometry 81, DividerTracking 132 | drag&drop di pane e divisori |
| F-WORK-07 | WorkspaceFocusCoordinator 191, SpatialNeighbors 101 | focus e navigazione spaziale |
| F-WORK-08 | WorkspaceAccessibility 170, WorkspaceAnnouncements 131 | accessibilità del workspace |

**Blocco C — strato chat dell'app + singoli, 9 righe** (sovrapposizione semantica parziale con
righe F-CHAT/F-CHG esistenti — che citano le view, non questi modelli):

| Proposta | File (righe) | Cosa fa |
|---|---|---|
| F-CHAT-C1 | ChatController 801 | ciclo di vita del turno, selezione driver, orchestrazione chat |
| F-CHAT-C2 | MarkdownAttributedStringRenderer 254, StreamingAgentTextView 127, AgentMessageSegmenter 156, AgentMarkdownTextView 189, ChatRowChrome 162, ChatPresentationSnapshot 112 | pipeline di render streaming/markdown |
| F-CHAT-C3 | ChatTextEditor 198, ComposerDocument 134, ComposerChip 127, ComposerLayout 104 | modello documento del composer (chip, mention) |
| F-PANEL-C1 | RightPanelModel 546 | modello del pannello destro (le righe F-CHG citano le view) |
| F-AGENT-C1 | AcpAgentCenter 230 | orchestrazione centrale degli agenti ACP nell'app |
| F-AGENT-C2 | AgentAccountStore 316 | account/credenziali per agente |
| F-BROW-C1 | BrowserSurface 439, SnapshotBuilder 74, BrowserAutomation 73, BrowserError 72, BrowserSurfaceBudget 116 | motore della browser surface (in scope per ruling WIN-06) |
| F-CODE-C1 | TreeSitterHighlighter 103, CodeDocument 94, DiffHighlighter 118, EditorTheme+Tiller 110 | highlighting tree-sitter dell'editor/diff |
| F-USE-C1 | OpenCodeGoUsageFetcher 77 | provider usage opencode-go (le F-USE citano gli altri provider) |

**Totale proposto: 31 righe** — 14 a copertura zero assoluta (Blocco A), 17 con giudizio di
sovrapposizione parziale (Blocchi B–C). Se la parità richiede N righe in più, **N = 31**.

**Semanticamente coperti, nessuna riga necessaria** (campione motivato): AppTheme/AppFont/AgentIcon
(righe theme/icone esistenti), CmuxCommands 396 (le F-CTRL-* coprono i metodi; la CLI è B-30),
AutoNamer (F-AUTO), PersistenceCoordinator (F-PER), AgentNotifier (F-CORE-ACT-19/20),
ProcessScanCoordinator (F-CORE-ACT-09/10), SystemPermissionProbe/UpdaterModel (N/A platform
SET-23/WIN-11), Scripts/render-icon (build). Il resto dei 216 sono helper sotto ~100 righe
dentro sottosistemi già proposti.

## Aritmetica del denominatore

- **In giù: 0.** Nessuna delle 149 righe perde il referente. Le uniche due già condannate dal
  ruling (SID-16/17) risorgono: il macchinario esiste ed è cablato. TAB-14 è una correzione di
  citazione, non una rimozione.
- **In su: +31** se il mattino accetta tutte le proposte — 389 → 420. Solo il Blocco A (+14,
  389 → 403) è indiscutibile: copertura zero, sottosistema intero, e il lato Rust ha al suo
  posto una scorciatoia non pinnata duplicata in 4 crate.
- **Target costruzione** (ortogonale al denominatore): +17 N/A riclassificabili — 102 → 119.
