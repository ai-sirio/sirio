# Lista modifiche espandibile — design

Sostituisce le due viste git del pannello destro (Status e Diff) con una sola:
l'elenco dei file modificati, dove ogni riga si espande e mostra sotto di sé il
proprio diff. Repo Swift (`~/Desktop/Progetti/tiller`).

## Cosa esiste già, e non si ricostruisce

- `GitDiff.parse` (`Packages/TillerGit/Sources/TillerGit/GitDiff.swift`) produce
  già `GitDiffLine` con `oldLineNumber`/`newLineNumber`, marca gli header `@@`
  come `.hunk` e conta additions/deletions. Il parsing degli hunk non si tocca.
- `GitDiff.load(entry:in:)` gestisce già untracked, repo senza HEAD, rename e
  lettura del testo `HEAD:` per la highlighting.
- `DiffHighlightCache.shared.highlights(path:oldText:newText:)` fa la syntax
  highlighting per riga; `AttributedCodeRenderer.renderLine` la rende.
- `FileIconKey` / `FileIconTheme` (`TillerCore`) + `Assets.xcassets/FileIcons`
  danno le icone per tipo di file, già usate dal Files sidebar.
- Le mutazioni (`stage`, `unstage`, discard) e il refresh vivono in
  `RightPanelModel` e restano dove sono.

## Il problema con la vista attuale

`GitStatusView` elenca i file ma non mostra codice. `GitDiffView` mostra il
codice ma di **un solo** file per volta, scelto da un `Picker` a tendina:
`RightPanelModel` tiene un singolo `diff: GitFileDiff?`. Per confrontare due
file si passa dalla tendina, e la lista non dice quanto è grande una modifica.

## Decisioni

1. **Una vista sola.** Status e Diff si fondono. Le sezioni Staged / Changes /
   Untracked restano; ogni riga file diventa espandibile.
2. **Niente barre «N unmodified lines».** Il contesto resta quello di
   `--unified=3` e gli hunk si susseguono separati dall'header `@@`, come oggi.
3. **Riga file:** icona per tipo di file (riuso del Files sidebar) + path
   completo troncato a metà + conteggi `+N −N`. Lo stato M/A/D/? passa nel
   colore, non più nella lettera monospace.
4. **Azioni in hover:** a riposo si vedono i conteggi; in hover al loro posto
   compaiono `Discard`, `Stage`/`Unstage` e apri-in-editor. Il menu contestuale
   resta e offre le stesse azioni.
5. **Tutto collassato all'apertura.** Nessun `git diff` parte finché non si
   espande un file.
6. **`RightPanelMode.diff` sparisce.** Restano `Files` e `Status`.

## Struttura

File nuovi, in `App/RightPanel/`:

| File | Responsabilità | ~righe |
|---|---|---|
| `ChangesListView.swift` | Header, sezioni, ScrollView, stati vuoto/errore | 130 |
| `ChangedFileRow.swift` | Riga file: chevron, icona, path, conteggi, hover | 110 |
| `FileDiffBody.swift` | Corpo espanso: header `@@`, righe numerate, highlight | 120 |
| `DiffLoadStore.swift` | `[GitPath: DiffLoadState]`, dedup richieste in volo | 90 |
| `FileTypeIcon.swift` | Icona per tipo file, estratta da `FileExplorerView` | 30 |
| `GitPanelTypes.swift` | `PendingGitDiscard`, simbolo e colore di stato | 40 |

File cancellati: `GitDiffView.swift`, `GitStatusView.swift`.

File modificati:

- `RightPanelModel.swift` — perde `diff`, `diffLoading`, `diffError`,
  `selectDiff`, `ensureDiffLoaded`; guadagna la mappa dei conteggi. Si accorcia.
- `RightPanelMode.swift` — via il case `.diff`; `effective` mappa il vecchio
  rawValue `"diff"` su `.status`.
- `RightPanelView.swift` — instrada su `ChangesListView`.
- `FileExplorerView.swift` — usa `FileTypeIcon`, perde `iconView` e il
  `statusSymbol` duplicato.
- `ContentView.swift:98` — il comando che apriva la modalità Diff punta a
  `.status`.

