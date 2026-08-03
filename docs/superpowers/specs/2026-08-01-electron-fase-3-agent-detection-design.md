# Fase 3 — Agent activity detection (migrazione Electron)

**Data:** 2026-08-01
**Repo di destinazione:** `~/Desktop/Progetti/tiller-electron`
**Origine (sola lettura):** `~/Desktop/Progetti/tiller` (Swift), `stablyai/orca` (TypeScript)

## Obiettivo

Portare i quattro layer di rilevamento dell'attività degli agenti — hook,
titolo OSC, contenuto di schermo, processo in primo piano — nel main Electron,
con lo stato agente leggibile da `tillerctl` senza alcuna UI.

Fuori scopo: badge in sidebar (Fase 4), notifiche di sistema (Fase 7), pannello
Agents (Fase 6).

## Contesto misurato

### Sorgente Swift

| File | Righe |
|---|---|
| `TillerCore/AgentActivityModel.swift` | 292 |
| `TillerCore/AgentTitleIdentity.swift` | 59 |
| `TillerCore/AgentTitleStatus.swift` | 57 |
| `TillerCore/ScreenManifest.swift` | 40 |
| `TillerCore/AgentSignalMerger.swift` | 20 |
| `App/ForegroundProcessAgent.swift` | 82 |

### Già presente in Electron (Fasi 1–2)

- 5 adapter + `AGENT_CATALOG`, con `prepare()` **no-op deliberato**
- evento `pane.title` e `AppState.setPaneTitle` (che scarta i titoli invariati)
- `terminal.readText(n)` sullo xterm headless del main
- `tillerctl run --agent <id>`

### Manca

- `prepare()` reale (scrittura degli hook)
- pid della shell esposto dal `PtyManager`
- ogni logica di detection

## Cattura dal vivo (2026-08-01)

Le regole di questo documento **non sono portate dallo Swift**: derivano da una
registrazione delle cinque CLI vere. L'harness (`docs/catture-agenti-2026-08-01/capture.mjs`
nel repo Electron) lancia l'app Electron isolata, apre un pane per agente e
campiona ogni 700 ms titolo OSC e coda di schermo. Le registrazioni grezze sono
accanto all'harness.

### Titoli osservati

| Agente | Inattivo | Al lavoro |
|---|---|---|
| claude | `✳ Claude Code`, `✳ Creare file prova.txt con parola ciao` | `⠂ Claude Code`, `⠂ Creare file …` |
| codex | *nessun titolo, mai* | *nessun titolo* |
| opencode | `OpenCode` | `OC \| Creazione file prova.txt con parola ciao` |
| pi | `π - cap-pi-0aPG6D` | *non osservato* |
| omp | `π > cap-omp-EafzYT`, `π > Crea file prova.txt con ciao` | `π ⠼ …`, `π ⠹ …`, `π ⠦ …` |

### Due difetti dell'implementazione Swift, confermati dai dati

1. **omp viene classificato come pi.** `AgentTitleIdentity` distingue il fork
   con `title.contains("π:")`; omp scrive `π > `. Il controllo fallisce e cade
   sul ramo successivo (`contains("π")` → `pi`). La convenzione col due punti
   era corretta quando fu catturata (2026-07-11) ed è cambiata da allora.
2. **OpenCode perde l'identità mentre lavora.** Il matcher generico richiede
   `lower.contains("opencode")`; al lavoro il titolo è `OC | <task>`.

Inoltre il ramo `title.hasPrefix(". ")` di `AgentTitleStatus.detectClaude` non
compare in nessuna cattura: convenzione superata, sopravvissuta come peso morto.

### Regole di contenuto: zero riscontri su otto

Nessuna delle otto stringhe cercate da `ScreenManifest` (`"Do you want to
proceed?"`, `"esc to interrupt"`, `(y/n)`, `[y/n]`, `proceed?`, `continue?`,
`allow?`, `confirm?`) compare nell'output reale di alcuno dei cinque agenti.

Il prompt di permesso di Claude esiste, ma è parametrizzato sull'azione:
`Do you want to create prova.txt?`.

