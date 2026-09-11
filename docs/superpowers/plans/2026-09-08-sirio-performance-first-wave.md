# Sirio Performance First Wave — Implementation Plan

> **For agentic workers:** usare la skill `executing-plans` per procedere task-by-task. La modalità `subagent-driven-development` è un'alternativa solo dopo permesso esplicito dell'utente; nessun subagent automatico. Le checkbox indicano lavoro futuro, non lavoro già eseguito.

**Goal:** eliminare preparazione Changes ridondante, discovery Git sincrona al focus e cattura completa per activity detection, senza cambiare comportamento osservabile.

**Architecture:** tre mini-piani indipendenti, ciascuno con baseline, test di caratterizzazione, modifica, misura e decisione. Conservare GPUI/bezel, virtualizzatori e terminal owner esistenti. Nessuna modifica a scheduler, renderer, schema DB o protocolli del control socket.

**Tech Stack:** Rust, bezel 0.1.4, bezel-gpui 0.3.8, libghostty-vt 0.2.1, canali `std::sync::mpsc`, test Rust/GPUI.

**Spec proposta:** [protocollo di baseline](2026-09-08-sirio-performance-baseline.md), con invarianti e criteri di accettazione. [Audit sorgente](../../../plans/004-zed-sirio-performance-audit.md). Nessuna specifica di implementazione era già approvata: questo documento richiede approvazione prima dell'esecuzione.

## Global Constraints

- Base verificata: `5a030ba18423ea9f8895d0dfa9ea8e90034a9b88`. Riconfermare simboli e stato Git prima di editare; non usare numeri di riga come identità stabili.
- Zig **esattamente 0.15.2** per qualsiasi comando che compili `sirio_terminal`, incluso `cargo test -p sirio`.
- Prima di build/test, diagnostica LSP mirata; prima di dichiarare finito, `lens_diagnostics mode=all`. Tool non disponibile o timeout non equivale a diagnostica pulita.
- Iterazione con `cargo test/build -p <crate>`; **mai** `Scripts/ci.sh`, `Scripts/ci-linux.sh` o gate workspace senza richiesta esplicita.
- Test first. I test nuovi di contratto devono fallire per il comportamento attuale, non per un errore di setup; se un helper non esiste ancora, distinguere errore di compilazione RED da fallimento comportamentale.
- Niente aggiornamento dipendenze, primitive UI custom, porting testuale GPL da Zed, modifiche a `rust/vendor/` o refactor generale dei grandi `main.rs`/`lib.rs`.
- Non toccare ownership activity A/B/C/D, 200 ms settle, 16 ms repaint, polling 4/1 ms, generation dei restart o vita dei PTY nascosti.
- Nessun commit/push/PR/issue automatico. I messaggi Conventional Commit sotto sono suggerimenti per un successivo commit autorizzato.
- Un solo writer per worktree. S1-B e S2 toccano entrambi `sirio_terminal/src/lib.rs`: non implementarli in parallelo nella stessa checkout.
- Snippet e nomi di API sotto sono **proposti**, non codice già presente o compilato. Ogni mini-piano definisce le proprie nuove interfacce prima dei test che le usano.

## Ordine, dipendenze e checkpoint

```text
B0: ambiente + baseline esistente
 ├─ B1-S1: probe Changes → S1-A: proiezione → S1-B: payload → confronto S1
 ├─ B1-S7: loader controllato → S7-A: separare apply → S7-B: coda → confronto S7
 └─ B1-S2: contatori cattura → S2-A: estrattore → S2-B: owner/pump → confronto S2
                                                        ↓
                                         verifica della composizione
```

Ordine operativo consigliato: **S1 → S7 → S2**. Non è una graduatoria di benefici misurati. Ogni mini-piano può essere approvato, rifiutato o rimandato senza gli altri; la baseline strumentata è il solo prerequisito condiviso. Fermarsi per revisione dei risultati alla fine di ciascun mini-piano.

