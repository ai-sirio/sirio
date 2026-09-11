# Zed → Sirio: analisi della fluidità e candidati di porting

- **Data:** 2026-09-08.
- **Stato:** ricerca completata; nessun intervento implementato o approvato. Questo è un rapporto, non un piano esecutivo.
- **Sirio analizzato:** `5a030ba18423ea9f8895d0dfa9ea8e90034a9b88`, branch `main`, working tree inizialmente pulito.
- **Zed analizzato:** `20d3cd1d7981ed2076be5b72c817a1febbba3cb9`; tutti i link upstream sono fissati a quel commit, non a `main`.
- **Metodo:** due analisi indipendenti in sola lettura, verifica diretta dei risultati sul codice e confronto con i sorgenti delle dipendenze installate. Nessuna build, test, CI o misura runtime eseguita.
- **Confidenza:** alta sui costi e meccanismi indicati dal codice; impatto temporale e miglioramenti ancora da misurare. Le priorità sono proposte di indagine, non una classifica di colli di bottiglia misurati.

## Sintesi

La fluidità di Zed non deriva da una singola ottimizzazione: combina invalidazioni mirate, riuso delle view e del testo, liste virtualizzate, batching degli eventi e delle primitive GPU, lavoro in background e coordinamento dei frame.

**Sirio possiede già gran parte di questi meccanismi.** Le opportunità più concrete sono nel lavoro che l'applicazione esegue **prima** che cache, virtualizzazione o debounce possano proteggerla: preparazione completa delle righe, estrazione dello scrollback, parsing di prefissi crescenti e operazioni sincrone nel thread UI.

Non raccomando come primo passo una sostituzione del terminale, un aggiornamento generalizzato di GPUI o un porting del renderer. Raccomando misure mirate, poi interventi applicativi separati e confrontabili.

## 1. Versioni e compatibilità

`rust/Cargo.toml:61–75` usa l'alias `gpui` per `bezel-gpui =0.3.8`, bezel `0.1.4` e la relativa famiglia di componenti. `rust/Cargo.lock` risolve il core GPUI a **`0.3.8+zed.82aeef`**; il terminale usa `libghostty-vt 0.2.1` e `portable-pty 0.9.0`.

Il suffisso della versione identifica il pacchetto, ma non dimostra parità con Zed corrente. Il confronto ha verificato direttamente nel pacchetto installato:

| Meccanismo | Evidenza nel sorgente pubblicato di Bezel GPUI | Esito |
| --- | --- | --- |
| View cache condizionata da geometria, stile, dirty state | `bezel-gpui/src/view.rs:375–475` | Presente: riuso di prepaint/paint |
| Invalidazione clean→dirty e risveglio della piattaforma | `bezel-gpui/src/window.rs:167–215` | Presente |
| Cache del layout testuale fra frame | `bezel-gpui/src/text_system/line_layout.rs:639–690` | Presente |
| Costruzione delle sole righe visibili | `bezel-gpui/src/elements/uniform_list.rs:473–510` | Presente |
| Raggruppamento delle primitive della scena | `bezel-gpui/src/scene.rs:193–233` | Presente |
| Atlas WGPU: rasterizzazione/upload solo su cache miss | `bezel-gpui-wgpu/src/wgpu_atlas.rs:108–128` | Presente nel backend WGPU |

I percorsi della tabella sono relativi ai pacchetti `0.3.8+zed.82aeef` della cache Cargo, non a file da vendorizzare nel repository.

Sirio mantiene override locali di `bezel-gpui-linux` e `bezel-gpui-windows`: vedere `rust/vendor/README.md` e `rust/Cargo.toml:112–132`. Un aggiornamento indiscriminato rischia di perdere le correzioni DnD e le scelte di composizione del browser. Il README corrente descrive un prodotto **macOS/Linux/Windows**: non assumere prestazioni o comportamento del compositor equivalenti fra sistemi.

`plans/001-performance-fluidity-roadmap.md` riguarda la versione Swift ritirata; non è evidenza del comportamento Rust corrente e non viene modificato da questa ricerca.

## 2. Cosa fa Zed e cosa significa per Sirio

### 2.1 Invalidazioni mirate e view cache esplicita

