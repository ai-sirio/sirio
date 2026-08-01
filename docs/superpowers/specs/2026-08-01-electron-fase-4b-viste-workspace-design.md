# Fase 4b — le viste del workspace

Data: 2026-08-01
Stato: approvata dall'utente, pronta per il piano di implementazione
Dipende da: Fase 4a (modello e motore del workspace), Fase 3 (stato degli agenti)

## Scopo

La Fase 4a produce il modello di layout di un worktree e la sua geometria
calcolata. La 4b disegna quel modello: albero della sidebar, barra superiore
con i tab, riquadri, barra inferiore, pannello degli agenti, stato vuoto, temi.

La 4b **non calcola geometria**. Nessuna delle viste chiama
`getBoundingClientRect` per sapere dove si trova qualcosa.

## Vincoli dell'utente

Fissati durante il brainstorming e non rinegoziabili qui. Il documento di
lavoro `fase-4-requisiti-ui.md` li riporta per esteso; qui la forma breve.

- **V1** — albero della sidebar a tre livelli, uniforme: un progetto con un
  solo worktree resta comunque espandibile.
- **V2** — barra superiore sottile in stile Orca, ~34 px, con i tab dentro la
  barra del titolo accanto ai semafori.
- **V3** — barra inferiore continua da bordo a bordo della finestra.
- **V4** — quote come barre di avanzamento, non testo concatenato.
- **V5** — `WorkspaceTabViewState` portato per intero, campi delle Fasi 5 e 6
  compresi (vincolo della 4a, qui solo consumato).
- **V6** — geometria esplicita, riquadri posizionati in assoluto.

## Decisioni sulle proposte aperte

Le sei proposte di `fase-4-requisiti-ui.md`, decise il 2026-08-01.

| | Decisione |
|---|---|
| **P1** — un solo stato vuoto | accolta |
| **P2** — azione dentro lo stato vuoto | accolta |
| **P3** — titolo finestra `<progetto> · <worktree>` | accolta |
| **P4** — icone della barra distinguibili | accolta |
| **P5** — pannello Agents fuso nell'albero | **respinta**: restano entrambi |
| **P6** — stato degli agenti sulle righe dell'albero | accolta |

### P5, e perché è stata respinta

La proposta partiva dal contenuto: il pannello dice meno dell'albero e della
tab bar, quindi è il più debole dei tre. Il ragionamento era corretto e la
conclusione sbagliata, perché il criterio giusto non è il contenuto ma la
copertura.

L'utente usa **entrambi**. Divisione del lavoro, da qui in avanti esplicita:

- **albero** — il worktree che si ha davanti, stato in contesto (P6);
- **pannello Agents** — tutti i worktree insieme, compresi i collassati e i
  chiusi.

Il difetto vero non era il pannello di troppo: era che il pannello si limitava
al worktree corrente, cioè faceva il lavoro dell'albero invece del proprio.

La terza occorrenza — `Chat` anche nella tab bar — non è ridondanza: è il
controllo con cui si cambia tab, non un rapporto di stato.

## Librerie

La domanda «non esiste già una libreria per questa roba» è stata verificata,
non risposta a memoria.

### Valutata e scartata: dockview

`dockview-core` 7.0.4, MIT, zero dipendenze, ~116k download/settimana, 3.3k
stelle. Fa tab, gruppi, split, drag&drop fra gruppi, finestre staccate,
serializzazione. È la libreria giusta per il problema generale.

Due fatti, letti dai `.d.ts` del pacchetto e non dalla documentazione:

```
dockview/dockviewGroupPanelModel.d.ts:29:   hideHeader?: boolean;
api/panelApi.d.ts:47-53:                    readonly width / readonly height
```

Gli header dei gruppi si possono nascondere. Ma l'API pubblica **solo le
dimensioni, non `x` e `y`**. E `npm view dockview-svelte` risponde 404: i
binding sono React, Vue e Angular; per Svelte esiste un repo di esempio da un
commit e due stelle.

Conseguenza per V2: si spengono le linguette di dockview per ridisegnarle nella
barra del titolo, ma i confini dei segmenti devono cadere sui divisori, e per
quello serve la `x` di ogni gruppo — che l'API non dà. Resterebbe
`getBoundingClientRect` sul DOM di dockview, cioè il reflow sincrono durante il
trascinamento che V6 esiste per evitare.

