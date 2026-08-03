# Fase 4a — Motore del workspace (migrazione Electron)

**Data:** 2026-08-01
**Repo di destinazione:** `~/Desktop/Progetti/tiller-electron`
**Origine (sola lettura):** `~/Desktop/Progetti/tiller` (Swift)

## Obiettivo

Portare il modello e il motore del layout del workspace: albero dei riquadri,
gruppi di pane, tab, divisori, geometria, bersagli di rilascio, navigazione
spaziale, persistenza. Tutto verificabile da `tillerctl`, senza interfaccia.

**Fuori scopo** — vanno in Fase 4b: le tre viste che disegnano il layout
(albero della sidebar, barra superiore con le tab strip, riquadri), la
gestualità (sessione di trascinamento, inseguimento del divisore col mouse) e i
temi.

## Contesto misurato

### Origine Swift

| Componente | Righe |
|---|---|
| `TillerCore/Workspace/` (modello, riduttore, invarianti, transizione) | 1 188 |
| `TillerWorkspace/` (geometria, rilascio, vicini, divisori, gestualità) | 1 850 |
| `App/Workspace/` (coordinatore, persistenza SQLite, menu, migrazione V15) | 3 184 |

Nota: `SplitTree.swift` è il **modello legacy**, superato. È referenziato solo
da `WorkspaceMigrationV15` (la migrazione che lo abbandona), da
`LegacyWorkspaceTab` e da `SplitViewRenderer`. Il modello vivo è
`TillerCore/Workspace/`. **Non portare `SplitTree`.**

### Già disponibile in Electron

Le tabelle `workspaceLayout`, `workspaceTab` e `workspaceLayoutQuarantine`
esistono già nello schema (`src/main/db/schema.ts`): la Fase 1 le ha portate.
La persistenza non va progettata da zero, e non serve alcuna migrazione V15 —
il database Electron nasce con lo schema finale.

## Il modello

```
LayoutNode  = group(PaneGroupID)
            | split(id: SplitID, axis, fraction: number, first: LayoutNode, second: LayoutNode)

PaneGroup   = { id: PaneGroupID, tabs: WorkspaceTab[], activeTabId: WorkspaceTabID | null }

WorkspaceLayout = { root: LayoutNode,
                    groups: Map<PaneGroupID, PaneGroup>,
                    activeGroupId: PaneGroupID }
```

**La foglia dell'albero non è un riquadro singolo: è un gruppo che contiene più
tab.** Da qui discendono due cose visibili nel prodotto: ogni riquadro ha la
propria barra dei tab, e l'albero della sidebar mostra `allTabs`, cioè i tab di
tutti i gruppi appiattiti in ordine di visita.

`WorkspaceTab` porta `id`, `title`, `titleIsAutoNamed`, `content`
(`WorkspaceContentRef`) e `viewState`.

