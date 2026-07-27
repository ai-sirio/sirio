# Piano 001: migliorare performance e fluidità di Tiller

> **Istruzioni per l'esecutore**: seguire il piano fase per fase. Eseguire ogni
> verifica e confermare il risultato atteso prima di continuare. Se si verifica
> una delle condizioni nella sezione "STOP conditions", fermarsi e riferire il
> problema senza improvvisare. Non sovrascrivere né annullare modifiche già
> presenti nel working tree.
>
> **Drift check iniziale**:
>
> ```bash
> git status --short
> git diff --stat 9ec2a6b..HEAD -- App Packages AppTests
> ```
>
> Il piano è stato preparato su `9ec2a6b` con modifiche locali già presenti.
> Prima di ogni fase confrontare il codice attuale con la sezione "Stato
> corrente". Se i simboli o le invarianti non corrispondono più, applicare la
> relativa STOP condition.

## Stato

- **Priorità**: P1 per chat, persistenza e pipeline terminale; P2 per right panel e snapshot; P3 per bootstrap e polling usage
- **Effort complessivo**: L, suddiviso in fasi indipendenti S/M
- **Rischio**: MED
- **Dipende da**: nessun altro piano; la baseline interna di questo documento precede ogni ottimizzazione
- **Categoria**: performance
- **Pianificato su**: commit `9ec2a6b`, 2026-07-26

## Perché è importante

Tiller ha già risolto diversi costi strutturali della pipeline terminale, ma
durante lo streaming ACP continua a ricostruire l'intero messaggio Markdown e
il relativo layout TextKit sul `MainActor`. Con transcript lunghi, la
persistenza codifica e riscrive inoltre l'intera conversazione in modo
sincrono. Sotto output terminale intenso restano append byte-per-byte nello
scrollback e scansioni di processo ridondanti. Questi costi possono causare
frame persi, input lag e ritardi percepibili al termine di un turno o durante
burst di modifiche filesystem.

Il piano conserva le invarianti fondamentali del prodotto: agenti e terminali
nascosti continuano a funzionare, nessun output viene perso e le risposte
completate mantengono l'attuale resa Markdown.

## Stato corrente

### Ottimizzazioni già presenti e da conservare

- `Packages/TillerTerminal/Sources/TillerTerminal/PtyProcess.swift` usa già un
  buffer di lettura PTY riutilizzabile da 64 KB.
- `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`
  estrae soltanto gli ultimi 10 KB per il content signal.
- `Packages/TillerTerminal/Sources/TillerTerminal/SurfaceVisibility.swift`
  propaga la visibilità alla superficie libghostty e ferma il display link dei
  pane nascosti.
- `App/Chat/AgentMarkdownTextView.swift` memorizza l'altezza TextKit per testo e
  larghezza invariati.
- `App/Chat/ChatController.swift` raggruppa gli eventi ACP e applica
  backpressure con un intervallo adattivo di 40-500 ms.
- `Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift`
  limita lo scrollback ausiliario a 256 KB.

Queste soluzioni non devono essere rimosse o reimplementate durante le fasi
seguenti.

### Hotspot confermati

1. `App/Chat/AgentMarkdownTextView.swift:updateNSView` ricostruisce l'intero
   `NSAttributedString` quando cambia il testo, ricrea gli header dei code
   block e invalida la misura dell'altezza.
2. `App/Chat/MarkdownAttributedStringRenderer.swift:render` analizza tutto il
   Markdown, applica metriche a tutti i code block e rilancia il syntax
   highlighting sul `MainActor`.
3. `App/Chat/ChatController.swift:persist` chiama sincronicamente
   `ChatSessionStore.saveTranscript` dal `MainActor`.
4. `Packages/TillerACP/Sources/TillerACP/ChatSessionStore.swift:saveTranscript`
   ricodifica tutti gli item, cancella tutte le righe della sessione e le
   reinserisce una alla volta.
5. `Packages/TillerTerminal/Sources/TillerTerminal/PtyTerminalPane.swift`
   crea un `Task` verso `ScrollbackBuffer` per ogni chunk PTY.
