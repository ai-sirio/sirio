# Prima tranche prestazionale — checkpoint B0/B1

**Data:** 2026-09-08. **Ambito autorizzato:** baseline B0/B1, esecuzione inline.

**Aggiornamento 2026-09-09:** [ripetizione su alimentazione esterna](2026-09-09-first-wave-ac-rerun.md) eseguita sul medesimo binario release: altri 140.000 campioni, conteggi identici, minore ma persistente variabilità temporale. Questo report conserva i risultati originali dell'8 settembre.

## Verdetto

- **R0 eseguita:** cinque diagnostici preesistenti, debug e release; dieci esecuzioni con almeno un test effettivo, tutte passate.
- **R1-S1 strumentata e verificata:** sonde test-only e sette workload Changes; raccolti **140.000 campioni numerici**, senza ottimizzazioni.
- **Conteggi strutturali utilizzabili. Tempi R1 non accettati come baseline:** prova complessivamente marcata **interferita**. Mac a batteria, carico concorrente non controllato; load average a un minuto 9,00 prima dell'invocazione Cargo e 49,85 dopo. Queste due letture includono anche la compilazione e non attribuiscono il carico a una specifica ripetizione/processo. Prudenzialmente tutte le ripetizioni sono da rifare per confronti temporali.
- **Checkpoint B ancora aperto per i tempi.** Nessuna patch S1-A/S1-B/S7/S2 implementata, nessun beneficio prestazionale rivendicato. Prossimo passo: ripetere i workload su alimentazione esterna e con carico stabile, conservando questa prova anziché scartare campioni lenti.

Riferimenti: [protocollo](../superpowers/plans/2026-09-08-sirio-performance-baseline.md), [piano](../superpowers/plans/2026-09-08-sirio-performance-first-wave.md), [audit](../../plans/004-zed-sirio-performance-audit.md). I tre documenti preesistenti non sono stati modificati né inclusi in commit.

## Revisioni e ambiente

| Voce | Valore |
| --- | --- |
| R0 / HEAD | `5a030ba18423ea9f8895d0dfa9ea8e90034a9b88` |
| Branch di lavoro | `perf/first-wave-baseline`, creato da HEAD; nessun commit |
| R1 | HEAD + patch `917959c2be6bfb091c10f6f1d7c7b938dd7701c8a2d1a69a88681f37456f4ebb` (SHA-256) |
| Host | Apple M4, arm64, RAM 24 GiB; GPU M4 10 core |
| OS | macOS 26.6.2, build 25G83 |
| Toolchain | rustc/cargo 1.98.1; Zig **0.15.2**, `/opt/homebrew/bin/zig` |
| Display fisico | 3420×2224, logico 1710×1112 @ 60 Hz |
| Finestra delle fixture Changes | 1200×800 pixel logici, una superficie Changes visibile, modalità unified |
| Tema/font | `Theme::init`, impostazioni di default del test, nessun override; non censita la risoluzione effettiva del font di sistema |
| Profilo R1 temporale | `--release`, feature Cargo default; sonde attive/inattive sono modalità del collector, **non feature Cargo** |
| Alimentazione / carico R1 | batteria al 63% prima del run; load 1/5/15 min prima 9,00/8,41/12,01, dopo 49,85/33,85/23,93 |
| Termica | prima: nessun warning riportato da `pmset`; nessuna serie termica durante il run |
| Backend misurato | finestra di test GPUI; non presentazione sul display, non profiling dell'app reale |

Il run R1 è iniziato intorno alle 20:02 locali. L'invocazione Cargo ha impiegato circa 409 s, di cui 319,76 s nel test; **i tempi di compilazione non entrano nei campioni**. Le condizioni di alimentazione/carico sopra sono state raccolte per R1, non retroattivamente per R0.

## R0: diagnostici preesistenti