Esito: si userebbe la libreria **e** si scriverebbe comunque la parte difficile,
dovendo per giunta tenere allineati due modelli di geometria, uno dei quali non
è leggibile. Scartata.

Va aggiunto che il modello della 4a non è codice da progettare ma un **port**:
il reducer Swift esiste, con nove comandi e dodici invarianti con nome, senza
SwiftUI né IO. E lo schema di persistenza (`workspaceLayout`, `workspaceTab`,
`workspaceLayoutQuarantine`) è già stato portato dalla Fase 1; adottare dockview
significherebbe buttarlo per il suo JSON.

### Adottate

Due librerie strette, che prendono in carico l'interazione senza possedere il
modello:

| Cosa | Libreria | Perché |
|---|---|---|
| trascinamento dei divisori | `paneforge` 1.0.2 | vincoli di dimensione, tastiera, accessibilità: fiddly e già risolto |
| drag&drop dei tab | `svelte-dnd-action` 0.9.77 | non usa il dnd nativo del browser, che su Electron è la fonte principale di grattacapi |

## Architettura

### Tre viste, un modello

La 4a fornisce, per un worktree, l'albero di layout e la geometria calcolata:
`{x, y, w, h}` per ogni gruppo di pane.

Quattro consumatori della stessa geometria:

| Consumatore | Cosa legge |
|---|---|
| riquadri | il proprio rettangolo, posizionato in assoluto |
| segmenti della barra superiore | `x` e `w` del gruppo sottostante |
| bersagli di rilascio | i rettangoli, per decidere dove cade un tab trascinato |
| navigazione da tastiera | i rettangoli, per sapere qual è il riquadro «a destra» |

L'albero della sidebar **non** legge geometria: legge `allTabs` appiattito. È
l'unica delle viste indipendente dagli split, e questo la rende la più semplice
delle tre, non la più complessa come suggerirebbe la sua forma.

### File

```
src/renderer/workspace/
  PaneGrid.svelte        riquadri, posizionamento assoluto; `paneforge` per i divisori
  TopBar.svelte          barra titolo + segmenti tab (V2)
  TabSegment.svelte      un segmento = un gruppo; `svelte-dnd-action` per i tab
  SidebarTree.svelte     albero a tre livelli (V1)
  TreeRow.svelte         riga: progetto | worktree | tab + stato (P6)
  StatusBar.svelte       striscia a tutta larghezza (V3)
  QuotaBar.svelte        icona + barra + percentuale (V4)
  AgentsPanel.svelte     tutti i worktree (P5)
  EmptyState.svelte      un solo vuoto + bottone (P1, P2)

src/shared/
  status-rollup.ts       riduzione di molti stati a uno (vedi sotto)
```

## Barra superiore (V2)

### Meccanica di piattaforma

```js
new BrowserWindow({
  titleBarStyle: 'hiddenInset',
  trafficLightPosition: { x: 12, y: 10 }
})
```

- La striscia è `-webkit-app-region: drag`; **ogni figlio interattivo va
  marcato `no-drag`**. Sintomo se lo si dimentica: i tab si vedono, l'hover
  funziona, il click no. Sembra un bug di gestione eventi ed è una regione di
  trascinamento.
- Riserva a sinistra per i semafori: ~78 px, altrimenti il primo tab ci
  finisce sotto.
- Altezza della barra: 34 px.

### Dove atterra la striscia di un gruppo

V2 descrive il caso degli split verticali, dove ogni segmento sta sopra il
proprio riquadro. Con uno split orizzontale il gruppo inferiore non tocca la
barra del titolo. Regola generale adottata:

> **Ogni gruppo ha la sua striscia di tab sul proprio bordo superiore.** Se quel
> bordo è a `y = 0`, la striscia è disegnata dentro la barra del titolo.
> Altrimenti è disegnata inline, sopra il riquadro.

Stesso componente `TabSegment`, due contenitori diversi. Nessun caso speciale
nel modello: è la geometria della 4a a decidere dove atterra, esattamente come
per i riquadri.

