# Fase 5a-2 — I quattro driver nativi: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** portare i quattro driver di chat nativi — Claude, Codex, OpenCode,
Pi — dietro l'interfaccia `AgentDriver` già costruita dalla 5a-1, verificabili da
CLI senza una riga di interfaccia.

**Architettura:** ogni driver è un file solo che parla il protocollo del suo CLI e
produce `ChatSessionEvent`. Nessun livello comune sotto l'interfaccia: **non
esiste un ciclo di vita condiviso sul wire**, e fingere che esista è il modo
sicuro di romperli tutti e quattro insieme.

**Spec:** `docs/superpowers/specs/2026-08-02-electron-fase-5a-trasporto-driver-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`. Il repo Swift `~/Desktop/Progetti/tiller/`
  è **sola lettura**.
- `src/shared/chat/` non importa **mai** da `src/main/`: `tillerctl` carica quel
  livello sotto Node senza bundler.
- Nessun `catch` vuoto. Un errore ingoiato a questo livello produce una chat che
  sembra pensare e non risponde — in questo progetto il pattern è già costato due
  diagnosi lunghe.
- `SessionUpdate.unknown` esiste apposta: un campo nuovo in una minor di un CLI
  non deve far cadere la connessione.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits, messaggio in inglese minuscolo imperativo.
- `npm run lint` deve dare **0 errori**.

## Cosa esiste già (5a-1) e cosa manca

