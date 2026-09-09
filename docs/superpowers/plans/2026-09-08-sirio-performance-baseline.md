# Sirio — protocollo di baseline prestazionale

**Stato:** protocollo proposto; misure non eseguite. La scelta «Baseline e piano» autorizza la preparazione di questi documenti, non l'implementazione o l'esecuzione dei comandi elencati.

**Obiettivo:** separare lavoro evitabile, latenza UI e correttezza per valutare tre interventi indipendenti: S1 Changes, S7 discovery al ritorno del focus, S2 contenuto recente per activity detection.

**Fonte:** [audit Zed → Sirio](../../../plans/004-zed-sirio-performance-audit.md). L'audit è una ricerca, non una specifica approvata. Questo protocollo e il [piano della prima tranche](2026-09-08-sirio-performance-first-wave.md) sono la proposta da approvare prima di modificare sorgenti.

## 1. Stato osservato e limiti

Verifica in sola lettura sul commit `5a030ba18423ea9f8895d0dfa9ea8e90034a9b88`:

- Branch `main`, due commit avanti a `origin/main`; nessuna modifica a file tracciati all'inizio della pianificazione.
- Il rapporto `plans/004-zed-sirio-performance-audit.md` era già presente e non tracciato: non modificarlo o includerlo automaticamente in commit.
- Host locale: Darwin arm64; `rustc 1.98.1`, `cargo 1.98.1`, Zig **0.15.2**, verificati tramite i rispettivi comandi versione.
- GPUI: alias `bezel-gpui =0.3.8`; lockfile `0.3.8+zed.82aeef`; bezel `=0.1.4`. Non aggiornare dipendenze o override `rust/vendor/` per questi esperimenti.
- Nessuna build, test, misura runtime, CI o comparazione con Zed eseguita durante questa pianificazione. Hardware, refresh display e condizioni termiche non censiti.
- Il risultato locale non rappresenta Linux/Wayland, Linux/X11 o Windows. Ripetere i workload sul sistema destinatario prima di generalizzare.

## 2. Strumenti realmente esistenti

I percorsi di seguito sono relativi alla radice del repository; le righe si riferiscono al commit sopra.

| Strumento | Cosa misura davvero | Limite |
| --- | --- | --- |
| `sirio_ui/src/changes.rs::measure_big_diff` (5601–5670) | Tempo wall-clock di preparazione righe, draw GPUI e dispatch scroll in finestra di test 1200×800 | Usa il minimo di 5 misure; non p95/p99 e non presentazione al display |
| `changes.rs::perf_a_large_expanded_diff_costs_a_frame_proportional_to_the_viewport` (5677–5703) | Rapporto draw fra 300 e 5.000 righe; soglia esistente `<4` | Non copre centinaia di file collassati, payload condivisi o 15.000 righe |
| `sirio_terminal/src/lib.rs::perf_keystroke_echo_latency` (9259–9319) | Tempo simulato fino alla notifica della view | Non key-to-photon |
| `sirio_terminal/tests/still_frame_cost.rs::still_pane_frame_cost` | Media wall-clock su 200 iterazioni della finestra GPUI di test dopo warm-up | Non distribuzione dei frame presentati; numeri storici nel commento non riprodotti |
| `sirio_terminal/src/lib.rs::perf_grid_rebuild_per_frame` (9423–9461) | Costo snapshot terminale | Non intera pipeline di rendering |
| `sirio_terminal/src/lib.rs::perf_idle_terminals_burn_cpu` (9342–9396) | Diagnostico di handle terminali idle | Non istanzia `TerminalView`, quindi non misura il pump GPUI per-pane |
| `Scripts/bench-workspace.sh` | Nessuna misura sulla versione Rust | Script storico Swift: termina con errore esplicito. **Non usarlo.** |

I contatori proposti sotto **non esistono ancora**. Il numero `renders` di Changes non prova quante proiezioni o copie del payload vengono prodotte.

## 3. Tre approcci e scelta raccomandata

1. **Contatori deterministici + diagnostici locali + profiling nativo:** raccomandato. Verifica l'eliminazione del lavoro e poi se questo migliora l'interazione reale. Più setup rispetto a un solo test, ma distingue correttezza, CPU e presentazione.
2. **Solo benchmark esistenti:** utile come primo smoke test; non basta per decidere S7/S2 o il costo dei file collassati.
3. **Subito scheduler/renderer/upstream GPUI:** escluso. Mescola troppe variabili e rischia le patch di piattaforma senza aver attribuito il collo di bottiglia.

