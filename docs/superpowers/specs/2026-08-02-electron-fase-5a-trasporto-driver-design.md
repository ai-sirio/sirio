# Fase 5a — Trasporto e driver di chat: design

**Obiettivo:** portare in Electron il livello che parla con gli agenti di chat —
JSON-RPC, trasporto di processo, i quattro driver nativi e l'ACP generico —
fino a poter mandare un prompt e ricevere una risposta **interamente da CLI**,
senza interfaccia.

**Repo:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
`~/Desktop/Progetti/tiller` è **sola lettura**.

**Dipende da:** Fase 1 (database, `chatSession`/`chatItem` già importati),
Fase 2 (regola del canale ad alto volume), Fase 4a/4b (workspace, per sapere
dove una chat vivrà — ma la 5a non tocca la UI).

---

## Perché questa fase è divisa in tre

In Swift la chat è ~11.100 righe: 6.760 in `TillerACP` e 4.326 in `App/Chat`.
Più di qualunque fase precedente. Un solo spec produrrebbe un piano
ingestibile, e i difetti di piano crescono con la sua superficie — nella 4a
l'esecutore si è fermato nove volte, sempre per difetti del piano.

Il taglio ricalca 4a/4b, che ha funzionato:

| Sotto-fase | Contenuto | Come si verifica |
|---|---|---|
| **5a** (questo spec) | JSON-RPC, trasporto, 4 driver nativi + ACP | da CLI, senza UI |
| **5b** | riduttore del trascritto, timeline, storico | test unitari, logica pura |
| **5c** | vista del trascritto, card, markdown, composer | e2e dal gesto dell'utente |

Il seam fra 5a e 5b è netto: i driver **producono** eventi, il riduttore li
**consuma**. La 5a chiude quando gli eventi arrivano e si possono stampare;
non sa nulla di come verranno mostrati.

> **Lezione della 4b, da applicare qui.** La 4a era un motore corretto che
> **nessuno chiamava**: quindici task costruivano viste che consumavano un
> layout e nessuno lo procurava dal main. Se ne sono accorti solo i criteri
> e2e. La 5c dovrà quindi avere un task **esplicito** di cablaggio, e i suoi
> criteri partire dal gesto dell'utente. Vale la regola generale: ogni voce in
> "dipende da" deve poter essere puntata a un task che la consuma.

---

## Decisioni prese

### Tutti e quattro i driver nativi, non il solo ACP

Scelta dell'utente, contro la mia raccomandazione iniziale — che però si
appoggiava a un presupposto falso, verificato e caduto:
`AgentLaunchSpec.resolved` **rifiuta esplicitamente** `claude-acp`,
`codex-acp`, `opencode` e `pi` prima ancora di consultare l'install store. Un
ACP-solo non avrebbe parlato con i quattro agenti principali, ma solo con omp
e con ciò che si installa dal registry.

I transport nativi non nascono da un guadagno di prestazioni misurato — i «40
secondi di Codex» furono diagnosticati come server MCP irraggiungibili fuori
VPN, non come costo del protocollo. Nascono dalla catena di dipendenze: gli SDK
degli agenti sono comunque wrapper dei loro protocolli nativi, quindi un
adattatore ACP aggiunge un processo, una traduzione e un terzo pezzo da tenere
aggiornato senza dare nulla in cambio.

I quattro protocolli, tutti diversi:

| Agente | Protocollo | Righe Swift |
|---|---|---|
| Claude | stream JSON su stdout | 915 (driver + wire) |
| Codex | app-server JSON-RPC | 490 |
| OpenCode | HTTP + connessione persistente | 801 |
| Pi | RPC proprio | 827 |
| tutti gli altri | ACP su `ProcessTransport` | — |

### Le fixture si ri-registrano dai CLI installati

Non si portano quelle Swift. Le versioni oggi sono `claude 2.1.220`,
`codex 0.146.0`, `opencode 1.18.10`, `pi 0.82.1`; le fixture Swift furono
registrate contro **Codex 0.145**, quindi almeno una è già scaduta.

Il motivo non è prudenza generica. Nella Fase 3 le catture dal vivo dei cinque
CLI hanno trovato **due difetti presenti nell'app Swift in uso** — omp letto
come pi perché il separatore era passato da `π:` a `π >`, e OpenCode che
perdeva identità mentre lavorava. Nessuna lettura del codice li avrebbe
rivelati: il codice era coerente con sé stesso, si era spostato il mondo. I
protocolli di chat sono più esposti dei titoli di terminale, non meno: sono
superfici JSON i cui nomi di campo cambiano fra le minor, e la nota su Codex
0.145 («usa i nomi moderni `thread/*`, `item/*`») documenta un rinominamento
già avvenuto una volta.

Ri-registrare *solo* Codex sembra il compromesso furbo e non lo è: sappiamo che
Codex è cambiato di **versione**, non sappiamo che gli altri tre non siano
cambiati di **protocollo**, e nessuno dei tre ha un numero che ce lo dica.

