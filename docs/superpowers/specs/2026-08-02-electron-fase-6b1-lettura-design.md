# Fase 6b-1 — Leggere

Data: 2026-08-02
Fase precedente: [6a — Diff](2026-08-02-electron-fase-6a-diff-design.md)

## Obiettivo

Navigare e leggere qualsiasi file del worktree, con evidenziazione sintattica, e
portare la stessa evidenziazione dentro il diff chiuso in 6a.

Fine fase: apri il pannello `Files`, espandi cartelle, clicchi un file, e lo
leggi colorato in un tab; il diff dello stesso file è colorato allo stesso modo.

## Perché 6b è divisa

La Fase 6 residua contiene quattro blocchi Swift — albero dei file, editor di
codice, editor markdown, evidenziazione nel diff — per circa 1.700 righe. Il
taglio per componente (prima l'albero, poi l'editor, poi il markdown) mette tre
blocchi su quattro in attesa dello stesso lavoro su CodeMirror, che verrebbe
affrontato due volte.

Il taglio adottato è per **capacità visibile**:

| Sotto-fase | Capacità | Contenuto |
| --- | --- | --- |
| **6b-1** | leggere | albero, tab file in sola lettura, evidenziazione nel visualizzatore e nel diff |
| **6b-2** | scrivere | editing, stato sporco, salvataggio, editor markdown con toolbar e anteprima |

La sola lettura è già CodeMirror completo in `readOnly`: l'editing di 6b-2 è un
incremento sopra qualcosa che funziona, non l'accensione di un blocco monolitico.

## Il motore di evidenziazione

**CodeMirror 6 per tutto**, un motore solo.

Le tre funzioni che servono vengono dallo stesso pacchetto: il visualizzatore in
sola lettura (6b-1), l'editor (6b-2), e per il diff `highlightTree` di
`@codemirror/language`, che tokenizza **senza istanziare un editor**. È
l'equivalente diretto di `LineHighlightMap` in Swift.

L'alternativa considerata era Shiki per la parte in lettura. `codeToTokens`
restituisce token già raggruppati per riga, quindi combacia con le righe del
diff senza scrivere alcun mapping — comodo esattamente nella settimana in cui si
scrive il diff. Ma 6b-2 introduce comunque CodeMirror per l'editing, e si
resterebbe con due motori e due sistemi di tema: lo stesso file TypeScript
colorato in due modi fra il diff e l'editor.

Copertura linguaggi: i pacchetti `@codemirror/lang-*` coprono una ventina di
linguaggi, **la stessa copertura che Tiller ha oggi** con tree-sitter e
`CodeEditLanguages`. Non è una regressione: è parità.

### La tokenizzazione parte dal file intero, non dalla riga

`DiffHighlighter.swift` non evidenzia il diff riga per riga. Costruisce
`LineHighlightMap` tokenizzando il **testo completo** del file, poi proietta gli
intervalli sulle singole righe.

Deve farlo. Una riga di diff isolata è ambigua: `}` da sola non dice se chiude
una funzione, una stringa multilinea o un commento a blocchi. Solo il file
completo disambigua. Il porting mantiene questa struttura — tokenizzare il tutto,
proiettare sulle righe — perché il vincolo è nella natura dei linguaggi, non
nell'implementazione Swift.

## Cosa cambia rispetto a Tiller Swift

### Un solo tema di icone

Tiller ha due temi: SF Symbols e Material. **Resta solo Material.**

SF Symbols è un font di sistema Apple: fuori da macOS non esiste. Mantenere il
tema significherebbe reimplementarlo con icone diverse sotto lo stesso nome — un
tema che mente su cosa è. Le icone Material arrivano da Iconify, **impacchettate
a build-time**: l'app non deve contattare la rete per disegnare un albero di file.

Il selettore di tema nelle impostazioni sparisce con il tema.

Il porting è quasi gratuito: la tabella `materialAssets` in `FileIconTheme.swift`
mappa già ogni `FileIconKey` a un nome di icona del set **`material-icon-theme`**
di Iconify, con il prefisso `mat-`. Sono gli stessi nomi che
`@iconify-json/material-icon-theme` espone. Si porta la tabella togliendo il
prefisso, non si riscelgono le icone.

`.symlink` è assente dalla tabella Material anche in Swift, deliberatamente, e
ripiega sull'icona generica: il porting mantiene l'assenza.

### Il watcher perde quasi tutto il suo codice

`FileSystemEventMonitor` è 143 righe, di cui circa 130 sono gestione manuale di
`FSEventStream`, lock, `DispatchQueue` e continuation. In Electron l'equivalente
è **chokidar, già installato** e usato dalla Fase 3: restano poche righe di
adattamento.