Non creare un framework generale di telemetry o un nuovo crate per tre esperimenti. Usare strumenti locali ai moduli e, se serve profilare l'app reale con contatori, una feature Cargo esplicita `perf-probes`, disattivata per default; nessuna dipendenza nuova. Questa feature è una **proposta**, non un comando oggi disponibile.

## 4. Disegno sperimentale

### Revisioni e autorizzazioni

- **R0:** commit base, diagnostici già esistenti, nessuna ottimizzazione.
- **R1:** R0 più fixture/contatori/diagnostici, senza cambiare scheduling o percorso di acquisizione. È la baseline strumentata.
- **R2-S1 / R2-S7 / R2-S2:** R1 più un solo intervento. Misurare ogni candidato separatamente; successivamente verificare la composizione.
- Identificare ogni prova con commit completo, diff eventualmente non committato, versione del protocollo e feature attive. Non attribuire a R0 risultati ottenuti dopo l'ottimizzazione.
- Creare branch/worktree solo al momento dell'esecuzione, seguendo le regole del repository. Niente reset, clean o scarto di modifiche preesistenti.
- `Scripts/ci.sh` e `Scripts/ci-linux.sh` richiedono richiesta esplicita separata; nessun gate workspace autonomo. Commit, push, PR e subagent richiedono autorizzazione.

### Condizioni

Per ciascun host registrare OS/versione, CPU, RAM, GPU, backend/compositor, scala, risoluzione e refresh display, font e dimensione, numero di split e visibilità dei pane. Registrare profilo Cargo, feature, toolchain, commit, cache calda/fredda, alimentazione e carico concorrente.

- Correttezza: test per-crate in profilo normale.
- Misure: profilo `--release`, workload seriali, compilazione conclusa prima del timer. Non confrontare un debug con un release.
- File Git sintetici in directory temporanee, mai nella checkout dell'utente. Contenuti fissi e pubblicabili; niente prompt, log agent o dati reali.
- Cache calda: 10 frame iniziali per draw; un passaggio di fixture/discovery/cattura non contato per gli altri workload.
- Cold-start separato: non aggiungerlo alla distribuzione steady-state.
- Registrare una prova non profilata e una profilata separatamente; overhead dei probe misurato confrontando feature off/on, non sottratto arbitrariamente.

### Campioni e statistica

- Draw/preparazione/cattura: 5 ripetizioni indipendenti, ciascuna da 1.000 campioni dopo warm-up. Alternare baseline/candidato (A/B, B/A) per ridurre il bias temporale.
- Focus/discovery reale: 5 ripetizioni da 100 transizioni deliberate. Per ogni ripetizione riportare mediana, p95 e massimo; non vendere il p99 di 100 eventi come coda stabile.
- Percentili con nearest rank `sorted[ceil(p*n)-1]`; registrare anche campioni grezzi numerici, numerosità e variabilità fra ripetizioni. Niente minimo come unico risultato.
- Non rimuovere campioni lenti a posteriori. Se il workload è interferito, marcare l'intera ripetizione e rifarla, conservando il motivo.
- I budget 16,7 ms/60 Hz e 8,3 ms/120 Hz sono riferimenti per i **frame presentati**, non soglie universali dei singoli helper.

### Record da produrre

File futuri, creati soltanto durante l'esecuzione:

- `docs/performance/2026-09-08-first-wave-results.md`: ambiente, revisioni, sintesi, verdetto e limiti per esperimento.
- Campioni/profili in una directory di artefatti esterna alla checkout; il report ne riporta percorso e checksum, non contenuti terminale.

Schema minimo per un campione: `revision`, `profile`, `features`, `host_id`, `scenario`, `repeat`, `sample`, `metric`, `unit`, `value`. Per esempio `metric=changes_projection_builds`, `unit=count`; mai mischiare conteggi e millisecondi nella stessa colonna semantica.

## 5. Workload e punti di misura

### B-S1 — Changes / sidebar e superficie diff