6. `Packages/TillerTerminal/Sources/TillerTerminal/ScrollbackBuffer.swift:append`
   inserisce i byte individualmente con un'operazione modulo per byte.
7. `App/AppModel.swift:handleContentSignal` può invocare due volte
   `checkForegroundAgent` per lo stesso segnale di un pane process-owned.
8. `App/RightPanel/RightPanelModel.swift:refresh` ricarica directory in sequenza,
   esegue sempre `git status` e può ricaricare anche il diff selezionato.
9. `App/Chat/ChatController.swift` ricostruisce più volte `items`, `grouped`,
   permessi, subagent e attività corrente partendo dal transcript completo.
10. `App/AppModel.swift:bootstrap` attraversa progetti e worktree in sequenza
    mentre il modello è isolato sul `MainActor`.
11. `App/UsageStore.swift` mantiene il task timer anche quando tutti i provider
    sono disabilitati.

## Comandi necessari

| Scopo | Comando | Risultato atteso |
|---|---|---|
| Test ACP | `cd Packages/TillerACP && swift test` | exit 0, tutti i test passano |
| Test terminale | `cd Packages/TillerTerminal && swift test` | exit 0, tutti i test passano |
| Test core | `cd Packages/TillerCore && swift test` | exit 0, tutti i test passano |
| Test Git | `cd Packages/TillerGit && swift test` | exit 0, tutti i test passano |
| Rigenerazione progetto | `xcodegen generate` | exit 0 |
| Test App | `xcodebuild test -project Tiller.xcodeproj -scheme Tiller -destination 'platform=macOS' CODE_SIGNING_ALLOWED=NO -only-testing:TillerTests` | exit 0, tutti i test passano |
| Gate repository | `Scripts/ci.sh` | exit 0, ultima riga `CI OK` |

Usare test focalizzati durante ogni fase. Eseguire il gate completo soltanto
quando la fase è pronta per essere considerata conclusa.

## Ambito

### In scope

- Rendering e presentazione della chat ACP.
- Persistenza di transcript e scrollback.
- Pipeline PTY verso scrollback ausiliario.
- Rilevamento processi per pane terminale.
- Refresh filesystem/Git del right panel.
- Stato derivato usato dalle view della chat.
- Bootstrap dell'app, solo se il profiling conferma il costo.
- Lifecycle del polling usage.
- Fixture, signpost e test necessari a verificare questi percorsi.

### Fuori scope

- Terminare automaticamente agenti o terminali nascosti.
- Cambiare il protocollo ACP, il formato dei messaggi o il comportamento degli adapter.
- Sostituire libghostty o modificarne gli internals.
- Introdurre libgit2 o altre dipendenze native.
- Cambiare lo schema GRDB nella prima fase sulla persistenza.
- Riscrivere l'intera chat con `NSCollectionView`.
- Modificare la resa finale delle risposte Markdown completate.
- Ottimizzare memoria o CPU dei processi agent esterni.

## Workflow Git

- Creare una branch per fase con prefisso `perf/`, per esempio
  `perf/streaming-markdown-rendering`.
- Usare Conventional Commits con soggetto imperativo e minuscolo.
- Mantenere un commit logico per fase o per sottofase verificabile.
- Esempi:
  - `test: add deterministic performance fixtures`
  - `perf: defer full markdown rendering until stream completion`
  - `perf: serialize chat persistence off the main actor`
  - `perf: bulk append terminal scrollback`
- Non eseguire push e non aprire PR senza istruzione esplicita dell'operatore.

## Fase 1: stabilire una baseline riproducibile

### Obiettivo

Creare workload deterministici e misure confrontabili prima di modificare i
percorsi caldi. Nessuna percentuale di miglioramento è valida senza la stessa
fixture e lo stesso hardware prima e dopo.

### Implementazione

1. Aggiungere una fixture di streaming con:
   - messaggio finale di almeno 50 KB;
   - almeno 1.000 frammenti;
   - paragrafi, liste, link, Insight e code block Swift;
   - transcript preesistente di almeno 200 elementi.
