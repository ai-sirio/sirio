# Migrazione Tiller a Electron + TypeScript — Design

Data: 2026-08-01
Stato: approvato, Fase 0 pronta per la pianificazione implementativa

## Obiettivo

Migrare Tiller da app nativa macOS (SwiftUI, Swift 6) a Electron + TypeScript,
replicando le funzionalità attuali. Il progetto nasce in una cartella separata;
il repo Swift resta intatto.

## Contesto

Tiller oggi: ~56.000 righe Swift in 9 package + target `App/`.

| area | righe | destinazione Electron | difficoltà |
|---|---|---|---|
| `App/` (SwiftUI) | 19.179 | `renderer/` | alta — riscrittura, non port |
| `TillerACP` | 12.172 | `main/acp/` | media — JSON-RPC, mappa bene su TS |
| `TillerCore` | 9.940 | `main/` + `shared/` | media — logica pura, port 1:1 |
| `TillerTerminal` | 3.682 | `main/pty` + `renderer/terminal` | alta — libghostty non esiste |
| `TillerWorkspace` | 3.375 | `shared/layout` + `renderer` | media |
| `TillerGit` | 2.124 | `main/git/` | bassa — già shell-out |
| `TillerControl` | 2.029 | `main/control/` + CLI | bassa — socket unix + JSON |
| `TillerAgents` | 1.393 | `main/agents/` | bassa — port diretto |
| `TillerPersistence` | 1.241 | `main/db/` | bassa/media — 16 migrazioni da riscrivere |
| `TillerCode` | 631 | `renderer` | media |

Rischio principale isolato: `TillerTerminal/Package.swift` dipende da
`libghostty-spm`. Non si porta — si rifà su xterm.js, portandosi dietro
scrollback, titolo OSC (Layer B della detection agenti) e split ricorsivi.

### Orca come riferimento

Tiller è una reimplementazione Swift di [Orca](https://github.com/stablyai/orca),
che è Electron/TypeScript. Migrare a Electron significa tornare allo stack
originale del progetto di riferimento.

Struttura di Orca (verificata sul repo):

| percorso | ruolo | stack |
|---|---|---|
| `src/main/` | Node, accesso OS: PTY, git, socket, SQLite | TypeScript |
| `src/preload/` | bridge IPC tipizzato (`api-types.ts`) | TypeScript |
| `src/renderer/src/` | UI: `components/ hooks/ store/ lib/ i18n/` | React 19 + Zustand |
| `src/shared/` | tipi/schema condivisi | Zod |

Nota: `src/main/ghostty/` **non** è libghostty — è un parser dei file di config
e tema di Ghostty (`parser.ts`, `theme-resolution.ts`, `mapper.ts`) usato per
importarne i temi. Il rendering è `@xterm/xterm` nel renderer; `src/main/pty/`
fa solo plumbing di environment per node-pty.

Conseguenza sulla scelta del framework UI: `main/` e `preload/` di Orca restano
riutilizzabili qualunque framework si scelga (circa metà del lavoro). La leva si
perde solo sul renderer.

## Decisioni

| decisione | valore | motivazione |
|---|---|---|
| collocazione | `~/Desktop/Progetti/tiller-electron/`, repo git nuovo e indipendente | il repo Swift resta app funzionante durante la migrazione e specifica eseguibile per i dubbi di comportamento; nessun rischio su `Tiller.xcodeproj`/`project.yml`/`Scripts/ci.sh`; niente due toolchain nello stesso albero |
| stack | Electron + TypeScript strict, Svelte 5 (rune) + Vite, pnpm | scelta esplicita dell'utente su Svelte |
| framework UI | Svelte 5, **non** SvelteKit | dentro un renderer Electron non c'è server né rotte: SSR, routing a file e server actions non si applicano |
| target piattaforma | macOS-first, cross-platform predisposto | path e spawn astratti; Windows/Linux non testati né spediti finché non serve |
| processi | `main/` · `preload/` · `renderer/` · `shared/` | split standard Electron |
| contratto IPC | protocollo `ControlRequest` unico, due trasporti | vedi sotto |
| collocazione stato | autoritativo nel main, locale nel renderer | vedi sotto |
| policy CLI | headless di default, `--focus` per aprire | vedi sotto |

### Nota sulla performance (React vs Svelte)

Sui benchmark sintetici Svelte è più veloce (compila via il virtual DOM, runtime
più piccolo, memoria per componente più bassa). In questa app però i costi
dominanti sono indipendenti dal framework:

| costo | chi lo paga |
|---|---|
| rendering terminale (xterm.js WebGL) | nessuno dei due — canvas/WebGL |
| serializzazione IPC main↔renderer | nessuno dei due — sta sotto |
| throughput PTY / backpressure | nessuno dei due — nel main |
| liste sidebar/tab | entrambi banalmente veloci |
| streaming token in chat | qui il framework conta |

Sull'ultima riga Svelte ha un vantaggio strutturale (reattività fine-grained vs
diff del sottoalbero). Ma è lo stesso problema già incontrato e risolto in Swift:
il freeze della chat T3 aveva come root cause la coda di transazioni SwiftUI
saturata da eventi per-token dei driver nativi, risolto con coalescing (`e3b64ab`);
stessa famiglia il fix di backpressure OpenCode (`50982d2`). Il rimedio corretto
— **coalescere al confine IPC prima che l'evento tocchi il framework** — è
identico in React e in Svelte, e neutralizza proprio il punto in cui Svelte
vincerebbe.