`WorkspaceTabViewState` si porta **per intero** (decisione dell'utente), inclusi
i campi che serviranno solo alle Fasi 5 e 6: posizione del cursore, lunghezza
della selezione, ancoraggio dello scroll, righe piegate, modalità dell'editor,
bozza del composer della chat, riferimenti agli allegati, ancoraggio della
trascrizione, `followsTail`, ancoraggio del viewport del terminale. I campi
inutilizzati restano definiti e persistiti, così le fasi successive non devono
migrare lo schema.

## Decisioni

**D1 — La logica pura sta in `src/shared/workspace/`, non in `src/main/`.**
Durante un trascinamento il bersaglio di rilascio va ricalcolato a ogni
movimento del mouse. Attraversare il confine IPC decine di volte al secondo
farebbe inseguire l'anteprima al cursore con ritardo visibile. Il renderer
anticipa localmente con le funzioni pure; al rilascio manda **un solo** comando
al main, che possiede il layout autorevole. Se il main lo rifiuta, il renderer
torna allo stato pubblicato: non deve saper annullare nulla.

**D2 — Uno stato illegale non deve essere costruibile.** `WorkspaceLayout` è un
tipo brandizzato senza costruttore esportato; l'unica via è `makeLayout()`, che
valida gli invarianti e restituisce un'unione discriminata su `ok`. Porta la
scelta dello Swift, dove `WorkspaceLayout.make` è l'unico costruttore pubblico.

**D3 — I tre tipi di identificatore restano distinti.** `PaneGroupID`,
`SplitID` e `WorkspaceTabID` diventano stringhe brandizzate, non `string` nudo.
Sono tre identificatori in volo contemporaneamente nelle stesse firme:
scambiarli è un errore che deve prendere il compilatore.

**D4 — Geometria esplicita, posizionamento assoluto.** I riquadri non si
dispongono con flexbox annidato: si percorre l'albero una volta e si ottiene
`{x, y, w, h}` per ogni gruppo. Quattro consumatori diversi hanno bisogno degli
stessi numeri — i riquadri, i segmenti della barra superiore, i bersagli di
rilascio, la navigazione spaziale. Lasciarli impliciti nel motore di layout del
browser costringerebbe a `getBoundingClientRect`, cioè a un reflow sincrono
proprio durante il trascinamento. Lo Swift conferma: `DropTargetResolver` e
`SpatialNeighbors` lavorano già su `CGRect`.

**D5 — La persistenza segue la classificazione dei comandi, non un timer
unico.** `insertTab`, `splitGroup`, `moveTab` e `closeTab` sono **strutturali**:
si scrivono subito, perché perderli a un crash significa perdere lavoro.
`activateTab`, `activateGroup`, `setPreferredFraction`, `updateViewState` e
`renameTab` non lo sono: si accumulano e si scrivono con calma, perché
trascinare un divisore ne produce centinaia al secondo. Lo Swift ha una sola
funzione per questa distinzione, dichiarata *"the one source of truth"*: si
porta come tale.

**D6 — Un layout corrotto non fa cadere l'app e non sparisce.** Va in
`workspaceLayoutQuarantine` con il motivo; il worktree riparte con un layout
vuoto valido; la tabella conserva solo gli ultimi record.

**D7 — I risolutori puri stanno in 4a, la gestualità in 4b.** Calcolare *quale*
sia il bersaglio di rilascio dato un punto è geometria testabile senza schermo.
Seguire il mouse, riconoscere l'inizio di un trascinamento oltre la soglia e
disegnare l'anteprima è stato temporaneo legato agli eventi.

**D8 — Niente `SplitTree` e niente migrazione V15.** Sono l'eredità che l'app
Swift sta abbandonando; il database Electron nasce con lo schema finale.

## Architettura

```
src/shared/workspace/            logica pura, usata da main e renderer
  layout-types.ts                LayoutNode, PaneGroup, WorkspaceTab, viewState (Zod)
  layout-ids.ts                  PaneGroupID, SplitID, WorkspaceTabID brandizzati
  layout-invariants.ts           validate() → errore con nome | null
  layout-engine.ts               apply(comando, layout) → Result<Transizione, Errore>
  layout-metrics.ts              costanti misurate (sotto)
  layout-geometry.ts             rects(layout, contenitore) → Map<PaneGroupID, Rect>
  drop-target.ts                 resolve(punto, rects) → DropTarget
  spatial-neighbors.ts           neighbor(da, direzione, rects) → PaneGroupID | null

src/main/workspace/
  layout-store.ts                carica e salva per worktree, con quarantena
  layout-persistence-policy.ts   strutturale → subito, resto → ritardato
```

Nessun file sotto `src/shared/workspace/` importa `electron`, `node:net` o
`node:fs`.

## Il riduttore

```ts
apply(comando, layout) → { ok: true, layout, delta, focusIntent }
                       | { ok: false, error }
```

### I nove comandi

`insertTab(tab, into, index?, activate)`,
`splitGroup(anchor, placement, newGroup, newSplit, content)`,
`moveTab(tabId, to)`, `closeTab(tabId)`, `activateTab(tabId)`,
`activateGroup(groupId)`, `setPreferredFraction(splitId, fraction)`,
`updateViewState(tabId, viewState)`, `renameTab(tabId, title, isAutoNamed)`.

`placement` ha **quattro** casi (`left`, `right`, `above`, `below`), non due:
il menu *Sposta tab in…* espone destinazioni esatte e l'anteprima di bordo
risolve il più vicino dei quattro. Collassare sinistra in destra dividerebbe
silenziosamente dal lato sbagliato dell'ancora.

`content` di `splitGroup` è `newTab(tab)` oppure `existingTab(tabId)`;
`to` di `moveTab` è `group(groupId, index)` oppure
`newSplit(anchor, placement, newGroup, newSplit)`.

### I dodici invarianti del riduttore

`emptyGroupRegistry`, `orphanGroup`, `unresolvedGroupLeaf`, `duplicateID`,
`emptyNonRootGroup`, `unknownActiveGroup`, `activeTabNotInGroup`,
`invalidFraction`, `duplicateContentOwnership`, `unknownGroup`, `unknownTab`,
`illegalSplitOfSoleTab`.

Ogni violazione ha un nome proprio: un errore che dice *quale* regola è stata
infranta è diagnosticabile, uno che dice "layout non valido" no.

Nello Swift l'enumerazione degli errori ne contiene un tredicesimo,
`staleRevision`, che **non appartiene al riduttore**: è la guardia della
persistenza descritta più sotto. Il riduttore non conosce revisioni.

### La transizione

Non solo il layout nuovo:

- **`delta`** — cosa è cambiato: tab inseriti, rimossi, spostati; gruppi
  inseriti, rimossi, riordinati; split inseriti, collassati; cambi di gruppo
  attivo, di tab attivo per gruppo, di frazione; e `isStructural`.
- **`focusIntent`** — `none`, `focusTab(id)` o `focusDivider(id)`.

**Il focus è calcolato dal riduttore, non deciso dalla vista.** Chiudendo un tab
è il riduttore a dire quale prende il fuoco. Sparpagliare quella logica nei
gestori di evento significa vederla contraddirsi.

## Geometria

### Costanti misurate

| Costante | Valore |
|---|---|
| Dimensione minima di un gruppo | 240 × 160 |
| Banda del divisore (afferrabile) | 6 px |
| Filo del divisore (dipinto) | 1 px |
| Altezza della barra dei tab | 32 px |
| Banda di bordo per il rilascio | 22% |
| Soglia di trascinamento | 4 px (usata in 4b) |

Banda e filo sono **due numeri diversi per due scopi diversi**: la banda resta
larga perché il divisore sia afferrabile, ma si dipinge solo il filo, perché una
banda piena si leggerebbe come un corridoio fra i riquadri invece che come una
giuntura.

### Calcolo

`rects(layout, contenitore)` percorre l'albero: un `split` con asse orizzontale
e frazione `f` assegna al primo figlio `w · f − banda/2` e al secondo il resto;
verticale fa lo stesso sull'altezza. Un gruppo prende il rettangolo che riceve.

Uno split che porterebbe un gruppo sotto la dimensione minima va **rifiutato dal
riduttore** (`illegalSplitOfSoleTab` copre il caso del tab unico; il caso della
dimensione si controlla prima di emettere il comando, perché il riduttore non
conosce la dimensione della finestra).

### Bersagli di rilascio

```
punto nella fascia alta di 32 px  → tabStrip(gruppo, indice di inserimento)
punto entro il 22% da un bordo    → edge(gruppo, left | right | above | below)
punto nel resto del gruppo        → center(gruppo)
punto fuori da ogni gruppo        → none
```

### Vicini spaziali

`neighbor(da, direzione, rects)`: fra i gruppi che stanno nella direzione
richiesta e si sovrappongono sull'asse perpendicolare, il più vicino. Serve alla
navigazione da tastiera fra riquadri.

## Persistenza

Un layout per worktree. Alla scrittura:

| Classe di comando | Politica |
|---|---|
| strutturale (`insertTab`, `splitGroup`, `moveTab`, `closeTab`) | scrittura immediata |
| non strutturale (gli altri cinque) | accumulo e scrittura ritardata |

### Revisione: una scrittura vecchia non deve sovrascriverne una nuova

Ogni record di layout porta una **revisione** crescente. Una scrittura la cui
revisione non è maggiore di quella già memorizzata viene scartata
(`staleRevision`). Serve perché le scritture non strutturali sono ritardate: una
scrittura accodata può arrivare al database dopo una strutturale più recente, e
senza questa guardia riporterebbe indietro il layout.

La revisione **non** vive dentro `WorkspaceLayout` — è un campo del record
persistito. Il riduttore resta ignaro del tempo e della durabilità.

### Caricamento

Se il record non si interpreta o viola un invariante:

1. la riga finisce in `workspaceLayoutQuarantine` con il motivo;
2. il worktree riparte con `emptyLayout()`;
3. la tabella di quarantena conserva solo gli ultimi record;
4. l'app resta viva e lo segnala nei log, non all'utente.

### Il sidecar di recupero: non si porta

L'app Swift affianca al database un file di recupero che può contenere una
revisione **più nuova** di quella nel database, e la reimporta all'avvio. Copre
un guasto diverso dalla corruzione: la scrittura sul database persa.

**Decisione: non portarlo in 4a.** La scrittura immediata dei comandi
strutturali (D5) riduce già la finestra di perdita a una singola operazione, e
la quarantena copre la corruzione. Un secondo percorso di durabilità aggiunge un
modo in più di essere incoerenti — due fonti che possono divergere — prima che
si sia osservato il guasto che dovrebbe risolvere. Se emergerà, si aggiunge
allora, con il caso reale sotto gli occhi.

## Protocollo

| Aggiunta | Forma |
|---|---|
| `ControlRequest` | `workspace.get` — `{ worktreeId }` |
| `ControlRequest` | `workspace.apply` — `{ worktreeId, command }` |
| `StateEvent` | `workspace.layout` — `{ worktreeId, layout, delta }` |
| `tillerctl` | `workspace get --worktree <id>`, `workspace apply --worktree <id> --command <json>` |

Il `delta` viaggia nell'evento perché il renderer sappia cosa è cambiato senza
confrontare due alberi.

`ControlRequest` e `StateEvent` sono unioni discriminate su cui il dispatcher e
il renderer fanno `switch` esaustivo: **le aggiunte al protocollo e i rami che
le gestiscono vanno nello stesso task**, o il compilatore rifiuta il taglio.

## Errori

| Caso | Comportamento |
|---|---|
| comando che viola un invariante | respinto con l'errore per nome; il layout resta intatto |
| layout persistito illeggibile o non valido | quarantena, worktree con layout vuoto, app viva |
| `workspace.apply` su un worktree sconosciuto | risposta d'errore, nessuno stato creato |
| split che porterebbe un gruppo sotto 240 × 160 | rifiutato prima di emettere il comando |

Un comando respinto **non** richiede al renderer di annullare: lo stato
autorevole non è cambiato, quindi basta ridisegnare quello pubblicato.

## Verifica

### Unità

Ogni modulo di `src/shared/workspace/` è una funzione pura o una classe senza IO:
si prova senza DOM, senza database e senza tempo. In particolare ognuno dei
tredici invarianti ha un test che lo viola di proposito e ne verifica il nome.

### e2e (da `tillerctl`, senza interfaccia)

1. **Uno split sopravvive al riavvio.** Dividi, chiudi l'app, riapri:
   `workspace get` restituisce lo stesso albero.
2. **Cento trascinamenti del divisore non fanno cento scritture.** Conteggio
   reale delle scritture sul database, non un'asserzione sul codice.
3. **Un layout corrotto finisce in quarantena e l'app resta viva.** Si scrive
   spazzatura nella riga del layout e si riavvia: il worktree riparte vuoto, la
   riga è in quarantena con il motivo, nessun crash.
4. **Un comando che viola un invariante viene respinto senza toccare il
   layout.** `workspace get` prima e dopo restituisce lo stesso albero.

Il terzo è il più importante: è l'unico che esercita il percorso di fallimento,
ed è quello che decide se un layout corrotto costa una sessione o l'app intera.

## Rischi aperti

- **La dimensione minima non è conoscibile dal riduttore**, che non sa quanto è
  grande la finestra. Il controllo sta a monte, in chi emette il comando — cioè
  in 4b. Se 4b lo dimentica, si ottiene uno split valido per gli invarianti ma
  illeggibile a schermo.
- **`WorkspaceContentRef` non è ancora definito in Electron.** Riferisce il
  contenuto di un tab (terminale, chat, documento). La Fase 2 ha introdotto
  `terminalContentId`; le Fasi 5 e 6 aggiungeranno gli altri casi. In 4a va
  definito come unione aperta con il solo caso terminale popolato.
- **La riconciliazione tra layout e viste vive** (`WorkspaceReconciler`, 198
  righe in Swift) è la parte che decide quali riquadri riusare e quali
  ricostruire quando l'albero cambia. In Electron il problema cambia forma —
  Svelte riconcilia da sé se le chiavi sono stabili — ma la scelta delle chiavi
  è esattamente ciò che decide se un terminale sopravvive a uno split. Va
  affrontata in 4b con la stessa attenzione che in Fase 2 ha richiesto il pane
  nascosto.
