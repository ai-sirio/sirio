# Fase 6a — Diff

Design della prima metà della Fase 6 della migrazione a Electron.

## Obiettivo

Il pannello destro elenca i file cambiati; il click su un file apre una tab con
il suo diff. Fetta verticale completa: alla fine della fase il gesto
dell'utente produce il risultato, senza pezzi in attesa di una fase successiva.

## Cosa cambia rispetto a Tiller Swift

Questa fase non è un port fedele. Tre decisioni dell'utente cambiano la forma
di ciò che si costruisce, e vanno registrate perché il codice Swift letto come
riferimento **non** le contiene.

### 1. Il diff esce dal pannello ed entra nei tab

In Swift `RightPanelMode` ha tre casi — `files`, `diff`, `status` — e il diff è
una modalità del pannello laterale. Qui il pannello mostra solo l'elenco; il
diff è contenuto di tab.

La conseguenza è che il diff eredita ciò che le Fasi 4a/4b hanno già
costruito per i tab: split, riordino, persistenza del layout, ripristino al
riavvio, appartenenza al worktree. Un pannello laterale non ha niente di
questo, e avrebbe richiesto uno stato parallelo con la sua persistenza.

In cambio il diff deve entrare in `WorkspaceContentRef`
(`src/shared/workspace/layout-types.ts:18`), oggi unione discriminata con un
solo membro (`terminal`). Il costo lo paga il compilatore, che elencherà ogni
`switch` esaustivo da aggiornare.

### 2. `Diff` e `Status` si fondono in `Changes`

Il pannello ha due modalità invece di tre:

| Modalità | Origine | Azioni |
|---|---|---|
| `Files` | albero del worktree | — (Fase 6b) |
| `Changes` | non committato **oppure** ramo vs base | dipende dalla vista |

`Files` resta dichiarato nel selettore ma la sua implementazione è **Fase 6b**:
il suo unico gesto — apri un file — ha senso solo quando esiste un editor che
lo riceva. In 6a il selettore mostra `Files` disabilitato con la ragione.

### 3. `Changes` ha due sorgenti, una sola resa

Dentro `Changes` un secondo selettore sceglie cosa si confronta:

| Vista | Comando git | Azioni |
|---|---|---|
| **Non committato** | `git status` + `git diff` | stage / unstage / scarta |
| **Ramo** | `git diff <base>...HEAD` | nessuna (sola lettura) |

Le due viste condividono lista, rendering e tab del diff. Cambia l'argomento a
git e un flag che accende le azioni.

Lo staging esiste solo sulla prima vista, e non è una scelta estetica: sulla
vista Ramo quelle righe sono **già committate**, non c'è niente da mettere in
stage.

## Il confronto del ramo

`git diff <base>...HEAD` usa i **tre punti**, che non confrontano con la punta
di `<base>` ma con il **merge-base** — il punto in cui il ramo si è staccato.
Equivale a `git diff $(git merge-base <base> HEAD) HEAD`.

La differenza si manifesta solo quando `<base>` è avanzata dopo il fork, e
allora è totale: coi due punti comparirebbe, invertito, anche il lavoro altrui
finito su `<base>`, attribuito a questo ramo.

### Da dove viene `<base>`

`WorktreeTable` (`src/main/db/schema.ts`) **non** ha una colonna per il ramo
base, e `addWorktree` accetta `base` senza persisterlo. Non la si aggiunge:
git sa rispondere da sé, e ricalcola quando la storia cambia, mentre un campo
salvato invecchierebbe in silenzio. Vale anche per i worktree creati fuori da
Tiller, che un campo nostro non coprirebbe comunque.

Risoluzione, in ordine:

1. `origin/HEAD` — il ramo di default dichiarato dal remoto
2. `main`, poi `master`, se esistono localmente
3. nessuno → la vista Ramo si spiega invece di mostrare una lista vuota

L'intestazione mostra `<base> → <ramo corrente>` ed è un **selettore**: la
scelta vale per il worktree corrente, non globalmente, così due worktree su
basi diverse non si pestano. I rami da elencare arrivano da `listBranches`,
già portata in Fase 1.

La base scelta **non si persiste**: torna al default a ogni avvio. Evita una
migrazione di schema per una preferenza che si reimposta in due click. Se
risulterà fastidiosa diventerà una colonna, non un rifacimento.

## Il pannello segue la sidebar

Il pannello mostra il worktree selezionato nella sidebar sinistra: da lì
vengono `HEAD` e il percorso del repo. È già il comportamento di
`RightPanelModel.activate(worktree:)` in Swift.