La performance non è quindi l'asse decisionale.

## Architettura

### Contratto IPC: un protocollo, due trasporti

Tiller ha già un protocollo di controllo in produzione: `ControlRequest` su
socket unix (`panel.create`, `panel.write`, `panel.read`, `panel.wait`, `notify`,
`session.ref`, `worktree.set`, più i gruppi cmux-parity `workspace.*`,
`surface.*`, `pane.surfaces`, `notification.*`, `system.*`, `session.restore`),
JSON line-delimited, già parlato da `tillerctl` e dagli hook degli agenti
(Layer A della detection).

Quello diventa il contratto IPC unico. Il renderer è un altro client dello stesso
protocollo.

```
                 ┌──────────────────────────────┐
  tillerctl ─────┤                              │
  agent hooks ───┤   protocollo ControlRequest  │
                 │   (schemi Zod in shared/)    │
  renderer ──────┤                              │
                 └──────────────┬───────────────┘
                    socket unix │ Electron IPC
                                ▼
                          main process
```

Direzione dei messaggi: il renderer manda **comandi**, il main emette **eventi**
(snapshot iniziale + delta). L'`AppModel` in Svelte è una proiezione osservabile
dello stato del main, non la sua sede.

Vantaggi: un solo protocollo da versionare, testare e documentare; le Fasi 1–3
sono verificabili headless mandando JSON sul socket, senza aprire una finestra.

Limite noto: il protocollo è nato per automazione CLI, non per pilotare una UI.
Non copre streaming di output terminale ad alta frequenza né stato di layout.
Struttura reale: **nucleo condiviso + estensioni UI-only**, con l'output PTY su
un canale separato ad alto volume, coalescato al confine.

Alternative scartate:

- **IPC dedicato al renderer + control socket separato**: duplica i concetti di
  dominio (pane, worktree) e li tiene sincronizzati a mano — deriva che genera
  bug in cui i due lati non concordano su chi possiede cosa.
- **RPC tipizzato (electron-trpc o simili)**: dipendenza e livello di astrazione
  in più per un problema che un helper tipizzato di poche decine di righe
  risolve, e allontana dal protocollo già esistente.

### Collocazione dello stato

Requisito dell'utente: la CLI deve poter controllare anche la UI (es. aprire un
terminale e lanciarci un comando).

Oggi in Swift il control socket arriva a `AppModel.handleControl`, che è
`@MainActor`: il socket tocca direttamente lo stato della UI, gratis, perché è
un processo solo. In Electron main e renderer sono processi separati, quindi
"apri un terminale e lancia un comando" attraversa il confine tre volte:

