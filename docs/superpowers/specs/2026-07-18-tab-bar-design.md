# Tab bar per terminali e chat, sincronizzata con la sidebar — Design

Data: 2026-07-18 · Branch: `feature/chat-interface`

## Obiettivo

Aggiungere una tab bar orizzontale sopra l'area contenuto che mostri le tab
(terminale / chat / markdown) del worktree selezionato, pienamente
sincronizzata con la sidebar. La sidebar guadagna il riordino drag & drop
delle tab dentro lo stesso worktree. Nessun cambiamento al modello dati:
`WorkspaceTab` e `tabs[worktreeId]` / `activeTabId[worktreeId]` in `AppModel`
restano l'unica source of truth.

## Requisiti (decisi in brainstorm)

1. Tab bar nell'area contenuto + sidebar come albero di navigazione, sempre
   in sync (stessa selezione, stesse tab, stesso ordine).
2. Tab bar completa: click per attivare, × per chiudere, icona per tipo,
   indicatore attività agente, drag & drop per riordinare, overflow con
   scroll + menu, rename con doppio click, menu contestuale (Rinomina,
   Chiudi, Chiudi altre, Chiudi a destra).
3. Sidebar: drag & drop per riordinare tab dentro lo stesso worktree. Niente
   spostamento cross-worktree (semantica ambigua per PTY/chat legati alla
   directory del worktree).
4. Tab bar sempre visibile quando esiste un worktree selezionato, anche con
   una sola tab; con zero tab mostra solo il pulsante "+". Scorciatoie:
   ⌘1–⌘9 (⌘9 = ultima tab), ⌃Tab / ⌃⇧Tab per ciclare con wrap-around.
5. Il pulsante "+" apre lo stesso menu della sidebar: Nuova shell + un item
   per ogni agente chat (`AgentCatalog.all` → `openChatTab`).

## Architettura

Approccio scelto: SwiftUI puro, stato invariato in `AppModel`
(`@MainActor @Observable`). Tab bar e sidebar leggono le stesse proprietà
osservabili, quindi la sincronizzazione è strutturale, non evento-driven.
Scartati: tab bar AppKit (troppo codice per beneficio estetico marginale) e
`TabView` SwiftUI nativo (chrome non personalizzabile).

### Componenti nuovi (App/)

- **`TabBarView.swift`** — montata in `ContentView` sopra `terminalStack`,
  visibile quando `selectedWorktree != nil`. Contiene:
  - `ScrollView(.horizontal)` di `TabBarItem` + pulsante "+" a destra.
  - Auto-scroll sulla tab attiva quando cambia selezione.
  - Chevron con menu "elenca tutte le tab" quando le tab eccedono la
    larghezza disponibile.
- **`TabBarItem`** (stesso file o file dedicato se cresce) — una tab:
  - Icona per tipo: riusa la stessa logica icone di `TabRow` in sidebar
    (agente / terminale / markdown, `AgentIcon` inclusa).
  - Titolo; doppio click = rename inline (pattern `draftTitle` di `TabRow`).
  - Indicatore attività agente: stessa aggregazione per-tab usata oggi da
    `TabRow` (`RunningDots` / glifo needs-input via `activityPaneIds`).
  - Dot "dirty" per markdown non salvato (`markdownDocuments[tab.id]?.isDirty`).
  - × visibile on-hover; menu contestuale: Rinomina, Chiudi, Chiudi altre,
    Chiudi a destra.
- **`NewTabMenu`** — componente condiviso estratto dal menu "+" della
  sidebar (Nuova shell + agenti chat), usato da entrambe le superfici per
  non duplicare la lista.

### Logica nuova (TillerCore, funzioni pure su `[WorkspaceTab]`)

- `move(tabId:before:)` (o equivalente su indici): riordino con clamp,
  no-op se la posizione non cambia, ordine stabile per le altre tab.
- `cycleIndex(current:forward:count:)`: indice successivo/precedente con
  wrap-around per ⌃Tab / ⌃⇧Tab.
- Selezione per indice (⌘1–⌘9): out-of-range = no-op; ⌘9 mappa sull'ultima
  tab.

### API nuove in AppModel (wrapper sottili)

- `moveTab(_ tabId: UUID, before targetId: UUID?, in worktreeId: UUID)` —
  usato da tab bar e sidebar. L'ordine persiste già: l'array `tabs` è già
  serializzato nello snapshot di sessione.
- `selectTab(atIndex: Int)` e `cycleTab(forward: Bool)` sul worktree
  selezionato.

### Drag & drop

- Payload: UUID della tab come `Transferable` con UTType custom interno
  (nessun export file).
- `.draggable` su ogni `TabBarItem`; `.dropDestination` con indicatore di
  inserimento.
- Stesso meccanismo su `TabRow` in sidebar; drop rifiutato se la tab
  proviene da un altro worktree (nessun target valido mostrato).

### Scorciatoie (TillerApp.swift, menu comandi esistente)

- ⌘1–⌘9 → `selectTab(atIndex:)` (⌘9 = ultima).
- ⌃Tab / ⌃⇧Tab → `cycleTab(forward:)`.
- ⌘T (nuova tab) e ⌘W (chiudi) esistono già, invariati.

## Edge case

- **Chiusura markdown dirty**: la × e le voci "Chiudi altre/a destra"
  passano dal percorso di chiusura esistente della sidebar, che già gestisce
  la conferma; chiusure multiple iterano il percorso singolo, una conferma
  alla volta.
- **Worktree con zero tab**: tab bar mostra solo "+"; sotto resta
  `EmptyWorktreeView`.
- **Nessun worktree selezionato**: tab bar nascosta.
- **⌘n con n oltre il numero di tab**: no-op.
- **Lifecycle PTY**: la tab bar cambia solo `activeTabId`; il ZStack di
  `ContentView` che tiene montate tutte le tab dei worktree aperti resta
  invariato — nessun impatto su PTY vivi o su `openWorktreeIds`.

## Testing

- **TillerCore, swift-testing, TDD (test prima)**: `move` (clamp, no-op,
  stabilità), `cycleIndex` (wrap avanti/indietro, lista vuota),
  selezione per indice (out-of-range, ⌘9 → ultima).
- **UI, checklist smoke manuale** in `docs/` (pattern già in uso nel repo):
  sync bidirezionale selezione sidebar↔tab bar, drag riordino su entrambe le
  superfici, overflow + menu elenco, rename doppio click, menu contestuale,
  ×, "+" con menu agenti, scorciatoie, chiusura con markdown dirty,
  indicatore attività durante run agente.
- **Gate**: `Scripts/ci.sh` deve stampare `CI OK`.

## Fuori scope

- Spostamento tab tra worktree.
- Animazioni drag stile Safari/Xcode (AppKit).
- Persistenza aggiuntiva (già coperta dallo snapshot esistente).