2. Aggiungere una fixture terminale che superi più volte la capacità dello
   scrollback e contenga sentinel iniziale/finale e sequenza numerata.
3. Aggiungere una fixture right-panel con almeno 1.000 percorsi modificati,
   inclusi duplicati e directory annidate.
4. Preparare uno scenario di bootstrap con almeno 10 progetti e 20 worktree
   usando database e filesystem temporanei.
5. Estendere `SignpostMetrics`, senza contenuti sensibili, per misurare:
   - rendering Markdown completo;
   - commit/layout del messaggio in streaming;
   - encoding e scrittura transcript;
   - `scrollbackAppend`;
   - scansione processi;
   - `panelRefresh`;
   - bootstrap fino al primo contenuto interattivo.
6. Registrare baseline con Time Profiler, SwiftUI, Allocations e Points of
   Interest sullo stesso Mac destinato alle misure finali.

### Verifica

- Eseguire le fixture due volte: devono produrre lo stesso numero di eventi,
  byte, elementi e refresh.
- `defaults read dev.tiller debug.signpostMetrics` assente o falso deve lasciare
  i signpost disabilitati.
- Le etichette dei signpost non devono includere testo terminale, prompt,
  comandi, percorsi o identità.
- Tutti i test focalizzati dei package toccati devono passare.

## Fase 2: alleggerire il rendering durante lo streaming

### Obiettivo

Durante `agentMessage(isComplete: false)` non eseguire parsing e layout
Markdown completi. Eseguire l'attuale rendering ricco una sola volta quando il
messaggio diventa completo.

### Implementazione scelta

1. Introdurre una view AppKit append-only per il messaggio incompleto:
   - `NSTextView` read-only e selezionabile;
   - font e colore coerenti con il body corrente;
   - aggiornamento del solo suffisso aggiunto;
   - altezza aggiornata senza ricostruire l'intero attributed string;
   - nessun syntax highlighting e nessun header di code block provvisorio.
2. In `TranscriptView`, scegliere la view provvisoria soltanto per il messaggio
   incompleto attivo.
3. Al passaggio `isComplete: false -> true`, sostituire la view provvisoria con
   l'attuale `AgentMarkdownTextView` e renderizzare una sola volta Markdown,
   highlighting e header.
4. Conservare:
   - selezione del testo;
   - accessibilità;
   - autoscroll controllato da `streamTick`;
   - posizione quando l'utente ha preso il controllo dello scroll;
   - Reduce Motion.
5. Non aumentare `maxFlushInterval` oltre 500 ms.

### Alternative non selezionate

- Parsing incrementale dell'ultimo blocco Markdown: migliore resa live ma
  complessità e rischio più alti.
- Parsing completo fuori-main: insufficiente da solo perché font, layout,
  highlighting e aggiornamento AppKit restano main-actor-bound.
- Debounce ACP più lungo: riduce le transazioni ma rende lo streaming scattoso.

### Test

- 1.000 frammenti incompleti producono zero rendering Markdown completi.
- Il completamento produce esattamente un rendering completo.
- Il testo finale corrisponde byte per byte alla concatenazione dei frammenti.
- Messaggi completati conservano liste, link, code block, highlighting e Insight.
- Il testo resta selezionabile durante e dopo lo streaming.
- L'autoscroll segue il fondo solo quando l'utente non ha preso il controllo.
- Stop, errore e disconnessione finalizzano correttamente la presentazione.

### Gate prestazionale

- Nessuno stall del main thread superiore a 50 ms durante la fixture di chat.
- p95 del lavoro UI per flush sotto 16,7 ms sul Mac di riferimento, oppure
  miglioramento documentato con una STOP condition se il framework impedisce
  di raggiungere il target.

## Fase 3: serializzare la persistenza fuori dal MainActor

### Obiettivo

Rimuovere encoding JSON e scritture GRDB pesanti dal `MainActor`, preservando
ordine e durabilità dell'ultimo snapshot.

### Implementazione scelta

1. Introdurre un actor di persistenza che riceva snapshot immutabili e
   `Sendable` di transcript e scrollback.
