# Fase 2 — Motore terminale (design)

Data: 2026-08-01
Fase precedente: [Fase 1 — fondamenta del main](2026-08-01-electron-fase-1-fondamenta-main-design.md)
Repo di destinazione: `~/Desktop/Progetti/tiller-electron` (il repo Swift non si tocca)

## Obiettivo

Portare il motore terminale da libghostty a xterm.js con parità d'uso, e
risolvere strutturalmente il problema di memoria dei pane nascosti che
nell'app Swift costava 497 MB di IOSurface.

Al termine della fase un pane terminale si usa davvero: si scrive, si
ridimensiona, si cerca nello scrollback, si clicca un link. Lo stato del
terminale sopravvive alla chiusura della finestra e al riavvio dell'app, ed è
leggibile da riga di comando. Gli split restano fuori: sono layout e
appartengono alla Fase 4.

## Contesto misurato

`Packages/TillerTerminal` nell'app Swift:

| | righe |
|---|---|
| Sorgente | 2.055 |
| Test | 1.627 |
| **Totale** | **3.682** |

I file portanti: `PtyTerminalPane.swift` (369, contiene `PtyRuntime`),
`SplitViewRenderer.swift` (284, **fuori scope**, è layout),
`PaneRegistry.swift` (193), `PtyProcess.swift` (191),
`ScrollbackBuffer.swift` (86), `TerminalPaneCache.swift` (64),
`SurfaceVisibility.swift` (33), `OutputSettleDebouncer.swift` (37).

Nell'app Electron esiste già, dalla Fase 0:

- `PtyManager` — spawn, write, kill, onData, onExit. **Nessun resize.**
- `createOutputCoalescer` — accumula l'output e lo consegna a lotti al renderer.
- `Terminal.svelte` — un xterm per pane con `FitAddon` e `WebglAddon`, e il
  seam di test `window.__tillerPaneBuffers`.

Manca tutto il resto: resize del PTY, scrollback, persistenza, titolo OSC,
ownership dei pane, tema, ricerca.

## Ricerca librerie

Verificata su npm il 2026-08-01.

| Pacchetto | Versione | Ultimo aggiornamento | Uso |
|---|---|---|---|
| `@xterm/headless` | 6.0.0 | 2026-07-27 | emulatore nel main |
| `@xterm/addon-serialize` | 0.14.0 | — | snapshot dello stato |
| `@xterm/addon-search` | 0.16.0 | — | ricerca nello scrollback |
| `@xterm/addon-unicode11` | 0.9.0 | — | larghezza emoji/CJK |
| `@xterm/addon-web-links` | 0.12.0 | — | link cliccabili |

`@xterm/xterm`, `@xterm/addon-fit` e `@xterm/addon-webgl` sono già
installati dalla Fase 0.

**Scartati.** `@xterm/addon-ligatures`: le legature si vedrebbero solo dentro
un TUI e costano frame di rendering. `@xterm/addon-image`: supporta Sixel e
iTerm2 IIP, **non** il protocollo grafico Kitty che ghostty implementa —
aggiungerebbe peso per un formato che i cinque agenti CLI non emettono.

### Misure fatte sul campo, non stimate

Sonda in `/tmp` con `@xterm/headless` 6.0.0 su questa macchina:

| Misura | Risultato |
|---|---|
| 30 pane × 2.000 righe × 120 colonne | **25,6 MB** totali, 0,85 MB a pane |
| `serialize({ scrollback: 1000 })` | 85.198 byte in **12 ms** |
| Throughput di parsing | **83,5 MB/s** |
| Titoli OSC 0 e OSC 2 nel headless | catturati, inclusi `✳ …` e `π - …` |
| Round-trip `serialize` → nuovo terminale | **identico byte a byte**, colori compresi |

Le ultime due righe sono le assunzioni portanti del design: il Layer B della
Fase 3 dipende dalla prima, la persistenza dalla seconda.

