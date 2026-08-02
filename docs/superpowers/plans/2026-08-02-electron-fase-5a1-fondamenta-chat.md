# Fase 5a-1 — Fondamenta della chat: piano di implementazione

> **Per esecutori agentici:** SOTTO-SKILL RICHIESTA: usare
> superpowers:subagent-driven-development (consigliato) o
> superpowers:executing-plans per implementare un task alla volta.

**Obiettivo:** il livello che parla con un agente di chat — JSON-RPC, trasporto
di processo, interfaccia dei driver, ACP generico, superficie di controllo —
fino a chattare con **omp** da riga di comando, senza interfaccia.

**Architettura:** i driver vivono nel processo main, perché le sessioni si
persistono e chiudere la finestra non deve uccidere gli agenti. Un'unica
interfaccia `AgentDriver` con un modello di eventi canonico: qui la implementa
solo l'ACP generico, nella 5a-2 anche i quattro driver nativi.

**Stack:** TypeScript, zod, node child_process, vitest, Playwright.

**Repo:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
`~/Desktop/Progetti/tiller` è **sola lettura**.

**Spec:** `docs/2026-08-02-electron-fase-5a-trasporto-driver-design.md`.

**Dipende da:** Fase 1 (`chatSession` e `chatItem` già nello schema), Fase 2
(la regola del canale ad alto volume e il coalescer di `pty:output`), Fase 4b
(`dispatch.ts` e l'emissione di `StateEvent`).

## Vincoli globali

- **`src/shared/` non importa mai da `src/main/`.** È il livello comune a main,
  renderer e CLI. L'inversione è già costata un difetto (commit `c78052f`).
- **Import relativi in `src/shared/` con estensione `.ts` esplicita.** Quel
  livello lo carica anche `tillerctl` sotto Node senza bundler.
- **Il gate è `bash scripts/ci.sh`, e stampa `CI OK`.** Non sostituirlo con i
  singoli sotto-comandi: esiste una classe di difetti che passa vitest, tsc ed
  eslint e rompe `tillerctl` a runtime, e solo gli e2e la prende.
- **`pnpm typecheck:node && pnpm typecheck:cli` + eslint PRIMA di ogni
  commit.** vitest NON fa typecheck: esbuild cancella i tipi senza
  verificarli, e un `import type` verso un simbolo inesistente passa centinaia
  di test verdi ed entra in un commit rotto.
- **`@typescript-eslint/explicit-function-return-type` vale anche sui test**:
  ogni funzione di supporto vuole il tipo di ritorno.
- **Non indebolire mai un tipo di produzione per far compilare un finto di
  test.** Se un finto non compila, è il finto a essere incompleto.
- **Nessun `catch` vuoto.** Un errore ingoiato qui produce una chat che sembra
  pensare e non risponde. Se un errore va tollerato, si registra.
- **Stringhe dell'interfaccia in inglese**, anche se piano e commenti sono in
  italiano.
- **Le tabelle esistono, le righe no**: le foreign key vogliono i record padre
  (`project` → `worktree` → `chatSession`).
- Baseline all'inizio della fase: gate verde con la Fase 4b completa.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/shared/chat/json-rpc.ts` | messaggi JSON-RPC 2.0: analisi e serializzazione |
| `src/shared/chat/content-block.ts` | blocchi di contenuto, con ripiego su ignoto |
| `src/shared/chat/session-update.ts` | il modello canonico degli eventi |
| `src/shared/chat/tool-call.ts` | chiamate a strumento e loro aggiornamenti |
| `src/shared/chat/permission.ts` | modalità e esiti dei permessi |
| `src/shared/chat/mcp-config.ts` | specifica dei server MCP |
| `src/main/chat/process-transport.ts` | processo figlio, righe entranti e uscenti |
| `src/main/chat/driver.ts` | l'interfaccia `AgentDriver` e i suoi eventi |
| `src/main/chat/acp/client.ts` | endpoint JSON-RPC: correla richieste e risposte |
| `src/main/chat/acp/session.ts` | l'ACP generico come `AgentDriver` |
| `src/main/chat/acp/file-system.ts` | serve le richieste di file dell'agente |
| `src/main/chat/driver-factory.ts` | da id di agente a driver, con probe del PATH |
| `src/main/chat/session-manager.ts` | quali sessioni sono vive, per quale worktree |
| `src/main/chat/chat-store.ts` | lettura e scrittura di `chatSession` |
| `Scripts/record-fixture.sh` | registra una conversazione da un CLI vero |

Modificati: `src/shared/protocol.ts` (metodi `chat.*` ed evento
`chat.update`), `src/main/control/dispatch.ts`, `src/main/index.ts` (canale
`chat:chunk`), `src/preload/index.ts`, `cli/args.ts`.

**`JSONValue` non si porta.** In Swift erano 85 righe per modellare a mano ciò
che `JSON.parse` restituisce nativamente, più 26 righe di codifica per
`JSONRPCID`, che qui è `number | string`. Portarle sarebbe tradurre la lingua
invece del significato.

**Due scostamenti dalla struttura dello spec, entrambi voluti:**

- `http-transport.ts` **non è qui**. Serve solo a OpenCode, che è un driver
  nativo, quindi va nella 5a-2 insieme al suo unico consumatore. Metterlo qui
  significherebbe scrivere un trasporto senza nessuno che lo usi, e nella 4b
  abbiamo già visto cosa succede al codice che nessuno chiama.
- `acp/file-system.ts` **c'è e nello spec non era elencato**. È una lacuna
  dello spec, non un'aggiunta di comodo: l'ACP prevede che sia l'agente a
  chiedere al client di leggere e scrivere file, e senza questo modulo la
  sessione non sa rispondere a `fs/read_text_file`. Un agente che non riceve
  risposta resta bloccato per sempre.

---

## Task 1: messaggi JSON-RPC

**File:**
- Crea: `src/shared/chat/json-rpc.ts`
- Test: `src/shared/chat/json-rpc.test.ts`

**Interfacce:**
- Consuma: `zod` (già in dipendenze).
- Produce: `type JSONRPCID = number | string`;
  `interface JSONRPCErrorObject { code: number; message: string; data?: unknown }`;
  `type JSONRPCMessage` (unione discriminata su `kind`);
  `decodeMessage(line: string): JSONRPCMessage | { kind: 'invalid'; reason: string }`;
  `encodeLine(message: JSONRPCMessage): string`.

**Contesto:** una riga sul filo è un oggetto JSON che può essere una richiesta
(ha `method` e `id`), una notifica (`method` senza `id`) o una risposta (`id`
senza `method`). La discriminazione **non** è un campo: si deduce dalla
presenza dei campi. È il punto in cui uno schema zod ingenuo sbaglia, quindi
l'analisi è scritta a mano e zod valida i pezzi.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { decodeMessage, encodeLine } from './json-rpc.ts'

test('una riga con method e id e una richiesta', () => {
  const m = decodeMessage('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"a":1}}')
  expect(m).toEqual({ kind: 'request', id: 1, method: 'initialize', params: { a: 1 } })
})

test('una riga con method e senza id e una notifica', () => {
  const m = decodeMessage('{"jsonrpc":"2.0","method":"session/update","params":{}}')
  expect(m).toEqual({ kind: 'notification', method: 'session/update', params: {} })
})

test('una riga con id e senza method e una risposta', () => {
  const m = decodeMessage('{"jsonrpc":"2.0","id":"a","result":{"ok":true}}')
  expect(m).toEqual({ kind: 'response', id: 'a', result: { ok: true }, error: null })
})

test('un id puo essere numero o stringa', () => {
  expect(decodeMessage('{"id":7,"result":null}')).toMatchObject({ id: 7 })
  expect(decodeMessage('{"id":"sette","result":null}')).toMatchObject({ id: 'sette' })
})

test('una riga senza method e senza id non e interpretabile', () => {
  // Non lancia: il livello superiore deve poter registrare e proseguire.
  expect(decodeMessage('{"jsonrpc":"2.0"}')).toEqual({
    kind: 'invalid',
    reason: 'messaggio senza method e senza id'
  })
})

test('una riga che non e JSON non e interpretabile', () => {
  expect(decodeMessage('non json')).toMatchObject({ kind: 'invalid' })
})

test('la riga codificata finisce con un a capo e riparsa uguale', () => {
  const originale = { kind: 'request', id: 3, method: 'x', params: { y: 'z' } } as const
  const riga = encodeLine(originale)
  expect(riga.endsWith('\n')).toBe(true)
  expect(decodeMessage(riga)).toEqual(originale)
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- json-rpc`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
export type JSONRPCID = number | string

export interface JSONRPCErrorObject {
  code: number
  message: string
  data?: unknown
}

export type JSONRPCMessage =
  | { kind: 'request'; id: JSONRPCID; method: string; params: unknown }
  | { kind: 'notification'; method: string; params: unknown }
  | { kind: 'response'; id: JSONRPCID; result: unknown; error: JSONRPCErrorObject | null }

export type DecodedMessage = JSONRPCMessage | { kind: 'invalid'; reason: string }

function isId(value: unknown): value is JSONRPCID {
  return typeof value === 'number' || typeof value === 'string'
}

export function decodeMessage(line: string): DecodedMessage {
  let grezzo: unknown
  try {
    grezzo = JSON.parse(line)
  } catch {
    return { kind: 'invalid', reason: 'riga non interpretabile come JSON' }
  }
  if (typeof grezzo !== 'object' || grezzo === null || Array.isArray(grezzo)) {
    return { kind: 'invalid', reason: 'il JSON di primo livello non e un oggetto' }
  }
  const campi = grezzo as Record<string, unknown>
  const id = isId(campi.id) ? campi.id : null

  if (typeof campi.method === 'string') {
    if (id === null) return { kind: 'notification', method: campi.method, params: campi.params }
    return { kind: 'request', id, method: campi.method, params: campi.params }
  }
  if (id === null) return { kind: 'invalid', reason: 'messaggio senza method e senza id' }
  const errore = campi.error
  return {
    kind: 'response',
    id,
    result: campi.result,
    error: typeof errore === 'object' && errore !== null
      ? (errore as JSONRPCErrorObject)
      : null
  }
}

export function encodeLine(message: JSONRPCMessage): string {
  const base: Record<string, unknown> = { jsonrpc: '2.0' }
  if (message.kind === 'request') {
    base.id = message.id
    base.method = message.method
    base.params = message.params
  } else if (message.kind === 'notification') {
    base.method = message.method
    base.params = message.params
  } else {
    base.id = message.id
    if (message.error !== null) base.error = message.error
    else base.result = message.result
  }
  return `${JSON.stringify(base)}\n`
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- json-rpc`
Atteso: PASS, 7 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/chat/
git commit -m "feat: messaggi json-rpc per la chat"
```

---

## Task 2: blocchi di contenuto

**File:**
- Crea: `src/shared/chat/content-block.ts`
- Test: `src/shared/chat/content-block.test.ts`

**Interfacce:**
- Produce: `ContentBlockSchema` (zod), `type ContentBlock`,
  `decodeContentBlock(value: unknown): ContentBlock`,
  `encodeContentBlock(block: ContentBlock): unknown`.

**Contesto:** un blocco è discriminato dal campo `type` sul filo. Un tipo
sconosciuto **non deve far cadere niente**: diventa `{ kind: 'unknown' }`. È la
stessa scelta che in Swift ha permesso ai CLI di aggiungere tipi senza rompere
Tiller, e vale a maggior ragione qui, dove i protocolli si ri-registrano
proprio perché cambiano.

Attenzione a `resource`: sul filo è annidato — `{type:"resource", resource:{uri, text}}` —
mentre `resource_link` è piatto. L'asimmetria è del protocollo, non un errore.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { decodeContentBlock, encodeContentBlock } from './content-block.ts'

test('un blocco di testo si interpreta', () => {
  expect(decodeContentBlock({ type: 'text', text: 'ciao' })).toEqual({
    kind: 'text',
    text: 'ciao'
  })
})

test('una risorsa incorporata e annidata sul filo', () => {
  expect(
    decodeContentBlock({ type: 'resource', resource: { uri: 'file:///a', text: 'x' } })
  ).toEqual({ kind: 'resource', uri: 'file:///a', text: 'x' })
})

test('un collegamento a risorsa e piatto sul filo', () => {
  expect(decodeContentBlock({ type: 'resource_link', uri: 'file:///b', name: 'b' })).toEqual({
    kind: 'resourceLink',
    uri: 'file:///b',
    name: 'b'
  })
})

test('un tipo sconosciuto non fa cadere niente', () => {
  // Un CLI che aggiunge un tipo in una minor non deve rompere la chat.
  expect(decodeContentBlock({ type: 'video', src: 'x' })).toEqual({
    kind: 'unknown',
    type: 'video'
  })
})

test('testo e risorsa sopravvivono al giro completo', () => {
  for (const blocco of [
    { kind: 'text', text: 'a' },
    { kind: 'resource', uri: 'file:///a', text: 'b' },
    { kind: 'image', mimeType: 'image/png', data: 'AAAA' }
  ] as const) {
    expect(decodeContentBlock(encodeContentBlock(blocco))).toEqual(blocco)
  }
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- content-block`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'

export type ContentBlock =
  | { kind: 'text'; text: string }
  | { kind: 'image'; mimeType: string; data: string }
  | { kind: 'resourceLink'; uri: string; name: string }
  | { kind: 'resource'; uri: string; text: string }
  | { kind: 'unknown'; type: string }

const TestoSchema = z.object({ type: z.literal('text'), text: z.string() })
const ImmagineSchema = z.object({
  type: z.literal('image'),
  mimeType: z.string(),
  data: z.string()
})
const CollegamentoSchema = z.object({
  type: z.literal('resource_link'),
  uri: z.string(),
  name: z.string()
})
const RisorsaSchema = z.object({
  type: z.literal('resource'),
  resource: z.object({ uri: z.string(), text: z.string() })
})

export function decodeContentBlock(value: unknown): ContentBlock {
  const testo = TestoSchema.safeParse(value)
  if (testo.success) return { kind: 'text', text: testo.data.text }

  const immagine = ImmagineSchema.safeParse(value)
  if (immagine.success) {
    return { kind: 'image', mimeType: immagine.data.mimeType, data: immagine.data.data }
  }

  const collegamento = CollegamentoSchema.safeParse(value)
  if (collegamento.success) {
    return { kind: 'resourceLink', uri: collegamento.data.uri, name: collegamento.data.name }
  }

  const risorsa = RisorsaSchema.safeParse(value)
  if (risorsa.success) {
    return { kind: 'resource', uri: risorsa.data.resource.uri, text: risorsa.data.resource.text }
  }

  const tipo =
    typeof value === 'object' && value !== null && typeof (value as { type?: unknown }).type === 'string'
      ? (value as { type: string }).type
      : 'unknown'
  return { kind: 'unknown', type: tipo }
}

export function encodeContentBlock(block: ContentBlock): unknown {
  switch (block.kind) {
    case 'text':
      return { type: 'text', text: block.text }
    case 'image':
      return { type: 'image', mimeType: block.mimeType, data: block.data }
    case 'resourceLink':
      return { type: 'resource_link', uri: block.uri, name: block.name }
    case 'resource':
      return { type: 'resource', resource: { uri: block.uri, text: block.text } }
    case 'unknown':
      return { type: block.type }
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- content-block`
Atteso: PASS, 5 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/chat/
git commit -m "feat: blocchi di contenuto della chat"
```

---

## Task 3: chiamate a strumento

**File:**
- Crea: `src/shared/chat/tool-call.ts`
- Test: `src/shared/chat/tool-call.test.ts`

**Interfacce:**
- Consuma: `ContentBlock` dal Task 2.
- Produce: `type ToolCallStatus = 'pending' | 'inProgress' | 'completed' | 'failed'`;
  `interface ToolCall { id: string; title: string; kind: string; status: ToolCallStatus; content: ContentBlock[]; locations: ToolCallLocation[]; rawInput: unknown }`;
  `interface ToolCallUpdate` (stessi campi, tutti opzionali tranne `id`);
  `decodeToolCall(value: unknown): ToolCall | null`;
  `decodeToolCallUpdate(value: unknown): ToolCallUpdate | null`;
  `applyToolCallUpdate(call: ToolCall, update: ToolCallUpdate): ToolCall`.

**Contesto:** una chiamata a strumento nasce e poi viene aggiornata più volte
— cambia stato, si aggiunge contenuto. L'aggiornamento è **parziale**: porta
solo i campi cambiati, e i campi assenti vanno lasciati com'erano. Confondere
"assente" con "svuotato" cancella il contenuto già arrivato, ed è l'errore
tipico di questo livello.

`applyToolCallUpdate` restituisce una copia nuova, non muta l'originale:
vincolo di immutabilità del progetto.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { applyToolCallUpdate, decodeToolCall, type ToolCall } from './tool-call.ts'

function chiamata(): ToolCall {
  return {
    id: 't1',
    title: 'Read file',
    kind: 'read',
    status: 'pending',
    content: [{ kind: 'text', text: 'parziale' }],
    locations: [],
    rawInput: null
  }
}

test('una chiamata a strumento si interpreta', () => {
  const c = decodeToolCall({
    toolCallId: 't1',
    title: 'Read file',
    kind: 'read',
    status: 'pending'
  })
  expect(c).toMatchObject({ id: 't1', title: 'Read file', status: 'pending' })
})

test('un aggiornamento cambia solo i campi che porta', () => {
  const dopo = applyToolCallUpdate(chiamata(), { id: 't1', status: 'completed' })
  expect(dopo.status).toBe('completed')
  // La trappola: un campo assente NON e un campo svuotato.
  expect(dopo.title).toBe('Read file')
  expect(dopo.content).toEqual([{ kind: 'text', text: 'parziale' }])
})

test('un aggiornamento con contenuto lo sostituisce', () => {
  const dopo = applyToolCallUpdate(chiamata(), {
    id: 't1',
    content: [{ kind: 'text', text: 'finale' }]
  })
  expect(dopo.content).toEqual([{ kind: 'text', text: 'finale' }])
})

test('applicare un aggiornamento non muta l originale', () => {
  const prima = chiamata()
  applyToolCallUpdate(prima, { id: 't1', status: 'failed' })
  expect(prima.status).toBe('pending')
})

test('uno stato sconosciuto non fa cadere l interpretazione', () => {
  const c = decodeToolCall({ toolCallId: 't2', status: 'teleporting' })
  expect(c?.status).toBe('pending')
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- tool-call`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'
import { decodeContentBlock, type ContentBlock } from './content-block.ts'

export type ToolCallStatus = 'pending' | 'inProgress' | 'completed' | 'failed'

export interface ToolCallLocation {
  path: string
  line: number | null
}

export interface ToolCall {
  id: string
  title: string
  kind: string
  status: ToolCallStatus
  content: ContentBlock[]
  locations: ToolCallLocation[]
  rawInput: unknown
}

export interface ToolCallUpdate {
  id: string
  title?: string
  kind?: string
  status?: ToolCallStatus
  content?: ContentBlock[]
  locations?: ToolCallLocation[]
  rawInput?: unknown
}

const STATI: Record<string, ToolCallStatus> = {
  pending: 'pending',
  in_progress: 'inProgress',
  completed: 'completed',
  failed: 'failed'
}

const GrezzoSchema = z.object({
  toolCallId: z.string().min(1),
  title: z.string().optional(),
  kind: z.string().optional(),
  status: z.string().optional(),
  content: z.array(z.unknown()).optional(),
  locations: z
    .array(z.object({ path: z.string(), line: z.number().int().nullable().optional() }))
    .optional(),
  rawInput: z.unknown().optional()
})

function posizioni(grezze: { path: string; line?: number | null }[] | undefined): ToolCallLocation[] {
  return (grezze ?? []).map((p) => ({ path: p.path, line: p.line ?? null }))
}

export function decodeToolCall(value: unknown): ToolCall | null {
  const analisi = GrezzoSchema.safeParse(value)
  if (!analisi.success) return null
  const g = analisi.data
  return {
    id: g.toolCallId,
    title: g.title ?? '',
    kind: g.kind ?? 'other',
    // Uno stato che non conosciamo non e un errore: e un CLI piu nuovo di noi.
    status: STATI[g.status ?? 'pending'] ?? 'pending',
    content: (g.content ?? []).map(decodeContentBlock),
    locations: posizioni(g.locations),
    rawInput: g.rawInput ?? null
  }
}

export function decodeToolCallUpdate(value: unknown): ToolCallUpdate | null {
  const analisi = GrezzoSchema.safeParse(value)
  if (!analisi.success) return null
  const g = analisi.data
  const aggiornamento: ToolCallUpdate = { id: g.toolCallId }
  if (g.title !== undefined) aggiornamento.title = g.title
  if (g.kind !== undefined) aggiornamento.kind = g.kind
  if (g.status !== undefined) aggiornamento.status = STATI[g.status] ?? 'pending'
  if (g.content !== undefined) aggiornamento.content = g.content.map(decodeContentBlock)
  if (g.locations !== undefined) aggiornamento.locations = posizioni(g.locations)
  if (g.rawInput !== undefined) aggiornamento.rawInput = g.rawInput
  return aggiornamento
}

/** Copia nuova: i campi assenti nell aggiornamento restano quelli di prima. */
export function applyToolCallUpdate(call: ToolCall, update: ToolCallUpdate): ToolCall {
  return {
    id: call.id,
    title: update.title ?? call.title,
    kind: update.kind ?? call.kind,
    status: update.status ?? call.status,
    content: update.content ?? call.content,
    locations: update.locations ?? call.locations,
    rawInput: update.rawInput ?? call.rawInput
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- tool-call`
Atteso: PASS, 5 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/chat/
git commit -m "feat: chiamate a strumento e loro aggiornamenti"
```

---

## Task 4: il modello canonico degli eventi

**File:**
- Crea: `src/shared/chat/session-update.ts`
- Test: `src/shared/chat/session-update.test.ts`

**Interfacce:**
- Consuma: `ContentBlock` (Task 2), `ToolCall`/`ToolCallUpdate` (Task 3).
- Produce: `type SessionUpdate` (unione discriminata su `kind`),
  `decodeSessionUpdate(value: unknown): SessionUpdate`;
  `interface PlanEntry { content: string; priority: string; status: string }`;
  `interface AvailableCommand { name: string; description: string }`;
  `interface ContextUsage { used: number; size: number }`;
  `type StopReason = 'endTurn' | 'maxTokens' | 'refusal' | 'cancelled' | 'error'`;
  `isChunk(update: SessionUpdate): boolean`.

**Contesto:** questo è **il pezzo centrale della fase**. Quattro protocolli
diversi entreranno nella 5a-2 e usciranno tutti da qui: se questo tipo è giusto,
i driver sono traduttori; se è sbagliato, ogni driver si inventa una variante.

`isChunk` esiste per il Task 13 e non è un dettaglio: è la funzione che decide
quali eventi passano dal canale ad alto volume invece che dallo stato
autoritativo. Vive qui, accanto al tipo, perché è una proprietà dell'evento —
non della sua consegna.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { decodeSessionUpdate, isChunk } from './session-update.ts'

test('un pezzo di messaggio dell agente si interpreta', () => {
  expect(
    decodeSessionUpdate({
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text: 'ciao' }
    })
  ).toEqual({ kind: 'agentMessageChunk', content: { kind: 'text', text: 'ciao' } })
})

test('un piano si interpreta con le sue voci', () => {
  const u = decodeSessionUpdate({
    sessionUpdate: 'plan',
    entries: [{ content: 'fare x', priority: 'high', status: 'pending' }]
  })
  expect(u).toMatchObject({ kind: 'plan' })
})

test('l uso del contesto si interpreta', () => {
  expect(decodeSessionUpdate({ sessionUpdate: 'usage_update', used: 10, size: 100 })).toEqual({
    kind: 'usageUpdate',
    usage: { used: 10, size: 100 }
  })
})

test('un aggiornamento sconosciuto conserva il discriminante', () => {
  // Un campo nuovo in una minor del CLI non deve far cadere la connessione.
  expect(decodeSessionUpdate({ sessionUpdate: 'telepathy_chunk' })).toEqual({
    kind: 'unknown',
    discriminator: 'telepathy_chunk'
  })
})

test('solo i pezzi di testo sono a raffica', () => {
  // La distinzione che decide il canale: non il tipo di dato, la frequenza.
  expect(isChunk({ kind: 'agentMessageChunk', content: { kind: 'text', text: 'a' } })).toBe(true)
  expect(isChunk({ kind: 'agentThoughtChunk', content: { kind: 'text', text: 'a' } })).toBe(true)
  expect(isChunk({ kind: 'usageUpdate', usage: { used: 1, size: 2 } })).toBe(false)
  expect(isChunk({ kind: 'plan', entries: [] })).toBe(false)
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- session-update`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'
import { decodeContentBlock, type ContentBlock } from './content-block.ts'
import {
  decodeToolCall,
  decodeToolCallUpdate,
  type ToolCall,
  type ToolCallUpdate
} from './tool-call.ts'

export interface PlanEntry {
  content: string
  priority: string
  status: string
}

export interface AvailableCommand {
  name: string
  description: string
}

export interface ContextUsage {
  used: number
  size: number
}

export type StopReason = 'endTurn' | 'maxTokens' | 'refusal' | 'cancelled' | 'error'

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

const PianoSchema = z.array(
  z.object({ content: z.string(), priority: z.string(), status: z.string() })
)
const ComandiSchema = z.array(z.object({ name: z.string(), description: z.string().default('') }))
const UsoSchema = z.object({ used: z.number().int(), size: z.number().int() })

export function decodeSessionUpdate(value: unknown): SessionUpdate {
  const campi = (value ?? {}) as Record<string, unknown>
  const discriminante = typeof campi.sessionUpdate === 'string' ? campi.sessionUpdate : ''

  switch (discriminante) {
    case 'user_message_chunk':
      return { kind: 'userMessageChunk', content: decodeContentBlock(campi.content) }
    case 'agent_message_chunk':
      return { kind: 'agentMessageChunk', content: decodeContentBlock(campi.content) }
    case 'agent_thought_chunk':
      return { kind: 'agentThoughtChunk', content: decodeContentBlock(campi.content) }
    case 'tool_call': {
      const call = decodeToolCall(campi)
      return call === null ? { kind: 'unknown', discriminator: discriminante } : { kind: 'toolCall', call }
    }
    case 'tool_call_update': {
      const update = decodeToolCallUpdate(campi)
      return update === null
        ? { kind: 'unknown', discriminator: discriminante }
        : { kind: 'toolCallUpdate', update }
    }
    case 'plan': {
      const analisi = PianoSchema.safeParse(campi.entries)
      return { kind: 'plan', entries: analisi.success ? analisi.data : [] }
    }
    case 'available_commands_update': {
      const analisi = ComandiSchema.safeParse(campi.availableCommands)
      return { kind: 'availableCommandsUpdate', commands: analisi.success ? analisi.data : [] }
    }
    case 'current_mode_update':
      return {
        kind: 'currentModeUpdate',
        modeId: typeof campi.currentModeId === 'string' ? campi.currentModeId : ''
      }
    case 'usage_update': {
      const analisi = UsoSchema.safeParse(campi)
      return analisi.success
        ? { kind: 'usageUpdate', usage: { used: analisi.data.used, size: analisi.data.size } }
        : { kind: 'unknown', discriminator: discriminante }
    }
    default:
      return { kind: 'unknown', discriminator: discriminante }
  }
}

/**
 * Vero per gli eventi che arrivano a raffica, uno per token. Decide il canale
 * di consegna: questi vanno coalescati, tutti gli altri sono autoritativi e
 * rari. Il criterio non e il tipo del dato, e la frequenza.
 */
export function isChunk(update: SessionUpdate): boolean {
  return (
    update.kind === 'agentMessageChunk' ||
    update.kind === 'agentThoughtChunk' ||
    update.kind === 'userMessageChunk'
  )
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- session-update`
Atteso: PASS, 5 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/chat/
git commit -m "feat: il modello canonico degli eventi di sessione"
```

---

## Task 5: modalità dei permessi e server MCP

**File:**
- Crea: `src/shared/chat/permission.ts`
- Crea: `src/shared/chat/mcp-config.ts`
- Test: `src/shared/chat/permission.test.ts`

**Interfacce:**
- Produce: `type PermissionMode = 'ask' | 'acceptEdits' | 'plan' | 'fullAuto'`;
  `interface PermissionOption { optionId: string; name: string; kind: string }`;
  `type PermissionOutcome = { kind: 'selected'; optionId: string } | { kind: 'cancelled' }`;
  `interface McpServerSpec { name: string; command: string; args: string[]; env: Record<string, string> }`;
  `encodeMcpServers(specs: McpServerSpec[]): unknown[]`.

**Contesto:** i due pezzi stanno in un task solo perché sono piccoli e nessuno
dei due merita un gate a sé. Le quattro modalità sono quelle già in uso in
Swift e persistite nella colonna `chatSession.permissionMode`: non si
reinventano, e il valore letto dal database deve continuare a significare la
stessa cosa.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { PermissionModeSchema } from './permission.ts'
import { encodeMcpServers } from './mcp-config.ts'

test('le quattro modalita persistite restano valide', () => {
  // Sono i valori gia scritti in chatSession.permissionMode dal database
  // importato in Fase 1: cambiarli renderebbe illeggibili le sessioni vecchie.
  for (const modo of ['ask', 'acceptEdits', 'plan', 'fullAuto']) {
    expect(PermissionModeSchema.safeParse(modo).success).toBe(true)
  }
})

test('una modalita inventata viene rifiutata', () => {
  expect(PermissionModeSchema.safeParse('yolo').success).toBe(false)
})

test('un server mcp si serializza con nome, comando e argomenti', () => {
  expect(
    encodeMcpServers([{ name: 'x', command: '/bin/x', args: ['--a'], env: { K: 'v' } }])
  ).toEqual([{ name: 'x', command: '/bin/x', args: ['--a'], env: [{ name: 'K', value: 'v' }] }])
})

test('nessun server mcp produce una lista vuota, non null', () => {
  expect(encodeMcpServers([])).toEqual([])
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- permission`
Atteso: FAIL — i moduli non esistono.

- [ ] **Passo 3: implementare**

`src/shared/chat/permission.ts`:

```ts
import { z } from 'zod'

export const PermissionModeSchema = z.enum(['ask', 'acceptEdits', 'plan', 'fullAuto'])
export type PermissionMode = z.infer<typeof PermissionModeSchema>

export interface PermissionOption {
  optionId: string
  name: string
  kind: string
}

export type PermissionOutcome =
  | { kind: 'selected'; optionId: string }
  | { kind: 'cancelled' }
```

`src/shared/chat/mcp-config.ts`:

```ts
export interface McpServerSpec {
  name: string
  command: string
  args: string[]
  env: Record<string, string>
}

/** L ambiente viaggia come lista di coppie, non come oggetto: e la forma ACP. */
export function encodeMcpServers(specs: McpServerSpec[]): unknown[] {
  return specs.map((spec) => ({
    name: spec.name,
    command: spec.command,
    args: spec.args,
    env: Object.entries(spec.env).map(([name, value]) => ({ name, value }))
  }))
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- permission`
Atteso: PASS, 4 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/chat/
git commit -m "feat: modalita dei permessi e configurazione mcp"
```

---

## Task 6: trasporto di processo

**File:**
- Crea: `src/main/chat/process-transport.ts`
- Test: `src/main/chat/process-transport.test.ts`

**Interfacce:**
- Produce: `interface ChatTransport { start(): Promise<void>; send(line: string): Promise<void>; onLine(cb: (line: string) => void): () => void; onClose(cb: (code: number | null) => void): () => void; terminate(): Promise<void> }`;
  `class ProcessTransport implements ChatTransport` con
  `constructor(opts: { executable: string; args: string[]; cwd: string; env?: NodeJS.ProcessEnv; onStderrLine?: (line: string) => void })`.

**Contesto:** un processo figlio che parla JSON-RPC a righe su stdin/stdout.
Tre cose che sembrano dettagli e non lo sono:

- **stderr non entra mai nel protocollo.** Gli agenti ci scrivono diagnostica,
  e mescolarla alle righe JSON fa cadere l'analisi su output che non è un
  errore.
- **Le righe vanno riassemblate.** `data` su una pipe non arriva allineato agli
  a capo: un JSON può arrivare spezzato in due `data`, e due JSON possono
  arrivare in uno solo. Serve un buffer.
- **Alla morte del processo il buffer va svuotato prima di chiudere.** L'ultima
  riga utile arriva spesso insieme all'uscita, ed è quella che dice perché.

Il lancio passa dalla shell di login (`zsh -lc "exec …"`), come in Swift: i
binari npm sono script con shebang `#!/usr/bin/env node` e hanno bisogno del
PATH dell'utente, che un'app lanciata dal Finder non ha.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { ProcessTransport } from './process-transport.ts'

function raccogli(): { righe: string[]; push: (r: string) => void } {
  const righe: string[] = []
  return { righe, push: (r) => righe.push(r) }
}

test('le righe si riassemblano anche se arrivano spezzate', async () => {
  // printf senza a capo finale, poi il resto: due `data` per una riga sola.
  const t = new ProcessTransport({
    executable: '/bin/sh',
    args: ['-c', 'printf \'{"a":\'; sleep 0.05; printf \'1}\\n\''],
    cwd: '/tmp'
  })
  const out = raccogli()
  t.onLine(out.push)
  await t.start()
  await new Promise((r) => setTimeout(r, 300))
  expect(out.righe).toEqual(['{"a":1}'])
})

test('due righe in un solo blocco si separano', async () => {
  const t = new ProcessTransport({
    executable: '/bin/sh',
    args: ['-c', 'printf \'{"a":1}\\n{"b":2}\\n\''],
    cwd: '/tmp'
  })
  const out = raccogli()
  t.onLine(out.push)
  await t.start()
  await new Promise((r) => setTimeout(r, 300))
  expect(out.righe).toEqual(['{"a":1}', '{"b":2}'])
})

test('stderr non finisce mai fra le righe del protocollo', async () => {
  const out = raccogli()
  const err = raccogli()
  const t = new ProcessTransport({
    executable: '/bin/sh',
    args: ['-c', 'echo diagnostica >&2; printf \'{"a":1}\\n\''],
    cwd: '/tmp',
    onStderrLine: err.push
  })
  t.onLine(out.push)
  await t.start()
  await new Promise((r) => setTimeout(r, 300))
  expect(out.righe).toEqual(['{"a":1}'])
  expect(err.righe).toEqual(['diagnostica'])
})

test('l ultima riga arriva anche se il processo esce subito dopo', async () => {
  // Senza svuotamento del buffer alla chiusura, questa riga si perde — ed e
  // quasi sempre quella che dice perche il processo e morto.
  const t = new ProcessTransport({
    executable: '/bin/sh',
    args: ['-c', 'printf \'{"ultima":true}\\n\'; exit 3'],
    cwd: '/tmp'
  })
  const out = raccogli()
  let codice: number | null = -1
  t.onLine(out.push)
  t.onClose((c) => {
    codice = c
  })
  await t.start()
  await new Promise((r) => setTimeout(r, 400))
  expect(out.righe).toEqual(['{"ultima":true}'])
  expect(codice).toBe(3)
})

test('quel che si manda arriva al processo', async () => {
  const t = new ProcessTransport({
    executable: '/bin/sh',
    args: ['-c', 'read riga; printf \'{"eco":"%s"}\\n\' "$riga"'],
    cwd: '/tmp'
  })
  const out = raccogli()
  t.onLine(out.push)
  await t.start()
  await t.send('ciao\n')
  await new Promise((r) => setTimeout(r, 300))
  expect(out.righe).toEqual(['{"eco":"ciao"}'])
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- process-transport`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'

export interface ChatTransport {
  start(): Promise<void>
  send(line: string): Promise<void>
  onLine(cb: (line: string) => void): () => void
  onClose(cb: (code: number | null) => void): () => void
  terminate(): Promise<void>
}

export interface ProcessTransportOptions {
  executable: string
  args: string[]
  cwd: string
  env?: NodeJS.ProcessEnv
  onStderrLine?: (line: string) => void
}

/**
 * Righe JSON-RPC su stdin/stdout di un processo figlio. stderr resta fuori dal
 * protocollo: gli agenti ci scrivono diagnostica, e mescolarla alle righe fa
 * cadere l analisi su output che non e un errore.
 */
export class ProcessTransport implements ChatTransport {
  readonly #opts: ProcessTransportOptions
  #child: ChildProcessWithoutNullStreams | null = null
  #bufferStdout = ''
  #bufferStderr = ''
  readonly #ascoltatoriRiga = new Set<(line: string) => void>()
  readonly #ascoltatoriChiusura = new Set<(code: number | null) => void>()

  constructor(opts: ProcessTransportOptions) {
    this.#opts = opts
  }

  async start(): Promise<void> {
    const child = spawn(this.#opts.executable, this.#opts.args, {
      cwd: this.#opts.cwd,
      env: this.#opts.env ?? process.env,
      stdio: ['pipe', 'pipe', 'pipe']
    })
    this.#child = child
    child.stdout.setEncoding('utf8')
    child.stderr.setEncoding('utf8')
    child.stdout.on('data', (pezzo: string) => this.#consumaStdout(pezzo))
    child.stderr.on('data', (pezzo: string) => this.#consumaStderr(pezzo))
    child.on('close', (code) => {
      // Svuotare PRIMA di annunciare la chiusura: l ultima riga arriva spesso
      // insieme all uscita, ed e quella che dice perche.
      this.#consumaStdout('\n')
      for (const cb of this.#ascoltatoriChiusura) cb(code)
    })
  }

  async send(line: string): Promise<void> {
    const child = this.#child
    if (child === null) throw new Error('trasporto non avviato')
    await new Promise<void>((resolve, reject) => {
      child.stdin.write(line, (errore) => (errore ? reject(errore) : resolve()))
    })
  }

  onLine(cb: (line: string) => void): () => void {
    this.#ascoltatoriRiga.add(cb)
    return () => this.#ascoltatoriRiga.delete(cb)
  }

  onClose(cb: (code: number | null) => void): () => void {
    this.#ascoltatoriChiusura.add(cb)
    return () => this.#ascoltatoriChiusura.delete(cb)
  }

  async terminate(): Promise<void> {
    this.#child?.kill()
  }

  #consumaStdout(pezzo: string): void {
    this.#bufferStdout += pezzo
    for (const riga of this.#estrai('#bufferStdout')) {
      for (const cb of this.#ascoltatoriRiga) cb(riga)
    }
  }

  #consumaStderr(pezzo: string): void {
    this.#bufferStderr += pezzo
    const onStderrLine = this.#opts.onStderrLine
    for (const riga of this.#estrai('#bufferStderr')) {
      if (onStderrLine !== undefined) onStderrLine(riga)
    }
  }

  #estrai(quale: '#bufferStdout' | '#bufferStderr'): string[] {
    const testo = quale === '#bufferStdout' ? this.#bufferStdout : this.#bufferStderr
    const pezzi = testo.split('\n')
    const resto = pezzi.pop() ?? ''
    if (quale === '#bufferStdout') this.#bufferStdout = resto
    else this.#bufferStderr = resto
    return pezzi.filter((riga) => riga.length > 0)
  }
}

/**
 * Lancio attraverso la shell di login: i binari npm sono script con shebang
 * `#!/usr/bin/env node` e vogliono il PATH dell utente, che un app lanciata
 * dal Finder non ha. `exec` sostituisce la shell, cosi il figlio E l agente
 * e non un suo discendente — altrimenti terminarlo lascerebbe l agente vivo.
 */
export function loginShellLaunch(command: string): { executable: string; args: string[] } {
  return { executable: '/bin/zsh', args: ['-lc', `exec ${command}`] }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- process-transport`
Atteso: PASS, 5 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: trasporto di processo per gli agenti di chat"
```

---

## Task 7: l'interfaccia dei driver

**File:**
- Crea: `src/main/chat/driver.ts`
- Test: nessuno (sole dichiarazioni di tipo; la verifica è il typecheck).

**Interfacce:**
- Consuma: `ContentBlock`, `SessionUpdate`, `StopReason`, `PermissionOption`,
  `PermissionOutcome`, `McpServerSpec`, `JSONRPCID` dai Task 1-5.
- Produce: `type ChatSessionEvent`; `interface SessionHandle`;
  `interface SessionConfigOption`; `interface AgentDriver`.

**Contesto:** è l'astrazione che rende sostenibile la scelta dei quattro
driver nativi — quattro protocolli entrano, un modello di eventi esce. La
superficie è quella già validata da cinque implementazioni in Swift; qui non si
migliora, si porta.

`turnEnded` è l'**ultimo** evento del turno, emesso dopo gli aggiornamenti: chi
consuma deve chiudere il turno dietro a ciò che ha già letto, non davanti a
quel che è ancora in coda.

- [ ] **Passo 1: scrivere le dichiarazioni**

```ts
import type { ContentBlock } from '../../shared/chat/content-block.ts'
import type { JSONRPCID } from '../../shared/chat/json-rpc.ts'
import type { McpServerSpec } from '../../shared/chat/mcp-config.ts'
import type { PermissionOption, PermissionOutcome } from '../../shared/chat/permission.ts'
import type { SessionUpdate, StopReason } from '../../shared/chat/session-update.ts'
import type { ToolCallUpdate } from '../../shared/chat/tool-call.ts'

export interface SessionConfigOption {
  id: string
  name: string
  values: { id: string; name: string }[]
  currentValue: string | null
}

export interface SessionHandle {
  sessionId: string
  modes: { currentModeId: string; available: { id: string; name: string }[] } | null
  models: { currentModelId: string; available: { id: string; name: string }[] } | null
  configOptions: SessionConfigOption[]
  didResume: boolean
  /**
   * L agente ha ri-mandato la conversazione, quindi la nostra copia va
   * scartata per non mostrarla due volte. Vero solo per il `session/load`
   * dell ACP: i driver nativi ripristinano il contesto dentro il proprio CLI
   * senza ristamparlo, e li scartare la copia lascerebbe la chat vuota.
   */
  didReplayHistory: boolean
}

export type ChatSessionEvent =
  | { kind: 'update'; update: SessionUpdate }
  | {
      kind: 'permissionRequested'
      requestId: JSONRPCID
      toolCall: ToolCallUpdate
      options: PermissionOption[]
    }
  /** Ultimo evento del turno: arriva DOPO gli aggiornamenti che lo compongono. */
  | { kind: 'turnEnded'; reason: StopReason }
  | { kind: 'disconnected' }

export interface AgentDriver {
  readonly events: AsyncIterable<ChatSessionEvent>
  readonly supportsStructuredAnswers: boolean
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
}
```

- [ ] **Passo 2: verificare che compili**

Comando: `pnpm typecheck:node`
Atteso: PASS. Se un tipo importato non esiste, il difetto è nel task che
doveva produrlo — non si aggiusta indebolendo questa interfaccia.

- [ ] **Passo 3: lint e commit**

```bash
pnpm lint
git add src/main/chat/driver.ts
git commit -m "feat: l interfaccia comune dei driver di chat"
```

---

## Task 8: endpoint JSON-RPC

**File:**
- Crea: `src/main/chat/acp/client.ts`
- Test: `src/main/chat/acp/client.test.ts`

**Interfacce:**
- Consuma: `ChatTransport` (Task 6), i messaggi JSON-RPC (Task 1).
- Produce: `class ACPClient` con `start()`, `stop()`,
  `request(method: string, params: unknown): Promise<unknown>`,
  `notify(method: string, params: unknown): Promise<void>`,
  `respond(id: JSONRPCID, result: unknown): Promise<void>`,
  `respondError(id: JSONRPCID, code: number, message: string): Promise<void>`,
  `onIncoming(cb: (incoming: ACPIncoming) => void): () => void`;
  `type ACPIncoming`.

**Contesto:** correla le nostre richieste con le risposte dell'agente e
inoltra al livello superiore ciò che l'agente inizia di suo. Un solo dettaglio
decide se la chat si pianta o no: **quando il trasporto si chiude, ogni
richiesta in attesa va fatta fallire**. Una promessa mai risolta è un'attesa
infinita senza traccia — la chat resta a pensare per sempre.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect, vi } from 'vitest'
import { ACPClient, type ACPIncoming } from './client.ts'
import type { ChatTransport } from '../process-transport.ts'

function trasportoFinto(): ChatTransport & { emetti: (line: string) => void; chiudi: () => void; inviati: string[] } {
  const righe = new Set<(line: string) => void>()
  const chiusure = new Set<(code: number | null) => void>()
  const inviati: string[] = []
  return {
    inviati,
    start: async () => {},
    send: async (line) => {
      inviati.push(line)
    },
    onLine: (cb) => {
      righe.add(cb)
      return () => righe.delete(cb)
    },
    onClose: (cb) => {
      chiusure.add(cb)
      return () => chiusure.delete(cb)
    },
    terminate: async () => {},
    emetti: (line) => {
      for (const cb of righe) cb(line)
    },
    chiudi: () => {
      for (const cb of chiusure) cb(0)
    }
  }
}

test('una richiesta si risolve con il risultato che torna', async () => {
  const t = trasportoFinto()
  const c = new ACPClient(t)
  await c.start()
  const attesa = c.request('initialize', { protocolVersion: 1 })
  t.emetti('{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}')
  await expect(attesa).resolves.toEqual({ protocolVersion: 1 })
})

test('un errore dell agente fa fallire la richiesta', async () => {
  const t = trasportoFinto()
  const c = new ACPClient(t)
  await c.start()
  const attesa = c.request('x', {})
  t.emetti('{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"non trovato"}}')
  await expect(attesa).rejects.toThrow('non trovato')
})

test('la chiusura del trasporto fa fallire tutte le richieste in attesa', async () => {
  // La trappola: una promessa mai risolta e un attesa infinita senza traccia.
  // La chat resterebbe a pensare per sempre, e nessun log direbbe perche.
  const t = trasportoFinto()
  const c = new ACPClient(t)
  await c.start()
  const a = c.request('uno', {})
  const b = c.request('due', {})
  t.chiudi()
  await expect(a).rejects.toThrow(/chiuso/)
  await expect(b).rejects.toThrow(/chiuso/)
})

test('le notifiche e le richieste dell agente arrivano a chi ascolta', async () => {
  const t = trasportoFinto()
  const c = new ACPClient(t)
  const visti: ACPIncoming[] = []
  c.onIncoming((i) => visti.push(i))
  await c.start()
  t.emetti('{"jsonrpc":"2.0","method":"session/update","params":{"a":1}}')
  t.emetti('{"jsonrpc":"2.0","id":9,"method":"fs/read_text_file","params":{}}')
  expect(visti).toEqual([
    { kind: 'notification', method: 'session/update', params: { a: 1 } },
    { kind: 'request', id: 9, method: 'fs/read_text_file', params: {} }
  ])
})

test('una riga illeggibile non fa cadere la connessione', async () => {
  const t = trasportoFinto()
  const c = new ACPClient(t)
  const visti: ACPIncoming[] = []
  c.onIncoming((i) => visti.push(i))
  await c.start()
  t.emetti('spazzatura non json')
  t.emetti('{"jsonrpc":"2.0","method":"session/update","params":{}}')
  expect(visti).toHaveLength(1)
})

test('ogni richiesta usa un id nuovo', async () => {
  const t = trasportoFinto()
  const c = new ACPClient(t)
  await c.start()
  void c.request('a', {})
  void c.request('b', {})
  const ids = t.inviati.map((riga) => JSON.parse(riga).id)
  expect(new Set(ids).size).toBe(2)
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- acp/client`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { decodeMessage, encodeLine, type JSONRPCID } from '../../../shared/chat/json-rpc.ts'
import type { ChatTransport } from '../process-transport.ts'

export type ACPIncoming =
  | { kind: 'notification'; method: string; params: unknown }
  | { kind: 'request'; id: JSONRPCID; method: string; params: unknown }

interface InAttesa {
  resolve: (value: unknown) => void
  reject: (errore: Error) => void
}

export class ACPClient {
  readonly #transport: ChatTransport
  readonly #pendenti = new Map<JSONRPCID, InAttesa>()
  readonly #ascoltatori = new Set<(incoming: ACPIncoming) => void>()
  #prossimoId = 0
  #chiuso = false

  constructor(transport: ChatTransport) {
    this.#transport = transport
  }

  async start(): Promise<void> {
    this.#transport.onLine((riga) => this.#gestisci(riga))
    this.#transport.onClose(() => this.#chiudi())
    await this.#transport.start()
  }

  async stop(): Promise<void> {
    await this.#transport.terminate()
    this.#chiudi()
  }

  async request(method: string, params: unknown): Promise<unknown> {
    if (this.#chiuso) throw new Error('trasporto chiuso')
    this.#prossimoId += 1
    const id = this.#prossimoId
    const attesa = new Promise<unknown>((resolve, reject) => {
      this.#pendenti.set(id, { resolve, reject })
    })
    await this.#transport.send(encodeLine({ kind: 'request', id, method, params }))
    return attesa
  }

  async notify(method: string, params: unknown): Promise<void> {
    await this.#transport.send(encodeLine({ kind: 'notification', method, params }))
  }

  async respond(id: JSONRPCID, result: unknown): Promise<void> {
    await this.#transport.send(encodeLine({ kind: 'response', id, result, error: null }))
  }

  async respondError(id: JSONRPCID, code: number, message: string): Promise<void> {
    await this.#transport.send(
      encodeLine({ kind: 'response', id, result: null, error: { code, message } })
    )
  }

  onIncoming(cb: (incoming: ACPIncoming) => void): () => void {
    this.#ascoltatori.add(cb)
    return () => this.#ascoltatori.delete(cb)
  }

  #gestisci(riga: string): void {
    const messaggio = decodeMessage(riga)
    switch (messaggio.kind) {
      case 'invalid':
        // Una riga illeggibile non fa cadere la connessione, ma non sparisce:
        // e quasi sempre il primo segno che il CLI ha cambiato protocollo.
        console.warn(`riga non interpretabile dall agente: ${messaggio.reason}`)
        return
      case 'response': {
        const attesa = this.#pendenti.get(messaggio.id)
        if (attesa === undefined) return
        this.#pendenti.delete(messaggio.id)
        if (messaggio.error !== null) attesa.reject(new Error(messaggio.error.message))
        else attesa.resolve(messaggio.result)
        return
      }
      case 'notification':
        for (const cb of this.#ascoltatori) {
          cb({ kind: 'notification', method: messaggio.method, params: messaggio.params })
        }
        return
      case 'request':
        for (const cb of this.#ascoltatori) {
          cb({
            kind: 'request',
            id: messaggio.id,
            method: messaggio.method,
            params: messaggio.params
          })
        }
    }
  }

  #chiudi(): void {
    if (this.#chiuso) return
    this.#chiuso = true
    for (const attesa of this.#pendenti.values()) {
      attesa.reject(new Error('trasporto chiuso mentre la richiesta era in attesa'))
    }
    this.#pendenti.clear()
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- acp/client`
Atteso: PASS, 6 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: endpoint json-rpc verso l agente"
```

---

## Task 9: il file system esposto all'agente

**File:**
- Crea: `src/main/chat/acp/file-system.ts`
- Test: `src/main/chat/acp/file-system.test.ts`

**Interfacce:**
- Produce: `interface ACPFileSystem { readTextFile(path: string, line: number | null, limit: number | null): Promise<string>; writeTextFile(path: string, content: string): Promise<void> }`;
  `class WorktreeFileSystem implements ACPFileSystem` con
  `constructor(worktreePath: string)`.

**Contesto:** l'agente chiede a noi di leggere e scrivere file. **Il confine è
il worktree**: un percorso che esce dalla radice va rifiutato, e va rifiutato
dopo aver risolto i collegamenti simbolici e i `..`, non prima — un controllo
sulla stringa grezza si aggira con `../`.

Questo è codice che riceve percorsi da un processo esterno e tocca il disco:
il controllo non è una formalità.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { mkdtemp, writeFile, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { WorktreeFileSystem } from './file-system.ts'

async function radice(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'fs-acp-'))
  await mkdir(join(dir, 'wt'))
  await writeFile(join(dir, 'wt', 'dentro.txt'), 'riga1\nriga2\nriga3\n')
  await writeFile(join(dir, 'fuori.txt'), 'segreto')
  return dir
}

test('un file dentro il worktree si legge', async () => {
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await expect(fs.readTextFile('dentro.txt', null, null)).resolves.toBe('riga1\nriga2\nriga3\n')
})

test('si puo leggere da una riga per un numero di righe', async () => {
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await expect(fs.readTextFile('dentro.txt', 2, 1)).resolves.toBe('riga2\n')
})

test('un percorso che esce dal worktree viene rifiutato', async () => {
  // Il controllo va fatto sul percorso RISOLTO: `../` sulla stringa grezza
  // aggirerebbe qualunque confronto di prefisso.
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await expect(fs.readTextFile('../fuori.txt', null, null)).rejects.toThrow(/fuori dal worktree/)
})

test('un percorso assoluto fuori dal worktree viene rifiutato', async () => {
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await expect(fs.readTextFile('/etc/hosts', null, null)).rejects.toThrow(/fuori dal worktree/)
})

test('scrivere fuori dal worktree viene rifiutato', async () => {
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await expect(fs.writeTextFile('../fuori.txt', 'x')).rejects.toThrow(/fuori dal worktree/)
})

test('scrivere dentro il worktree funziona e si rilegge', async () => {
  const dir = await radice()
  const fs = new WorktreeFileSystem(join(dir, 'wt'))
  await fs.writeTextFile('nuovo.txt', 'contenuto')
  await expect(fs.readTextFile('nuovo.txt', null, null)).resolves.toBe('contenuto')
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- file-system`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { readFile, writeFile, mkdir, realpath } from 'node:fs/promises'
import { dirname, isAbsolute, resolve, sep } from 'node:path'

export interface ACPFileSystem {
  readTextFile(path: string, line: number | null, limit: number | null): Promise<string>
  writeTextFile(path: string, content: string): Promise<void>
}

/**
 * Il worktree e il confine. Un percorso che ne esce viene rifiutato DOPO la
 * risoluzione di `..` e dei collegamenti simbolici: un controllo di prefisso
 * sulla stringa grezza si aggira con un `../`, e qui i percorsi arrivano da un
 * processo esterno.
 */
export class WorktreeFileSystem implements ACPFileSystem {
  readonly #radice: string

  constructor(worktreePath: string) {
    this.#radice = resolve(worktreePath)
  }

  async readTextFile(path: string, line: number | null, limit: number | null): Promise<string> {
    const assoluto = await this.#dentro(path)
    const testo = await readFile(assoluto, 'utf8')
    if (line === null && limit === null) return testo
    const righe = testo.split('\n')
    const da = line === null ? 0 : Math.max(0, line - 1)
    const a = limit === null ? righe.length : da + limit
    return righe.slice(da, a).map((r) => `${r}\n`).join('')
  }

  async writeTextFile(path: string, content: string): Promise<void> {
    const assoluto = await this.#dentro(path)
    await mkdir(dirname(assoluto), { recursive: true })
    await writeFile(assoluto, content, 'utf8')
  }

  /** Risolve e verifica che il percorso resti dentro la radice. */
  async #dentro(path: string): Promise<string> {
    const candidato = isAbsolute(path) ? path : resolve(this.#radice, path)
    const normalizzato = resolve(candidato)
    // realpath sulla cartella: il file puo non esistere ancora (scrittura).
    let radiceReale = this.#radice
    try {
      radiceReale = await realpath(this.#radice)
    } catch {
      // Worktree cancellato mentre l agente lavora: si resta sul percorso
      // logico, e il confronto sotto rifiuta comunque quel che esce.
    }
    if (normalizzato !== radiceReale && !normalizzato.startsWith(radiceReale + sep)) {
      throw new Error(`percorso fuori dal worktree: ${path}`)
    }
    return normalizzato
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- file-system`
Atteso: PASS, 6 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: file system del worktree esposto all agente"
```

---

## Task 10: la sessione ACP generica

**File:**
- Crea: `src/main/chat/acp/session.ts`
- Test: `src/main/chat/acp/session.test.ts`

**Interfacce:**
- Consuma: `ACPClient` (Task 8), `ACPFileSystem` (Task 9), `AgentDriver` (Task 7),
  il modello degli eventi (Task 4).
- Produce: `class ACPSession implements AgentDriver` con
  `constructor(client: ACPClient, fileSystem: ACPFileSystem)`.

**Contesto:** il primo `AgentDriver` completo. Fa la stretta di mano, apre o
riprende la sessione, manda i prompt, serve le richieste di file dell'agente e
traduce le sue notifiche in `SessionUpdate`.

Due punti che sembrano dettagli:

- **Il ripristino avviene solo se l'agente dichiara `loadSession`** *e*
  abbiamo un id da riprendere. Chiedere `session/load` a un agente che non lo
  sa fare fa fallire la connessione invece di aprire una sessione nuova.
- **`didReplayHistory` è vero solo qui.** L'ACP ri-manda la conversazione con
  `session/load`, quindi la nostra copia va scartata per non mostrarla due
  volte. I driver nativi della 5a-2 ripristinano dentro il proprio CLI senza
  ristampare: metterlo vero anche lì lascerebbe la chat vuota.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { ACPSession } from './session.ts'
import { ACPClient } from './client.ts'
import type { ChatSessionEvent } from '../driver.ts'
import type { ChatTransport } from '../process-transport.ts'

/** Trasporto che risponde a `initialize` e `session/new` come un agente vero. */
function agenteFinto(opts: { loadSession: boolean }): ChatTransport & {
  emetti: (line: string) => void
} {
  const righe = new Set<(line: string) => void>()
  const chiusure = new Set<(code: number | null) => void>()
  const emetti = (line: string): void => {
    for (const cb of righe) cb(line)
  }
  return {
    start: async () => {},
    send: async (line) => {
      const m = JSON.parse(line)
      if (m.method === 'initialize') {
        emetti(
          JSON.stringify({
            jsonrpc: '2.0',
            id: m.id,
            result: {
              protocolVersion: 1,
              agentCapabilities: { loadSession: opts.loadSession }
            }
          })
        )
      }
      if (m.method === 'session/new') {
        emetti(JSON.stringify({ jsonrpc: '2.0', id: m.id, result: { sessionId: 's-1' } }))
      }
      if (m.method === 'session/load') {
        emetti(JSON.stringify({ jsonrpc: '2.0', id: m.id, result: {} }))
      }
      if (m.method === 'session/prompt') {
        emetti(JSON.stringify({ jsonrpc: '2.0', id: m.id, result: { stopReason: 'end_turn' } }))
      }
    },
    onLine: (cb) => {
      righe.add(cb)
      return () => righe.delete(cb)
    },
    onClose: (cb) => {
      chiusure.add(cb)
      return () => chiusure.delete(cb)
    },
    terminate: async () => {},
    emetti
  }
}

async function raccogliEventi(sessione: ACPSession, quanti: number): Promise<ChatSessionEvent[]> {
  const visti: ChatSessionEvent[] = []
  for await (const evento of sessione.events) {
    visti.push(evento)
    if (visti.length >= quanti) break
  }
  return visti
}

test('connettersi apre una sessione nuova', async () => {
  const t = agenteFinto({ loadSession: false })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  const handle = await s.connect({ cwd: '/tmp', resumeSessionId: null, mcpServers: [] })
  expect(handle.sessionId).toBe('s-1')
  expect(handle.didResume).toBe(false)
  expect(handle.didReplayHistory).toBe(false)
})

test('senza loadSession non si tenta il ripristino', async () => {
  // Chiedere session/load a un agente che non lo sa fare fa fallire la
  // connessione invece di aprire una sessione nuova.
  const t = agenteFinto({ loadSession: false })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  const handle = await s.connect({ cwd: '/tmp', resumeSessionId: 'vecchia', mcpServers: [] })
  expect(handle.sessionId).toBe('s-1')
  expect(handle.didResume).toBe(false)
})

test('con loadSession il ripristino avviene e la cronologia va scartata', async () => {
  const t = agenteFinto({ loadSession: true })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  const handle = await s.connect({ cwd: '/tmp', resumeSessionId: 'vecchia', mcpServers: [] })
  expect(handle.didResume).toBe(true)
  // Solo l ACP ri-manda la conversazione: i driver nativi no.
  expect(handle.didReplayHistory).toBe(true)
})

test('le notifiche di aggiornamento diventano eventi', async () => {
  const t = agenteFinto({ loadSession: false })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  await s.connect({ cwd: '/tmp', resumeSessionId: null, mcpServers: [] })
  const attesa = raccogliEventi(s, 1)
  t.emetti(
    JSON.stringify({
      jsonrpc: '2.0',
      method: 'session/update',
      params: { update: { sessionUpdate: 'agent_message_chunk', content: { type: 'text', text: 'ciao' } } }
    })
  )
  await expect(attesa).resolves.toEqual([
    { kind: 'update', update: { kind: 'agentMessageChunk', content: { kind: 'text', text: 'ciao' } } }
  ])
})

test('un prompt restituisce il motivo di fine turno', async () => {
  const t = agenteFinto({ loadSession: false })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  await s.connect({ cwd: '/tmp', resumeSessionId: null, mcpServers: [] })
  await expect(s.prompt([{ kind: 'text', text: 'ciao' }])).resolves.toBe('endTurn')
})

test('promptare senza essersi connessi e un errore, non un silenzio', async () => {
  const t = agenteFinto({ loadSession: false })
  const s = new ACPSession(new ACPClient(t), { readTextFile: async () => '', writeTextFile: async () => {} })
  await s.start()
  await expect(s.prompt([{ kind: 'text', text: 'x' }])).rejects.toThrow(/non connessa/)
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- acp/session`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

Nota per chi implementa: `events` è un `AsyncIterable`, quindi serve una coda
con risvegli. Non usare un array letto a intervalli.

```ts
import { encodeContentBlock, type ContentBlock } from '../../../shared/chat/content-block.ts'
import type { JSONRPCID } from '../../../shared/chat/json-rpc.ts'
import { encodeMcpServers, type McpServerSpec } from '../../../shared/chat/mcp-config.ts'
import type { PermissionOutcome } from '../../../shared/chat/permission.ts'
import { decodeSessionUpdate, type StopReason } from '../../../shared/chat/session-update.ts'
import type { AgentDriver, ChatSessionEvent, SessionConfigOption, SessionHandle } from '../driver.ts'
import type { ACPFileSystem } from './file-system.ts'
import { ACPClient, type ACPIncoming } from './client.ts'

const VERSIONE_PROTOCOLLO = 1

const MOTIVI: Record<string, StopReason> = {
  end_turn: 'endTurn',
  max_tokens: 'maxTokens',
  refusal: 'refusal',
  cancelled: 'cancelled'
}

export class ACPSession implements AgentDriver {
  readonly supportsStructuredAnswers = true
  readonly #client: ACPClient
  readonly #fileSystem: ACPFileSystem
  #sessionId: string | null = null
  readonly #coda: ChatSessionEvent[] = []
  #sveglia: (() => void) | null = null
  #finita = false

  constructor(client: ACPClient, fileSystem: ACPFileSystem) {
    this.#client = client
    this.#fileSystem = fileSystem
  }

  get events(): AsyncIterable<ChatSessionEvent> {
    const self = this
    return {
      async *[Symbol.asyncIterator]() {
        while (!self.#finita || self.#coda.length > 0) {
          const evento = self.#coda.shift()
          if (evento !== undefined) {
            yield evento
            continue
          }
          await new Promise<void>((resolve) => {
            self.#sveglia = resolve
          })
        }
      }
    }
  }

  async start(): Promise<void> {
    this.#client.onIncoming((incoming) => {
      void this.#gestisci(incoming)
    })
    await this.#client.start()
  }

  async stop(): Promise<void> {
    await this.#client.stop()
    this.#emetti({ kind: 'disconnected' })
    this.#finita = true
    this.#sveglia?.()
  }

  async connect(opts: {
    cwd: string
    resumeSessionId: string | null
    mcpServers: McpServerSpec[]
  }): Promise<SessionHandle> {
    const iniziale = (await this.#client.request('initialize', {
      protocolVersion: VERSIONE_PROTOCOLLO,
      clientCapabilities: {
        fs: { readTextFile: true, writeTextFile: true },
        terminal: false
      }
    })) as { protocolVersion?: number; agentCapabilities?: { loadSession?: boolean } }

    if (iniziale.protocolVersion !== VERSIONE_PROTOCOLLO) {
      throw new Error(`versione di protocollo non supportata: ${iniziale.protocolVersion}`)
    }

    const sannoRiprendere = iniziale.agentCapabilities?.loadSession === true
    const mcpServers = encodeMcpServers(opts.mcpServers)

    // Il ripristino solo se l agente lo dichiara E abbiamo un id: chiederlo a
    // chi non lo sa fare fa fallire la connessione invece di aprirne una nuova.
    if (sannoRiprendere && opts.resumeSessionId !== null) {
      await this.#client.request('session/load', {
        sessionId: opts.resumeSessionId,
        cwd: opts.cwd,
        mcpServers
      })
      this.#sessionId = opts.resumeSessionId
      return {
        sessionId: opts.resumeSessionId,
        modes: null,
        models: null,
        configOptions: [],
        didResume: true,
        // Solo l ACP ri-manda la conversazione con session/load.
        didReplayHistory: true
      }
    }

    const nuova = (await this.#client.request('session/new', {
      cwd: opts.cwd,
      mcpServers
    })) as { sessionId: string }
    this.#sessionId = nuova.sessionId
    return {
      sessionId: nuova.sessionId,
      modes: null,
      models: null,
      configOptions: [],
      didResume: false,
      didReplayHistory: false
    }
  }

  async prompt(blocks: ContentBlock[]): Promise<StopReason> {
    const sessionId = this.#sessionId
    if (sessionId === null) throw new Error('sessione non connessa')
    const risposta = (await this.#client.request('session/prompt', {
      sessionId,
      prompt: blocks.map(encodeContentBlock)
    })) as { stopReason?: string }
    const motivo = MOTIVI[risposta.stopReason ?? 'end_turn'] ?? 'endTurn'
    this.#emetti({ kind: 'turnEnded', reason: motivo })
    return motivo
  }

  async cancel(): Promise<void> {
    if (this.#sessionId === null) return
    await this.#client.notify('session/cancel', { sessionId: this.#sessionId })
  }

  async setMode(modeId: string): Promise<void> {
    if (this.#sessionId === null) throw new Error('sessione non connessa')
    await this.#client.request('session/set_mode', { sessionId: this.#sessionId, modeId })
  }

  async setModel(modelId: string): Promise<void> {
    if (this.#sessionId === null) throw new Error('sessione non connessa')
    await this.#client.request('session/set_model', { sessionId: this.#sessionId, modelId })
  }

  async setConfigOption(id: string, value: string): Promise<SessionConfigOption[] | null> {
    if (this.#sessionId === null) throw new Error('sessione non connessa')
    const risposta = (await this.#client.request('session/set_config_option', {
      sessionId: this.#sessionId,
      id,
      value
    })) as { configOptions?: SessionConfigOption[] }
    return risposta.configOptions ?? null
  }

  async setEffort(): Promise<void> {
    // L ACP non ha un concetto di sforzo separato: passa da setConfigOption.
  }

  async staticEffortOptions(): Promise<SessionConfigOption | null> {
    return null
  }

  async answerPermission(requestId: JSONRPCID, outcome: PermissionOutcome): Promise<void> {
    await this.#client.respond(requestId, {
      outcome:
        outcome.kind === 'selected'
          ? { outcome: 'selected', optionId: outcome.optionId }
          : { outcome: 'cancelled' }
    })
  }

  async #gestisci(incoming: ACPIncoming): Promise<void> {
    if (incoming.kind === 'notification') {
      if (incoming.method !== 'session/update') return
      const params = (incoming.params ?? {}) as { update?: unknown }
      this.#emetti({ kind: 'update', update: decodeSessionUpdate(params.update) })
      return
    }

    const params = (incoming.params ?? {}) as { path?: string; line?: number; limit?: number; content?: string }
    try {
      if (incoming.method === 'fs/read_text_file') {
        const contenuto = await this.#fileSystem.readTextFile(
          params.path ?? '',
          params.line ?? null,
          params.limit ?? null
        )
        await this.#client.respond(incoming.id, { content: contenuto })
        return
      }
      if (incoming.method === 'fs/write_text_file') {
        await this.#fileSystem.writeTextFile(params.path ?? '', params.content ?? '')
        await this.#client.respond(incoming.id, {})
        return
      }
      await this.#client.respondError(incoming.id, -32601, `metodo non gestito: ${incoming.method}`)
    } catch (errore) {
      // L errore torna all agente come risposta, non come silenzio: un agente
      // che non riceve risposta resta bloccato per sempre.
      await this.#client.respondError(incoming.id, -32603, String(errore))
    }
  }

  #emetti(evento: ChatSessionEvent): void {
    this.#coda.push(evento)
    const sveglia = this.#sveglia
    this.#sveglia = null
    sveglia?.()
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- acp/session`
Atteso: PASS, 6 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: la sessione acp generica come driver"
```

---

## Task 11: fabbrica dei driver

**File:**
- Crea: `src/main/chat/driver-factory.ts`
- Test: `src/main/chat/driver-factory.test.ts`

**Interfacce:**
- Consuma: `AgentDriver` (Task 7), `ACPSession` (Task 10), `ProcessTransport` (Task 6).
- Produce:
  `type DriverResult = { ok: true; driver: AgentDriver } | { ok: false; reason: string }`;
  `makeDriver(opts: { agentId: string; worktreePath: string; pathProbe?: (binary: string) => boolean }): DriverResult`;
  `builtinAcpCommand(agentId: string): string | null`.

**Contesto:** dall'id di un agente al suo driver. Un agente non installato
**non è un'eccezione: è una risposta** — l'utente deve leggere quale binario
manca, non uno stack trace.

Nella 5a-1 l'unico agente predefinito è `omp` (`omp acp`); gli altri arrivano
dai manifest del registry, che nella 5a-1 non esistono ancora e la cui assenza
non deve rompere nulla. I quattro driver nativi entrano nella 5a-2.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { builtinAcpCommand, makeDriver } from './driver-factory.ts'

test('omp e l agente acp predefinito', () => {
  expect(builtinAcpCommand('omp')).toBe('omp acp')
})

test('un agente sconosciuto non ha comando predefinito', () => {
  expect(builtinAcpCommand('inesistente')).toBeNull()
})

test('un agente con il binario presente produce un driver', () => {
  const r = makeDriver({ agentId: 'omp', worktreePath: '/tmp', pathProbe: () => true })
  expect(r.ok).toBe(true)
})

test('un agente senza binario risponde, non lancia', () => {
  // L utente deve leggere quale binario manca, non uno stack trace.
  const r = makeDriver({ agentId: 'omp', worktreePath: '/tmp', pathProbe: () => false })
  expect(r).toEqual({ ok: false, reason: 'binario non trovato nel PATH: omp' })
})

test('un agente che non conosciamo risponde con il motivo', () => {
  const r = makeDriver({ agentId: 'inesistente', worktreePath: '/tmp', pathProbe: () => true })
  expect(r).toEqual({ ok: false, reason: 'agente sconosciuto: inesistente' })
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- driver-factory`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

```ts
import { execFileSync } from 'node:child_process'
import { ACPClient } from './acp/client.ts'
import { ACPSession } from './acp/session.ts'
import { WorktreeFileSystem } from './acp/file-system.ts'
import { loginShellLaunch, ProcessTransport } from './process-transport.ts'
import type { AgentDriver } from './driver.ts'

export type DriverResult =
  | { ok: true; driver: AgentDriver }
  | { ok: false; reason: string }

/** Comandi ACP predefiniti. Gli altri agenti arrivano dai manifest (Fase 7). */
const PREDEFINITI: Record<string, { binary: string; command: string }> = {
  omp: { binary: 'omp', command: 'omp acp' }
}

export function builtinAcpCommand(agentId: string): string | null {
  return PREDEFINITI[agentId]?.command ?? null
}

export function defaultPathProbe(binary: string): boolean {
  try {
    execFileSync('/bin/zsh', ['-lc', `command -v ${binary}`], { stdio: 'pipe' })
    return true
  } catch {
    return false
  }
}

export function makeDriver(opts: {
  agentId: string
  worktreePath: string
  pathProbe?: (binary: string) => boolean
}): DriverResult {
  const predefinito = PREDEFINITI[opts.agentId]
  if (predefinito === undefined) {
    return { ok: false, reason: `agente sconosciuto: ${opts.agentId}` }
  }
  const probe = opts.pathProbe ?? defaultPathProbe
  if (!probe(predefinito.binary)) {
    return { ok: false, reason: `binario non trovato nel PATH: ${predefinito.binary}` }
  }
  const { executable, args } = loginShellLaunch(predefinito.command)
  const transport = new ProcessTransport({
    executable,
    args,
    cwd: opts.worktreePath,
    onStderrLine: (riga) => console.warn(`[${opts.agentId}] ${riga}`)
  })
  const driver = new ACPSession(
    new ACPClient(transport),
    new WorktreeFileSystem(opts.worktreePath)
  )
  return { ok: true, driver }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- driver-factory`
Atteso: PASS, 5 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: fabbrica dei driver di chat"
```

---

## Task 12: persistenza della sessione e gestore del ciclo di vita

**File:**
- Crea: `src/main/chat/chat-store.ts`
- Crea: `src/main/chat/session-manager.ts`
- Test: `src/main/chat/chat-store.test.ts`

**Interfacce:**
- Consuma: `Kysely<TillerDatabase>` (Fase 1), `AgentDriver` (Task 7),
  `makeDriver` (Task 11).
- Produce:
  `interface ChatSessionRow { id: string; worktreeId: string; agentId: string; acpSessionId: string | null; permissionMode: PermissionMode | null; selectedModel: string | null; selectedEffort: string | null; transportKind: string; title: string | null }`;
  `createChatSession(db, row): Promise<void>`;
  `loadChatSession(db, id): Promise<ChatSessionRow | null>`;
  `updateAcpSessionId(db, id, acpSessionId): Promise<void>`;
  `listChatSessions(db, worktreeId): Promise<ChatSessionRow[]>`;
  `class SessionManager` con `open`, `get`, `close`, `closeAll`.

**Contesto:** `chatSession` è della 5a; **`chatItem` no**. La 5a persiste la
*sessione* — agente, modello, modalità, `acpSessionId` — perché le serve per
riprendere dopo un riavvio. Il *contenuto* del trascritto lo decide il
riduttore della 5b: se la 5a scrivesse anche gli item dovrebbe inventarsi un
formato che la 5b rifarebbe.

`acpSessionId` va scritto **appena l'agente lo dà**, non alla chiusura: se
l'app muore nel mezzo, una sessione senza id non si riprende più.

Attenzione alle foreign key: `chatSession.worktreeId` referenzia `worktree.id`,
che referenzia `project.id`. Le fixture di test devono creare i padri.

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { creaDatabaseDiProva } from '../db/test-helpers.ts'
import { createChatSession, loadChatSession, listChatSessions, updateAcpSessionId } from './chat-store.ts'

/** Le foreign key vogliono i padri: project -> worktree -> chatSession. */
async function conPadri(): Promise<Awaited<ReturnType<typeof creaDatabaseDiProva>>> {
  const db = await creaDatabaseDiProva()
  await db.insertInto('project').values({ id: 'p1', rootPath: '/tmp/p', name: 'p' }).execute()
  await db
    .insertInto('worktree')
    .values({ id: 'wt1', projectId: 'p1', branch: 'main', path: '/tmp/p' })
    .execute()
  return db
}

test('una sessione si scrive e si rilegge', async () => {
  const db = await conPadri()
  await createChatSession(db, {
    id: 'c1',
    worktreeId: 'wt1',
    agentId: 'omp',
    acpSessionId: null,
    permissionMode: 'ask',
    selectedModel: null,
    selectedEffort: null,
    transportKind: 'acp',
    title: null
  })
  const letta = await loadChatSession(db, 'c1')
  expect(letta).toMatchObject({ id: 'c1', agentId: 'omp', permissionMode: 'ask' })
})

test('l id di sessione dell agente si aggiorna appena arriva', async () => {
  // Se l app muore nel mezzo, una sessione senza id non si riprende piu:
  // per questo si scrive subito e non alla chiusura.
  const db = await conPadri()
  await createChatSession(db, {
    id: 'c1',
    worktreeId: 'wt1',
    agentId: 'omp',
    acpSessionId: null,
    permissionMode: null,
    selectedModel: null,
    selectedEffort: null,
    transportKind: 'acp',
    title: null
  })
  await updateAcpSessionId(db, 'c1', 's-42')
  expect((await loadChatSession(db, 'c1'))?.acpSessionId).toBe('s-42')
})

test('le sessioni si elencano per worktree', async () => {
  const db = await conPadri()
  for (const id of ['c1', 'c2']) {
    await createChatSession(db, {
      id,
      worktreeId: 'wt1',
      agentId: 'omp',
      acpSessionId: null,
      permissionMode: null,
      selectedModel: null,
      selectedEffort: null,
      transportKind: 'acp',
      title: null
    })
  }
  expect(await listChatSessions(db, 'wt1')).toHaveLength(2)
})

test('una sessione inesistente si legge come null, non come errore', async () => {
  const db = await conPadri()
  expect(await loadChatSession(db, 'mai-esistita')).toBeNull()
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- chat-store`
Atteso: FAIL — il modulo non esiste.

- [ ] **Passo 3: implementare**

`chat-store.ts` usa kysely come gli altri repository della Fase 1
(`src/main/db/repository.ts` è il modello da seguire per lo stile).

```ts
import type { Kysely } from 'kysely'
import type { TillerDatabase } from '../db/schema'
import type { PermissionMode } from '../../shared/chat/permission.ts'

export interface ChatSessionRow {
  id: string
  worktreeId: string
  agentId: string
  acpSessionId: string | null
  permissionMode: PermissionMode | null
  selectedModel: string | null
  selectedEffort: string | null
  transportKind: string
  title: string | null
}

export async function createChatSession(
  db: Kysely<TillerDatabase>,
  row: ChatSessionRow
): Promise<void> {
  const adesso = new Date().toISOString()
  await db
    .insertInto('chatSession')
    .values({ ...row, createdAt: adesso, lastActivityAt: adesso })
    .execute()
}

export async function loadChatSession(
  db: Kysely<TillerDatabase>,
  id: string
): Promise<ChatSessionRow | null> {
  const riga = await db
    .selectFrom('chatSession')
    .selectAll()
    .where('id', '=', id)
    .executeTakeFirst()
  if (riga === undefined) return null
  return {
    id: riga.id,
    worktreeId: riga.worktreeId,
    agentId: riga.agentId,
    acpSessionId: riga.acpSessionId ?? null,
    permissionMode: (riga.permissionMode as PermissionMode | null) ?? null,
    selectedModel: riga.selectedModel ?? null,
    selectedEffort: riga.selectedEffort ?? null,
    transportKind: riga.transportKind,
    title: riga.title ?? null
  }
}

export async function updateAcpSessionId(
  db: Kysely<TillerDatabase>,
  id: string,
  acpSessionId: string
): Promise<void> {
  await db
    .updateTable('chatSession')
    .set({ acpSessionId, lastActivityAt: new Date().toISOString() })
    .where('id', '=', id)
    .execute()
}

export async function listChatSessions(
  db: Kysely<TillerDatabase>,
  worktreeId: string
): Promise<ChatSessionRow[]> {
  const righe = await db
    .selectFrom('chatSession')
    .selectAll()
    .where('worktreeId', '=', worktreeId)
    .orderBy('lastActivityAt', 'desc')
    .execute()
  return righe.map((riga) => ({
    id: riga.id,
    worktreeId: riga.worktreeId,
    agentId: riga.agentId,
    acpSessionId: riga.acpSessionId ?? null,
    permissionMode: (riga.permissionMode as PermissionMode | null) ?? null,
    selectedModel: riga.selectedModel ?? null,
    selectedEffort: riga.selectedEffort ?? null,
    transportKind: riga.transportKind,
    title: riga.title ?? null
  }))
}
```

`session-manager.ts`:

```ts
import type { AgentDriver } from './driver.ts'

/** Quali sessioni sono vive adesso. Non persiste nulla: quello e chat-store. */
export class SessionManager {
  readonly #vive = new Map<string, AgentDriver>()

  open(sessionId: string, driver: AgentDriver): void {
    this.#vive.set(sessionId, driver)
  }

  get(sessionId: string): AgentDriver | null {
    return this.#vive.get(sessionId) ?? null
  }

  async close(sessionId: string): Promise<void> {
    const driver = this.#vive.get(sessionId)
    if (driver === undefined) return
    this.#vive.delete(sessionId)
    await driver.stop()
  }

  /**
   * Attende davvero le chiusure. La Fase 2 ha insegnato che un modulo nativo
   * sorpreso a meta operazione da `app.exit()` aborta il processo: qui i
   * driver hanno processi figli, e valgono le stesse regole.
   */
  async closeAll(): Promise<void> {
    const sessioni = [...this.#vive.keys()]
    await Promise.all(sessioni.map((id) => this.close(id)))
  }
}
```

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- chat-store`
Atteso: PASS, 4 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/main/chat/
git commit -m "feat: persistenza delle sessioni di chat e ciclo di vita"
```

---

## Task 13: protocollo di controllo e due canali di consegna

**File:**
- Modifica: `src/shared/protocol.ts`
- Modifica: `src/main/control/dispatch.ts`
- Modifica: `src/main/index.ts`
- Modifica: `src/preload/index.ts`
- Test: `src/main/control/dispatch.test.ts`

**Interfacce:**
- Consuma: `SessionManager`, `makeDriver`, `chat-store` (Task 11-12), `isChunk` (Task 4).
- Produce: metodi `chat.create`, `chat.prompt`, `chat.cancel`, `chat.setMode`,
  `chat.setModel`, `chat.setEffort`, `chat.answerPermission`, `chat.get`,
  `chat.close` su `ControlRequest`; `StateEvent` `chat.update`; canale
  `chat:chunk` sul preload.

**Contesto:** è il task che rende la fase verificabile da CLI, e contiene la
decisione strutturale dello spec.

**Due canali, non uno.** `agentMessageChunk` arriva per token. In Swift questo
ha prodotto un freeze reale il 2026-07-24: la coda delle transazioni si
riempiva di eventi per-token e l'interfaccia si bloccava. E la Fase 2 aveva già
scritto la regola nel commento di `protocol.ts`: *«Canale separato ad alto
volume: NON passa da StateEvent»*. I chunk di chat sono l'output PTY con un
altro nome.

Quindi: eventi strutturali su `StateEvent`, chunk su `chat:chunk` coalescato
riusando il debouncer già in produzione per `pty:output`. Il criterio che
separa i due non è il tipo del dato, **è la frequenza**.

- [ ] **Passo 1: scrivere i test rossi**

```ts
test('chat.create apre una sessione e la persiste', async () => {
  const { dispatch, db } = await makeChatDeps()
  const r = await dispatch({
    id: 'r1',
    method: 'chat.create',
    params: { worktreeId: 'wt1', agentId: 'omp' }
  } as never)
  expect(r.ok).toBe(true)
  const sessionId = (r as { result: { sessionId: string } }).result.sessionId
  expect(await loadChatSession(db, sessionId)).not.toBeNull()
})

test('chat.create su un agente non installato risponde, non lancia', async () => {
  const { dispatch } = await makeChatDeps({ pathProbe: () => false })
  const r = await dispatch({
    id: 'r1',
    method: 'chat.create',
    params: { worktreeId: 'wt1', agentId: 'omp' }
  } as never)
  expect(r).toMatchObject({ ok: false, error: expect.stringContaining('omp') })
})

test('gli eventi strutturali passano da StateEvent', async () => {
  const emessi: StateEvent[] = []
  const { dispatch, driverFinto } = await makeChatDeps({ emit: (e) => emessi.push(e) })
  const sessionId = await creaChat(dispatch)
  driverFinto.emetti({ kind: 'update', update: { kind: 'usageUpdate', usage: { used: 1, size: 2 } } })
  await attendi(() => emessi.some((e) => e.type === 'chat.update'))
  expect(emessi.filter((e) => e.type === 'chat.update')).toHaveLength(1)
})

test('i pezzi di testo NON passano da StateEvent', async () => {
  // La regola della Fase 2: se puo arrivare a raffica non e stato autoritativo.
  const emessi: StateEvent[] = []
  const { dispatch, driverFinto } = await makeChatDeps({ emit: (e) => emessi.push(e) })
  await creaChat(dispatch)
  for (let i = 0; i < 50; i += 1) {
    driverFinto.emetti({
      kind: 'update',
      update: { kind: 'agentMessageChunk', content: { kind: 'text', text: 'x' } }
    })
  }
  expect(emessi.filter((e) => e.type === 'chat.update')).toHaveLength(0)
})

test('cinquanta pezzi si consegnano in molte meno consegne', async () => {
  // Il criterio che protegge dalla sparizione della coalescenza.
  const consegne: unknown[][] = []
  const { dispatch, driverFinto } = await makeChatDeps({ onChunkBatch: (b) => consegne.push(b) })
  await creaChat(dispatch)
  for (let i = 0; i < 50; i += 1) {
    driverFinto.emetti({
      kind: 'update',
      update: { kind: 'agentMessageChunk', content: { kind: 'text', text: 'x' } }
    })
  }
  await attendi(() => consegne.length > 0)
  expect(consegne.length).toBeLessThan(10)
  expect(consegne.flat()).toHaveLength(50)
})

test('chat.close termina il driver e lo toglie dai vivi', async () => {
  const { dispatch, manager } = await makeChatDeps()
  const sessionId = await creaChat(dispatch)
  await dispatch({ id: 'r2', method: 'chat.close', params: { sessionId } } as never)
  expect(manager.get(sessionId)).toBeNull()
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- dispatch`
Atteso: FAIL — i metodi `chat.*` non esistono.

- [ ] **Passo 3: aggiungere i metodi al protocollo**

In `src/shared/protocol.ts`, dentro l'unione di `ControlRequest`:

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.create'),
    params: z.object({
      worktreeId: z.string().min(1),
      agentId: z.string().min(1),
      resumeSessionId: z.string().min(1).nullable().default(null)
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.prompt'),
    params: z.object({ sessionId: z.string().min(1), text: z.string() })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.cancel'),
    params: z.object({ sessionId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.setMode'),
    params: z.object({ sessionId: z.string().min(1), modeId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.setModel'),
    params: z.object({ sessionId: z.string().min(1), modelId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.setEffort'),
    params: z.object({ sessionId: z.string().min(1), effort: z.string().nullable() })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.answerPermission'),
    params: z.object({
      sessionId: z.string().min(1),
      requestId: z.union([z.number(), z.string()]),
      optionId: z.string().nullable()
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.get'),
    params: z.object({ sessionId: z.string().min(1) })
  }),
```

Il risultato di `chat.get` è questa forma esatta — la usa il criterio 1 del
Task 16, e un campo che il criterio nomina ma il metodo non restituisce è un
riferimento morto:

```ts
{
  sessionId: string
  worktreeId: string
  agentId: string
  acpSessionId: string | null
  permissionMode: PermissionMode | null
  selectedModel: string | null
  selectedEffort: string | null
  /** Motivo dell ultimo turno concluso, o null se nessun turno e finito. */
  lastStopReason: StopReason | null
  /** Vero se il driver e ancora vivo in questo processo. */
  live: boolean
}
```

`lastStopReason` sta in memoria nel `SessionManager`, non nel database: è lo
stato del turno, non della sessione. Sopravvivere al riavvio non ha senso per
lui — dopo un riavvio nessun turno è in corso.

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('chat.close'),
    params: z.object({ sessionId: z.string().min(1) })
  }),
```

E nell'unione di `StateEvent`:

```ts
  z.object({
    type: z.literal('chat.update'),
    sessionId: z.string().min(1),
    /**
     * Un `ChatSessionEvent` serializzato — non un `SessionUpdate`. Il campo si
     * chiama `event` e non `update` di proposito: su questo canale passano
     * anche `permissionRequested`, `turnEnded` e `disconnected`, che
     * aggiornamenti non sono. Chiamarlo `update` farebbe scrivere al primo
     * consumatore un `decodeSessionUpdate(payload.update)` che fallisce in
     * silenzio su tre casi su quattro.
     */
    event: z.unknown()
  }),
```

**Nota di taglio, imparata nella Fase 2:** un tipo somma esaustivo non si può
spezzare fra due task. Aggiungere rami a `ControlRequest` rompe lo `switch` di
`dispatch.ts`, quindi protocollo e dispatcher stanno nello stesso task.

- [ ] **Passo 4: gestire i metodi nel dispatcher**

In `dispatch.ts`, `DispatchDeps` guadagna:

```ts
  chat: {
    manager: SessionManager
    makeDriver: typeof makeDriver
  }
  /** Consegna coalescata dei pezzi di testo. NON passa da StateEvent. */
  emitChunks?(sessionId: string, chunks: unknown[]): void
```

I casi seguono lo stile esistente. `chat.create`:

```ts
        case 'chat.create': {
          const esito = deps.chat.makeDriver({
            agentId: request.params.agentId,
            worktreePath: await percorsoWorktree(deps.db, request.params.worktreeId)
          })
          if (!esito.ok) return { id: request.id, ok: false, error: esito.reason }

          const sessionId = deps.newId()
          await esito.driver.start()
          const handle = await esito.driver.connect({
            cwd: await percorsoWorktree(deps.db, request.params.worktreeId),
            resumeSessionId: request.params.resumeSessionId,
            mcpServers: []
          })
          await createChatSession(deps.db, {
            id: sessionId,
            worktreeId: request.params.worktreeId,
            agentId: request.params.agentId,
            // Subito, non alla chiusura: se l app muore nel mezzo, una
            // sessione senza id non si riprende piu.
            acpSessionId: handle.sessionId,
            permissionMode: null,
            selectedModel: null,
            selectedEffort: null,
            transportKind: 'acp',
            title: null
          })
          deps.chat.manager.open(sessionId, esito.driver)
          pompaEventi(deps, sessionId, esito.driver)
          return { id: request.id, ok: true, result: { sessionId } }
        }
```

La pompa che smista i due canali:

```ts
/**
 * Un evento per token non deve arrivare al renderer uno per volta: e la
 * regola della Fase 2 per l output PTY, e in Swift ignorarla ha prodotto un
 * freeze. `isChunk` decide il canale; il debouncer e lo stesso gia in uso.
 */
function pompaEventi(deps: DispatchDeps, sessionId: string, driver: AgentDriver): void {
  let accumulati: unknown[] = []
  const consegna = createDebouncer<[]>({
    delayMs: 16,
    onSettle: () => {
      if (accumulati.length === 0) return
      const lotto = accumulati
      accumulati = []
      deps.emitChunks?.(sessionId, lotto)
    },
    schedule: (fn, ms) => setTimeout(fn, ms),
    cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>)
  })

  void (async () => {
    for await (const evento of driver.events) {
      if (evento.kind === 'update' && isChunk(evento.update)) {
        accumulati.push(evento.update)
        consegna.push()
        continue
      }
      deps.emit?.({ type: 'chat.update', sessionId, event: evento })
    }
  })()
}
```

- [ ] **Passo 5: esporre il canale nel preload**

In `src/preload/index.ts`, accanto a `onPtyOutput`:

```ts
  onChatChunks: (
    callback: (payload: { sessionId: string; chunks: unknown[] }) => void
  ): (() => void) => {
    const listener = (
      _event: IpcRendererEvent,
      payload: { sessionId: string; chunks: unknown[] }
    ): void => callback(payload)
    ipcRenderer.on('chat:chunk', listener)
    return () => {
      ipcRenderer.removeListener('chat:chunk', listener)
    }
  }
```

E in `src/main/index.ts`, `emitChunks` inoltra sul canale `chat:chunk`, e
`sessionManager.closeAll()` va nella sequenza di spegnimento **prima** di
`closeDatabase`, insieme a `pty.disposeAll()`.

- [ ] **Passo 6: eseguire i test e vederli passare**

Comando: `pnpm test:unit -- dispatch`
Atteso: PASS, 6 test nuovi.

- [ ] **Passo 7: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add src/shared/protocol.ts src/main/ src/preload/
git commit -m "feat: protocollo di chat e consegna su due canali"
```

---

## Task 14: comandi di chat in tillerctl

**File:**
- Modifica: `cli/args.ts`
- Test: `cli/args.test.ts`

**Interfacce:**
- Consuma: i metodi `chat.*` del Task 13.
- Produce: comandi `chat-create`, `chat-prompt`, `chat-cancel`, `chat-close`,
  `chat-get`.

**Contesto:** è ciò che rende la fase verificabile senza interfaccia, e i nomi
kebab seguono quelli già esistenti (`worktree-create`, `workspace-apply`).

- [ ] **Passo 1: scrivere i test rossi**

```ts
import { test, expect } from 'vitest'
import { parseArgs } from './args.ts'

test('chat-create costruisce la richiesta', () => {
  const r = parseArgs(['chat-create', '--worktree', 'wt1', '--agent', 'omp'])
  expect(r).toMatchObject({
    method: 'chat.create',
    params: { worktreeId: 'wt1', agentId: 'omp', resumeSessionId: null }
  })
})

test('chat-create accetta una sessione da riprendere', () => {
  const r = parseArgs(['chat-create', '--worktree', 'wt1', '--agent', 'omp', '--resume', 's-1'])
  expect(r).toMatchObject({ params: { resumeSessionId: 's-1' } })
})

test('chat-prompt manda il testo', () => {
  const r = parseArgs(['chat-prompt', '--session', 'c1', '--text', 'ciao'])
  expect(r).toMatchObject({ method: 'chat.prompt', params: { sessionId: 'c1', text: 'ciao' } })
})

test('chat-create senza agente e un errore d uso', () => {
  expect(() => parseArgs(['chat-create', '--worktree', 'wt1'])).toThrow(/agent/)
})
```

- [ ] **Passo 2: eseguirlo e vederlo fallire**

Comando: `pnpm test:unit -- args`
Atteso: FAIL — i comandi non esistono.

- [ ] **Passo 3: implementare**

In `cli/args.ts`, nella tabella dei comandi:

```ts
  'chat-create': {
    method: 'chat.create',
    options: { worktree: { type: 'string' }, agent: { type: 'string' }, resume: { type: 'string' } },
    required: ['worktree', 'agent'],
    build: (v: Record<string, string>) => ({
      worktreeId: v.worktree,
      agentId: v.agent,
      resumeSessionId: v.resume ?? null
    })
  },
  'chat-prompt': {
    method: 'chat.prompt',
    options: { session: { type: 'string' }, text: { type: 'string' } },
    required: ['session', 'text'],
    build: (v: Record<string, string>) => ({ sessionId: v.session, text: v.text })
  },
  'chat-cancel': {
    method: 'chat.cancel',
    options: { session: { type: 'string' } },
    required: ['session'],
    build: (v: Record<string, string>) => ({ sessionId: v.session })
  },
  'chat-get': {
    method: 'chat.get',
    options: { session: { type: 'string' } },
    required: ['session'],
    build: (v: Record<string, string>) => ({ sessionId: v.session })
  },
  'chat-close': {
    method: 'chat.close',
    options: { session: { type: 'string' } },
    required: ['session'],
    build: (v: Record<string, string>) => ({ sessionId: v.session })
  },
```

E le righe corrispondenti nel testo di aiuto, accanto a `workspace-apply`.

- [ ] **Passo 4: eseguirlo e vederlo passare**

Comando: `pnpm test:unit -- args`
Atteso: PASS, 4 test.

- [ ] **Passo 5: typecheck, lint e commit**

```bash
pnpm typecheck:node && pnpm typecheck:cli && pnpm lint
git add cli/
git commit -m "feat: comandi di chat in tillerctl"
```

---

## Task 15: script di registrazione delle fixture

**File:**
- Crea: `Scripts/record-fixture.sh`
- Crea: `docs/fixture-chat/README.md`

**Interfacce:**
- Produce: uno script che salva una conversazione grezza da un CLI vero in
  `docs/fixture-chat/<agente>-<versione>/`.

**Contesto:** questo task **non serve alla 5a-1**, serve alla 5a-2 — ed è qui
perché la 5a-2 non si può scrivere senza. Lo spec decide che le fixture si
registrano dai CLI installati e non si portano da Swift: le versioni sono
`claude 2.1.220`, `codex 0.146.0`, `opencode 1.18.10`, `pi 0.82.1`, mentre le
fixture Swift sono di Codex 0.145.

Il motivo non è prudenza: nella Fase 3 le catture dal vivo hanno trovato **due
difetti presenti nell'app Swift in uso**, che nessuna lettura del codice
avrebbe rivelato — il codice era coerente con sé stesso, si era spostato il
mondo.

**La versione va nel nome della cartella.** Una fixture senza versione non dice
quando è scaduta, e fra sei mesi nessuno saprà se la differenza col protocollo
di allora è un difetto o un'evoluzione.

- [ ] **Passo 1: scrivere lo script**

```bash
#!/usr/bin/env bash
# Registra una conversazione grezza da un CLI di agente, per costruire le
# fixture dei driver. La versione finisce nel nome della cartella: una fixture
# che non dice contro quale versione e stata presa non dice quando e scaduta.
set -euo pipefail

agente="${1:?uso: record-fixture.sh <agente> <comando...>}"
shift

versione="$("$agente" --version 2>/dev/null | head -1 | tr -c 'a-zA-Z0-9.' '-' | sed 's/-*$//')"
destinazione="docs/fixture-chat/${agente}-${versione}"
mkdir -p "$destinazione"

echo "registro ${agente} ${versione} in ${destinazione}" >&2
echo "scrivi il prompt sullo standard input, poi Ctrl-D" >&2

"$@" > "${destinazione}/stdout.jsonl" 2> "${destinazione}/stderr.log"

echo "registrate $(wc -l < "${destinazione}/stdout.jsonl") righe" >&2
```

- [ ] **Passo 2: renderlo eseguibile e provarlo**

```bash
chmod +x Scripts/record-fixture.sh
Scripts/record-fixture.sh omp omp acp <<< '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}'
```

Atteso: `docs/fixture-chat/omp-<versione>/stdout.jsonl` contiene almeno una
riga, e quella riga è la risposta a `initialize`.

- [ ] **Passo 3: scrivere il README**

`docs/fixture-chat/README.md` deve dire, in prosa: che cosa sono queste
cartelle, che la versione nel nome è la versione del CLI al momento della
registrazione, che si ri-registrano quando un driver comincia a fallire su un
CLI aggiornato, e che **non si modificano a mano** — una fixture ritoccata per
far passare un test smette di essere una registrazione del mondo e diventa
un'opinione su di esso.

- [ ] **Passo 4: commit**

```bash
git add Scripts/record-fixture.sh docs/fixture-chat/
git commit -m "chore: registrazione delle fixture dai cli veri"
```

---

## Task 16: i criteri end-to-end

**File:**
- Crea: `e2e/fase-5a-chat.spec.ts`

**Interfacce:**
- Consuma: `tillerctl` e i metodi `chat.*`.

**Contesto:** stesso stampo di `e2e/fase-4b-viste.spec.ts`, con socket **e**
database in cartella temporanea e `--user-data-dir` isolato. Ometterne uno fa
girare i test sul database reale dell'utente ed è già costato una diagnosi
lunga (commit `862e79c`).

**Trappola nota dalla Fase 3:** un criterio che usa la scorciatoia sbagliata è
vacuo. Qui la scorciatoia sarebbe far girare i criteri contro un driver finto:
proverebbero l'infrastruttura, non il protocollo. Il criterio 1 deve parlare
con **omp vero**.

- [ ] **Passo 1: scrivere i criteri**

```ts
test('criterio 1: un prompt a omp riceve una risposta', async () => {
  // Con omp VERO, non con un finto: un criterio su un driver finto prova
  // l infrastruttura e non il protocollo, ed e vacuo per definizione.
  const { worktreeId } = await worktreeReale()
  const creata = JSON.parse(
    await tillerctl(['chat-create', '--worktree', worktreeId, '--agent', 'omp'])
  )
  expect(creata.sessionId).toBeTruthy()

  await tillerctl(['chat-prompt', '--session', creata.sessionId, '--text', 'rispondi solo: ok'])

  await attendi(async () => {
    const stato = JSON.parse(await tillerctl(['chat-get', '--session', creata.sessionId]))
    return stato.lastStopReason !== null
  }, 120_000)
})

test('criterio 2: un agente non installato risponde con il binario mancante', async () => {
  // L errore e una risposta leggibile, non uno stack trace.
  const { worktreeId } = await worktreeReale()
  await expect(
    tillerctl(['chat-create', '--worktree', worktreeId, '--agent', 'inesistente'])
  ).rejects.toThrow(/inesistente/)
})

test('criterio 3: la sessione sopravvive alla chiusura della finestra', async () => {
  // La regola dell architettura: lo stato persistito e del main. Chiudere la
  // finestra non deve perdere la sessione.
  const { worktreeId } = await worktreeReale()
  const creata = JSON.parse(
    await tillerctl(['chat-create', '--worktree', worktreeId, '--agent', 'omp'])
  )
  await closeElectronCleanly(app)
  await launch()
  const stato = JSON.parse(await tillerctl(['chat-get', '--session', creata.sessionId]))
  expect(stato.agentId).toBe('omp')
})

test('criterio 4: i pezzi di testo non arrivano uno per volta', async () => {
  // Il criterio che protegge la coalescenza. Senza di esso puo sparire senza
  // che nulla diventi rosso — ed e il difetto che in Swift ha prodotto un
  // freeze reale il 2026-07-24.
  const window = await app.firstWindow()
  await window.evaluate(() => {
    const w = window as unknown as { __consegne: number; __pezzi: number }
    w.__consegne = 0
    w.__pezzi = 0
    window.tiller.onChatChunks((payload: { chunks: unknown[] }) => {
      w.__consegne += 1
      w.__pezzi += payload.chunks.length
    })
  })

  const { worktreeId } = await worktreeReale()
  const creata = JSON.parse(
    await tillerctl(['chat-create', '--worktree', worktreeId, '--agent', 'omp'])
  )
  await tillerctl([
    'chat-prompt',
    '--session',
    creata.sessionId,
    '--text',
    'conta da uno a cinquanta, un numero per riga'
  ])

  await attendi(async () => {
    const m = await window.evaluate(
      () => (window as unknown as { __pezzi: number }).__pezzi
    )
    return m > 20
  }, 120_000)

  const misure = await window.evaluate(() => ({
    consegne: (window as unknown as { __consegne: number }).__consegne,
    pezzi: (window as unknown as { __pezzi: number }).__pezzi
  }))
  expect(misure.consegne).toBeLessThan(misure.pezzi / 3)
})
```

- [ ] **Passo 2: eseguirli**

Comando: `npx playwright test e2e/fase-5a-chat.spec.ts --reporter=list`
Atteso: 4 verdi.

- [ ] **Passo 3: gate completo**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 4: committare PRIMA di mutare**

```bash
git add e2e/
git commit -m "test: i criteri end-to-end della chat"
```

`git checkout <file>` su lavoro non committato lo **cancella**: è già successo
due volte in questo progetto.

- [ ] **Passo 5: validare i criteri per mutazione**

Due mutazioni, ognuna su un baseline verde. La build per mutare è
`npx electron-vite build`, **non** `pnpm build`: quest'ultimo include il
typecheck, e una mutazione che rende una funzione inutilizzata lo fa fallire —
Playwright girerebbe sul vecchio `out/` e passerebbe. La mutazione deve anche
**compilare**: un commento al posto di un attributo è un errore di sintassi,
stessa dinamica.

| Mutazione | Criterio | Atteso |
|---|---|---|
| in `pompaEventi`, mandare i chunk uno per uno invece di accumularli | 4 | **rosso** |
| in `driver-factory`, far lanciare un'eccezione invece di restituire `{ok:false}` | 2 | **rosso** |

Ripristinare con `git checkout <file>` e verificare `git status -sb` pulito.

- [ ] **Passo 6: gate finale**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

---

## Checklist manuale (una volta, con i CLI veri)

Gli e2e provano il protocollo; solo l'uso prova l'esperienza.

- [ ] `tillerctl chat-create --agent omp` in un worktree vero apre una sessione
      e risponde in meno di cinque secondi
- [ ] un prompt lungo non fa salire la CPU del renderer mentre arriva
- [ ] uccidere il processo dell'agente a metà turno lascia la sessione
      leggibile invece di piantare l'app
- [ ] chiudere la finestra e riaprirla ritrova la sessione con lo stesso id
- [ ] `tillerctl chat-close` termina davvero il processo figlio (verificare con
      `ps`, non solo con l'assenza dalla lista)