**Fixture:** riusare `synthetic_big_diff_tab` e aggiungere una fixture multi-file. Matrice: 300 / 5.000 / 15.000 righe in un file; 300 file da 50 righe ciascuno; tutto collassato, un file espanso, tutti espansi. Stesso totale di righe non implica stesso costo, quindi riportare entrambe le dimensioni.

**Azioni:** prima apertura; 1.000 invalidazioni della sola view senza nuovo snapshot; scroll; selezione up/down; resize; unified↔split; expand/collapse di sezione/file/band; nuovo snapshot con testo diverso ma identiche coordinate; drag dal file attualmente visibile. Separare tick del refresh Git e redraw puro usando la fixture commit-mode già esistente.

**Probe da aggiungere:** entrate in `render_body`, costruzioni `section_rows`, righe appiattite/hashate, `ListState::splice`, invocazioni `diff_payload` e byte prodotti; costruzioni proiezione e cache hit dopo S1. Contare la serializzazione e la copia integrale dei payload separatamente dalle copie dei riferimenti.

**Tempi:** preparazione proiezione, draw test-GPUI, dispatch input, durata applicazione snapshot. Un payload grande con il file header visibile deve avere un caso dedicato: la virtualizzazione non evita il clone di una `String` contenuta in quella riga.

**Criterio strutturale dopo S1:** a snapshot/mode/espansione invariati, zero ricostruzioni, hash globali e serializzazioni payload aggiuntive durante redraw e navigazione; un nuovo snapshot invalida anche quando `hash_identity` è identico. La selezione deve continuare a scrollare in vista sui cache hit.

### B-S7 — ritorno del focus

**Fixture:** progetto temporaneo con 1 / 20 / 100 worktree. La fixture lenta usa un loader controllabile con canali/barriere iniettati, non `sleep` come sincronizzazione dei test. In un run diagnostico distinto, ritardare il loader di 250 ms; il ritardo è sintetico, non la latenza Git del prodotto.

**Azioni:** perdita/ritorno del focus; focus storm mentre la discovery è bloccata; selezione A→B; rimozione/reinserimento progetto con lo stesso id; refresh da create/remove worktree mentre è pendente quello da focus; uscita dalla view prima della risposta; rimozione esterna di un worktree con PTY ancora montato.

**Probe:** richieste, job avviati, massimo job contemporanei, pending, risultati applicati/scartati, durata discovery background, durata apply foreground, durata render di focus. Contare separatamente le letture `read_head_label`.

**Criterio strutturale:** con discovery bloccata, un frame e un evento input vengono serviti; nessuna esecuzione di `discover_project`/processo Git nella porzione sincrona del render al focus. Un job globale in-flight e al massimo una richiesta pending per progetto vivo; raffiche sullo stesso progetto collassate. Risposte obsolete non ripristinano selezioni o progetti rimossi.

**Limite intenzionale:** `refresh_worktree_branches` legge ancora HEAD; la fusione del catalogo può ancora normalizzare path sul filesystem. Questa tranche elimina la discovery Git dal render, **non dimostra zero I/O sul thread UI**. Se apply/HEAD restano dominanti, riportarlo e chiedere un intervento distinto.

### B-S2 — contenuto recente dal terminal owner

**Fixture:** 80 / 240 colonne, viewport 24 / 60 righe, history 0 / 1.000 / fino al limite effettivamente trattenuto. Il test helper corrente configura `max_scrollback=10_000`: verificare `total_rows`, non equiparare le righe inviate a quelle conservate.

**Contenuto:** ASCII, righe vuote interne/finali, CJK largo, accenti combinati, emoji ZWJ/flag, wrapping, resize, alternate screen, riga singola oltre 10 KiB in UTF-8. Testare confini 39/40/41 righe e 10.239/10.240/10.241 byte.

**Oracle di correttezza:** sullo stesso stato del terminale, `recent_content_window(&capture_scrollback_text(&mut term))`. Gli output devono essere byte-per-byte uguali. I grapheme vengono catturati come oggi; il taglio iniziale mantiene la semantica UTF-8 del matcher, non introduce un nuovo contratto di taglio per grapheme.

**Probe:** richieste Text complete vs richieste recenti, righe/celle visitate, byte prodotti, durata owner della cattura, tempo dalla richiesta alla risposta. La durata richiesta-risposta include la coda e non va chiamata tempo di estrazione.