## Decisioni

### D1 — Confine della fase: motore più un pane singolo usabile

La roadmap assegnava alla Fase 2 anche gli split, ma la Fase 4 possiede il
`workspaceLayout`. Gli split sono layout: anticiparli qui significherebbe
scrivere due volte la stessa struttura ad albero.

Restano dentro: PTY resize, scrollback, persistenza, titolo OSC, ownership,
tema, selezione e copia, link cliccabili, ricerca.

*Alternative scartate.* Motore puro senza pixel: il rischio dichiarato della
fase è la parità di resa, e la resa non si falsifica headless. Tutto compresi
gli split: raddoppia il lavoro sull'albero di layout.

### D2 — Asticella di resa: essenziale più ricerca

Truecolor, box drawing, larghezza corretta di emoji e CJK (`unicode11`), link
cliccabili, ricerca nello scrollback. Niente legature, niente immagini.

La ricerca è l'unica delle tre voci opzionali che si usa ogni giorno, e
nell'app Swift **non esiste**: qui è parità superata, non raggiunta.

### D3 — Lo stato del terminale è del main (`@xterm/headless`)

Un `@xterm/headless` per pane nel processo main è la verità. L'xterm del
renderer è una vista alimentata dallo stesso flusso di byte.

*Alternativa scartata — parità Swift.* Emulatore solo nel renderer, ring
buffer di byte grezzi nel main. Meno codice, ma: il main non avrebbe uno
schermo e `pane.read` restituirebbe ANSI da ripulire a mano; il taglio del
ring buffer può spezzare una sequenza di escape a metà; i pane nascosti
dovrebbero restare montati.

*Alternativa scartata — renderer passivo.* Solo il main emula e spinge il
contenuto già reso. Ogni tasto premuto farebbe un giro processo→processo
prima di comparire a schermo.

L'argomento decisivo non è la memoria ma la **Fase 3**: chiudere la finestra
in Tiller non uccide gli agenti, quindi la detection deve continuare a
funzionare a finestra chiusa. Con l'emulatore nel renderer, in quello stato il
main possiede solo byte non interpretati. Coerente anche con la regola fissata
in Fase 0: *se lo persisti è del main*.

Costo accettato: gli stessi byte si analizzano due volte. A 83,5 MB/s un pane
che produce 5 MB/s consuma il 6% di un core.

### D4 — Il main possiede il contenuto, il renderer possiede la dimensione

Solo il renderer conosce i pixel, quindi calcola righe e colonne con
`FitAddon` e le manda al main con `pane.resize`. Il main le applica **insieme**
al PTY e al headless, così i due riflussi restano allineati e uno snapshot
serializzato non si sfalsa.

Senza finestra aperta — pane creato da `tillerctl` — il headless parte a 80×24
e si adatta appena un renderer si aggancia.

Il resize passa per un debouncer nel main, come `ResizeDebouncer` in Swift.
Trascinare il bordo della finestra produce un evento per frame, e ogni
`TIOCSWINSZ` fa ridisegnare da capo il TUI dell'agente: senza coalescing il
terminale sfarfalla per tutta la durata del trascinamento. Il PTY e il
headless si ridimensionano insieme, a trascinamento fermo.

### D5 — Pane nascosto: xterm vivo, contesto WebGL rilasciato

Il browser tiene un numero limitato di contesti WebGL per pagina (circa
sedici) e oltre quella soglia scarta il più vecchio, che perde il contenuto.
Con un `WebglAddon` per pane, oltre la dozzina di pane l'app comincerebbe a
sbiancarne alcuni da sola. Il criterio di uscita 4 è scritto per catturare
esattamente questo.

Il pane nascosto conserva il proprio xterm ma perde il contesto WebGL e passa
a `display:none`, che fa saltare al browser layout e paint. Il ritorno in
primo piano ricarica l'addon.