## B0/B1 — baseline senza ottimizzazione

**File esistenti:** `rust/crates/sirio_ui/src/changes.rs`, `rust/crates/sirio_terminal/src/lib.rs`, `rust/crates/sirio_terminal/tests/still_frame_cost.rs`, `rust/crates/sirio/src/main.rs`.

**Eventuali file da modificare per probe live:** `rust/crates/{sirio,sirio_ui,sirio_terminal}/Cargo.toml`; aggiungere `perf-probes` solo se i contatori dell'app reale sono necessari. Il protocollo specifica feature off/on, buffering e privacy; niente framework di telemetry.

- [ ] Verificare checkout, toolchain e host; registrare lo stato, senza modificare il ramo principale per preparare misure.
- [ ] Eseguire i diagnostici esistenti elencati nel protocollo; conservare esito completo, anche se falliscono. Non abbassare soglie per ottenere una baseline verde.
- [ ] Aggiungere fixture e contatori locali per **il solo mini-piano che si sta iniziando**. Misurare prima di introdurre la relativa ottimizzazione.
- [ ] Aggiungere test del conteggio: due invocazioni esplicite del builder incrementano di due; reset di una fixture non azzera un'altra. Contare dentro il builder, non nel chiamante, per non nascondere invocazioni alternative.
- [ ] Per ogni diagnostico nuovo usare 10 warm-up e 1.000 campioni; non cambiare silenziosamente il test storico basato sul minimo. Tenere un test deterministico dei contatori distinto dal diagnostico wall-clock ignored.

Funzione proposta per i diagnostici, locale al rispettivo modulo `tests`; nessun nuovo crate condiviso:

```rust
fn percentile_ns(samples: &[u128], percentile: usize) -> u128 {
    assert!(!samples.is_empty());
    assert!((1..=100).contains(&percentile));
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let index = (sorted.len() * percentile).div_ceil(100) - 1;
    sorted[index]
}

#[test]
fn percentile_uses_nearest_rank_without_dropping_slow_samples() {
    assert_eq!(percentile_ns(&[40, 10, 30, 20], 50), 20);
    assert_eq!(percentile_ns(&[40, 10, 30, 20], 95), 40);
}
```

- [ ] Salvare R1 e campioni nel report descritto dal protocollo. **Checkpoint B:** senza misure attribuibili e riproducibili non presentare una patch come miglioramento prestazionale.

## Mini-piano S1 — proiezione Changes conservata

### S1-A — cache dei dati della lista, non degli elementi GPUI

**File:** modificare e testare `rust/crates/sirio_ui/src/changes.rs`.

**Punti letti:** `section_rows` 1166–1209; `sync_list_rows` 1316–1353; `render_body` 2296–2421; `selectable_rows` 983–992; `apply_snapshot` 710–727; mutatori espansione/focus 1039–1158. La modalità è un global: non basta invalidare dal bottone locale `set_view_mode`.

**Consuma:** `GitSnapshot`, `DiffViewMode`, `ChangeRow`, `ListRow`, `ListState` esistenti.

**Produce:** strutture/metodi privati in `changes.rs`:

```rust
struct ChangesProjection {
    mode: DiffViewMode,
    rows: Rc<Vec<ListRow>>,
    selectable: Rc<Vec<(ChangeSection, PathBuf)>>,
}
// Nuovo campo in ChangesTab: projection: Option<ChangesProjection>
// Nuovi metodi:
// fn invalidate_projection(&mut self)
// fn projected_rows(&mut self, mode: DiffViewMode) -> Rc<Vec<ListRow>>
// fn reveal_selected_in(&mut self, rows: &[ListRow])
// Firma aggiornata:
// fn selectable_rows(&mut self, mode: DiffViewMode)
//     -> Rc<Vec<(ChangeSection, PathBuf)>>
```

La revisione dello snapshot è rappresentata dall'invalidazione obbligatoria su **ogni** `apply_snapshot`, non da un hash del testo. Conservare una sola proiezione per superficie; mode differente = cache miss. `list_fingerprint` resta un'identità geometrica per splice, mai una prova di contenuto invariato.

