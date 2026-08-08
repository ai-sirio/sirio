# 003 — Diff tab: drag da Changes al pane centrale, resa side-by-side

Stato: spec approvata (grilling 2026-08-08), implementazione da fare.

## Obiettivo

Trascinare una riga della lista **Changes** (right sidebar) dentro il pane
centrale apre un **tab diff** per quel file. Il tab rende il diff
**side-by-side** (due colonne), non unified. Il pannello destro resta com'è.

## Stato del codice oggi (verificato)

- `App/RightPanel/ChangesListView.swift` + `ChangedFileRow.swift`: la riga
  espande in-place e mostra `FileDiffBody`, un diff **unified** numerato con
  syntax highlight (`DiffHighlightCache`). Nessun renderer side-by-side esiste.
- `Packages/TillerGit/GitDiff.load(entry:in:)` produce `GitFileDiff`
  (`lines`, `additions`, `deletions`, `isBinary`, `isSubmodule`, `oldText`,
  `newText`). **Non ha `--cached`**: Staged e Changes passano entrambi da
  `git diff HEAD`, solo Untracked usa il ramo `--no-index`. Rinominati sono
  già gestiti via `entry.originalPath` / `entry.mutationPaths`.
  Limiti: 5 MB / 20.000 righe (`GitDiff.outputLimits`).
- Pane centrale: registry di adapter (`App/Workspace/WorkspaceContentAdapter.swift`).
  `WorkspaceContentKind` = `terminal | chat | document | browser`.
  Vincolo di unicità DB: **`(worktreeId, kind, contentId)`**, e per un document
  `contentId` è solo `documentID.canonicalPath` — l'editor **non** fa parte
  dell'identità.
- Drag dei tab: **non** è un drag di sistema. `WorkspaceDragCoordinator` guida
  un gesto custom su `NSEvent.mouseLocation`, con overlay disegnato a mano e
  `DropTargetResolver` in coordinate root flipped. Nessuna pasteboard.
- `DropTargetResolver.resolve` richiede `draggedTab` (che poi ignora:
  `_ = draggedTab`) e `sourceGroup`, usato solo dalla Rule 7.
- `AppModel.openDocument` fa già dedup: se il file è aperto, attiva quel tab.

## Decisioni

### D1 — Il diff è un content kind proprio

Nuovo `WorkspaceContentKind.diff` con adapter dedicato, **non** un terzo
`DocumentEditorKind`.