*Alternativa tenuta in riserva.* Smontare del tutto il pane nascosto e
ricostruirlo dallo snapshot al ritorno. Non richiede alcuna riprogettazione —
è possibile proprio perché il main è autoritativo — ma costa 12 ms a ogni
cambio di tab. Si adotta solo se la misura di memoria del criterio 4 sfora.

### D6 — Persistenza: snapshot serializzato, non byte grezzi

`serialize` emette lo stato del terminale come stringa ANSI già coerente,
mentre il ring buffer dello Swift persiste una finestra arbitraria di byte che
può cominciare a metà di una sequenza di escape.

### D7 — Throttle di persistenza a 30 secondi

Aggiunta rispetto allo Swift, che persiste solo alla chiusura del pane e al
quit. Costa 0,4 ms al secondo per pane e trasforma un crash da "perdo tutto
dall'avvio dell'app" a "perdo mezzo minuto".

## Architettura

Un solo PTY per pane nel main, e il suo output si dirama in due:

```
node-pty ──┬──> @xterm/headless (main)   → stato autoritativo:
           │                                schermo, scrollback, titolo OSC
           │
           └──> coalescer ──IPC──> xterm (renderer)  → pixel
```

L'input fa il giro opposto e non cambia dalla Fase 0: tasto nel renderer →
`pane.write` → stdin del PTY. Il headless non riceve mai input diretto: vede
solo ciò che il PTY riecheggia, come un terminale vero.

**La terza diramazione dello Swift sparisce.** Là il segnale di contenuto del
Layer C leggeva la coda del ring buffer e passava per `stripANSI()`, una
funzione che indovina dove finisce una sequenza di escape. Qui `pane.read` e
il futuro Layer C leggono `translateToString()` dal buffer del headless: il
testo è già pulito perché un parser vero l'ha interpretato.

### Errori

- **PTY morto**: il headless resta vivo e leggibile, così l'ultimo output di
  un agente crashato si può ancora leggere.
- **Renderer staccato**: il main continua a consumare l'output; alla
  riconnessione il renderer riceve uno snapshot, non il flusso perso.
- **Snapshot persistito illeggibile**: il pane parte vuoto e l'errore finisce
  nel log. Uno scrollback corrotto non deve impedire di aprire un terminale.

## Componenti

```
src/main/terminal/
  terminal-state.ts     un headless per pane: write, resize, readText,
                        readSnapshot, onTitle
  terminal-registry.ts  paneId → TerminalState; creazione, distruzione, elenco
  debounce.ts           coalescing a tempo; in questa fase serve al resize
                        (porta di ResizeDebouncer), in Fase 3 al Layer C
  scrollback-store.ts   lettura/scrittura di paneScrollback, coalescing
                        per sostituzione
src/main/pty/pty-manager.ts              + resize()
src/shared/protocol.ts                   + pane.resize, pane.read, pane.close,
                                           evento pane.title
src/renderer/src/lib/terminal-theme.ts   font, colori, translucenza
src/renderer/src/components/
  Terminal.svelte                        tema, resize→IPC, snapshot al mount,
                                           link cliccabili
  TerminalSearch.svelte                  barra di ricerca
cli/tillerctl.ts                         + read, resize, close
```

**Vincolo architetturale.** `src/main/terminal/*` non importa `electron`,
stesso vincolo di `src/main/control/dispatch.ts` e per la stessa ragione: il
motore del terminale si testa senza avviare un'app. Il headless gira in Node
puro.

Stima: ~700 righe di sorgente, ~900 di test.

**`TerminalPaneCache` non ha corrispettivo.** In AppKit esisteva perché un
pane spostato fra tab perdeva il proprio `NSHostingController` e con esso il
PTY. Qui il PTY e lo stato vivono nel main e il renderer è una vista
rimpiazzabile: spostare un pane fra tab è ricostruire una vista da uno
snapshot.

### Protocollo

Quattro voci nuove:

| Metodo | Parametri | Risposta |
|---|---|---|
| `pane.resize` | `paneId`, `cols`, `rows` | ok |
| `pane.read` | `paneId`, `format: 'text' \| 'snapshot'`, `lines?` | contenuto |
| `pane.close` | `paneId` | ok |
| `pane.title` (evento) | `paneId`, `title` | — |

`pane.read` prende un formato perché i due consumatori vogliono cose diverse e
reali: `text` restituisce il testo già interpretato (per `tillerctl` e, in
Fase 3, per il Layer C), `snapshot` la stringa ANSI di `serialize` (per
rimontare un terminale identico nel renderer).

### Ownership

Una sola regola, e vale la pena scriverla perché in Swift è la fonte di metà
dei bug di stato: **smontare la vista non uccide niente**. Un PTY muore solo
per `pane.close` esplicito, per la chiusura del worktree (Fase 4), o al quit
dell'app. Chiudere la finestra non tocca nessun agente — comportamento che
l'app Swift ha e che va conservato.

## Persistenza

La stringa di `serialize` va nella colonna `paneScrollback.data`. La tabella
esiste già dalla Fase 1 e la colonna è BLOB: **nessuna migrazione**.

| | |
|---|---|
| Scrollback vivo nel headless | 5.000 righe (~2,1 MB a pane) |
| Snapshot persistito | 1.000 righe ≈ 85 KB (tetto Swift: 256 KB) |
| Quando si scrive | chiusura del pane, quit dell'app, throttle a 30 s |

La coda di scrittura è **coalescing per sostituzione**, come
`ScrollbackQueue` in Swift: l'ultimo snapshot vince, una scrittura in volo per
volta.

Al riavvio il main ricostruisce il headless, ci scrive dentro lo snapshot e
**poi** avvia il PTY: la cronologia precede l'output nuovo. Il renderer, al
mount, chiede `pane.read` in formato `snapshot`.

## Test

Unità con Vitest su tutto ciò che non tocca Electron: `terminal-state`,
registry, settle, scrollback-store, protocollo.

Quattro criteri end-to-end, ciascuno scritto per poter fallire:

**1 — Lo stato vive nel main.** `tillerctl` crea un pane ed esegue un comando,
la finestra si chiude, `tillerctl read` restituisce l'output. Riaperta la
finestra, il terminale mostra lo stesso contenuto. Cade se lo stato scivola
nel renderer.

**2 — Sopravvive al riavvio.** Pane con output colorato, quit, riavvio: lo
scrollback c'è, con i suoi attributi. Cade se serialize o restore perdono
pezzi.

**3 — Il titolo OSC arriva al main.** Un comando emette `\e]0;✳ lavoro\a`;
`tillerctl state.get` riporta quel titolo sul pane. È la fondazione del Layer
B della Fase 3.

**4 — Venti pane, nessuno che sbianca.** Venti pane aperti, uno visibile per
volta: percorrendoli tutti, ognuno mostra il proprio contenuto — è il modo in
cui si manifesta l'eviction dei contesti WebGL. In più la RSS di main +
renderer deve restare **sotto 100 MB oltre la baseline** con venti pane da
2.000 righe. Misura attesa intorno ai 50 MB; sforare è il segnale per adottare
l'alternativa di D5, non per aprire un'indagine.

Il criterio 4 è l'unico che verifica una risorsa invece di una funzionalità, e
per questo è scritto in modo comportamentale: non conta i contesti WebGL,
guarda se un pane resta leggibile. Un test che contasse i contesti si legherebbe
alla scelta di implementazione e diventerebbe falso rosso il giorno in cui
cambiamo politica.

Le e2e girano sul build in `out/`, quindi la validazione per mutazione
richiede `electron-vite build` prima — la trappola che in Fase 1 ha prodotto
un falso verde.

Gate: `bash scripts/ci.sh` stampa `CI OK`.