2. Mantenere una coda ordinata per sessione/pane.
3. Coalescere snapshot ravvicinati della stessa chiave conservando quello più
   recente non ancora scritto.
4. Esporre operazioni equivalenti a:
   - `enqueueTranscript(sessionId:items:)`;
   - `enqueueScrollback(worktreeId:paneId:data:)`;
   - `flush(sessionId:)` / `flush(paneId:)`;
   - `flushAll()` per quit.
5. Lasciare `ChatSessionStore` responsabile del formato GRDB corrente; in
   questa fase non cambiare lo schema né il formato JSON.
6. Chiusura tab, stop e quit devono attendere il flush più recente con un
   timeout esplicito coerente con il lifecycle esistente.
7. Riportare gli errori senza bloccare il flusso ACP e senza consentire a uno
   snapshot vecchio di sovrascriverne uno nuovo.

### Alternative non selezionate

- `Task.detached` senza coordinatore: può completare fuori ordine.
- Persistenza incrementale per singolo item: potenziale fase successiva solo
  se la scrittura completa resta costosa dopo lo spostamento off-main.

### Test

- Snapshot A, B e C accodati rapidamente persistono C.
- `flush()` non termina prima della scrittura più recente.
- Sessioni e pane diversi restano isolati.
- Uno snapshot vecchio non può completare dopo e sostituire quello nuovo.
- Un errore non lascia l'actor bloccato.
- Chiusura e quit conservano ultimo transcript e scrollback.
- I test di round-trip e resume esistenti restano invariati.
- Instruments non mostra encoding o `database.write` pesanti sul main thread.

## Fase 4: ottimizzare lo scrollback terminale

### Obiettivo

Sostituire l'append byte-per-byte con copie contigue bulk senza cambiare l'API
pubblica o la sequenza di byte osservabile.

### Implementazione scelta

1. In `ScrollbackBuffer.append`, calcolare:
   - indice logico di scrittura;
   - spazio contiguo fino alla fine dello storage;
   - eventuale seconda porzione dopo il wrap.
2. Usare al massimo due `copyBytes` per chunk.
3. Aggiornare `head` e `count` una volta per append.
4. Per input maggiore o uguale alla capacità, conservare direttamente gli
   ultimi `capacity` byte.
5. Conservare esattamente le semantiche di `snapshot()` e `tail(_:)`.
6. Misurare il numero di `Task` accodati dal callback PTY. Solo se resta
   significativo, aggiungere batching limitato da dimensione e breve durata.
7. Prima di snapshot, read, stop o quit, svuotare immediatamente un eventuale
   batch pendente.

### Test

- Append senza wrap, con wrap singolo e multiplo.
- Input vuoto, esattamente pari alla capacità e maggiore della capacità.
- Append ripetuti con dimensioni non divisibili per la capacità.
- UTF-8 multibyte e sequenze ANSI divise sul confine.
- Confronto di `snapshot()` e `tail()` con un modello reference semplice.
- Sentinel iniziale/finale, conteggio e ordine esatti dopo output intenso.

### Gate prestazionale

- Almeno 20% di riduzione CPU in `scrollbackAppend` rispetto alla baseline.
- Zero differenze byte-per-byte nelle fixture.

## Fase 5: deduplicare il rilevamento dei processi

### Obiettivo

Eseguire al massimo una scansione processi per pane alla volta ed evitare
invalidazioni osservabili quando il risultato non cambia.

### Implementazione scelta

1. Rimuovere la doppia invocazione di `checkForegroundAgent` per i pane
   process-owned.
2. Introdurre uno stato per pane con:
   - scansione in-flight;
   - richiesta di follow-up;
   - generation/lifecycle token.
3. Se arrivano più segnali mentre una scansione è attiva, registrare un solo
   follow-up.
4. Confrontare agent ID e process tree prima di mutare `agentActivity` e
   `paneProcessTrees`.
5. Ignorare risultati tardivi di pane chiusi o appartenenti a una generation
   precedente.
6. Continuare a eseguire libproc fuori dal main thread.

