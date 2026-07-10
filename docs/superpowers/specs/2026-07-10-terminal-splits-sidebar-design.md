# Split dei terminali nell'albero della sidebar — Design

**Data:** 2026-07-10
**Stato:** approvato a sezioni in brainstorming

## Obiettivo

1. I pane degli split terminale compaiono nell'albero della sidebar (oggi un tab splittato è un singolo nodo).
2. Menu contestuale sui nodi terminale della sidebar: split mirato, "affianca al terminale corrente" (sposta un terminale esistente in split accanto a quello aperto), chiusura con conferma.

## Contesto esistente

- `SplitTree` (TillerCore): enum binario immutabile `.leaf(id: UUID)` / `.split(axis:first:second:)`, con `splitting(leaf:axis:newLeaf:)`, `removing(leaf:)`, `leafIds`.
- `WorkspaceTab.content = .terminal(SplitTree)`; un `TerminalSplitHost` per tab renderizza l'albero cachando i `NSHostingController` per leaf ("instead of respawning shells").
- Il PTY è posseduto dalla view (`PtyTerminalPane.onDisappear → runtime?.stop()`); la cache è per-host, quindi senza modifiche uno spostamento cross-tab ucciderebbe il processo.
- `AppModel`: `split(paneId:axis:)`, `closePane(paneId:)`, `tabContaining(paneId:)`, `handleTitleChange(paneId:title:)` già esistenti.

## Decisioni (Q&A)

| Domanda | Decisione |
|---|---|
| Semantica click destro | Entrambe: split nuovo pane E affianca terminale esistente; più chiusura con conferma |
| Rappresentazione nell'albero | Pane come figli del nodo tab solo quando `leafIds.count > 1` (4° livello) |
| Etichetta nodi pane | Ultimo titolo PTY (`paneTitles`), fallback posizionale "Pane N"; solo in memoria |
| Scope "affianca" | Solo terminali dello stesso worktree del tab attivo |
| Sopravvivenza PTY nel move | Approccio 1: cache controller condivisa per worktree |

## Sezione 1 — Nodi pane nella sidebar

Quando un tab terminale ha più di un pane, sotto il suo `TabRow` compare una riga `PaneRow` per ogni leaf, in ordine `leafIds`:

- **Indent**: 4° livello, leading 60 (tab a 44). Guide albero: verticali passanti a x 3 e 26, gomito a x 42 con `branchLength` 14; ultima riga con raccordo curvo (stile `TreeGuideLines` esistente).
- **Icona**: agente da `model.agentActivity.paneAgents[paneId]` se presente, altrimenti `terminal`.
- **Etichetta**: `model.paneTitles[paneId]` — nuovo `[UUID: String]` in AppModel popolato da `handleTitleChange(paneId:title:)`; fallback "Pane 1", "Pane 2" (posizionale). Nessuna persistenza: al riavvio la shell rigenera il titolo.
- **Click**: seleziona worktree + attiva il tab. Nessun focus per-pane.
- **Stile**: font 11.5, colore `AppTheme.subtitle`.
- Il `TabRow` del tab splittato resta invariato.

## Sezione 2 — Menu contestuale sui nodi terminale

Target: `PaneRow` e `TabRow` di tab terminale (per tab mono-pane il target è l'unico leaf). I nodi markdown mantengono il menu attuale.

1. **Split orizzontale** / **Split verticale** — `model.split(paneId:axis:)` mirato su quel pane.
2. **Affianca al terminale corrente** — visibile solo se il pane non appartiene al tab attivo e il suo worktree coincide con quello del tab attivo. Azione: `model.adoptPane(paneId)`.
3. **Chiudi terminale…** (destructive, dopo Divider) — alert di conferma "Chiudere il terminale? Il processo in esecuzione verrà terminato." (Annulla / Chiudi). Conferma → `closePane(paneId)`; se era l'unico pane del tab → chiusura dell'intero tab.

Su `TabRow` terminale restano Rinomina e Chiudi; "Chiudi" passa dalla stessa conferma. "Chiudi" su tab markdown resta senza conferma.

## Sezione 3 — Cache pane condivisa e `adoptPane`

**`TerminalPaneCache`** (nuova, package TillerTerminal): classe `@MainActor` con `controllers: [UUID: NSViewController]`.

`TerminalSplitHost` accetta due parametri opzionali:
- `paneCache: TerminalPaneCache?` — se presente, il Coordinator la usa al posto della cache privata;
- `liveLeafIds: (() -> Set<UUID>)?` — set dei pane vivi in tutti i tab del worktree; il pruning scarta solo controller fuori dal set (default: `tree.leafIds` del solo host, comportamento attuale).

In `node()`: se il controller cached ha già un parent (arriva da un altro tab), `removeFromParent()` prima del riuso.

`AppModel` tiene `paneCaches: [UUID: TerminalPaneCache]` (chiave worktreeId, creazione lazy); `ContentView` lo passa a ogni `TerminalSplitHost` insieme alla closure `liveLeafIds`.

**`adoptPane(_ paneId: UUID)`** in AppModel:
1. Guardie: `tabContaining(paneId:)` esiste, stesso worktree del tab attivo, pane non già nel tab attivo, tab attivo è terminale.
2. `removing(leaf: paneId)` dall'albero sorgente; se il tab resta senza pane viene rimosso dalla lista tab.
3. `splitting(leaf: primoLeafDelTabAttivo, axis: .horizontal, newLeaf: paneId)` sull'albero del tab attivo.
4. `persistTabs(for:)`.

Solo mutazioni immutabili di `SplitTree`; nessuna API nuova in TillerCore.

### Rischio dichiarato

Il reparent cross-host avviene nello stesso update SwiftUI, come i rebuild interni che oggi preservano il PTY. Se in verifica manuale `onDisappear` scattasse comunque (processo terminato), il fallback naturale è il respawn con replay scrollback: degrado, non rottura. Test manuale: `top` in un pane → affianca → processo vivo.

## Test

- **TillerCoreTests**: mutazioni combinate su `SplitTree` — graft di leaf esistente via `splitting(newLeaf: idEsistente)`, remove che svuota l'albero (nil), ordine `leafIds` dopo remove+graft.
- **Manuali**: pane nodes visibili solo con >1 pane; titoli PTY aggiornati; menu contestuale (split mirato, affianca solo stesso worktree/tab non attivo, conferma chiusura); processo vivo dopo affianca; tab sorgente svuotato chiuso.

## Fuori scope

- Focus del singolo pane dal click sul nodo.
- Affianca cross-worktree.
- Persistenza dei titoli pane.
- Riordino drag&drop dei pane.
