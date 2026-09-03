# QA report — Sirio (Windows, 2026-09-03)

Stato: **in corso** — aggiornato a fine sessione. La checklist operativa con lo stato di ogni voce è in
`qa/checklist.md`; screenshot e log di evidenza in `qa/screenshots/` e `qa/logs/`.

## Ambiente reale vs. piano

Il piano prevedeva un Mac con computer-use dentro herdr. La sessione è girata invece su:

- **Windows 11 Pro (10.0.28000)**, due monitor (primario 3440×1440 @100%, secondario 2560×1440 a sinistra).
- Sessione Claude Code dentro un pane herdr 0.8.2 (`w2:p1`), repo `D:\Progetti\sirio\sirio` su `main`,
  shell **elevate** (Administrator).
- **Computer-use**: nessun tool nativo disponibile; sostituito da uno script PowerShell
  (`SendInput` per mouse/tastiera, `Graphics.CopyFromScreen` per gli screenshot, letti come immagini).
  Tutto ciò che è riportato come osservato è stato visto in uno screenshot; dove non è stato possibile
  osservare, è indicato esplicitamente. Tre trappole dell'harness (input unicode `VK_PACKET` ignorato da
  gpui, tasti speciali che richiedono scan code, `$parts[-1]` di PowerShell) hanno prodotto falsi
  "input persi" prima di essere riconosciute; nessuno di questi è stato attribuito a Sirio.
- **Toolchain**: all'avvio non esistevano più né rustup/cargo, né MSVC, né Windows SDK (presenti il 29/08).
  Reinstallati: rustup `stable-x86_64-pc-windows-msvc` (rustc 1.98.1), VS 2022 Build Tools (VCTools,
  MSVC 14.44), sccache 0.17.0 prebuilt; Zig 0.15.2 già in `D:\toolchains\zig`. Prima build 4m39s.
- Il binario preesistente (29/08) andava in panic all'avvio (`Error creating DirectWriteTextSystem`):
  la build debug di gpui compila gli shader HLSL a runtime dal path del registry cargo cancellato.
  Artefatto ambientale, non bug di Sirio.
- **Instabilità di sistema**: alle 20:54–21:07 sul PC girava un installer driver ASUS/AMD da `G:\BIN`
  (crash WER di `Instv2.exe`, `AsusSetup.exe`, `Explorer.EXE` in `amdxx64.dll`). Il primo avvio di Sirio
  è caduto lì: le anomalie viste allora (dialog `cmd.exe 0xc0000142`, uscita silenziosa) non si sono
  più ripresentate dopo la fine dell'installazione e non sono state classificate.
- Il PATH utente conteneva una voce malformata (`"\n   C:\Users\enzop\.local\bin"`, scritta dall'installer
  di Claude Code): è la causa di #365 ed è stata normalizzata dopo la riproduzione.
- Agenti di fix: **pi** (`opencode-go/gpt-5.6-luna`, thinking xhigh) e **opencode-go**
  (`opencode-go/muse-spark-1.3-contributor`, reasoningEffort xhigh), lanciati in worktree herdr
  `fix/<n>` (`pane run pi.cmd/opencode.cmd`: `herdr agent start` non gestisce gli shim npm su Windows).

## Funzionalità coperte

Tutte le 30 voci della checklist sono state esercitate (vedi `qa/checklist.md`): avvio e persistenza,
sidebar (progetti, filtro, worktree, New/Remove Worktree, menu contestuale), titlebar e finestra
(max/restore/minimizza/resize/doppio click), tab e pane (Ctrl+T, ×, split, Close Pane, palette),
terminale (output 30k–40k righe, scroll, incolla, 3 worktree in parallelo), harness Claude Code e Pi
(Codex e Oh-My-Pi non installati, correttamente segnalati), activity indicator, chat ACP, editor
Markdown (preview, code, salvataggio), pannello destro (Files, tab list, Changes con
stage/unstage/discard, Git log con ricerca), browser tab, Settings (tutte le sezioni, tema, font,
Agents), status bar, `sirioctl`, notifiche.

## Issue aperte durante la sessione