| Diagnostico | Debug | Release | Statistica/limite |
| --- | --- | --- | --- |
| Changes, 300 righe | draw 6,77 ms; section rows 0,04 ms | draw 0,54 ms; section rows 0,01 ms | draw: minimo storico di 5; non p95 |
| Changes, 5.000 righe | draw 8,58 ms; section rows 0,38 ms | draw 0,95 ms; section rows 0,15 ms | rapporto large/small 1,3× / 1,8×, soglia `<4` passata |
| Keystroke echo | PASS | PASS | asserzione `≤16 ms` **simulati**, nessun valore esatto stampato su successo |
| Still frame | 4,142571 ms | 0,353663 ms | media di 200 frame della finestra di test |
| Grid rebuild | 1,344506 ms | 0,581528 ms | costo su 10.000 celle |
| Idle CPU, 8 terminali, 2 s | overhead 23,171 ms | overhead 53,926 ms | baseline 9 µs / 44 µs; non misura il pump delle `TerminalView` |

Scroll Changes small/large: dispatch debug 7,27/9,02 ms, release 0,59/1,00 ms; scroll-frame debug 6,97/8,62 ms, release 0,56/0,96 ms. Sono i minimi del diagnostico storico, non distribuzioni.

Le prime invocazioni Changes debug sono state ripetute per correggere la raccolta log (`tee` con percorso relativo errato, poi output compresso). La tabella usa l'ultima cattura **completa** su file, non una prova scelta per il risultato temporale. I log iniziali incompleti non costituiscono campioni della distribuzione R1. Non confrontare direttamente R0 minimo con R1 p95, né debug con release.

## R1: cosa è cambiato

- `rust/crates/sirio_ui/src/changes.rs`: agganci `#[cfg(test)]` dentro `section_rows`, `sync_list_rows`, `render_body`, `diff_payload` e immediatamente prima della copia della riga nel callback di disegno. Fixture esistente resa visibile al modulo test fratello.
- `rust/crates/sirio_ui/src/changes/perf_baseline.rs`: collector scoped, fixture multi-file, test deterministici, nearest-rank e diagnostico ignored con export CSV.
- Il collector possiede i conteggi per esercizio sincrono della fixture; il thread-local instrada soltanto le chiamate. Scope annidati ripristinano il precedente anche in unwinding. Nessuno scope attraversa `.await`. I contatori non sono condivisi fra fixture/test.
- Le invocazioni di `diff_payload` sono contate **nel builder**, anche quando invocato direttamente o quando rifiuta un binary. I byte serializzati sono registrati dopo la costruzione. `draw_payload_*` conta esclusivamente la copia del testo nel `row.clone()` della lista, non ogni allocazione/clone dell'app o della libreria GPUI.
- `flattened_rows` e `hashed_rows` contano le righe visitate da `sync_list_rows`; il secondo non è il numero di chiamate primitive al hasher.
- Nessun nuovo campo in `ChangesTab`, cache, cambio di scheduling, dipendenza, API/protocollo o schema persistito. Build non-test: sonde e collector esclusi. Il diagnostico storico basato sul minimo resta invariato.

Due autofix del runner avevano toccato `browser.rs` e `file_view.rs`: sono stati annullati puntualmente prima delle verifiche e misure R1. Non fanno parte della patch misurata.

### Disegno della raccolta

Sette scenari × cinque ripetizioni × due modalità collector × due metriche × 1.000 campioni = **140 serie / 140.000 campioni**. Ogni serie ha dieci warm-up, non esportati. Ordine collector alternato inattivo/attivo, attivo/inattivo tra ripetizioni.

- `prepare`: costruzione sezioni, appiattimento/hash e distruzione delle righe temporanee; tempo nell'update dell'entity, escluso l'ingresso GPUI.
- `draw`: `window.draw(cx).clear(cx)` dopo `cx.notify`; la notifica resta fuori dal timer.
- I dati sono bufferizzati; CSV e stampa dei percentili avvengono **dopo** il ciclo misurato. Tutti i campioni sono conservati. File creati con `create_new`: una ripetizione successiva deve scegliere una directory nuova.
- Nei CSV `sample=0` rappresenta il totale di un contatore sulle 1.000 operazioni; gli altri sample sono durate in ns. Le serie con collector inattivo non esportano conteggi: non osservato non significa lavoro zero.
- Collector inattivo lascia i punti di dispatch test-only: attivo/inattivo stima solo l'incremento del collector, non l'intero costo rispetto a un binario privo di instrumentation. Non è stata aggiunta una feature live `perf-probes`.

### Conteggi strutturali per redraw puro