GPUI permette di riutilizzare il lavoro di sottoviste stabili attraverso `.cached(...)`; il riuso dipende da bounds, content mask, stile e assenza di invalidazioni. Non ogni `Entity` viene automaticamente memoizzata. Una notifica può invalidare anche il percorso degli antenati.

Fonti: [view caching][Z-view], [notifiche][Z-notify].

**Sirio:** il meccanismo è presente nel fork; `SirioWorkspace::child_view`, `rust/crates/sirio/src/main.rs:12976–12983`, lo usa quando la cache è abilitata. Conservare i confini esistenti, correggere eventuali invalidazioni troppo ampie solo dopo averle contate. Non aggiungere una seconda cache grafica generica.

### 2.2 Virtualizzazione reale, non solo clipping

`uniform_list` calcola un range dalla viewport e costruisce solo quegli elementi. `list` gestisce altezze variabili e metadati di layout. Questo non rende automaticamente economici ordinamento, filtraggio o preparazione di tutte le righe effettuati a monte.

Fonte: [UniformList][Z-list].

**Sirio:** Changes usa già `gpui::list`; Files e History usano `uniform_list`; la chat è virtualizzata. Il candidato S1 riguarda proprio la preparazione precedente alla lista, non l'aggiunta di un virtualizzatore.

### 2.3 Cache del testo e batching GPU

La cache testuale controlla il frame corrente e precedente prima di eseguire shaping di piattaforma. Gli atlas riusano glyph già rasterizzati; il renderer emette draw instanziati su intervalli di primitive compatibili. Non significa un unico draw call per finestra, né eliminazione della costruzione CPU della scena.

Fonti: [cache testuale][Z-text], [atlas][Z-atlas], [draw instanziati][Z-gpu].

**Sirio:** i meccanismi verificati nella sezione 1 sono già disponibili. Il terminale inoltre conserva griglia, righe sagomate, backgrounds e cursore, con una chiave di assembly: `rust/crates/sirio_terminal/src/lib.rs:2079–2091,4001–4009,4109–4142`. L'invalidazione applicativa è a griglia intera, non una ricostruzione incrementale delle sole righe sporche.

### 2.4 Eventi terminale: prima risposta rapida e burst raggruppati

In `TerminalBuilder::subscribe`, Zed attende un evento, elabora immediatamente il primo e poi accumula quelli successivi in una finestra di **4 ms**, collassando i `Wakeup` ripetuti e cedendo il controllo fra batch. A riposo torna ad attendere il canale. Nei test la temporizzazione usa un percorso simulato.

Fonte: [event loop applicativo Zed][Z-events].

**Sirio:** `pump_terminal_events`, `rust/crates/sirio_terminal/src/lib.rs:3084–3249`, già coalesca fino a 100 eventi per drain e separa il repaint sostenuto a 16 ms dal settle dell'attività a 200 ms. La differenza è l'attesa: il pump usa un timer ricorrente a 4 ms, oppure 1 ms durante il retry della cattura. Non copiare solo il numero “4 ms”: in Zed delimita un burst, in Sirio serve anche a controllare il canale a riposo.

**Vincolo:** il codice Sirio giustifica il polling con il scheduler deterministico GPUI e i wake da thread esterni. Sostituirlo ingenuamente con `rx.next().await` non è una soluzione verificata.

### 2.5 Snapshot separati dal rendering, senza idealizzare i lock

Zed produce `last_content` e lo usa per il rendering, ma `Terminal::sync` acquisisce un **lock bloccante**. Non dimostra la tecnica “se il parser è occupato, riusa sempre il vecchio snapshot senza attendere”.

Fonte: [Terminal::sync][Z-sync].

**Sirio:** il round trip sincrono `Snapshot` attende la risposta dell'owner. La pubblicazione asincrona proposta in S8 è un possibile miglioramento specifico di Sirio, **non un'ottimizzazione lock-free già osservata in Zed da copiare**.

### 2.6 Frame richiesto, scena ricostruita e presentazione sono cose diverse

GPUI ricostruisce la scena quando dirty o quando è richiesto un render forzato; può presentare un frame già costruito senza rifare il draw. Nel codice corrente Zed prolunga inoltre la presentazione dopo input ad alta frequenza per evitare il rallentamento del display. Non dedurre “nessun callback” da “nessun redraw”.

