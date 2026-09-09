# R1-S1 — ripetizione su alimentazione esterna

**Data:** 2026-09-09, circa 09:27–09:30 CEST. **Ambito:** ripetere la baseline approvata, senza implementare ottimizzazioni.

Segue il [checkpoint B0/B1 dell'8 settembre](2026-09-08-first-wave-results.md). La prova precedente e i suoi artefatti restano intatti.

## Esito

- Diagnostico effettivamente eseguito: **1 pass, 0 failure**, 166,78 s nel test.
- Sette scenari, cinque ripetizioni, collector attivo/inattivo, preparazione/draw: **140 serie e 140.000 nuovi campioni**.
- Tutti i conteggi per redraw, in tutte le cinque ripetizioni di ogni scenario, sono **identici al primo run**. Confermate ricostruzione globale delle righe, serializzazione dei payload e copie per header visibile.
- Preflight su AC superato, ma **resta variabilità temporale**: load a un minuto durante il run 4,03–15,67; p95 draw del diff da 15.000 righe 2,502–6,055 ms. Non è stata attribuita a processi specifici e il load include il benchmark stesso.
- **Checkpoint temporale di precisione ancora aperto.** I tempi sono descrittivi; non dimostrano un beneficio di codice né consentono una stima precisa dell'overhead del collector. Nessuna ripetizione è stata eliminata per ottenere una distribuzione più favorevole.

## Provenienza: stesso eseguibile, niente ricompilazione

Per non incorporare modifiche estranee nel confronto, è stato riutilizzato il binario release individuato nel log Cargo del primo run:

```text
rust/target/release/deps/sirio_ui-60a46ab7738ad174
```

- mtime del binario: 2026-09-08 20:03:51 CEST, precedente alla chiusura del log originale alle 20:09:12.
- Copia archiviata byte-identica all'eseguibile trovato nella checkout.
- SHA-256 registrato prima del nuovo run e verificato invariato dopo:
  `0f1e7e31bb5c6c38a3682762f6e3324a7c0622b445b693d52c29b8f1e38b21db`.
- Il primo run **non aveva registrato l'hash del binario**: la provenienza storica si basa su log/percorso/mtime, non su un confronto con un hash storico inesistente.
- Identità R1 nei campioni: `5a030ba18423ea9f8895d0dfa9ea8e90034a9b88+r1.917959c2be6b`, patch già archiviata e ricopiata negli artefatti nuovi.

`browser.rs` e `file_view.rs` presentavano nuovamente modifiche di formattazione/autofix all'inizio di questa ripetizione. **Non sono state annullate né compilate** per il run: il binario archiviato non legge i sorgenti correnti per costruire le fixture. Questa verifica non certifica quindi la build dello stato corrente completo della checkout.

## Condizioni e metodo

Stesso host M4/24 GiB, stessa fixture GPUI release 1200×800, unified, tema di default. Nessun profiling nativo o misurazione dei frame presentati sul display.

Preflight: sette letture `top -l 7 -s 5 -n 0`; scartata la prima cumulativa, conservati sei intervalli da cinque secondi. Regola operativa impostata **prima della raccolta**: AC, mediana CPU idle almeno 75%, minimo almeno 60%, nessun processo cargo/rustc/clippy-driver/zig rilevato al termine del preflight. Non è una soglia prestazionale del prodotto.

| Controllo | Osservazione |
| --- | --- |
| CPU idle nei sei intervalli | 83,87%; 79,12%; 82,53%; 78,16%; 83,49%; 72,68% |
| Mediana / minimo idle | 80,825% / 72,68% |
| Alimentazione prima e dopo | AC, batteria al 43% in carica |
| Compilatori al termine del preflight | nessuno rilevato |
| Termica prima/dopo | nessun warning riportato da `pmset`; non monitorata continuamente |
| Load 1 min fine preflight | 3,26 |
| Load 1 min durante le serie | 4,03–15,67 |
| Load 1 min finale | 6,08 |

Durante la misura sono state registrate soltanto letture leggere di `getloadavg()` alla ricezione di ciascuna riga di risultato. `top` e compilazioni non sono stati lanciati da questa sessione durante il benchmark. Non si afferma che altri processi dell'host siano rimasti inattivi per tutta la durata.

Ogni serie ha 10 warm-up e 1.000 campioni; l'ordine active/inactive alterna tra ripetizioni. Export e percentili invariati rispetto a R1. Collector inattivo non equivale a binario senza agganci test-only.

## Risultati temporali

Collector inattivo; millisecondi. Le mediane sono calcolate sui cinque percentili per ripetizione, **non** su un unico pool. Ogni p50/p95/p99/max individuale è conservato in `summary.json`.

| Scenario | Mediana p50 draw | Mediana p95 draw | Range p95 draw | Mediana p95 prepare |
| --- | ---: | ---: | ---: | ---: |
| 300 righe | 0,554 | 0,589 | 0,569–0,614 | 0,028 |
| 5.000 righe | 1,000 | 1,228 | 0,997–2,250 | 0,612 |
| 15.000 righe | 2,818 | 5,194 | 2,502–6,055 | 3,207 |
| 15.000, header nascosto | 1,995 | 2,331 | 2,193–3,302 | 1,522 |
| 300 file collassati | 0,711 | 0,872 | 0,825–0,966 | 0,268 |
| 300 file, uno espanso | 0,793 | 0,929 | 0,910–0,958 | 0,264 |
| 300 file, tutti espansi | 2,294 | 2,417 | 2,308–3,464 | 1,703 |

Alcune serie sono molto più stabili, ma non tutte. Per 15.000 righe, i cinque p95 draw sono 6,055 / 5,891 / 5,194 / 2,502 / 2,741 ms; il massimo singolo campione è 32,775 ms. I delta active/inactive cambiano ancora segno e ampiezza: per 5.000 righe −0,4% / −16,2% / −24,1% / +111,6% / +95,6%. Nessun overhead viene sottratto arbitrariamente.

**Non chiamare il calo rispetto all'8 settembre un'ottimizzazione:** il codice del benchmark e il percorso applicativo misurato sono gli stessi; sono cambiate le condizioni dell'host.

## Evidenza strutturale confermata

- Ogni redraw ricostruisce sezioni e lista; nessuno splice dopo warm-up a geometria invariata.
- 300 file collassati: 300 serializzazioni, **1.091.400 byte prodotti per redraw**; 25 copie dei payload per i file disegnati, 90.950 byte.
- Diff da 15.000 righe: **1.196.692 byte serializzati per redraw**; altrettanti copiati nel disegno con header visibile.
- Header fuori vista: zero copie payload nel draw, ma serializzazione e 15.003 righe appiattite/hashate persistono.
- Questi conteggi non sono RSS, memoria trattenuta o tutte le allocazioni di GPUI; hanno il perimetro dichiarato nel report iniziale.

## Artefatti, riproduzione e integrità

Directory separata, locale, non pubblicata:

```text
/Users/enzopiopalmisano/Desktop/Progetti/sirio-perf-artifacts/2026-09-09-first-wave-ac-092527/
  sirio_ui-baseline
  instrumentation.patch
  preflight.json
  test-list.log
  baseline-release.log
  run-status.json
  load-timeline.json
  measurement-quality.json
  samples-release/*.csv
  summary.json
  summarize.py
  SHA256SUMS
```

Manifesto di **150 artefatti**; SHA-256 di `SHA256SUMS`:

`370a7c1b84ee4c0507a88af76b0b91aec146bee0addd4413eae814b866047908`

Esecuzione riproducibile sul medesimo host/runtime, usando una **directory campioni nuova**:

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/sirio/rust
ARTIFACTS=/Users/enzopiopalmisano/Desktop/Progetti/sirio-perf-artifacts/2026-09-09-first-wave-ac-092527
SIRIO_PERF_SAMPLE_DIR=/absolute/external/new-samples \
SIRIO_PERF_REVISION=5a030ba18423ea9f8895d0dfa9ea8e90034a9b88+r1.917959c2be6b \
SIRIO_PERF_HOST=darwin-arm64-m4-24g \
"$ARTIFACTS/sirio_ui-baseline" \
  changes::perf_baseline::changes_hot_baseline \
  --ignored --exact --nocapture --test-threads=1
```

`python3 "$ARTIFACTS/summarize.py"` rigenera summary e manifesto della raccolta archiviata; verifica 140 serie e 1.000 sample distinti per serie. Nessun CSV o log originale viene riscritto.

## Stato del lavoro

Ripetizione richiesta eseguita e dati preservati; **nessuna ottimizzazione, modifica ai sorgenti, compilazione, CI, commit, push o delega** in questo passo. Sono stati aggiornati soltanto i report e aggiunti artefatti esterni.

Rimane da isolare meglio la variabilità dei workload grandi prima di usarne il p95 per accettare un miglioramento sottile. Non sono stati avviati automaticamente profiling di processo o ulteriori cicli di benchmark. Restano invariati i residui del protocollo: cold/apply, navigazione/resize/mode switch, drag reale, RSS, app nativa e piattaforme target.
