# Note QA (osservazioni grezze, da confermare con 2 riproduzioni)

- 19:29 Rimosso progetto "/" (unico): la tab strip mantiene la tab "Terminal" orfana e la status bar "main · /" → da riprodurre (B8)
- 19:27 Tema Light: testo status bar (usage, path) quasi invisibile → da riverificare con zoom (K1/L3)
- 19:32 Popup macOS "Sirio.app vorrebbe accedere alla libreria di foto" comparso mentre il menu "+" era aperto (hover su New Chat). Negato. Trigger sconosciuto → da riprodurre
- 19:33/19:34 "+" della titlebar: click senza effetto visibile (2/2) → da riverificare con hover (A6)
- Socket: project.add di qualunque path fallisce (added=false) se esiste un progetto con root "/" (containment check). Edge case
- Status bar badge "Claude …"/"Codex …" troncati con ellissi subito dopo l'aggiunta del progetto, poi tornati completi (L3, intermittente)
- 19:47 Resize a 820x560: righe worktree della sidebar vanno a capo e si sovrappongono (1/2) → riprodurre
- 19:48 Scroll wheel (CGEvent, unit line, 75 tick) sul terminale non muove la vista → verificare eventi su History
- 19:50 Aperte #338 (freeze) e #339 (frammento prompt)

- 19:58 Scroll wheel sul terminale: 3/3 senza movimento (line ticks ×75, pixel delta, terminale 2 ×40); Shift+PageUp stampa `;2~` nella shell. Issue #340 aperta, worktree fix-340 (w1H:p1, codex fix340). fix339: codex si era auto-aggiornato (0.151→0.152.1) ed era uscito; riavviato e ricontrattato.

- 20:47 A3/A4 ok. Reflow al resize: p10k ridisegna il prompt e restano righe prompt duplicate (comportamento zsh/p10k su SIGWINCH, non attribuibile a Sirio con certezza). Tab 2 mostra un glifo "?" al posto di ○ (da capire). fix338: regressione su restore_replays_persisted_terminal_scrollback e render_never_captures_scrollback (passano su main) → rimandato. fix339: causa = shell_breadcrumb() overlay app-level, fix = rimozione overlay (81 righe); #230 aveva corretto il contenuto, non messo in dubbio l'esistenza. #341 aperta (Codex API key → "logged out"), worktree fix-341 (w1J:p1).