Fonte: [frame gating][Z-frame].

Il pacing resta dipendente dalla piattaforma: nessuna proposta di uniformare Wayland, X11, Metal e Windows sulla base di un solo sistema. Gli override Windows di Sirio sono scelte documentate, non problemi da eliminare incidentalmente.

## 3. Files, Changes e Diff nella sidebar destra

### Struttura di Zed

```text
Workspace
├── area editor → file e ProjectDiff
│                  └── DiffMultibuffer → editor
└── dock laterale → ProjectPanel / GitPanel
                    un pannello attivo alla volta
```

Nel commit esaminato, Files (`project_panel`) e Git (`git_panel`) hanno entrambi `dock: right`, rispettivamente con larghezza predefinita 240 e 360. Possono essere spostati a sinistra. Il dock conserva le entità e renderizza il pannello visibile con `.cached(...)`.

Fonti: [default Files][Z-files-default], [default Git][Z-git-default], [dock][Z-dock].

- **Files:** riceve eventi del progetto, prende snapshot dei worktree e del Git store, costruisce la rappresentazione dell'albero in background, poi virtualizza gli elementi. Vedere [preparazione Files][Z-files] e [render Files][Z-files-list].
- **Changes:** `schedule_update` raggruppa aggiornamenti con debounce di 50 ms. La lista è virtualizzata. Non tutto è incrementale: `update_visible_entries` ricostruisce strutture e conteggi. Vedere [aggiornamento GitPanel][Z-git-schedule] e [righe GitPanel][Z-git-list].
- **Diff:** `ProjectDiff::deploy_at` riattiva una vista esistente quando possibile. `DiffMultibuffer` ascolta cambiamenti per buffer. Durante il caricamento cede esplicitamente il controllo anche se le future sono già pronte, evitando un ciclo asincrono che monopolizzi il foreground. Vedere [riuso ProjectDiff][Z-diff] e [yield][Z-diff-yield].

**Applicazione a Sirio:** separare frequenza degli eventi filesystem/Git, aggiornamento dei modelli e frame. Non è necessario spostare il diff dall'attuale superficie secondaria: il guadagno architetturale è conservare dati già pronti e non rifare I/O o preparazione globale durante un semplice ridisegno.

## 4. Candidati verificati in Sirio

I percorsi qui sotto sono relativi alla radice del repository al commit dichiarato. Tutti i costi descritti sono stati verificati direttamente nel sorgente; nessun tempo o guadagno è stato misurato. **M** indica un intervento circoscritto con test; **L** un intervento multi-giorno o trasversale. Sono stime, non impegni.

| ID | Priorità proposta | Candidato | Effort | Rischio |
| --- | --- | --- | --- | --- |
| S1 | P1 | Conservare la proiezione di Changes prima della lista | M | Medio |
| S2 | P1 | Estrarre soltanto il contenuto recente per activity detection | M | Medio |
| S3 | P1 | Raggruppare parsing chat e conservare metadati del transcript | L | Medio |
| S4 | P2 | Ridurre acquisizione Git per-file e diff non richiesti | L | Medio |
| S5 | P1 | Coalescere l'acquisizione della sessione, non solo la scrittura | L | Alto |
| S6 | P2 | Portare la persistenza di fine turno fuori dal callback UI | L | Alto |
| S7 | P1 | Togliere la discovery Git dal render al ritorno del focus | M | Medio |
| S8 | P2 | Evitare l'attesa dell'owner terminale durante prepaint | L | Alto |
| S9 | P2 | Ridurre il polling idle con un bridge/coordinatore supportato | L | Alto |

### S1 — Changes: il costo viene pagato prima del virtualizzatore

**Evidenza:** `rust/crates/sirio_ui/src/changes.rs:2337–2357` chiama `section_rows` e `sync_list_rows` nel render del body. `:1193` costruisce il payload drag anche per file con diff collassato, quando la sezione è aperta. `diff_payload`, `:2517–2537`, percorre hunks e righe. `sync_list_rows`, `:1316–1353`, appiattisce e calcola il fingerprint: il fingerprint evita uno splice, non la costruzione appena eseguita.