Le sette serie seguenti hanno una costruzione sezioni e una costruzione lista per redraw. Il fingerprint resta invariato: **zero splice aggiuntivi** dopo warm-up, ma continua la preparazione globale.

| Scenario | Righe appiattite/hashate | Serializzazioni | Byte serializzati | Copie payload nel draw | Byte copiati nel draw |
| --- | ---: | ---: | ---: | ---: | ---: |
| 300 righe, espanso | 303 | 1 | 22.490 | 1 | 22.490 |
| 5.000 righe, espanso | 5.003 | 1 | 391.692 | 1 | 391.692 |
| 15.000 righe, espanso | 15.003 | 1 | 1.196.692 | 1 | 1.196.692 |
| 15.000 righe, header fuori vista | 15.003 | 1 | 1.196.692 | 0 | 0 |
| 300×50 righe, file collassati | 301 | 300 | 1.091.400 | 25 | 90.950 |
| 300×50, un file espanso | 352 | 300 | 1.091.400 | 1 | 3.638 |
| 300×50, tutti espansi | 15.601 | 300 | 1.091.400 | 1 | 3.638 |

Le righe includono header di sezione, file e hunk; non sono solo linee di testo. Le copie visibili dipendono dalla viewport della fixture. Il caso header nascosto conserva tutta la serializzazione ma elimina la copia nel disegno: prova che i due costi sono distinti. Un test separato usa 300 path, con il primo presente sia staged sia changed: 301 chiamate al builder, senza condivisione fra sezioni nella versione attuale.

### Tempi raccolti — prova interferita, non baseline accettata

Valori in ms, collector inattivo. Le colonne centrali sono la **mediana dei cinque percentili**, non percentili di un pool unico. `summary.json` conserva p50/p95/p99/max di ogni singola serie e i CSV permettono il ricalcolo.

| Scenario | Mediana p50 draw | Mediana p95 draw | Range p95 fra ripetizioni | Mediana p95 prepare |
| --- | ---: | ---: | ---: | ---: |
| 300 righe | 1,234 | 1,671 | 1,243–4,487 | 0,062 |
| 5.000 righe | 2,424 | 4,985 | 2,422–7,142 | 0,952 |
| 15.000 righe | 5,458 | 9,314 | 8,111–10,125 | 6,085 |
| 15.000, header nascosto | 3,554 | 4,690 | 3,256–7,573 | 3,435 |
| 300 file collassati | 0,875 | 0,974 | 0,837–1,360 | 0,242 |
| 300 file, uno espanso | 0,933 | 1,029 | 0,928–1,321 | 0,263 |
| 300 file, tutti espansi | 2,928 | 3,647 | 2,808–13,184 | 2,275 |

Anche i delta appaiati active/inactive del p95 hanno segno e ampiezza instabili (ad esempio, tutti i file espansi: +92,9%, +20,4%, +133,5%, −17,9%, −33,0%). **Non è una stima affidabile dell'overhead** e non viene sottratta dai tempi. Non si attribuisce questa variazione a uno specifico processo o a throttling non misurato.

## Verifiche eseguite

| Comando / verifica | Risultato |
| --- | --- |
| Sonde prima degli agganci, `cargo test -p sirio_ui --lib changes::perf_baseline -- --test-threads=1` | RED: 5 failure comportamentali, 1 pass; compilazione riuscita, contatori 0 invece degli attesi |
| Stesso filtro dopo gli agganci e matrice aggiuntiva | GREEN: 7 pass, 1 ignored |
| `cargo test -p sirio_ui --lib changes::` con un thread | 54 pass, 1 ignored |
| Sonde con parallelismo libtest di default | 7 pass, 1 ignored |
| `cargo test -p sirio_ui --lib changes::perf_baseline::changes_hot_baseline --release -- --ignored --exact --nocapture --test-threads=1` | 1 pass effettivo, 140 serie esportate |
| `cargo build -p sirio_ui` | exit 0, build non-test |
| LSP primario sui due file sorgente della patch | entrambi confermati senza errori dopo retry dei timeout iniziali |
| `lens_diagnostics mode=all` | nessun errore bloccante in cache; warning preesistenti nei file browser/file_view ripristinati e hint nel terminale, non una scansione workspace completa |
| Identità sorgenti rispetto a `instrumentation.patch` | SHA-256 identico dopo build/misure |
| `git diff --check` | nessun errore |