### Perché lo store è separato dal model

`RightPanelModel` è `@Observable`: se la mappa dei diff vivesse lì, il
caricamento di un file invaliderebbe ogni riga della lista. Con uno store
separato, e passando alla riga solo il proprio stato, l'invalidazione resta
locale al file espanso.

## Conteggi `+N −N`

`GitFileDiff.additions/deletions` esiste solo dopo aver caricato il patch, ma le
righe partono collassate: caricare tutto per mostrare un numero annullerebbe il
lazy. Serve quindi un comando che dia i conteggi di tutti i file in un colpo:

    git diff --numstat HEAD

Nuova funzione `GitDiff.numstat(in:)` in `TillerGit` (~30 righe), chiamata a
ogni `refresh()`, che restituisce `[GitPath: (additions: Int, deletions: Int)]`.

Casi limite:

- **Untracked** — non compare in numstat. Conteggio righe del file, `+N −0`,
  con lo stesso limite di 500KB di `GitDiff.readUTF8`; oltre quel limite,
  nessun conteggio.
- **Binario** — numstat emette `-\t-`. La riga mostra `bin`.
- **Rename** — numstat emette il path nella forma `vecchio => nuovo`; il
  conteggio si associa al path nuovo, quello che `GitStatusEntry.path` riporta.

## Comportamento

**Espansione.** Click sulla riga o sul chevron fa toggle. All'espansione, se lo
stato del path è `.idle`, `DiffLoadStore` lancia `GitDiff.load(entry:in:)`.
Richieste concorrenti sullo stesso path vengono deduplicate. Collassare non
scarta il diff: riespandere è immediato.

**Highlighting.** Invariata: `FileDiffBody` chiama `DiffHighlightCache` in un
`.task(id:)`, quindi parte solo per i file davvero espansi.

**Refresh e mutazioni.** Dopo `refresh`, `stage`, `unstage` o discard: i path
spariti dallo status escono da `expanded` e dallo store; i path ancora espansi
vengono invalidati e ricaricati, gli altri no. Mettere in stage un file lo
sposta da Changes a Staged ma resta espanso, perché espansione e store sono
chiavati su `GitPath`, non sulla posizione in lista.

## Errori

| Caso | Comportamento |
|---|---|
| `--numstat` fallisce | Lista funziona, conteggi nascosti |
| Load del diff fallisce | Errore dentro la riga espansa, con `Retry` e `Open file`; gli altri file non ne risentono |
| File binario | `Binary diff unavailable` inline nella riga, non a tutto pannello |
| Conflitto | Espandibile, azioni disabilitate, etichetta `Resolve in terminal` |
| Working tree pulito | `ContentUnavailableView` «Working tree clean», come oggi |

## Test, scritti prima

`swift-testing` (`@Test` / `#expect`).

`Packages/TillerGit/Tests/TillerGitTests/GitDiffTests.swift` — parsing numstat:
riga normale, path con spazi, rename `a => b`, binario `-\t-`, output vuoto.
Più un test d'integrazione su repo temporaneo con `GitTestSupport`.

`AppTests/ChangesListTests.swift` (nuovo) — la macchina a stati:

- espandere un path `.idle` lo porta a `.loading`
- due espansioni ravvicinate sullo stesso path fanno una sola richiesta
- collassare conserva il diff caricato
- dopo refresh, i path spariti dallo status escono da `expanded`
- dopo stage, il path resta espanso pur cambiando sezione
- `RightPanelMode.effective(rawValue: "diff", isGitRepository: true) == .status`

Gli AppTests girano in `Scripts/ci.sh`; il selector va sulla struct, non sul
`@Suite`, o la run passa a vuoto con 0 test.

## Fuori scope

Barre «N unmodified lines» ed espansione del contesto; word-level diff dentro
la riga; vista side-by-side; ricerca dentro il diff; indicatore di troncamento
per i diff oltre `GitDiff.outputLimits`.

## Note

`Packages/TillerGit/Sources/TillerGit/GitDiffSideBySide.swift` non è
referenziato da nessun file in `App/`: vive solo per i propri test. È codice
morto preesistente, estraneo a questo lavoro; segnalato, non toccato.