**Costo:** ricostruzione dei dati e delle stringhe del diff su render del body senza modifica dei dati. Non affermare che ogni tick terminale invalida automaticamente questa view: contare i trigger reali.

**Direzione:** proiezione conservata per revisione dello snapshot, modalità e stato di espansione; payload immutabile per revisione del file oppure creato al drag. Non duplicare il virtualizzatore.

**Verifica proposta:** diff da 15.000 righe, centinaia di file prevalentemente collassati; contare costruzioni durante redraw, selezione, resize e switch unified/split. Refresh del contenuto, espansione, copy e drag devono restare corretti anche se l'identità delle righe non cambia.

### S2 — Activity detection: la coda limitata arriva dopo la cattura completa

**Evidenza:** `rust/crates/sirio_terminal/src/lib.rs:2197–2241` visita history e viewport cella per cella. Nel pump, `:3221–3241`, si ottiene prima lo scrollback e poi lo si passa a `recent_content_window` (`:2622–2634`), limitato a 10 KiB e 40 righe.

**Costo:** estrazione e allocazione proporzionali alla history per ottenere una piccola coda. Il settle usa già una richiesta non bloccante per la UI; il lavoro resta sull'owner terminale, anche per agenti nascosti.

**Direzione:** richiesta dedicata di contenuto recente lato owner. Conservare la cattura completa per persistenza e richieste esplicite del control socket.

**Verifica proposta:** equivalenza con “cattura completa + trim” per history lunga, righe vuote finali, wrapping, UTF-8, grapheme cluster, alternate screen e ultimo output ritardato; contare celle visitate e byte prodotti. Non troncare più aggressivamente l'input del matcher.

### S3 — Chat: meno parse e meno lavoro sul transcript storico

**Evidenza:** `rust/crates/sirio_ui/src/chat/mod.rs:2192–2205` riparsa tutto il messaggio accumulato a ogni `AgentMessageChunk`; `:4285–4292` applica gli eventi uno per volta. `transcript_entry_ranges`, `:2705–2720`, materializza il plain text di ogni entry per conoscerne la lunghezza; il render lo richiama a `:7389`.

**Costo:** visite ripetute a prefissi crescenti; con chunk di dimensione costante, il volume cumulativo dei prefissi riprocessati cresce quadraticamente. La virtualizzazione non evita il calcolo dei metadati dell'intero transcript.

**Direzione:** accorpare chunk di prosa adiacenti entro un budget di presentazione, mantenendo l'ordine con tool, permessi e footer; conservare lunghezze e offset delle entry già completate. Non degradare automaticamente il Markdown live a solo testo senza una decisione UX.

**Verifica proposta:** stessa risposta da 50 KiB in 1.000 chunk e in pochi chunk, transcript con 200+ entry; conteggio parse, tempo UI p95/p99, correttezza di selezione/copia, scroll anchoring e ordine degli eventi.

### S4 — Git: ogni refresh acquisisce anche i diff collassati

**Evidenza:** `rust/crates/sirio_ui/src/changes.rs:2717–2734` richiama `diff_entry` per ogni entry di status. `rust/crates/sirio_git/src/diff.rs:97–141` esegue diff per file e, per entry tracciate, controlla la disponibilità di HEAD.

**Costo:** processi Git e parsing proporzionali a tutti i file sporchi. L'acquisizione è già in background: il problema non è presentarla come I/O sincrono UI, ma ridurne CPU/I/O e tempo al risultato.

**Direzione:** disponibilità HEAD una volta per snapshot; batching dei diff tracciati dove semanticamente equivalente; dettaglio lazy/cache per espansione o drag, senza perdere status e conteggi.

**Verifica proposta:** centinaia di file sporchi; contare invocazioni Git. Confrontare rename, binary, untracked, conflitti, staged+unstaged, repository senza HEAD, errori per file e operazioni di staging serializzate.

### S5 — Sessione: il debounce arriva dopo le operazioni pesanti

**Evidenza:** `rust/crates/sirio/src/main.rs:5272–5280` passa `self.layout(cx)` a `session.schedule`. `layout`, `:5191–5268`, chiama `current_branch` e cattura lo scrollback dei terminali del worktree corrente. `current_branch`, `rust/crates/sirio_project/src/discovery.rs:93–101`, avvia Git. La cattura sincrona `rust/crates/sirio_terminal/src/lib.rs:555–572` attende `recv()`.