**Il caso più pericoloso è OpenCode:** la sua barra di stato contiene
`esc interrupt tab agents ctrl+p commands • OpenCode 1.18.0` in **ogni**
istante, anche a riposo. Una regola generica "esc to interrupt → al lavoro"
marcherebbe OpenCode come eternamente occupato, e nessun altro segnale lo
correggerebbe, perché OpenCode non ha hook nativi.

### Dimensione della finestra di contenuto

Con le ultime 6 righe la richiesta di permesso di Claude è invisibile: sotto di
essa c'è solo la barra di stato fissa della TUI. Con 30 righe compare. La
specifica fissa **40 righe**: il confine misurato più margine.

### Layer D: il rischio dichiarato nel piano di migrazione non esiste

Il documento di migrazione indicava `proc_listchildpids` come nodo della Fase 3.
Misurazioni:

| Segnale | Costo | claude | codex | opencode | pi | omp |
|---|---|---|---|---|---|---|
| `pty.process` (node-pty) | 0 | `2.1.220` | `codex` | `opencode.exe` | `node` | `bun` |
| `ps` — nome eseguibile | 24 ms / 794 processi | `claude` | `codex` | `opencode` | `pi` | `bun` |
| `ps` — riga di comando | stessa chiamata | ✓ | ✓ | ✓ | ✓ | `bun /Users/…/.bun/bin/omp` |

Una sola `ps -axo pid=,ppid=,comm=,args=` risolve tutti e cinque gli agenti.
`pty.process` non serve come identità (Claude riscrive il proprio titolo di
processo nella versione, `2.1.220`) ma è un **rilevatore di cambiamento
gratuito**: passa da `zsh` ad altro quando un agente parte.

Cade anche l'affermazione del commento Swift secondo cui i CLI ospitati da
Node/Bun sarebbero invisibili al Layer D: **pi si vede** (`comm = pi`);
l'unico opaco è omp, e la riga di comando lo scopre.

## Decisioni

**D1 — Una scansione processi per tutti i pane, non una per pane.**
In Swift libproc costa microsecondi, quindi scansionare per pane è gratis. In
Node ogni scansione è un fork da 24 ms, ma quella singola tabella contiene già
tutti i pane. Portare la struttura Swift alla lettera darebbe 20 pane × 24 ms
a ogni raffica di output. Scartate: `ps-list` come dipendenza (su macOS esegue
la stessa `ps` e non espone `args`), modulo nativo (costo di build senza
guadagno).

**D2 — Innesco della scansione: `pty.process` cambia.** Gratuito, preciso,
e scatta all'avvio dell'agente prima ancora del primo titolo.

**D3 — L'identità la dà il Layer D; il Layer B solo quando è inequivocabile.**
Un titolo `π …` dichiara la famiglia pi, non quale membro. Swift indovina il
separatore (e sbaglia); Orca rinuncia e fonde pi e omp in un'unica etichetta.
Il separatore è cosmetico ed è già cambiato una volta; il nome dell'eseguibile
no. Questo elimina la classe di bug, non l'istanza.

**D4 — Regole di contenuto solo ancorate per agente, mai generiche.**
Due regole con evidenza, tre astensioni dichiarate. `null` non forza mai una
transizione: un agente senza regola resta governato dagli altri layer.

**D5 — Di Orca si prende la struttura, non il catalogo.** Orca copre ~15
agenti con un albero di casi speciali (Gemini, Cursor, Droid, Hermes,
Antigravity, Grok, MiMo, Aider, OpenClaude, Copilot, Devin). Tiller ne ha 5.
Si adottano tre sue idee, migliori dello Swift:
- tracker a **transizioni** (`onBecameIdle` / `onBecameWorking` / `onAgentExited`)
  invece che a stati;
- un titolo che sparisce **mentre lavora** è transitorio e non azzera nulla;
  sparisce **mentre è fermo** significa uscito (Swift azzera in entrambi i casi);
- normalizzazione dei titoli animati.