Motivo: con `.document` lo stesso file aperto come editor e come diff
collide sul vincolo `(worktreeId, kind, contentId)`, perché l'editor non entra
nell'identità. Le alternative erano cambiare il formato di `contentId`
(riscrive l'identità di ogni document tab già persistito → migrazione) o un
path sintetico tipo `/path/file.swift#diff` (hack che filtra in
save/isDirty/recovery). Aggiungere un **case** a un enum persistito come
stringa non richiede migrazione: le righe esistenti non cambiano.

L'adapter diff è più sottile del document: read-only, niente buffer, niente
`save`, niente `isDirty`.

### D2 — Semantica del diff

Il tab riusa `GitDiff.load(entry:in:)` così com'è, quindi mostra esattamente
ciò che la riga di origine mostrava. Nessun lavoro sul lato git.

### D3 — Persistenza e ricostruzione

Il tab persiste il path. Al restore (e a ogni refresh di Changes) l'entry si
ricava dal `git status` corrente. Se il file non è più modificato, **il tab
resta aperto** e mostra uno stato vuoto: un tab che si chiude da solo mentre
l'agente lavora è peggio di un tab vuoto, e uno snapshot congelato nel DB
mente al primo salvataggio.

### D4 — Meccanica del drop: AppKit

`registerForDraggedTypes` sulla view root del workspace +
`NSDraggingDestination`:

- `draggingUpdated` → `DropTargetResolver` + overlay esistente
- `performDragOperation` → intent

Non `.dropDestination` SwiftUI: l'overlay e il resolver parlano già coordinate
root flipped AppKit, e il pane centrale è pieno di `NSView` annidate
(terminali libghostty, hosting controller) che mangerebbero il drop — il repo
ha già questa cicatrice con `NSTextView`.

### D5 — Payload: solo UTType custom

La riga di Changes espone **solo** un UTType Tiller dedicato, **non**
`public.file-url`.

Motivo: con AppKit vince la view sotto il puntatore. Con `public.file-url` nel
payload, droppare sopra un terminale farebbe incollare il path invece di
aprire il diff — cioè quasi sempre, visto che i pane contengono terminali. La
semantica dipende quindi dalla **sorgente**: da Changes si trascina un diff,
dal Files explorer si trascina un file (e verso terminale/chat continua a
funzionare come oggi).

Il nuovo UTType va dichiarato in `project.yml` (`UTExportedTypeDeclarations`)
e `xcodegen generate` va rieseguito — un `UTType(exportedAs:)` non dichiarato
non esiste a runtime.

### D6 — Bersagli del drop

Riuso pieno di `DropTargetResolver`: centro = nuovo tab, bordo = edge split,
tab strip = inserimento a indice preciso.

`draggedTab` e `sourceGroup` diventano **opzionali**; `nil` significa drag
esterno e fa saltare la Rule 7 (che protegge solo dallo spostare un tab dentro
il pane da cui viene — situazione inesistente per un drag esterno).
`draggedTab` era già ignorato nel corpo, quindi diventa onesto invece che
finto. I ~6 call site in `DropTargetResolverTests` vanno aggiornati.

### D7 — Dedup

Come `openDocument`: se esiste già un tab diff per quel path nel worktree, lo
**attiva** (portando il focus sul suo gruppo) invece di crearne un secondo.

### D8 — Altri ingressi

Oltre al drag: **doppio click** sulla riga e voce di menu contestuale
"Open Diff in Editor". Il drag da solo è invisibile.

Attenzione nota: un `DragGesture` simultaneo nella sidebar mangia i tap
(sintomo: hover sì, click no). L'ordine dei gesture va verificato a mano.

### D9 — Tab: titolo e icona

Titolo = `lastPathComponent`. Path relativo nel tooltip. Icona dedicata,
distinta da quella del file. `kindLabel` = "Diff" per l'accessibilità.
Niente titolo decorato (`file.swift (diff)`): la larghezza del tab è la
risorsa scarsa, l'icona distingue già.

### D10 — Read-only + auto-refresh

Nessun editing, nessun salvataggio, nessuno stage per hunk. Il tab si aggiorna
sullo stesso segnale che aggiorna Changes.

### D11 — Casi che un diff non può mostrare

Uno stato vuoto unico (`ContentUnavailableView`) con la ragione a testo:
binario, submodule, oltre i limiti, file tornato pulito.

### D12 — Renderer side-by-side

- Allineamento per hunk, righe filler dal lato mancante.
- **Niente** evidenziazione word-level in fase 1 (richiede diff intra-riga:
  feature separata).
- Niente wrap: scroll orizzontale sincronizzato tra le colonne.
- Scroll verticale unico — le due colonne stanno in un solo `ScrollView`,
  quindi la sincronia è gratis e non ci sono due offset da inseguire.
- Riuso di `DiffHighlightCache` (già async e condiviso), non di `FileDiffBody`.

## Test prima del codice (swift-testing)

1. **Allineamento side-by-side** (puro, senza UI): `GitFileDiff` → righe
   appaiate con filler. Casi: addizioni pure, cancellazioni pure, hunk
   multipli, file vuoto, hunk che inizia con contesto.
2. **`DropTargetResolver` con sorgente esterna** (`nil`): centro, ogni bordo,
   tab strip con indice, e verifica che la Rule 7 **non** scatti.
3. **Dedup**: due drop dello stesso path → un solo tab, il secondo attiva.
4. **Restore**: tab diff persistito riapre con kind `.diff` e **non** collide
   con un tab editor dello stesso file (è il test che protegge D1).
5. **Stati vuoti**: binario, submodule, oltre i limiti, file tornato pulito.

Fuori dai test automatici, in checklist manuale: gesto di drag, doppio click,
menu contestuale, overlay durante il drag.

## Checklist manuale

- [ ] Trascinare una riga da Changes sul centro di un pane → si apre il tab diff
- [ ] Trascinare sul bordo → edge split
- [ ] Trascinare sul tab strip → inserito nella posizione giusta
- [ ] L'overlay si accende durante il drag esterno come per un tab
- [ ] Doppio click sulla riga apre il diff; il click singolo continua a
      espandere/collassare (verifica del conflitto di gesture)
- [ ] Menu contestuale → "Open Diff in Editor"
- [ ] Stesso file droppato due volte → un solo tab, il secondo lo attiva
- [ ] File aperto sia come editor sia come diff → due tab, nessun crash,
      entrambi sopravvivono al riavvio
- [ ] Diff side-by-side: colonne allineate, scroll verticale unico, scroll
      orizzontale sincronizzato
- [ ] File binario / submodule → stato vuoto con ragione
- [ ] Agente modifica il file mentre il tab è aperto → il diff si aggiorna
- [ ] File tornato pulito → tab aperto con stato vuoto, non chiuso
- [ ] Drag dal Files explorer verso terminale → continua a incollare il path

## Verifica

`Scripts/ci.sh` è **rosso di baseline** dal 2026-08-06 su 4 suite preesistenti.
La verifica si fa scagionando il diff: due run sullo stesso albero, non
confronto con `HEAD`. Il gate va eseguito una volta sola, a fine fase.

## Fasi

1. `WorkspaceContentKind.diff` + `WorkspaceContentRef.diff` + mapping in
   `SQLiteWorkspacePersistence` → test 4 verde
2. Allineamento side-by-side puro → test 1 verde
3. `SideBySideDiffView` + `DiffContentAdapter` + `PaneTabPresentation`
4. `DropTargetResolver` con sorgente opzionale → test 2 verde
5. UTType in `project.yml` + `.draggable` sulla riga di Changes
6. `NSDraggingDestination` sul workspace + intent → test 3 verde
7. Doppio click + menu contestuale
8. Stati vuoti → test 5 verde
9. Gate + checklist manuale