- [ ] Scrivere il test RED di riuso usando `synthetic_big_diff_tab`; introdurre solo lo scheletro necessario a compilare, senza cache, per osservare il fallimento comportamentale.

```rust
#[test]
fn unchanged_changes_projection_reuses_rows() {
    let mut tab = synthetic_big_diff_tab(15_000);
    let first = tab.projected_rows(DiffViewMode::Unified);
    for _ in 0..100 {
        let next = tab.projected_rows(DiffViewMode::Unified);
        assert!(Rc::ptr_eq(&first, &next));
    }
}
```

- [ ] Implementare hit/miss: su hit restituire `Rc::clone`; su miss costruire sezioni, appiattire/hashare con la logica esistente, aggiornare `ListState` se serve e conservare righe + soli file navigabili. Costruire anche la proiezione vuota, così «No changes» non ripete lavoro.
- [ ] Separare da `sync_list_rows` la logica `reveal_selected`; eseguirla nel render sia su hit sia su miss. La selezione è stato di presentazione, non deve invalidare l'intera proiezione.
- [ ] Usare la stessa proiezione in `on_change_key` tramite `selectable_rows`: non lasciare `section_rows` sul percorso tastiera. Restano leciti confronto/ricerca fra file navigabili; non deve riespandere i diff.
- [ ] Invalidare in `apply_snapshot`, `toggle_change`, `apply_focus`, `toggle_band`, `expand_all`, `collapse_all`, `toggle_section`. Inizializzare il nuovo campo in tutti i costruttori e nelle fixture letterali. Testare il focus differito applicato dal primo snapshot.
- [ ] Nel render leggere sempre mode corrente, selezione, tema e permessi; non conservare elementi GPUI/closure della view nella cache dati. Resize continua a fare layout, non ricostruisce la proiezione.

**Test di freshness obbligatorio:** costruire 300 righe, primo `projected_rows`; costruire un secondo `GitSnapshot` dalla fixture con lo stesso path/coordinate e cambiare `hunks[0].lines[0].content` in `"UPDATED_CONTENT"`; applicarlo; verificare `!Rc::ptr_eq`, testo nuovo nella `ChangeRow::Line` e `list_fingerprint` invariato. Questo separa aggiornamento dei dati da splice geometrico.

**Matrice aggiuntiva:** unified→split modificato da un'altra superficie; error/loading/empty→loaded; contesti espansi; sezione collassata; staged+unstaged dello stesso path; up/down con reveal su cache hit; selezione e scroll dopo aggiornamento di pari identità.

```bash
cd rust
cargo test -p sirio_ui --lib unchanged_changes_projection_reuses_rows
cargo test -p sirio_ui --lib changes::tests
```

**Atteso:** test nuovo RED senza cache, GREEN con cache; regressioni Changes verdi e contatori B-S1 senza ricostruzione nei redraw puri. I nomi dei test nuovi vanno controllati nell'output: zero test non vale come verifica.

**Commit proposto, solo se autorizzato:** `perf: retain changes list projections between invalidations`.

### S1-B — evitare copie integrali del diff nelle righe visibili

**Perché separato:** S1-A da solo lascia `ChangeRow::clone` copiare il `String` in `DiffPayload=(PathBuf, String)` per ogni file header disegnato. Non dichiarare risolto questo costo soltanto perché la proiezione è conservata.

**File:** `rust/crates/sirio_ui/src/changes.rs`, `rust/crates/sirio_terminal/src/lib.rs`; verificare il test integrato in `rust/crates/sirio/src/main.rs::drawn_terminal_diff_drop_opens_a_changes_tab_focused_on_that_file`.