Le tab del diff appartengono al worktree, non alla finestra: cambiare
selezione smonta un host e ne monta un altro, e la tab sparisce e riappare
col suo worktree senza codice dedicato.

## Architettura

```
main                                        renderer
────                                        ────────
git/diff.ts        load(...)  →  FileDiff   RightPanel
  └ parse-diff  →  DiffLine[]                 ├ selettore  Files | Changes
git/merge-base.ts  resolveBase()              └ Changes
git/directory-status.ts  rollup                    ├ selettore  Non committato | Ramo
git/watch.ts       chokidar + debounce             └ lista file  +N −M
                                                          │ click
shared/diff/side-by-side.ts                               ▼
  └ rows(DiffLine[]) → Row[]                          DiffTab
```

Il confine è quello di sempre: `src/main/` parla con git e col filesystem,
`src/shared/` contiene le funzioni pure e gli schemi, `src/renderer/` disegna.

### Chi decide cosa è cambiato

**Git.** `parse-diff` (npm) legge l'unified diff prodotto da git; non lo
ricalcola. Una libreria che ridiffa i due testi lato client produce un
risultato plausibile ma diverso: niente rilevamento dei rename, e i casi
binario e submodule degradano a "file intero cambiato".

Questo esclude `@codemirror/merge` e `diff2html` per il calcolo. `@codemirror/
merge` resta interessante come *resa* ma ricalcola il diff da sé, e i gesti
vanno innestati in un componente pensato per l'editing.

### Modello dati

Porta i tipi Swift di `Packages/TillerGit/Sources/TillerGit/GitDiff.swift`:

```ts
type DiffLineKind = 'metadata' | 'hunk' | 'context' | 'addition' | 'deletion'

interface DiffLine {
  kind: DiffLineKind
  oldLineNumber: number | null
  newLineNumber: number | null
  text: string
}

interface FileDiff {
  path: string
  lines: DiffLine[]
  additions: number
  deletions: number
  isBinary: boolean
  isSubmodule: boolean
}
```

`oldText`/`newText` del corrispettivo Swift **non** si portano in 6a: là
servono solo ad alimentare l'evidenziatore tree-sitter, e l'evidenziazione
sintattica è Fase 6b. Aggiungerli ora sarebbe portare un campo per un
consumatore che non esiste.

### La funzione pura

`shared/diff/side-by-side.ts` porta le 50 righe di
`GitDiffSideBySide.rows`: accoppia ogni sequenza di cancellazioni con la
sequenza di aggiunte che la segue, il contesto compare su entrambi i lati, le
intestazioni di hunk occupano tutta la larghezza, i metadata si scartano.

```ts
interface SideBySideRow {
  left: DiffLine | null
  right: DiffLine | null
}

function rows(lines: DiffLine[]): SideBySideRow[]
```

Pura, senza I/O, con un solo tipo in ingresso e uno in uscita: è il pezzo su
cui il TDD e la verifica per mutazione rendono di più.

### Limiti

Porta i limiti di `GitDiff.outputLimits`: **5 MB** e **20.000 righe**. Oltre,
il diff non si carica e la tab mostra il motivo. Senza limite un file
generato di 200 MB blocca la UI mentre la si parsifica.

## La tab del diff

`WorkspaceContentRef` acquista un membro:

```ts
z.object({ kind: z.literal('diff'), path: z.string().min(1) })
```

### Anteprima e promozione

Il click singolo apre in una tab **provvisoria**, che il click successivo
rimpiazza; il doppio click la rende permanente. Modello VS Code.

Costo, da pagare per intero: `WorkspaceTab` acquista `isPreview: boolean`, il
codec una colonna, e gli invarianti una regola — **al massimo una tab in
anteprima per gruppo**.

L'ordine di costruzione è invertito rispetto al solito: **prima** l'invariante
e il suo test di violazione, **poi** il campo, **poi** la UI.
`src/shared/workspace/layout-invariants.ts` ha già un tipo somma di violazioni
(`duplicateContentOwnership`, `unknownTab`, …): aggiungere `multiplePreviewTabs`
lì fa sì che ogni fixture e ogni test di round-trip esistente cominci a
controllarlo gratis.

Nell'ordine opposto esisterebbe una finestra in cui il layout può salvare due
anteprime, e quello stato finirebbe **persistito su disco**, dove nessun
controllo successivo lo troverebbe.

## Il watcher