Il conteggio delle righe Swift misura la distanza dalla piattaforma di partenza,
non il lavoro. All'opposto, `FileIconKey` è 111 righe di **tabelle** e si porta
quasi uno a uno, perché è dati e non meccanica.

## Architettura

```
src/shared/files/
  types.ts        FileNode, FileContent — schemi zod condivisi
  icon-key.ts     tabelle nome/estensione -> chiave icona (porta FileIconKey)
  language.ts     estensione -> linguaggio CodeMirror (porta CodeLanguageResolver)

src/main/files/
  tree.ts         elenco figli di una cartella, confinato alla radice
  read.ts         lettura file con limiti e rilevamento binario
  watch.ts        chokidar sulla radice del worktree -> evento files.changed

src/renderer/src/lib/workspace/
  FilesTree.svelte    albero, espansione pigra, icone
  FileTab.svelte      CodeMirror in readOnly
  highlight.ts        highlightTree -> intervalli per riga (per file e per diff)
```

`highlight.ts` sta nel renderer e non nel main perché è l'unico consumatore, e
perché spostare token attraverso il socket per ogni file aperto costerebbe più
del calcolo.

### Protocollo

Tre voci nuove accanto alle sei di `git.*` della Fase 6a:

| Richiesta | Parametri | Risposta |
| --- | --- | --- |
| `files.list` | `worktreeId`, `path` | `{ nodes: FileNode[] }` |
| `files.read` | `worktreeId`, `path` | `{ text, isBinary, truncated }` |

| Evento | Quando |
| --- | --- |
| `files.changed` | il watcher rileva scritture, con le cartelle toccate |

### L'albero si espande una cartella per volta

`FileTreeLoader.children(at:rootURL:)` elenca **una sola cartella**, non
ricorsivamente. L'espansione pigra si porta identica: un worktree con
`node_modules` non deve essere percorso interamente per mostrare la radice.

`.git` resta escluso, come in Swift.

Il confinamento alla radice si porta insieme all'errore che lo esprime:
`FileTreeError.pathOutsideRoot`. Un percorso che esce dalla radice è un errore
con un motivo, non un elenco vuoto.

### Il watcher non ricarica l'albero

`files.changed` porta le cartelle toccate. Il renderer invalida **solo quelle**
fra le cartelle espanse, e le ricarica. Un albero espanso in profondità non si
richiude perché qualcuno ha salvato un file altrove.

### Limiti

Gli stessi di 6a, riusati e non ridefiniti: 5 MB e 20.000 righe. Un file oltre
il limite si apre troncato e lo dice; non si apre a metà in silenzio.

Il rilevamento binario riusa l'euristica già scritta in `changes.ts` — un byte
nullo nei primi 8.000 — che è quella di git stessa.

## La tab del file

`WorkspaceContentRef` guadagna il terzo membro:

```ts
z.object({ kind: z.literal('file'), path: z.string().min(1) })
```

Aggiungere un **membro** all'unione è additivo: i payload esistenti continuano a
validare. È il contrario di quanto accaduto in 6a con `isPreview`, un **campo**
nuovo e obbligatorio che ha rotto tre criteri e2e della Fase 4b finché non ha
ricevuto `.default(false)`.

`contentKey` in `layout-invariants.ts` guadagna il caso corrispondente
(`file:${path}`), così due tab sullo stesso file restano un tab solo.

### Anteprima e promozione

Click singolo apre in **anteprima**, doppio click **promuove** a permanente —
identico al diff di 6a, e identico a `FileExplorerView` in Swift, che già fa tap
singolo per selezionare e doppio per aprire.

L'invariante `multiplePreviewTabs` introdotta in 6a copre già il tab file: un
gruppo ha al più un'anteprima, qualunque sia il suo contenuto.

### Lo stato di vista esiste già

`WorkspaceTabViewStateSchema` contiene `documentCaretOffset`,
`documentSelectionLength`, `documentScrollAnchor`, `documentFoldedRanges` ed
`editorMode` dalla Fase 4, nulli in attesa di questa fase. **Nessuna migrazione
di schema è necessaria** per lo stato dell'editor: i campi ci sono già.

## L'evidenziazione entra nel diff

`FileDiffSchema` guadagna due campi:

```ts
oldText: z.string().nullable().default(null),
newText: z.string().nullable().default(null)
```

Il `.default(null)` non è opzionale: lo schema viaggia sul socket di controllo,
quindi è un contratto di rete oltre che un tipo, e un campo obbligatorio nuovo
rifiuterebbe ogni chiamante esistente. La lezione di `isPreview` si applica qui
**prima** del danno invece che dopo.