**Interfaccia proposta:** mantenere `diff_payload(&FileDiff) -> Option<(PathBuf, String)>`; cache per snapshot `HashMap<PathBuf, Rc<DiffPayload>>` in Changes. `ChangeRow::File.drag_payload` e argomento di `render_change_file` diventano `Option<Rc<DiffPayload>>`. La cache viene riempita alla prima necessità per un file e svuotata su ogni snapshot; non su espansione/mode/selection. Non trattenere una cronologia di payload.

**Compatibilità DnD:** `on_drag` di Bezel GPUI conserva il valore tipizzato, non offre in quel metodo un factory lazy del payload. L'API installata è stata letta in `bezel-gpui-0.3.8+zed.82aeef/src/elements/div.rs:599–618,1562–1575`. Il consumer deve riconoscere lo stesso tipo usato dal producer.

- [ ] Aggiungere un test con 300 file e due sezioni per lo stesso path: la serializzazione avviene al massimo una volta per path/snapshot, nessuna nei successivi redraw/mode switch. I riferimenti condividono l'allocazione (`Rc::ptr_eq`).
- [ ] Conservare il payload condiviso fino alla fine del drag iniziato: un refresh a metà gesto non ne cambia i byte. Il gesto successivo usa lo snapshot nuovo.
- [ ] Aggiungere al terminale il drop del tipo condiviso e **mantenere** quello esistente `(PathBuf, String)`; `receive_diff_drop` e `last_dropped_diff` non cambiano contratto. Copiare il testo solo alla consegna del drop, non al render.

Forma del nuovo listener, da collegare alla stessa entity terminale già usata dal listener esistente:

```rust
.on_drop::<std::rc::Rc<(PathBuf, String)>>(move |payload, _, cx| {
    terminal.update(cx, |view, cx| {
        view.receive_diff_drop(payload.as_ref().clone(), cx);
    });
})
```

- [ ] Aggiornare la fixture UI `DiffDropTargetFixture` al payload condiviso; mantenere la fixture terminale del payload legacy e aggiungerne una per quello condiviso. Testare drop reale, non soltanto chiamata diretta al metodo ricevente.
- [ ] Conservare path con spazi, testo esatto, binary non draggable, preview esistente e promozione al diff tab. Nessuna dipendenza `sirio_ui -> sirio_terminal`.
- [ ] Rimisurare B-S1 con header visibile e nascosto, cold snapshot e cache calda. Registrare memoria trattenuta dalla cache e nessuna crescita fra snapshot successivi dopo il rilascio dei vecchi frame/drag.

```bash
cd rust
cargo test -p sirio_ui --lib changes::tests
cargo test -p sirio_terminal --lib diff
cargo test -p sirio --bin sirio drawn_terminal_diff_drop_opens_a_changes_tab_focused_on_that_file
```

**Checkpoint S1:** correttezza + zero ricostruzioni/serializzazioni/copie integrali su redraw puro + confronto numerico col protocollo. Se il beneficio è dentro rumore o la memoria peggiora fuori rumore, non promuovere automaticamente.

**Commit proposto:** `perf: share changes drag payloads within each snapshot`.

## Mini-piano S7 — discovery coalesciuta fuori dal render

### S7-A — separare discovery e applicazione, preservando la semantica

**File:** `rust/crates/sirio/src/session.rs`; test nello stesso file. Nessuna modifica iniziale ai tempi di esecuzione del chiamante.

**Punti letti:** `ProjectCatalog::refresh_project_with_mounted_worktrees` 668–713, `catalog_project` da 762; `SirioWorkspace::refresh_catalog_project` 5373–5415.

**Nuova interfaccia interna al crate app:**

```rust
// Metodo di ProjectCatalog, in session.rs:
// pub(crate) fn apply_discovered_project(
//     &mut self,
//     id: &str,
//     discovered: DiscoveredProject,
//     mounted_paths: &[PathBuf],
// ) -> Result<(), String>
```

