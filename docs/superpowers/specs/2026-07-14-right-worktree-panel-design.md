# Tiller — pannello destro Worktree Tools — Design

**Data:** 2026-07-14  
**Stato:** design approvato nel brainstorming; in attesa della revisione della spec da parte dell’utente.

## Obiettivo

Aggiungere al workspace di Tiller un pannello ridimensionabile e nascondibile a
destra del contenuto centrale. Il pannello segue sempre il worktree selezionato e
offre tre modalità:

1. **Files** — file explorer lazy della root del worktree;
2. **Diff** — diff unified di tutte le modifiche locali rispetto a `HEAD`;
3. **Status** — stato Git interattivo con stage, unstage e discard.

La feature deve restare coerente con il ruolo di Tiller come orchestratore di
terminali e agent: migliora consultazione e review, ma non introduce un editor
generico o un client Git completo.

## Decisioni approvate

- Layout a tre colonne nello stesso `HSplitView` esistente:
  `Sidebar | contenuto centrale | pannello destro`.
- Navigazione del pannello con selettore segmentato superiore
  **Files / Diff / Status**.
- Diff sempre **unified**, ottimizzato per una colonna laterale stretta.
- Diff predefinito: tutte le modifiche locali rispetto a `HEAD`, incluse staged,
  unstaged e untracked.
- Explorer: i Markdown aprono l’editor Tiller esistente; gli altri file vengono
  aperti dall’app macOS predefinita.
- Status interattivo: stage, unstage e discard per file e per sezione; niente
  commit, push o branch management.
- Aggiornamento automatico tramite filesystem events con debounce, più refresh
  manuale.
- Visibilità, larghezza e modalità attiva sono globali e persistite; il contenuto
  cambia con `selectedWorktree`.
- Implementazione scelta: terza colonna `HSplitView`, non `.inspector` e non un
  bridge `NSSplitViewController` dedicato.

## Scope

### Incluso

- pannello destro nascondibile e ridimensionabile;
- persistenza globale di visibilità, larghezza e modalità;
- file tree lazy, apertura file, reveal in Finder e copia percorso;
- badge Git sui file usando lo snapshot Status già caricato;
- status staged/unstaged/untracked;
- diff unified per file;
- stage, unstage e discard per file e bulk per sezione;
- aggiornamento automatico per modifiche filesystem e Git esterne;
- empty state, errori inline, conferme distruttive e limiti per diff grandi;
- supporto ai progetti non Git nella modalità Files.

### Escluso

- editor interno per file non Markdown;
- create, rename, move o delete dal file explorer;
- ricerca contenuto/nome file;
- diff contro branch base, commit o pull request;
- staging per hunk o linea;
- commenti inline sul diff;
- commit, amend, pull, push, branch, merge e conflict resolution;
- persistenza di cartelle espanse o file selezionato;
- replica delle feature remote/SSH di Orca.

## Riferimenti usati

La feature riprende selettivamente pattern già validati, senza copiarne lo scope:

- [Orca right sidebar](https://github.com/stablyai/orca/blob/840d3277d1d040ad059e442063f9d47972f25b6a/src/renderer/src/components/right-sidebar/index.tsx): pannello trailing ridimensionabile, modalità separate e lavoro sospeso quando chiuso;
- [Orca panel routing](https://github.com/stablyai/orca/blob/840d3277d1d040ad059e442063f9d47972f25b6a/src/renderer/src/components/right-sidebar/right-sidebar-panel-content.tsx): contenuti isolati per modalità;
- [Orca File Explorer](https://github.com/stablyai/orca/blob/840d3277d1d040ad059e442063f9d47972f25b6a/src/renderer/src/components/right-sidebar/FileExplorer.tsx): caricamento lazy, refresh da eventi e stato Git nel tree;
- [Orca Source Control](https://github.com/stablyai/orca/blob/840d3277d1d040ad059e442063f9d47972f25b6a/src/renderer/src/components/right-sidebar/SourceControl.tsx): separazione staged/unstaged/untracked e azioni contestuali;
- [OpenChamber Diff View](https://github.com/openchamber/openchamber/blob/a56584bd0919aafbe4c0ac6a6d135ff6aec670a4/packages/ui/src/components/views/DiffView.tsx): file selection, conteggi e diff per file.

Tiller mantiene però un’implementazione SwiftUI/macOS nativa e uno scope molto più
piccolo.

## 1. Layout e stato UI

`ContentView.splitContent` resta il proprietario del workspace. Il suo
`HSplitView` contiene:

1. la sidebar sinistra esistente, condizionale;
2. il `VStack` centrale esistente con `terminalStack` e `UsageBarView`;
3. `RightPanelView`, condizionale.

Il pannello usa:

- larghezza minima **280 pt**;
- larghezza ideale **360 pt**;
- larghezza massima **600 pt**.

La larghezza effettiva viene osservata e salvata; al lancio torna come larghezza
ideale. I nuovi valori persistiti sono:

- `rightPanel.visible`;
- `rightPanel.width`;
- `rightPanel.mode`.

Le chiavi vengono dichiarate in `AppSettings`, mentre `ContentView` e `TillerApp`
usano `@AppStorage` come già avviene per `sidebar.visible`. Non serve una
migrazione GRDB.

La superficie del pannello lascia visibile lo stesso materiale del chrome
laterale. Il contenuto centrale resta opaco. Il divider tra centro e pannello
viene coperto con lo stesso pattern già usato per il divider della sidebar
sinistra, senza introdurre un secondo contenitore di split.

### Comandi

- toolbar trailing: pulsante SF Symbol `sidebar.right`;
- menu View: **Mostra pannello destro** / **Nascondi pannello destro**;
- shortcut: `⌃⌘I`;
- il toggle sinistro `⌃⌘S` resta invariato.

### Routing delle modalità

`RightPanelMode` è un enum UI `String` con casi `files`, `diff`, `status`.

- Il valore salvato resta globale.
- Files è sempre disponibile quando esiste un worktree.
- Diff e Status sono disponibili soltanto per progetti Git.
- Se il valore salvato è Git-only ma il worktree corrente non è Git, la modalità
  effettiva è Files; il valore salvato non viene sovrascritto. Tornando a un
  repository Git si ripristina quindi la modalità preferita.
- Senza worktree selezionato il pannello mostra un empty state e non avvia watcher
  o processi Git.

Il pannello e il suo coordinatore lavorano soltanto mentre la colonna è visibile.
Nasconderla cancella task e monitor; riaprirla ricostruisce lo snapshot corrente.

## 2. Componenti e confini di package

### App

#### `RightPanelView`

Responsabilità limitate a:

- testata segmentata Files / Diff / Status;
- pulsante di chiusura;
- routing verso la view della modalità effettiva;
- empty state globale.

Non legge direttamente filesystem o Git.

#### `RightPanelModel`

`@MainActor @Observable`, creato una volta dal workspace. Possiede soltanto stato
volatile:

- worktree attualmente caricato;
- snapshot delle directory già espanse;
- snapshot Git Status;
- file Diff selezionato e relativo risultato;
- loading/error per modalità;
- task di load, refresh e debounce;
- monitor filesystem corrente;
- stato della mutazione Git corrente.

Cambiando worktree il modello cancella tutti i task, ferma il monitor, azzera
espansioni/selezione e avvia una nuova generazione. Ogni risultato asincrono porta
un generation token: un risultato del worktree precedente non può aggiornare lo
stato nuovo.

#### `FileExplorerView`

- carica la root all’apertura;
- carica i figli soltanto quando una directory viene espansa;
- ordina directory prima dei file, poi per nome localizzato case-insensitive;
- mostra i dotfile, ma esclude `.git`;
- mostra i symlink senza attraversarli come directory;
- usa lettere/simboli oltre al colore per i badge Git;
- singolo click seleziona la riga;
- doppio click: Markdown → `AppModel.openMarkdownTab`, altro file →
  `NSWorkspace.shared.open`;
- context menu: Apri, Mostra nel Finder, Copia percorso.

#### `GitDiffView`

- file picker derivato dallo snapshot Status;
- navigazione precedente/successivo;
- percorso, stato e conteggi aggiunte/rimozioni;
- `LazyVStack` di righe unified con numeri vecchio/nuovo, marker e hunk;
- Stage quando il file ha Changes o è Untracked, Unstage quando è Staged e
  Discard soltanto per Changes/Untracked, con le stesse conferme di Status;
- click su un file in Status apre Diff con quel file selezionato.

#### `GitStatusView`

Sezioni indipendenti:

1. **Staged**;
2. **Changes**;
3. **Untracked**.

Una entry con stato sia index sia worktree compare in Staged e Changes. Ogni riga
mostra path, stato testuale/simbolo e azioni valide. Ogni sezione offre le stesse
azioni in bulk sui propri path.

### TillerCore

Contiene solo logica filesystem senza SwiftUI o Git:

- `FileTreeNode`: value type `Identifiable`, `Equatable`, `Sendable` basato su path
  relativo e kind;
- loader di una singola directory;
- regole pure di ordinamento, esclusione `.git` e non ricorsione dei symlink;
- monitor ricorsivo macOS basato su FSEvents con flag file-level e supporto a più
  root;
- emissione di path cambiati; cancellazione idempotente.

Il monitor non contiene debounce o stato UI. Il debounce appartiene al
coordinatore App.

### TillerGit

Resta l’unico package che esegue `/usr/bin/git` e aggiunge API tipizzate per:

- risoluzione del git-dir reale;
- status;
- diff per file;
- stage;
- unstage;
- restore del working tree;
- rimozione di untracked.

`GitStatusEntry` conserva path corrente, eventuale path originale e stati index /
working tree. Il parser usa `git status --porcelain=v1 -z --untracked-files=all`,
quindi spazi, Unicode, newline e rename non richiedono parsing per righe o shell
quoting.

Gli argomenti passano sempre come `[String]` a `Process`. Nessuna API riceve un
comando shell completo.

## 3. Flusso dati

### Apertura o cambio worktree

1. Incremento generation e cancellazione dello stato operativo precedente.
2. Load della root Files.
3. Per un progetto Git, in parallelo:
   - load Status;
   - risoluzione del git-dir.
4. Avvio FSEvents su root worktree e git-dir reale.
5. Se la modalità effettiva è Diff, scelta del primo file modificato e load del
   relativo diff.

Il git-dir aggiuntivo serve a rilevare operazioni come `git add`, `commit` o
checkout eseguite da un terminale, incluse linked worktree dove `.git` è un file
che punta a una directory esterna.

### Eventi e refresh

- Gli eventi vengono coalesciti con un debounce di **250 ms**.
- Files invalida e ricarica soltanto le directory già caricate interessate e i
  loro antenati necessari.
- Per progetti Git viene aggiornato Status.
- Diff viene ricaricato soltanto se la modalità è attiva e il file selezionato è
  ancora modificato.
- Se il file selezionato non è più modificato, viene selezionato il primo file
  rimanente; senza file resta l’empty state.
- Refresh manuale cancella il load corrente e avvia subito un nuovo snapshot.

Dopo una mutazione Git il refresh è immediato. L’evento FSEvents successivo viene
assorbito dal debounce e non genera una seconda catena concorrente.

## 4. Semantica Diff

Per file tracked con `HEAD` esistente, il diff usa un confronto contro `HEAD`,
così staged e unstaged compaiono nello stesso risultato. I file untracked vengono
rappresentati come aggiunte da `/dev/null` usando `git diff --no-index`; exit code
`1` significa “differenze trovate” ed è accettato, non trattato come errore.

Per repository senza commit (`HEAD` unborn), ogni file presente viene confrontato
con `/dev/null`: il risultato rappresenta tutte le modifiche locali correnti,
indipendentemente da quanto sia già staged.

Il renderer supporta:

- header di file e hunk;
- righe context/add/delete;
- old/new line number;
- marker testuale `+`, `-`, spazio;
- riepiloghi Git per submodule;
- stato esplicito per diff binari.

Non applica syntax highlighting per linguaggio e non implementa staging per hunk
o linea.

## 5. Semantica delle azioni Git

Le mutazioni sono serializzate: una sola operazione per worktree alla volta. I
controlli coinvolti restano disabilitati fino al refresh conclusivo; non esiste
stato ottimistico.

### Stage

Aggiunge all’index le modifiche correnti dei path selezionati. È disponibile per
Changes e Untracked.

### Unstage

Ripristina l’index da `HEAD`, lasciando intatto il working tree. È disponibile per
Staged. In repository senza `HEAD`, dove le entry staged sono aggiunte nuove,
usa `git rm --cached --force -- <paths>`: rimuove i path dall’index senza
rimuoverli dal working tree.

### Discard Changes

Ripristina il working tree dall’index. Se un file ha parti staged e unstaged,
vengono eliminate soltanto quelle unstaged; il contenuto staged viene preservato.

### Discard Untracked

Rimuove i path untracked tramite Git, dopo conferma. I path sono limitati alla
root del worktree e passati come argomenti separati.

### Staged e discard

La sezione Staged non offre Discard. Il flusso sicuro è Unstage, poi eventuale
Discard dalla sezione Changes. Evita che un singolo comando ambiguo cancelli sia
index sia working tree.

### Bulk

Le azioni bulk operano esclusivamente sulle entry della propria sezione e
mantengono la stessa semantica delle azioni singole. Ogni entry espone path
corrente e path originale opzionale; se il path originale esiste, entrambe le
forme entrano nel pathset della mutazione.

## 6. Errori, limiti e sicurezza

Gli errori restano inline nel pannello e non bloccano terminali o agent.

- Il primo load mostra un indicatore; i refresh mantengono visibile lo snapshot
  precedente.
- Errori filesystem/Git mostrano messaggio concreto e Riprova.
- Una directory non leggibile mostra l’errore sulla propria riga.
- La cancellazione di task non produce un errore visibile.
- Se un file sparisce durante un’azione, l’errore viene mostrato e segue un
  refresh forzato.
- Discard mostra un dialog nativo con path esatto o numero di file. Annulla è il
  default.
- Il capture di TillerGit limita il diff a **5 MiB o 20.000 righe mentre legge
  stdout**, non dopo averlo accumulato. Al superamento termina il processo,
  scarta il risultato incompleto e mostra “Diff troppo grande da renderizzare”
  con apertura del file nell’app predefinita.
- I diff binari mostrano “Binary diff unavailable” ma mantengono le azioni file.
- I symlink non vengono seguiti dal tree loader.
- Lo stderr Git è preservato nel dettaglio dell’errore.
- Nessun path viene interpolato in una shell.
- Le mutazioni accettano soltanto path relativi provenienti dallo snapshot
  Status corrente e rifiutano path assoluti, componenti `..` o destinazioni
  risolte fuori dalla root del worktree.

## 7. Accessibilità e interazione

- Il selettore segmentato è raggiungibile da tastiera e ha label complete.
- Il pulsante toolbar e quello di chiusura hanno help e accessibility label.
- Gli stati Git non dipendono soltanto dal colore: mostrano anche simbolo e testo.
- Le righe hanno focus ring, selezione distinta e target minimo coerente con il
  resto della sidebar.
- Return apre il file selezionato; frecce navigano le righe; Space espande o
  comprime una directory.
- I dialog distruttivi dichiarano chiaramente l’effetto e mantengono Annulla come
  azione predefinita.

## 8. Test e verifica

### TillerGit — Swift Testing

- parser NUL-delimited: untracked, staged, unstaged, mixed, delete, rename;
- path con spazi, Unicode e newline;
- diff tracked combinato staged + unstaged;
- diff untracked, unborn `HEAD`, binary e submodule;
- stage/unstage/discard su repository temporanei reali;
- discard Changes preserva il contenuto staged;
- rename usa i path corretti;
- limite output produce un errore tipizzato, mai un diff parziale.

### TillerCore — Swift Testing

- ordinamento directory-prima stabile;
- esclusione `.git`, dotfile inclusi, symlink non ricorsivi;
- caricamento lazy e invalidazione mirata;
- FSEvents rileva create/write/delete annidati e una seconda root;
- stop del monitor impedisce eventi successivi.

### App — smoke test reale

L’App target non ha un test bundle. Non ne viene introdotto uno soltanto per il
wiring SwiftUI; la logica pura resta nei package.

Checklist:

1. apertura, resize, hide/show e persistenza dopo rilancio;
2. cambio rapido worktree durante refresh senza dati stale;
3. modifica annidata prodotta da un agent aggiorna Files, Status e Diff;
4. stage, unstage e discard singoli/bulk, inclusi mixed e untracked;
5. click Status apre Diff sul file corretto;
6. Markdown apre in Tiller, altri file nell’app predefinita;
7. progetto non Git, repository unborn, file binario, diff enorme e directory non leggibile;
8. pannello nascosto senza watcher o processi Git attivi;
9. nessuna regressione su sidebar sinistra, terminal split, Markdown editor e usage bar.

Gate finale: `Scripts/ci.sh` deve stampare **CI OK**, poi la checklist viene
esercitata nell’app reale.

## Criteri di accettazione

La feature è completa quando:

- il pannello destro è apribile, nascondibile, ridimensionabile e persistente;
- le tre modalità mostrano dati del worktree selezionato;
- Files reagisce alle modifiche esterne senza scansione ricorsiva iniziale;
- Diff mostra correttamente tutte le modifiche locali per file;
- Status applica stage/unstage/discard con le semantiche approvate;
- progetti non Git e repository senza commit hanno stati espliciti e funzionanti;
- cambio worktree e cancellazione task non mostrano dati stale;
- errori e limiti non producono fallback silenziosi;
- `Scripts/ci.sh` stampa `CI OK` e lo smoke test reale passa.