**D6 — Gli hook citano un path stabile, non il binario corrente.**
Nel Tiller Swift gli hook incorporavano un path assoluto a `tillerctl`; l'app
si è spostata e i worktree hanno continuato a chiamare un binario inesistente,
in silenzio. Gli hook citeranno
`~/Library/Application Support/tiller-electron/bin/tillerctl`, uno shim
rigenerato a ogni avvio.

**D7 — `prepare()` scrive negli stessi path dello Swift.** L'app Swift è
dismessa: nessun namespace separato, nessuna coesistenza da gestire. Il
commento di Fase 1 che dichiara il no-op temporaneo va rimosso.

**D8 — Gli e2e usano agenti finti costruiti dalle catture.** `scripts/ci.sh`
gira di continuo; chiamare LLM reali consumerebbe quota a ogni esecuzione.
Cinque script riproducono le sequenze OSC e il testo registrati dal vivo,
ciascuno col nome eseguibile giusto perché anche il Layer D li riconosca. Le
CLI vere restano in una checklist manuale, da eseguire una volta.

## Architettura

Tutto in `src/main/agents/detection/`, logica pura: nessun import di `electron`
né di `node:net`, come già vale per `src/main/terminal/`.

```
src/main/agents/detection/
  agent-status.ts       AgentStatus + priorità (error > needs-input > running > done)
  title-identity.ts     identify(titolo) → agentId | null
  title-status.ts       detect(titolo, agentId) → status | null
  title-normalize.ts    collassa i fotogrammi animati
  content-status.ts     detect(coda, agentId) → status | null
  process-table.ts      una ps → Map<pid, {ppid, comm, args}> + figli di un pid
  process-identity.ts   recognize(comm, args) → agentId | null
  signal-merger.ts      finestra di debounce Layer A → Layer B
  activity-model.ts     macchina a stati: proprietà del pane, fusione dei layer

src/main/agents/detection-coordinator.ts   innesco: timer, PTY, scansioni
```

I moduli in `detection/` sono funzioni pure su stringhe: si testano senza PTY,
senza app e senza tempo. Il coordinator è l'unico pezzo che conosce timer e
processi, ed è l'unico difficile da testare — tenerli mescolati è il motivo per
cui in Swift la logica dei layer si prova solo attraverso l'app intera
(`AppModel.swift`, 2 600+ righe).

### Inneschi

| Layer | Innesco | Sorgente |
|---|---|---|
| A — hook | `tillerctl notify` sul socket di controllo | l'agente stesso |
| B — titolo | evento `pane.title` (Fase 2) | xterm headless nel main |
| C — contenuto | quiete dell'output, debounce 400 ms | `terminal.readText(40)` |
| D — processo | `pty.process` cambia → **una** `ps` per tutti i pane | tabella processi |

### Uscita del processo

Non è un quinto layer ma un ripiego trasversale: alla morte del processo del
pane, codice `0` → `done`, diverso da zero → `error`. È **l'unica via a `done`**
per gli agenti privi di hook nativi (pi, opencode). Si applica solo a un pane
che ha già uno stato registrato: un pane mai riconosciuto come agente non
acquista uno stato morendo.

### Debounce fra layer

`signal-merger.ts` scarta un segnale di titolo (Layer B) quando un hook (Layer A)
per lo stesso pane è arrivato da meno di **1,5 s**, così un glifo in ritardo non
sovrascrive un evento esplicito più fresco. Stesso valore dello Swift.

Il Layer C **non** passa da questo filtro: una corrispondenza di contenuto è più
vicina alla realtà di una convenzione di titolo e può correggere uno stato
stantio. Un `notify` successivo la sovrascrive comunque senza condizioni, quindi
il Layer C non può bloccare permanentemente uno stato sbagliato.

Il debounce di quiete dell'output per innescare il Layer C parte da **400 ms**,
da tarare se le TUI più chiacchierone non si fermano mai abbastanza a lungo.

### Proprietà del pane

Si mantiene il modello Swift a tre proprietari:

- **spawn-owned** — lanciato da Tiller; azzerato dall'uscita del processo
- **title-owned** — riconosciuto dal titolo; azzerato quando il titolo smette
  di somigliare a quell'agente **e** l'agente non risulta al lavoro (D5)
- **process-owned** — riconosciuto dalla `ps`; azzerato **solo** da una
  scansione riuscita che non trova più il processo, mai da un titolo che cambia

## Regole

### `title-identity.ts` — identify(titolo)

| Titolo | Esito |
|---|---|
| `✳` da solo, o prefisso `✳ ` | `claude` |
| carattere braille (U+2800–U+28FF) **in posizione 0** | `claude` |
| inizia con `π` | `null` — famiglia pi, decide il Layer D |
| `OpenCode` esatto, oppure prefisso `OC \| ` | `opencode` |
| parola intera `codex` (confine di parola, non sottostringa) | `codex` |
| altro | `null` |

Il confine di parola serve a non identificare un titolo di cwd come
`~/codex-notes` o `opencode-experiment`.

### `title-status.ts` — detect(titolo, agentId)

| Agente | Al lavoro | In attesa |
|---|---|---|
| claude | braille in posizione 0 | prefisso `✳ ` o `✳` da solo |
| pi, omp | braille in qualunque posizione | `π > …`, `π - …` senza braille |
| opencode | prefisso `OC \| ` | `OpenCode` esatto |
| codex | — (nessun titolo) | — |

"In attesa" (`needs-input`) è lo stato di un agente fermo al proprio prompt, in
attesa dell'utente — stessa convenzione dello Swift.

### `content-status.ts` — detect(coda40righe, agentId)

| Agente | Regola | Esito |
|---|---|---|
| claude | riga che soddisfa `/Do you want to .+\?/` | `needs-input` |
| omp | riga che soddisfa `/^[⠀-⣿]\s.*⟨esc⟩\s*$/` | `running` |
| opencode | **nessuna regola, deliberatamente** | — |
| codex, pi | **nessuna regola** — comportamento non osservato | — |

Le astensioni sono parte della specifica, non lacune.

### `process-identity.ts` — recognize(comm, args)

```
args contiene un segmento di path che termina in /omp   → "omp"   (per primo)
basename(comm) ∈ {claude, codex, opencode, pi}          → quell'id
altrimenti                                              → null
```

L'ordine conta: omp va riconosciuto dalla riga di comando prima che `comm`
(`bun`) porti fuori strada.

### `title-normalize.ts`

omp riscrive il titolo a ogni fotogramma dello spinner (`π ⠼`, `π ⠹`, `π ⠦`…).
La normalizzazione sostituisce il carattere braille con uno fisso: i fotogrammi
diventano titoli identici e `AppState.setPaneTitle`, che già scarta i titoli
invariati dalla Fase 2, assorbe la raffica senza codice nuovo.

## Layer A — hook

`prepare()` diventa reale per gli adapter che ne hanno bisogno:

| Agente | Cosa scrive |
|---|---|
| claude | `<worktree>/.claude/settings.local.json`, sostituendo **solo** i cinque array di hook (`Stop`, `Notification`, `SessionStart`, `UserPromptSubmit`, `SessionEnd`) e preservando ogni altra chiave |
| codex | niente su disco: override `-c notify=[…]` nella riga di comando (TOML, quindi `jsonStringLiteral` senza escape delle barre) |
| opencode | `<worktree>/.opencode/plugin/tiller-session.js` — riporta solo l'id di sessione, non lo stato |
| omp | `<worktree>/.tiller/omp-hook.ts`, caricato con `omp --hook <file>`; chiama `notify` su `session_start` (running), `turn_start` (running), `turn_end` (needs-input), `session_shutdown` (done) |
| pi | niente: pi non ha meccanismo di hook. `hasNativeHooks: false`, lo stato arriva dai Layer B/C/D e dal codice di uscita |

Mai la configurazione globale dell'utente (`~/.claude/settings.json`,
`~/.codex/config.toml`).

### Shim a path stabile (D6)

