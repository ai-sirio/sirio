# Tiller Electron — Fase 0: Walking Skeleton — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Attraversare una volta sola tutti i confini di processo di Tiller in Electron — CLI → socket unix → main → PTY → renderer — con il caso più sottile possibile, in modo che la decisione architetturale sulla collocazione dello stato sia dimostrata o falsificata prima di scrivere qualunque feature.

**Architecture:** Tre processi Electron (`main`, `preload`, `renderer`) più una CLI. Un solo protocollo (`ControlRequest`, schemi Zod in `src/shared/`) servito da due trasporti: socket unix per `tillerctl` e gli hook degli agenti, IPC Electron per il renderer. Lo stato autoritativo (quali pane esistono) vive nel main; il renderer ne è una proiezione osservabile. L'output PTY viaggia su un canale separato ad alto volume, coalescato al confine IPC.

**Tech Stack:** Electron 43 (Node 24.18.1), TypeScript strict, Svelte 5 (rune), Vite via electron-vite 5, node-pty 1.1, @xterm/xterm 6 + addon WebGL, Zod, Vitest 4, Playwright 1.62, pnpm.

## Global Constraints

- **Cartella di lavoro:** tutto il codice va in `~/Desktop/Progetti/tiller-electron/`, repo git **nuovo e indipendente**. Il repo Swift `~/Desktop/Progetti/tiller/` non va mai modificato.
- **Piattaforma:** macOS-first. Path e spawn vanno scritti astraendo la piattaforma, ma Windows e Linux non si testano né si spediscono in questa fase.
- **Framework UI:** Svelte 5 con le **rune** (`$state`, `$derived`, `$effect`). Non Svelte 4 (niente `export let`, niente store `writable` per lo stato locale). Non SvelteKit.
- **TypeScript strict:** `strict: true`, nessun `any` implicito, nessun `@ts-ignore` nuovo.
- **Versioni di Node:** il runtime di sistema è Node 26.5.0 (Homebrew, `/opt/homebrew/bin/node`), Electron 43 include Node 24.18.1. `@types/node` va tenuto a **`^24`**, cioè al pavimento: così TypeScript rifiuta a compile-time ogni API che il Node di Electron non ha, invece di lasciarla fallire a runtime dentro l'app. Non allineare le versioni, non alzare `@types/node` a 26.
- **Validazione ai confini:** ogni messaggio che entra dal socket o dall'IPC va validato con Zod prima di essere usato. Mai fidarsi del payload.
- **Immutabilità:** nessuna mutazione in place delle strutture di dominio; le funzioni restituiscono nuove copie.
- **Commit:** Conventional Commits, soggetto imperativo minuscolo (`feat:`, `fix:`, `test:`, `chore:`, `docs:`).
- **Gate unico:** `scripts/ci.sh` deve stampare `CI OK`. Nessun task è finito se il gate non passa.
- **Per ogni file `.svelte`:** usare il server MCP Svelte e le skill `svelte-core-bestpractices` / `svelte-code-writer` già configurate.
- **Fuori scope in Fase 0:** split, tab, sidebar, persistenza SQLite, adapter agenti, detection agenti, chat, temi. Un pane solo, nessun layout, nessun database.

## Struttura dei file

| file | responsabilità |
|---|---|
| `src/shared/protocol.ts` | schemi Zod: `ControlRequest`, `ControlResponse`, `StateEvent`, `PtyOutput` |
| `src/shared/paths.ts` | risoluzione del path del socket, condivisa fra app e CLI |
| `src/main/state/app-state.ts` | stato autoritativo dei pane + notifica ai subscriber (puro, niente IO) |
| `src/main/pty/pty-spawner.ts` | interfaccia `PtySpawner` + implementazione node-pty |
| `src/main/pty/pty-manager.ts` | ciclo di vita dei PTY per pane (spawner iniettato) |
| `src/main/control/dispatch.ts` | logica del protocollo: `ControlRequest` → effetti → `ControlResponse` (puro rispetto al trasporto) |
| `src/main/control/socket-server.ts` | trasporto socket unix: framing line-delimited JSON |
| `src/main/control/ipc-transport.ts` | trasporto Electron IPC + coalescing dell'output PTY |
| `src/main/window/window-controller.ts` | apertura/focus finestra, policy headless |
| `src/main/index.ts` | composizione: crea le parti e le collega |
| `src/preload/index.ts` | bridge tipizzato, unica superficie esposta al renderer |
| `src/renderer/src/lib/app-model.svelte.ts` | proiezione osservabile dello stato del main |
| `src/renderer/src/components/Terminal.svelte` | rendering xterm.js di un pane |
| `cli/tillerctl.ts` | client CLI del socket |
| `scripts/ci.sh` | gate unico |

La separazione fra `dispatch.ts` e i due file di trasporto è il punto in cui la decisione "un protocollo, due trasporti" diventa codice: il dispatcher non sa se la richiesta è arrivata da un socket o dall'IPC.

---

### Task 1: Scaffolding, repo e gate CI

**Files:**
- Create: `~/Desktop/Progetti/tiller-electron/` (intero scaffold da template)
- Create: `scripts/ci.sh`
- Create: `vitest.config.ts`
- Create: `src/shared/version.test.ts`
- Modify: `package.json` (versioni, script)
- Modify: `electron.vite.config.ts`

**Interfaces:**
- Consumes: niente (primo task)
- Produces: progetto compilabile, `pnpm test:unit` funzionante, `scripts/ci.sh` che stampa `CI OK`

- [ ] **Step 1: Creare il progetto dal template**

```bash
cd ~/Desktop/Progetti
pnpm create @quick-start/electron tiller-electron --template svelte-ts
cd tiller-electron
```

Se il comando chiede conferme interattive, rispondere: nome progetto `tiller-electron`, ESLint sì, Prettier sì.

- [ ] **Step 2: Allineare le versioni a Electron 43 / Node 24**

Modificare `package.json`, sezione `devDependencies`:

```json
"electron": "^43.2.0",
"@types/node": "^24.0.0"
```

Aggiungere le dipendenze runtime e di test:

```bash
pnpm add zod@^4.4.3 node-pty@^1.1.0 @xterm/xterm@^6.0.0 @xterm/addon-webgl@^0.19.0 @xterm/addon-fit@^0.11.0
pnpm add -D vitest@^4.1.10 @playwright/test@^1.62.1
```

`zod` e `node-pty` vanno in `dependencies` (non `devDependencies`): node-pty è un modulo nativo che deve essere impacchettato e ricompilato per l'ABI di Electron, e lo script `postinstall` del template (`electron-builder install-app-deps`) se ne occupa già.

- [ ] **Step 3: Externalizzare le dipendenze native in electron-vite**

Sostituire `electron.vite.config.ts`:

```typescript
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()]
  },
  preload: {
    plugins: [externalizeDepsPlugin()]
  },
  renderer: {
    plugins: [svelte()]
  }
})
```

`externalizeDepsPlugin()` impedisce a Vite di provare a bundlare `node-pty`: un `.node` compilato non è bundlabile e il build fallirebbe.

- [ ] **Step 4: Configurare Vitest**

Creare `vitest.config.ts`:

```typescript
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    include: ['src/**/*.test.ts', 'cli/**/*.test.ts'],
    exclude: ['**/node_modules/**', 'e2e/**'],
    environment: 'node'
  }
})
```

Gli e2e sono esclusi di proposito: girano con Playwright dentro Electron, non con Vitest.

- [ ] **Step 5: Aggiungere gli script npm**

In `package.json`, sezione `scripts`, sostituire `typecheck` e aggiungere i test:

```json
"typecheck:node": "tsc --noEmit -p tsconfig.node.json --composite false",
"typecheck": "pnpm typecheck:node && pnpm svelte-check",
"test:unit": "vitest run",
"test:e2e": "playwright test",
"ci": "bash scripts/ci.sh"
```

Nota: il template usa `npm run` dentro gli script; sostituire con `pnpm` per coerenza.

- [ ] **Step 6: Scrivere il test che prova che Vitest gira**

Creare `src/shared/version.test.ts`:

```typescript
import { expect, test } from 'vitest'
import { PROTOCOL_VERSION } from './version'

test('il protocollo dichiara una versione', () => {
  expect(PROTOCOL_VERSION).toBe(1)
})
```

- [ ] **Step 7: Eseguire il test e verificare che fallisca**

Run: `pnpm test:unit`
Expected: FAIL — `Failed to resolve import "./version"`

- [ ] **Step 8: Implementare il minimo**

Creare `src/shared/version.ts`:

```typescript
export const PROTOCOL_VERSION = 1
```

- [ ] **Step 9: Eseguire il test e verificare che passi**

Run: `pnpm test:unit`
Expected: PASS, 1 test

- [ ] **Step 10: Creare il gate CI**

Creare `scripts/ci.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> typecheck"
pnpm typecheck

echo "==> lint"
pnpm lint

echo "==> unit test"
pnpm test:unit

echo "==> e2e"
pnpm test:e2e

echo "CI OK"
```

Renderlo eseguibile:

```bash
chmod +x scripts/ci.sh
```

Finché non esiste nessun test e2e (fino al Task 12), commentare temporaneamente la riga `pnpm test:e2e` con `#` e rimuovere il commento nel Task 12.

- [ ] **Step 11: Verificare il gate**

Run: `bash scripts/ci.sh`
Expected: termina stampando `CI OK`

- [ ] **Step 12: Inizializzare il repo e committare**

```bash
git init
git add -A
git commit -m "chore: scaffold electron + svelte 5 project with ci gate"
```

- [ ] **Step 13: Copiare la spec di design nel nuovo repo**

```bash
mkdir -p docs
cp ~/Desktop/Progetti/tiller/docs/superpowers/specs/2026-08-01-tiller-electron-migration-design.md docs/
git add docs/
git commit -m "docs: import migration design spec"
```

---

### Task 2: Schemi del protocollo in `shared/`

**Files:**
- Create: `src/shared/protocol.ts`
- Test: `src/shared/protocol.test.ts`

**Interfaces:**
- Consumes: `PROTOCOL_VERSION` da `src/shared/version.ts`
- Produces:
  - `ControlRequest` (schema Zod) e `type ControlRequest`
  - `ControlResponse` (schema Zod) e `type ControlResponse`
  - `StateEvent` (schema Zod) e `type StateEvent`
  - `PtyOutput` (schema Zod) e `type PtyOutput`
  - `type PaneSnapshot = { id: string; cwd: string; createdAt: number }`

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `src/shared/protocol.test.ts`:

```typescript
import { expect, test } from 'vitest'
import { ControlRequest, ControlResponse, StateEvent } from './protocol'

test('accetta una richiesta pane.create valida', () => {
  const parsed = ControlRequest.parse({
    id: 'req-1',
    method: 'pane.create',
    params: { cwd: '/tmp', focus: false }
  })
  expect(parsed.method).toBe('pane.create')
})

test('rifiuta un metodo sconosciuto', () => {
  expect(() =>
    ControlRequest.parse({ id: 'req-1', method: 'pane.explode', params: {} })
  ).toThrow()
})

test('rifiuta pane.write senza paneId', () => {
  expect(() =>
    ControlRequest.parse({ id: 'req-1', method: 'pane.write', params: { data: 'ciao' } })
  ).toThrow()
})

test('una risposta di errore trasporta il messaggio', () => {
  const parsed = ControlResponse.parse({ id: 'req-1', ok: false, error: 'pane inesistente' })
  expect(parsed.ok).toBe(false)
})

test('lo snapshot di stato elenca i pane', () => {
  const parsed = StateEvent.parse({
    type: 'snapshot',
    panes: [{ id: 'p1', cwd: '/tmp', createdAt: 0 }]
  })
  expect(parsed.type).toBe('snapshot')
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/shared/protocol.test.ts`
Expected: FAIL — `Failed to resolve import "./protocol"`

- [ ] **Step 3: Implementare gli schemi**

Creare `src/shared/protocol.ts`:

```typescript
import { z } from 'zod'

export const PaneSnapshot = z.object({
  id: z.string().min(1),
  cwd: z.string().min(1),
  createdAt: z.number().int().nonnegative()
})
export type PaneSnapshot = z.infer<typeof PaneSnapshot>

export const ControlRequest = z.discriminatedUnion('method', [
  z.object({
    id: z.string().min(1),
    method: z.literal('pane.create'),
    params: z.object({
      cwd: z.string().min(1),
      focus: z.boolean().default(false)
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('pane.write'),
    params: z.object({
      paneId: z.string().min(1),
      data: z.string()
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('state.get'),
    params: z.object({})
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('window.focus'),
    params: z.object({})
  })
])
export type ControlRequest = z.infer<typeof ControlRequest>

export const ControlResponse = z.union([
  z.object({ id: z.string().min(1), ok: z.literal(true), result: z.unknown() }),
  z.object({ id: z.string().min(1), ok: z.literal(false), error: z.string() })
])
export type ControlResponse = z.infer<typeof ControlResponse>

export const StateEvent = z.discriminatedUnion('type', [
  z.object({ type: z.literal('snapshot'), panes: z.array(PaneSnapshot) }),
  z.object({ type: z.literal('pane.added'), pane: PaneSnapshot }),
  z.object({ type: z.literal('pane.removed'), paneId: z.string().min(1) })
])
export type StateEvent = z.infer<typeof StateEvent>

/**
 * Canale separato ad alto volume: NON passa da StateEvent.
 * L'output PTY arriva a raffica e va coalescato al confine IPC,
 * lo stato autoritativo no.
 */
export const PtyOutput = z.object({
  paneId: z.string().min(1),
  data: z.string()
})
export type PtyOutput = z.infer<typeof PtyOutput>
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/shared/protocol.test.ts`
Expected: PASS, 5 test

- [ ] **Step 5: Commit**

```bash
git add src/shared/protocol.ts src/shared/protocol.test.ts
git commit -m "feat: add zod schemas for the control protocol"
```

---

### Task 3: Stato autoritativo nel main

**Files:**
- Create: `src/main/state/app-state.ts`
- Test: `src/main/state/app-state.test.ts`

**Interfaces:**
- Consumes: `PaneSnapshot`, `StateEvent` da `src/shared/protocol.ts`
- Produces: classe `AppState` con
  - `addPane(pane: PaneSnapshot): void`
  - `removePane(paneId: string): void`
  - `hasPane(paneId: string): boolean`
  - `snapshot(): PaneSnapshot[]`
  - `subscribe(listener: (event: StateEvent) => void): () => void`

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `src/main/state/app-state.test.ts`:

```typescript
import { expect, test, vi } from 'vitest'
import { AppState } from './app-state'
import type { StateEvent } from '../../shared/protocol'

const pane = { id: 'p1', cwd: '/tmp', createdAt: 100 }

test('uno stato appena creato non ha pane', () => {
  expect(new AppState().snapshot()).toEqual([])
})

test('aggiungere un pane lo rende visibile nello snapshot', () => {
  const state = new AppState()
  state.addPane(pane)
  expect(state.snapshot()).toEqual([pane])
})

test('aggiungere un pane notifica i subscriber', () => {
  const state = new AppState()
  const events: StateEvent[] = []
  state.subscribe((e) => events.push(e))
  state.addPane(pane)
  expect(events).toEqual([{ type: 'pane.added', pane }])
})

test('rimuovere un pane notifica i subscriber', () => {
  const state = new AppState()
  state.addPane(pane)
  const events: StateEvent[] = []
  state.subscribe((e) => events.push(e))
  state.removePane('p1')
  expect(events).toEqual([{ type: 'pane.removed', paneId: 'p1' }])
})

test('rimuovere un pane inesistente non notifica nulla', () => {
  const state = new AppState()
  const listener = vi.fn()
  state.subscribe(listener)
  state.removePane('assente')
  expect(listener).not.toHaveBeenCalled()
})

test('disiscriversi ferma le notifiche', () => {
  const state = new AppState()
  const listener = vi.fn()
  const unsubscribe = state.subscribe(listener)
  unsubscribe()
  state.addPane(pane)
  expect(listener).not.toHaveBeenCalled()
})

test('lo snapshot non è mutabile dall_esterno', () => {
  const state = new AppState()
  state.addPane(pane)
  state.snapshot().pop()
  expect(state.snapshot()).toHaveLength(1)
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/main/state/app-state.test.ts`
Expected: FAIL — `Failed to resolve import "./app-state"`

- [ ] **Step 3: Implementare AppState**

Creare `src/main/state/app-state.ts`:

```typescript
import type { PaneSnapshot, StateEvent } from '../../shared/protocol'

type Listener = (event: StateEvent) => void

/**
 * Stato autoritativo dell'applicazione.
 *
 * Regola di taglio (vedi la spec di design): qui vive solo ciò che verrà
 * persistito. Hover, scroll e selezione restano nel renderer.
 *
 * Nessun IO, nessuna dipendenza da Electron: interamente testabile a unità.
 */
export class AppState {
  readonly #panes = new Map<string, PaneSnapshot>()
  readonly #listeners = new Set<Listener>()

  addPane(pane: PaneSnapshot): void {
    this.#panes.set(pane.id, pane)
    this.#emit({ type: 'pane.added', pane })
  }

  removePane(paneId: string): void {
    if (!this.#panes.delete(paneId)) return
    this.#emit({ type: 'pane.removed', paneId })
  }

  hasPane(paneId: string): boolean {
    return this.#panes.has(paneId)
  }

  snapshot(): PaneSnapshot[] {
    return [...this.#panes.values()]
  }

  subscribe(listener: Listener): () => void {
    this.#listeners.add(listener)
    return () => {
      this.#listeners.delete(listener)
    }
  }

  #emit(event: StateEvent): void {
    for (const listener of this.#listeners) listener(event)
  }
}
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/state/app-state.test.ts`
Expected: PASS, 7 test

- [ ] **Step 5: Commit**

```bash
git add src/main/state/
git commit -m "feat: add authoritative pane state with subscriptions"
```

---

### Task 4: Gestione dei PTY con spawner iniettabile

**Files:**
- Create: `src/main/pty/pty-spawner.ts`
- Create: `src/main/pty/pty-manager.ts`
- Test: `src/main/pty/pty-manager.test.ts`

**Interfaces:**
- Consumes: niente dai task precedenti
- Produces:
  - `interface PtyHandle { write(data: string): void; onData(cb: (d: string) => void): void; onExit(cb: () => void): void; kill(): void }`
  - `interface PtySpawnOptions { cwd: string; shell: string; args: string[]; cols: number; rows: number }`
  - `type PtySpawner = (options: PtySpawnOptions) => PtyHandle`
  - `createNodePtySpawner(): PtySpawner`
  - classe `PtyManager` con `spawn(paneId, options)`, `write(paneId, data)`, `kill(paneId)`, `onData(cb)`, `onExit(cb)`, `disposeAll()`

**Perché lo spawner è iniettato:** per rendere `PtyManager` testabile senza processi reali. Spawnare shell vere in un test a unità significa dipendere da timing, da variabili d'ambiente e dalla shell dell'utente; con uno spawner finto le asserzioni sono deterministiche. Il `node-pty` vero è esercitato dall'e2e Playwright (Task 12).

**Nota verificata sul campo, non assumere il contrario:** `node-pty` 1.1.0 è basato su N-API (`node-addon-api`), quindi lo **stesso** binario prebuilt carica sia in Node 24 sia in Electron 43. Non serve `electron-rebuild` e non c'è nessun conflitto di ABI fra Vitest e Electron. Se qualcuno "scopre" questo e ne deduce che l'iniezione dello spawner è inutile: non lo è, il motivo è il determinismo dei test, non l'ABI.

**Trappola di packaging già incontrata (Task 1):** sotto pnpm, `node-pty/prebuilds/<piattaforma>/spawn-helper` viene installato **senza bit di esecuzione**, e il `postinstall` di node-pty non lo ripristina. Ogni spawn fallisce allora con `posix_spawnp failed`, messaggio che non nomina né il file né la causa. Il rimedio è già in `package.json`:

```json
"postinstall": "electron-builder install-app-deps && chmod +x node_modules/node-pty/prebuilds/*/spawn-helper"
```

più la guardia `src/main/pty/node-pty-packaging.test.ts`, che fa fallire la CI sul permesso invece che sullo spawn.

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `src/main/pty/pty-manager.test.ts`:

```typescript
import { expect, test, vi } from 'vitest'
import { PtyManager } from './pty-manager'
import type { PtyHandle, PtySpawner } from './pty-spawner'

function fakeSpawner(): { spawner: PtySpawner; handles: FakeHandle[] } {
  const handles: FakeHandle[] = []
  const spawner: PtySpawner = () => {
    const handle = new FakeHandle()
    handles.push(handle)
    return handle
  }
  return { spawner, handles }
}

class FakeHandle implements PtyHandle {
  written: string[] = []
  killed = false
  #dataCb: ((d: string) => void) | null = null
  #exitCb: (() => void) | null = null

  write(data: string): void {
    this.written.push(data)
  }
  onData(cb: (d: string) => void): void {
    this.#dataCb = cb
  }
  onExit(cb: () => void): void {
    this.#exitCb = cb
  }
  kill(): void {
    this.killed = true
  }
  emitData(d: string): void {
    this.#dataCb?.(d)
  }
  emitExit(): void {
    this.#exitCb?.()
  }
}

const options = { cwd: '/tmp', shell: '/bin/zsh', args: [], cols: 80, rows: 24 }

test('scrivere su un pane raggiunge il suo pty', () => {
  const { spawner, handles } = fakeSpawner()
  const manager = new PtyManager(spawner)
  manager.spawn('p1', options)
  manager.write('p1', 'echo ciao\n')
  expect(handles[0].written).toEqual(['echo ciao\n'])
})

test('scrivere su un pane inesistente lancia un errore', () => {
  const { spawner } = fakeSpawner()
  const manager = new PtyManager(spawner)
  expect(() => manager.write('assente', 'x')).toThrow('pane sconosciuto: assente')
})

test('l_output del pty viene inoltrato con il suo paneId', () => {
  const { spawner, handles } = fakeSpawner()
  const manager = new PtyManager(spawner)
  const received: Array<[string, string]> = []
  manager.onData((paneId, data) => received.push([paneId, data]))
  manager.spawn('p1', options)
  handles[0].emitData('ciao')
  expect(received).toEqual([['p1', 'ciao']])
})

test('l_uscita del processo notifica e libera il pane', () => {
  const { spawner, handles } = fakeSpawner()
  const manager = new PtyManager(spawner)
  const exited: string[] = []
  manager.onExit((paneId) => exited.push(paneId))
  manager.spawn('p1', options)
  handles[0].emitExit()
  expect(exited).toEqual(['p1'])
  expect(() => manager.write('p1', 'x')).toThrow()
})

test('disposeAll uccide tutti i pty', () => {
  const { spawner, handles } = fakeSpawner()
  const manager = new PtyManager(spawner)
  manager.spawn('p1', options)
  manager.spawn('p2', options)
  manager.disposeAll()
  expect(handles.map((h) => h.killed)).toEqual([true, true])
})

test('spawnare due volte lo stesso paneId lancia un errore', () => {
  const { spawner } = fakeSpawner()
  const manager = new PtyManager(spawner)
  manager.spawn('p1', options)
  expect(() => manager.spawn('p1', options)).toThrow('pane già attivo: p1')
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/main/pty/pty-manager.test.ts`
Expected: FAIL — `Failed to resolve import "./pty-manager"`

- [ ] **Step 3: Definire l'interfaccia dello spawner**

Creare `src/main/pty/pty-spawner.ts`:

```typescript
export interface PtySpawnOptions {
  cwd: string
  shell: string
  args: string[]
  cols: number
  rows: number
}

export interface PtyHandle {
  write(data: string): void
  onData(cb: (data: string) => void): void
  onExit(cb: () => void): void
  kill(): void
}

export type PtySpawner = (options: PtySpawnOptions) => PtyHandle

/**
 * Implementazione reale su node-pty.
 *
 * L'import è dinamico e confinato qui dentro: node-pty è un modulo nativo
 * compilato per l'ABI di Electron, e questo file non viene mai caricato
 * dai test a unità.
 */
export function createNodePtySpawner(): PtySpawner {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const pty = require('node-pty') as typeof import('node-pty')

  return (options: PtySpawnOptions): PtyHandle => {
    const proc = pty.spawn(options.shell, options.args, {
      cwd: options.cwd,
      cols: options.cols,
      rows: options.rows,
      env: { ...process.env, TERM: 'xterm-256color' } as Record<string, string>
    })

    return {
      write: (data) => proc.write(data),
      onData: (cb) => {
        proc.onData(cb)
      },
      onExit: (cb) => {
        proc.onExit(() => cb())
      },
      kill: () => proc.kill()
    }
  }
}

/** Shell di default su macOS. */
export const DEFAULT_SHELL = process.env.SHELL ?? '/bin/zsh'
```

- [ ] **Step 4: Implementare PtyManager**

Creare `src/main/pty/pty-manager.ts`:

```typescript
import type { PtyHandle, PtySpawner, PtySpawnOptions } from './pty-spawner'

type DataListener = (paneId: string, data: string) => void
type ExitListener = (paneId: string) => void

export class PtyManager {
  readonly #spawner: PtySpawner
  readonly #handles = new Map<string, PtyHandle>()
  readonly #dataListeners = new Set<DataListener>()
  readonly #exitListeners = new Set<ExitListener>()

  constructor(spawner: PtySpawner) {
    this.#spawner = spawner
  }

  spawn(paneId: string, options: PtySpawnOptions): void {
    if (this.#handles.has(paneId)) throw new Error(`pane già attivo: ${paneId}`)

    const handle = this.#spawner(options)
    this.#handles.set(paneId, handle)

    handle.onData((data) => {
      for (const listener of this.#dataListeners) listener(paneId, data)
    })
    handle.onExit(() => {
      this.#handles.delete(paneId)
      for (const listener of this.#exitListeners) listener(paneId)
    })
  }

  write(paneId: string, data: string): void {
    const handle = this.#handles.get(paneId)
    if (!handle) throw new Error(`pane sconosciuto: ${paneId}`)
    handle.write(data)
  }

  kill(paneId: string): void {
    this.#handles.get(paneId)?.kill()
  }

  onData(listener: DataListener): () => void {
    this.#dataListeners.add(listener)
    return () => {
      this.#dataListeners.delete(listener)
    }
  }

  onExit(listener: ExitListener): () => void {
    this.#exitListeners.add(listener)
    return () => {
      this.#exitListeners.delete(listener)
    }
  }

  disposeAll(): void {
    for (const handle of this.#handles.values()) handle.kill()
    this.#handles.clear()
  }
}
```

- [ ] **Step 5: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/pty/pty-manager.test.ts`
Expected: PASS, 6 test

- [ ] **Step 6: Commit**

```bash
git add src/main/pty/
git commit -m "feat: add pty manager with injectable spawner"
```

---

### Task 5: Dispatcher del protocollo

**Files:**
- Create: `src/main/control/dispatch.ts`
- Test: `src/main/control/dispatch.test.ts`

**Interfaces:**
- Consumes: `AppState`, `PtyManager`, `ControlRequest`, `ControlResponse`
- Produces:
  - `interface WindowController { focus(): void; isOpen(): boolean }`
  - `interface DispatchDeps { state: AppState; pty: PtyManager; window: WindowController; now(): number; newPaneId(): string; defaultShell: string }`
  - `createDispatcher(deps: DispatchDeps): (request: ControlRequest) => ControlResponse`

Questo è il file che rende reale "un protocollo, due trasporti": non sa da dove arriva la richiesta.

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `src/main/control/dispatch.test.ts`:

```typescript
import { expect, test, vi } from 'vitest'
import { createDispatcher } from './dispatch'
import { AppState } from '../state/app-state'
import { PtyManager } from '../pty/pty-manager'
import type { PtyHandle, PtySpawner } from '../pty/pty-spawner'

class FakeHandle implements PtyHandle {
  written: string[] = []
  write(data: string): void {
    this.written.push(data)
  }
  onData(): void {}
  onExit(): void {}
  kill(): void {}
}

function makeDeps(): {
  deps: Parameters<typeof createDispatcher>[0]
  handles: FakeHandle[]
  focus: ReturnType<typeof vi.fn>
} {
  const handles: FakeHandle[] = []
  const spawner: PtySpawner = () => {
    const handle = new FakeHandle()
    handles.push(handle)
    return handle
  }
  const focus = vi.fn()
  let counter = 0
  return {
    handles,
    focus,
    deps: {
      state: new AppState(),
      pty: new PtyManager(spawner),
      window: { focus, isOpen: () => false },
      now: () => 42,
      newPaneId: () => `p${++counter}`,
      defaultShell: '/bin/zsh'
    }
  }
}

test('pane.create registra il pane nello stato e risponde con il suo id', () => {
  const { deps } = makeDeps()
  const dispatch = createDispatcher(deps)
  const response = dispatch({
    id: 'r1',
    method: 'pane.create',
    params: { cwd: '/tmp', focus: false }
  })
  expect(response).toEqual({ id: 'r1', ok: true, result: { paneId: 'p1' } })
  expect(deps.state.snapshot()).toEqual([{ id: 'p1', cwd: '/tmp', createdAt: 42 }])
})

test('pane.create senza focus non apre la finestra', () => {
  const { deps, focus } = makeDeps()
  createDispatcher(deps)({ id: 'r1', method: 'pane.create', params: { cwd: '/tmp', focus: false } })
  expect(focus).not.toHaveBeenCalled()
})

test('pane.create con focus apre la finestra', () => {
  const { deps, focus } = makeDeps()
  createDispatcher(deps)({ id: 'r1', method: 'pane.create', params: { cwd: '/tmp', focus: true } })
  expect(focus).toHaveBeenCalledOnce()
})

test('pane.write inoltra i dati al pty del pane', () => {
  const { deps, handles } = makeDeps()
  const dispatch = createDispatcher(deps)
  dispatch({ id: 'r1', method: 'pane.create', params: { cwd: '/tmp', focus: false } })
  const response = dispatch({
    id: 'r2',
    method: 'pane.write',
    params: { paneId: 'p1', data: 'echo ciao\n' }
  })
  expect(response).toEqual({ id: 'r2', ok: true, result: null })
  expect(handles[0].written).toEqual(['echo ciao\n'])
})

test('pane.write su un pane inesistente risponde con errore, non lancia', () => {
  const { deps } = makeDeps()
  const response = createDispatcher(deps)({
    id: 'r1',
    method: 'pane.write',
    params: { paneId: 'assente', data: 'x' }
  })
  expect(response).toEqual({ id: 'r1', ok: false, error: 'pane sconosciuto: assente' })
})

test('state.get restituisce lo snapshot corrente', () => {
  const { deps } = makeDeps()
  const dispatch = createDispatcher(deps)
  dispatch({ id: 'r1', method: 'pane.create', params: { cwd: '/tmp', focus: false } })
  const response = dispatch({ id: 'r2', method: 'state.get', params: {} })
  expect(response).toEqual({
    id: 'r2',
    ok: true,
    result: { panes: [{ id: 'p1', cwd: '/tmp', createdAt: 42 }] }
  })
})