- 21:05 Merge locali su main: dee3bd31 (#339, rimozione overlay shell_breadcrumb) e 9e21dc7d (#338, try_capture + LIBGHOSTTY_VT_SYS_OPTIMIZE=ReleaseFast in .cargo/config.toml). Rebuild in corso. index.lock orfano di 54 min rimosso (0 byte, nessun processo). #342 aperta (status bar Light illeggibile, 2 riproduzioni), worktree fix-342 (w1K:p1). Terminal font size 14 salvato in sqlite ma nessun effetto visibile né sul pane esistente né su un nuovo pane: da verificare dopo il relaunch. Tab pane-2 mostra "?" giallo = ActivityStatus::NeedsInput (main.rs:3506) su una zsh idle senza agente; causa ignota (codex notify → SkyComputerUse, non sirioctl). 1×.

- 21:55 Relaunch dopo rebuild: sessione ripristinata (6 Terminal, progetto sirio/main); pannello destro riaperto su Files invece di History. Cascata di prompt TCC all'avvio: volume di rete (negato), Scrivania (consentito), Download (negato) — verosimilmente dalla scansione del progetto "/" che era stato rimosso via UI e che dopo il relaunch è ricomparso (riga project p-af63a24c… ancora nel DB). Durante il prompt TCC Sirio non risponde agli AppleEvent (timeout -1712) e la finestra non è raggiungibile via AX: i click/testo di cliclick sono finiti in Ghostty. Merge locali: ff3dc787 (#340), b660056d (#341), f0eca883 (#342).

- 22:05 #339 chiuso (verified), worktree w1G rimosso, branch fix/339 cancellato. #338: regressione post-merge (ultimo frame non disegnato dopo il burst; scrollback già completo) → revert 8f68a89f, commento su #338, codex fix338 rilanciato con contesto. Test suite sirio su main con i 5 merge: 319 ok / 5 rossi, tutti nel bacino baseline (restoring_a_catalog…, real_pty_layer_a_debounce…, drawn_changes_open_diff…, opening_changes_with_a_path…). Remove Project "/" → persistito nel DB (0 worktree residui). Rebuild in corso con 339+340+341+342.

## 22:48–22:52 — build f0eca883, seconda ondata TCC
- Dopo la cascata iniziale (Documenti negato, Musica negato, Scrivania consentito) sono comparsi ALTRI due prompt TCC in momenti successivi: "volume di rete" (~22:48, finestra ferma da minuti) e "cartella Download" (22:52, subito dopo aver ricliccato il worktree main in sidebar). Quindi #343 non è solo startup: la scansione scatta anche su cambio worktree/selezione. Aggiungere commento a #343.
- Con il prompt "volume di rete" aperto, la catena di click (gear/Light/Dark/back) è finita sulla sidebar: selezionato il worktree fix/next (path ~/Desktop/Progetti/worktrees/tiller-rust-gpui/fix-next) → "No Terminals". Retest #342/#340 da rifare. Nessun danno.
- #341 retest OK sul build f0eca883: status bar mostra "Codex API key" (shot 109-112).
- Screenshot 114: dopo la riselezione di main il viewport del terminale attivo (pane-7) è vuoto (solo cursore in alto a sinistra) mentre il prompt TCC è aperto; verificare dopo il dismiss (shot 115).

## 22:53–22:58 — TCC ripetuti, PTY, sidebar
- Prompt TCC "Apple Music" ricomparso (22:53) benché negato ~22:21 nello stesso run; "Documenti" ricomparso (22:55) selezionando fix/338. Le decisioni "Non consentire" non vengono ricordate (bundle firmato ad hoc → identità TCC instabile?) e ogni selezione di worktree rifà l'accesso. Commentato su #343.
- Processi figli di Sirio (PID 71867): solo 2 zsh (72260 dalle 22:21, 3920 dalle 22:52:28) a fronte di 7 righe "Terminal" in sidebar e 7 pane in `sirioctl panel list` (pane-1..6 scrollbackBytes 0, "running"). Le righe restaurate dalla sessione non hanno PTY finché non vengono mostrate.
- 22:52:28: pane-7 ha ottenuto un nuovo zsh (scrollback azzerato a 135 byte, prompt fresco) dopo fix/next → main. Repro controllata con MARKER-A: fix/338→main e fix/next→main NON perdono nulla (stesso PID, scrollback 263 byte). Ipotesi: la perdita avviene solo al primo cambio dopo il restore (worktree non ancora `mounted`). Da ritentare dopo il prossimo relaunch (marker → cambio worktree → ritorno).
- 22:55:26: Sirio ha spawnato un processo `claude` figlio diretto (PPID 71867), durato <1 s, senza tty. Probabile fetch usage (sirio_usage). Non è un bug osservabile.
- Sidebar: con main selezionato e 7 righe Terminal la lista arriva al bordo inferiore della finestra e la STATUS BAR NON È VISIBILE (shot 107, 116, 119, 122); con lista corta (shot 108/109, fix/next senza terminali) la status bar c'è (gear a 63,779). Verifica con resize 1200×1000 → shot 123.
- Selezionare fix/338 o fix/next mostra le 7 righe Terminal sotto il worktree selezionato mentre il centro continua a mostrare il terminale di main (cwd sirio): le righe "Terminal" seguono la selezione, non il worktree proprietario (shot 118, 121). Alle 22:48 invece fix/next mostrava "No Terminals" (shot 108). Comportamento incoerente tra due selezioni identiche.

## 23:02–23:10 — retest, #344, font size
- Finestra 1200×1000: status bar torna visibile e tutte le righe sidebar rientrano (shot 123); a 760 sparisce di nuovo, rotella sulla sidebar non scorre (shot 124/125) → issue #344 (major).
- Retest su f0eca883: #342 Light leggibile (shot 127); #340 rotella 457–500 → 427–472 e ritorno (shot 131/132); #341 "Codex API key". Chiusi tutti e tre con commento "verified". Worktree w1H/w1J/w1K rimossi, branch fix/340-342 cancellati.
- Terminal font size: 14 pt (dal turno precedente, persistito dopo relaunch) e 16 pt (shot 133/134) → spaziatura righe del terminale attivo identica a prima (18 px, es. 457@y125, 458@y143). Il valore si salva ma non si applica al terminale live. Il "+" della tab bar apre un menu (New Terminal, Changes, New Browser, Claude Code, Codex, OpenCode, Pi, Oh-My-Pi, Split Claude Code, New Chat ›) invece di creare subito un terminale (shot 135) — non ho verificato un terminale nuovo; lo faccio dopo il relaunch con 16 pt salvato.
- Dopo la rimozione dei worktree fix/340-342 (23:09) la sidebar mostrava ancora le tre righe (shot 135, stesso minuto). Da controllare dopo il relaunch se spariscono.

## 23:11–23:17 — relaunch su 0b0ca8f5 (PID 78557)
- Cascata TCC di nuovo da zero (Scrivania → consentito; Download, Documenti, Apple Music → negati). Ad ogni build l'identità ad hoc cambia, quindi ogni relaunch ripete tutto. Selezionando ddb1a22 (23:15) altri due prompt (Documenti, Apple Music).
- PROGETTO FANTASMA: in sidebar è comparso il progetto "fix-next" (in cima) con un worktree "main". sirio.sqlite: project p-c7842c2a9a69bf20 name=fix-next root_path=~/Desktop/Progetti/worktrees/tiller-rust-gpui/fix-next, worktree branch=main (il checkout è su fix/next: etichetta errata). Mai aggiunto da me: alle 22:48 avevo solo selezionato il worktree fix/next di sirio (shot 108: nessun progetto fix-next in cima) e digitato "seq 1 400"+Enter senza terminale attivo. Enter e doppio click su una riga worktree NON creano progetti (DB invariato). È la 2ª volta che un progetto non richiesto appare dopo un relaunch (la 1ª: "/" riapparso dopo la rimozione). Log /tmp/sirio-dev.log: "[session] project fix-next could not be refreshed, keeping its last-known state: git worktree list --porcelain did not finish within 10s and was killed".
- Scrollback ripristinato dopo relaunch disegnato "a scaletta" (ogni riga parte dove finiva la precedente: LF senza CR) — shot 136/139; già visto alle 22:48 (shot 107, seq 299980…). 2/2 relaunch → da aprire come issue.

## 23:18–23:36 — #338 chiuso, #345 aperto, #343 round 1 fallito
- #338 retest su 0b0ca8f5: 12 probe AppleEvent ≤0,6 s durante `time seq 1 300000` con altra tab davanti; al ritorno frame finale corretto (shot 145). Chiuso "verified", worktree w1F e branch fix/338 rimossi.
- Registro pane: sul build 0b0ca8f5 `sirioctl panel list` mostrava SOLO pane-8 (creato a mano) e non i 2 terminali ripristinati con zsh vivo (1×). Dopo il relaunch successivo panel list mostrava pane-1..8 regolarmente. Non riprodotto → non classificato.
- Issue #345: scrollback ripristinato a scaletta (2/2 relaunch).
- Codex fix343 → 8620d8a0 (RightPanel::refresh passava repo_root stantio "/" a git status/read_tree; read_tree ora limitato alle root del catalogo). Merge squash 95c8f99e, rebuild, relaunch 23:28: prompt Scrivania (legittimo) ma l'app resta bloccata ~2 min con figlio `git rev-parse --short HEAD` in attesa del prompt (socket non in ascolto, nessuna shell). Poi prompt Download (23:30) e volume di rete (23:32) senza figli oltre a zsh. → revert 7ceaff0b, commento su #343.
- Esperimento: avvio diretto del binario con cwd = repo (23:35) → usa un DB "checkout-local" (~/Library/Application Support/Sirio/checkouts/sirio-4569c14a/sirio.sqlite, solo progetto sirio, niente fix-next) e compare comunque il prompt Documenti (shot 154). Quindi né il cwd "/" né il progetto fantasma sono la causa. tccutil reset fallisce ("No such bundle identifier app.sirioai.sirio": bundle dev non registrato in LaunchServices). Log tccd non leggibile senza sudo.
- Ipotesi in verifica: lo startup di `zsh -il` (rc utente, OrbStack in .zprofile, oh-my-posh) tocca Documenti/Download/volumi e TCC lo attribuisce all'app responsabile (Sirio). Spiegherebbe anche i prompt a ogni nuovo terminale/selezione worktree.
- Progetto fantasma: il worktree fix/next ha .git → ~/Desktop/Progetti/tiller-rust-gpui/.git/worktrees/fix-next (repo rinominato/sparito): `git status` → "fatal: not a git repository". Il progetto fix-next nel DB ha 1 pseudo-worktree "main" (fallback cartella semplice). Trigger UI non ancora individuato (Enter/doppio click esclusi).

## 23:36–00:03 — attribuzione dei prompt TCC (trovata)
- .zshrc/.zprofile puliti (xtrace di `zsh -il`: solo nvm/oh-my-posh, nessun path protetto). PATH, mount e symlink in home puliti; nessun symlink nei checkout.
- lldb attaccato a Sirio (script /tmp/qa-shots/fstrace2.py, breakpoint su open/opendir/stat/getattrlist… filtrati per Downloads|Documents|Music|/Volumes): 2500 hit ma zero path protetti nella finestra di osservazione — gli accessi avvengono nella syscall bloccata dal prompt, prima dell'attach. NB: uccidere lldb v1 con SIGTERM ha lasciato i breakpoint nel processo → Sirio crashato alle 23:57 (EXC_BREAKPOINT/SIGTRAP, report sirio-2026-09-02-235721.ips): crash indotto da me, NON bug di Sirio.
- `sample <pid>` (senza root) durante il prompt Apple Music sul build 7ceaff0b (fix revertato): thread bloccato in `opendir` con stack `sirio_ui::right_panel::files::RightPanel::refresh` → `read_tree` → `std::fs::read_dir`; stesso refresh spawna in parallelo `sirio_git::status` (run_with_timeout, posix_spawn git). Quindi il Files panel legge una directory i cui figli sono Documents/Downloads/Music (= $HOME) e una con /Volumes (= "/"): probabile stato "directory espanse" stantio del vecchio progetto "/" riapplicato a ogni refresh. Da verificare sul build con il fix (re-merge 95c8f99e) con lo stesso `sample`.
- Il Files panel mostrava intanto l'albero di nimble-ravine (worktree selezionato), quindi la directory letta non è quella visibile.

## 00:03–00:22 — #343 causa trovata (build 76c124a8 = fix round 1 riapplicato)
- Avvio 00:07: prompt Scrivania (legittimo). `sample` del main thread: bloccato in `main.rs:15448 → worktree_context (3569) → short_head (3586) → Command::output → poll` in attesa di `git rev-parse --short HEAD` (figlio 51078, cwd sotto ~/Desktop). Startup sincrono sul main thread.
- Dopo "Consenti" su Scrivania: subito prompt Download (00:09). Negato → subito Apple Music (00:15) → negato → volume di rete (00:18) → negato → Documenti (00:19) → negato → fine coda (shot 165–170). I prompt sono ACCODATI da un unico burst, non generati uno alla volta: per questo nessun thread di Sirio risulta bloccato mentre il dialogo è visibile.
- Tracer lldb v4 (/tmp/qa-shots/fstrace4.py, con /usr/bin/python3 — la venv non importa lldb) attaccato 240 s: ZERO path protetti aperti da Sirio stesso; 190 spawn di `git status --porcelain=v2 -z --untracked-files=all` in nimble-ravine/sirio (~1 ogni 1.2 s, da RightPanel::refresh — da annotare come osservazione prestazionale) + 2 `security find-generic-password` (cookie opencode/ollama).
- Poller figli (ps ogni 0.3 s): `00:19:35 pid=79506 cwd=/ args=claude` → è il probe usage (`sirio_usage::claude`, `$SHELL -lc claude` in PTY senza current_dir, ogni 5 min = REFRESH_INTERVAL status_bar.rs:26). cwd di Sirio = `/` (lancio via `open`). Claude Code avviato in `/` scandisce la cwd → /Volumes e ~/{Documents,Downloads,Music,Pictures} → prompt TCC attribuiti a Sirio.app.
- Il campione precedente (`read_tree → opendir`, build revertato) era una coincidenza: la directory letta era il worktree visibile; non è mai comparso nel tracer con path protetti.
- Round-1 di codex (allowlist in read_tree) puntava all'accessor sbagliato → round 2 con questo contesto.
- 00:24 aperta #346 (progetto fantasma: `/` e `fix-next`); causa del secondo: worktree stantio di sirio (`.git/worktrees/fix-next`) il cui checkout punta al vecchio path `tiller-rust-gpui` → promosso a progetto top-level. `sirioctl list-workspaces` lo mostra due volte (sirio/fix/next e fix-next/main).
- 00:23 creati worktree herdr w1N (fix/344) e w1P (fix/345), codex `fix344`/`fix345` avviati con contratto; `fix343` (w1M) ricevuto il round 2.

## 00:30–00:53 — #343 round 2, #344 pronto, #346/#347 aperte
- Codex fix343 round 2: 90d9063d "fix: isolate usage probes from launch directory" — probe usage (PTY claude, curl, security) con `current_dir(temp_dir())`; `short_head` (git rev-parse sul main thread) rimosso a favore di `sirio_project::discovery::read_head_label` (legge .git/HEAD). Test `usage_probe_command_has_a_safe_working_directory`. Squash-merge su main = 06c7a6e2; rebuild + relaunch 00:52.
- Relaunch 00:52: prompt Scrivania (legittimo). Main thread bloccato in `main.rs:15362 → worktree_context (3565) → read_head_label → fs::read_to_string → open` di .git/HEAD sotto ~/Desktop: la open stessa attende la risposta al prompt. Socket rifiutato finché non rispondo; dopo "Consenti" → pong subito. Inevitabile finché il progetto vive in una cartella protetta (osservazione, non bug della fix). Verifica in corso: nessun altro prompt per ≥7 min (watcher pixel /tmp/qa-shots/tcc-watch.sh) e dopo riselezione worktree.
- Codex fix344 done: 14fb7239 "fix: keep long sidebar lists scrollable" (solo sidebar.rs: `.min_h(px(0.0))` sul contenitore scroll, `flex_none()` sul tree, test `a_long_worktree_list_scrolls_inside_the_sidebar_viewport`). Merge rinviato a dopo la verifica #343 (il rebuild resetta lo stato TCC).
- Aperte #346 (progetto fantasma) e #347 (font size terminale senza effetto, passo 18 px a 13/14/16 pt). Interface font size risulta 12 pt (default 13): non so se l'ho cambiato io — da riportare a 13 a fine sessione.
- 00:56 riprodotto (2/2) il riavvio del terminale al ritorno su ddb1a22 dopo aver selezionato main: pane-9 invariato ma nuova zsh (73465, 00:56:45) e la vecchia (64591, 00:53:54) resta viva e orfana; scrollback 137 byte = solo il nuovo prompt. Aperta #348.
- 01:00 squash-merge su main (senza rebuild, in attesa della finestra di verifica #343): 0112aa30 fix/344, 6ed8b476 fix/345.

## 00:56–01:05 — verifica #343 ok, filtro, menu contestuale
- #343 su 06c7a6e2: dopo "Consenti" su Scrivania nessun altro prompt in 7 min (watcher pixel 00:56:30–01:03:30, 0 rilevamenti; copre i fetch usage delle ~00:54 e ~00:59 e il cambio worktree main↔ddb1a22). Conferma finale al prossimo relaunch (rebuild 344+345 → TCC azzerato → atteso solo Scrivania).
- B4 filtro sidebar "fix": ok (mostra fix-next, fix/343-345, fix/next; nasconde il resto). Cmd+A nel campo non seleziona tutto (1×, forse cliclick): backspace cancella un carattere alla volta.
- B12 menu contestuale worktree (tasto destro): voci Set Primary / New Terminal / Claude Code / Codex / OpenCode / Pi / Oh-My-Pi / New Chat / Remove Worktree. BUG: il menu compare sempre in alto a sinistra della finestra (69,100) e non al puntatore (click a y=565 e y=429, 2/2); Esc non lo chiude (2/2). Shot 187–190.

## 01:07–01:20 — relaunch con 344+345, #344 non risolto
- Relaunch 01:07 (6ed8b476): solo prompt Scrivania → Consenti → pong. Watcher 01:08–01:14: 14 rilevamenti ma tutti falsi positivi (frame mostrano la tab strip/prompt del terminale sotto le sonde; nessun dialogo TCC). #343 confermato anche su questo build.
- #344 su 6ed8b476: a 1200×760 lista ancora tagliata, status bar assente, rotella (2 tentativi) inerte; a 1200×1000 con 21 righe la status bar sparisce. Revert 0112aa30 (main 6ed8b476 → revert), commento su #344, round 2 a codex fix344 (ipotesi: vincolo di altezza mancante negli antenati reali in main.rs, il test monta la Sidebar nuda).
- #345: MARKER-C + seq 1 30 stampati nel tab attivo (shot 198) prima del relaunch; verifica allineamento al riavvio.
- Aperta #349 (menu contestuale in alto a sinistra, Esc inerte). Codex fix346/347/348 in lavoro.

## 01:19–01:28 — #343 chiuso, persistenza scrollback
- Relaunch 01:19 (revert 344): solo Scrivania; watcher con sonde corrette per la cattura 2× (le due run precedenti avevano coordinate raw sbagliate → falsi positivi/negativi): 0 dialoghi in 6 min. #343 chiuso "verified"; worktree w1M e branch fix/343 rimossi.
- #345: dopo il relaunch nessuno scrollback ripristinato (tab vuoti): tab_state in DB non contiene MARKER-C — la sessione non aveva salvato l'output (build-dev uccide l'app con pkill; il flush non cattura l'ultimo output). Test non valido → ripeto con quit graceful (Cmd+Q via osascript) e `open -n` dello stesso bundle.
- Al relaunch l'ordine dei worktree sotto sirio è cambiato: ddb1a22 (selezionato) sopra main/Primary (1×, shot 201).
- Codex fix346 done: eed5d3cd (session.rs: layout con working directory git non in catalogo → scartato; progetto che è linked worktree di un altro → scartato al load; main.rs: fallback project solo se in catalogo). 4 rossi tutti nel pool noto. Squash-merge su main (rebuild dopo il test #345).

## 01:41–01:50 — chiusura #345, merge round 2 #344, merge #347/#348

- **#345 chiuso "verified"**: dopo Cmd+Q (osascript quit) e relaunch dello stesso bundle,
  shot 204 mostra `echo MARKER-D; seq 1 25` + `1…25` allineati a sinistra sopra il nuovo
  prompt (01:41:18). Evidenza `staircase-03-fixed-after-relaunch.png`. Worktree w1P e
  branch fix/345 rimossi. Nota: i controlli `instr(state,'MARKER')` su tab_state erano
  privi di senso — lo scrollback è serializzato come array di byte, non come testo.
- **#344 round 2** (576fbda7 su fix/344): causa vera = wrapper della work-area in
  `SirioWorkspace::render` era `.flex_1()` senza `.min_h_0()`, quindi la sidebar riceveva
  l'altezza sovradimensionata dell'antenato. Test a livello workspace
  `workspace_keeps_a_long_sidebar_scrollable_and_status_bar_pinned` (1200×760, 20
  worktree, wheel). Squash su main **9190750d**. Da ritestare sull'app.
- **#347** (519a02e9): font size Settings → `TerminalView::set_font_size` →
  `TerminalElement` usa `self.font_size` e `line_height_for_font_size`; cell height misurata
  condivisa via `last_cell_height` per mouse/scroll/kitty. Squash su main.
- **#348** (7856e66e): `select_worktree` rifaceva sempre `restore_tabs`; ora la cache
  `(worktree, content_id)` parcheggia le entity in uscita e `restore_tabs_with_terminal_cache`
  riusa la view montata (`is_host_mounted`). Squash su main.
- Codex ha segnalato per entrambi il pool rosso noto (5 test) + flake da carico PTY quando
  più worktree testano insieme — coerente con la baseline.

## 01:51–02:05 — rebuild (344 r2 + 346 + 347 + 348), regressione da fix346

- Rebuild 01:51: solo prompt TCC Scrivania (ok). Log: `project fix-next is a linked worktree of
  another project and was dropped` (fix346 attivo). Sidebar: progetto fantasma sparito (shot 206).
- **#347**: passo riga misurato 22 pt a 16 pt (crop BMP di shot 206; prima 18 fisso) → il
  setting arriva al renderer. Manca la controprova a 13 pt.
- **#344**: a 1200×760 status bar ancorata (207) e lista scorre con la rotella fino a New
  Worktree… (208) → chiuso "verified", w1N e fix/344 rimossi, B14 ok.
- **#345**: 2ª conferma (206: righe 7…25 ripristinate allineate).
- **REGRESSIONE (fix346, b3ab441d)**: selezionando `main` e tornando su `ddb1a22`, "No
  Terminals": i 9 tab persi, registry vuoto, DB `tab` a 0 righe (210). Repro con marker
  (MARKER-F): tabs(wt-9) 1 → [select main] 1 → [reselect ddb1a22] 0. Round trip su `main`
  (wt-0) con MARKER-G: il tab sopravvive (214).
  Causa: gli id worktree persistiti sono `wt-<indice in git worktree list>`. Dopo la rimozione
  dei worktree fix-344/fix-345 (herdr) ddb1a22 è passato dall'indice 9 all'8, ma nel DB resta
  wt-9. Il lettore `restore_tabs_for` → `persisted_worktree_id` → `catalog_ids_for_path`
  (Git) cerca wt-8 → 0 tab; lo scrittore `write_layout` da b3ab441d usa
  `persisted_catalog_ids_for_path` (DB, per path) → scrive il layout vuoto su wt-9 →
  cancellazione. Prima di b3ab441d le due parti concordavano (entrambe Git), quindi i tab
  "migravano" all'id sbagliato invece di sparire — probabile origine anche del sintomo
  originale di #348 (avevo rimosso il worktree fix-343 poco prima della repro).
- Registry pane vuoto dopo cambio worktree = comportamento preesistente (riflette solo i tab
  a schermo; `worktrees.limitMounted=false` → cap 0 → nessuna eviction).
- Shell zsh orfane accumulate: 72592, 35112, 43017, 48260 con 1 solo pane → le entity
  parcheggiate nella cache di fix348 non vengono mai riusate se il restore non trova tab.
- Azione: revert di b3ab441d su main; commento su #346; round 2 a codex fix346 (w1Q) con il
  vincolo: lettore e scrittore devono risolvere l'id dallo stesso posto (DB per path, con
  fallback Git), e l'id non deve dipendere dall'indice della lista.

## 02:10–02:30 — rebuild senza fix346, retest #348 fallito, round 2 a fix346 e fix348

- Rebuild 02:10 (main 5b802b7a: 344 r2 + 347 + 348 + revert 346). Il prompt TCC Scrivania è
  apparso DOPO il ping (asincrono) e ha bloccato il main thread: `select-workspace` →
  "control action dispatch bound (5.0s) fired before the worker replied"; il primo tentativo di
  retest è invalido (marker digitato nel terminale di main).
- fix349 done (8c11119f): posizione dal MouseDownEvent, focus handle + Esc → dismiss, test
  rosso→verde. Squash su main af4123b1 (non ancora ricompilato).
- **#348 retest (DB campionato)**: ddb1a22 (wt-8) 2 tab → select main → wt-8 = 0, wt-0 = 2
  (i tab di ddb1a22 scritti sulla riga di main!) → ritorno: "No Terminals"; zsh 57739,
  75199, 77000 vive con 0 pane. `catalog_ids_for_path` usa `position(...).unwrap_or(0)`:
  quando la lista scoperta non contiene il path, il layout finisce sotto wt-0. Stessa
  funzione aveva dato wt-8 pochi secondi prima → non deterministica (git lento/ucciso?).
- Commenti su #348 e #346; round 2 a fix348: ricostruire i tab dalle entity vive della cache
  alla riselezione di un worktree montato, rilasciare le entity non riusate, test con PTY
  reale e conteggio zsh. fix346 r2 già in corso (id per path dal DB, niente unwrap_or(0)).
- Memoria salvata: `sirio-worktree-ids-are-index-based`.

## 02:28–02:35 — #347 chiuso, titlebar A6/A7, review fix346 r2

- #347 chiuso "verified" (16 pt → 22 pt, 13 pt → 18 pt); w1R e fix/347 rimossi; setting a 13.
- A6 "+" titlebar (210,59): nessun effetto visibile (shot 226 = 227). 1 osservazione, da ripetere.
- A7 refresh titlebar (1190,59): la selezione salta da ddb1a22 a main, che mostra 3 terminali
  (incluso il MARKER-H di ddb1a22): il tab creato su ddb1a22 è stato riscritto sotto main
  (bug identità #346/#348). Da ritestare a fix mergiati.
- fix346 r2 (1c93dadc): `worktree_id_for_database` (DB per path, fallback Git);
  `SessionStore::persisted_worktree_id`/`new_tab_id`; `write_catalog` conserva gli id per path
  e alloca id nuovi senza collisioni; `restore_tabs` usa `persisted_worktree_id_for_database`.
  Test `worktree_ids_and_tabs_survive_a_preceding_worktree_removal` rosso→verde. Resta
  `unwrap_or(0)` nel fallback Git → round 3 richiesto. Squash-merge su main in corso
  (conflitto main.rs con 347/348).

## 02:40–02:55 — rebuild 3 (346 r2 + 349), retest #349 ok, #346 id stabili, r3 e 348 r2

- Rebuild 02:40 (main 432f51c0). Dopo il riavvio gli id DB restano ancorati al path: ddb1a22
  = wt-8 con indice git 7; la riga wt-3 (fix-347 rimosso) sparisce senza rinumerare.
- **#349 chiuso "verified"** 2/2 (shot 230/232 menu al puntatore, 231/233 Esc chiude). w1T e
  fix/349 rimossi, B12 ok.
- Il prompt TCC Scrivania compare ~1–3 min dopo il launch (dopo il ping) e blocca il main
  thread → `sirioctl` "dispatch bound (5.0s)"; input digitato nel frattempo finisce a caso.
  Creato `/tmp/qa-shots/tcc-allow.sh <sec>` (poll screenshot, soglia 45 sull'icona, click
  Consenti) da lanciare dopo ogni relaunch.
- fix346 r3 (03c35342): `catalog_ids_for_discovered_path` → path assente dalla lista scoperta
  → id derivati dal path, mai wt-0; test dedicato; cherry-pick su main e3ec9f78.
- fix348 r2 (818a32b3): `parked_worktree_tabs` (layout + pane id per tab) alla partenza dal
  worktree; `restore_tabs_for_mounted_worktree` ricostruisce dai pane vivi
  (`is_host_mounted`), integra i tab DB non in conflitto, `remove_unkept_in_worktree`
  rilascia le entity non riusate. Cherry-pick su main.

## 02:50–03:00 — rebuild 4 (346 r3 + 348 r2), retest e chiusure

- Rebuild 02:51 (main 6273c7a7). Watcher `tcc-allow.sh` ha consentito il prompt alle 02:52:28.
- Trappola: gli id di `sirioctl list-workspaces` sono posizionali (control state), NON quelli del
  DB: dopo le rimozioni wt-8 era 8f60f62. `roundtrip.sh` ora risolve gli id per nome branch.
  Primo giro (MARKER-K) ha testato 8f60f62: tab persistito sotto wt-10 (giusto), stessa shell.
- Round trip ddb1a22 (control wt-6, DB wt-8): MARKER-L → tabs8 1/1/1, zsh 4→4, marker nello
  scrollback e a schermo (239c). Con shift (rimosso worktree qa-evidence, indice 6→5):
  MARKER-M → tabs8 2/2/2, zsh 5→5, marker presente (240c). qa-evidence ri-aggiunto.
- Relaunch stesso bundle: 1 progetto, ddb1a22 = wt-8 con 2 tab, selezione ripristinata;
  refresh titlebar mantiene selezione e tab (A7 ok).
- **#346 e #348 chiusi "verified"**; w1Q/w1S e branch fix/346, fix/348 rimossi.
- Stato: nessuna issue aperta dalla QA; restano i todo di Fase 1.

## 03:00–03:10 — Settings (K2–K10), titlebar A5/A6, terminale C2/C6/C8/C10

- Settings: tutte le sezioni osservate (K02–K06). Auto-rename → `general.autoNaming` true/false in
  sqlite e ripristinato; Agents Refresh aggiorna il timestamp; Base color/Translucency/Interface
  font rispondono e sono stati ripristinati (Neutral, on, 12 pt). Nota: "Automatic updates" ON
  con "Updates are off" (canale dev).
- **#350 aperta**: lo scroll resta al cambio sezione (General in fondo → AI Providers apre con
  Claude Code fuori schermo, 2/2: K18, K21). minor.
- Titlebar back/forward/"+": inerti in 3 stati, 1 finestra, panel list 2→2. È un seam non
  cablato per design (titlebar.rs, commento in testa; #299 aperta sulle etichette). A5/A6 ok con
  nota nel report, nessuna issue.
- cliclick non digita `]` sul layout italiano: `printf '\e]0;…'` arrivava senza `]`. Per i test
  OSC usare uno script file (`sh /tmp/qa-shots/osc2.sh`).
- **#351 aperta (major)**: Cmd+V nel terminale non incolla (4/4: corto, OSC, 1500 char; via
  cliclick e System Events). Right-click → Paste funziona (CTX-PASTE). Nessun binding cmd-v nel
  codice del pane.
- C10: titolo OSC non cambia il nome tab ("Terminal" anche in `panel list`); "Set Title" nel menu.
  Badge "Running" in basso al pane durante `sleep 8`, sparisce a fine comando. Non bug.
- C8: Split Right dal menu → 2 pane, focus per click, LEFT-PANE/RIGHT-PANE nel pane giusto.
  Dopo lo split: tab status "?" (era "○"), ddb1a22 salta in cima alla sidebar con punto giallo,
  persistente >60 s. Da capire/riprodurre (falso "needs input"?).
- Menu contestuale terminale: Copy, Paste, Copy Context, Set Title, Copy Pane ID, Copy Terminal
  ID, Split Left/Right/Above/Down, Clear Terminal, Restart Terminal, Close Terminal….

## 03:10–03:25 — C8 split, C2/C5/C6, resize, issue #352

- Falso needs-input dopo il primo Split Right non riprodotto (0/2 con la stessa sequenza OSC+sleep).
  Ordinamento sidebar per stato è by design (sort.rs: NeedsInput=1 in cima). Resta osservazione.
- Cmd+C non copia la selezione (2/2 su testo reale) → commento su #351. Copy dal menu funziona.
  Attenzione: dopo un resize le righe vuote sotto il prompt accettano la selezione ma Copy non
  fa nulla (selected_text vuoto) — i primi 3 tentativi erano su righe vuote, non un bug.
- Resize 820×620: reflow ok; etichette sidebar lunghe a capo e sovrapposte (2/2) → **#352 aperta**.
- Menu contestuale vicino al bordo inferiore si sposta in alto per restare in finestra; lo sfondo è
  semitrasparente e le voci si leggono male sopra il testo del terminale (estetico, non bug).
- Stato: 3 issue aperte dalla QA in questa fascia (#350 minor, #351 major, #352 minor).

## 03:25–03:40 — forms "+", Esc, catalogo stale, hijack tab; codex su 350/351/352

- Clone/Create card: Esc ignorato 2/2 ciascuna, click fuori ignorato (Create), Project Settings
  Esc ignorato 2/2 (contro decisione #155) → **#353 aperta** (minor). Cancel/Close funzionano.
  Project Settings apre sopra la Create card ancora aperta (impilati).
- "Show in File Manager" non verificato: il click è finito sul modale impilato. Da rifare.
- Il ↻ della titlebar è `on_history` → RestoreLaunchSnapshot, non un refresh. Non esiste un
  refresh del catalogo a runtime: fix/346 e fix/348 (rimossi da git) restano in sidebar, fix/350–352
  (creati da herdr) non compaiono → **#355 aperta** (major).
- Click su fix/346 (cartella mancante): tab Chat + Terminal "failed to start"; tornando a ddb1a22
  i suoi 2 terminali spariscono (panel list = solo pane-0 Chat, 1 zsh viva su 2) e i tab falliti
  seguono ogni selezione (fix/348, ddb1a22 di nuovo). DB: wt-8 (ddb1a22) ora chat+terminal.
  2/2 → **#354 aperta** (major). Probabile canonicalize() fallita su path mancante in
  `worktree_id_for_database` → fallback posizionale → wt-8.
- codex: fix350/351/352 avviati (herdr w1V/w1W/w1X). Trappola zsh: `set -- $pair` non splitta →
  nome agente con spazio → "invalid_agent_name". Trappola regex: pane id contengono maiuscole (w1V).
- Stato UI corrotto (tab dirottati): serve relaunch prima di B10/B11/B7.

## 03:40–03:50 — merge 350/351/352, rebuild 5

- Report codex: fix350 8f239d4b (ScrollHandle + reset a cambio categoria, test drawn RED→GREEN,
  sirio_ui 562 ok); fix351 a5a0b476 (cmd-c/cmd-v → Copy/Paste, bracketed paste 2004, 2 test PTY
  headless; sirio_terminal 128 ok + `mutation_stamp_stays_flat_while_idle_and_bumps_on_output`
  rosso dichiarato baseline — verifica su main in corso); fix352 37106686 (nowrap+overflow_hidden
  al posto di line_clamp(3); test drawn a 180 px).
- Squash-merge su main: 4917610a (#350), 95555605 (#351), 16daf80c (#352). build-dev avviato.
- Prompt a fix354/fix355 partito con header "#350" per errore (template già renderizzato):
  correzione inviata subito dopo; entrambi "working".

## 03:50–03:58 — rebuild 5 (350+351+352), retest e chiusure

- Rebuild 03:42 (main 16daf80c). TCC Scrivania consentito dal watcher alle 03:42:56. Catalogo
  ridiscoperto al relaunch: fix/350–355 presenti, fix/346/348 spariti; ddb1a22 ripristinato con
  Chat + Terminal (il layout dirottato da #354: MARKER-L/M persi).
- **#350 chiuso** (AI Providers e Agents in cima dopo scroll di General, 2/2), **#351 chiuso**
  (Cmd+V incolla `echo CMDV-OK`, Cmd+C copia la selezione), **#352 chiuso** (ellissi a 820 e 700 px).
- `mutation_stamp_stays_flat_while_idle_and_bumps_on_output` (sirio_terminal) rosso anche su main
  con build in parallelo: "startup never settled" → flake da carico, aggiunto alla baseline.
- Worktree herdr fix-350/351/352 NON rimossi ora: rimuoverli a runtime crea righe stale (#355) e
  sposterebbe le coordinate; li tolgo prima del prossimo relaunch.
- All'avvio della shell nel pane: "pyenv: cannot rehash: couldn't acquire lock" (ambiente locale,
  non Sirio).

## 04:05–04:20 B10/B11 New Worktree + Remove, issue #356 #357, codex #353
- New Worktree… ×3 (qa-wt-test, -2, -3): git crea branch+worktree, riga appare in sidebar (B11e/B11f). `sirioctl list-workspaces` resta a 12 righe, senza il nuovo worktree, anche dopo averlo selezionato → #356 (minor, probabile stessa radice di #355).
- × su riga → dialog "permanently deletes the worktree's directory and branch on disk" → Remove Worktree: worktree deregistrato, cartella cancellata, ma `git branch --list qa-wt-test*` restituisce tutti e tre i branch anche dopo minuti → #357 (minor). Log app muto.
- Prima verifica del branch di qa-wt-test (5 s dopo) già "presente"; le due misure "0" del run successivo erano artefatti (`$G` quotato / cwd in /tmp): invalidate, rifatte con `git -C`.
- Branch di test cancellati a mano (0 commit unici).
- codex fix354 done: 043110ca "fix: isolate missing worktree session state" (main.rs +94/-, session.rs +76). 2 test nuovi verdi, 4 rossi = baseline. Diff in revisione.
- codex fix353 avviato in w10 (worktree fix-353) con contratto prompt-353.txt. fix355 ancora working.

## 04:30–04:42 D1 Claude Code, D2 Codex, merge 354+355
- Menu "+" del tab strip: New Terminal / Changes / New Browser / Claude Code / Codex / OpenCode / Pi / Oh-My-Pi / Split Claude Code / New Chat ▸ (y 133…421, x≈850).
- D1: Claude Code parte (hook utente rumorosi ma estranei), glifo "?" al prompt, "●" arancio in lavoro, riga ddb1a22 sale in cima con NeedsInput e torna in posizione con exit; "/exit" → "✓ exit 0" + "Process exited successfully". D1a–D1d.
- D2: Codex 0.152.1 YOLO. Al prompt iniziale glifo "●" + icona codex sulla riga (Layer D = processo vivo ⇒ "running"), dopo il primo turno "?" via notify. Risposta "ok". D2a–D2d. Tab strip va in overflow con chevron ⌄ quando i 4 tab non entrano (Chat/Terminal nascosti).
- fix/354 (fb10762e, test #346 ripristinato su mia richiesta) → squash 22f792a8. fix/355 (6808ca2e) → squash 569ffccc, nessun conflitto. Nel run di codex 355 `reselecting_a_worktree_reuses_the_mounted_terminal_handle` era rosso: da rieseguire su main mergiato.

## 04:43–04:52 rebuild 6 (569ffccc) e retest #354/#355
- Relaunch: 4 tab di ddb1a22 ripristinati, Claude Code e Codex rilanciati (Codex mostra "○" al prompt dopo restore, mentre al lancio fresco mostrava "●"). 9 worktree.
- #355: `git worktree add /tmp/qa-wt-ext` → nessuna riga finché la finestra non torna attiva (R6b), poi riga presente (R6c) e in `list-workspaces`; `git worktree remove` → riga sparita dopo riattivazione (R6d), control 0. Menu progetto: Project Settings / Refresh Project / Initialize Git (disabilitato) / Show in File Manager / Remove Project (R6e). Chiusa verified.
- #354: worktree esterno rimosso con Sirio in primo piano, click sulla riga stantia → la riga sparisce prima della selezione (refresh in select_worktree), ddb1a22 resta selezionato con i suoi 4 tab (R6g); Terminal risponde a `echo ALIVE-354` (R6i). Chiusa verified.
- Test mirati su main mergiato: reselecting_a_worktree_reuses_the_mounted_terminal_handle ok (44 s), missing_worktree_switch_… ok, refreshing_a_project_… ok → il rosso nel run di codex 355 era flake da carico.
- fix/357 (eb11316e: `git branch -D` dopo `worktree remove`, branch = titolo riga) → squash 5ea8a3e4. fix/353 (915537d4: Escape in capture al livello shell + on_mouse_down_out + esclusione mutua, 7 test gpui) in merge.
- DB: due righe worktree con branch "main" (3 e 4 tab): da controllare i path.

## 04:52–05:02 rebuild 7 (8e5729c3): retest #353, #357, #356
- #353: Settings Esc ✓, Settings click fuori ✓, Clone Esc ✓, Create click fuori ✓; con Settings aperto il click su "+" chiude solo la scheda (niente impilamento) (353a–353h). Chiusa verified.
- #357: qa-wt-in creato dal form, × → Remove Worktree → worktree, cartella e branch tutti rimossi (2/2 con qa-wt-in2). Chiusa verified.
- #356 riprodotto e peggiorato: click sulla riga qa-wt-in/qa-wt-in2 → nessun cambio workspace (status bar resta ddb1a22), control 0 di 7 anche dopo il click (356b, 356h). Dopo la rimozione: control 1 di 8 con riga stantia, highlight su 8f60f62 (357b), tutto pulito al prossimo select/focus (356c/d). Riletto B11f della build vecchia: stesso sintomo (l'avevo mancato). Issue aggiornata (titolo + commento, major), codex fix356 avviato in worktree fix-356.
- Log app: nessun errore per la selezione fallita (silenzioso). Un "[control] failed to clear stale application panes: invalid pane working directory" all'avvio (pane di un worktree rimosso).
- Layout dopo relaunch 7: main 206, qa-evidence 259, green-meadow 293, green-valley 327, ddb1a22 361 (Chat 395, Terminal 429, Claude Code 463, Codex 497), fix/next 531, 8f60f62 565, New Worktree… 599.

## 05:00–05:08 D3–D6 in worktree green-valley (click di cleanup finito su quella riga: fix/356 aveva spostato le righe)
- D3 OpenCode 1.18.15 (GPT-5.6 Luna): "●" a 3 s, in lavoro e al prompt (solo Layer D); risposta "ok" 5,4 s; /exit → "✓ exit 0". D3a–D3e.
- D4 Pi (glm-5.3-flash): identificato dal titolo (icona π), "●" al lancio, "?" dopo il turno; /exit non è un comando (il modello risponde), /quit esce → "✓ exit 0". D4a–D4e, D45.
- D5 Oh-My-Pi (GPT-5.6-Luna): "●" al lancio e in lavoro, "?" a fine turno; /quit → "✓ exit 0". D5a–D5e.
- Riga worktree: sale in cima con pallino arancio quando un pane è NeedsInput; icona dell'agente attivo a destra; pallino verde quando tutti usciti.
- Badge arancio sull'icona Chat del pannello destro comparso durante D4/D5 (da guardare in J1).
- D6 Split Claude Code sul tab "Pi" (uscito): il tab resta intitolato "Pi" ma prende icona ✳ Claude, agente "claude" anche per il pane Pi in `panel list`, glifo "? exit 0" insieme (D6a). 1 osservazione, ripeto sul tab Terminal.

## 05:12–05:20 — Sezione E avviata, merge #356

- Status bar (E0c): badge completi "Claude 30% 5h · 27% wk / Codex API key" → il collasso in "⋮" è intermittente, non riprodotto ora. Terminal ddb1a22 non vuoto (prompt visibile).
- Sottomenu New Chat: si apre solo al click su "New Chat ▸" (non all'hover). Voci: Claude Code, Codex, OpenCode, Pi, Other agents….
- E1 New Chat → Claude Code: tab "Claude Code" creato, composer "connecting" → dopo ~4 s "Message Claude Code — @ for files, / for commands", mode "Don't Ask", Effort Xhigh, Context 0%. Notifica macOS "Claude Code — sirio/ddb1a22" alla creazione (E1a).
- Messaggio "Reply with exactly the word ok…": bolla utente, "Thinking", status "working", risposta in ~10 s, orario 05:18, Context 9%. OK funzionale.
- Osservazione E1d: il testo della risposta ("ok") è renderizzato attaccato al blocco "Notice: UserPromptSubmit says: … run /remember:doctor" (→ "…/remember:doctor.ok"), senza separazione fra notice di sistema e messaggio assistant. Da riprodurre (serve una nuova chat: la notice arriva solo al primo prompt di sessione).
- Osservazione E1c: mentre la chat Claude lavorava, il tab inattivo "Chat" (Codex chat, stato "?") mostrava "●"; a fine turno è tornato "?". Possibile leak di stato fra tab chat dello stesso worktree. Da riosservare in E5.
- Codex fix356 done: commit 07f678e2 (sidebar emette WorktreeCreated/WorktreeRemoved; host refresh_catalog_project + select; 2 test). Nota: refresh_catalog_project ora sincronizza control state + sidebar anche senza delta del catalogo (chiamata a ogni attivazione finestra) → verificare che la sidebar non perda stato all'attivazione. Squash-merge su main = 5c583b15. Branch fix/353, fix/357 cancellati. Worktree fix-356 (w22) tenuto fino al retest.

## 05:20–05:25 — E2 avvio, leak di stato fra tab chat, control state con id duplicati

- E5a (2ª osservazione): mentre la chat Claude lavora, il tab "Chat" (chat legacy, "?") mostra "●"; a fine turno torna "?". Il tab "Chat" aperto (E2a) è una chat legacy: "This chat was saved before Sirio recorded which agent it belonged to…", composer "Agent offline — reconnecting when you send…", stato "offline", "Unknown agent", bottone "Open Settings". Quindi il "?" è lo stato di quella chat offline; il "●" durante il turno dell'altra chat è un leak. Codice: `pane_status` (main.rs ~6455) legge `activity.status("pane-{id}")` con fallback `tab-{tab.id}`; `sync_entity_evidence` (~6377) spinge `surface_evidence` per pane. Da capire perché il pane chat legacy riceve Running.
- E2b/E2c: New Chat → Codex crea tab "Cod…" con "○", composer "connecting" → "Message Codex — @ for files". Con 6 tab il tab strip mostra solo gli ultimi 2, gli altri finiscono nel chevron "⌄" (già noto).
- E2c: dopo che herdr ha creato il worktree fix-358 e la finestra è stata riattivata, la sidebar mostra la riga fix/358 (refresh #355 ok) MA l'evidenziazione è saltata su "main" e ddb1a22 si è collassato, mentre il workspace reale resta ddb1a22 (status bar + `panel list`). 1ª osservazione; fix356 (5c583b15, non ancora in build) dichiara di ripristinare l'highlight dopo il refresh → riverificare dopo il rebuild.
- `sirioctl list-workspaces` dopo quel refresh: id duplicati — `wt-2` sia per fix/356 sia per green-meadow, `wt-3` sia per fix/358 sia per green-valley (id conservati per path per le righe vecchie, nuovi indici per le nuove). `select-workspace wt-2` è ambiguo. 1ª osservazione; ricontrollare al retest #356 (creazione worktree in-app) prima di aprire issue.
- `panel list`: i pane chat (pane-4 "Claude Code", pane-5 "Codex") hanno colonna agent vuota, il terminale Claude (pane-2) ha "claude". Osservazione, non classificata.

## 05:27–05:33 — E3/E4, issue #359, codex fix359/fix358

- E3 OpenCode chat: alla digitazione il composer era ancora "connecting" (12 s) ma il messaggio è stato accodato e inviato alla connessione; risposta "ok" in <17 s; mode "build", "M Effort None", Context 6%.
- E4 Pi chat: connessa in <8 s ("Thinking: high", "Model …"); la cartella `.pi` compare nel file tree del worktree (creata da Pi). Risposta "ok" con blocco "Thought" collassabile e la riga "Pi input UI request is not supported in ACP yet; cancelling it." (limite dell'adapter ACP di Pi, upstream, non Sirio).
- Durante il turno Codex (E2e) il tab chat Claude inattivo resta "✓": il leak di #359 riguarda solo il tab legacy. Issue #359 aperta (minor), evidenze su qa-evidence (ac9b1446). Codex fix359 lanciato in ~/.herdr/worktrees/sirio/fix-359 (w24:p1).
- Nel status bar compare "· 47% Fable" accanto a Claude dopo il turno Claude (usage per modello), poi sparisce: intermittente come gli altri badge.
- Codex fix358 done: 586efb35 — SplitClaudeCode non assegna più agent_id/icona al tab; `pane_agent_id` per pane in `sync_control_panes` (panel list riporta "" e "claude"); `terminal_exit_label` soppressa se c'è un terminale vivo; `tab_agent_mark` legge il pane primario (`tab_primary_pane_id`) invece del primo pane con agente; 2 test. Squash-merge su main dopo fix356.

## 05:33–05:40 — E6/E7, merge #358, build main

- Chevron "⌄" del tab strip: elenca tutti gli 8 tab con spunta sull'attivo; selezione funziona (E6a).
- Menu modalità permessi chat Claude: Auto, Manual, Accept Edits, Plan Mode, Don't Ask, Bypass Permissions (E6c). Codex: Ask for approval, Approve for me, Full access (E6i).
- E6: in Manual (Claude) `ls -1` e `touch /tmp/sirio-qa-perm-test.txt` eseguiti senza prompt; in "Ask for approval" (Codex) `touch /tmp/sirio-qa-perm-codex.txt` idem. Causa: config utente globale permissiva (vedi checklist). Non classificabile come bug di Sirio; non osservabile qui. File temporanei rimossi.
- E7: stop durante lo streaming → "cancelled" nel separatore orario, stato idle. I turni precedenti collassano in righe "Turn: …" quando la conversazione cresce.
- Osservazione: durante un turno il tab attivo mostra "● × ●" (secondo pallino a destra della ×) — probabile indicatore "attività non letta"; scompare a fine turno.
- Merge fix/358 → main 4187a6fc (dopo 5c583b15). `cargo build -p sirio` verde; i 4 test nuovi (split_claude_keeps_a_non_claude_tab_identity, drawn_split_tab_shows_live_status_without_exited_sibling_label, sidebar_create/remove_worktree_refreshes_catalog_and_control_state) passano. Rebuild + relaunch rimandato a fine sezione E per non interrompere le chat.

## 05:41–05:53 — F1 Changes, #360/#361, merge #359

- `+` → Changes apre il pane Changes in un gruppo affiancato (split) al gruppo dei tab; toolbar: "Local changes (N)", toggle lista/side-by-side, refresh, ⇕. Gruppi Changed/Untracked/Staged con ⊞/⊟ (Stage all/Unstage all) sull'intestazione; espandendo una riga compaiono Discard, Stage (o Unstage), ↗ e l'hunk inline con "N hidden lines ⌄". Nessun menu contestuale sulle righe.
- File tree destro marca i file modificati con "• Diff" appena cambiano su disco (F4 parziale ok).
- BUG #360 (major): con `…/.git/worktrees/sirio/index.lock` presente (stantio dalle 01:12, 0 byte, nessun processo git) Stage/Stage all non fanno nulla e la UI non mostra errore (titolo resta "Local changes (4)"); `sirioctl surface changes stage README.md` → "git exited with status 128: fatal: Unable to create '…/index.lock': File exists". Rimosso il lock, Stage funziona (CONTEXT.md staged). Riprodotto ricreando il lock con touch. Evidenze su qa-evidence 230be6e3.
- BUG #361 (minor): header hunk lungo (README.md, CONTEXT.md) va a capo su 3 righe e si sovrappone a "24 hidden lines" e alla prima riga del diff.
- Discard su LICENSE: dialog "Discard changes? … This cannot be undone. [Cancel] [Discard]" ok, ma dopo Discard il file resta " M" → verificare se il lock è ricomparso (chi lo crea? il log mostra `git worktree list --porcelain did not finish within 10s and was killed` ripetuto per "project fix-next": Sirio uccide processi git al timeout).
- Log: `[session] project fix-next could not be refreshed … git worktree list --porcelain did not finish within 10s and was killed` ripetuto molte volte + `fatal: not a git repository: …/tiller-rust-gpui/.git/worktrees/fix-next`. Da riportare (osservazione, possibile causa di lag al focus finestra).
- Merge fix/359 → main 73d4fc13. Codex 359: causa = tab chat legacy ripristinato senza root_id prendeva il fallback pane-3 già usato da un altro tab → chiave `pane-3` condivisa; fix = allocatore di id per i tab ripristinati (`restored_pane_ids`), test restored_legacy_chat_does_not_inherit_sibling_pane_activity.

## 05:53–06:02 — F1 azioni git dalla UI: perse in modo intermittente

- Senza lock: Discard LICENSE → dialog ok, conferma → file resta " M"; `sirioctl surface changes discard LICENSE` lo scarta. Discard README: 1° click non apre il dialog, 2° tentativo apre e la conferma scarta davvero. Unstage CONTEXT.md: 2 click, nessun effetto; `sirioctl surface changes unstage` funziona. Nessun processo git vivo durante le prove. Commenti aggiunti a #360 (issuecomment-5520101393, -5520128430). Codex fix360 lanciato in ~/.herdr/worktrees/sirio/fix-360 (w25:p1) con contratto che copre errore non mostrato + azioni scartate con git_task occupato.
- Il pane Changes si aggiorna da solo quando lo stato git cambia dall'esterno (unstage via sirioctl → CONTEXT.md torna in Changed) — F4 ok (2ª evidenza dopo il marker "• Diff" nel file tree).
- Screenshot a piena risoluzione con `screencapture -x` senza shot.sh: coordinate ×0.855 per tornare ai punti schermo.
- Stato worktree ddb1a22 a fine F1: CONTEXT.md modificato (preesistente + mio edit? no: il +22 è la modifica preesistente del worktree), qa-f1-new.txt untracked (da rimuovere a fine sezione F). README e LICENSE ripristinati.

## 06:00–06:04 — F2/F3/F4

- F3: la freccia ↗ nella riga del tab Changes apre il file (CONTEXT.md renderizzato come markdown) in un tab del gruppo destro; il gruppo destro ora ha Changes + CONTEXT.md (chevron). Per design (changes.rs:128-136): il tab Changes È la vista diff; "Open diff" vive solo nel pannello destro. F3 ok.
- F2: pannello destro → icona "±" mostra la stessa vista "Local changes (2)" con toolbar identica e gruppi Changed/Untracked. Espandendo una riga nel pannello (largo ~300 px) compaiono Discard / Stage / "Open di…" e il NOME DEL FILE SPARISCE dalla riga (F2b): perdita di informazione in un pannello stretto, da segnalare come minor (stessa famiglia di #361: layout che non gestisce lo spazio).
- Build in background (bv8cymp4l) non ha prodotto output e il binario è ancora delle 05:34: rifare il build con Scripts/build-dev.sh (che rilancia l'app) al termine di F2.

## 06:03–06:06 — rebuild 73d4fc13 e relaunch

- F2c: "Open diff" dal pannello destro porta in primo piano il tab Changes del centro con la stessa riga espansa. F2 ok.
- Build 73d4fc13 (fix #356 5c583b15, #358 4187a6fc, #359 73d4fc13) con `Scripts/build-dev.sh`; i 3 test di regressione passano (bv8cymp4l). App rilanciata 06:03:37, `pong`. TCC "Consenti" gestito 2 volte. Log: `[session] tab "CONTEXT.md" (file) is not restorable in this build; skipped` → i tab file non vengono ripristinati (osservazione).
- `list-workspaces` dopo il relaunch: id di nuovo univoci wt-0…wt-10 (ricostruiti per indice). I duplicati visti alle 05:24 erano dovuti al refresh a caldo (#355) con nuovi worktree esterni: da ricontrollare al retest #356.
- Il worktree ddb1a22 è tornato allo stato di partenza (solo CONTEXT.md modificato, preesistente).

## 06:08–06:12 — #362 scoperta (id worktree posizionali)
- Click destinato alla riga "Claude Code" (167,376) ha preso `fix/358` (sidebar riordinata all'attivazione). Sotto `fix/358` compaiono i tab di ddb1a22: Terminal con scrollback e prompt "detached at ddb1a22", chat Claude/Codex/OpenCode/Pi/Chat legacy; status bar e file tree dicono fix/358. `list-workspaces`: solo fix/358 montato; `panel list`: pane delle chat + Changes con cwd fix-358.
- Riselezionando ddb1a22: stessi 8 tab, stesso Terminal; la riga fix/358 torna senza figli. Il set di tab segue la selezione.
- DB: `wt-3` ha 9 tab, `wt-8` (path ddb1a22) solo CONTEXT.md. Causa: `worktree_id(project, index)` = posizione in `git worktree list`; i worktree herdr fix-35x creati con l'app chiusa hanno spostato gli indici. `sync_control_state` conserva gli id per path → duplicati a runtime.
- Issue #362 (major) con dump DB su qa-evidence. Corretto poi il paragrafo "Osservato" (avevo scritto un dettaglio dedotto, non visto).
- Screenshot: status bar di nuovo collassata ("⚙ ⋮ Claude … Codex …", 3ª osservazione) → `L-statusbar-collapsed-3rd.png`.

## 06:14–06:17 — dispatch fix362, retest #358
- Codex `fix362` in `~/.herdr/worktrees/sirio/fix-362` (w26:p1), prompt contratto `prompt-362.txt` (id stabili per path + migrazione DB).
- Retest #358 su 73d4fc13: tab Terminal di ddb1a22 → `+` → Split Claude Code. Tab tiene titolo "Termi…" e icona terminale, glifo "?" (Claude al prompt), nessun "? exit 0"; riga sidebar "Terminal" con icona terminale. `panel list`: pane-0 Terminal agente "", pane-10 Terminal agente claude. Come atteso → 1/1, manca il caso tab Pi uscito.

## 06:17–06:19 — retest #359 su 73d4fc13
- Chat Claude Code di ddb1a22 (pane-4): prompt "Reply with exactly the word ok…". Mentre lavora (tab "Claude Co… ●", composer "working"), il tab legacy "Chat" resta "○" in due crop successivi del tab strip e la riga sidebar "Chat" non ha pallino. Prima del fix mostrava "●". → 1/1 ok; seconda osservazione nel turno successivo.
- fix/360 (2537102a, solo changes.rs: coda `pending_operations` dietro `git_task`, errore mutazione → `git_error`, 3 test) squash-mergiato su main come 9094475a. Rebuild dopo il retest di #356.

## 06:21–06:24 — retest #356 su 73d4fc13 (parte 1)
- New Worktree… → `qa-356-retest` + Enter: riga creata e **selezionata subito** (status bar `qa-356-retest · ~/Desktop/Progetti/sirio-qa-356-retest`, file tree del nuovo worktree), `list-workspaces` la elenca montata → la parte "riga non selezionabile / assente in list-workspaces" è corretta.
- Ma sotto la nuova riga compaiono gli 8 tab di ddb1a22 (chat Claude al centro); riselezionando ddb1a22 tornano sotto ddb1a22 e qa-356-retest resta senza figli; riselezionando qa-356-retest tornano lì. Id duplicati in `list-workspaces`: wt-9 (qa-356-retest, fix/next), wt-6 (fix/362, green-meadow). → variante a runtime di #362, commentata sull'issue con screenshot e passata a codex fix362.
- Status bar collassata ("⚙ ⋮ Claude … Codex …") anche qui, 4ª osservazione: compare quando il path in status bar è lungo? (da verificare nel report).

## 06:24–06:27 — retest #356 (parte 2), chiusure
- × su qa-356-retest → dialog "Remove worktree? This permanently deletes…" → Remove Worktree: riga sparita, nessun highlight stantio, selezione su main (3 Terminal), `list-workspaces` senza qa-356 (mounted = main), worktree+branch+dir rimossi. I pannelli passano da 10 a 4: i tab di ddb1a22 mostrati sotto il worktree rimosso sono stati chiusi (conseguenza di #362; DB: wt-8 ddb1a22 = 0 tab).
- Chiusi "verified": #356, #358, #359. Checklist: B11, D6, E10 → ok. Rimossi worktree herdr w22/w23/w24 e branch fix/356, fix/358, fix/359.
- Rebuild + rilancio su 9094475a (fix/360) per il retest di F1.

## 06:28–06:33 — rilancio su 9094475a, retest #360 (F1) FALLITO, incidente `claude`
- Dopo il rilancio il tab Claude ripristinato sotto main mostra `zsh:1: command not found: claude`, "Process exited with status 127", status bar "Claude not found". Causa ambiente, non Sirio: `~/.local/bin/claude` puntava a `~/.herdr/worktrees/sirio/fix-358/rust/target/fix358-xdg/claude/versions/2.1.259` (l'auto-update di Claude Code lanciato dentro il worktree di codex con XDG redirezionato ha rimesso il symlink lì; rimosso il worktree, il binario è sparito). Ripristinato: `ln -sfn ~/.local/share/claude/versions/2.1.259 ~/.local/bin/claude` → `claude --version` = 2.1.259. Da riportare nel report (rischio dei test che lanciano il vero `claude`).
- Retest #360 su ddb1a22 (CONTEXT.md modificato), `touch .git/worktrees/sirio/index.lock`, `+` → Changes, riga espansa, click Stage (2 volte): nessun errore visibile in 6 frame (0–3 s) né dopo, titolo resta "Local changes (1)", file resta in Changed, lock intatto. Stesso sintomo dell'issue → revert del merge 9094475a, commento su #360, codex fix360 rilanciato nello stesso worktree con contesto.
- #361 visibile anche qui: header `@@ -18,24 +18,46 @@ This is what the control socket's` sovrapposto a "24 hidden lines".

## 06:44–06:47 — fix/361 integrato, G1
- Codex fix361 done: 9fda962a (solo changes.rs: header hunk `flex_1 + min_w(0) + truncate()`, selettore `changes-hunk-header`, test drawn a 330 px; 575 test verdi). Il branch era basato su 9094475a (conteneva anche fix/360, poi revertito) → integrato con `git cherry-pick -x` del solo commit, non squash: main = 6eb124ac (dopo amend del soggetto con "(#361)").
- G1 Files: chevron su `docs` espande (sottocartelle + file, icone md), click singolo su README.md evidenzia solo la riga: nessuna preview. Provo il doppio click.
- Status bar: dopo la riparazione del symlink `claude` il badge è passato "Claude not found" → "Claude logged out" → "Claude 45% 5h" in ~15 min: dipende dall'ambiente, non lo classifico.

## 06:47–06:52 — rilancio su 57e33b80, fix/362 pronto
- Doppio click su README.md nel pannello Files apre un tab file "README.md" nel gruppo di destra con breadcrumb del path e il **sorgente** markdown (HTML `<p align="center">` visibile, non renderizzato); il tab Changes non era più visibile nel gruppo (chevron overflow presente). Da approfondire in G2/G4.
- Rilancio su 57e33b80 (revert #360 + fix #361): ddb1a22 selezionato con 3 Terminal; il tab Claude (split) ora parte (symlink riparato). Tab file e Changes non ripristinati (`[session] tab "CONTEXT.md" (file) is not restorable in this build; skipped`, e nessuna riga per Changes).
- Primo tentativo `+` → Changes andato a vuoto: il click su (789,100) ha selezionato il terzo tab (il `+` in questo layout è coperto/spostato dal tab strip a 3 tab). Riprovo con screenshot del menu.
- Codex fix362 done: 5d69cc2f — `stable_worktree_id(project_id, path)` (hash FNV del path canonico), migrazione schema v17 con id temporanei per le FK (worktree, tab.worktree_id, sidebar_state.selected_worktree_id), mappa tab→path a runtime; 4 test nuovi verdi, `cargo test -p sirio` 333 ok / 7 rossi (6 baseline + `render_never_captures_scrollback` verde in isolamento). Squash-merge su main.

## 06:52–06:55 — menu `+`: voce "Changes" senza effetto (3/3 con tab Claude attivo)
- Con il tab "Termi… ?" (Claude split, vivo) attivo su ddb1a22: `+` apre il menu (screenshot), click su "Changes" → il menu si chiude e non compare nessun tab (3 tentativi consecutivi, 1–1,5 s fra apertura e click). Alle 06:29 (V360c) la stessa sequenza con il tab "Termi… !" (Claude uscito) attivo aveva funzionato. Provo con il primo Terminal (shell) attivo per isolare la variabile.
- Integrazione fix/362: main 95cd783a; `cargo build -p sirio` ok; `cargo test -p sirio_persistence` 62 ok; 6 test di regressione mirati verdi (46,9 s).

## 06:55–06:58 — menu `+`, seconda serie; fix/360 v2
- Click sul tab 2 nel tab strip → attivo (funziona); click sulla riga sidebar "Terminal" → tab 1 attivo (funziona). Quindi il tab strip risponde; a non rispondere è la voce "Changes" del menu (menu aperto, click → si chiude senza effetto) quando è attivo il tab split Claude. Verifico ora con il tab 1 (shell) attivo.
- Codex fix360 v2 done: 40f707ca "retain mutation errors across changes refreshes" (changes.rs +138/−12): l'errore di mutazione sopravvive ai refresh periodici finché Retry o una mutazione riuscita; test drawn sul percorso `+` → Changes con lock e refresh periodico (rosso prima/verde dopo), 575 verdi. Squash dei due commit su main.

## 06:56–06:59 — tab Changes ripristinato ma invisibile (build 57e33b80)
- `panel list`: `pane-4 Changes Changes  true` esiste ed è *attivo* dopo `+` → Changes, ma nel tab strip (3 tab, nessun chevron visibile) non compare e il contenuto resta quello del tab split Claude; sidebar evidenzia "Terminal" 1 mentre il centro mostra il tab 3 → strip/contenuto/sidebar/control state incoerenti. DB: `wt-8-tab-…-3|diff|Changes` (persistito, ripristinato: nel log solo i tab file sono "not restorable").
- `+` → New Terminal funziona (pane-5, attivo, visibile); ora il strip mostra 2 tab + chevron overflow (763,101). Sidebar sotto ddb1a22: 4 righe Terminal, con **due** righe evidenziate (la 1ª e la 4ª).
- Ipotesi da verificare al prossimo rilancio: il tab Changes ripristinato finisce fuori dal tab strip e "rivelarlo" da `+` → Changes non lo porta a video. Se si ripete (2/2) → issue.

## 07:00–07:03 — il tab Changes era nel pane Secondary chiuso
- Il menu overflow del primary pane elenca 4 "Terminal" e nessun Changes; `secondary_pane_open` = 0 per tutti i worktree. **ctrl-shift-b** (ToggleSecondaryPane) apre il pane Secondary con il tab "Chang…" ("Local changes (1)", CONTEXT.md) → il tab ripristinato stava nel Secondary chiuso; `+` → Changes lo attivava (pane-4 `true`) senza aprire il pane. Esc non chiude il menu overflow (resta aperto anche dopo il toggle).
- Ora provo la ripro pulita senza rilancio: chiudo il Secondary con ctrl-shift-b (tab conservati) e rifaccio `+` → Changes.

## 07:02–07:05 — ripro pulita "+ → Changes con Secondary chiuso" 2/2
- Stato: ddb1a22 con tab Changes nel Secondary; ctrl-shift-b chiude il Secondary (`secondary_pane_open` 0). `+` → Changes: il Secondary resta chiuso, nessun Changes a video, il primary strip non evidenzia nessun tab attivo, sidebar evidenzia "Terminal" 1; `panel list` → `pane-4 Changes … true`. Ripetuto dopo toggle apri/chiudi: identico (2/2). Prima osservazione: subito dopo il rilancio 57e33b80 (Secondary ripristinato chiuso) → 4 tentativi. → issue.
- Issue **#363** (major) aperta con 3 screenshot su qa-evidence; checklist F5 → bug. Codex `fix363` in ~/.herdr/worktrees/sirio/fix-363 (prompt-363.txt: aprire il Secondary quando un suo tab diventa attivo, tutti i percorsi + restore).

## 07:07 — G2 apertura file
- Doppio click su `Scripts/build-dev.sh` nel pannello Files: il Secondary si apre (percorso "nuovo tab" ok, a differenza di #363) con tab "build-dev.sh", breadcrumb del path, numeri di riga, highlight shell (keyword/stringhe colorate). Il tab Changes resta sotto il chevron overflow del Secondary. Provo scroll (PageDown) ed editing su un file scratch.
- G2: rotella (helper Swift `/tmp/qa-shots/scroll`, 4×10 righe) scorre l'editor fluidamente (righe 16–52); PageDown ×3 e freccia giù ×45 non muovono né caret né vista (caret resta a riga 16): niente navigazione da tastiera nell'editor (da classificare dopo G3). Doppio click su `qa-scratch.txt` (untracked, "• Diff" grigio nel tree) → apre nel Secondary al posto del tab visibile, strip Secondary mostra un solo tab + chevron.

## 07:10–07:14 — G3 editing su qa-scratch.txt
- Digitazione ok, pallino "●" di modificato sul tab. **Click a destra della fine riga non sposta il caret** (2/2: " edited" e "X" inseriti alla posizione precedente del caret, riga 1), click dentro il testo sì ("lineY two").
- **cmd+s non salva** (disco invariato, pallino resta); la scorciatoia è `ctrl-s` (tabella `linux_window_shortcuts()` usata su tutte le piattaforme, come già notato in #324) e ctrl-s salva davvero (disco aggiornato). Su macOS, piattaforma di riferimento, cmd+s è l'atteso: stesso genere di #351 (Cmd+V). Seconda osservazione in corso.
- Frecce/PageDown: verifica in corso su file corto.
- G3 (segue): con il caret piazzato nel testo, freccia destra ×2 sposta il caret ("lineAY Btwo") → le frecce funzionano; i fallimenti precedenti erano di focus (dopo ctrl-s l'editor perde il focus: frecce + "Z" non hanno prodotto nulla). Resta: click a destra della fine riga non muove il caret (2/2, minor), cmd+s non salva su macOS (2/2, minor; ctrl-s sì).
- 07:19 rilancio su ac7a2c04 (revert #360 v1 + #361 + #362 + #360 v2): migrazione v17 eseguita, id worktree ora `wt-<hash del path>` (es. ddb1a22 → wt-990bbdd140b3e5b8).

## 07:21–07:27 — retest #362 su ac7a2c04 (parte 1)
- Test sirio_ui su main NON compilavano (E0063: `git_error_from_mutation` mancante nel literal `ChangesTab` del test di #361, campo aggiunto da #360 v2): corretto con 4d359d07 (test-only), suite rilanciata.
- `list-workspaces`: id `wt-<hash>` tutti univoci; ddb1a22 (4 Terminal + Changes) e main mantengono i propri tab dopo il rilancio.
- New Worktree… `qa-362-retest`: la riga nuova è selezionata e **vuota** ("No Terminals"), ddb1a22 riselezionato ha ancora i suoi 4 Terminal; `panel list` 6 pane. × → Remove: ddb1a22 intatto (DB 5 tab), nessun id duplicato.
- Osservazioni: durante la vita di qa-362-retest il DB riportava `branch = main` per la riga di ddb1a22 (fallback `is_primary → "main"` in session.rs ~794), tornata "ddb1a22" dopo la rimozione; la UI mostrava sempre ddb1a22. Dopo la rimozione la sidebar evidenzia due righe (ddb1a22 selezionata + fix/next): 2ª osservazione del doppio highlight (1ª: M5, due righe Terminal).
- `git worktree add ../sirio-qa-362-ext -b qa-362-ext` con app aperta: nessun id duplicato, tab di ddb1a22 invariati; la riga nuova compare in `list-workspaces` solo dopo il refresh del catalogo (verifica in corso).

## 07:37–07:48 — retest #362 su ac7a2c04 (parte 2) → chiusa verified
- `cargo test -p sirio_ui` su main 4d359d07: 576 passed (3 suite).
- Relaunch (build-dev) con `qa-362-ext` presente: `list-workspaces` senza duplicati, solo ddb1a22 mounted; `panel list` 5 Terminal + Changes; DB `40b3e5b8|ddb1a22|5` (4 terminal + diff Changes), sidebar 4 righe Terminal (una con ✳ claude), prompt "detached at ddb1a22" → `V362i-relaunch-with-ext.png`.
- `git worktree remove ../sirio-qa-362-ext` + `branch -D` mentre l'app gira: la riga resta finché non cambia il focus (Finder → Sirio) → poi `rows=11`, `ext_rows=0`; tab e DB invariati. Il catalogo si aggiorna al focus regain, non in tempo reale (già visto in aggiunta).
- Chiusa #362 "verified" (`close-362.md`). N2 → ok.
- **L2**: il ↻ in status bar (97,985) è il refresh usage: al click i badge diventano "⚙ ⋮ Claude … Codex …" (= la "status bar collassata" delle 4 osservazioni precedenti: stato di caricamento del refresh periodico), dopo ~10 s "Claude 7% 5h · 31% wk" (era 6%). Non è un bug; nit: il placeholder "…" fa sparire i dati per qualche secondo. L2 → ok. → `L2b-after-refresh.png`, `L2c-statusbar.png`.
- **pane-5**: `panel list` mostra 6 pane per ddb1a22 (5 Terminal + Changes) ma DB e sidebar ne hanno 4 Terminal; `panel state pane-5`: scrollback vuoto, cwd ddb1a22, running. Pane senza tab visibile dopo il relaunch (il log dice "tab build-dev.sh/qa-scratch.txt (file) not restorable; skipped"). Da riosservare al prossimo relaunch prima di classificare.
- File tab (qa-scratch.txt, build-dev.sh) non ripristinati al relaunch: "not restorable in this build" (già in elenco osservazioni).
- Sidebar: dopo il relaunch la riga alla y≈531 (prima fix/362, poi fix/next dopo lo shift delle righe) resta evidenziata oltre alla selezione ddb1a22 senza mouse sopra → test hover HV1–HV4 (vedi sotto).