### Test

- Un content signal produce una sola scansione.
- Dieci segnali concorrenti producono una scansione più al massimo un follow-up.
- Un risultato invariato non modifica lo stato osservabile.
- Un risultato tardivo dopo chiusura viene ignorato.
- Un processo comparso durante la prima scansione viene rilevato dal follow-up.
- Spawn-owned, title-owned e process-owned conservano le regole di clearing
  definite da `AgentActivityModel`.

## Fase 6: rendere single-flight il refresh del right panel

### Obiettivo

Evitare refresh Git/filesystem sovrapposti e limitare il lavoro alle directory
e al diff effettivamente interessati.

### Implementazione scelta

1. Mantenere una sola operazione `panelRefresh` attiva per generation.
2. Accumulare in un `Set` i percorsi arrivati durante il refresh.
3. Alla fine, eseguire al massimo un follow-up con l'unione dei percorsi.
4. Caricare le directory interessate in parallelo fuori dal `MainActor` e
   applicare un unico snapshot coerente.
5. Usare debounce adattivo:
   - 250 ms per modifiche isolate;
   - crescita progressiva fino a 1 secondo durante burst continui;
   - ritorno a 250 ms dopo il settle.
6. Eseguire `git status` una volta per ciclo coalescente.
7. Ricaricare il diff soltanto quando:
   - cambia il file selezionato;
   - il file selezionato è tra i percorsi interessati;
   - il nuovo snapshot Git ne cambia lo stato.
8. Non avviare monitor o refresh quando il pannello è disattivato.

### Alternative non selezionate

- libgit2: dipendenza e complessità non giustificate.
- Polling fisso: spreca risorse quando il repository è inattivo.
- Cancellazione cieca di ogni refresh precedente: può perdere eventi e lasciare
  UI incoerente.

### Test

- Un burst genera un solo refresh più al massimo un follow-up.
- Percorsi duplicati vengono coalesciati.
- Directory estranee non vengono ricaricate.
- Risultati di generation precedenti non vengono applicati.
- Nascondere o cambiare worktree cancella correttamente il lifecycle.
- Il diff non viene ricaricato per modifiche estranee.
- Eventi arrivati durante il refresh non vengono persi.

## Fase 7: pubblicare uno snapshot derivato della chat

### Obiettivo

Calcolare una volta per batch ACP gli aggregati usati da SwiftUI, evitando
concatenazioni e scansioni ripetute durante lo stesso redraw.

### Implementazione scelta

1. Introdurre un `ChatPresentationSnapshot` contenente:
   - items combinati restaurati/correnti;
   - roots e children dei tool call;
   - permessi pendenti;
   - subagent attivi;
   - attività corrente;
   - segmenti prose/Insight dei messaggi.
2. Ricostruire lo snapshot dopo ogni flush ACP, non a ogni accesso della view.
3. Riutilizzare segmenti e aggregati degli item invariati per ID/versione.
4. Pubblicare lo snapshot soltanto se è cambiato.
5. Migrare gradualmente le view mantenendo temporaneamente test di equivalenza
   con i computed property attuali.
6. Rimuovere i vecchi computed property solo dopo la migrazione completa.

### Dipendenza

Questa fase deve iniziare dopo la Fase 2, perché lo snapshot deve rappresentare
in modo univoco il nuovo stato streaming/completato.

### Test

- Equivalenza con `items`, `grouped`, permessi e subagent correnti.
- Aggiornamento corretto del solo messaggio attivo.
- Transcript ripristinato più nuovi item mantiene ID e ordine.
- Risultato invariato non viene ripubblicato.
- Insight e children dei tool call restano associati al parent corretto.

## Fase 8: rendere progressivo il bootstrap

### Gate di ingresso

Implementare questa fase soltanto se la fixture della Fase 1 mostra stall del
main thread superiori a 100 ms o un tempo al primo contenuto interattivo non
accettabile. In caso contrario registrare la fase come `REJECTED — no measured
benefit` senza cambiare il codice.

### Implementazione proposta