`chokidar` (nuova dipendenza: FSEvents su macOS, ricorsivo, maturo) più il
`createDebouncer` che esiste già in `src/main/terminal/debounce.ts`.

Sopra va il debounce **adattivo** portato da `AdaptiveDebounce`
(`App/RightPanel/RightPanelModel.swift:14`): il ritardo cresce sotto raffica e
si azzera alla quiete, così un `npm install` dentro il worktree non ricarica
la lista quaranta volte.

Esclusioni: `.git/` (tranne `HEAD` e `index`, che segnalano commit e staging),
`node_modules/`, e le directory ignorate da git.

## Errori

Ogni caso ha una risposta leggibile; nessuno degrada in lista vuota.

| Caso | Comportamento |
|---|---|
| non è un repo git | `Changes` disabilitato con la ragione, come `RightPanelMode.requiresGit` |
| nessun merge-base | la vista Ramo spiega perché, non mostra zero file |
| diff oltre i limiti | la tab dice "troppo grande", la UI non si blocca |
| file binario | riga sola: "file binario", niente parsing |
| submodule | riga sola col vecchio e nuovo commit |
| git assente o fallisce | il messaggio di git risale alla UI, non uno stack trace |

## Test

### Unitari

- `rows()`: contesto su entrambi i lati, cancellazioni zippate con le aggiunte
  che seguono, sequenze di lunghezza diversa, hunk a tutta larghezza, metadata
  scartati, ingresso vuoto
- parsing: rename, binario, submodule, file nuovo, file cancellato, CRLF
- `resolveBase()`: `origin/HEAD`, ripiego su `main`/`master`, nessuna base
- rollup di stato per cartella
- debounce adattivo: il ritardo cresce sotto raffica, si azzera alla quiete
- invariante `multiplePreviewTabs`, e round-trip del codec col nuovo campo

### End-to-end

I criteri partono dal **gesto dell'utente**, non dal comando che apre la tab.
È la lacuna che la Fase 4b ha scoperto: quindici task costruivano consumatori
e nessuno il produttore, e i test passavano lo stesso.

1. In un worktree con un file modificato, `Changes` lo elenca con `+N −M`
   corretti.
2. Il **click** sulla riga apre una tab il cui contenuto mostra le righe
   cambiate di quel file.
3. Un secondo click su un altro file **rimpiazza** la tab provvisoria; dopo un
   doppio click la tab resta e il click seguente ne apre una nuova.
4. La vista Ramo su un ramo con due commit elenca i file di entrambi, e **non**
   elenca un file toccato solo su `<base>` dopo il fork.
5. Le azioni di stage esistono nella vista non committato e **non** in quella
   Ramo.
6. Un file modificato da fuori dell'app compare nella lista senza interazione
   (il watcher), entro il ritardo del debounce.

Ogni criterio va verificato per mutazione: neutralizzare ciò che dovrebbe
provare, e vederlo diventare rosso. Un criterio che sopravvive alla rimozione
di ciò che verifica sta verificando qualcos'altro.

Trappole già pagate nelle fasi precedenti, da non ripagare:

- `pnpm build` include il typecheck: una mutazione che non compila lascia
  Playwright sul `out/` vecchio e il test passa a vuoto. Costruire con
  `npx electron-vite build`.
- Mai mutare su una base rossa: il rosso non sarebbe attribuibile.
- Asserire il **verdetto**, non il dato in ingresso: un'asserzione che aggancia
  ciò che il test stesso ha fornito passa qualunque cosa il codice decida.

## Fuori perimetro

- **Albero dei file** (`Files`): Fase 6b, insieme agli editor che ne ricevono
  il click
- **Evidenziazione sintattica** dentro il diff: Fase 6b, con CodeMirror, così
  l'app ha un solo motore di evidenziazione invece di due
- **Sezione Agents** del pannello (`App/RightPanel/AgentsSectionView.swift`):
  dipende dallo stato della chat, quindi dopo la Fase 5
- **Diff intra-riga** (le parole cambiate dentro una riga modificata): non
  esiste in Swift, non si aggiunge qui
- **Commit dall'app**: fuori dalla migrazione

## Dipendenze nuove

| Pacchetto | Perché | Alternativa scartata |
|---|---|---|
| `parse-diff` | parser di unified diff maturo | scriverlo a mano |
| `chokidar` | watcher fs ricorsivo su FSEvents | `fs.watch` grezzo |

Nessuna delle due entra nel renderer: vivono in `src/main/`.