**Costo:** il writer può collassare le scritture ma non elimina le acquisizioni già effettuate sul percorso UI, per esempio durante cambi rapidi di tab.

**Direzione:** coalescere prima dell'acquisizione; prendere metadati UI leggeri e reader clonabili, raccogliere i dati pesanti fuori thread UI, applicare revisioni/ordine per worktree. Conservare durabilità dell'ultimo stato.

**Verifica proposta:** switch rapidi fra tab e worktree con history lunga, output contemporaneo, chiusura/quit durante save; misurare separatamente acquisizioni e commit. Nessun risultato vecchio può sovrascrivere uno nuovo.

### S6 — Fine turno: conversione e SQLite nel callback UI

**Evidenza:** `rust/crates/sirio_ui/src/chat/mod.rs:2618` chiama `persist_settled_transcript` prima di avviare il turno accodato e di emettere `TurnEnded`. `:3000–3015` apre il database e prepara il transcript; `rust/crates/sirio_persistence/src/db.rs:556–588` serializza i turni trattenuti e riscrive le relative righe in transazione.

**Costo:** conversione, setup connessione e attese SQLite possono ritardare la conclusione visibile del turno. Il limite di retention non rende asincrona l'operazione.

**Direzione:** owner di persistenza serializzato in background, connessione riutilizzata e snapshot revisionati. Definire esplicitamente la relazione fra save, turno accodato e notifica di completamento prima di cambiare il callback.

**Verifica proposta:** turni accodati, DB conteso o non scrivibile, delete di tab durante save, shutdown e resume. Un save ritardato non deve ricreare dati eliminati.

### S7 — Focus regain: la discovery resta nel render

**Evidenza:** `rust/crates/sirio/src/main.rs:14729–14735` esegue `refresh_project_for_path` nel render sulla transizione inattiva→attiva. Il refresh del catalogo (`:5373–5394`) raggiunge `discover_project` in `rust/crates/sirio/src/session.rs:680`; `rust/crates/sirio_project/src/discovery.rs:82–85` esegue `git worktree list --porcelain`.

**Costo:** il primo frame al ritorno nell'app include discovery Git/filesystem. Il refresh leggero delle etichette via lettura di HEAD, già esistente, non elimina questo percorso distinto.

**Direzione:** discovery background coalesciuta, mantenendo il catalogo precedente fino al risultato validato per revisione. Preservare i worktree mancanti ma ancora montati.

**Verifica proposta:** fixture Git lenta, molti worktree, cambi focus/selection durante discovery, rimozione esterna di worktree con PTY vivo. Misurare separatamente tempo al primo frame e tempo alla freschezza del catalogo.

### S8 — Snapshot terminale: niente attese prolungate in prepaint

**Evidenza:** `rust/crates/sirio_terminal/src/lib.rs:4109` chiama `current_grid`; su mutation stamp nuovo (`:2079–2091`) `snapshot` attende una risposta sincrona (`:2058–2067`). L'owner drena i chunk PTY (`:1375–1394`) prima di servire comandi, senza un budget esplicito del batch di byte. L'assembly miss visita tutta la griglia visibile (`:4142` in avanti).

**Costo:** la preparazione del frame può attendere parsing ed estrazione; una piccola mutazione invalida l'intero assembly applicativo, anche se la cache testuale GPUI evita parte dello shaping.

**Direzione:** prima misurare attesa e fairness del parser; poi valutare snapshot immutabili pubblicati asincronamente e riuso dell'ultimo coerente. Eventuale dirty-row assembly è una fase successiva. Questa proposta non è un porting letterale del lock Zed, anch'esso bloccante.

**Verifica proposta:** output sostenuto mentre si digita, più split, cursore/selection, resize, alternate screen, restart e shutdown; l'ultimo frame deve arrivare anche dopo la fine dell'ultimo burst.

### S9 — Idle: eventi e deadline al posto di polling per-pane permanente

**Evidenza:** `rust/crates/sirio_terminal/src/lib.rs:2555–2577,3122` definisce il polling 4 ms/1 ms; il pump continua anche senza output. L'owner usa inoltre un timeout proprio (`:1556`). Il codice non lega queste attese alla visibilità della presentazione.