In 6a questi due campi erano stati deliberatamente non portati, con la
motivazione scritta che servivano solo all'evidenziatore. È il punto previsto in
cui si riaprono.

Il renderer tokenizza `oldText` e `newText` con `highlightTree` e proietta gli
intervalli sulle righe. L'innesto è in **`DiffTab.svelte`**, dove oggi una riga è
un singolo nodo di testo:

```svelte
<span class="line-text">{row.left.text}</span>
```

che diventa una sequenza di span, uno per intervallo di token. La resa è
affiancata, quindi la colonna sinistra usa la mappa costruita su `oldText` e la
destra quella su `newText`: sono due file diversi e due tokenizzazioni diverse.

(`InlineSegments.svelte` non c'entra: è la barra dei tab dei gruppi di pane.)

**Quando i testi mancano** (file binario, oltre i limiti, file nuovo senza
versione precedente) il diff resta esattamente come oggi: righe non colorate,
struttura intatta. L'evidenziazione è un miglioramento, non una dipendenza.

## Errori

Ogni condizione ha il suo motivo mostrato, mai un albero vuoto o un tab bianco:

| Condizione | Cosa vede |
| --- | --- |
| percorso fuori dalla radice | `Path escapes the worktree root: <path>` |
| percorso non è una cartella | `Path is not a directory: <path>` |
| file illeggibile (permessi, sparito) | motivo di sistema, la riga resta nell'albero |
| file binario | `This file is binary and cannot be shown.` |
| file oltre i limiti | contenuto troncato con avviso esplicito |
| linguaggio non riconosciuto | testo senza colore, nessun errore |

## Test

### Unitari

- `icon-key.ts`: nome esatto prima dell'estensione (`Dockerfile`, `.gitignore`),
  estensione senza distinzione di maiuscole, ripiego su `file` per l'ignoto,
  ripiego su `folder` per le cartelle sconosciute.
- `language.ts`: estensione nota, estensione ignota, file senza estensione.
- `tree.ts`: elenca una sola cartella; esclude `.git`; rifiuta `../` e i percorsi
  assoluti fuori radice; distingue file, cartella e collegamento simbolico.
- `read.ts`: file di testo; file binario; file oltre il limite di righe; file
  oltre il limite di byte; file inesistente.
- `highlight.ts`: gli intervalli di un token che attraversa più righe finiscono
  su **tutte** le righe che tocca, con gli offset rimappati sull'inizio di ogni
  riga — è il caso che `LineHighlightMap` risolve e quello che una porta
  ingenua sbaglia.

### End-to-end

Tutti partono dal gesto dell'utente, mai da una chiamata al protocollo:

1. Espandere una cartella mostra i suoi figli, e non i figli delle sottocartelle.
2. Click su un file apre un tab in anteprima con il contenuto del file.
3. Doppio click promuove il tab; un successivo click singolo su un altro file
   sostituisce l'anteprima e lascia vivo il tab promosso.
4. Scrivere un file sul disco aggiorna la sua cartella nell'albero, e le altre
   cartelle espanse restano espanse.
5. Un file `.ts` aperto mostra token con colori distinti (parola chiave contro
   stringa), non un blocco monocromo.
6. Il diff di un file `.ts` modificato mostra gli stessi token colorati.
7. Un file binario apre un tab che ne spiega il motivo, senza byte grezzi.

## Fuori perimetro

- **Modifica e salvataggio**: 6b-2.
- **Editor markdown**, toolbar, anteprima: 6b-2.
- **Ricerca nei file** e ricerca globale: non pianificate.
- **Operazioni sui file** (creare, rinominare, eliminare dall'albero): non
  pianificate; l'albero di 6b-1 è di sola navigazione.
- **Tema di icone SF Symbols**: eliminato, vedi sopra.

## Dipendenze nuove

| Pacchetto | Versione | A cosa serve |
| --- | --- | --- |
| `codemirror` | 6.0.2 | meta-pacchetto dell'editor |
| `@codemirror/view` | 6.43.7 | vista, sola lettura in 6b-1 |
| `@codemirror/language` | 6.12.4 | `highlightTree` per file e diff |
| `@lezer/highlight` | 1.2.3 | tag dei token, tema |
| `@codemirror/lang-*` | correnti | grammatiche, una per linguaggio supportato |
| `@iconify-json/material-icon-theme` | 1.2.70 | icone impacchettate a build-time, stesso set di Swift |

`chokidar` è già presente dalla Fase 3 e non va aggiunto.