1. Estrarre caricamento database, controlli filesystem e costruzione dei
   modelli in un loader non isolato dal `MainActor`.
2. Restituire snapshot `Sendable` anziché mutare `AppModel` durante ogni query.
3. Pubblicare prima progetti e worktree.
4. Ripristinare tab, sessioni e riferimenti agent progressivamente.
5. Spostare migrazione hook e controlli file fuori dal percorso del primo
   contenuto interattivo.
6. Conservare l'ordine attuale per control socket, notifiche e usage store.
7. Usare generation token per impedire a snapshot tardivi di sovrascrivere
   interazioni già effettuate dall'utente.
8. Non parallelizzare scritture di configurazione degli adapter senza una
   serializzazione per worktree.

### Test

- Progetti, worktree, tab e selezione finali equivalgono al bootstrap corrente.
- Worktree vuoti restano validi e non creano PTY.
- File Markdown mancanti vengono ancora scartati.
- Un worktree corrotto non impedisce di caricare gli altri.
- Nessuna sessione o pane viene ripristinato due volte.
- Interazione utente durante il bootstrap non viene annullata da snapshot tardivi.
- Nessuno stall superiore a 100 ms nella fixture di riferimento.

## Fase 9: fermare il polling usage quando non serve

### Obiettivo

Non mantenere task timer e wake-up quando nessun provider usage è abilitato.

### Implementazione scelta

1. Introdurre `UsageStore.updatePolling(enabledProviders:)`.
2. Non creare il timer con set vuoto.
3. Avviare idempotentemente un solo timer quando viene abilitato il primo
   provider.
4. Cancellarlo quando viene disabilitato l'ultimo provider.
5. Al cambio dell'intervallo, cancellare e riarmare il timer.
6. Conservare il single-flight già esistente per ogni provider.
7. Collegare tutte le preferenze `showInBar` al lifecycle del timer.

### Test

- Zero wake-up con nessun provider.
- Un solo timer con uno o più provider.
- Disabilitare l'ultimo provider cancella il timer.
- Riabilitare un provider esegue refresh e riavvia la cadenza.
- Cambiare intervallo non crea timer duplicati.
- Valori corrotti continuano a essere normalizzati da `AppSettings`.

## Ordine di esecuzione

| Fase | Titolo | Priorità | Effort | Dipende da | Stato |
|---|---|---|---|---|---|
| 1 | Baseline e fixture | P1 | M | — | TODO |
| 2 | Rendering leggero dello streaming | P1 | M | 1 | TODO |
| 3 | Persistenza actor off-main | P1 | M | 1 | TODO |
| 4 | Append bulk dello scrollback | P1 | S | 1 | TODO |
| 5 | Process probe single-flight | P1 | S | 1 | TODO |
| 6 | Right panel single-flight | P2 | M | 1 | TODO |
| 7 | Snapshot derivato della chat | P2 | M | 2 | TODO |
| 8 | Bootstrap progressivo | P3 | L | 1 e gate misurato | FATTO (variante minima: riordino, non loader off-MainActor) — time-to-interactive -39% mediano, 6/7 vittorie appaiate; vedi docs/superpowers/notes/baseline-2026-07-17.md |
| 9 | Stop polling usage inattivo | P3 | S | 1 | TODO |

Le fasi 2-6 possono essere sviluppate su branch separati dopo la baseline. La
Fase 7 dipende esplicitamente dalla Fase 2. La Fase 8 non deve essere eseguita
senza evidenza strumentale.

## Test plan complessivo

### Regressioni funzionali

- Streaming, stop, error, disconnect, permission request e turn end.
- Resume di transcript persistiti e normalizzazione degli ID.
- Selezione e copy di Markdown e code block.
- Output terminale, scrollback, `panel.read`, hide/show e quit.
- Activity detection per tutti gli ownership layer.
- File tree, Git status, diff e cambio worktree.
- Bootstrap con worktree vuoti e dati parzialmente corrotti.
- Polling usage con tutte le combinazioni abilitate/disabilitate.

### Scenari prestazionali