test('window.focus apre la finestra', () => {
  const { deps, focus } = makeDeps()
  createDispatcher(deps)({ id: 'r1', method: 'window.focus', params: {} })
  expect(focus).toHaveBeenCalledOnce()
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/main/control/dispatch.test.ts`
Expected: FAIL — `Failed to resolve import "./dispatch"`

- [ ] **Step 3: Implementare il dispatcher**

Creare `src/main/control/dispatch.ts`:

```typescript
import type { ControlRequest, ControlResponse } from '../../shared/protocol'
import type { AppState } from '../state/app-state'
import type { PtyManager } from '../pty/pty-manager'

export interface WindowController {
  focus(): void
  isOpen(): boolean
}

export interface DispatchDeps {
  state: AppState
  pty: PtyManager
  window: WindowController
  now(): number
  newPaneId(): string
  defaultShell: string
}

const DEFAULT_COLS = 80
const DEFAULT_ROWS = 24

/**
 * Traduce una ControlRequest in effetti sullo stato e sui PTY.
 *
 * Non sa da quale trasporto arriva la richiesta: socket unix (tillerctl,
 * hook agenti) e IPC Electron (renderer) chiamano entrambi questa funzione.
 * È il punto in cui "un protocollo, due trasporti" diventa codice.
 *
 * Non lancia mai: ogni errore diventa una ControlResponse con ok: false,
 * perché un client remoto deve ricevere una risposta, non veder cadere
 * la connessione.
 */
export function createDispatcher(deps: DispatchDeps): (request: ControlRequest) => ControlResponse {
  return (request: ControlRequest): ControlResponse => {
    try {
      switch (request.method) {
        case 'pane.create': {
          const pane = {
            id: deps.newPaneId(),
            cwd: request.params.cwd,
            createdAt: deps.now()
          }
          deps.pty.spawn(pane.id, {
            cwd: pane.cwd,
            shell: deps.defaultShell,
            args: [],
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS
          })
          deps.state.addPane(pane)
          if (request.params.focus) deps.window.focus()
          return { id: request.id, ok: true, result: { paneId: pane.id } }
        }

        case 'pane.write': {
          deps.pty.write(request.params.paneId, request.params.data)
          return { id: request.id, ok: true, result: null }
        }

        case 'state.get': {
          return { id: request.id, ok: true, result: { panes: deps.state.snapshot() } }
        }

        case 'window.focus': {
          deps.window.focus()
          return { id: request.id, ok: true, result: null }
        }
      }
    } catch (error) {
      return {
        id: request.id,
        ok: false,
        error: error instanceof Error ? error.message : String(error)
      }
    }
  }
}
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/control/dispatch.test.ts`
Expected: PASS, 7 test

- [ ] **Step 5: Commit**

```bash
git add src/main/control/dispatch.ts src/main/control/dispatch.test.ts
git commit -m "feat: add transport-agnostic control request dispatcher"
```

---

### Task 6: Trasporto socket unix

**Files:**
- Create: `src/shared/paths.ts`
- Create: `src/main/control/framing.ts`
- Create: `src/main/control/socket-server.ts`
- Test: `src/main/control/framing.test.ts`
- Test: `src/main/control/socket-server.test.ts`

**Interfaces:**
- Consumes: `createDispatcher` da `dispatch.ts`, `ControlRequest`/`ControlResponse` da `shared/protocol.ts`
- Produces:
  - `resolveSocketPath(): string` da `src/shared/paths.ts`
  - `createLineDecoder(onLine: (line: string) => void): (chunk: string) => void` da `framing.ts`
  - `startSocketServer(options: { socketPath: string; handle: (req: ControlRequest) => ControlResponse }): Promise<{ close(): Promise<void> }>`

- [ ] **Step 1: Scrivere il test del framing**

Creare `src/main/control/framing.test.ts`:

```typescript
import { expect, test } from 'vitest'
import { createLineDecoder } from './framing'

test('emette una riga completa', () => {
  const lines: string[] = []
  const decode = createLineDecoder((l) => lines.push(l))
  decode('ciao\n')
  expect(lines).toEqual(['ciao'])
})

test('accumula una riga arrivata in più pezzi', () => {
  const lines: string[] = []
  const decode = createLineDecoder((l) => lines.push(l))
  decode('ci')
  decode('ao')
  expect(lines).toEqual([])
  decode('\n')
  expect(lines).toEqual(['ciao'])
})

test('emette più righe da un solo chunk', () => {
  const lines: string[] = []
  const decode = createLineDecoder((l) => lines.push(l))
  decode('uno\ndue\ntre\n')
  expect(lines).toEqual(['uno', 'due', 'tre'])
})

test('ignora le righe vuote', () => {
  const lines: string[] = []
  const decode = createLineDecoder((l) => lines.push(l))
  decode('uno\n\ndue\n')
  expect(lines).toEqual(['uno', 'due'])
})
```

- [ ] **Step 2: Eseguire il test e verificare che fallisca**

Run: `pnpm test:unit src/main/control/framing.test.ts`
Expected: FAIL — `Failed to resolve import "./framing"`

- [ ] **Step 3: Implementare il framing**

Creare `src/main/control/framing.ts`:

```typescript
/**
 * Decoder line-delimited: i messaggi TCP/unix arrivano frammentati,
 * un JSON può essere spezzato a metà fra due chunk.
 */
export function createLineDecoder(onLine: (line: string) => void): (chunk: string) => void {
  let buffer = ''

  return (chunk: string): void => {
    buffer += chunk
    let index = buffer.indexOf('\n')
    while (index !== -1) {
      const line = buffer.slice(0, index)
      buffer = buffer.slice(index + 1)
      if (line.length > 0) onLine(line)
      index = buffer.indexOf('\n')
    }
  }
}
```

- [ ] **Step 4: Verificare che il test del framing passi**

Run: `pnpm test:unit src/main/control/framing.test.ts`
Expected: PASS, 4 test

- [ ] **Step 5: Implementare il resolver del path del socket**

Creare `src/shared/paths.ts`:

```typescript
import { homedir } from 'node:os'
import { join } from 'node:path'

const APP_DIR_NAME = 'tiller-electron'

/**
 * Path del socket di controllo, condiviso fra app e CLI.
 *
 * Non usa `app.getPath('userData')` di proposito: tillerctl gira fuori da
 * Electron e deve calcolare lo stesso path senza importare `electron`.
 * Override con $TILLER_SOCKET (usato dai test e2e per isolarsi).
 */
export function resolveSocketPath(): string {
  const override = process.env.TILLER_SOCKET
  if (override && override.length > 0) return override

  if (process.platform === 'darwin') {
    return join(homedir(), 'Library', 'Application Support', APP_DIR_NAME, 'control.sock')
  }
  // macOS-first: gli altri sistemi useranno XDG, non testato in Fase 0.
  return join(homedir(), '.local', 'state', APP_DIR_NAME, 'control.sock')
}
```

- [ ] **Step 6: Scrivere il test del server socket**

Creare `src/main/control/socket-server.test.ts`:

```typescript
import { afterEach, expect, test } from 'vitest'
import net from 'node:net'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startSocketServer } from './socket-server'
import type { ControlRequest, ControlResponse } from '../../shared/protocol'

let cleanup: Array<() => Promise<void>> = []

afterEach(async () => {
  for (const fn of cleanup) await fn()
  cleanup = []
})

async function withServer(
  handle: (req: ControlRequest) => ControlResponse
): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-sock-'))
  const socketPath = join(dir, 'control.sock')
  const server = await startSocketServer({ socketPath, handle })
  cleanup.push(async () => {
    await server.close()
    await rm(dir, { recursive: true, force: true })
  })
  return socketPath
}

function request(socketPath: string, payload: unknown): Promise<string> {
  return new Promise((resolve, reject) => {
    const client = net.connect(socketPath, () => {
      client.write(JSON.stringify(payload) + '\n')
    })
    client.setEncoding('utf8')
    client.once('data', (data: string) => {
      client.end()
      resolve(data.trim())
    })
    client.once('error', reject)
  })
}

test('una richiesta valida riceve la risposta del gestore', async () => {
  const socketPath = await withServer((req) => ({ id: req.id, ok: true, result: 'fatto' }))
  const raw = await request(socketPath, {
    id: 'r1',
    method: 'state.get',
    params: {}
  })
  expect(JSON.parse(raw)).toEqual({ id: 'r1', ok: true, result: 'fatto' })
})

test('una richiesta malformata riceve un errore, il server resta vivo', async () => {
  const socketPath = await withServer((req) => ({ id: req.id, ok: true, result: null }))
  const raw = await request(socketPath, { id: 'r1', method: 'pane.explode', params: {} })
  const parsed = JSON.parse(raw) as ControlResponse
  expect(parsed.ok).toBe(false)

  const second = await request(socketPath, { id: 'r2', method: 'state.get', params: {} })
  expect((JSON.parse(second) as ControlResponse).ok).toBe(true)
})

test('un socket stantio viene rimosso e il server riparte', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-sock-'))
  const socketPath = join(dir, 'control.sock')
  const handle = (req: ControlRequest): ControlResponse => ({ id: req.id, ok: true, result: null })

  const first = await startSocketServer({ socketPath, handle })
  await first.close()
  const second = await startSocketServer({ socketPath, handle })
  cleanup.push(async () => {
    await second.close()
    await rm(dir, { recursive: true, force: true })
  })

  const raw = await request(socketPath, { id: 'r1', method: 'state.get', params: {} })
  expect((JSON.parse(raw) as ControlResponse).ok).toBe(true)
})
```

- [ ] **Step 7: Eseguire il test e verificare che fallisca**

Run: `pnpm test:unit src/main/control/socket-server.test.ts`
Expected: FAIL — `Failed to resolve import "./socket-server"`

- [ ] **Step 8: Implementare il server socket**

Creare `src/main/control/socket-server.ts`:

```typescript
import net from 'node:net'
import { mkdir, unlink } from 'node:fs/promises'
import { dirname } from 'node:path'
import { ControlRequest, type ControlResponse } from '../../shared/protocol'
import { createLineDecoder } from './framing'

export interface SocketServerOptions {
  socketPath: string
  handle: (request: ControlRequest) => ControlResponse
}

export interface SocketServer {
  close(): Promise<void>
}