| passo | natura |
|---|---|
| crea il PTY | backend → main |
| rendilo visibile, focalizzato, nel tab giusto | **UI → da decidere** |
| scrivi il comando sul PTY | backend → main |

Se lo stato UI vivesse nel renderer, la CLI potrebbe pilotarlo solo con una
finestra aperta.

**Regola di taglio:**

> Se lo persisti, è autoritativo → main.
> Se lo butti alla chiusura della finestra, è locale → renderer.

| autoritativo (main) | locale (renderer) |
|---|---|
| layout degli split, ordine tab, tab attivo | hover, focus ring |
| worktree aperti, selezione sidebar | posizione di scroll |
| contenuto/identità dei pane | selezione di testo |
| sessioni chat e agenti | popover aperti, drag in corso |

La colonna sinistra coincide con le tabelle già persistite oggi: `project`,
`worktree`, `workspaceLayout`, `workspaceTab`, `terminalTab`, `terminalContent`,
`paneScrollback`, `chatSession`, `chatItem`, `agentSession`, `agentAccount`. Il
taglio è già validato da sedici migrazioni di schema.

Costo accettato: ogni interazione UI che tocca la colonna sinistra fa un
round-trip al main (sub-millisecondo in locale). Scroll e hover **devono** stare
a destra; la regola di taglio protegge da sola dal caso patologico.

Flusso risultante:

```
tillerctl run --worktree feat/x --cmd "npm test"
   │
   ├─ main: crea terminalContent + PTY (node-pty)
   ├─ main: aggiorna workspaceLayout + terminalTab   ← stato autoritativo
   ├─ main: emette evento di stato
   │        └─ renderer: proietta → il pane appare, focalizzato
   └─ main: scrive "npm test\n" sul PTY
```

Proprietà chiave: funziona **anche senza finestra aperta**. Il main crea pane e
PTY comunque; quando una finestra si apre, proietta lo stato che c'è già. È il
comportamento che Tiller ha già oggi — chiudere la finestra non uccide i PTY,
solo l'uscita dall'app lo fa.

Effetto collaterale: se la UI parla lo stesso protocollo degli agenti, ogni
azione utente è per costruzione automatizzabile da `tillerctl`; la parità CLI/UI
diventa una proprietà strutturale invece che una feature da mantenere.

### Policy di focus della CLI

Default headless, `--focus` per aprire:

```bash
tillerctl run --worktree feat/x --cmd "npm test"           # crea, esegue, silenzioso
tillerctl run --worktree feat/x --cmd "npm test" --focus   # crea, esegue, apre e focalizza
```

Motivazione: gli hook degli agenti (Layer A, `tillerctl notify`) sparano decine
di volte per sessione; se ogni notify alzasse una finestra l'app sarebbe
inusabile mentre si lavora altrove. `notify` non apre mai finestre per
costruzione e non accetta `--focus`.

## Decomposizione in fasi

Ogni fase avrà la sua spec e il suo piano.

```
Fase 0  Stack + walking skeleton    → Svelte 5, electron-vite, IPC tipizzato,
                                       1 pane node-pty+xterm, CI
Fase 1  Fondamenta main             → Git, Persistence+migrazioni, Agents,
                                       Control+tillerctl (headless, zero UI)
Fase 2  Motore terminale            → panes, splits, scrollback, ownership,
                                       titolo OSC
Fase 3  Agent activity detection    → i 4 layer (D è il nodo: sostituto di libproc)
Fase 4  UI shell                    → sidebar, tab bar, workspace layout, temi
Fase 5  Chat ACP                    → driver + timeline + UI chat
Fase 6  Pannelli & editor           → RightPanel/diff, CodeEditor,
                                       MarkdownEditor, Files
Fase 7  Piattaforma                 → auto-update (Sparkle→electron-updater),
                                       menu bar, notifiche, permessi, usage
```