`Scripts/record-{claude,codex,opencode}-fixture.sh` esistono nel repo Swift e
si portano; per pi va scritto.

### I driver vivono nel processo main

Non è una scelta libera. La regola dell'architettura fissata nel design della
migrazione dice *se lo persisti è del main, se lo butti alla chiusura della
finestra è del renderer*: le sessioni di chat sono persistite. E chiudere la
finestra non deve uccidere gli agenti — lo stesso vincolo che in Fase 2 ha
portato l'emulatore di terminale nel main.

Conseguenza: il renderer non parla mai con un agente. Manda comandi al main e
riceve eventi, esattamente come per i pane.

### `AgentDriver` si porta com'è

L'interfaccia Swift è già validata da cinque implementazioni. Diventa
un'interfaccia TypeScript con la stessa superficie:

```ts
export interface AgentDriver {
  readonly events: AsyncIterable<SessionEvent>
  start(): Promise<void>
  stop(): Promise<void>
  connect(opts: {
    cwd: string
    resumeSessionId: string | null
    mcpServers: McpServerSpec[]
  }): Promise<SessionHandle>
  prompt(blocks: ContentBlock[]): Promise<StopReason>
  cancel(): Promise<void>
  setMode(modeId: string): Promise<void>
  setModel(modelId: string): Promise<void>
  setConfigOption(id: string, value: string): Promise<SessionConfigOption[] | null>
  setEffort(effort: string | null): Promise<void>
  staticEffortOptions(): Promise<SessionConfigOption | null>
  answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome): Promise<void>
  readonly supportsStructuredAnswers: boolean
}
```

È questa interfaccia a rendere sostenibile la scelta dei quattro driver:
quattro protocolli entrano, **un solo modello di eventi esce**. Il modello
canonico è `SessionUpdate`, portato invariato:

```ts
export type SessionUpdate =
  | { kind: 'userMessageChunk'; content: ContentBlock }
  | { kind: 'agentMessageChunk'; content: ContentBlock }
  | { kind: 'agentThoughtChunk'; content: ContentBlock }
  | { kind: 'toolCall'; call: ToolCall }
  | { kind: 'toolCallUpdate'; update: ToolCallUpdate }
  | { kind: 'plan'; entries: PlanEntry[] }
  | { kind: 'availableCommandsUpdate'; commands: AvailableCommand[] }
  | { kind: 'currentModeUpdate'; modeId: string }
  | { kind: 'usageUpdate'; usage: ContextUsage }
  | { kind: 'unknown'; discriminator: string }
```

### Nessuna migrazione di schema

`chatSession` e `chatItem` esistono già dalla Fase 1, con `acpSessionId`,
`permissionMode`, `selectedModel`, `selectedEffort`, `transportKind`,
`contextUsageUsed`/`Size`. La 5a li usa e basta.

---

## Il punto di design che non è ovvio: due canali, non uno

La via naturale sarebbe aggiungere un tipo a `StateEvent`, come `workspace.layout`
nella 4b. **Per i chunk è sbagliato.**

`agentMessageChunk` arriva **per token**. In Swift questo ha causato un freeze
reale, diagnosticato il 2026-07-24: la coda delle transazioni SwiftUI si
riempiva di eventi per-token dei driver nativi e l'interfaccia si bloccava; il
rimedio fu il coalescing. E la Fase 2 aveva già fissato la regola, scritta nel
commento di `src/shared/protocol.ts`:

> Canale separato ad alto volume: NON passa da StateEvent. L'output PTY arriva
> a raffica e va coalescato al confine IPC, lo stato autoritativo no.

I chunk di chat sono l'output PTY con un altro nome. Quindi:

| Cosa | Canale | Perché |
|---|---|---|
| `toolCall`, `toolCallUpdate`, `plan`, `currentModeUpdate`, `usageUpdate`, `availableCommandsUpdate`, fine turno | `StateEvent` → `chat.update` | rari, autoritativi, ognuno conta |
| `agentMessageChunk`, `agentThoughtChunk`, `userMessageChunk` | canale separato `chat:chunk`, coalescato | a raffica, e conta il testo accumulato, non il singolo pezzo |

Il criterio che separa i due non è il tipo del dato: **è la frequenza**. Se può
arrivare a raffica non passa dallo stato autoritativo.

La coalescenza riusa il meccanismo già in produzione per `pty:output`, non ne
introduce uno nuovo.

---

## Struttura

```
src/shared/chat/
  json-rpc.ts          messaggi JSON-RPC, id, errori
  json-value.ts        valore JSON tipato (il `JSONValue` Swift)
  content-block.ts     blocchi di contenuto (testo, immagine, risorsa)
  session-update.ts    il modello canonico degli eventi
  tool-call.ts         ToolCall / ToolCallUpdate / stati
  permission.ts        PermissionMode, PermissionOutcome
  mcp-config.ts        McpServerSpec e serializzazione

src/main/chat/
  transport/process-transport.ts   spawn via login shell, righe in/out
  transport/http-transport.ts      per OpenCode
  driver.ts                        l'interfaccia AgentDriver
  driver-factory.ts                id → driver, con probe del PATH
  acp/client.ts                    ACP generico: client JSON-RPC
  acp/session.ts                   ACP generico: sessione
  drivers/claude-stream-json.ts
  drivers/codex-app-server.ts
  drivers/opencode-http.ts
  drivers/pi-rpc.ts
  session-manager.ts               ciclo di vita: chi è vivo, per quale worktree
  chat-store.ts                    lettura/scrittura di chatSession (NON chatItem)
```