export async function startSocketServer(options: SocketServerOptions): Promise<SocketServer> {
  await mkdir(dirname(options.socketPath), { recursive: true })
  // Un socket rimasto da un crash impedirebbe il listen: va rimosso.
  await unlink(options.socketPath).catch(() => undefined)

  const server = net.createServer((socket) => {
    socket.setEncoding('utf8')

    const decode = createLineDecoder((line) => {
      const response = handleLine(line, options.handle)
      socket.write(JSON.stringify(response) + '\n')
    })

    socket.on('data', (chunk: string) => decode(chunk))
    // Un client che muore non deve abbattere il server.
    socket.on('error', () => socket.destroy())
  })

  await new Promise<void>((resolve, reject) => {
    server.once('error', reject)
    server.listen(options.socketPath, () => {
      server.removeListener('error', reject)
      resolve()
    })
  })

  return {
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => resolve())
      })
  }
}

function handleLine(
  line: string,
  handle: (request: ControlRequest) => ControlResponse
): ControlResponse {
  let payload: unknown
  try {
    payload = JSON.parse(line)
  } catch {
    return { id: 'unknown', ok: false, error: 'JSON non valido' }
  }

  const parsed = ControlRequest.safeParse(payload)
  if (!parsed.success) {
    const id =
      typeof payload === 'object' && payload !== null && 'id' in payload
        ? String((payload as { id: unknown }).id)
        : 'unknown'
    return { id, ok: false, error: `richiesta non valida: ${parsed.error.issues[0]?.message}` }
  }

  return handle(parsed.data)
}
```

- [ ] **Step 9: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/control/`
Expected: PASS, tutti i test di `framing`, `dispatch` e `socket-server`

- [ ] **Step 10: Commit**

```bash
git add src/shared/paths.ts src/main/control/framing.ts src/main/control/framing.test.ts src/main/control/socket-server.ts src/main/control/socket-server.test.ts
git commit -m "feat: serve the control protocol over a unix socket"
```

---

### Task 7: CLI `tillerctl`

**Files:**
- Create: `cli/tillerctl.ts`
- Create: `cli/args.ts`
- Test: `cli/args.test.ts`
- Test: `cli/tillerctl.test.ts`
- Modify: `package.json` (script `tillerctl`, `build:cli`)
- Modify: `electron.vite.config.ts` (nessuna modifica — la CLI si compila a parte)

**Interfaces:**
- Consumes: `resolveSocketPath` da `src/shared/paths.ts`, `ControlRequest`/`ControlResponse` da `src/shared/protocol.ts`
- Produces:
  - `parseCliArgs(argv: string[]): CliCommand` da `cli/args.ts`, dove
    `type CliCommand = { kind: 'run'; cmd: string; cwd: string; focus: boolean } | { kind: 'help' }`
  - `sendRequest(socketPath: string, request: ControlRequest): Promise<ControlResponse>` da `cli/tillerctl.ts`

Il parsing usa `parseArgs` di `node:util`, stabile in Node 24: nessuna dipendenza da aggiungere.

- [ ] **Step 1: Scrivere il test del parsing**

Creare `cli/args.test.ts`:

```typescript
import { expect, test } from 'vitest'
import { parseCliArgs } from './args'

test('run richiede --cmd', () => {
  expect(() => parseCliArgs(['run'])).toThrow('--cmd è obbligatorio')
})

test('run senza --focus è headless', () => {
  const command = parseCliArgs(['run', '--cmd', 'echo ciao'])
  expect(command).toEqual({ kind: 'run', cmd: 'echo ciao', cwd: process.cwd(), focus: false })
})

test('run con --focus chiede la finestra', () => {
  const command = parseCliArgs(['run', '--cmd', 'echo ciao', '--focus'])
  expect(command).toMatchObject({ kind: 'run', focus: true })
})

test('run accetta un cwd esplicito', () => {
  const command = parseCliArgs(['run', '--cmd', 'ls', '--cwd', '/tmp'])
  expect(command).toMatchObject({ cwd: '/tmp' })
})

test('un comando sconosciuto lancia un errore', () => {
  expect(() => parseCliArgs(['esplodi'])).toThrow('comando sconosciuto: esplodi')
})

test('senza argomenti chiede aiuto', () => {
  expect(parseCliArgs([])).toEqual({ kind: 'help' })
})
```

- [ ] **Step 2: Eseguire il test e verificare che fallisca**

Run: `pnpm test:unit cli/args.test.ts`
Expected: FAIL — `Failed to resolve import "./args"`

- [ ] **Step 3: Implementare il parsing**

Creare `cli/args.ts`:

```typescript
import { parseArgs } from 'node:util'

export type CliCommand =
  | { kind: 'run'; cmd: string; cwd: string; focus: boolean }
  | { kind: 'help' }

export function parseCliArgs(argv: string[]): CliCommand {
  const [subcommand, ...rest] = argv
  if (subcommand === undefined) return { kind: 'help' }
  if (subcommand === 'help' || subcommand === '--help') return { kind: 'help' }
  if (subcommand !== 'run') throw new Error(`comando sconosciuto: ${subcommand}`)

  const { values } = parseArgs({
    args: rest,
    options: {
      cmd: { type: 'string' },
      cwd: { type: 'string' },
      focus: { type: 'boolean', default: false }
    },
    strict: true
  })

  if (values.cmd === undefined) throw new Error('--cmd è obbligatorio')

  return {
    kind: 'run',
    cmd: values.cmd,
    cwd: values.cwd ?? process.cwd(),
    focus: values.focus === true
  }
}

export const HELP_TEXT = `tillerctl — controlla Tiller dalla riga di comando

  tillerctl run --cmd "<comando>" [--cwd <path>] [--focus]

    Crea un pane, ci esegue dentro <comando>.
    Senza --focus non apre nessuna finestra: il pane esiste comunque
    e comparirà alla prossima apertura.
`
```

- [ ] **Step 4: Eseguire il test e verificare che passi**

Run: `pnpm test:unit cli/args.test.ts`
Expected: PASS, 6 test

- [ ] **Step 5: Scrivere il test del client socket**

Creare `cli/tillerctl.test.ts`:

```typescript
import { afterEach, expect, test } from 'vitest'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { startSocketServer } from '../src/main/control/socket-server'
import { sendRequest } from './tillerctl'

let cleanup: Array<() => Promise<void>> = []

afterEach(async () => {
  for (const fn of cleanup) await fn()
  cleanup = []
})

test('sendRequest raggiunge il server e riporta la risposta', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-cli-'))
  const socketPath = join(dir, 'control.sock')
  const server = await startSocketServer({
    socketPath,
    handle: (req) => ({ id: req.id, ok: true, result: { paneId: 'p1' } })
  })
  cleanup.push(async () => {
    await server.close()
    await rm(dir, { recursive: true, force: true })
  })

  const response = await sendRequest(socketPath, {
    id: 'r1',
    method: 'pane.create',
    params: { cwd: '/tmp', focus: false }
  })

  expect(response).toEqual({ id: 'r1', ok: true, result: { paneId: 'p1' } })
})

test('sendRequest fallisce con un messaggio chiaro se l_app non è in ascolto', async () => {
  await expect(
    sendRequest('/tmp/tiller-inesistente.sock', {
      id: 'r1',
      method: 'state.get',
      params: {}
    })
  ).rejects.toThrow('Tiller non è in ascolto')
})
```

- [ ] **Step 6: Eseguire il test e verificare che fallisca**

Run: `pnpm test:unit cli/tillerctl.test.ts`
Expected: FAIL — `Failed to resolve import "./tillerctl"`

- [ ] **Step 7: Implementare la CLI**

Creare `cli/tillerctl.ts`:

```typescript
import net from 'node:net'
import { randomUUID } from 'node:crypto'
import { ControlResponse, type ControlRequest } from '../src/shared/protocol'
import { resolveSocketPath } from '../src/shared/paths'
import { parseCliArgs, HELP_TEXT } from './args'

export function sendRequest(
  socketPath: string,
  request: ControlRequest
): Promise<ControlResponse> {
  return new Promise((resolve, reject) => {
    const client = net.connect(socketPath)
    client.setEncoding('utf8')

    client.on('connect', () => {
      client.write(JSON.stringify(request) + '\n')
    })

    let buffer = ''
    client.on('data', (chunk: string) => {
      buffer += chunk
      const newline = buffer.indexOf('\n')
      if (newline === -1) return
      client.end()
      const parsed = ControlResponse.safeParse(JSON.parse(buffer.slice(0, newline)))
      if (!parsed.success) {
        reject(new Error('risposta non valida dal server'))
        return
      }
      resolve(parsed.data)
    })

    client.on('error', (error: NodeJS.ErrnoException) => {
      if (error.code === 'ENOENT' || error.code === 'ECONNREFUSED') {
        reject(new Error(`Tiller non è in ascolto su ${socketPath}`))
        return
      }
      reject(error)
    })
  })
}

async function main(): Promise<number> {
  let command
  try {
    command = parseCliArgs(process.argv.slice(2))
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n\n`)
    process.stderr.write(HELP_TEXT)
    return 1
  }

  if (command.kind === 'help') {
    process.stdout.write(HELP_TEXT)
    return 0
  }

  const socketPath = resolveSocketPath()

  const created = await sendRequest(socketPath, {
    id: randomUUID(),
    method: 'pane.create',
    params: { cwd: command.cwd, focus: command.focus }
  })
  if (!created.ok) {
    process.stderr.write(`${created.error}\n`)
    return 1
  }

  const { paneId } = created.result as { paneId: string }

  const written = await sendRequest(socketPath, {
    id: randomUUID(),
    method: 'pane.write',
    params: { paneId, data: `${command.cmd}\n` }
  })
  if (!written.ok) {
    process.stderr.write(`${written.error}\n`)
    return 1
  }

  process.stdout.write(`${paneId}\n`)
  return 0
}