L'alternativa scartata — «la barra del titolo mostra i tab del gruppo attivo» —
sembra più semplice ma introduce uno stato che il modello non ha (*quale gruppo
possiede la barra*), da tenere poi in sincronia con il focus, con la chiusura
dei gruppi e con gli split. È stato derivato che si può calcolare e che, se
memorizzato, prima o poi diverge.

Conseguenza gradita: con un solo gruppo — il caso normale — il bordo superiore è
a `y = 0`, quindi la finestra ha davvero una riga sola da 34 px. La seconda riga
compare solo splittando in orizzontale.

## Albero della sidebar (V1) e stato degli agenti (P6)

Tre livelli a forma fissa: progetto → worktree → tab. Il terzo livello è
`allTabs` appiattito.

Il pallino di stato sta su **ogni riga**, non solo sui tab: worktree e progetto
mostrano il rollup dei discendenti, sempre, anche da espansi. Una regola sola,
nessuna dipendenza dallo stato di apertura.

Precedenza del rollup, dalla più urgente:

```
needs-input  >  error  >  running  >  done  >  (niente)
```

`needs-input` batte `error` perché è l'unico stato in cui il lavoro è fermo
sull'utente **adesso**; l'errore è già concluso e resta lì.

Il pallino non distingue per solo colore: forma distinta per stato.

### Nota sulla provenienza

`highestPriority` / `statusForPanes` erano state scritte nel piano della Fase 3
e **cancellate durante l'auto-revisione**, perché in Fase 3 nessuno le chiamava.
Tornano qui con l'ordinamento dettato dal consumatore reale invece che
inventato. Scritte allora, avrebbero quasi certamente messo `error` prima di
`needs-input` — sembra più grave — e l'errore sarebbe emerso solo guardando la
sidebar con dieci worktree aperti.

Collocazione: `src/shared/status-rollup.ts`. È logica pura sugli stati, non sul
DOM, e i test la prendono senza avviare Electron.

## Barra inferiore (V3) e quote (V4)

Striscia continua da bordo a bordo, ~24 px, sotto tutto, sidebar compresa. La
riga inferiore separata che oggi la sidebar ha per sé (ingranaggio e aiuto)
sparisce e confluisce qui a destra.

Disposizione: a sinistra il contesto (`progetto · worktree`), a destra le quote,
poi ingranaggio e aiuto.

**Quote.** Un widget per agente — icona, una barra, percentuale — e la barra
mostra la finestra temporale **più vicina all'esaurimento**. Con
`37% 5h · 83% wk` la barra dice `83%`, la settimanale, perché è quella che
blocca per prima. Il dettaglio completo, entrambe le finestre più l'ora di
ripristino, sta nel tooltip.

Motivo: una barra di avanzamento comunica una sola cosa, quanto manca alla fine.
Mostrarne due affiancate obbliga chi guarda a calcolare il massimo, che è
l'unica operazione che poi compie.

**Da dove arrivano i dati.** Fuori ambito: la 4b disegna quello che il main le
passa. Il recupero delle quote (in Swift un fetcher OAuth diretto per Codex) è
una fase a sé. Se il main non pubblica quote per un agente, il widget **non
compare** — nessun segnaposto, nessuna barra a zero, che si leggerebbe come
«quota esaurita».

Questa riduzione ha la stessa forma del rollup dell'albero — molti valori, un
indicatore, una regola di precedenza dichiarata — e va scritta come **una sola
funzione generica** in `src/shared/status-rollup.ts` con due ordinamenti. Tenerle
separate le farebbe divergere in silenzio.

**Collocazione.** La striscia sta fuori dal contenitore che ospita i riquadri:
altrimenti la geometria della 4a, che copre l'area dei pane, dovrebbe conoscerne
l'altezza. La striscia è chrome della finestra, non del workspace.

## Stato vuoto (P1, P2)

Pannello destro e pannello agenti **nascosti**, non vuoti. Al centro un
messaggio solo, in due varianti perché le situazioni sono due:

| Situazione | Messaggio | Azione |
|---|---|---|
| nessun progetto | «Nessun progetto» | bottone **Aggiungi progetto** |
| progetti sì, worktree non selezionato | «Seleziona un worktree» | nessuna: l'azione è nella sidebar, già visibile |

Il `+` in alto a destra resta dov'è.