**Costo:** lavoro di scheduling ricorrente per terminale vivo. Il timer 4 ms corrisponde nominalmente a 250 controlli/s per pump; non è una misura di wakeup fisici, CPU o frame renderizzati. Terminali nascosti non vengono tutti disegnati.

**Direzione:** bridge supportato dal scheduler GPUI o coordinatore condiviso con deadline distinte per repaint, settle, retry, titoli e exit. Non sospendere i processi né la gestione del protocollo. Non sostituire il canale senza caratterizzare i test deterministici.

**Verifica proposta:** 1/8/20 terminali idle e agenti attivi nascosti; CPU, timer/logical wake count, prima risposta, exit/title, generation dei restart e consegna dell'ultimo frame. Priorità finale subordinata alle misure: potrebbe incidere meno sullo stutter di I/O e preparazione sul thread UI.

## 5. Baseline e verifica prima di implementare

### Riutilizzare i test esistenti, conoscendone i limiti

- `perf_keystroke_echo_latency`, `rust/crates/sirio_terminal/src/lib.rs:9259–9319`: misura millisecondi **simulati fino alla notifica della view**, non key-to-photon o frame presentato.
- `perf_idle_terminals_burn_cpu`, `:9342–9396`: diagnostico ignorato per default; crea `TerminalHandle`, **non TerminalView**, quindi non misura il pump GPUI sopra i handle.
- `perf_grid_rebuild_per_frame`, `:9423–9461`: diagnostico di snapshot; non misura da solo l'intera pipeline UI/GPU.
- `a_four_kib_streaming_turn_builds_a_bezel_doc_within_the_frame_budget`, `rust/crates/sirio_ui/src/chat/mod.rs:8411–8437`: singolo parse di circa 4 KiB, non un turno lungo in molti chunk.
- I numeri storici nei commenti non sono stati riprodotti e non costituiscono la baseline di questo rapporto.

### Workload proposti

1. Un terminale con echo interattivo mentre altri generano output; 1/8/20 terminali a riposo, inclusi nascosti.
2. Changes con 15.000 righe e centinaia di file; selezione, scroll, resize, espansione, drag e refresh.
3. Chat da 50 KiB in chunk piccoli con storia lunga, tool/permessi intercalati e completamento.
4. Switch rapido di tab/worktree, focus regain, save e quit con history lunga.

Registrare build/profile, OS/compositor, hardware, refresh del display e stato delle cache. Misurare tempo UI p50/p95/p99, attese sincrone, CPU idle, conteggi parse/snapshot/proiezioni/Git e memoria allocata. **16,7 ms a 60 Hz e 8,3 ms a 120 Hz sono budget di frame, non risultati ottenuti.** Usare profiling nativo sul sistema target; il tempo fino a `notify` non sostituisce la presentazione reale.

Cambiare una sola leva per volta, ripetere baseline e misura nelle stesse condizioni. Tenere l'intervento soltanto se il guadagno supera il rumore e le invarianti restano valide; altrimenti scartarlo. Non registrare prompt o contenuti terminale nella telemetria prestazionale.

Comandi indicativi per la futura iterazione, **non eseguiti in questa ricerca**:

```bash
cd rust
cargo test -p sirio_ui
cargo test -p sirio_project
cargo test -p sirio_persistence
cargo test -p sirio_terminal perf_keystroke_echo_latency
cargo test -p sirio_terminal perf_idle_terminals_burn_cpu -- --ignored --nocapture
cargo test -p sirio_terminal perf_grid_rebuild_per_frame -- --ignored --nocapture
```

Tutti i comandi che compilano `sirio_terminal` richiedono **Zig esattamente 0.15.2**. Prima di build/test verificare diagnostica LSP. `Scripts/ci.sh` e `Scripts/ci-linux.sh` restano eseguibili **solo su richiesta esplicita dell'utente**; il gate richiesto prima di una PR non è stato eseguito né implicitamente autorizzato.

## 6. Decisioni da prendere e ambiti esclusi

### Ordine consigliato da confermare

