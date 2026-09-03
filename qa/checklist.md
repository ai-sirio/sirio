# QA checklist — Sirio (Windows build, main)

Stato: `todo` / `ok` / `bug` (con numero issue e agente assegnato). Ambiente reale: Windows 11,
build debug `rust/target/debug/sirio.exe` da `main`, computer-use via screenshot + SendInput
(vedi `qa/report.md` per le differenze rispetto al piano "Mac + herdr computer-use").

| # | Area | Funzionalità | Stato | Issue | Agente | Note |
|---|------|--------------|-------|-------|--------|------|
| 1 | Avvio | Avvio app, finestra principale, titlebar custom | ok | #364 (chiusa, 2 tentativi) | opencode-go | nessuna console extra né console lampeggianti (2 avvii da Explorer); log su stderr da terminale |
| 2 | Avvio | Stato vuoto "Add a project, then select a worktree" | ok |  |  | stato vuoto + spinner; dialog "not a git repository" (Initialize/Add without Git/Cancel) |
| 3 | Sidebar | Aggiungi progetto (`+`), elenco worktree per progetto | ok |  |  | menu Open/Clone/Create Project, picker nativo, worktree elencati |
| 4 | Sidebar | Filtro progetti/worktree | ok |  |  | filtro per sottostringa ok, ripristino lista ok |
| 5 | Sidebar | Selezione worktree, cambio worktree rapido, stato `mounted` | ok | #372 (chiusa) | opencode-go | switch, catalogo live, New Worktree, Remove Worktree con dialog nominativo e primario protetto |
| 6 | Sidebar | Toggle sidebar (icona titlebar, `Ctrl+Shift+S`) | bug | #374 | opencode-go | toggle da titlebar/palette ok; Ctrl+Shift+S inerte |
| 7 | Titlebar | Indietro / avanti (history), `+` nuova tab | ok |  |  | ← → senza effetto visibile con history vuota; + titlebar senza effetto visibile e senza tooltip (#299) |
| 8 | Titlebar | Caption buttons Windows (min/max/close), doppio click, resize, snap | ok |  |  | max/restore da caption e doppio click (#288 non riprodotta), minimizza/ripristina, resize da bordo |
| 9 | Tabs | Nuova tab terminale (`Ctrl+T`), chiusura (`Ctrl+W`), riordino, overflow con molte tab | ok |  |  | Ctrl+T ok; Ctrl+W chiude solo tab non-terminale (by design #226, README da allineare); × con conferma "Close dirty tab?" |
| 10 | Terminale | Shell di default, input, output massiccio (`type` file grande), scroll | ok |  |  | cmd.exe; 30k righe senza freeze, input accodato, scroll ok, ctrl+shift+v incolla |
| 11 | Terminale | Split ricorsivi, resize dei pane, focus | ok |  |  | split destra/giù (Ctrl+Alt+Shift+frecce), Close Pane con conferma; focus tra pane da palette |
| 12 | Terminale | Harness: Claude Code | ok |  |  | Claude Code parte dal menu + e risponde; hook sirioctl falliscono con Sirio elevato (nota su #370) |
| 13 | Terminale | Harness: Codex | ok |  |  | n/a: Codex non installato; il menu lo segnala "Not found on PATH" |
| 14 | Terminale | Harness: OpenCode | ok |  |  | lancio interattivo dal menu + non testato a fondo (TUI parte via PTY); ACP coperto da #377 |
| 15 | Terminale | Harness: Pi | ok |  |  | Pi lanciato dal menu contestuale, risponde; stato tab resta "?" per #370 |
| 16 | Terminale | Harness: Oh-My-Pi | ok |  |  | n/a: Oh-My-Pi non installato; segnalato "Not found on PATH" |
| 17 | Activity | Stato running/idle/needs-input per pane (dot nella tab, sidebar, tray) | ok | #370 (chiusa) | opencode-go | sirioctl raggiunge l'app (elevato↔elevato, non-elevato↔non-elevato); stato agente da riverificare con hook |
| 18 | Chat | Tab Chat (ACP): invio prompt lungo, streaming, markdown, permessi | bug | #377 | pi | New Chat › OpenCode: ACP "program not found" (shim .cmd); chat legacy mostra banner agente sconosciuto |
| 19 | Editor/Markdown | Apri file `.md` (`Ctrl+O`, drag&drop, link nel terminale), preview/code mode, live reload, salva (`Ctrl+S`) | ok | #373 (chiusa) | pi | apertura/preview/live reload ok; Code edita il sorgente raw e Ctrl+S salva solo le modifiche; pulsante Code clippato in pane stretto (nota) |
| 20 | Right panel | Toggle (`Ctrl+Shift+I`), tab Files | ok |  |  | toggle da titlebar ok; Ctrl+Shift+I non registrato su Windows (vedi note); Files aggiornato live |
| 21 | Right panel | Tab Changes: stage/unstage/discard, apri diff tab | ok | #367 (chiusa) | pi | gutter 5 cifre corretto; stage/unstage/discard (con conferma) ok |
| 22 | Right panel | Tab Git (grafo/branch) e altre icone del pannello | ok | #366 (chiusa) | opencode-go | colonna Date corretta; ricerca testo ok; filtri Branch/User/Date/Paths non esercitati |
| 23 | Browser | Tab browser embedded | ok | #368 (chiusa), #369 | opencode-go / pi | crash risolto (2/2 + 2 restore); barra indirizzi in retest |
| 24 | Settings | Apertura, modifica (tema, shell, socket, provider), applicazione, ripristino | ok | #365, #371 (chiuse) | pi | tema, font terminale e font Interface si applicano (live e al riavvio) e si ripristinano; Agents ok |
| 25 | Status bar | Indicatori provider usage (Claude/Codex/...), refresh | ok |  |  | indicatori Claude/Codex presenti; tooltip = etichetta (vedi note) |
| 26 | Controllo | `sirioctl` / control socket | ok | #370 (chiusa) | opencode-go | ping/version/list-workspaces ok da PowerShell e Git Bash |
| 27 | Notifiche/Tray | Notifica fine agente, icona tray | ok |  |  | nessuna notifica desktop osservata alla fine del turno (Layer A spento con Sirio elevato); tray non presente su Windows (#99 scope macOS) |
| 28 | Persistenza | Riavvio app: tab, worktree, sessioni ripristinate | ok |  |  | progetto, tab e pane secondario ripristinati (2 riavvii) |
| 29 | Finestra | Resize continuo, minimizza/ripristina, multi-monitor/DPI | ok |  |  | resize continuo e massimizzato: layout coerente; multi-monitor non testato (secondario a sinistra, non usato) |
| 30 | Stress | Molti terminali aperti insieme + output pesante + cambio worktree | ok |  |  | 3 loop da 40k righe in 3 worktree: nessun freeze, switch ~180 ms, loop nascosti completati; throughput condiviso ~240 righe/s per terminale; RSS 105 MB |

## Assegnazione issue → agente

| Issue | Agente | Worktree | Tentativi | Esito |
|-------|--------|----------|-----------|-------|
| #364 console Windows | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-364` (w4 e wE rimossi) | 2 | tent. 1 mergiato (a4f4bbef) e **revertito** (fa638c0b, console `git.exe` lampeggianti); tent. 2 verificato e chiuso (contenuto in 08bd201a) |
| #365 registry agenti / PATH | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-365` (herdr w5, rimosso) | 1 | verificata e chiusa (ae816097) |
| #366 git log colonna Date | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-366` (herdr w6, rimosso) | 1 | verificata e chiusa (bc846cb1) |
| #367 gutter diff 5 cifre | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-367` (herdr w7, rimosso) | 1 | verificata e chiusa (df0ba45f) |
| #368 crash New Browser (RefCell) | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-368` (herdr w8, rimosso) | 1 | verificata e chiusa (6525bc00) |
| #369 barra indirizzi browser | pi → opencode-go | `~/.herdr/worktrees/sirio/fix-369` (herdr w9) | 3 | pi: tent. 1 mergiato (click ok), tent. 2 revertito (e3bc7d94, Ctrl+L peggiorato); passata a opencode-go, tent. 3 in corso |
| #370 sirioctl pipe/owner Windows | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-370` (herdr wA, rimosso) | 1 | verificata e chiusa (33e96f85) |
| #371 font Interface inerte | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-371` (herdr wB, rimosso) | 1 | verificata e chiusa (5b3bfb28) |
| #372 dialog Remove Worktree | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-372` (herdr wC, rimosso) | 1 | verificata e chiusa (901f0c2e) |
| #373 editor Markdown riscrive il file | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-373` (herdr wD, rimosso) | 1 | verificata e chiusa (b3d48c2b) |
| #374 Ctrl+Shift+S/I inerti | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-374` (herdr wF) | 1 | in corso |
| #375 overflow command palette | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-375` (herdr wG) | 1 | in corso |
| #376 menu popup dietro la webview | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-376` (herdr wH) | 1 | in corso |
| #377 chat ACP OpenCode "program not found" | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-377` | 1 | in corso |

## Note non classificate come bug

- (vedi `qa/report.md`)