All'avvio l'app rigenera
`~/Library/Application Support/tiller-electron/bin/tillerctl` in modo che punti
al binario corrente. Gli hook citano quel path. Un hook è una scrittura che
sopravvive a chi l'ha scritta: nessun compilatore, nessun tipo e nessun test
attraversa quel confine, quindi il testo scritto non deve contenere
informazioni volatili.

## Protocollo

| Aggiunta | Forma |
|---|---|
| `ControlRequest` | `notify` — `{ paneId: string, status: AgentStatus }` |
| `StateEvent` | `pane.agent` — `{ paneId, agentId: string \| null, status: AgentStatus \| null }` |
| `PaneSnapshot` | campi `agentId: string \| null`, `status: AgentStatus \| null` |
| `tillerctl` | `notify --pane <id> --status <s>` |

`tillerctl state` mostra agente e stato di ogni pane: la Fase 3 resta
verificabile interamente da CLI, senza UI.

## Errori

| Caso | Comportamento |
|---|---|
| `ps` fallisce o va in timeout | nessun cambio d'identità; **mai** azzerare. Solo una scansione riuscita che non trova l'agente significa "uscito" |
| `prepare()` non può scrivere (worktree in sola lettura) | l'agente parte comunque; Layer A assente, gli altri layer coprono |
| `notify` malformata | respinta dallo schema Zod, risposta d'errore |
| titolo assente mentre l'agente lavora | transitorio: si ignora (D5) |
| titolo assente mentre l'agente è fermo | l'agente è uscito: si azzera, se title-owned |

La prima riga è la più importante: confondere "non ho potuto guardare" con "non
c'è" cancellerebbe lo stato di un agente vivo a ogni scansione fallita.

## Verifica

### Unità

Funzioni pure con le catture del 2026-08-01 come fixture. Ogni riga delle
tabelle di regole sopra corrisponde a un test con la stringa reale registrata,
non con una stringa inventata.

### e2e (agenti finti, deterministici)

1. **omp è omp, non pi.** Guardia diretta sul difetto trovato. Validazione per
   mutazione: rimettere l'identità sul titolo deve far tornare rosso il criterio.
2. **Codex, senza titolo, risulta comunque attivo.** L'unica prova del Layer D.
3. **Una scansione, non venti.** 20 pane, conteggio delle invocazioni di `ps`:
   costante per innesco, non proporzionale al numero di pane.
4. **Una scansione fallita non cancella nulla.** Stato vivo, scansione forzata
   a fallire, stato intatto.

### Checklist manuale (una volta, con le CLI vere)

Gli agenti finti provano la logica; solo le CLI vere provano le convenzioni.

- [ ] claude: al prompt risulta in attesa; inviato un compito passa a al-lavoro e torna in attesa
- [ ] claude: a una richiesta di permesso risulta in attesa (Layer C)
- [ ] codex: identificato e attivo pur senza titolo
- [ ] opencode: identificato da fermo e **non** eternamente al lavoro
- [ ] pi: identificato come `pi`, non come `omp`
- [ ] omp: identificato come `omp`, non come `pi`
- [ ] hook: `tillerctl notify` di Claude arriva e vince sul titolo entro la finestra di debounce

## Rischi aperti

- **pi al lavoro non è stato osservato.** Nella cattura pi non ha risposto entro
  80 s. La regola "braille ovunque → al lavoro" per la famiglia pi è derivata da
  omp e dall'implementazione di Orca, non da una registrazione diretta di pi. La
  checklist manuale la verifica.
- **Codex non emette alcun titolo** in questa versione, mentre il `CLAUDE.md`
  Swift afferma che Codex 0.144+ scrive uno spinner braille nel titolo. Non
  osservato; il design non ci fa affidamento (Codex dipende dal Layer D).
- **Le convenzioni di titolo cambieranno di nuovo.** È già successo due volte in
  tre settimane. D3 riduce il danno spostando l'identità sul nome del processo,
  ma lo *stato* resta legato al titolo per gli agenti senza hook.