- Chat: 50 KB / 1.000 frammenti / 200 item preesistenti.
- Terminale: quattro pane concorrenti e almeno tre wrap del ring buffer.
- Processi: burst di dieci content signal per lo stesso pane.
- Right panel: 1.000 path in più burst mentre un refresh è attivo.
- Bootstrap: 10 progetti / 20 worktree / mix di tab terminale, chat e Markdown.

### Gate finale

```bash
Scripts/ci.sh
```

Risultato richiesto: exit code `0` e ultima riga `CI OK`.

## Done criteria complessivi

- [ ] Le fixture deterministiche e la baseline sono documentate.
- [ ] Nessun full Markdown render durante gli update incompleti; uno al completamento.
- [ ] Nessuna codifica o scrittura GRDB pesante sul `MainActor`.
- [ ] Nessuna perdita o alterazione byte-per-byte dello scrollback.
- [ ] Una sola scansione processi in-flight per pane.
- [ ] Nessun refresh Git/file explorer sovrapposto.
- [ ] Nessuna ricostruzione dello stesso snapshot chat per accesso della view.
- [ ] Bootstrap senza stall >100 ms oppure Fase 8 esplicitamente rifiutata per assenza di beneficio.
- [ ] Nessun timer usage quando tutti i provider sono disabilitati.
- [ ] Nessuna regressione nella sospensione delle superfici libghostty nascoste.
- [ ] Ogni fase modifica soltanto i file dichiarati nel proprio scope operativo.
- [ ] Tutti i test focalizzati passano.
- [ ] `Scripts/ci.sh` termina con `CI OK`.

## STOP conditions

Fermarsi e riferire senza improvvisare se:

- il codice corrente non corrisponde più agli hotspot descritti;
- una fase richiede di sovrascrivere modifiche locali preesistenti;
- una soluzione termina automaticamente agenti o terminali nascosti;
- il rendering provvisorio perde selezione, autoscroll o accessibilità;
- non è possibile garantire l'ordine delle scritture asincrone;
- anche un solo byte differisce nelle fixture dello scrollback;
- il coordinatore del right panel può perdere eventi durante un refresh;
- il bootstrap richiede una migrazione schema non prevista;
- una modifica richiede una nuova dipendenza non autorizzata;
- la stessa verifica fallisce due volte senza una correzione circoscritta;
- il gate finale non raggiunge `CI OK`.

## Miglioramenti considerati e non raccomandati

- Aumentare ulteriormente il debounce ACP: peggiora la fluidità percepita senza
  eliminare il costo del full render.
- Reimplementare il read buffer PTY: è già riutilizzabile.
- Rileggere l'intero scrollback nel content signal: `tail(10 KB)` è già presente.
- Nascondere i terminali soltanto con opacity: l'occlusione libghostty è già implementata.
- Introdurre libgit2: costo architetturale non giustificato.
- Terminare pane o agenti nascosti: viola le invarianti del prodotto.
- Usare `Task.detached` per la persistenza senza actor seriale: può riordinare le scritture.
- Riscrivere subito la chat con una collection view AppKit: intervento troppo
  ampio prima di esaurire le ottimizzazioni mirate.
- Ottimizzare il bootstrap senza misura: rischio e blast radius non giustificati.

## Note di manutenzione

- Ogni nuova sorgente di eventi ACP deve passare dal batching e aggiornare lo
  snapshot di presentazione una sola volta per flush.
- Ogni nuovo punto di persistenza deve usare il coordinatore seriale e definire
  chiaramente se richiede `flush()` al lifecycle boundary.
- Modifiche future alla capacità dello scrollback devono rieseguire tutte le
  fixture byte-level e il sentinel test hide/show.
- Nuovi ownership layer per l'agent detection devono evitare che l'assenza di
  un segnale cancelli lo stato posseduto da un altro layer.
- Nuove modalità del right panel devono condividere il lifecycle single-flight,
  non aggiungere monitor indipendenti.
- Il reviewer deve controllare in particolare thread/actor isolation, ordine
  delle scritture, cancellazione, risultati tardivi e assenza di invalidazioni
  SwiftUI quando il valore non cambia.