| Issue | Titolo (sintesi) | Severità | Agente | Tentativi | Stato |
|-------|------------------|----------|--------|-----------|-------|
| #364 | Windows: finestra console extra accanto all'app | major | opencode-go | 2 | chiusa, verificata (tent. 1 revertito: console `git.exe` lampeggianti) |
| #365 | Windows: una voce PATH non sondabile svuota il registry agenti | major | pi | 1 | chiusa, verificata |
| #366 | Git log: colonna Date troncata e disallineata | minor | opencode-go | 1 | chiusa, verificata |
| #367 | Diff: numeri di riga a 5 cifre sovrapposti | minor | pi | 1 | chiusa, verificata |
| #368 | Windows: New Browser abortisce l'app (`RefCell already borrowed`, anche al restore) | blocker | opencode-go | 1 | chiusa, verificata |
| #369 | Browser: barra indirizzi senza input, campo URL 60 px | major | pi → opencode-go | 4 | aperta: tent. 1 (click) mergiato; tent. 2 (pi) e 3 (opencode-go) revertiti; tent. 4 in corso |
| #370 | Windows: `sirioctl` non raggiunge l'app (pipe da HOME, owner elevato) | major | opencode-go | 1 | chiusa, verificata (caso residuo: hook di Claude Code con Sirio elevato) |
| #371 | Font Interface inerte | minor | pi | 1 | chiusa, verificata |
| #372 | Remove Worktree senza nome del bersaglio, anche sul primario | major | opencode-go | 1 | chiusa, verificata |
| #373 | Editor Markdown riscrive il file al salvataggio | major | pi | 1 | chiusa, verificata |
| #374 | Ctrl+Shift+S / Ctrl+Shift+I inerti (hotkey globali di un altro programma) | minor | opencode-go | 1 | chiusa, verificata (chord Windows ritargate a Ctrl+Shift+D/R/H — da confermare dal maintainer) |
| #375 | Command palette sfora sotto il pannello | minor | pi | 1 | chiusa, verificata |
| #376 | Menu popup disegnati dietro la webview | major | opencode-go | 1 | chiusa, verificata |
| #377 | New Chat › OpenCode: ACP "program not found" (shim .cmd) | major | pi | 2 | chiusa, verificata (tent. 1 revertito: nome nudo non risolto) |

## Fix mergiate per agente

| Agente | Issue (tentativi) |
|--------|-------------------|
| pi (gpt-5.6-luna) | #365 (1), #367 (1), #371 (1), #373 (1), #375 (1), #377 (2: tent. 1 revertito), #369 tent. 1 parziale (mergiato) e tent. 2 (revertito) |
| opencode-go (muse-spark-1.3-contributor) | #364 (2: tent. 1 revertito), #366 (1), #368 (1), #370 (1), #372 (1), #374 (1), #376 (1), #369 tent. 3 (revertito) |

Ogni fix è stata squash-mergiata su `main`, ricompilata e ritestata ripetendo gli step della issue
prima della chiusura; i worktree dei fix chiusi sono stati rimossi (alcune cartelle vuote
`fix-365/369/371/373` restano bloccate da un handle esterno e vanno cancellate a mano).

Deviazione dichiarata: per #369 il tentativo 1 (percorso via click) **non** è stato revertito quando il
caso residuo (Ctrl+L) è stato riscontrato, perché il revert avrebbe reintrodotto lo stato peggiore
(nessun input possibile); il tentativo 2 è stato revertito e la issue passata all'altro agente.

## Problemi notati ma non classificati come bug

- Log all'avvio pre-#370: `[control] listening on /tmp\.local/state\Sirio\control.sock` (path POSIX su
  Windows); risolto dalla fix #370 (`%LOCALAPPDATA%\Sirio\control.sock`).
- Tab Changes ripristinata nascosta nel pane secondario che mostrava un diff stantio (stato staged di
  un commit precedente) e non si aggiornava con Refresh: osservato una volta al primo avvio, non
  riprodotto con avvii successivi.
- Ordine delle tab cambiato una volta dopo un cambio di worktree (Chat|Terminal → Terminal|Chat):
  osservato una volta.
- Popover «New worktree»: una volta il primo campo, pur con focus, non ha ricevuto testo; non riprodotto.
- Git log: a ~660 px di larghezza un soggetto va a capo su due righe invece di troncarsi con «…».
- Status bar: i tooltip ripetono l'etichetta («Claude not supported here») senza spiegare; Settings dice
  invece «Signed in» per Claude Code.
- Settings › Agents: colonna dei contenuti a larghezza fissa, path troncati anche da sinistra
  («ata\Roaming\npm\opencode.cmd»); menu `+`: etichette troncate senza ellissi quando c'è il badge
  «Not found on PATH» («Claude Cc», «Split Clau»); menu contestuale: motivazione «The primary worktre…»
  troncata; dialog Remove Worktree mostra il path con prefisso `\\?\`.
- Editor Markdown in pane stretto (~420 px): il pulsante «Code» dello switch Preview|Code esce dal bordo
  destro ed è irraggiungibile finché non si allarga il pane.
- `Esc` non chiude Settings (serve la freccia «‹») né il menu contestuale della sidebar.
- `Ctrl+W` non chiude una tab terminale: scelta deliberata (#226, binding `!Terminal`), ma il README lo
  elenca senza eccezioni.
- Titlebar «+» e «↻» senza tooltip né effetto visibile con history vuota (già #299).
- Terminale: dopo split e chiusura del pane restano due barre blu (residui di selezione) nel pane vuoto.
- Doppio click sulla titlebar: massimizza **e** ripristina correttamente (#288 non riprodotta).
- Avvio con un browser tab persistito prima di #368: crash in ~1 avvio su 2 (race), ora risolto.
- Con Sirio elevato gli hook `sirioctl notify` di Claude Code falliscono per owner diverso (caso
  cross-elevation escluso dalla fix #370): l'indicatore di stato dell'agente resta «?» e non arrivano
  notifiche di fine turno. Non verificato con Sirio non elevato.
- Multi-monitor/DPI: non testato (secondario non usato).