Esiste: `json-rpc.ts`, `content-block.ts`, `session-update.ts`, `tool-call.ts`,
`permission.ts`, `mcp-config.ts`, `process-transport.ts`, `driver.ts` (l'interfaccia),
`acp/client.ts`, `acp/session.ts`, `acp/file-system.ts`, `chat-store.ts`,
`session-manager.ts`, i metodi `chat.*` sul socket, i comandi in `tillerctl`, e
quattro criteri e2e (`e2e/fase-5a-chat.spec.ts`).

`driver-factory.ts` conosce **un solo agente**, `omp`, via ACP.

Manca: `transport/http-transport.ts` e i quattro file in `drivers/`.

## La premessa che decide la forma del piano

I quattro CLI non sono varianti dello stesso protocollo. Dalla ricognizione del
codice Swift:

| Driver | Avvio | Connect | Prompt | Fine turno |
|---|---|---|---|---|
| Claude | processo + reader | `control_request initialize` | riga `type=user` | messaggio `result` |
| Codex | client JSON-RPC | `initialize`, `initialized`, `thread/resume\|start` | `turn/start` | notifica `turn/completed` |
| OpenCode | stream SSE pigro | `POST /session` + catalogo provider | `POST /session/:id/message` | SSE idle/completed |
| Pi | processo + reader | cinque comandi RPC in sequenza | comando `prompt` | evento `agent_settled` |

Quattro trasporti, quattro handshake, quattro vocabolari di permessi, quattro
modi di convertire lo **stesso** `ContentBlock`. Perciò: **un task per driver,
ciascuno completo di fixture e test**, e nessun tentativo di fattorizzare un
"driver base" prima di averli scritti tutti e quattro. Se dopo emergerà una parte
comune vera, si estrae allora — non prima, e non a partire dal primo scritto.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/main/chat/transport/http-transport.ts` | *nuovo*: HTTP + SSE con la scala di autenticazione |
| `src/main/chat/drivers/claude-wire.ts` | *nuovo*: schemi delle righe di Claude |
| `src/main/chat/drivers/claude-stream-json.ts` | *nuovo*: il driver |
| `src/main/chat/drivers/codex-app-server.ts` | *nuovo*: il driver, due dialetti di permessi |
| `src/main/chat/drivers/opencode-connection.ts` | *nuovo*: connessione, autenticazione, SSE |
| `src/main/chat/drivers/opencode-http.ts` | *nuovo*: il driver |
| `src/main/chat/drivers/pi-wire.ts` | *nuovo*: schemi rigidi dei messaggi Pi |
| `src/main/chat/drivers/pi-rpc.ts` | *nuovo*: il driver, scritture serializzate |
| `src/main/chat/driver-factory.ts` | *modifica*: i quattro agenti nativi |
| `e2e/fase-5a2-driver.spec.ts` | *nuovo*: un criterio per driver, coi CLI veri |

Ogni driver sta fra le 300 e le 600 righe. La soglia dei 400 del progetto è
indicativa: spezzare un protocollo in due file lo rende più difficile da leggere,
non meno. I file `*-wire.ts` sono l'eccezione utile — lì stanno solo gli schemi.

## Le fixture

`scripts/record-fixture.sh` esiste dalla 5a-1 e registra stdin/stdout di un CLI
in `docs/fixture-chat/<agente>-<versione>/`. Oggi c'è solo `omp-omp-17.2.4`.

Ogni task di driver **registra la propria fixture dal CLI vero** e ci scrive i
test contro. Deterministico, nessun processo nei test, nessuna quota consumata.

**Limite noto, da dichiarare e non aggirare:** lo script registra una pipe
stdin/stdout, quindi **non serve a OpenCode**, che parla HTTP e SSE. Per OpenCode
la fixture è un file di eventi SSE catturato a mano (`curl -N` sull'endpoint degli
eventi) salvato con la stessa convenzione di cartella. Se si finisse per testare
OpenCode contro un finto scritto a memoria invece che contro una registrazione
vera, il test proverebbe il finto — è la trappola già vista nella Fase 3.

---

## Task 1: Trasporto HTTP con la scala di autenticazione

**File:**
- Crea: `src/main/chat/transport/http-transport.ts`
- Test: `src/main/chat/transport/http-transport.test.ts`

**Interfacce:**
- Produce: `httpRequest(opts)` e `sseStream(opts)`, entrambi con la stessa scala di
  autenticazione. Li consuma il Task 4.

- [ ] **Passo 1: scrivere i test che falliscono**

Un server HTTP finto (`node:http`, non una libreria) che rifiuta i primi due
schemi e accetta il terzo, per verificare che la scala scenda tutti i gradini:

```ts
test('la richiesta scende la scala di autenticazione fino a quella accettata', async () => {
  const tentativi: string[] = []
  // server che risponde 401 salvo per Basic ':<password>'
  const esito = await httpRequest({ url, password: 'segreto', method: 'GET', path: '/config' })
  expect(esito.ok).toBe(true)
  expect(tentativi).toEqual([
    'Bearer segreto',
    'Basic ' + Buffer.from('opencode:segreto').toString('base64'),
    'Basic ' + Buffer.from(':segreto').toString('base64')
  ])
})

test('anche lo stream SSE scende la stessa scala', async () => {
  // stesso server, ma sull'endpoint degli eventi
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Tre gradini, in quest'ordine: `Bearer <password>`, poi
`Basic opencode:<password>`, poi `Basic :<password>`.

**Il secondo test è il punto del task.** In Swift il fallback è applicato sia alle
API REST sia al canale SSE, e la ragione è che l'autorizzazione può fallire su uno
e non sull'altro. Un porting che lo mette solo sulla `fetch` produce un OpenCode
che risponde alle richieste e non consegna mai un evento: sintomo «la chat non
risponde», causa «l'SSE è stato rifiutato con 401 e nessuno l'ha guardato».

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/chat/transport/
git commit -m "feat: add an http transport with the opencode auth ladder"
```

---

## Task 2: Il driver di Claude

**File:**
- Crea: `src/main/chat/drivers/claude-wire.ts`, `src/main/chat/drivers/claude-stream-json.ts`
- Test: `src/main/chat/drivers/claude-stream-json.test.ts`
- Fixture: `docs/fixture-chat/claude-<versione>/`

**Riferimento Swift:** `Packages/TillerACP/Sources/TillerACP/Drivers/ClaudeStreamJSONDriver.swift`
(583 righe) e `ClaudeWire.swift` (332).

- [ ] **Passo 1: registrare la fixture dal CLI vero**

```bash
bash scripts/record-fixture.sh claude claude --print --output-format stream-json --verbose
```

Un prompt corto che consumi poco. La versione finisce nel nome della cartella:
una fixture che non dice contro quale versione è stata presa non dice quando è
scaduta.

- [ ] **Passo 2: scrivere i test contro la fixture, e vederli fallire**

Almeno: un `agentMessageChunk` arriva; il turno finisce con uno `StopReason`
pulito; una riga sconosciuta diventa `SessionUpdate.unknown` invece di far cadere
la connessione.

- [ ] **Passo 3: implementare**

Punti da non sbagliare, tutti verificati nel codice Swift:

- Il processo si lancia con `/bin/zsh -lc`, non con l'eseguibile diretto
  (`ClaudeStreamJSONDriver.swift:88`). Vale per tutti e quattro: il PATH degli
  agenti vive nella shell di login dell'utente, e senza quella un binario
  installato con un version manager risulta assente.
- `connect` manda un `control_request initialize` e attende la risposta.
- Il prompt è una riga con `type: "user"`.
- La fine turno è il messaggio `result`, e `turnEnded` si emette **dopo** aver
  consegnato gli aggiornamenti che lo compongono: è il marcatore finale del turno,
  non un evento riordinabile.
- I decoder sono permissivi (molti campi mancanti diventano stringa vuota o array
  vuoto). Permissivo nel decoder **non** significa opzionale nel protocollo:
  copiare la permissività senza capire quali campi contano nasconde gli errori
  veri dietro valori vuoti plausibili.
- `fullAuto` per Claude **non è un flag**: si lancia in `acceptEdits` e il driver
  approva da sé i `can_use_tool` (`ClaudeStreamJSONDriver.swift`, sezione
  permessi). Portarlo come flag inesistente lo fa fallire in silenzio.
- L'effort si passa come **prefisso testuale**, non come parametro.
- La conversione dei `ContentBlock` usa i tag della Messages API e trasforma i
  resource link in `@percorso`. **Non riusare la conversione di un altro driver:**
  i quattro convertono lo stesso blocco in quattro modi incompatibili.
- EOF ed errore di trasporto si trattano allo stesso modo: entrambi
  `disconnected`.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/chat/drivers/claude-wire.ts src/main/chat/drivers/claude-stream-json.ts src/main/chat/drivers/claude-stream-json.test.ts docs/fixture-chat/
git commit -m "feat: drive claude over stream json"
```

---

## Task 3: Il driver di Codex

**File:**
- Crea: `src/main/chat/drivers/codex-app-server.ts`
- Test: `src/main/chat/drivers/codex-app-server.test.ts`
- Fixture: `docs/fixture-chat/codex-<versione>/`

**Riferimento Swift:** `Drivers/CodexAppServerDriver.swift` (490 righe).

- [ ] **Passo 1: registrare la fixture**, poi scrivere i test e vederli falliti.

- [ ] **Passo 2: implementare**

- Handshake: `initialize`, `initialized`, poi `thread/resume` **con fallback
  silenzioso a `thread/start`**. Il fallback non è difensivo a caso: un thread che
  non esiste più è normale, non è un errore da mostrare.
- Prompt: `turn/start`. Fine turno: notifica `turn/completed`.
- **Due dialetti di permessi.** Codex espone sia i metodi legacy sia quelli
  moderni dell'app-server, e cambiano *entrambi* i lati:
  - richiesta: due nomi diversi, vanno riconosciuti tutti e due;
  - risposta: legacy `approved`/`denied`, moderno `accept`/`decline`/`cancel`.

  Riconoscerne uno solo produce un permesso che resta appeso per sempre su metà
  delle versioni installate.
- `supportsStructuredAnswers` è **falso**: Codex ignora l'`updatedInput` di
  `PermissionOutcome.answered`. Dichiararlo vero fa sparire in silenzio la
  modifica dell'utente.
- `fullAuto` diventa approval policy `never` più sandbox `workspace-write`.
- Codex espone **solo il modello selezionato**, non un catalogo: il campo dei
  modelli disponibili resta vuoto, e non è un difetto da colmare inventando una
  lista.
- L'effort è un parametro nativo di `turn/start`.
- I blocchi ACP si mandano codificati direttamente.

- [ ] **Passo 3: eseguire e vedere passare** → PASS.

- [ ] **Passo 4: committare**

```bash
git commit -m "feat: drive codex over its app server"
```

---

## Task 4: Il driver di OpenCode

**File:**
- Crea: `src/main/chat/drivers/opencode-connection.ts`, `src/main/chat/drivers/opencode-http.ts`
- Test: `src/main/chat/drivers/opencode-http.test.ts`
- Fixture: `docs/fixture-chat/opencode-<versione>/` (eventi SSE catturati, vedi sopra)

**Riferimento Swift:** `Drivers/OpenCodeHTTPDriver.swift` (530) e
`OpenCodeConnection.swift` (271).

**Interfacce:**
- Consuma: `httpRequest` e `sseStream` (Task 1).

- [ ] **Passo 1: catturare la fixture SSE**, poi test rossi.

- [ ] **Passo 2: implementare**

- Il server si avvia con la shell di login (`OpenCodeConnection.swift:128`); lo
  stream SSE è **pigro**, si apre quando serve.
- Sessione: `POST /session`, oppure un id già noto — che si assume valido, senza
  verifica.
- Prompt: `POST /session/:id/message`. Fine turno: evento SSE idle/completed.
- **Deduplica obbligatoria.** OpenCode rimanda lo **stesso** tool part a ogni tick
  SSE. Il confronto è su `status`, `input`, `output`, `error`: senza, ogni tick
  riapplica lo stesso aggiornamento e il costo di rendering si moltiplica. È lo
  stesso difetto che in Swift ha prodotto il freeze di OpenCode.
- Una chiusura SSE **ordinata** si tratta come errore di trasporto a livello di
  sessione: un `disconnected`, non un turno finito bene.
- I modelli si derivano da `/config/providers`; l'effort è il `variant`.
- `fullAuto` diventa permission `* -> allow`.
- Le immagini si mandano come `file` con data URL.

- [ ] **Passo 3: eseguire e vedere passare** → PASS.

- [ ] **Passo 4: committare**

```bash
git commit -m "feat: drive opencode over http and sse"
```

---

## Task 5: Il driver di Pi

**File:**
- Crea: `src/main/chat/drivers/pi-wire.ts`, `src/main/chat/drivers/pi-rpc.ts`
- Test: `src/main/chat/drivers/pi-rpc.test.ts`
- Fixture: `docs/fixture-chat/pi-<versione>/`

**Riferimento Swift:** `Drivers/PiRPCDriver.swift` (755 righe, il più grande) e
`PiWire.swift` (72).

- [ ] **Passo 1: registrare la fixture**, poi test rossi.

- [ ] **Passo 2: implementare**

- `connect` è una **sequenza di cinque comandi RPC**, non uno.
- Prompt: comando `prompt`. Fine turno: evento `agent_settled`, emesso dentro la
  chiusura del run **prima** di risolvere la promessa di `prompt`.
- **Le scritture vanno serializzate, e l'ordine è parte del protocollo.** Il
  prompt successivo non deve sorpassare l'abort; le risposte all'interfaccia non
  devono sorpassare comandi già in coda. In Swift la serializzazione avviene
  **prima** che il chiamante async si sospenda. Una semplice catena di `Promise`
  non è equivalente: `p = p.then(() => scrivi())` accoda la *chiamata*, ma il
  momento in cui la riga entra davvero nella pipe dipende da quando gira il
  microtask, e due comandi emessi nello stesso tick possono invertirsi. Serve una
  coda esplicita con l'ordine deciso al momento dell'accodamento.
- Gli schemi di `pi-wire.ts` sono **rigidi**, al contrario di quelli di Claude:
  oggetto al livello superiore, `type` stringa, e per una risposta `command` e
  `success` obbligatori (`PiWire.swift:21`). Rendere questi schemi permissivi per
  uniformarli a Claude trasforma un messaggio malformato in un evento vuoto che
  attraversa tutto il sistema.
- Modelli da `get_available_models`, sforzo da `get_available_thinking_levels` e
  `set_thinking_level`.
- Pi è l'unico che dichiara `loadSession: true`; il resume passa da `--session` al
  lancio e poi da `get_state`.
- Permessi: risposta con `value`, `confirmed` o `cancelled` alla UI extension.
- La conversione dei blocchi concatena risorse e testo in **una stringa**, e tiene
  le immagini in un array separato.

- [ ] **Passo 3: eseguire e vedere passare** → PASS.

- [ ] **Passo 4: committare**

```bash
git commit -m "feat: drive pi over its rpc protocol"
```

---

## Task 6: La fabbrica conosce i quattro agenti

**File:**
- Modifica: `src/main/chat/driver-factory.ts`
- Test: `src/main/chat/driver-factory.test.ts`

**Questo task esiste perché la Fase 4a è stata un motore corretto che nessuno
chiamava.** Quindici task costruivano viste che consumavano un layout, e nessuno
lo procurava dal main; se ne sono accorti solo i criteri e2e. Quattro driver
perfetti che la fabbrica non sa costruire sono lo stesso difetto.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test.each(['claude', 'codex', 'opencode', 'pi'])(
  'la fabbrica costruisce il driver di %s quando il binario c e',
  (agentId) => {
    const esito = makeDriver({ agentId, worktreePath: '/tmp', pathProbe: () => true })
    expect(esito.ok).toBe(true)
  }
)

test('un binario assente diventa un motivo, non un errore', () => {
  const esito = makeDriver({ agentId: 'claude', worktreePath: '/tmp', pathProbe: () => false })
  expect(esito).toEqual({ ok: false, reason: expect.stringContaining('claude') })
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare.** Il probe del PATH usa la shell di login, come già
  fa `defaultPathProbe`. L'agente non installato è una **risposta**, non
  un'eccezione: l'errore nomina il binario mancante.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git commit -m "feat: build the native drivers from the factory"
```

---

## Task 7: I criteri end-to-end, coi CLI veri

**File:**
- Crea: `e2e/fase-5a2-driver.spec.ts`

I quattro criteri della 5a-1 (`e2e/fase-5a-chat.spec.ts`) restano: prompt a omp,
agente sconosciuto, sessione che sopravvive alla chiusura della finestra, pezzi di
testo che non arrivano uno per volta.

- [ ] **Passo 1: scrivere i quattro criteri nuovi**

Uno per driver nativo. Ciascuno manda un prompt che consuma quasi nulla e
verifica **due** cose: che arrivi almeno un `agentMessageChunk`, e che il turno
finisca in modo pulito.

- [ ] **Passo 2: eseguirli**

```bash
npx electron-vite build && npx playwright test e2e/fase-5a2-driver.spec.ts
```

**La trappola da non prendere.** La scorciatoia sarebbe far girare questi criteri
contro un driver finto: proverebbero l'infrastruttura, non i protocolli — e i
protocolli sono l'unica cosa che questa fase aggiunge. I quattro criteri devono
parlare coi CLI veri.

Se un CLI non è installato sulla macchina, il criterio si **salta dichiarandolo**
(`test.skip` con il motivo), non si trasforma in un test contro un finto. Un
criterio saltato è un buco noto; un criterio verde contro un finto è un buco
nascosto.

- [ ] **Passo 3: mutazioni di verifica**

Una per volta, ricostruendo ogni volta:

| Mutazione | Rosso atteso |
| --- | --- |
| la fabbrica restituisce sempre il driver ACP | **tutti e quattro** i criteri nuovi |
| la deduplica di OpenCode viene tolta | il criterio di OpenCode **non** diventa rosso — vedi sotto |
| la coda delle scritture di Pi diventa una `Promise` concatenata | il criterio di Pi, **a intermittenza** — vedi sotto |

Le ultime due righe sono la parte onesta della tabella. La deduplica di OpenCode
si vede nel **costo**, non nel risultato: senza, gli stessi aggiornamenti
arrivano molte volte e la chat resta corretta finché non si impunta. Il criterio
sui pezzi di testo della 5a-1 misura qualcosa di simile per il canale, ma non
copre questo. L'ordine delle scritture di Pi produce un difetto di
concorrenza, e un criterio che dipende da un intreccio non è un criterio: è un
generatore di falsi verdi.

Entrambi si coprono con **test di unità sulla fixture** — quanti aggiornamenti
distinti escono da N tick identici, in che ordine escono i comandi da una raffica
— non con criteri e2e. Va scritto qui perché è una scelta, non una dimenticanza.

- [ ] **Passo 4: committare**

```bash
git commit -m "test: end-to-end criteria for the native drivers"
```

---

## Chiusura

Al termine riportare: l'elenco dei commit; l'esito di `npx vitest run
src/main/chat/`; `npm run typecheck` e `npm run lint` (0 errori); l'esito di ogni
mutazione col criterio diventato rosso; **quali criteri sono stati saltati e
perché**; ogni punto del piano trovato sbagliato; e ciò che non si è riusciti a
verificare, dicendolo invece di ometterlo.

Fuori scope, già deciso nella spec: riduttore del trascritto, timeline,
`ToolCallTree`, subagent, diff (**5b**); qualunque componente Svelte (**5c**);
registry e installazione degli agenti (**Fase 7**); `FileMentionIndex` (**5c**).