- **Prima:** baseline e contatori mirati, soprattutto su Changes/rightsidebar e sulle attese UI.
- **Primo gruppo circoscritto:** S1 (proiezioni Changes), S7 (focus discovery), S2 (coda per activity detection), ciascuno separatamente.
- **Secondo gruppo:** S3 (streaming/metadati chat) e S5 (acquisizione sessione); richiedono più caratterizzazione.
- **Successivi secondo profiling:** S4, S6, S8, S9. Nessuna modifica automatica di scheduler/renderer.

La scelta del gruppo da trasformare in piani esecutivi resta all'utente. Questo documento non autorizza implementazione, commit, push o issue.

### Ipotesi respinte o corrette

- “Sirio non ha cache del testo o virtualizzazione”: falso; sono già presenti.
- “Il debounce del writer elimina il costo degli snapshot”: falso; `layout(cx)` viene preparato prima.
- “Changes virtualizzato costa solo quanto la viewport”: vero per gli elementi disegnati, non per tutta la preparazione dei dati.
- “Zed non blocca mai il thread UI sul terminale”: falso; `Terminal::sync` prende un lock bloccante.
- “4 ms significa 250 FPS”: falso; in Zed è una finestra di batching, in Sirio anche un intervallo di polling.
- “Nascondere un terminale ne ferma ogni lavoro”: falso e non desiderabile; PTY, attività e protocollo devono proseguire.
- “Un task async esegue automaticamente il lavoro fuori dal main thread”: falso; contano executor e lock condivisi.
- “Basta portare tutte le modifiche GPUI upstream”: non giustificato; fork e patch di piattaforma vanno confrontati e misurati.

### Licenze

Zed dichiara **Apache-2.0 per GPUI**, ma **GPL-3.0-or-later per `terminal` e `git_ui`**. Usare questa ricerca per adottare principi e implementazioni proprie compatibili con Sirio; non trattare tutto il repository Zed come codice liberamente copiabile sotto la licenza di Sirio. Prima di un porting testuale verificare le licenze del modulo e delle dipendenze coinvolte.

Fonti: [licenza GPUI][Z-license-gpui], [licenza terminale][Z-license-terminal], [licenza Git UI][Z-license-git].

### Non verificato

Nessun profiling live comparativo Zed/Sirio; nessuna misura di smoothness, consumo energetico o input-to-photon; nessuna copertura esaustiva dell'editor Zed, CRDT, rope, LSP o di tutti i percorsi agent; nessuna dimostrazione di parità dei backend GPU su tutti gli OS. Queste aree non sono necessarie per confermare i costi applicativi sopra, ma impediscono promesse quantitative o conclusioni globali del tipo “Sirio diventerà veloce quanto Zed”.

[Z-view]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/src/view.rs#L223-L509
[Z-notify]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/src/app.rs#L2690-L2723
[Z-list]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/src/elements/uniform_list.rs#L473-L510
[Z-text]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/src/text_system/line_layout.rs#L639-L690
[Z-atlas]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui_wgpu/src/wgpu_atlas.rs#L108-L128
[Z-gpu]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui_wgpu/src/wgpu_renderer.rs#L1605-L1647
[Z-events]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/terminal/src/terminal.rs#L1402-L1467
[Z-sync]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/terminal/src/terminal.rs#L2411-L2430
[Z-frame]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/src/window.rs#L1771-L1825
[Z-files-default]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/assets/settings/default.json#L858-L868
[Z-git-default]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/assets/settings/default.json#L1060-L1066
[Z-dock]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/workspace/src/dock.rs#L945-L980
[Z-files]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/project_panel/src/project_panel.rs#L4351-L4388
[Z-files-list]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/project_panel/src/project_panel.rs#L7321-L7343
[Z-git-schedule]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/git_ui/src/git_panel.rs#L5069-L5106
[Z-git-list]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/git_ui/src/git_panel.rs#L7760-L7785
[Z-diff]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/git_ui/src/project_diff.rs#L130-L184
[Z-diff-yield]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/git_ui/src/diff_multibuffer.rs#L679-L685
[Z-license-gpui]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/gpui/Cargo.toml#L9
[Z-license-terminal]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/terminal/Cargo.toml#L6
[Z-license-git]: https://github.com/zed-industries/zed/blob/20d3cd1d7981ed2076be5b72c817a1febbba3cb9/crates/git_ui/Cargo.toml#L6