**Terzo caso: worktree selezionato ma senza tab.** Non è uno stato vuoto della
finestra ma di un gruppo, e la 4a non lo produce — chiudere l'ultimo tab di un
gruppo chiude il gruppo. Resta possibile solo per il worktree appena aperto,
dove la 4a crea un tab terminale iniziale. La 4b non disegna un terzo stato
vuoto: se ne trova uno, è un difetto della 4a, non un caso da coprire qui.

## Titolo della finestra (P3)

`<progetto> · <worktree>`, con ripiego su `Tiller` quando non c'è selezione.

## Icone della barra (P4)

Simboli distinti più tooltip. Nessuna scelta di design, solo un difetto da
chiudere.

## Pannello Agents (P5)

Globale su tutti i worktree, righe raggruppate per worktree. Cliccare una riga
porta a quel tab **anche se il worktree è chiuso**: lo apre e ci va.

### Perché qui è possibile e in Swift non lo era

Nell'app Swift `openWorktreeIds` decide quali host terminale restano montati, e
smontarne uno termina i suoi PTY: un pannello che elencasse agenti di worktree
chiusi elencherebbe processi morti.

La Fase 2 della migrazione ha spostato lo stato del terminale nel processo
principale — i pane vivono nel main e il renderer tiene solo quello visibile.
Un agente in un worktree chiuso è quindi ancora vivo e ancora interrogabile, e
la riga che lo mostra è vera.

Non è una funzione aggiunta: è una restrizione dell'architettura precedente che
è caduta. Va scritto, perché a chi legge il codice Swift sembrerebbe un
requisito cambiato senza motivo.

## Temi e trasparenza

Tre impostazioni: sistema, chiaro, scuro. Token come variabili CSS custom.

**Due meccanismi, non uno:** `nativeTheme.themeSource` per il chrome nativo
(semafori, menu contestuali) e l'attributo sul `<html>` per i token. Impostando
solo il secondo, in tema chiaro restano i semafori scuri e sembra un glitch di
rendering.

**Vibrancy solo sul chrome** — sidebar, barra superiore, barra inferiore — e
**non** sull'area dei riquadri. xterm.js disegna su `<canvas>`, che è opaco:
una vibrancy dietro un canvas opaco costa il blur della finestra senza che si
veda nulla. Nell'app Swift la scelta di `#1F1F26` con blur 20 aveva senso perché
lì il terminale è libghostty su Metal e il compositing è diverso.

La spec porta la decisione — superficie traslucida uniforme — non la costante.

## Criteri di verifica

Tutti e2e, su asserzioni del DOM e sulla geometria. **Niente confronto di
screenshot**: i diff di immagine falliscono per antialiasing e insegnano a
ignorare il gate, che è il modo peggiore in cui un controllo può rompersi.

| # | Criterio | Che cosa prende |
|---|---|---|
| 1 | un tab nella barra del titolo si clicca e cambia pane | la trappola `-webkit-app-region` |
| 2 | split verticale: i confini dei segmenti coincidono con le `x` dei divisori lette dalla 4a | che la barra non si inventi la geometria |
| 3 | split orizzontale: il gruppo inferiore ha la sua striscia inline | la regola di collocazione della striscia |
| 4 | worktree collassato con `running` + `needs-input` mostra `needs-input` | la precedenza del rollup |
| 5 | la barra inferiore è larga quanto la finestra | V3, oggi rotto in Tiller |
| 6 | il pannello Agents elenca un agente in un worktree chiuso, e cliccarlo lo apre | P5 |

Ognuno con **validazione per mutazione**, come in Fase 3. In particolare il
criterio 1: si toglie `no-drag` dal tab e il criterio deve diventare rosso. È
l'unico modo di sapere che prova davvero il click e non la presenza
dell'elemento.

## Fuori ambito

- Il modello e il motore del workspace: sono la Fase 4a.
- Il contenuto dei pane oltre al terminale (chat, editor, diff): Fasi 5 e 6.
  La 4b disegna il tab e il riquadro; che cosa ci vive dentro non la riguarda.
- I campi di `WorkspaceTabViewState` delle Fasi 5 e 6 (V5): la 4b li persiste
  attraverso la 4a e non li legge.
