# Sidebar Tree Navigation — Design

**Data:** 2026-07-10
**Stato:** Approvato

## Obiettivo

Refactoring UI ispirato allo screenshot Conductor-style: eliminare la tab bar e la
gestione a tab nell'area principale; navigare tra i terminali tramite una struttura
ad albero a 3 livelli nella sidebar. La zona titlebar sopra il terminale assume il
colore del terminale. La usage bar resta invariata. Si aggiunge un pulsante per
nascondere la sidebar.

## Scope

**Incluso:** rimozione TabBarView, tree a 3 livelli in sidebar (Project → Worktree
→ Tab), titlebar colore terminale con titolo centrato, toggle sidebar.
**Escluso:** usage bar (invariata), persistenza (schema v7 invariato), split
terminale (invariato), editor markdown (sopravvive come nodo del tree).

## Architettura — Approccio scelto

**Minimal rewire (view-layer only).** AppModel non cambia strutturalmente:
`projects` → `worktrees[projectId]` → `tabs[worktreeId]` esprimono già l'albero.
Selezionare un nodo tab = `selectedWorktree = w` + `activateTab(tabId, in: w.id)`
(API esistenti). Alternative scartate: unified selection model (churn su AppModel
42K + test, stesso risultato visivo); List/OutlineGroup di sistema (già rigettata
NavigationSplitView su macOS 26, righe custom comunque necessarie).

## Sezione 1 — Sidebar tree

Dentro `SidebarView` esistente (LazyVStack custom, nessuna List):

```
▼ 📁 tiller                    ← ProjectRow (invariata)
    ● main  [primary]          ← WorktreeRow (+ hover "+")
        🤖 claude               ← TabRow NUOVO (indent ~28)
        ⌨ zsh                  ← TabRow
        📄 README.md  •         ← TabRow markdown (dirty dot)
    ○ feature-x
    + New Worktree…
```

**TabRow** (nuovo componente):
- Icona: `AgentIcon` se un pane ha un agente, SF `terminal` altrimenti, `doc.text`
  per markdown; dirty dot per markdown non salvati.
- Titolo = `tab.title` (già aggiornato via onTitleChange).
- Selezione: pill `AppTheme.selectionFill` (stessa di WorktreeRow) quando
  `selectedWorktree == worktree && activeTab == tab`. Tap = seleziona worktree +
  attiva tab.
- Hover: "×" trailing (chiudi). Double-click = rename inline (TextField, stessa
  logica migrata da TabBarView).
- Context menu: Rinomina, Chiudi.

**WorktreeRow:** hover mostra "+" trailing (nuovo terminale). Context menu
esistente unificato: Nuovo Terminale + agenti ("New X Panel" esistenti), Set/Unset
Primary, Remove Worktree.

**Espansione:** i nodi tab sono sempre visibili sotto ogni worktree del progetto
espanso — nessun chevron di secondo livello. Tree visibile per tutti i worktree,
non solo il selezionato.

**Shortcut:** ⌘T (nuova tab) e ⌘W (chiudi tab) invariati — già su AppModel.

## Sezione 2 — Area principale + titlebar

- `ContentView.workspaceView`: riga `TabBarView(...)` eliminata; file
  `TabBarView.swift` cancellato. Colonna destra = `terminalStack` + `UsageBarView`.
- Drop di file .md sull'area → apre nodo markdown nel tree (`openMarkdownTab`
  invariato).
- **Titlebar colore terminale:** il background `AppTheme.background` della colonna
  destra si estende sotto la titlebar (`.ignoresSafeArea(edges: .top)`). La sidebar
  mantiene il glass fino in cima. `.toolbarBackgroundVisibility(.hidden)` resta.
- **Titolo centrato:** overlay custom in cima alla colonna destra —
  `branch ⋅ project` (branch primario, project secondario, `AppFont` esistente).
  Non `.navigationTitle` (su macOS finisce leading).
- Toolbar trailing: split + permessi invariati.

## Sezione 3 — Toggle sidebar

- Bottone toolbar **leading** (accanto ai semafori): SF `sidebar.left`.
- Shortcut **⌃⌘S** (standard macOS Hide Sidebar) via `.commands` in TillerApp,
  voce menu View → "Nascondi/Mostra Sidebar".
- Stato: `@AppStorage("sidebar.visible")` — persiste tra lanci.
- Implementazione: `if sidebarVisible { SidebarView… }` dentro HSplitView, con
  `.animation(.easeInOut(duration: 0.2))`. Overlay divider si spegne quando
  nascosta.

## Sezione 4 — Dati, AppModel, persistenza

- **AppModel: zero cambi strutturali.** Eventuale helper one-liner
  `selectTab(_:in:)` se serve in più punti.
- **Persistenza invariata:** schema v7 tabs identico, nessuna migrazione. Le righe
  tab del DB diventano i nodi del tree.
- **Rimozioni:** `TabBarView.swift`; `tillerTabBackground` se orfano dopo la
  rimozione (verificare usi).

## Sezione 5 — Test e verifica

- Nessuna nuova logica core → i 219 test TillerCore devono restare verdi.
- Logica pura eventualmente estratta (es. formattazione titolo) → Swift Testing
  solo se non banale.
- Checklist manuale: tree naviga tra terminali; ×/+/rename da tree; markdown apre
  da tree; drop .md; ⌘T/⌘W; toggle sidebar ⌃⌘S con persistenza; titlebar colore
  terminale; usage bar intatta; regression split terminale.

## Decisioni chiave (Q&A)

1. Tree a 3 livelli Project → Worktree → Terminal (ogni tab → nodo).
2. Markdown editor sopravvive: nodi `doc.text` fratelli dei terminali.
3. Titlebar minimale: toggle leading, titolo centrato, split+permessi trailing.
4. Lifecycle nodi: hover buttons + context menu (entrambi).
5. Tab sempre visibili sotto i worktree (nessun collapse di secondo livello).