**`chatSession` è della 5a, `chatItem` è della 5b.** Il confine è netto e vale
la pena scriverlo, perché è il punto in cui le due sotto-fasi si toccano: la
5a persiste la *sessione* — agente, modello, modalità, `acpSessionId` — perché
le serve per riprendere una conversazione dopo un riavvio. Il *contenuto* del
trascritto no: cosa finisca nel `payload` di `chatItem` lo decide il riduttore,
che è della 5b. Se la 5a scrivesse anche gli item dovrebbe inventarsi un
formato che la 5b poi rifarebbe.

Ogni driver è un file solo, fra 300 e 600 righe: la soglia dei 400 del progetto
è indicativa, e spezzare un protocollo in due file lo rende più difficile da
leggere, non meno.

`src/shared/chat/` non importa mai da `src/main/` — vale il vincolo globale,
e serve perché `tillerctl` carica quel livello sotto Node senza bundler.

---

## Superficie di controllo

Nuovi metodi su `ControlRequest`, tutti già nello stile esistente:

| Metodo | Effetto |
|---|---|
| `chat.create` | apre una sessione per un worktree e un agente, restituisce l'id |
| `chat.prompt` | manda blocchi di contenuto, risponde con lo `StopReason` |
| `chat.cancel` | interrompe il turno in corso |
| `chat.setMode` / `chat.setModel` / `chat.setEffort` | configurazione a caldo |
| `chat.answerPermission` | risponde a una richiesta di permesso |
| `chat.get` | stato della sessione (agente, modello, modalità, uso del contesto). **Non** il trascritto: quello è della 5b |
| `chat.close` | chiude la sessione e termina il processo |

E i comandi kebab corrispondenti in `tillerctl`, che è ciò che rende la fase
verificabile senza UI.

---

## Errori

Tre categorie, trattate diversamente:

- **L'agente non è installato.** `driver-factory` fa il probe del binario nel
  PATH della shell di login prima di costruire il driver, e risponde con un
  errore che nomina il binario mancante. Non è un'eccezione: è una risposta.
- **Il processo muore a metà turno.** Il driver emette un evento di fine turno
  con lo `StopReason` di errore e la sessione resta leggibile. Un agente morto
  non deve portarsi via il trascritto.
- **Un messaggio non si interpreta.** `SessionUpdate.unknown` esiste per
  questo: un campo nuovo in una minor del CLI non deve far cadere la
  connessione. L'evento sconosciuto si registra e si va avanti.

**Nessun `catch` vuoto.** Un errore ingoiato in silenzio in questo livello
produce una chat che sembra pensare e non risponde — e in questo progetto il
pattern è già costato due diagnosi lunghe.

---

## Come si verifica

**Test unitari, guidati dalle fixture.** Ogni driver ha le sue registrazioni
dal CLI vero e si testa contro quelle: deterministico, nessun processo, nessuna
quota consumata. È il grosso della copertura.

**Un criterio e2e per driver, con il CLI vero.** Manda un prompt che non
consuma quasi nulla e verifica che arrivi almeno un `agentMessageChunk` e una
fine turno pulita. Cinque criteri in tutto (quattro nativi più omp via ACP).

**Un criterio e2e sul canale.** Un turno lungo non deve consegnare al renderer
un evento per token: si verifica che il numero di consegne sia sensibilmente
minore del numero di chunk prodotti. Senza questo criterio la coalescenza può
sparire senza che nulla diventi rosso — ed è esattamente il difetto che ha
prodotto il freeze in Swift.

**Trappola nota, dalla Fase 3:** un criterio che usa la scorciatoia sbagliata è
vacuo. Qui la scorciatoia sarebbe far girare i criteri contro un finto driver:
quelli provano l'infrastruttura, non i protocolli. I cinque criteri e2e devono
parlare con i CLI veri.

---

## Cosa NON entra nella 5a

- Riduttore del trascritto, timeline, `ToolCallTree`, subagent, diff — **5b**.
- Qualunque componente Svelte — **5c**.
- Registry e installazione degli agenti (`AgentRegistryClient`,
  `AgentInstaller`, `AgentInstallStore`): serve solo agli agenti ACP
  installabili, e con i quattro nativi presenti non blocca nulla. Va in **Fase 7**
  con il resto della piattaforma. Nella 5a `driver-factory` legge i manifest se
  ci sono e ignora l'assenza.
- `FileMentionIndex` (le menzioni `@file` nel composer) — **5c**, è del
  composer.