- [ ] Caratterizzare il merge: id stabile; primary applicativo preservato; worktree scomparso mantenuto e marcato missing se attualmente montato; rimosso se non montato; errore su id ignoto senza mutare il catalogo; plain folder/bare repo come oggi.
- [ ] Estrarre il corpo di merge dopo `discover_project`, facendogli leggere il progetto **corrente** da `self`, non una copia precedente al job. Il wrapper sincrono continua a chiamare discovery e poi il metodo estratto: i chiamanti mutanti esistenti mantengono ordine e risultato booleano.
- [ ] Non mettere Git nel nuovo metodo apply. Non chiamarlo «puro» o «zero I/O»: `catalog_project`/`canonical_path` possono interrogare il filesystem. Misurare apply separatamente.
- [ ] Testare con un `DiscoveredProject` già preparato, quindi cambiare primary/mounted prima di apply: devono prevalere i dati live pertinenti, non quelli catturati all'inizio del job.

```bash
cd rust
cargo test -p sirio --bin sirio session::tests
```

**Atteso:** stesso comportamento di refresh sincrono; nessuna ottimizzazione rivendicata in questo task di separazione.

**Commit proposto:** `refactor: separate project discovery from catalog application`.

### S7-B — coda revisionata e collegamento al focus

**File da creare:** `rust/crates/sirio/src/catalog_refresh.rs`, contenente solo stato puro della coda/token e relativi test; non esegue Git e non dipende da GPUI.

**File da modificare:** `rust/crates/sirio/src/main.rs`, per task GPUI, loader e apply; `session.rs` usa l'interfaccia S7-A.

**Interfacce proposte in `catalog_refresh.rs`:**

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RefreshRequest {
    pub serial: u64,
    pub project_id: String,
    pub root_path: PathBuf,
    pub catalog_epoch: u64,
}
// ProjectRefreshQueue: Default
// request(&mut self, project_id: String, root_path: PathBuf,
//         catalog_epoch: u64) -> RefreshRequest
// start_next(&mut self) -> Option<RefreshRequest>
// is_current(&self, request: &RefreshRequest, catalog_epoch: u64) -> bool
// finish(&mut self, request: &RefreshRequest)
// remove_project(&mut self, project_id: &str)
```

Stato: serial monotono, richiesta più recente per progetto, coda FIFO per progetto senza duplicati, un solo token in-flight. `request` sostituisce il pending dello stesso progetto senza far saltare la fila agli altri. `finish` libera solo l'in-flight corrispondente, non cancella un pending più recente. `remove_project` elimina pending e latest; un job già partito può terminare, ma non applicarsi. Pulire le entry latest quando non hanno più lavoro, evitando una mappa crescente di progetti rimossi.

Test minimo della coalescenza:

```rust
#[test]
fn focus_refresh_coalesces_and_rejects_an_older_reply() {
    let mut queue = ProjectRefreshQueue::default();
    let first = queue.request("a".into(), PathBuf::from("/a"), 0);
    assert_eq!(queue.start_next(), Some(first.clone()));
    queue.request("a".into(), PathBuf::from("/a"), 0);
    let latest = queue.request("a".into(), PathBuf::from("/a"), 0);
    assert!(queue.start_next().is_none());
    assert!(!queue.is_current(&first, 0));
    queue.finish(&first);
    assert_eq!(queue.start_next(), Some(latest.clone()));
    assert!(queue.is_current(&latest, 0));
    assert!(!queue.is_current(&latest, 1));
}
```

**Loader del task app, iniettabile nei test:**

```rust
type FocusDiscovery = std::sync::Arc<
    dyn Fn(&Path) -> Result<Option<DiscoveredProject>, String> + Send + Sync
