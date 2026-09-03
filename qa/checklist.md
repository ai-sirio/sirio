# QA checklist — Sirio (Windows build, main)

Stato: `todo` / `ok` / `bug` (con numero issue e agente assegnato). Ambiente reale: Windows 11,
build debug `rust/target/debug/sirio.exe` da `main`, computer-use via screenshot + SendInput
(vedi `qa/report.md` per le differenze rispetto al piano "Mac + herdr computer-use").

| # | Area | Funzionalità | Stato | Issue | Agente | Note |
|---|------|--------------|-------|-------|--------|------|
| 1 | Avvio | Avvio app, finestra principale, titlebar custom | ok | #364 (chiusa) | opencode-go | console extra corretta; verificato 2 avvii post-merge |
| 2 | Avvio | Stato vuoto "Add a project, then select a worktree" | ok |  |  | stato vuoto + spinner; dialog "not a git repository" (Initialize/Add without Git/Cancel) |
| 3 | Sidebar | Aggiungi progetto (`+`), elenco worktree per progetto | ok |  |  | menu Open/Clone/Create Project, picker nativo, worktree elencati |
| 4 | Sidebar | Filtro progetti/worktree | todo | | | |
| 5 | Sidebar | Selezione worktree, cambio worktree rapido, stato `mounted` | bug | #372 | opencode-go | switch istantaneo e catalogo aggiornato; New Worktree ok; Remove Worktree senza nome del bersaglio |
| 6 | Sidebar | Toggle sidebar (icona titlebar, `Ctrl+Shift+S`) | ok |  |  | toggle da titlebar ok |
| 7 | Titlebar | Indietro / avanti (history), `+` nuova tab | ok |  |  | ← → senza effetto visibile con history vuota; + titlebar senza effetto visibile e senza tooltip (#299) |
| 8 | Titlebar | Caption buttons Windows (min/max/close), doppio click, resize, snap | todo | | | |
| 9 | Tabs | Nuova tab terminale (`Ctrl+T`), chiusura (`Ctrl+W`), riordino, overflow con molte tab | ok |  |  | Ctrl+T ok; Ctrl+W chiude solo tab non-terminale (by design #226, README da allineare); × con conferma "Close dirty tab?" |
| 10 | Terminale | Shell di default, input, output massiccio (`type` file grande), scroll | ok |  |  | cmd.exe; 30k righe senza freeze, input accodato, scroll ok, ctrl+shift+v incolla |
| 11 | Terminale | Split ricorsivi, resize dei pane, focus | todo | | | |
| 12 | Terminale | Harness: Claude Code | todo | | | |
| 13 | Terminale | Harness: Codex | todo | | | |
| 14 | Terminale | Harness: OpenCode | todo | | | |
| 15 | Terminale | Harness: Pi | ok |  |  | Pi lanciato dal menu contestuale, risponde; stato tab resta "?" per #370 |
| 16 | Terminale | Harness: Oh-My-Pi | todo | | | |
| 17 | Activity | Stato running/idle/needs-input per pane (dot nella tab, sidebar, tray) | ok | #370 (chiusa) | opencode-go | sirioctl raggiunge l'app (elevato↔elevato, non-elevato↔non-elevato); stato agente da riverificare con hook |
| 18 | Chat | Tab Chat (ACP): invio prompt lungo, streaming, markdown, permessi | todo | | | |
| 19 | Editor/Markdown | Apri file `.md` (`Ctrl+O`, drag&drop, link nel terminale), preview/code mode, live reload, salva (`Ctrl+S`) | bug | #373 | pi | apertura/preview/live ok; Code clippato in pane stretto; salvataggio riscrive il file |
| 20 | Right panel | Toggle (`Ctrl+Shift+I`), tab Files | ok |  |  | toggle da titlebar ok; Ctrl+Shift+I non registrato su Windows (vedi note); Files aggiornato live |
| 21 | Right panel | Tab Changes: stage/unstage/discard, apri diff tab | ok | #367 (chiusa) | pi | gutter 5 cifre corretto; stage/unstage/discard (con conferma) ok |
| 22 | Right panel | Tab Git (grafo/branch) e altre icone del pannello | ok | #366 (chiusa) | opencode-go | colonna Date corretta post-merge; filtri/ricerca da testare |
| 23 | Browser | Tab browser embedded | ok | #368 (chiusa), #369 | opencode-go / pi | crash risolto (2/2 + 2 restore); barra indirizzi in retest |
| 24 | Settings | Apertura, modifica (tema, shell, socket, provider), applicazione, ripristino | bug | #371 (aperta), #365 (chiusa) | pi | tema Light/Dark/System e font terminale si applicano; font Interface inerte |
| 25 | Status bar | Indicatori provider usage (Claude/Codex/...), refresh | ok |  |  | indicatori Claude/Codex presenti; tooltip = etichetta (vedi note) |
| 26 | Controllo | `sirioctl` / control socket | ok | #370 (chiusa) | opencode-go | ping/version/list-workspaces ok da PowerShell e Git Bash |
| 27 | Notifiche/Tray | Notifica fine agente, icona tray | todo | | | |
| 28 | Persistenza | Riavvio app: tab, worktree, sessioni ripristinate | ok |  |  | progetto, tab e pane secondario ripristinati (2 riavvii) |
| 29 | Finestra | Resize continuo, minimizza/ripristina, multi-monitor/DPI | todo | | | |
| 30 | Stress | Molti terminali aperti insieme + output pesante + cambio worktree | todo | | | |

## Assegnazione issue → agente

| Issue | Agente | Worktree | Tentativi | Esito |
|-------|--------|----------|-----------|-------|
| #364 console Windows | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-364` (w4 rimosso; tentativo 2 in herdr wE) | 2 | tentativo 1 mergiato (a4f4bbef) e **revertito** (fa638c0b): console `git.exe` lampeggianti; tentativo 2 in corso |
| #365 registry agenti / PATH | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-365` (herdr w5, rimosso) | 1 | verificata e chiusa (ae816097) |
| #366 git log colonna Date | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-366` (herdr w6, rimosso) | 1 | verificata e chiusa (bc846cb1) |
| #367 gutter diff 5 cifre | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-367` (herdr w7, rimosso) | 1 | verificata e chiusa (df0ba45f) |
| #368 crash New Browser (RefCell) | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-368` (herdr w8, rimosso) | 1 | verificata e chiusa (6525bc00) |
| #369 barra indirizzi browser | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-369` (herdr w9) | 2 | tentativo 1 mergiato (33e96f85→); riaperta per il caso Ctrl+L (primi 3 caratteri persi), tentativo 2 in corso |
| #370 sirioctl pipe/owner Windows | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-370` (herdr wA, rimosso) | 1 | verificata e chiusa (33e96f85) |
| #371 font Interface inerte | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-371` (herdr wB) | 1 | in corso |
| #372 dialog Remove Worktree | opencode-go (muse-spark-1.3-contributor, xhigh) | `~/.herdr/worktrees/sirio/fix-372` (herdr wC) | 1 | in corso |
| #373 editor Markdown riscrive il file | pi (gpt-5.6-luna, xhigh) | `~/.herdr/worktrees/sirio/fix-373` | 1 | in corso |

## Note non classificate come bug

- (vedi `qa/report.md`)