Ordine orizzontale (prima il backend, poi la UI) e non verticale: le Fasi 1–3
sono verificabili senza UI, quindi il backend gira e si testa da CLI mentre
l'UI è ancora vuota, e la logica agenti non va debuggata attraverso i pixel.

## Fase 0 — walking skeleton

Obiettivo: attraversare tutti i confini una volta sola con il caso più sottile
possibile. Se il percorso CLI → main → PTY → renderer funziona per un `echo`,
funziona strutturalmente per tutto il resto.

### Struttura

```
tiller-electron/
├── src/
│   ├── shared/          schemi Zod: ControlRequest, eventi di stato, tipi dominio
│   ├── main/
│   │   ├── control/     server socket unix + dispatch (unico punto di ingresso)
│   │   ├── pty/         node-pty, spawn, environment
│   │   ├── state/       stato autoritativo + emissione eventi
│   │   └── index.ts
│   ├── preload/         bridge tipizzato, unica superficie esposta al renderer
│   └── renderer/
│       ├── lib/         AppModel in .svelte.ts (rune $state)
│       ├── components/  Terminal.svelte (xterm.js + addon WebGL)
│       └── App.svelte
├── cli/                 tillerctl (client del socket)
└── scripts/ci.sh        gate unico → stampa "CI OK"
```

### Criteri di successo

1. `tillerctl run --cmd "echo hello" --focus` → si apre la finestra, compare un
   pane, il comando esegue, l'output è visibile.
2. `tillerctl run --cmd "echo hello"` con l'app a finestra chiusa → crea comunque
   pane e PTY; aprendo poi la finestra, il pane è già lì con il suo output.

Il criterio 2 è quello che conta: è l'unico che può **falsificare** la decisione
sulla collocazione dello stato. Un walking skeleton che apre sempre una finestra
passerebbe anche con lo stato messo nel renderer, cioè non distinguerebbe il
disegno giusto da quello sbagliato.

### Fuori scope in Fase 0

Split, tab, sidebar, persistenza SQLite, adapter agenti, detection, chat, temi.
Un pane solo, nessun layout, nessun database.

### Gate CI

`scripts/ci.sh`, stessa filosofia di `Scripts/ci.sh` oggi (un solo comando,
stampa `CI OK`): typecheck TypeScript strict, lint, unit test con Vitest, e un
e2e Playwright-Electron che esegue esattamente i due criteri di successo.

### Nota operativa

Per ogni file `.svelte` usare il server MCP Svelte, l'agente `svelte-file-editor`
e le skill `svelte-core-bestpractices` / `svelte-code-writer`, già configurati.
Serve a scrivere Svelte 5 con le rune e non Svelte 5 con la testa a Svelte 4.

Mappatura utile: l'`AppModel` attuale (`@MainActor @Observable`) ha un analogo
quasi letterale in una classe con campi `$state` dentro un modulo `.svelte.ts`.
Il vincolo nuovo è che quello stato vive nel renderer, mentre in Swift `AppModel`
toccava direttamente PTY e filesystem: in Electron quella parte va nel main e
l'`AppModel` diventa una proiezione dello stato del backend.

## Questioni aperte (per le fasi successive)

- **Fase 1**: SQLite in Node — `better-sqlite3` o `node:sqlite`. Electron 43
  include Node 24.18.1 (verificato nei `DEPS` del branch `43-x-y`), dove
  `node:sqlite` non è più sperimentale: possibile zero dipendenze native da
  ricompilare per Electron. Da decidere nella spec di Fase 1, valutando cosa
  serve alle 16 migrazioni esistenti.
- **Fase 3**: Layer D della detection agenti usa oggi libproc
  (`proc_listchildpids`/`proc_name`) per elencare i processi figli della shell.
  In Node serve un modulo nativo o uno shell-out; da valutare nella spec di
  Fase 3. È l'unico segnale che intercetta gli agenti senza convenzione di
  titolo usabile (es. Codex), quindi non è opzionale.
- **Fase 2**: parità di rendering terminale xterm.js vs libghostty — scrollback,
  performance con pane nascosti (in Swift il problema era IOSurface a 497 MB),
  titolo OSC.