**Criterio strutturale:** nella fixture con coda non vuota e stessa viewport, le celle lette dalla cattura recente non crescono aggiungendo history prima della finestra utile; il pump non richiede Text completo. Persistenza e control socket continuano a ricevere lo scrollback completo.

**Limite intenzionale:** trovare l'ultima riga non vuota può richiedere di attraversare una lunga coda vuota, fino all'intera history. Misurare anche questo caso; non promettere O(40) assoluto e non troncare prima di trovare la coda solo per far passare il benchmark.

## 6. Instrumentazione proposta, senza distorcere il percorso caldo

- Contatori di correttezza `#[cfg(test)]` per istanza/fixture, non globali che interferiscono fra test.
- Eventuali probe dell'app sotto feature `perf-probes`: feature forwarded dall'app ai soli crate interessati, disattivata per default; capability da aggiungere e verificare prima di usare i relativi comandi.
- Timer `std::time::Instant` intorno alle tre operazioni, buffer numerici bounded e output a fine run. Niente `println!`, file I/O o costruzione JSON per cella/riga/frame.
- Nessun contenuto terminale, path utente o diff nei record; id sintetici e conteggi. I log di errore eventualmente raccolti vanno revisionati prima di condividere artefatti.
- Profiling nativo: Instruments sul Mac locale; strumento appropriato al compositor/backend sul sistema target. Installazione strumenti, lancio app e cattura interattiva richiedono un passo di esecuzione concordato.

## 7. Comandi esistenti per la futura esecuzione

Prima: controllare LSP dei file interessati, poi versioni e prerequisiti. Non sono stati eseguiti qui.

```bash
cd rust
cargo test -p sirio_ui --lib perf_a_large_expanded_diff_costs_a_frame_proportional_to_the_viewport -- --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_keystroke_echo_latency -- --nocapture --test-threads=1
cargo test -p sirio_terminal --test still_frame_cost -- --ignored --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_grid_rebuild_per_frame -- --ignored --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_idle_terminals_burn_cpu -- --ignored --nocapture --test-threads=1
```

Per diagnostici wall-clock ripetere con `--release` immediatamente dopo `cargo test`; non usare tempi di compilazione come campioni. Un test filtrato che esegue **zero test** è una verifica fallita del protocollo, non un PASS.

I diagnostici nuovi del piano devono essere prima aggiunti e verificati; nessun comando non ancora implementato è presentato come strumento pronto.

## 8. Decisione keep / stop / rollback

1. Prima gate funzionale e strutturale: un test rosso o un risultato stale blocca l'intervento, anche con tempi migliori.
2. Per ciascuna delle 5 coppie A/B calcolare il delta della metrica primaria: draw caldo p95 per S1, render-focus p95 con discovery lenta per S7, estrazione owner p95 per S2.
3. Accettare una rivendicazione prestazionale solo con miglioramento nella stessa direzione in almeno 4 coppie su 5 e delta mediano oltre la variabilità osservata fra ripetizioni. Registrare tutti i numeri; non inventare una percentuale garantita in anticipo.
4. La metrica primaria non basta: controllare cold load, apply, memoria/RSS, drag, terminal input e correttezza della freshness. Peggioramenti ripetibili fuori rumore richiedono revisione, non vengono nascosti dalla media.
5. Risultato entro rumore: non promuovere la patch come ottimizzazione; scartarla dalla tranche o approfondire solo con nuova approvazione. Conservarne l'esito nel report.
6. Rollback solo dei commit dell'esperimento, dopo autorizzazione; niente reset della checkout. Prima di commit usare le convenzioni e le skill del repository. Nessuna PR prima del gate esplicitamente richiesto dall'utente.

## 9. Checklist di handoff

- [ ] Protocollo e piano approvati per l'esecuzione.
- [ ] Host target e condizioni registrati, strumenti disponibili.
- [ ] R0 misurata con strumenti esistenti; limiti annotati.
- [ ] R1 strumentata e verificata senza ottimizzazioni; campioni salvati.
- [ ] Ogni candidato misurato separatamente contro R1.
- [ ] Test funzionali, gate strutturali e verifica live completati.
- [ ] Report contiene anche esiti negativi, campioni insufficienti e blocker.
- [ ] Eventuale CI/commit/PR autorizzati separatamente.