Non eseguiti: suite intera `sirio_ui`, CI/workspace gate, profiling nativo, smoke interattivo dell'app. Nessun commit/push/PR, nessun subagent. I warning preesistenti del compilatore non sono stati corretti per rendere artificialmente verde la baseline.

## Riproduzione

Dalla radice del repository, per i diagnostici R0 (ripetere anche con `--release` subito dopo `cargo test`):

```bash
cd rust
cargo test -p sirio_ui --lib perf_a_large_expanded_diff_costs_a_frame_proportional_to_the_viewport -- --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_keystroke_echo_latency -- --nocapture --test-threads=1
cargo test -p sirio_terminal --test still_frame_cost -- --ignored --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_grid_rebuild_per_frame -- --ignored --nocapture --test-threads=1
cargo test -p sirio_terminal --lib perf_idle_terminals_burn_cpu -- --ignored --nocapture --test-threads=1
```

Per ripetere R1, dopo diagnostica LSP e verifica del toolchain, scegliere una **directory esterna nuova** e impostare l'identità della patch realmente eseguita:

```bash
cd rust
SIRIO_PERF_SAMPLE_DIR=/absolute/external/new-run \
SIRIO_PERF_REVISION=5a030ba18423ea9f8895d0dfa9ea8e90034a9b88+r1.917959c2be6b \
SIRIO_PERF_HOST=darwin-arm64-m4-24g \
cargo test -p sirio_ui --lib changes::perf_baseline::changes_hot_baseline \
  --release -- --ignored --exact --nocapture --test-threads=1
```

L'identificativo sopra vale solo per i sorgenti della patch archiviata. Un'altra patch/host richiede identificativi aggiornati. `--ignored` senza filtro non è equivalente al comando riportato. Le durate sono nearest-rank `sorted[ceil(p*n)-1]`, senza esclusione dei campioni lenti.

## Artefatti e integrità

Directory locale esterna alla checkout, non caricata su servizi remoti:

```text
/Users/enzopiopalmisano/Desktop/Progetti/sirio-perf-artifacts/2026-09-08-first-wave/
  r0-debug/*.log
  r0-release/*.log
  r1/instrumentation.patch
  r1/environment.json
  r1/measurement-quality.json
  r1/run-status.json
  r1/*probes.log, changes-regressions.log, production-build.log
  r1/baseline-release.log
  r1/samples-release/*.csv
  r1/summary.json
  summarize.py
  SHA256SUMS
```

Manifesto di **162 artefatti**, SHA-256 di `SHA256SUMS`:

`641e3b0ac82bbd08829dfa2665dca5db695e62dbce0853de8d6a307090a11f66`

`python3 <directory>/summarize.py` ricalcola summary e manifesto dai CSV, verificando 1.000 sample distinti per serie e 140 serie. I dati non includono testo terminale, prompt o diff dell'utente; solo fixture sintetiche, conteggi, tempi, codice della patch e log tecnici locali.

## Residui e prossimo checkpoint

- [x] R0, ambiente disponibile, diagnostici esistenti registrati.
- [x] R1-S1: test delle sonde, fixture multi-file, redraw caldo/preparazione, header visibile/nascosto, campioni e integrità.
- [ ] Ripetizione temporale in condizioni controllate; overhead collector ancora non stimabile.
- [ ] Estendere la baseline quando si avvia S1: cold snapshot/apply, navigazione, scroll/resize, switch unified/split, drag reale e memoria/RSS. Questa tranche misura redraw e preparazione, non l'intera matrice del protocollo.
- [ ] Probe live/feature off-on, profiling dell'app e piattaforme target: non eseguiti.
- [ ] S7 e S2: nessuna strumentazione nuova; soltanto i diagnostici terminali R0 preesistenti.
- [ ] Approvazione separata prima delle ottimizzazioni; nessuna promozione sulla base di tempi interferiti.

**Esito operativo:** strumentazione e evidenza strutturale pronte per revisione. Conservare la prova temporale negativa e rifarla prima di usare R1 come riferimento di accettazione prestazionale.