// Eseguito solo quando invocato come programma, non quando importato dai test.
if (process.argv[1]?.endsWith('tillerctl.ts') || process.argv[1]?.endsWith('tillerctl.js')) {
  main()
    .then((code) => process.exit(code))
    .catch((error: unknown) => {
      process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`)
      process.exit(1)
    })
}
```

- [ ] **Step 8: Aggiungere lo script npm**

In `package.json`, sezione `scripts`:

```json
"tillerctl": "node cli/tillerctl.ts"
```

Da Node 24 in poi lo stripping dei tipi è attivo di default: nessun flag e nessuno step di build per la CLI in Fase 0. Verificato sul Node di questa macchina (26.5.0).

- [ ] **Step 9: Eseguire i test e verificare che passino**

Run: `pnpm test:unit cli/`
Expected: PASS, 8 test

- [ ] **Step 10: Commit**

```bash
git add cli/ package.json
git commit -m "feat: add tillerctl cli client"
```

---

### Task 8: Finestra e policy di focus

**Files:**
- Create: `src/main/window/window-controller.ts`
- Test: `src/main/window/window-controller.test.ts`

**Interfaces:**
- Consumes: `WindowController` (interfaccia dichiarata in `dispatch.ts`)
- Produces: `createWindowController(deps: { createWindow(): BrowserWindowLike; getExisting(): BrowserWindowLike | null }): WindowController`, dove
  `interface BrowserWindowLike { show(): void; focus(): void; isDestroyed(): boolean }`

- [ ] **Step 1: Scrivere i test che falliscono**

Creare `src/main/window/window-controller.test.ts`:

```typescript
import { expect, test, vi } from 'vitest'
import { createWindowController, type BrowserWindowLike } from './window-controller'

function fakeWindow(): BrowserWindowLike & { show: ReturnType<typeof vi.fn>; focus: ReturnType<typeof vi.fn> } {
  return {
    show: vi.fn(),
    focus: vi.fn(),
    isDestroyed: () => false
  }
}

test('focus crea la finestra se non esiste', () => {
  const created = fakeWindow()
  const createWindow = vi.fn(() => created)
  const controller = createWindowController({ createWindow, getExisting: () => null })

  controller.focus()

  expect(createWindow).toHaveBeenCalledOnce()
  expect(created.focus).toHaveBeenCalledOnce()
})

test('focus riusa la finestra esistente invece di crearne una seconda', () => {
  const existing = fakeWindow()
  const createWindow = vi.fn(() => fakeWindow())
  const controller = createWindowController({ createWindow, getExisting: () => existing })

  controller.focus()

  expect(createWindow).not.toHaveBeenCalled()
  expect(existing.show).toHaveBeenCalledOnce()
  expect(existing.focus).toHaveBeenCalledOnce()
})

test('isOpen è falso senza finestra', () => {
  const controller = createWindowController({
    createWindow: () => fakeWindow(),
    getExisting: () => null
  })
  expect(controller.isOpen()).toBe(false)
})

test('isOpen è vero con una finestra viva', () => {
  const controller = createWindowController({
    createWindow: () => fakeWindow(),
    getExisting: () => fakeWindow()
  })
  expect(controller.isOpen()).toBe(true)
})

test('una finestra distrutta conta come assente', () => {
  const destroyed: BrowserWindowLike = { show: vi.fn(), focus: vi.fn(), isDestroyed: () => true }
  const controller = createWindowController({
    createWindow: () => fakeWindow(),
    getExisting: () => destroyed
  })
  expect(controller.isOpen()).toBe(false)
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/main/window/window-controller.test.ts`
Expected: FAIL — `Failed to resolve import "./window-controller"`

- [ ] **Step 3: Implementare il controller**

Creare `src/main/window/window-controller.ts`:

```typescript
import type { WindowController } from '../control/dispatch'

export interface BrowserWindowLike {
  show(): void
  focus(): void
  isDestroyed(): boolean
}

export interface WindowControllerDeps {
  createWindow(): BrowserWindowLike
  getExisting(): BrowserWindowLike | null
}

/**
 * Policy di focus (vedi la spec): la finestra si apre solo su richiesta
 * esplicita. Gli hook degli agenti chiamano il protocollo decine di volte
 * per sessione e non devono mai rubare il fuoco.
 *
 * Astratto su BrowserWindowLike invece che su BrowserWindow di Electron
 * per poter testare la policy senza avviare Electron.
 */
export function createWindowController(deps: WindowControllerDeps): WindowController {
  const alive = (): BrowserWindowLike | null => {
    const existing = deps.getExisting()
    return existing !== null && !existing.isDestroyed() ? existing : null
  }

  return {
    focus(): void {
      const existing = alive()
      if (existing !== null) {
        existing.show()
        existing.focus()
        return
      }
      deps.createWindow().focus()
    },
    isOpen(): boolean {
      return alive() !== null
    }
  }
}
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/window/window-controller.test.ts`
Expected: PASS, 5 test

- [ ] **Step 5: Commit**

```bash
git add src/main/window/
git commit -m "feat: add window controller with headless-by-default focus policy"
```

---

### Task 9: Trasporto IPC con coalescing dell'output

**Files:**
- Create: `src/main/control/output-coalescer.ts`
- Create: `src/main/control/ipc-transport.ts`
- Test: `src/main/control/output-coalescer.test.ts`

**Interfaces:**
- Consumes: `PtyManager`, `AppState`, dispatcher da `dispatch.ts`
- Produces:
  - `createOutputCoalescer(options: { flushMs: number; flush(batches: PtyOutput[]): void; schedule(fn: () => void, ms: number): unknown; cancel(handle: unknown): void }): { push(paneId: string, data: string): void; dispose(): void }`
  - `registerIpcTransport(deps: { state, pty, dispatch, send(channel: string, payload: unknown): void }): () => void`

**Canali IPC:**
- `control:request` (renderer → main, `invoke`/`handle`) — payload `ControlRequest`, ritorna `ControlResponse`
- `state:event` (main → renderer, `send`) — payload `StateEvent`
- `pty:output` (main → renderer, `send`) — payload `PtyOutput[]`, **in lotti coalescati**

- [ ] **Step 1: Scrivere i test del coalescer**

Creare `src/main/control/output-coalescer.test.ts`:

```typescript
import { expect, test, vi } from 'vitest'
import { createOutputCoalescer } from './output-coalescer'
import type { PtyOutput } from '../../shared/protocol'

function harness(): {
  flushed: PtyOutput[][]
  run(): void
  coalescer: ReturnType<typeof createOutputCoalescer>
} {
  const flushed: PtyOutput[][] = []
  let pending: (() => void) | null = null
  const coalescer = createOutputCoalescer({
    flushMs: 16,
    flush: (batch) => flushed.push(batch),
    schedule: (fn) => {
      pending = fn
      return 1
    },
    cancel: () => {
      pending = null
    }
  })
  return {
    flushed,
    coalescer,
    run: () => {
      const fn = pending
      pending = null
      fn?.()
    }
  }
}

test('non emette nulla prima del flush', () => {
  const { coalescer, flushed } = harness()
  coalescer.push('p1', 'a')
  expect(flushed).toEqual([])
})

test('unisce le scritture consecutive sullo stesso pane', () => {
  const { coalescer, flushed, run } = harness()
  coalescer.push('p1', 'a')
  coalescer.push('p1', 'b')
  coalescer.push('p1', 'c')
  run()
  expect(flushed).toEqual([[{ paneId: 'p1', data: 'abc' }]])
})

test('tiene separati i pane diversi', () => {
  const { coalescer, flushed, run } = harness()
  coalescer.push('p1', 'a')
  coalescer.push('p2', 'x')
  coalescer.push('p1', 'b')
  run()
  expect(flushed).toEqual([[{ paneId: 'p1', data: 'ab' }, { paneId: 'p2', data: 'x' }]])
})

test('dopo un flush riparte vuoto', () => {
  const { coalescer, flushed, run } = harness()
  coalescer.push('p1', 'a')
  run()
  coalescer.push('p1', 'b')
  run()
  expect(flushed).toEqual([[{ paneId: 'p1', data: 'a' }], [{ paneId: 'p1', data: 'b' }]])
})

test('un flush senza dati in coda non emette lotti vuoti', () => {
  const flush = vi.fn()
  const coalescer = createOutputCoalescer({
    flushMs: 16,
    flush,
    schedule: () => 1,
    cancel: () => {}
  })
  coalescer.dispose()
  expect(flush).not.toHaveBeenCalled()
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/main/control/output-coalescer.test.ts`
Expected: FAIL — `Failed to resolve import "./output-coalescer"`

- [ ] **Step 3: Implementare il coalescer**

Creare `src/main/control/output-coalescer.ts`:

```typescript
import type { PtyOutput } from '../../shared/protocol'

export interface CoalescerOptions {
  flushMs: number
  flush(batch: PtyOutput[]): void
  schedule(fn: () => void, ms: number): unknown
  cancel(handle: unknown): void
}

export interface OutputCoalescer {
  push(paneId: string, data: string): void
  dispose(): void
}

/**
 * Accumula l'output dei PTY e lo consegna a lotti.
 *
 * Un webContents.send() per token è insostenibile: ogni messaggio costa una
 * serializzazione JSON più un passaggio fra processi. È lo stesso problema che
 * in SwiftUI saturava la coda di transazioni con gli eventi per-token dei
 * driver nativi; il rimedio è identico, coalescere prima del confine.
 *
 * `schedule`/`cancel` sono iniettati per rendere il tempo deterministico
 * nei test invece di dipendere da timer reali.
 */
export function createOutputCoalescer(options: CoalescerOptions): OutputCoalescer {
  const pending = new Map<string, string>()
  let handle: unknown = null

  const flushNow = (): void => {
    handle = null
    if (pending.size === 0) return
    const batch: PtyOutput[] = [...pending.entries()].map(([paneId, data]) => ({ paneId, data }))
    pending.clear()
    options.flush(batch)
  }

  return {
    push(paneId: string, data: string): void {
      pending.set(paneId, (pending.get(paneId) ?? '') + data)
      if (handle === null) handle = options.schedule(flushNow, options.flushMs)
    },
    dispose(): void {
      if (handle !== null) options.cancel(handle)
      handle = null
      pending.clear()
    }
  }
}
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/main/control/output-coalescer.test.ts`
Expected: PASS, 5 test

- [ ] **Step 5: Implementare il trasporto IPC**

Creare `src/main/control/ipc-transport.ts`:

```typescript
import { ipcMain } from 'electron'
import { ControlRequest, type ControlResponse, type StateEvent } from '../../shared/protocol'
import type { AppState } from '../state/app-state'
import type { PtyManager } from '../pty/pty-manager'
import { createOutputCoalescer } from './output-coalescer'

const FLUSH_MS = 16

export interface IpcTransportDeps {
  state: AppState
  pty: PtyManager
  dispatch(request: ControlRequest): ControlResponse
  send(channel: string, payload: unknown): void
}

/**
 * Espone lo stesso protocollo del socket unix sull'IPC di Electron.
 * Il dispatcher è condiviso: questo file è solo trasporto.
 */
export function registerIpcTransport(deps: IpcTransportDeps): () => void {
  ipcMain.handle('control:request', (_event, payload: unknown): ControlResponse => {
    const parsed = ControlRequest.safeParse(payload)
    if (!parsed.success) {
      return { id: 'unknown', ok: false, error: 'richiesta non valida' }
    }
    return deps.dispatch(parsed.data)
  })

  ipcMain.handle('state:snapshot', (): StateEvent => ({
    type: 'snapshot',
    panes: deps.state.snapshot()
  }))

  const unsubscribeState = deps.state.subscribe((event) => {
    deps.send('state:event', event)
  })

  const coalescer = createOutputCoalescer({
    flushMs: FLUSH_MS,
    flush: (batch) => deps.send('pty:output', batch),
    schedule: (fn, ms) => setTimeout(fn, ms),
    cancel: (handle) => clearTimeout(handle as NodeJS.Timeout)
  })

  const unsubscribeData = deps.pty.onData((paneId, data) => coalescer.push(paneId, data))

  return (): void => {
    unsubscribeState()
    unsubscribeData()
    coalescer.dispose()
    ipcMain.removeHandler('control:request')
    ipcMain.removeHandler('state:snapshot')
  }
}
```

- [ ] **Step 6: Verificare il typecheck**

Run: `pnpm typecheck`
Expected: nessun errore

- [ ] **Step 7: Commit**

```bash
git add src/main/control/output-coalescer.ts src/main/control/output-coalescer.test.ts src/main/control/ipc-transport.ts
git commit -m "feat: expose the control protocol over electron ipc with output coalescing"
```

---

### Task 10: Bridge del preload e composizione del main

**Files:**
- Modify: `src/preload/index.ts`
- Modify: `src/preload/index.d.ts`
- Modify: `src/main/index.ts`

**Interfaces:**
- Consumes: tutto dai Task 2–9
- Produces:
  - `window.tiller` nel renderer, con
    - `request(req: ControlRequest): Promise<ControlResponse>`
    - `snapshot(): Promise<StateEvent>`
    - `onStateEvent(cb: (event: StateEvent) => void): () => void`
    - `onPtyOutput(cb: (batch: PtyOutput[]) => void): () => void`

- [ ] **Step 1: Scrivere il bridge del preload**

Sostituire `src/preload/index.ts`:

```typescript
import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import type {
  ControlRequest,
  ControlResponse,
  PtyOutput,
  StateEvent
} from '../shared/protocol'

/**
 * Unica superficie esposta al renderer.
 *
 * Il renderer non può aprire socket né spawnare processi: tutto ciò che può
 * fare passa da qui, e ogni payload è validato di nuovo nel main.
 */
const tiller = {
  request: (request: ControlRequest): Promise<ControlResponse> =>
    ipcRenderer.invoke('control:request', request),

  snapshot: (): Promise<StateEvent> => ipcRenderer.invoke('state:snapshot'),

  onStateEvent: (callback: (event: StateEvent) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, payload: StateEvent): void => callback(payload)
    ipcRenderer.on('state:event', listener)
    return () => {
      ipcRenderer.removeListener('state:event', listener)
    }
  },

  onPtyOutput: (callback: (batch: PtyOutput[]) => void): (() => void) => {
    const listener = (_event: IpcRendererEvent, payload: PtyOutput[]): void => callback(payload)
    ipcRenderer.on('pty:output', listener)
    return () => {
      ipcRenderer.removeListener('pty:output', listener)
    }
  }
}

export type TillerApi = typeof tiller

contextBridge.exposeInMainWorld('tiller', tiller)
```

- [ ] **Step 2: Dichiarare i tipi globali**

Sostituire `src/preload/index.d.ts`:

```typescript
import type { TillerApi } from './index'

declare global {
  interface Window {
    tiller: TillerApi
  }
}

export {}
```

- [ ] **Step 3: Comporre il main**

Sostituire `src/main/index.ts`:

```typescript
import { app, BrowserWindow } from 'electron'
import { join } from 'node:path'
import { randomUUID } from 'node:crypto'
import { electronApp, optimizer, is } from '@electron-toolkit/utils'
import { AppState } from './state/app-state'
import { PtyManager } from './pty/pty-manager'
import { createNodePtySpawner, DEFAULT_SHELL } from './pty/pty-spawner'
import { createDispatcher } from './control/dispatch'
import { startSocketServer, type SocketServer } from './control/socket-server'
import { registerIpcTransport } from './control/ipc-transport'
import { createWindowController, type BrowserWindowLike } from './window/window-controller'
import { resolveSocketPath } from '../shared/paths'

const state = new AppState()
const pty = new PtyManager(createNodePtySpawner())

let mainWindow: BrowserWindow | null = null
let socketServer: SocketServer | null = null
let disposeIpc: (() => void) | null = null

function createWindow(): BrowserWindowLike {
  const window = new BrowserWindow({
    width: 1100,
    height: 720,
    show: false,
    autoHideMenuBar: true,
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      sandbox: false
    }
  })

  window.on('ready-to-show', () => window.show())
  window.on('closed', () => {
    mainWindow = null
  })

  if (is.dev && process.env['ELECTRON_RENDERER_URL']) {
    window.loadURL(process.env['ELECTRON_RENDERER_URL'])
  } else {
    window.loadFile(join(__dirname, '../renderer/index.html'))
  }

  mainWindow = window
  return window
}

const windowController = createWindowController({
  createWindow,
  getExisting: () => mainWindow
})

const dispatch = createDispatcher({
  state,
  pty,
  window: windowController,
  now: () => Date.now(),
  newPaneId: () => randomUUID(),
  defaultShell: DEFAULT_SHELL
})

app.whenReady().then(async () => {
  electronApp.setAppUserModelId('dev.tiller.electron')
  app.on('browser-window-created', (_, window) => optimizer.watchWindowShortcuts(window))

  disposeIpc = registerIpcTransport({
    state,
    pty,
    dispatch,
    send: (channel, payload) => {
      // Se non c'è finestra, l'evento si perde ed è corretto così:
      // lo stato autoritativo resta nel main e la prossima finestra
      // lo ricostruisce con state:snapshot.
      mainWindow?.webContents.send(channel, payload)
    }
  })

  socketServer = await startSocketServer({
    socketPath: resolveSocketPath(),
    handle: dispatch
  })

  createWindow()

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) createWindow()
  })
})

// Su macOS chiudere la finestra NON termina l'app: i PTY restano vivi e
// tillerctl continua a funzionare. È il comportamento di Tiller oggi.
app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') app.quit()
})

// Non è `async`: Electron non attende gli handler asincroni di before-quit.
// Il socket potrebbe quindi restare sul filesystem dopo l'uscita — è previsto,
// startSocketServer rimuove il socket stantio al prossimo avvio.
app.on('before-quit', () => {
  disposeIpc?.()
  pty.disposeAll()
  void socketServer?.close()
})
```

- [ ] **Step 4: Verificare che tutto compili**

Run: `pnpm typecheck`
Expected: nessun errore

- [ ] **Step 5: Verificare che l'app si avvii**

Run: `pnpm dev`
Expected: si apre una finestra (ancora senza terminale). Chiudere con Ctrl+C.

- [ ] **Step 6: Commit**

```bash
git add src/preload/ src/main/index.ts
git commit -m "feat: wire preload bridge and compose main process"
```

---

### Task 11: Proiezione nel renderer e terminale xterm

**Files:**
- Create: `src/renderer/src/lib/app-model.svelte.ts`
- Create: `src/renderer/src/components/Terminal.svelte`
- Modify: `src/renderer/src/App.svelte`
- Test: `src/renderer/src/lib/app-model.test.ts`

**Interfaces:**
- Consumes: `window.tiller` dal preload, `PaneSnapshot`/`StateEvent`/`PtyOutput` da `shared/protocol.ts`
- Produces: classe `AppModel` con
  - `readonly panes: PaneSnapshot[]` (rune `$state`)
  - `applyEvent(event: StateEvent): void`
  - `static attach(api: TillerApi): AppModel`

**Nota Svelte 5:** usare le rune. `$state` va dichiarato come campo di classe in un file `.svelte.ts`; non usare `writable()` né `export let`.

- [ ] **Step 1: Scrivere i test della proiezione**

Creare `src/renderer/src/lib/app-model.test.ts`:

```typescript
import { expect, test } from 'vitest'
import { AppModel } from './app-model.svelte'

const pane = { id: 'p1', cwd: '/tmp', createdAt: 1 }

test('un modello nuovo non ha pane', () => {
  expect(new AppModel().panes).toEqual([])
})

test('uno snapshot sostituisce interamente i pane', () => {
  const model = new AppModel()
  model.applyEvent({ type: 'pane.added', pane })
  model.applyEvent({ type: 'snapshot', panes: [] })
  expect(model.panes).toEqual([])
})

test('pane.added aggiunge il pane', () => {
  const model = new AppModel()
  model.applyEvent({ type: 'pane.added', pane })
  expect(model.panes).toEqual([pane])
})

test('pane.removed toglie il pane', () => {
  const model = new AppModel()
  model.applyEvent({ type: 'pane.added', pane })
  model.applyEvent({ type: 'pane.removed', paneId: 'p1' })
  expect(model.panes).toEqual([])
})

test('pane.added due volte non duplica lo stesso id', () => {
  const model = new AppModel()
  model.applyEvent({ type: 'pane.added', pane })
  model.applyEvent({ type: 'pane.added', pane })
  expect(model.panes).toHaveLength(1)
})
```

Perché i test girano: Vitest 4 compila i file `.svelte.ts` se è presente il plugin Svelte. Aggiungere a `vitest.config.ts`:

```typescript
import { defineConfig } from 'vitest/config'
import { svelte } from '@sveltejs/vite-plugin-svelte'

export default defineConfig({
  plugins: [svelte({ hot: false })],
  test: {
    include: ['src/**/*.test.ts', 'cli/**/*.test.ts'],
    exclude: ['**/node_modules/**', 'e2e/**'],
    environment: 'node'
  }
})
```

- [ ] **Step 2: Eseguire i test e verificare che falliscano**

Run: `pnpm test:unit src/renderer/src/lib/app-model.test.ts`
Expected: FAIL — `Failed to resolve import "./app-model.svelte"`

- [ ] **Step 3: Implementare il modello**

Creare `src/renderer/src/lib/app-model.svelte.ts`:

```typescript
import type { PaneSnapshot, StateEvent } from '../../../shared/protocol'
import type { TillerApi } from '../../../preload'

/**
 * Proiezione osservabile dello stato del main.
 *
 * Non è la sede dello stato: il main resta autoritativo. Questa classe
 * applica gli eventi che riceve e non decide mai nulla da sola, così una
 * finestra aperta in ritardo ricostruisce la verità con uno snapshot.
 */
export class AppModel {
  panes = $state<PaneSnapshot[]>([])

  applyEvent(event: StateEvent): void {
    switch (event.type) {
      case 'snapshot':
        this.panes = event.panes
        return
      case 'pane.added':
        if (this.panes.some((p) => p.id === event.pane.id)) return
        this.panes = [...this.panes, event.pane]
        return
      case 'pane.removed':
        this.panes = this.panes.filter((p) => p.id !== event.paneId)
        return
    }
  }

  static attach(api: TillerApi): AppModel {
    const model = new AppModel()
    void api.snapshot().then((snapshot) => model.applyEvent(snapshot))
    api.onStateEvent((event) => model.applyEvent(event))
    return model
  }
}
```

- [ ] **Step 4: Eseguire i test e verificare che passino**

Run: `pnpm test:unit src/renderer/src/lib/app-model.test.ts`
Expected: PASS, 5 test

- [ ] **Step 5: Implementare il componente terminale**

Creare `src/renderer/src/components/Terminal.svelte`:

```svelte
<script lang="ts">
  import { Terminal } from '@xterm/xterm'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { FitAddon } from '@xterm/addon-fit'
  import '@xterm/xterm/css/xterm.css'

  interface Props {
    paneId: string
  }

  const { paneId }: Props = $props()

  let host: HTMLDivElement

  $effect(() => {
    const term = new Terminal({
      fontFamily: 'SF Mono, Menlo, monospace',
      fontSize: 13,
      cursorBlink: true
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.open(host)

    // WebGL può fallire su GPU non supportate: il canvas resta il fallback.
    try {
      term.loadAddon(new WebglAddon())
    } catch {
      // rendering su canvas, nessuna azione necessaria
    }

    fit.fit()

    const unsubscribe = window.tiller.onPtyOutput((batch) => {
      for (const chunk of batch) {
        if (chunk.paneId === paneId) term.write(chunk.data)
      }
    })

    term.onData((data) => {
      void window.tiller.request({
        id: crypto.randomUUID(),
        method: 'pane.write',
        params: { paneId, data }
      })
    })

    const onResize = (): void => fit.fit()
    window.addEventListener('resize', onResize)

    return () => {
      window.removeEventListener('resize', onResize)
      unsubscribe()
      term.dispose()
    }
  })
</script>

<div class="terminal" bind:this={host}></div>

<style>
  .terminal {
    width: 100%;
    height: 100%;
  }
</style>
```

- [ ] **Step 6: Collegare App.svelte**

Sostituire `src/renderer/src/App.svelte`:

```svelte
<script lang="ts">
  import { AppModel } from './lib/app-model.svelte'
  import Terminal from './components/Terminal.svelte'

  const model = AppModel.attach(window.tiller)
</script>

<main>
  {#if model.panes.length === 0}
    <p class="empty">Nessun pane. Provane uno con <code>tillerctl run --cmd "echo ciao"</code>.</p>
  {:else}
    {#each model.panes as pane (pane.id)}
      <Terminal paneId={pane.id} />
    {/each}
  {/if}
</main>

<style>
  main {
    width: 100vw;
    height: 100vh;
    background: #1f1f26;
  }

  .empty {
    color: #8a8a94;
    font-family: system-ui, sans-serif;
    padding: 2rem;
  }
</style>
```

- [ ] **Step 7: Verificare il typecheck e provare a mano**

Run: `pnpm typecheck`
Expected: nessun errore

Run: `pnpm dev`, poi in un altro terminale: `pnpm tillerctl run --cmd "echo ciao"`
Expected: nella finestra compare un terminale che mostra `ciao`

- [ ] **Step 8: Commit**

```bash
git add src/renderer/ vitest.config.ts
git commit -m "feat: project main state into svelte renderer with xterm panes"
```

---

### Task 12: Gate end-to-end

**Files:**
- Create: `playwright.config.ts`
- Create: `e2e/walking-skeleton.spec.ts`
- Modify: `scripts/ci.sh` (riabilitare la riga e2e)

**Interfaces:**
- Consumes: l'app costruita, `tillerctl`
- Produces: i due criteri di successo della Fase 0, eseguibili

- [ ] **Step 1: Configurare Playwright**

Creare `playwright.config.ts`:

```typescript
import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  fullyParallel: false,
  workers: 1,
  reporter: 'list'
})
```

- [ ] **Step 2: Scrivere il test e2e**

Creare `e2e/walking-skeleton.spec.ts`:

```typescript
import { test, expect, _electron as electron, type ElectronApplication } from '@playwright/test'
import { execFile } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const run = promisify(execFile)

let app: ElectronApplication
let socketDir: string
let socketPath: string

test.beforeEach(async () => {
  socketDir = await mkdtemp(join(tmpdir(), 'tiller-e2e-'))
  socketPath = join(socketDir, 'control.sock')
  app = await electron.launch({
    args: ['.'],
    env: { ...process.env, TILLER_SOCKET: socketPath }
  })
  // Attendere la prima finestra garantisce anche che il socket sia in ascolto:
  // in main/index.ts startSocketServer viene atteso PRIMA di createWindow().
  // Se quell'ordine cambia, questi test diventano intermittenti.
  await app.firstWindow()
})

test.afterEach(async () => {
  await app.close()
  await rm(socketDir, { recursive: true, force: true })
})

async function tillerctl(args: string[]): Promise<string> {
  const { stdout } = await run('node', ['cli/tillerctl.ts', ...args], {
    env: { ...process.env, TILLER_SOCKET: socketPath }
  })
  return stdout.trim()
}

test('criterio 1: run --focus mostra il comando in un pane', async () => {
  await tillerctl(['run', '--cmd', 'echo ciao-dal-pane', '--focus'])

  const window = await app.firstWindow()
  await expect(window.locator('.xterm')).toBeVisible({ timeout: 10_000 })
  await expect(window.locator('.xterm-screen')).toContainText('ciao-dal-pane', {
    timeout: 10_000
  })
})

test('criterio 2: senza finestra il pane viene creato comunque e appare dopo', async () => {
  // Chiude la finestra: su macOS l'app resta viva.
  const window = await app.firstWindow()
  await window.close()

  const paneId = await tillerctl(['run', '--cmd', 'echo pane-senza-finestra'])
  expect(paneId).not.toBe('')

  // Si mette in ascolto PRIMA di chiedere il focus, altrimenti la finestra
  // può nascere prima che il listener sia attaccato.
  const reopened = app.waitForEvent('window')
  await tillerctl(['run', '--cmd', 'echo secondo-pane', '--focus'])
  const secondWindow = await reopened

  // Il pane creato a finestra chiusa deve esserci: sono due pane, non uno.
  // Se lo stato vivesse nel renderer, qui ce ne sarebbe uno solo.
  await expect(secondWindow.locator('.xterm')).toHaveCount(2, { timeout: 10_000 })
})
```

Il secondo test è quello che conta: se lo stato dei pane vivesse nel renderer, il pane creato a finestra chiusa non esisterebbe e il conteggio sarebbe 1.

- [ ] **Step 3: Costruire l'app e lanciare gli e2e**

```bash
pnpm build
pnpm test:e2e
```

Expected: 2 test PASS

- [ ] **Step 4: Riabilitare gli e2e nel gate**

In `scripts/ci.sh`, rimuovere il `#` dalla riga `pnpm test:e2e` e aggiungere il build prima:

```bash
echo "==> build"
pnpm build

echo "==> e2e"
pnpm test:e2e
```

- [ ] **Step 5: Eseguire il gate completo**

Run: `bash scripts/ci.sh`
Expected: termina stampando `CI OK`

- [ ] **Step 6: Commit**

```bash
git add playwright.config.ts e2e/ scripts/ci.sh
git commit -m "test: add end-to-end gate for the walking skeleton criteria"
```

---

## Esito dell'esecuzione (2026-08-01)

Fase 0 completata: 12/12 task, 17 commit, 90 test unitari + 2 e2e, gate verde.
Eseguita delegando a codex `gpt-5.6-luna` un task alla volta, con revisione del
diff in mezzo.

**Entrambi i criteri di successo passano.** Il criterio 2 — pane creato a
finestra chiusa e ritrovato riaprendo — è quello che poteva falsificare la
collocazione dello stato: non l'ha fatto.

Correzioni al piano emerse eseguendolo:

- **Task 4** — `node-pty` è N-API: lo stesso binario carica in Node 24 e in
  Electron 43, nessun `electron-rebuild`. Lo spawner iniettato resta giusto, ma
  per il determinismo dei test, non per l'ABI.
- **Task 4** — `spawn-helper` di node-pty arriva senza bit di esecuzione sotto
  pnpm: ogni spawn muore con `posix_spawnp failed`. Fix nel `postinstall` più
  un test di guardia.
- **Task 7** — `tsconfig.node.json` non copriva `cli/` né alcun file di test:
  aggiunto `tsconfig.cli.json` al gate. Ha subito trovato un errore di tipo
  reale in `protocol.test.ts`.
- **Task 11** — Svelte 5.29+ ha gli **attachment** (`{@attach}`), che
  sostituiscono `bind:this` + `$effect`. Il codice Svelte va validato con il
  server MCP **prima** di delegarlo.
- **Task 12** — xterm renderizza su **canvas**: il testo del terminale non
  esiste nel DOM e `toContainText` su `.xterm-screen` non può funzionare. La
  e2e legge il buffer di xterm tramite il seam `window.__tillerPaneBuffers`,
  che espone un lettore e non l'oggetto Terminal. Verificato per mutazione.

## Definizione di fatto per la Fase 0

- [ ] `bash scripts/ci.sh` stampa `CI OK`
- [ ] `tillerctl run --cmd "echo ciao" --focus` apre la finestra e mostra `ciao`
- [ ] `tillerctl run --cmd "echo ciao"` con la finestra chiusa crea il pane; riaprendo, il pane è presente
- [ ] Nessun `any` implicito, nessun `@ts-ignore` nuovo
- [ ] Il repo Swift `~/Desktop/Progetti/tiller/` non è stato toccato