>;
```

Il loader di produzione fa il gate `is_git_repository` **in background**; `None` mantiene il no-op attuale per root non Git, `Some` contiene `discover_project`. Non migrare automaticamente plain folder a repo nel solo focus handler.

- [ ] Testare la coda RED→GREEN: raffica A, alternanza A/B, epoch invalidato, rimozione/reinserimento stesso id, fine job vecchio che non cancella pending nuovo, errore che libera la coda.
- [ ] Aggiungere a `SirioWorkspace` coda, `Option<Task<()>>`, loader e `catalog_epoch: u64` runtime. Non inserire l'epoch nell'uguaglianza o nella persistenza di `ProjectCatalog`: causerebbe falsi delta e salvataggi a ogni refresh.
- [ ] Aggiungere `invalidate_catalog_refreshes(&mut self)` che avanza l'epoch ai confini delle mutazioni del catalogo. Verificare **tutti** i chiamanti runtime di add/remove/replace/reorder/set-primary/refresh sincrono; punti noti: 6098–6105, 6161, 6217, 6377, 6421–6442, 6514, 7570, 7704. Invalidare anche remove+add con identico id/root, che un semplice confronto finale dei valori non rileva.
- [ ] Sostituire solo la chiamata nel ramo focus di `render` (14729–14736) con una richiesta leggera: individuare id/root dal catalogo, accodare e avviare il worker con `cx.background_spawn`. Niente chiamata sincrona al vecchio `refresh_project_for_path` in quel ramo. I chiamanti di selezione/create/remove restano sincroni in questa tranche.
- [ ] Collegare l'epoch alla lettura branch: se `refresh_worktree_branches` muta il catalogo, invalidare la discovery già partita oppure accodarla dopo quel refresh. Un pending con epoch superato viene riformulato su id/root correnti prima di partire, non riavviato per sempre con lo stesso token obsoleto.
- [ ] Su completamento, verificare token latest, epoch, esistenza del progetto e root ancora corrispondente. Rifiutare risultati vecchi; se il progetto è ancora vivo e richiede freschezza, accodare una sola richiesta aggiornata. Se è stato rimosso, nessun retry.
- [ ] Applicare con `apply_discovered_project` e `mounted_worktree_paths(None)` calcolati **al completamento**. Non rimpiazzare `self.project_catalog` con un clone mutato in background.
- [ ] Riutilizzare le azioni post-apply di `refresh_catalog_project`: persistere solo delta reale, `sync_control_state`, `refresh_sidebar`, selezione letta al completamento e `cx.notify`. Anche senza delta Git mantenere sidebar/control coerenti come fa il metodo attuale.
- [ ] Errori del token corrente: tenere catalogo precedente, mostrare notice, liberare task e servire il pending; niente retry stretto automatico. Errori di token obsoleto: ignorare, senza sovrascrivere notice del progetto ora selezionato. View distrutta: nessuna update o resurrezione.

**Test GPUI indispensabili:** loader fermo su barriera mentre si disegna e si processa input; rilascio controllato e apply; A→B durante job A; create/remove sincrono che invalida focus job; remove+add stesso id; mounted cambiati durante job; error/retry su nuova richiesta; title/PTY/tab rows e chevron preservati.

I test attuali `regaining_window_focus_keeps_a_worktrees_tab_rows_and_chevron` e `regaining_window_focus_keeps_an_unselected_worktrees_parked_rows` chiamano il metodo sincrono direttamente: conservarli, ma **non considerarli copertura del nuovo wiring**. Aggiungere un test con reale transizione `window.is_window_active` e draw. La barriera blocca solo il worker; il foreground non deve fare `recv()` o attendere il worker nel callback.

```bash
cd rust
cargo test -p sirio --bin sirio catalog_refresh
cargo test -p sirio --bin sirio regaining_window_focus
cargo test -p sirio --bin sirio runtime_worktree_refresh_keeps_live_tabs_with_their_original_worktree
cargo test -p sirio --bin sirio missing_worktree_switch_keeps_live_terminals_and_database_rows_intact
cargo test -p sirio --bin sirio
```

**Checkpoint S7:** job limitati, nessuna discovery Git sul render-focus, apply obsoleto impossibile nei test, selezione/sidebar/control coerenti e misure B-S7. Il primo frame può ancora pagare HEAD/normalizzazione path: segnalare il residuo invece di ampliare il task a tutta la persistenza.

**Commit proposto:** `perf: coalesce focus discovery outside the ui render path`.

## Mini-piano S2 — cattura recente lato owner

### S2-A — estrazione equivalente che visita solo la coda utile

**File:** `rust/crates/sirio_terminal/src/lib.rs`; test headless nello stesso modulo.

**Punti letti:** `capture_scrollback_text` 2197–2241; `grid_ref_cluster`; `recent_content_window` 2622–2634; helper `headless_term` 6334–6350 e `advance_headless` 6352–6354.

**Nuove interfacce private:**

```rust
// fn capture_row_text(terminal: &mut Terminal<'static, 'static>,
//                     row: usize, columns: usize) -> String
// fn capture_recent_content_text(terminal: &mut Terminal<'static, 'static>) -> String
```

- [ ] Estrarre la lettura di una riga dalla cattura completa, senza cambiare gestione spacer, grapheme, errori grid-ref, trimming degli spazi ASCII e newline. Il percorso completo deve conservare esattamente i suoi byte.
- [ ] Aggiungere test di equivalenza; la prima versione della nuova funzione può delegare alla cattura completa per passare i test funzionali, ma deve fallire il test di lavoro limitato.

```rust
#[test]
fn recent_capture_matches_full_capture_for_long_utf8_history() {
    let mut term = headless_term(80, 24);
    for i in 0..2_000 {
        advance_headless(&mut term, format!("{i}: é {FAMILY_EMOJI}\r\n").as_bytes());
    }
    let expected = recent_content_window(&capture_scrollback_text(&mut term));
    assert_eq!(capture_recent_content_text(&mut term), expected);
}
```

- [ ] Usare l'oracle anche per history vuota, sole righe vuote, lunghe code vuote, righe vuote interne, 39/40/41 righe, confini byte, CJK, flag, combining, wide spacer, wrapping, resize e alternate screen.
- [ ] Implementare visita inversa da `total_rows - 1`: saltare righe vuote solo finché non si trova l'ultima non vuota; poi conservare anche le vuote interne. Fermarsi dopo 40 righe conservate oppure quando i byte delle righe + separatori raggiungono 10 KiB, oppure a inizio griglia. Invertire le sole righe raccolte e applicare ancora `recent_content_window` per il taglio UTF-8/linee identico all'oracle.
- [ ] La memoria intermedia può superare 10 KiB della dimensione dell'ultima riga letta; non attribuirle un limite assoluto di 10 KiB. Non leggere metà grapheme per risparmiare memoria. Misurare riga molto larga e lunghe code vuote.
- [ ] Aggiungere contatore per fixture delle celle lette nel row helper; confrontare 1.000 e 8.000 righe di history con identica coda utile. Il nuovo percorso visita lo stesso numero di celle in questo caso, quello completo no. Svuotare/reset contatori prima di ogni cattura, fuori dal timer.

```bash
cd rust
cargo test -p sirio_terminal --lib recent_capture
cargo test -p sirio_terminal --lib capture_scrollback_normalises_known_bytes_with_clusters_intact
cargo test -p sirio_terminal --lib activity_content
```

**Atteso:** equivalenza esatta e test deterministico del lavoro limitato; non una nuova euristica del matcher.

**Commit proposto:** `perf: extract recent terminal content without scanning full history`.

### S2-B — comando dedicato e richiesta pending indipendente

**File:** `rust/crates/sirio_terminal/src/lib.rs`; nessuna modifica necessaria a `sirio_activity` o al control socket.

**Nuove interfacce private:**

```rust
// Variante TerminalCommand:
// RecentText(std::sync::mpsc::Sender<String>)
// Tipo RecentContentCapture: Clone, con sender e pending dedicato:
// new(commands: std::sync::mpsc::Sender<TerminalCommand>) -> Self
// try_capture(&self) -> Option<String>
// TerminalHandle:
// fn try_capture_recent_content(&self) -> Option<String>
```

Il tipo recente replica il contratto non bloccante di `ScrollbackCapture`, non ne cambia il significato. Le clone della sorgente recente condividono il proprio pending; **non** condividono quello Text completo. `ScrollbackCapture::capture`, `scrollback_source`, persistenza/replay e catture esplicite rimangono complete.

Test minimo del pending indipendente dalla velocità del thread:

```rust
#[test]
fn recent_capture_poll_enqueues_only_one_request() {
    let (commands, command_rx) = std::sync::mpsc::channel();
    let source = RecentContentCapture::new(commands);
    assert!(source.try_capture().is_none());
    let TerminalCommand::RecentText(reply) = command_rx.try_recv().unwrap() else {
        panic!("expected the recent-text command, not full Text");
    };
    assert!(source.try_capture().is_none());
    assert!(matches!(command_rx.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)));
    reply.send("LATEST".to_string()).unwrap();
    assert_eq!(source.try_capture(), Some("LATEST".to_string()));
}
```

- [ ] Aggiungere test RED del comando/dispatch owner e del pending; testare sender/receiver disconnessi con `Some(String::new())` come risposta terminale vuota, non retry infinito.
- [ ] Collegare `RecentText` all'estrattore S2-A nello stesso match owner che gestisce `Text` (attualmente intorno a 1471). Non modificare drain PTY, priorità, fairness o loop di polling.
- [ ] Nel settle di `pump_terminal_events` usare `try_capture_recent_content`; emettere la `String` già limitata. Mantenere `capture_retry_pending`, `unsettled_output`, generation guard e `cx.notify` finale esattamente nel loro ruolo.
- [ ] Aggiornare il commento della cattura completa: non viene più eseguita per activity settle. Conservare i test del percorso completo; aggiungere equivalenti per quello recente, non sostituirli.
- [ ] Testare richieste recent/full simultanee con risposte diverse: nessun consumer riceve il tipo di contenuto dell'altro. Ripetere con capture completa sincrona mentre la recente è pending.
- [ ] Test GPUI: risposta owner dopo l'ultimo evento PTY produce ancora OutputSettled e notifica finale; risposta da generation vecchia non altera la view riavviata; un agent nascosto mantiene la rilevazione; title/exit restano tempestivi.

```bash
cd rust
cargo test -p sirio_terminal --lib recent_capture
cargo test -p sirio_terminal --lib scrollback_capture
cargo test -p sirio_terminal --lib scrollback_source
cargo test -p sirio_terminal --lib real_pty_emits_osc_title_and_settled_output
cargo test -p sirio_terminal
cargo test -p sirio_activity
```

**Checkpoint S2:** B-S2 con output identico, richieste complete assenti dal settle, lavoro ridotto su history non vuota e nessuna perdita dell'ultimo evento/frame. Dichiarare il costo residuo della coda vuota e del row access Ghostty.

**Commit proposto:** `perf: request bounded activity content from the terminal owner`.

## Chiusura e handoff

- [ ] Per ciascun mini-piano eseguito, riportare file cambiati, comandi effettivi, numero test, errori, revisioni e risultati prima/dopo. I comandi di questo documento non sono prova di esecuzione.
- [ ] Confrontare ogni candidato con R1 prima di confrontare il risultato cumulativo; annotare interazioni S1-B/S2 nello stesso terminal crate.
- [ ] Rivedere inline correttezza, API, invalidazioni, ownership, test e complessità. Proporre reviewer separato solo chiedendo permesso.
- [ ] Verificare diagnostics e diff; confermare che non siano entrate modifiche a scheduler/renderer, persistenza completa o dipendenze.
- [ ] Compilare il report anche per tentativi scartati. Non mantenere complessità senza beneficio misurabile soltanto perché è stata implementata.
- [ ] Chiedere quale mini-piano eseguire e con quale modalità: inline oppure subagent autorizzato. Il primo passo tecnico dell'esecuzione resta B0/B1, non applicare tutte e tre le ottimizzazioni insieme.
