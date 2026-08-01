# Fase 2 — Motore terminale: piano di implementazione

> **Per esecutori automatici:** SOTTO-SKILL RICHIESTA: usa
> `superpowers:subagent-driven-development` (consigliata) oppure
> `superpowers:executing-plans` per eseguire questo piano un task alla volta.
> I passi usano caselle (`- [ ]`) per il tracciamento.

**Obiettivo:** portare il motore terminale da libghostty a xterm.js, con lo
stato autoritativo nel processo main, e risolvere il problema di memoria dei
pane nascosti.

**Architettura:** un `@xterm/headless` per pane vive nel main ed è la verità
(schermo, scrollback, titolo OSC); l'xterm del renderer è una vista alimentata
dallo stesso flusso di byte del PTY. Il main possiede il contenuto, il renderer
possiede la dimensione. La persistenza usa `serialize` invece di byte grezzi.

**Stack:** `@xterm/headless` 6, `@xterm/addon-serialize`,
`@xterm/addon-search`, `@xterm/addon-unicode11`, `@xterm/addon-web-links`,
node-pty, kysely, Vitest 4, Playwright.

**Spec:** `docs/superpowers/specs/2026-08-01-electron-fase-2-motore-terminale-design.md`

## Vincoli globali

- Repo di lavoro: `~/Desktop/Progetti/tiller-electron`. **Il repo Swift
  `~/Desktop/Progetti/tiller` non si tocca mai**, nemmeno per una formattazione.
- Gate: `bash scripts/ci.sh` deve stampare `CI OK`. Eseguirlo per intero alla
  fine di ogni task, non solo i test dell'area toccata.
- Commenti e messaggi di errore in **italiano**, come il resto del codice.
  Le stringhe rivolte all'utente finale nella UI restano in **inglese**.
- Commit in formato Conventional Commits, soggetto minuscolo all'imperativo.
- `src/main/terminal/*` e `src/main/control/dispatch.ts` **non importano
  `electron` né `net`**.
- Scrollback vivo: 5.000 righe. Snapshot persistito: 1.000 righe.
- Throttle di persistenza: 30 secondi.
- Test con Vitest: `import { expect, test } from 'vitest'`, stile
  Arrange-Act-Assert.
- **Se un test di questo piano fallisce per un motivo che ritieni sbagliato,
  NON indebolire l'asserzione: lascialo rosso e segnalalo nel report.** Il
  piano può contenere errori; ammorbidire un test li nasconde.

## Fatti verificati sul campo (non ri-verificarli, non contraddirli)

- `term.write(data)` è **asincrono**: subito dopo, il buffer è ancora vuoto.
  Serve `write('', callback)` per sapere quando il parsing è finito.
- `buffer.active.length` vale **il numero di righe del viewport più lo
  scrollback occupato**, non il numero di righe con contenuto: un terminale
  24×80 appena creato riporta già 24. Le righe in coda sono vuote e vanno
  scartate prima di prendere "le ultime N".
- `term.onTitleChange` cattura sia OSC 0 sia OSC 2 nel build headless.
- Il round-trip `serialize` → nuovo terminale è identico byte a byte, colori
  compresi.
- `paneScrollback.worktreeId` ha `references('worktree.id')` e i pragma
  attivano `foreign_keys = ON`: **scrivere scrollback per un pane senza un
  worktree esistente viola un vincolo**. `terminalContentId` invece è solo
  chiave primaria, senza riferimenti: il paneId può usarla direttamente.
- `node-pty` espone `resize(columns, rows)`.
- `app.on('before-quit')` **non attende gli handler asincroni**.

---

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/main/terminal/terminal-state.ts` (nuovo) | un headless per pane: write, flush, resize, readText, readSnapshot, onTitle, dispose |
| `src/main/terminal/terminal-registry.ts` (nuovo) | paneId → TerminalState; creazione, distruzione, elenco |
| `src/main/terminal/debounce.ts` (nuovo) | `createDebouncer` (resize) e `createThrottle` (persistenza) |
| `src/main/terminal/scrollback-store.ts` (nuovo) | lettura e scrittura di `paneScrollback`, coalescing per sostituzione |
| `src/main/pty/pty-spawner.ts` (modifica) | `PtyHandle.resize` |
| `src/main/pty/pty-manager.ts` (modifica) | `resize(paneId, cols, rows)` |
| `src/shared/protocol.ts` (modifica) | `pane.resize`, `pane.read`, `pane.close`, `PaneSnapshot.title`, evento `pane.title` |
| `src/main/state/app-state.ts` (modifica) | `setPaneTitle` |
| `src/main/control/dispatch.ts` (modifica) | cabla il registry, i tre metodi nuovi, i titoli |
| `src/main/index.ts` (modifica) | cablaggio persistenza, throttle, flush al quit |
| `cli/tillerctl.ts`, `cli/args.ts` (modifica) | comandi `read`, `resize`, `close` |
| `src/renderer/src/lib/terminal-theme.ts` (nuovo) | tema e opzioni font |
| `src/renderer/src/components/Terminal.svelte` (modifica) | tema, resize→IPC, snapshot al mount, link, visibilità |
| `src/renderer/src/components/TerminalSearch.svelte` (nuovo) | barra di ricerca |
| `e2e/fase-2-terminale.spec.ts` (nuovo) | i quattro criteri |

---

### Task 1: TerminalState — l'emulatore headless di un pane

**File:**
- Modifica: `package.json` (dipendenze)
- Crea: `src/main/terminal/terminal-state.ts`
- Test: `src/main/terminal/terminal-state.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `createTerminalState(options?: { cols?: number; rows?: number }): TerminalState`, dove
  `TerminalState = { readonly cols: number; readonly rows: number; write(data: string): void; flush(): Promise<void>; resize(cols: number, rows: number): void; readText(lines?: number): string; readSnapshot(): string; onTitle(listener: (title: string) => void): () => void; dispose(): void }`.
  Costanti esportate: `LIVE_SCROLLBACK_LINES = 5000`, `PERSISTED_SCROLLBACK_LINES = 1000`, `TERMINAL_DEFAULT_COLS = 80`, `TERMINAL_DEFAULT_ROWS = 24`.

- [ ] **Passo 1: installa le dipendenze**

```bash
cd ~/Desktop/Progetti/tiller-electron
pnpm add @xterm/headless @xterm/addon-serialize @xterm/addon-search @xterm/addon-unicode11 @xterm/addon-web-links
```

- [ ] **Passo 2: scrivi il test che fallisce**

Crea `src/main/terminal/terminal-state.test.ts`:

```ts
import { expect, test } from 'vitest'
import { createTerminalState, LIVE_SCROLLBACK_LINES } from './terminal-state'

test('la scrittura è asincrona: readText vede il testo solo dopo flush', async () => {
  // Arrange
  const state = createTerminalState()

  // Act
  state.write('ciao')

  // Assert — xterm accoda il parsing, il buffer non è ancora aggiornato.
  // Se questa riga cade, l'API di xterm è cambiata: NON rimuovere il flush,
  // segnalalo.
  expect(state.readText()).toBe('')
  await state.flush()
  expect(state.readText()).toBe('ciao')
})

test('readText scarta le righe vuote in coda prima di prendere le ultime N', async () => {
  // Arrange — un terminale 80x24 riporta 24 righe di buffer anche se il
  // contenuto ne occupa tre: senza lo scarto, "ultime 3" sono tre righe vuote.
  const state = createTerminalState()

  // Act
  state.write('alfa\r\nbeta\r\ngamma\r\n')
  await state.flush()

  // Assert
  expect(state.readText(2)).toBe('beta\ngamma')
  expect(state.readText()).toBe('alfa\nbeta\ngamma')
})

test('cattura i titoli OSC 0 e OSC 2', async () => {
  // Arrange
  const state = createTerminalState()
  const titles: string[] = []
  state.onTitle((title) => titles.push(title))

  // Act — i due formati che gli agenti usano davvero.
  state.write('\x1b]0;✳ Claude\x07')
  state.write('\x1b]2;π - /Users/x/progetti\x07')
  await state.flush()

  // Assert
  expect(titles).toEqual(['✳ Claude', 'π - /Users/x/progetti'])
})

test('disiscriversi ferma le notifiche di titolo', async () => {
  // Arrange
  const state = createTerminalState()
  const titles: string[] = []
  const unsubscribe = state.onTitle((title) => titles.push(title))

  // Act
  state.write('\x1b]0;primo\x07')
  await state.flush()
  unsubscribe()
  state.write('\x1b]0;secondo\x07')
  await state.flush()

  // Assert
  expect(titles).toEqual(['primo'])
})

test('lo snapshot ricostruisce un terminale identico', async () => {
  // Arrange
  const origin = createTerminalState()
  for (let i = 0; i < 50; i++) origin.write(`\x1b[3${i % 8}mriga ${i}\x1b[0m\r\n`)
  await origin.flush()

  // Act
  const copy = createTerminalState()
  copy.write(origin.readSnapshot())
  await copy.flush()

  // Assert
  expect(copy.readText()).toBe(origin.readText())
})

test('resize cambia le dimensioni riportate', () => {
  // Arrange
  const state = createTerminalState()
  expect(state.cols).toBe(80)
  expect(state.rows).toBe(24)

  // Act
  state.resize(120, 40)

  // Assert
  expect(state.cols).toBe(120)
  expect(state.rows).toBe(40)
})

test('lo scrollback vivo tiene molto più di uno schermo', async () => {
  // Arrange
  const state = createTerminalState()

  // Act
  for (let i = 0; i < 3000; i++) state.write(`riga ${i}\r\n`)
  await state.flush()

  // Assert — la riga 100 è scorsa fuori dallo schermo ma non dalla memoria.
  expect(LIVE_SCROLLBACK_LINES).toBe(5000)
  expect(state.readText()).toContain('riga 100')
})
```

- [ ] **Passo 3: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/terminal/terminal-state.test.ts
```

Atteso: FALLISCE con `Failed to resolve import "./terminal-state"`.

- [ ] **Passo 4: scrivi l'implementazione**

Crea `src/main/terminal/terminal-state.ts`:

```ts
import { Terminal } from '@xterm/headless'
import { SerializeAddon } from '@xterm/addon-serialize'

/** Righe tenute in memoria dal terminale del main. */
export const LIVE_SCROLLBACK_LINES = 5000
/** Righe salvate su disco: 1.000 righe pesano circa 85 KB serializzate. */
export const PERSISTED_SCROLLBACK_LINES = 1000
export const TERMINAL_DEFAULT_COLS = 80
export const TERMINAL_DEFAULT_ROWS = 24

export interface TerminalState {
  readonly cols: number
  readonly rows: number
  write(data: string): void
  /** Risolve quando xterm ha finito di analizzare tutto ciò che è stato scritto. */
  flush(): Promise<void>
  resize(cols: number, rows: number): void
  /** Testo già interpretato, senza sequenze di escape. */
  readText(lines?: number): string
  /** Stato del terminale come stringa ANSI ricaricabile. */
  readSnapshot(): string
  onTitle(listener: (title: string) => void): () => void
  dispose(): void
}

/**
 * Stato autoritativo di un pane: un emulatore completo nel processo main.
 *
 * Non è un doppione dell'xterm del renderer, è la sua sorgente. Il renderer
 * disegna e può sparire in qualsiasi momento; questo resta, ed è ciò che
 * permette di leggere un pane a finestra chiusa e di persisterlo.
 */
export function createTerminalState(
  options: { cols?: number; rows?: number } = {}
): TerminalState {
  const term = new Terminal({
    cols: options.cols ?? TERMINAL_DEFAULT_COLS,
    rows: options.rows ?? TERMINAL_DEFAULT_ROWS,
    scrollback: LIVE_SCROLLBACK_LINES,
    allowProposedApi: true
  })
  const serializer = new SerializeAddon()
  term.loadAddon(serializer)

  return {
    get cols(): number {
      return term.cols
    },
    get rows(): number {
      return term.rows
    },
    write(data: string): void {
      term.write(data)
    },
    flush(): Promise<void> {
      // Una scrittura vuota entra nella stessa coda: il suo callback scatta
      // dopo che tutto il resto è stato analizzato.
      return new Promise<void>((resolve) => term.write('', () => resolve()))
    },
    resize(cols: number, rows: number): void {
      term.resize(cols, rows)
    },
    readText(lines?: number): string {
      const buffer = term.buffer.active
      const all: string[] = []
      for (let i = 0; i < buffer.length; i++) {
        all.push(buffer.getLine(i)?.translateToString(true) ?? '')
      }
      // buffer.length conta le righe del viewport, non quelle con contenuto:
      // senza questo scarto, "le ultime tre righe" sono tre righe vuote.
      while (all.length > 0 && all[all.length - 1] === '') all.pop()
      return (lines === undefined ? all : all.slice(-lines)).join('\n')
    },
    readSnapshot(): string {
      return serializer.serialize({ scrollback: PERSISTED_SCROLLBACK_LINES })
    },
    onTitle(listener: (title: string) => void): () => void {
      const subscription = term.onTitleChange(listener)
      return () => subscription.dispose()
    },
    dispose(): void {
      term.dispose()
    }
  }
}
```

- [ ] **Passo 5: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/terminal/terminal-state.test.ts
```

Atteso: 7 test verdi.

- [ ] **Passo 6: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add package.json pnpm-lock.yaml src/main/terminal/
git commit -m "feat: stato autoritativo del terminale nel processo main"
```

---

### Task 2: TerminalRegistry — ciclo di vita dei terminali

**File:**
- Crea: `src/main/terminal/terminal-registry.ts`
- Test: `src/main/terminal/terminal-registry.test.ts`

**Interfacce:**
- Consuma: `createTerminalState`, `TerminalState` da `./terminal-state`.
- Produce: `class TerminalRegistry` con
  `create(paneId: string, options?: { cols?: number; rows?: number }): TerminalState`,
  `get(paneId: string): TerminalState | undefined`,
  `remove(paneId: string): void`,
  `ids(): string[]`,
  `disposeAll(): void`,
  `onTitle(listener: (paneId: string, title: string) => void): () => void`.

- [ ] **Passo 1: scrivi il test che fallisce**

Crea `src/main/terminal/terminal-registry.test.ts`:

```ts
import { expect, test } from 'vitest'
import { TerminalRegistry } from './terminal-registry'

test('create restituisce uno stato recuperabile con get', () => {
  // Arrange
  const registry = new TerminalRegistry()

  // Act
  const created = registry.create('pane-1')

  // Assert
  expect(registry.get('pane-1')).toBe(created)
  expect(registry.ids()).toEqual(['pane-1'])
})

test('creare due volte lo stesso pane è un errore', () => {
  // Arrange
  const registry = new TerminalRegistry()
  registry.create('pane-1')

  // Act & Assert
  expect(() => registry.create('pane-1')).toThrow('terminale già attivo: pane-1')
})

test('un pane sconosciuto restituisce undefined invece di lanciare', () => {
  // Arrange
  const registry = new TerminalRegistry()

  // Act & Assert — il chiamante è il dispatcher, che deve poter rispondere
  // con un errore di protocollo invece di far cadere la connessione.
  expect(registry.get('inesistente')).toBeUndefined()
})

test('remove toglie il pane e lo rende non recuperabile', () => {
  // Arrange
  const registry = new TerminalRegistry()
  registry.create('pane-1')

  // Act
  registry.remove('pane-1')

  // Assert
  expect(registry.get('pane-1')).toBeUndefined()
  expect(registry.ids()).toEqual([])
})

test('remove su un pane inesistente non lancia', () => {
  // Arrange
  const registry = new TerminalRegistry()

  // Act & Assert
  expect(() => registry.remove('mai-esistito')).not.toThrow()
})

test('i titoli arrivano al listener con il paneId di provenienza', async () => {
  // Arrange
  const registry = new TerminalRegistry()
  const seen: Array<[string, string]> = []
  registry.onTitle((paneId, title) => seen.push([paneId, title]))
  const primo = registry.create('pane-1')
  const secondo = registry.create('pane-2')

  // Act
  primo.write('\x1b]0;titolo uno\x07')
  secondo.write('\x1b]0;titolo due\x07')
  await primo.flush()
  await secondo.flush()

  // Assert
  expect(seen).toEqual([
    ['pane-1', 'titolo uno'],
    ['pane-2', 'titolo due']
  ])
})

test('dopo remove il pane non emette più titoli', async () => {
  // Arrange
  const registry = new TerminalRegistry()
  const seen: string[] = []
  registry.onTitle((_, title) => seen.push(title))
  const state = registry.create('pane-1')

  // Act
  registry.remove('pane-1')
  state.write('\x1b]0;fantasma\x07')
  await state.flush()

  // Assert — remove deve disiscrivere, non solo dimenticare la chiave.
  expect(seen).toEqual([])
})

test('disposeAll svuota il registro', () => {
  // Arrange
  const registry = new TerminalRegistry()
  registry.create('pane-1')
  registry.create('pane-2')

  // Act
  registry.disposeAll()

  // Assert
  expect(registry.ids()).toEqual([])
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/terminal/terminal-registry.test.ts
```

Atteso: FALLISCE con `Failed to resolve import "./terminal-registry"`.

- [ ] **Passo 3: scrivi l'implementazione**

Crea `src/main/terminal/terminal-registry.ts`:

```ts
import { createTerminalState, type TerminalState } from './terminal-state'

type TitleListener = (paneId: string, title: string) => void

/**
 * Registro dei terminali vivi, uno per pane.
 *
 * Regola di ownership: smontare la vista non uccide niente. Un terminale
 * sparisce solo per remove() esplicita o per disposeAll() al quit — mai
 * perché una finestra si è chiusa.
 */
export class TerminalRegistry {
  readonly #states = new Map<string, TerminalState>()
  readonly #unsubscribes = new Map<string, () => void>()
  readonly #titleListeners = new Set<TitleListener>()

  create(paneId: string, options: { cols?: number; rows?: number } = {}): TerminalState {
    if (this.#states.has(paneId)) throw new Error(`terminale già attivo: ${paneId}`)

    const state = createTerminalState(options)
    this.#states.set(paneId, state)
    this.#unsubscribes.set(
      paneId,
      state.onTitle((title) => {
        for (const listener of this.#titleListeners) listener(paneId, title)
      })
    )
    return state
  }

  get(paneId: string): TerminalState | undefined {
    return this.#states.get(paneId)
  }

  remove(paneId: string): void {
    // La disiscrizione precede il dispose: un terminale rimosso non deve
    // poter emettere un titolo che riporterebbe in vita uno stato morto.
    this.#unsubscribes.get(paneId)?.()
    this.#unsubscribes.delete(paneId)
    this.#states.get(paneId)?.dispose()
    this.#states.delete(paneId)
  }

  ids(): string[] {
    return [...this.#states.keys()]
  }

  disposeAll(): void {
    for (const paneId of this.ids()) this.remove(paneId)
  }

  onTitle(listener: TitleListener): () => void {
    this.#titleListeners.add(listener)
    return () => {
      this.#titleListeners.delete(listener)
    }
  }
}
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/terminal/terminal-registry.test.ts
```

Atteso: 8 test verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/main/terminal/
git commit -m "feat: registro dei terminali con ciclo di vita esplicito"
```

---

### Task 3: debounce e throttle

**File:**
- Crea: `src/main/terminal/debounce.ts`
- Test: `src/main/terminal/debounce.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce:
  `createDebouncer<T>(options: { delayMs: number; onSettle(value: T): void; schedule(fn: () => void, ms: number): unknown; cancel(handle: unknown): void }): { push(value: T): void; cancel(): void }`
  e
  `createThrottle(options: { intervalMs: number; onTick(): void; schedule(fn: () => void, ms: number): unknown; cancel(handle: unknown): void }): { touch(): void; cancel(): void }`.

**Nota di progettazione da rispettare:** `schedule`/`cancel` sono iniettati per
rendere il tempo deterministico nei test, come già fa
`src/main/control/output-coalescer.ts`. Non usare `setTimeout` direttamente.

- [ ] **Passo 1: scrivi il test che fallisce**

Crea `src/main/terminal/debounce.test.ts`:

```ts
import { expect, test } from 'vitest'
import { createDebouncer, createThrottle } from './debounce'

/** Orologio finto: esegue i lavori quando glielo si dice. */
function fakeClock(): {
  schedule: (fn: () => void, ms: number) => unknown
  cancel: (handle: unknown) => void
  run: () => void
} {
  let jobs = new Map<number, () => void>()
  let next = 0
  return {
    schedule: (fn) => {
      const id = next++
      jobs.set(id, fn)
      return id
    },
    cancel: (handle) => {
      jobs.delete(handle as number)
    },
    run: () => {
      const pending = [...jobs.values()]
      jobs = new Map()
      for (const job of pending) job()
    }
  }
}

test('il debouncer consegna solo l ultimo valore', () => {
  // Arrange
  const clock = fakeClock()
  const settled: Array<[number, number]> = []
  const debouncer = createDebouncer<[number, number]>({
    delayMs: 50,
    onSettle: (value) => settled.push(value),
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act — trascinare il bordo della finestra produce una raffica così.
  debouncer.push([80, 24])
  debouncer.push([90, 26])
  debouncer.push([120, 40])
  clock.run()

  // Assert
  expect(settled).toEqual([[120, 40]])
})

test('ogni push rimanda la consegna', () => {
  // Arrange
  const clock = fakeClock()
  const settled: number[] = []
  const debouncer = createDebouncer<number>({
    delayMs: 50,
    onSettle: (value) => settled.push(value),
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act
  debouncer.push(1)
  debouncer.push(2)
  clock.run()
  debouncer.push(3)
  clock.run()

  // Assert — due raffiche distinte, due consegne.
  expect(settled).toEqual([2, 3])
})

test('cancel annulla la consegna in sospeso', () => {
  // Arrange
  const clock = fakeClock()
  const settled: number[] = []
  const debouncer = createDebouncer<number>({
    delayMs: 50,
    onSettle: (value) => settled.push(value),
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act
  debouncer.push(1)
  debouncer.cancel()
  clock.run()

  // Assert
  expect(settled).toEqual([])
})

test('il throttle scatta una volta sola per raffica', () => {
  // Arrange
  const clock = fakeClock()
  let ticks = 0
  const throttle = createThrottle({
    intervalMs: 30_000,
    onTick: () => ticks++,
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act — mille chunk di output non devono produrre mille salvataggi.
  for (let i = 0; i < 1000; i++) throttle.touch()
  clock.run()

  // Assert
  expect(ticks).toBe(1)
})

test('il throttle non rimanda: dopo lo scatto riparte', () => {
  // Arrange
  const clock = fakeClock()
  let ticks = 0
  const throttle = createThrottle({
    intervalMs: 30_000,
    onTick: () => ticks++,
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act
  throttle.touch()
  clock.run()
  throttle.touch()
  clock.run()

  // Assert — differenza con il debouncer: un flusso continuo di output
  // salva comunque ogni intervallo, invece di non salvare mai.
  expect(ticks).toBe(2)
})

test('cancel del throttle annulla lo scatto in sospeso', () => {
  // Arrange
  const clock = fakeClock()
  let ticks = 0
  const throttle = createThrottle({
    intervalMs: 30_000,
    onTick: () => ticks++,
    schedule: clock.schedule,
    cancel: clock.cancel
  })

  // Act
  throttle.touch()
  throttle.cancel()
  clock.run()

  // Assert
  expect(ticks).toBe(0)
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/terminal/debounce.test.ts
```

Atteso: FALLISCE con `Failed to resolve import "./debounce"`.

- [ ] **Passo 3: scrivi l'implementazione**

Crea `src/main/terminal/debounce.ts`:

```ts
export interface Debouncer<T> {
  push(value: T): void
  cancel(): void
}

export interface Throttle {
  touch(): void
  cancel(): void
}

/**
 * Consegna solo l'ultimo valore di una raffica, a raffica finita.
 *
 * Serve al resize: trascinare il bordo della finestra emette un evento per
 * frame, e ogni TIOCSWINSZ fa ridisegnare da capo il TUI dell'agente.
 */
export function createDebouncer<T>(options: {
  delayMs: number
  onSettle(value: T): void
  schedule(fn: () => void, ms: number): unknown
  cancel(handle: unknown): void
}): Debouncer<T> {
  let handle: unknown = null
  let pending: { value: T } | null = null

  const fire = (): void => {
    handle = null
    const current = pending
    pending = null
    if (current !== null) options.onSettle(current.value)
  }

  return {
    push(value: T): void {
      pending = { value }
      if (handle !== null) options.cancel(handle)
      handle = options.schedule(fire, options.delayMs)
    },
    cancel(): void {
      if (handle !== null) options.cancel(handle)
      handle = null
      pending = null
    }
  }
}

/**
 * Scatta al più una volta per intervallo, senza rimandare.
 *
 * Differenza dal debouncer, e ragione per cui esistono entrambi: un pane che
 * produce output di continuo non farebbe mai scattare un debouncer, quindi
 * non verrebbe mai salvato.
 */
export function createThrottle(options: {
  intervalMs: number
  onTick(): void
  schedule(fn: () => void, ms: number): unknown
  cancel(handle: unknown): void
}): Throttle {
  let handle: unknown = null

  const fire = (): void => {
    handle = null
    options.onTick()
  }

  return {
    touch(): void {
      if (handle !== null) return
      handle = options.schedule(fire, options.intervalMs)
    },
    cancel(): void {
      if (handle !== null) options.cancel(handle)
      handle = null
    }
  }
}
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/terminal/debounce.test.ts
```

Atteso: 6 test verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/main/terminal/
git commit -m "feat: debouncer per il resize e throttle per la persistenza"
```

---

### Task 4: resize del PTY

**File:**
- Modifica: `src/main/pty/pty-spawner.ts`
- Modifica: `src/main/pty/pty-manager.ts`
- Test: `src/main/pty/pty-manager.test.ts` (esistente, aggiungi in coda)

**Interfacce:**
- Consuma: `PtyHandle`, `PtySpawner` da `./pty-spawner`.
- Produce: `PtyHandle.resize(cols: number, rows: number): void` e
  `PtyManager.resize(paneId: string, cols: number, rows: number): void`.

**Attenzione:** `PtyHandle` è implementato anche dai finti nei test esistenti.
Aggiungere un metodo obbligatorio all'interfaccia rompe la compilazione di quei
finti: vanno aggiornati nello stesso passo.

- [ ] **Passo 1: scrivi il test che fallisce**

Aggiungi in coda a `src/main/pty/pty-manager.test.ts`:

```ts
test('resize inoltra le dimensioni al pty del pane giusto', () => {
  // Arrange
  const resizes: Array<[string, number, number]> = []
  const manager = new PtyManager((options) => {
    const label = options.cwd
    return {
      write: () => {},
      onData: () => {},
      onExit: () => {},
      kill: () => {},
      resize: (cols, rows) => resizes.push([label, cols, rows])
    }
  })
  manager.spawn('pane-1', { cwd: '/uno', shell: '/bin/zsh', args: [], cols: 80, rows: 24 })
  manager.spawn('pane-2', { cwd: '/due', shell: '/bin/zsh', args: [], cols: 80, rows: 24 })

  // Act
  manager.resize('pane-2', 120, 40)

  // Assert
  expect(resizes).toEqual([['/due', 120, 40]])
})

test('resize su un pane sconosciuto lancia un errore nominato', () => {
  // Arrange
  const manager = new PtyManager(() => ({
    write: () => {},
    onData: () => {},
    onExit: () => {},
    kill: () => {},
    resize: () => {}
  }))

  // Act & Assert
  expect(() => manager.resize('inesistente', 80, 24)).toThrow('pane sconosciuto: inesistente')
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/pty/pty-manager.test.ts
```

Atteso: FALLISCE con `manager.resize is not a function`.

- [ ] **Passo 3: aggiungi `resize` all'interfaccia e allo spawner reale**

In `src/main/pty/pty-spawner.ts`, aggiungi il metodo a `PtyHandle`:

```ts
export interface PtyHandle {
  write(data: string): void
  onData(cb: (data: string) => void): void
  onExit(cb: () => void): void
  kill(): void
  resize(cols: number, rows: number): void
}
```

e all'oggetto restituito da `createNodePtySpawner`, subito dopo `kill`:

```ts
      kill: () => proc.kill(),
      // node-pty chiama il primo parametro `columns`; è posizionale.
      resize: (cols, rows) => proc.resize(cols, rows)
```

- [ ] **Passo 4: aggiungi `resize` al PtyManager**

In `src/main/pty/pty-manager.ts`, subito dopo il metodo `write`:

```ts
  resize(paneId: string, cols: number, rows: number): void {
    const handle = this.#handles.get(paneId)
    if (!handle) throw new Error(`pane sconosciuto: ${paneId}`)
    handle.resize(cols, rows)
  }
```

- [ ] **Passo 5: aggiorna i finti esistenti**

Esistono due classi `FakeHandle implements PtyHandle`, una in
`src/main/pty/pty-manager.test.ts:5` e una in
`src/main/control/dispatch.test.ts:52`. Aggiungi a **entrambe**, accanto agli
altri metodi:

```ts
  resized: Array<[number, number]> = []
  resize(cols: number, rows: number): void {
    this.resized.push([cols, rows])
  }
```

Poi conferma che non ne restino altri:

```bash
pnpm typecheck:node
```

Atteso: nessun errore. Se ne segnala altri, completali allo stesso modo senza
toccare altro.

- [ ] **Passo 6: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/pty/
```

Atteso: tutti verdi, inclusi i due nuovi.

- [ ] **Passo 7: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 8: commit**

```bash
git add src/main/pty/
git commit -m "feat: resize del pty per pane"
```

---

### Task 5: protocollo — resize, read, close, titolo

**File:**
- Modifica: `src/shared/protocol.ts`
- Test: `src/shared/protocol.test.ts` (esistente, aggiungi in coda)

**Interfacce:**
- Consuma: niente.
- Produce: nuovi rami di `ControlRequest` con method `pane.resize`
  (`{ paneId, cols, rows }`), `pane.read`
  (`{ paneId, format: 'text' | 'snapshot', lines? }`), `pane.close`
  (`{ paneId }`); `PaneSnapshot` guadagna `title: string` con default `''`;
  `StateEvent` guadagna `{ type: 'pane.title', paneId, title }`.

- [ ] **Passo 1: scrivi il test che fallisce**

Aggiungi in coda a `src/shared/protocol.test.ts`:

```ts
test('pane.resize richiede dimensioni intere positive', () => {
  // Arrange
  const valido = {
    id: 'r1',
    method: 'pane.resize',
    params: { paneId: 'p1', cols: 120, rows: 40 }
  }

  // Act & Assert
  expect(ControlRequest.safeParse(valido).success).toBe(true)
  expect(
    ControlRequest.safeParse({ ...valido, params: { paneId: 'p1', cols: 0, rows: 40 } }).success
  ).toBe(false)
  expect(
    ControlRequest.safeParse({ ...valido, params: { paneId: 'p1', cols: 80.5, rows: 24 } }).success
  ).toBe(false)
})

test('pane.read accetta solo i due formati previsti', () => {
  // Arrange
  const base = { id: 'r1', method: 'pane.read' }

  // Act & Assert
  expect(ControlRequest.safeParse({ ...base, params: { paneId: 'p1', format: 'text' } }).success).toBe(true)
  expect(
    ControlRequest.safeParse({ ...base, params: { paneId: 'p1', format: 'snapshot' } }).success
  ).toBe(true)
  expect(ControlRequest.safeParse({ ...base, params: { paneId: 'p1', format: 'html' } }).success).toBe(false)
})

test('pane.read usa il formato testo quando non è specificato', () => {
  // Arrange & Act
  const parsed = ControlRequest.parse({ id: 'r1', method: 'pane.read', params: { paneId: 'p1' } })

  // Assert — tillerctl read senza opzioni deve dare testo leggibile.
  expect(parsed.method).toBe('pane.read')
  if (parsed.method === 'pane.read') expect(parsed.params.format).toBe('text')
})

test('pane.close richiede un paneId', () => {
  // Arrange & Act & Assert
  expect(
    ControlRequest.safeParse({ id: 'r1', method: 'pane.close', params: { paneId: 'p1' } }).success
  ).toBe(true)
  expect(
    ControlRequest.safeParse({ id: 'r1', method: 'pane.close', params: { paneId: '' } }).success
  ).toBe(false)
})

test('un pane senza titolo si legge come stringa vuota, non come assente', () => {
  // Arrange & Act
  const pane = PaneSnapshot.parse({ id: 'p1', cwd: '/tmp', createdAt: 0 })

  // Assert — il renderer mostra sempre qualcosa, non deve gestire undefined.
  expect(pane.title).toBe('')
})

test('l evento pane.title porta paneId e titolo', () => {
  // Arrange & Act
  const parsed = StateEvent.safeParse({ type: 'pane.title', paneId: 'p1', title: '✳ lavoro' })

  // Assert
  expect(parsed.success).toBe(true)
})

test('pane.create accetta un paneId scelto da chi chiama', () => {
  // Arrange & Act — serve a riagganciare lo scrollback di un pane chiuso
  // senza esporre API interne: chi ricrea il pane conosce già il suo id.
  const parsed = ControlRequest.parse({
    id: 'r1',
    method: 'pane.create',
    params: { cwd: '/tmp', paneId: 'scelto-da-me' }
  })

  // Assert
  expect(parsed.method).toBe('pane.create')
  if (parsed.method === 'pane.create') expect(parsed.params.paneId).toBe('scelto-da-me')
})
```

Se `PaneSnapshot` o `StateEvent` non sono già importati in quel file,
aggiungili all'import esistente da `./protocol`.

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/shared/protocol.test.ts
```

Atteso: FALLISCE — i metodi nuovi non sono nell'unione discriminata.

- [ ] **Passo 3: estendi il protocollo**

In `src/shared/protocol.ts`, aggiungi `title` a `PaneSnapshot`:

```ts
export const PaneSnapshot = z.object({
  id: z.string().min(1),
  cwd: z.string().min(1),
  createdAt: z.number().int().nonnegative(),
  /** Titolo OSC riportato dal terminale; vuoto finché non ne arriva uno. */
  title: z.string().default('')
})
```

Aggiungi `paneId` ai parametri di `pane.create`, subito dopo `worktreeId`:

```ts
      worktreeId: z.string().optional(),
      /** Se assente, il main ne genera uno. */
      paneId: z.string().min(1).optional()
```

Aggiungi i tre rami dentro `ControlRequest`, subito dopo quello di
`pane.write`:

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('pane.resize'),
    params: z.object({
      paneId: z.string().min(1),
      cols: z.number().int().positive(),
      rows: z.number().int().positive()
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('pane.read'),
    params: z.object({
      paneId: z.string().min(1),
      // `text` è già interpretato e serve a tillerctl e alla detection;
      // `snapshot` è ANSI ricaricabile e serve a rimontare una vista.
      format: z.enum(['text', 'snapshot']).default('text'),
      lines: z.number().int().positive().optional()
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('pane.close'),
    params: z.object({ paneId: z.string().min(1) })
  }),
```

Aggiungi il ramo dentro `StateEvent`:

```ts
  z.object({
    type: z.literal('pane.title'),
    paneId: z.string().min(1),
    title: z.string()
  }),
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/shared/protocol.test.ts
```

Atteso: tutti verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`. Se `typecheck` segnala che il ramo `pane.title` di
`StateEvent` non è gestito da qualche `switch`, completa quel `switch` — non
aggiungere un `default` che nasconda il caso.

- [ ] **Passo 6: commit**

```bash
git add src/shared/
git commit -m "feat: metodi di protocollo per resize, lettura e chiusura dei pane"
```

---

### Task 6: AppState — titolo del pane

**File:**
- Modifica: `src/main/state/app-state.ts`
- Test: `src/main/state/app-state.test.ts` (esistente, aggiungi in coda)

**Interfacce:**
- Consuma: `PaneSnapshot`, `StateEvent` da `../../shared/protocol`.
- Produce: `AppState.setPaneTitle(paneId: string, title: string): void`.

- [ ] **Passo 1: scrivi il test che fallisce**

Aggiungi in coda a `src/main/state/app-state.test.ts`:

```ts
test('setPaneTitle aggiorna lo snapshot ed emette un evento', () => {
  // Arrange
  const state = new AppState()
  const events: StateEvent[] = []
  state.addPane({ id: 'p1', cwd: '/tmp', createdAt: 0, title: '' })
  state.subscribe((event) => events.push(event))

  // Act
  state.setPaneTitle('p1', '✳ lavoro')

  // Assert
  expect(state.snapshot()[0].title).toBe('✳ lavoro')
  expect(events).toEqual([{ type: 'pane.title', paneId: 'p1', title: '✳ lavoro' }])
})

test('setPaneTitle su un pane inesistente non emette nulla', () => {
  // Arrange
  const state = new AppState()
  const events: StateEvent[] = []
  state.subscribe((event) => events.push(event))

  // Act — un titolo può arrivare mentre il pane si sta chiudendo.
  state.setPaneTitle('mai-esistito', 'x')

  // Assert
  expect(events).toEqual([])
})

test('lo stesso titolo due volte emette un evento solo', () => {
  // Arrange
  const state = new AppState()
  state.addPane({ id: 'p1', cwd: '/tmp', createdAt: 0, title: '' })
  const events: StateEvent[] = []
  state.subscribe((event) => events.push(event))

  // Act — un TUI riscrive il proprio titolo di continuo; ogni evento
  // attraversa il confine IPC, quindi i duplicati vanno fermati qui.
  state.setPaneTitle('p1', 'uguale')
  state.setPaneTitle('p1', 'uguale')

  // Assert
  expect(events).toHaveLength(1)
})
```

Aggiungi `StateEvent` all'import da `../../shared/protocol` se manca.
I test esistenti che chiamano `addPane` senza `title` vanno completati con
`title: ''`, perché il tipo ora lo richiede.

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/state/app-state.test.ts
```

Atteso: FALLISCE con `state.setPaneTitle is not a function`.

- [ ] **Passo 3: scrivi l'implementazione**

In `src/main/state/app-state.ts`, subito dopo `removePane`:

```ts
  setPaneTitle(paneId: string, title: string): void {
    const pane = this.#panes.get(paneId)
    if (pane === undefined || pane.title === title) return
    this.#panes.set(paneId, { ...pane, title })
    this.#emit({ type: 'pane.title', paneId, title })
  }
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/state/
```

Atteso: tutti verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/main/state/
git commit -m "feat: titolo del pane nello stato autoritativo"
```

---

### Task 7: dispatcher — cabla il registry

**File:**
- Modifica: `src/main/control/dispatch.ts`
- Test: `src/main/control/dispatch.test.ts` (esistente, aggiungi in coda)

**Interfacce:**
- Consuma: `TerminalRegistry` da `../terminal/terminal-registry`,
  `createDebouncer` da `../terminal/debounce`,
  `TERMINAL_DEFAULT_COLS`/`TERMINAL_DEFAULT_ROWS` da `../terminal/terminal-state`.
- Produce: `DispatchDeps` guadagna `terminals: TerminalRegistry` e
  `resizeDelayMs?: number` (default 80). Il dispatcher gestisce
  `pane.resize`, `pane.read`, `pane.close`.

**Vincolo:** `dispatch.ts` continua a non importare `electron` né `net`. C'è
già un test che lo verifica: non toccarlo.

- [ ] **Passo 1: scrivi il test che fallisce**

**Attenzione — il finto esistente non basta.** L'helper `makeDispatcher()` già
presente nel file costruisce il PtyManager come
`{ spawn: () => {}, write: () => {}, kill: () => {} } as never`: non
implementa `onData`, che da questo task in poi il dispatcher chiama sempre.
Aggiungi un helper nuovo accanto a quello esistente invece di modificarlo, così
i test già verdi restano tali.

Aggiungi questi import in cima a `src/main/control/dispatch.test.ts`:

```ts
import { TerminalRegistry } from '../terminal/terminal-registry'
import type { PtyManager as PtyManagerType } from '../pty/pty-manager'
```

Aggiungi l'helper subito dopo `makeDispatcher`:

```ts
/**
 * Dipendenze complete per i test del motore terminale, con un finto di
 * PtyManager che sa davvero notificare l'output e registrare i resize.
 */
async function makeTerminalDeps(
  overrides: Partial<DispatchDeps> = {}
): Promise<{
  dispatch: ReturnType<typeof createDispatcher>
  terminals: TerminalRegistry
  state: AppState
  emitOutput: (paneId: string, data: string) => void
  resizes: Array<[string, number, number]>
  killed: string[]
}> {
  const db = openDatabase(':memory:')
  await migrateToLatest(db)
  databases.push(db)

  const dataListeners = new Set<(paneId: string, data: string) => void>()
  const resizes: Array<[string, number, number]> = []
  const killed: string[] = []
  const pty = {
    spawn: () => {},
    write: () => {},
    kill: (paneId: string) => killed.push(paneId),
    resize: (paneId: string, cols: number, rows: number) => resizes.push([paneId, cols, rows]),
    onData: (listener: (paneId: string, data: string) => void) => {
      dataListeners.add(listener)
      return () => dataListeners.delete(listener)
    },
    onExit: () => () => {},
    disposeAll: () => {}
  } as unknown as PtyManagerType

  const terminals = new TerminalRegistry()
  const state = new AppState()
  let counter = 0
  const dispatch = createDispatcher({
    state,
    pty,
    terminals,
    window: { focus: () => {}, isOpen: () => true },
    now: () => 1000,
    newPaneId: () => `pane-${++counter}`,
    newId: () => `id-${++counter}`,
    defaultShell: '/bin/sh',
    db,
    ...overrides
  })

  return {
    dispatch,
    terminals,
    state,
    emitOutput: (paneId, data) => {
      for (const listener of dataListeners) listener(paneId, data)
    },
    resizes,
    killed
  }
}

/** Crea un pane e restituisce il suo id, che nei test è sempre `pane-1`. */
async function creaPane(
  dispatch: ReturnType<typeof createDispatcher>,
  params: Record<string, unknown> = {}
): Promise<string> {
  const created = await dispatch({
    id: 'crea',
    method: 'pane.create',
    params: { cwd: '/tmp', focus: false, ...params }
  } as never)
  return (created as { result: { paneId: string } }).result.paneId
}
```

Poi aggiungi in coda al file:

```ts
test('pane.create crea anche il terminale nel main', async () => {
  // Arrange
  const { dispatch, terminals } = await makeTerminalDeps()

  // Act
  const paneId = await creaPane(dispatch)

  // Assert
  expect(terminals.get(paneId)).toBeDefined()
})

test('l output del pty finisce nel terminale del main', async () => {
  // Arrange
  const { dispatch, terminals, emitOutput } = await makeTerminalDeps()
  const paneId = await creaPane(dispatch)

  // Act
  emitOutput(paneId, 'ciao dal processo')
  await terminals.get(paneId)?.flush()

  // Assert
  const read = await dispatch({
    id: 'r2',
    method: 'pane.read',
    params: { paneId, format: 'text' }
  } as never)
  expect((read as { result: { content: string } }).result.content).toContain('ciao dal processo')
})

test('pane.read in formato snapshot restituisce ANSI ricaricabile', async () => {
  // Arrange
  const { dispatch, emitOutput } = await makeTerminalDeps()
  const paneId = await creaPane(dispatch)
  emitOutput(paneId, '\x1b[31mrosso\x1b[0m')

  // Act
  const read = await dispatch({
    id: 'r2',
    method: 'pane.read',
    params: { paneId, format: 'snapshot' }
  } as never)

  // Assert — lo snapshot conserva gli attributi, il testo no.
  const content = (read as { result: { content: string } }).result.content
  expect(content).toContain('rosso')
  expect(content).toContain('\x1b[')
})

test('pane.read su un pane sconosciuto risponde con errore, non lancia', async () => {
  // Arrange
  const { dispatch } = await makeTerminalDeps()

  // Act
  const response = await dispatch({
    id: 'r1',
    method: 'pane.read',
    params: { paneId: 'inesistente', format: 'text' }
  } as never)

  // Assert
  expect(response).toEqual({ id: 'r1', ok: false, error: 'pane sconosciuto: inesistente' })
})

test('il terminale resta leggibile dopo la morte del pty', async () => {
  // Arrange — un agente che va in crash lascia dietro di sé l'output che
  // spiega perché: quell'output deve sopravvivere al processo.
  const { dispatch, terminals, emitOutput } = await makeTerminalDeps()
  const paneId = await creaPane(dispatch)
  emitOutput(paneId, 'ultimo respiro')
  await terminals.get(paneId)?.flush()

  // Act — il pty esce da solo, senza pane.close.
  // (Il finto non emette onExit: il punto è che nessuno rimuove il
  // terminale, ed è esattamente ciò che va verificato.)
  const read = await dispatch({
    id: 'r2',
    method: 'pane.read',
    params: { paneId, format: 'text' }
  } as never)

  // Assert
  expect((read as { result: { content: string } }).result.content).toContain('ultimo respiro')
})

test('pane.resize applica le stesse dimensioni al pty e al terminale', async () => {
  // Arrange
  const { dispatch, terminals, resizes } = await makeTerminalDeps({ resizeDelayMs: 0 })
  const paneId = await creaPane(dispatch)

  // Act
  await dispatch({
    id: 'r2',
    method: 'pane.resize',
    params: { paneId, cols: 120, rows: 40 }
  } as never)
  await new Promise((resolve) => setTimeout(resolve, 20))

  // Assert — disallineare i due riflussi sfalsa ogni snapshot successivo.
  expect(terminals.get(paneId)?.cols).toBe(120)
  expect(terminals.get(paneId)?.rows).toBe(40)
  expect(resizes).toEqual([[paneId, 120, 40]])
})

test('una raffica di resize produce un solo ridimensionamento', async () => {
  // Arrange
  const { dispatch, resizes } = await makeTerminalDeps({ resizeDelayMs: 10 })
  const paneId = await creaPane(dispatch)

  // Act — trascinamento del bordo finestra.
  for (const cols of [82, 90, 100, 118, 120]) {
    await dispatch({
      id: 'r2',
      method: 'pane.resize',
      params: { paneId, cols, rows: 40 }
    } as never)
  }
  await new Promise((resolve) => setTimeout(resolve, 60))

  // Assert
  expect(resizes).toEqual([[paneId, 120, 40]])
})

test('pane.close termina il pty, il terminale e lo stato', async () => {
  // Arrange
  const { dispatch, terminals, state, killed } = await makeTerminalDeps()
  const paneId = await creaPane(dispatch)

  // Act
  await dispatch({ id: 'r2', method: 'pane.close', params: { paneId } } as never)

  // Assert
  expect(terminals.get(paneId)).toBeUndefined()
  expect(state.hasPane(paneId)).toBe(false)
  expect(killed).toEqual([paneId])
})

test('il titolo OSC del terminale arriva nello stato', async () => {
  // Arrange
  const { dispatch, terminals, state, emitOutput } = await makeTerminalDeps()
  const paneId = await creaPane(dispatch)

  // Act
  emitOutput(paneId, '\x1b]0;✳ Claude\x07')
  await terminals.get(paneId)?.flush()

  // Assert — è la fondazione del Layer B della Fase 3.
  expect(state.snapshot()[0].title).toBe('✳ Claude')
})

test('pane.create riusa il paneId fornito dal chiamante', async () => {
  // Arrange
  const { dispatch, terminals } = await makeTerminalDeps()

  // Act
  const paneId = await creaPane(dispatch, { paneId: 'scelto-da-me' })

  // Assert
  expect(paneId).toBe('scelto-da-me')
  expect(terminals.get('scelto-da-me')).toBeDefined()
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/control/dispatch.test.ts
```

Atteso: FALLISCE — `terminals` non è una dipendenza riconosciuta e i tre
metodi non sono gestiti.

- [ ] **Passo 3: estendi le dipendenze del dispatcher**

In `src/main/control/dispatch.ts`, aggiungi gli import:

```ts
import type { TerminalRegistry } from '../terminal/terminal-registry'
import { createDebouncer, type Debouncer } from '../terminal/debounce'
import { TERMINAL_DEFAULT_COLS, TERMINAL_DEFAULT_ROWS } from '../terminal/terminal-state'
```

Sostituisci le costanti locali `DEFAULT_COLS`/`DEFAULT_ROWS` con quelle
importate (cancella le definizioni locali, così esiste una sola verità sulle
dimensioni iniziali) e aggiungi a `DispatchDeps`:

```ts
  terminals: TerminalRegistry
  /** Attesa prima di applicare un resize; 0 nei test che non vogliono aspettare. */
  resizeDelayMs?: number
```

- [ ] **Passo 4: cabla output, titoli e i tre metodi**

Dentro `createDispatcher`, prima del `return`, aggiungi il cablaggio che vale
per tutti i pane:

```ts
  // L'output del pty alimenta il terminale del main. Il renderer riceve lo
  // stesso flusso dal coalescer, che è cablato altrove: qui non lo tocchiamo.
  deps.pty.onData((paneId, data) => {
    deps.terminals.get(paneId)?.write(data)
  })

  deps.terminals.onTitle((paneId, title) => {
    deps.state.setPaneTitle(paneId, title)
  })

  // Un debouncer per pane: due pane ridimensionati insieme non devono
  // annullarsi a vicenda.
  const resizeDebouncers = new Map<string, Debouncer<[number, number]>>()
  const debouncerFor = (paneId: string): Debouncer<[number, number]> => {
    const existing = resizeDebouncers.get(paneId)
    if (existing !== undefined) return existing
    const created = createDebouncer<[number, number]>({
      delayMs: deps.resizeDelayMs ?? 80,
      onSettle: ([cols, rows]) => {
        // Il terminale prima del pty: se il pty muore nel frattempo, lo
        // stato letto resta comunque della dimensione giusta.
        deps.terminals.get(paneId)?.resize(cols, rows)
        try {
          deps.pty.resize(paneId, cols, rows)
        } catch {
          // Pane già chiuso mentre il resize era in coda: nulla da fare.
        }
      },
      schedule: (fn, ms) => setTimeout(fn, ms),
      cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>)
    })
    resizeDebouncers.set(paneId, created)
    return created
  }
```

Dentro `case 'pane.create'`, subito dopo `deps.pty.spawn(...)` e prima di
`deps.state.addPane(pane)`:

```ts
          deps.terminals.create(pane.id, {
            cols: TERMINAL_DEFAULT_COLS,
            rows: TERMINAL_DEFAULT_ROWS
          })
```

e cambia la costruzione di `pane` per includere il titolo e rispettare un id
scelto dal chiamante:

```ts
          const pane = {
            // Chi ricrea un pane dopo un riavvio conosce già il suo id e lo
            // impone, così lo scrollback salvato si riaggancia.
            id: request.params.paneId ?? deps.newPaneId(),
            cwd: request.params.cwd,
            createdAt: deps.now(),
            title: ''
          }
```

**I tre casi esistono già come segnaposto.** Il Task 5 ha dovuto aggiungerli
per far compilare lo `switch` esaustivo su `request.method`: `pane.resize`
applica il resize direttamente al pty, `pane.read` e `pane.close` rispondono
`metodo non ancora disponibile`. **Sostituisci quei tre rami**, non
aggiungerne di nuovi accanto — due `case` con la stessa etichetta non
compilano.

Rimpiazza i tre casi esistenti con questi:

```ts
        case 'pane.resize': {
          const { paneId, cols, rows } = request.params
          if (deps.terminals.get(paneId) === undefined) {
            return { id: request.id, ok: false, error: `pane sconosciuto: ${paneId}` }
          }
          debouncerFor(paneId).push([cols, rows])
          return { id: request.id, ok: true, result: null }
        }

        case 'pane.read': {
          const terminal = deps.terminals.get(request.params.paneId)
          if (terminal === undefined) {
            return {
              id: request.id,
              ok: false,
              error: `pane sconosciuto: ${request.params.paneId}`
            }
          }
          // Senza flush si legge lo stato di un istante fa: xterm analizza
          // in modo asincrono.
          await terminal.flush()
          const content =
            request.params.format === 'snapshot'
              ? terminal.readSnapshot()
              : terminal.readText(request.params.lines)
          return { id: request.id, ok: true, result: { content } }
        }

        case 'pane.close': {
          const { paneId } = request.params
          resizeDebouncers.get(paneId)?.cancel()
          resizeDebouncers.delete(paneId)
          deps.pty.kill(paneId)
          deps.terminals.remove(paneId)
          deps.state.removePane(paneId)
          return { id: request.id, ok: true, result: null }
        }
```

- [ ] **Passo 5: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/control/
```

Atteso: tutti verdi, incluso il test che vieta l'import di `electron`.

- [ ] **Passo 6: valida per mutazione il debouncer del resize**

Sostituisci temporaneamente `debouncerFor(paneId).push([cols, rows])` con
l'applicazione diretta:

```ts
          deps.terminals.get(paneId)?.resize(cols, rows)
          deps.pty.resize(paneId, cols, rows)
```

```bash
pnpm exec vitest run src/main/control/dispatch.test.ts
```

Atteso: il test *"una raffica di resize produce un solo ridimensionamento"*
diventa ROSSO. Se resta verde il test è cieco: correggilo prima di procedere.
Poi ripristina la versione con il debouncer.

- [ ] **Passo 7: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 8: commit**

```bash
git add src/main/control/
git commit -m "feat: il dispatcher cabla i terminali del main"
```

---

### Task 8: scrollback-store — persistenza

**File:**
- Crea: `src/main/terminal/scrollback-store.ts`
- Test: `src/main/terminal/scrollback-store.test.ts`

**Interfacce:**
- Consuma: `Kysely<TillerDatabase>` da `../db/schema`,
  `openDatabase`/`migrateToLatest` da `../db/database` (solo nei test).
- Produce:
  `loadScrollback(db: Kysely<TillerDatabase>, paneId: string): Promise<string | null>`
  e
  `createScrollbackWriter(db: Kysely<TillerDatabase>): ScrollbackWriter`, dove
  `ScrollbackWriter = { enqueue(paneId: string, worktreeId: string, snapshot: string): void; flush(paneId: string): Promise<void>; flushAll(): Promise<void> }`.

**Vincolo dello schema:** `paneScrollback.worktreeId` ha una foreign key su
`worktree.id` e i pragma attivano `foreign_keys = ON`. Un pane senza worktree
**non si persiste**: è effimero per costruzione, e forzarlo violerebbe un
vincolo. `terminalContentId` non ha riferimenti, quindi il paneId ci sta
dentro direttamente.

- [ ] **Passo 1: scrivi il test che fallisce**

Crea `src/main/terminal/scrollback-store.test.ts`:

```ts
import { expect, test } from 'vitest'
import type { Kysely } from 'kysely'
import { openDatabase, migrateToLatest } from '../db/database'
import type { TillerDatabase } from '../db/schema'
import { loadScrollback, createScrollbackWriter } from './scrollback-store'

async function dbConWorktree(): Promise<{ db: Kysely<TillerDatabase>; worktreeId: string }> {
  const db = openDatabase(':memory:')
  await migrateToLatest(db)
  await db
    .insertInto('project')
    .values({
      id: 'prj',
      name: 'Test',
      rootPath: '/tmp/test',
      createdAt: '2026-01-01T00:00:00Z',
      colorHex: null,
      displayName: null,
      iconKind: 'icon',
      iconValue: null,
      avatarImage: null,
      defaultWorktreeBase: null,
      worktreeLocationOverride: null,
      orderIdx: 0
    })
    .execute()
  await db
    .insertInto('worktree')
    .values({
      id: 'wt',
      projectId: 'prj',
      branch: 'main',
      path: '/tmp/test',
      createdAt: '2026-01-01T00:00:00Z',
      comment: null,
      commentUpdatedAt: null,
      isPrimary: 1,
      orderIdx: 0
    })
    .execute()
  return { db, worktreeId: 'wt' }
}

test('uno snapshot salvato si rilegge identico', async () => {
  // Arrange
  const { db, worktreeId } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act
  writer.enqueue('pane-1', worktreeId, '\x1b[31mrosso\x1b[0m\r\nseconda riga')
  await writer.flush('pane-1')

  // Assert
  expect(await loadScrollback(db, 'pane-1')).toBe('\x1b[31mrosso\x1b[0m\r\nseconda riga')
})

test('un pane senza scrollback salvato restituisce null', async () => {
  // Arrange
  const { db } = await dbConWorktree()

  // Act & Assert
  expect(await loadScrollback(db, 'mai-salvato')).toBeNull()
})

test('salvataggi ripetuti sostituiscono invece di accumulare', async () => {
  // Arrange
  const { db, worktreeId } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act
  writer.enqueue('pane-1', worktreeId, 'primo')
  await writer.flush('pane-1')
  writer.enqueue('pane-1', worktreeId, 'secondo')
  await writer.flush('pane-1')

  // Assert
  expect(await loadScrollback(db, 'pane-1')).toBe('secondo')
  const righe = await db.selectFrom('paneScrollback').selectAll().execute()
  expect(righe).toHaveLength(1)
})

test('più enqueue prima di un flush salvano solo l ultimo', async () => {
  // Arrange
  const { db, worktreeId } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act — coalescing per sostituzione, come ScrollbackQueue nell app Swift.
  writer.enqueue('pane-1', worktreeId, 'vecchio')
  writer.enqueue('pane-1', worktreeId, 'intermedio')
  writer.enqueue('pane-1', worktreeId, 'ultimo')
  await writer.flush('pane-1')

  // Assert
  expect(await loadScrollback(db, 'pane-1')).toBe('ultimo')
})

test('flushAll salva ogni pane in coda', async () => {
  // Arrange
  const { db, worktreeId } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act
  writer.enqueue('pane-1', worktreeId, 'uno')
  writer.enqueue('pane-2', worktreeId, 'due')
  await writer.flushAll()

  // Assert
  expect(await loadScrollback(db, 'pane-1')).toBe('uno')
  expect(await loadScrollback(db, 'pane-2')).toBe('due')
})

test('gli accenti e i caratteri non ASCII sopravvivono al giro', async () => {
  // Arrange — il titolo di Claude usa ✳ e quello di pi usa π: se la codifica
  // sbaglia, la Fase 3 legge spazzatura.
  const { db, worktreeId } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act
  writer.enqueue('pane-1', worktreeId, '✳ π — città')
  await writer.flush('pane-1')

  // Assert
  expect(await loadScrollback(db, 'pane-1')).toBe('✳ π — città')
})

test('flush su un pane senza coda non lancia', async () => {
  // Arrange
  const { db } = await dbConWorktree()
  const writer = createScrollbackWriter(db)

  // Act & Assert
  await expect(writer.flush('mai-accodato')).resolves.toBeUndefined()
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/terminal/scrollback-store.test.ts
```

Atteso: FALLISCE con `Failed to resolve import "./scrollback-store"`.

- [ ] **Passo 3: scrivi l'implementazione**

Crea `src/main/terminal/scrollback-store.ts`:

```ts
import type { Kysely } from 'kysely'
import type { TillerDatabase } from '../db/schema'

export interface ScrollbackWriter {
  /** Registra lo snapshot più recente; l'ultimo vince. */
  enqueue(paneId: string, worktreeId: string, snapshot: string): void
  flush(paneId: string): Promise<void>
  flushAll(): Promise<void>
}

export async function loadScrollback(
  db: Kysely<TillerDatabase>,
  paneId: string
): Promise<string | null> {
  const row = await db
    .selectFrom('paneScrollback')
    .select('data')
    .where('terminalContentId', '=', paneId)
    .executeTakeFirst()
  if (row === undefined) return null
  return new TextDecoder().decode(row.data)
}

/**
 * Coda di scrittura con coalescing per sostituzione, come ScrollbackQueue
 * nell'app Swift: l'ultimo snapshot vince e c'è al più una scrittura in volo
 * per pane. Uno snapshot è lo stato completo, non un incremento, quindi
 * scartare quelli intermedi non perde niente.
 */
export function createScrollbackWriter(db: Kysely<TillerDatabase>): ScrollbackWriter {
  const pending = new Map<string, { worktreeId: string; snapshot: string }>()

  const writeNow = async (paneId: string): Promise<void> => {
    const entry = pending.get(paneId)
    if (entry === undefined) return
    pending.delete(paneId)
    const data = new TextEncoder().encode(entry.snapshot)
    await db
      .insertInto('paneScrollback')
      .values({
        terminalContentId: paneId,
        worktreeId: entry.worktreeId,
        data,
        updatedAt: new Date().toISOString()
      })
      .onConflict((oc) =>
        oc.column('terminalContentId').doUpdateSet({ data, updatedAt: new Date().toISOString() })
      )
      .execute()
  }

  return {
    enqueue(paneId: string, worktreeId: string, snapshot: string): void {
      pending.set(paneId, { worktreeId, snapshot })
    },
    flush: writeNow,
    async flushAll(): Promise<void> {
      await Promise.all([...pending.keys()].map(writeNow))
    }
  }
}
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/terminal/scrollback-store.test.ts
```

Atteso: 7 test verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/main/terminal/
git commit -m "feat: persistenza dello scrollback come snapshot serializzato"
```

---

### Task 9: cablaggio della persistenza nel main

**File:**
- Modifica: `src/main/index.ts`
- Modifica: `src/main/control/dispatch.ts` (aggancio per il salvataggio)
- Test: `src/main/control/dispatch.test.ts` (aggiungi in coda)

**Interfacce:**
- Consuma: `createScrollbackWriter`, `loadScrollback` da
  `../terminal/scrollback-store`; `createThrottle` da `../terminal/debounce`.
- Produce: `DispatchDeps` guadagna
  `scrollback?: { save(paneId: string, worktreeId: string, snapshot: string): void }`
  e `pane.create` accetta il `worktreeId` già presente nel protocollo,
  ricaricando lo scrollback salvato prima di avviare il pty.

**Costante:** `SCROLLBACK_THROTTLE_MS = 30_000`, definita in
`src/main/terminal/scrollback-store.ts` ed esportata.

- [ ] **Passo 1: scrivi il test che fallisce**

Aggiungi in coda a `src/main/control/dispatch.test.ts`:

```ts
test('un pane con worktree salva lo scrollback alla chiusura', async () => {
  // Arrange
  const salvati: Array<[string, string, string]> = []
  const { dispatch, terminals, emitOutput } = await makeTerminalDeps({
    scrollback: {
      save: (paneId, worktreeId, snapshot) => salvati.push([paneId, worktreeId, snapshot])
    }
  })
  const paneId = await creaPane(dispatch, { worktreeId: 'wt-1' })
  emitOutput(paneId, 'contenuto da salvare')
  await terminals.get(paneId)?.flush()

  // Act
  await dispatch({ id: 'r2', method: 'pane.close', params: { paneId } } as never)

  // Assert
  expect(salvati).toHaveLength(1)
  expect(salvati[0][0]).toBe(paneId)
  expect(salvati[0][1]).toBe('wt-1')
  expect(salvati[0][2]).toContain('contenuto da salvare')
})

test('un pane senza worktree non tenta di salvare', async () => {
  // Arrange — paneScrollback.worktreeId ha una foreign key: salvare qui
  // violerebbe un vincolo, non è una svista.
  const salvati: string[] = []
  const { dispatch } = await makeTerminalDeps({
    scrollback: { save: (paneId: string) => salvati.push(paneId) }
  })
  const paneId = await creaPane(dispatch)

  // Act
  await dispatch({ id: 'r2', method: 'pane.close', params: { paneId } } as never)

  // Assert
  expect(salvati).toEqual([])
})

test('pane.create ricarica lo scrollback prima di far partire il pty', async () => {
  // Arrange
  const { dispatch, terminals, emitOutput } = await makeTerminalDeps({
    scrollback: { save: () => {} },
    restoreScrollback: async () => 'cronologia precedente\r\n'
  })

  // Act
  const paneId = await creaPane(dispatch, { worktreeId: 'wt-1' })
  emitOutput(paneId, 'output nuovo')
  await terminals.get(paneId)?.flush()

  // Assert — la cronologia precede l'output nuovo, nell'ordine giusto.
  const read = await dispatch({
    id: 'r2',
    method: 'pane.read',
    params: { paneId, format: 'text' }
  } as never)
  const content = (read as { result: { content: string } }).result.content
  expect(content.indexOf('cronologia precedente')).toBeLessThan(content.indexOf('output nuovo'))
})

test('uno scrollback illeggibile non impedisce di aprire il pane', async () => {
  // Arrange — un database corrotto o una riga scritta male non devono
  // rendere impossibile aprire un terminale: si perde la cronologia, non
  // l'uso dell'app.
  const { dispatch, terminals, emitOutput } = await makeTerminalDeps({
    scrollback: { save: () => {} },
    restoreScrollback: async () => {
      throw new Error('database illeggibile')
    }
  })

  // Act
  const paneId = await creaPane(dispatch, { worktreeId: 'wt-1' })
  emitOutput(paneId, 'il pane funziona lo stesso')
  await terminals.get(paneId)?.flush()

  // Assert
  const read = await dispatch({
    id: 'r2',
    method: 'pane.read',
    params: { paneId, format: 'text' }
  } as never)
  expect((read as { result: { content: string } }).result.content).toContain(
    'il pane funziona lo stesso'
  )
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/main/control/dispatch.test.ts
```

Atteso: FALLISCE — `scrollback` e `restoreScrollback` non sono dipendenze
riconosciute.

- [ ] **Passo 3: estendi il dispatcher**

In `src/main/control/dispatch.ts`, aggiungi a `DispatchDeps`:

```ts
  scrollback?: { save(paneId: string, worktreeId: string, snapshot: string): void }
  restoreScrollback?: (paneId: string) => Promise<string | null>
```

Dentro `createDispatcher`, tieni traccia del worktree di ogni pane:

```ts
  const worktreeByPane = new Map<string, string>()
```

In `case 'pane.create'`, dopo `deps.terminals.create(...)` e **prima** dello
spawn del pty, ricarica la cronologia:

```ts
          const terminal = deps.terminals.create(pane.id, {
            cols: TERMINAL_DEFAULT_COLS,
            rows: TERMINAL_DEFAULT_ROWS
          })
          const worktreeId = request.params.worktreeId
          if (worktreeId !== undefined) {
            worktreeByPane.set(pane.id, worktreeId)
            try {
              const restored = await deps.restoreScrollback?.(pane.id)
              if (restored !== null && restored !== undefined) terminal.write(restored)
            } catch {
              // Cronologia illeggibile: si perde lo scrollback, non l'uso
              // del terminale. Aprire un pane deve riuscire comunque.
            }
          }
```

Sposta la chiamata `deps.pty.spawn(...)` **dopo** questo blocco, così la
cronologia entra nel terminale prima dell'output nuovo.

In `case 'pane.close'`, prima di rimuovere il terminale:

```ts
          const worktreeId = worktreeByPane.get(paneId)
          const terminal = deps.terminals.get(paneId)
          if (worktreeId !== undefined && terminal !== undefined) {
            await terminal.flush()
            deps.scrollback?.save(paneId, worktreeId, terminal.readSnapshot())
          }
          worktreeByPane.delete(paneId)
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/main/control/
```

Atteso: tutti verdi.

- [ ] **Passo 5: cabla il main**

In `src/main/index.ts`, aggiungi gli import:

```ts
import { TerminalRegistry } from './terminal/terminal-registry'
import {
  createScrollbackWriter,
  loadScrollback,
  SCROLLBACK_THROTTLE_MS
} from './terminal/scrollback-store'
import { createThrottle } from './terminal/debounce'
```

Aggiungi `SCROLLBACK_THROTTLE_MS` a `scrollback-store.ts`:

```ts
/** Salvataggio periodico mentre un pane produce output. */
export const SCROLLBACK_THROTTLE_MS = 30_000
```

Accanto a `const pty = new PtyManager(...)`:

```ts
const terminals = new TerminalRegistry()
const scrollbackWriter = createScrollbackWriter(db)
```

Nel corpo di `app.whenReady()`, prima di `createDispatcher`, prepara il
salvataggio periodico:

```ts
  // Un throttle per pane: un flusso continuo di output salva comunque ogni
  // trenta secondi, invece di non salvare mai come farebbe un debouncer.
  const throttles = new Map<string, ReturnType<typeof createThrottle>>()
  const saveScrollback = (paneId: string, worktreeId: string, snapshot: string): void => {
    scrollbackWriter.enqueue(paneId, worktreeId, snapshot)
    void scrollbackWriter.flush(paneId)
  }
  const touchScrollback = (paneId: string, worktreeId: string): void => {
    const existing = throttles.get(paneId)
    if (existing !== undefined) return existing.touch()
    const throttle = createThrottle({
      intervalMs: SCROLLBACK_THROTTLE_MS,
      onTick: () => {
        const terminal = terminals.get(paneId)
        if (terminal === undefined) return
        void terminal.flush().then(() => {
          scrollbackWriter.enqueue(paneId, worktreeId, terminal.readSnapshot())
          void scrollbackWriter.flush(paneId)
        })
      },
      schedule: (fn, ms) => setTimeout(fn, ms),
      cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>)
    })
    throttles.set(paneId, throttle)
    throttle.touch()
  }
```

Passa le dipendenze nuove a `createDispatcher`:

```ts
    terminals,
    scrollback: { save: saveScrollback },
    restoreScrollback: (paneId) => loadScrollback(db, paneId),
```

- [ ] **Passo 6: aggancia il throttle all'output e sistema il quit**

Ancora in `src/main/index.ts`, dopo `createDispatcher`, aggancia il throttle
al flusso di output — il `worktreeId` lo conosce lo stato:

```ts
  pty.onData((paneId) => {
    const worktreeId = paneWorktrees.get(paneId)
    if (worktreeId !== undefined) touchScrollback(paneId, worktreeId)
  })
```

dove `paneWorktrees` è una `Map<string, string>` popolata cablando l'evento
di creazione: aggiungi accanto alle altre costanti di modulo

```ts
const paneWorktrees = new Map<string, string>()
```

e in `saveScrollback` registra l'associazione:

```ts
  const saveScrollback = (paneId: string, worktreeId: string, snapshot: string): void => {
    paneWorktrees.set(paneId, worktreeId)
    scrollbackWriter.enqueue(paneId, worktreeId, snapshot)
    void scrollbackWriter.flush(paneId)
  }
```

Sostituisci l'handler `before-quit` con la versione che attende davvero:

```ts
// before-quit NON attende gli handler asincroni: senza preventDefault più
// app.exit(), il flush dello scrollback verrebbe troncato a metà.
let quitting = false
app.on('before-quit', (event) => {
  if (quitting) return
  quitting = true
  event.preventDefault()
  disposeIpc?.()
  void (async () => {
    for (const paneId of terminals.ids()) {
      const worktreeId = paneWorktrees.get(paneId)
      const terminal = terminals.get(paneId)
      if (worktreeId === undefined || terminal === undefined) continue
      await terminal.flush()
      scrollbackWriter.enqueue(paneId, worktreeId, terminal.readSnapshot())
    }
    await scrollbackWriter.flushAll()
    terminals.disposeAll()
    pty.disposeAll()
    await socketServer?.close()
    app.exit(0)
  })()
})
```

- [ ] **Passo 7: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 8: commit**

```bash
git add src/main/
git commit -m "feat: persistenza dello scrollback con throttle e flush al quit"
```

---

### Task 10: comandi tillerctl

**File:**
- Modifica: `cli/args.ts`
- Modifica: `cli/tillerctl.ts`
- Test: `cli/args.test.ts`, `cli/tillerctl.test.ts` (esistenti, aggiungi in coda)

**Interfacce:**
- Consuma: la tabella `COMMANDS` e `parseCommand(argv)` già presenti in
  `cli/args.ts`.
- Produce: i comandi `read`, `resize`, `close`, `send`, `state`, e le opzioni
  `--worktree` e `--pane-id` per `run`.

**Come è fatta davvero questa CLI.** `cli/args.ts` non usa uno `switch`: ha una
tabella dichiarativa `COMMANDS` in cui ogni voce è
`{ method, options, required, build }`. Aggiungere un comando significa
aggiungere una voce. `parseCommand(argv)` restituisce `{ method, params }` e
lancia `--<nome> è obbligatorio` per le opzioni mancanti; non inventare un
formato di messaggio diverso. `cli/tillerctl.ts` invia già qualunque metodo in
modo generico: **non va toccato**, salvo la resa a schermo di `pane.read`.

- [ ] **Passo 1: scrivi il test che fallisce**

Aggiungi in coda a `cli/args.test.ts`:

```ts
test('read costruisce una pane.read con il formato testo per default', () => {
  // Arrange & Act
  const parsed = parseCommand(['read', '--pane', 'p1'])

  // Assert
  expect(parsed).toEqual({ method: 'pane.read', params: { paneId: 'p1', format: 'text' } })
})

test('read accetta il formato snapshot e un numero di righe', () => {
  // Arrange & Act
  const parsed = parseCommand(['read', '--pane', 'p1', '--format', 'snapshot', '--lines', '20'])

  // Assert
  expect(parsed).toEqual({
    method: 'pane.read',
    params: { paneId: 'p1', format: 'snapshot', lines: 20 }
  })
})

test('read senza --pane dice quale opzione manca', () => {
  // Arrange & Act & Assert
  expect(() => parseCommand(['read'])).toThrow('--pane è obbligatorio')
})

test('resize converte le dimensioni in numeri', () => {
  // Arrange & Act
  const parsed = parseCommand(['resize', '--pane', 'p1', '--cols', '120', '--rows', '40'])

  // Assert — il protocollo vuole interi, parseArgs restituisce stringhe.
  expect(parsed).toEqual({ method: 'pane.resize', params: { paneId: 'p1', cols: 120, rows: 40 } })
})

test('resize rifiuta dimensioni non numeriche', () => {
  // Arrange & Act & Assert
  expect(() => parseCommand(['resize', '--pane', 'p1', '--cols', 'molte', '--rows', '40'])).toThrow(
    '--cols deve essere un numero intero positivo'
  )
})

test('resize rifiuta dimensioni nulle', () => {
  // Arrange & Act & Assert
  expect(() => parseCommand(['resize', '--pane', 'p1', '--cols', '0', '--rows', '40'])).toThrow(
    '--cols deve essere un numero intero positivo'
  )
})

test('close costruisce una pane.close', () => {
  // Arrange & Act
  const parsed = parseCommand(['close', '--pane', 'p1'])

  // Assert
  expect(parsed).toEqual({ method: 'pane.close', params: { paneId: 'p1' } })
})

test('send scrive nel pane senza aggiungere un a capo', () => {
  // Arrange & Act
  const parsed = parseCommand(['send', '--pane', 'p1', '--data', 'echo ciao\n'])

  // Assert — chi chiama decide se e dove va l'a capo.
  expect(parsed).toEqual({ method: 'pane.write', params: { paneId: 'p1', data: 'echo ciao\n' } })
})

test('state non richiede opzioni', () => {
  // Arrange & Act
  const parsed = parseCommand(['state'])

  // Assert
  expect(parsed).toEqual({ method: 'state.get', params: {} })
})

test('run accetta un worktree e un id di pane scelto da chi chiama', () => {
  // Arrange & Act
  const parsed = parseCommand([
    'run',
    '--cmd',
    'echo x',
    '--cwd',
    '/tmp',
    '--worktree',
    'wt-1',
    '--pane-id',
    'scelto'
  ])

  // Assert
  expect(parsed.method).toBe('pane.create')
  expect(parsed.params).toEqual({
    cwd: '/tmp',
    focus: false,
    worktreeId: 'wt-1',
    paneId: 'scelto'
  })
  expect(parsed.cmd).toBe('echo x')
})
```

- [ ] **Passo 2: esegui i test e verifica che falliscano**

```bash
pnpm exec vitest run cli/args.test.ts
```

Atteso: FALLISCONO con `comando sconosciuto: read`.

- [ ] **Passo 3: aggiungi il convertitore e le voci alla tabella**

In `cli/args.ts`, sopra la costante `COMMANDS`:

```ts
/** parseArgs restituisce stringhe; il protocollo vuole interi positivi. */
function intPositivo(raw: string, nome: string): number {
  const value = Number(raw)
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`--${nome} deve essere un numero intero positivo`)
  }
  return value
}
```

Aggiungi queste voci dentro `COMMANDS`, dopo `import`:

```ts
  read: {
    method: 'pane.read',
    options: {
      pane: { type: 'string' },
      format: { type: 'string' },
      lines: { type: 'string' }
    },
    required: ['pane'],
    build: (v: Record<string, string>) => ({
      paneId: v.pane,
      format: v.format === 'snapshot' ? 'snapshot' : 'text',
      ...(v.lines === undefined ? {} : { lines: intPositivo(v.lines, 'lines') })
    })
  },
  resize: {
    method: 'pane.resize',
    options: {
      pane: { type: 'string' },
      cols: { type: 'string' },
      rows: { type: 'string' }
    },
    required: ['pane', 'cols', 'rows'],
    build: (v: Record<string, string>) => ({
      paneId: v.pane,
      cols: intPositivo(v.cols, 'cols'),
      rows: intPositivo(v.rows, 'rows')
    })
  },
  close: {
    method: 'pane.close',
    options: { pane: { type: 'string' } },
    required: ['pane'],
    build: (v: Record<string, string>) => ({ paneId: v.pane })
  },
  send: {
    method: 'pane.write',
    options: { pane: { type: 'string' }, data: { type: 'string' } },
    required: ['pane', 'data'],
    build: (v: Record<string, string>) => ({ paneId: v.pane, data: v.data })
  },
  state: { method: 'state.get', options: {}, required: [], build: () => ({}) }
```

- [ ] **Passo 4: aggiungi `--worktree` e `--pane-id` a `run`**

`run` non passa dalla tabella: è il ramo esplicito dentro
`parseRequestCommand`. Aggiungi le due opzioni al suo `parseArgs`:

```ts
      options: {
        cmd: { type: 'string' },
        cwd: { type: 'string' },
        focus: { type: 'boolean', default: false },
        agent: { type: 'string' },
        worktree: { type: 'string' },
        'pane-id': { type: 'string' }
      },
```

e ai parametri costruiti:

```ts
      params: {
        cwd: values.cwd ?? process.cwd(),
        focus: values.focus === true,
        ...(values.agent === undefined ? {} : { agent: values.agent }),
        ...(values.worktree === undefined ? {} : { worktreeId: values.worktree }),
        ...(values['pane-id'] === undefined ? {} : { paneId: values['pane-id'] })
      },
```

Fai la stessa aggiunta al `parseArgs` dentro `parseCliArgs`, che condivide le
stesse opzioni, altrimenti i due parser divergono.

- [ ] **Passo 5: aggiorna il testo di aiuto**

In `HELP_TEXT`, dopo la riga di `import`:

```
  tillerctl read --pane <id> [--format text|snapshot] [--lines N]
  tillerctl resize --pane <id> --cols <n> --rows <n>
  tillerctl close --pane <id>
  tillerctl send --pane <id> --data <testo>
  tillerctl state
```

e completa la riga di `run` con le opzioni nuove:

```
  tillerctl run --cmd "<comando>" [--cwd <path>] [--focus] [--worktree <id>] [--pane-id <id>]
```

- [ ] **Passo 6: stampa il contenuto di `read` in chiaro**

In `cli/tillerctl.ts`, dentro il ramo generico, prima di
`process.stdout.write(\`${JSON.stringify(response.result)}\n\`)`:

```ts
    // read serve a leggere, non a ispezionare JSON: stampa il contenuto nudo.
    if (command.method === 'pane.read') {
      process.stdout.write(`${(response.result as { content: string }).content}\n`)
      return 0
    }
```

- [ ] **Passo 7: esegui i test e verifica che passino**

```bash
pnpm exec vitest run cli/
```

Atteso: tutti verdi.

- [ ] **Passo 8: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 9: commit**

```bash
git add cli/ src/shared/
git commit -m "feat: comandi tillerctl per leggere, ridimensionare e chiudere i pane"
```

---

### Task 11: tema del terminale

**File:**
- Crea: `src/renderer/src/lib/terminal-theme.ts`
- Test: `src/renderer/src/lib/terminal-theme.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce:
  `terminalOptions(settings: { fontSize: number; translucent: boolean }): { fontFamily: string; fontSize: number; cursorBlink: boolean; allowProposedApi: boolean; theme: { background: string; foreground: string; cursor: string } }`
  e `clampFontSize(value: number): number`.

**Riferimento:** i limiti replicano `AppSettings.clampTerminalFontSize`
nell'app Swift: minimo 9, massimo 24.

- [ ] **Passo 1: scrivi il test che fallisce**

Crea `src/renderer/src/lib/terminal-theme.test.ts`:

```ts
import { expect, test } from 'vitest'
import { terminalOptions, clampFontSize } from './terminal-theme'

test('la dimensione del font resta dentro i limiti', () => {
  // Arrange & Act & Assert — stessi limiti dell app Swift.
  expect(clampFontSize(2)).toBe(9)
  expect(clampFontSize(13)).toBe(13)
  expect(clampFontSize(99)).toBe(24)
})

test('una dimensione non finita ricade sul default', () => {
  // Arrange & Act & Assert
  expect(clampFontSize(Number.NaN)).toBe(13)
})

test('il tema traslucido ha uno sfondo trasparente', () => {
  // Arrange & Act
  const opaco = terminalOptions({ fontSize: 13, translucent: false })
  const traslucido = terminalOptions({ fontSize: 13, translucent: true })

  // Assert — con lo sfondo opaco la traslucenza della finestra non si vede.
  expect(opaco.theme.background).not.toBe('#00000000')
  expect(traslucido.theme.background).toBe('#00000000')
})

test('le opzioni riportano la dimensione limitata, non quella grezza', () => {
  // Arrange & Act
  const options = terminalOptions({ fontSize: 200, translucent: false })

  // Assert
  expect(options.fontSize).toBe(24)
})

test('allowProposedApi è attivo perché unicode11 lo richiede', () => {
  // Arrange & Act
  const options = terminalOptions({ fontSize: 13, translucent: false })

  // Assert
  expect(options.allowProposedApi).toBe(true)
})
```

- [ ] **Passo 2: esegui il test e verifica che fallisca**

```bash
pnpm exec vitest run src/renderer/src/lib/terminal-theme.test.ts
```

Atteso: FALLISCE con `Failed to resolve import "./terminal-theme"`.

- [ ] **Passo 3: scrivi l'implementazione**

Crea `src/renderer/src/lib/terminal-theme.ts`:

```ts
const MIN_FONT_SIZE = 9
const MAX_FONT_SIZE = 24
const DEFAULT_FONT_SIZE = 13

/** Stessi limiti di AppSettings.clampTerminalFontSize nell'app Swift. */
export function clampFontSize(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_FONT_SIZE
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(value)))
}

export function terminalOptions(settings: { fontSize: number; translucent: boolean }): {
  fontFamily: string
  fontSize: number
  cursorBlink: boolean
  allowProposedApi: boolean
  theme: { background: string; foreground: string; cursor: string }
} {
  return {
    fontFamily: 'SF Mono, Menlo, monospace',
    fontSize: clampFontSize(settings.fontSize),
    cursorBlink: true,
    // Richiesto da addon-unicode11, che usa API ancora marcate proposte.
    allowProposedApi: true,
    theme: {
      // Sfondo trasparente: la traslucenza è della finestra, non del
      // terminale. Uno sfondo opaco la coprirebbe.
      background: settings.translucent ? '#00000000' : '#1F1F26',
      foreground: '#E4E4E7',
      cursor: '#E4E4E7'
    }
  }
}
```

- [ ] **Passo 4: esegui i test e verifica che passino**

```bash
pnpm exec vitest run src/renderer/src/lib/terminal-theme.test.ts
```

Atteso: 5 test verdi.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/renderer/
git commit -m "feat: tema e limiti del font del terminale"
```

---

### Task 12: il pane nel renderer

**File:**
- Modifica: `src/renderer/src/components/Terminal.svelte`
- Crea: `src/renderer/src/components/TerminalSearch.svelte`

**Interfacce:**
- Consuma: `terminalOptions` da `../lib/terminal-theme`;
  `window.tiller.request` con i metodi `pane.resize` e `pane.read`.
- Produce: `Terminal.svelte` accetta `Props = { paneId: string; visible: boolean }`.

**Da rispettare, sono i punti che questa fase esiste per risolvere:**

1. Il browser tiene circa sedici contesti WebGL per pagina e oltre quella
   soglia scarta il più vecchio, che perde il contenuto. Un pane non visibile
   **deve** liberare il proprio addon WebGL.
2. Il montaggio chiede lo stato al main con `pane.read` in formato
   `snapshot`: il renderer non ha cronologia propria.
3. `FitAddon` calcola righe e colonne dai pixel; il risultato va mandato al
   main con `pane.resize`, che è l'unico a poterlo applicare al pty.

- [ ] **Passo 1: scrivi il componente di ricerca**

Crea `src/renderer/src/components/TerminalSearch.svelte`:

```svelte
<script lang="ts">
  import type { SearchAddon } from '@xterm/addon-search'

  interface Props {
    search: SearchAddon
    onClose: () => void
  }

  const { search, onClose }: Props = $props()
  let query = $state('')

  function onInput(event: Event): void {
    query = (event.target as HTMLInputElement).value
    if (query === '') search.clearDecorations()
    else search.findNext(query, { incremental: true })
  }

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') return onClose()
    if (event.key !== 'Enter') return
    if (event.shiftKey) search.findPrevious(query)
    else search.findNext(query)
  }
</script>

<div class="search">
  <!-- svelte-ignore a11y_autofocus -->
  <input
    autofocus
    type="search"
    placeholder="Search"
    value={query}
    oninput={onInput}
    onkeydown={onKeydown}
  />
  <button type="button" onclick={onClose}>Close</button>
</div>

<style>
  .search {
    position: absolute;
    top: 4px;
    right: 4px;
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 6px;
    background: rgb(0 0 0 / 0.6);
  }
</style>
```

- [ ] **Passo 2: riscrivi il pane**

Sostituisci il contenuto di `src/renderer/src/components/Terminal.svelte`:

```svelte
<script lang="ts">
  import { Terminal } from '@xterm/xterm'
  import { WebglAddon } from '@xterm/addon-webgl'
  import { FitAddon } from '@xterm/addon-fit'
  import { SearchAddon } from '@xterm/addon-search'
  import { Unicode11Addon } from '@xterm/addon-unicode11'
  import { WebLinksAddon } from '@xterm/addon-web-links'
  import type { Attachment } from 'svelte/attachments'
  import { terminalOptions } from '../lib/terminal-theme'
  import TerminalSearch from './TerminalSearch.svelte'
  import '@xterm/xterm/css/xterm.css'

  interface Props {
    paneId: string
    visible?: boolean
  }

  const { paneId, visible = true }: Props = $props()

  let searchAddon = $state<SearchAddon | null>(null)
  let searchOpen = $state(false)

  /**
   * Seam di test, non API applicativa: xterm renderizza su canvas, quindi il
   * testo non esiste nel DOM e nessuna asserzione su textContent lo legge.
   */
  function readBuffer(term: Terminal): string {
    const buffer = term.buffer.active
    const lines: string[] = []
    for (let i = 0; i < buffer.length; i++) {
      lines.push(buffer.getLine(i)?.translateToString(true) ?? '')
    }
    return lines.join('\n')
  }

  const attachTerminal: Attachment<HTMLDivElement> = (element) => {
    const term = new Terminal(terminalOptions({ fontSize: 13, translucent: true }))
    const fit = new FitAddon()
    const search = new SearchAddon()
    const unicode = new Unicode11Addon()
    term.loadAddon(fit)
    term.loadAddon(search)
    term.loadAddon(unicode)
    term.unicode.activeVersion = '11'
    term.loadAddon(new WebLinksAddon((_, uri) => window.open(uri, '_blank')))
    term.open(element)
    searchAddon = search

    // Il contesto WebGL è una risorsa scarsa: il browser ne tiene circa
    // sedici per pagina e oltre quella soglia scarta il più vecchio, che
    // sbianca. Sta acceso solo mentre il pane si vede.
    let webgl: WebglAddon | null = null
    const setVisible = (isVisible: boolean): void => {
      if (isVisible && webgl === null) {
        try {
          webgl = new WebglAddon()
          term.loadAddon(webgl)
        } catch {
          // GPU non disponibile: xterm resta sul renderer DOM.
          webgl = null
        }
        fit.fit()
      } else if (!isVisible && webgl !== null) {
        webgl.dispose()
        webgl = null
      }
    }

    const sendResize = (): void => {
      fit.fit()
      void window.tiller.request({
        id: crypto.randomUUID(),
        method: 'pane.resize',
        params: { paneId, cols: term.cols, rows: term.rows }
      })
    }

    // Il renderer non ha cronologia propria: la chiede al main, che è
    // l'unico a possederla.
    void window.tiller
      .request({
        id: crypto.randomUUID(),
        method: 'pane.read',
        params: { paneId, format: 'snapshot' }
      })
      .then((response) => {
        if (!response.ok) return
        const content = (response.result as { content: string }).content
        if (content !== '') term.write(content)
      })

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

    const onKeydown = (event: KeyboardEvent): void => {
      if (event.metaKey && event.key === 'f') {
        event.preventDefault()
        searchOpen = true
      }
    }
    window.addEventListener('resize', sendResize)
    window.addEventListener('keydown', onKeydown)
    const observer = new ResizeObserver(() => sendResize())
    observer.observe(element)

    window.__tillerPaneBuffers ??= new Map()
    window.__tillerPaneBuffers.set(paneId, () => readBuffer(term))
    window.__tillerPaneVisibility ??= new Map()
    window.__tillerPaneVisibility.set(paneId, setVisible)

    setVisible(visible)
    sendResize()

    return () => {
      observer.disconnect()
      window.removeEventListener('resize', sendResize)
      window.removeEventListener('keydown', onKeydown)
      window.__tillerPaneBuffers?.delete(paneId)
      window.__tillerPaneVisibility?.delete(paneId)
      unsubscribe()
      webgl?.dispose()
      searchAddon = null
      term.dispose()
    }
  }

  // Svelte richiama l'effetto quando `visible` cambia; l'attachment ha già
  // registrato come applicarlo a questo pane.
  $effect(() => {
    window.__tillerPaneVisibility?.get(paneId)?.(visible)
  })
</script>

<div class="terminal" class:hidden={!visible} {@attach attachTerminal}>
  {#if searchOpen && searchAddon !== null}
    <TerminalSearch search={searchAddon} onClose={() => (searchOpen = false)} />
  {/if}
</div>

<style>
  .terminal {
    position: relative;
    width: 100%;
    height: 100%;
  }

  /* display:none fa saltare al browser layout e paint per intero, cosa che
     opacity:0 non fa — è l'errore che nell'app Swift costava 497 MB. */
  .hidden {
    display: none;
  }
</style>
```

- [ ] **Passo 3: dichiara i seam nei tipi**

In `src/renderer/src/env.d.ts` (o dove è già dichiarato
`__tillerPaneBuffers`), aggiungi accanto:

```ts
    __tillerPaneVisibility?: Map<string, (visible: boolean) => void>
```

- [ ] **Passo 4: verifica la compilazione**

```bash
pnpm svelte-check
```

Atteso: nessun errore.

- [ ] **Passo 5: verifica il comportamento a mano**

```bash
pnpm dev
```

Controlla: il terminale si vede con il font e i colori attesi, scrivere
funziona, `⌘F` apre la barra di ricerca, ridimensionare la finestra non fa
sfarfallare il contenuto, un URL nell'output è cliccabile.

- [ ] **Passo 6: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add src/renderer/
git commit -m "feat: pane con tema, ricerca, link e gestione della visibilita"
```

---

### Task 13: i quattro criteri end-to-end

**File:**
- Crea: `e2e/fase-2-terminale.spec.ts`

**Interfacce:**
- Consuma: tutto quanto sopra, attraverso `tillerctl` e la finestra.

**Trappola nota dalla Fase 1:** Playwright avvia l'app da `out/`, non da
`src/`. Mutare un sorgente senza ricompilare dà un falso verde. Prima di ogni
validazione per mutazione esegui `pnpm exec electron-vite build`.

- [ ] **Passo 1: scrivi le e2e**

Crea `e2e/fase-2-terminale.spec.ts`:

```ts
import { test, expect, _electron as electron, type ElectronApplication } from '@playwright/test'
import { execFile, execFileSync } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const run = promisify(execFile)

let app: ElectronApplication
let scratch: string
let socketPath: string
let dbPath: string

async function launch(): Promise<void> {
  app = await electron.launch({
    args: ['.'],
    env: { ...process.env, TILLER_SOCKET: socketPath, TILLER_DB: dbPath }
  })
  await app.firstWindow()
}

test.beforeEach(async () => {
  scratch = await mkdtemp(join(tmpdir(), 'tiller-f2-'))
  socketPath = join(scratch, 'control.sock')
  dbPath = join(scratch, 'tiller.sqlite')
  await launch()
})

test.afterEach(async () => {
  await app.close().catch(() => {})
  await rm(scratch, { recursive: true, force: true })
})

async function tillerctl(args: string[]): Promise<string> {
  const { stdout } = await run('node', ['cli/tillerctl.ts', ...args], {
    env: { ...process.env, TILLER_SOCKET: socketPath, TILLER_DB: dbPath }
  })
  return stdout.trim()
}

/** Crea un repository git vero, lo registra e ne ricava un worktree. */
async function worktreeReale(): Promise<{ worktreeId: string; path: string }> {
  const dir = join(scratch, `repo-${Date.now()}`)
  execFileSync('mkdir', ['-p', dir])
  const git = (...args: string[]): void => {
    execFileSync('git', args, { cwd: dir, stdio: 'pipe' })
  }
  git('init', '--initial-branch=main')
  git('config', 'user.email', 'test@example.com')
  git('config', 'user.name', 'Test')
  execFileSync('sh', ['-c', 'echo x > f.txt'], { cwd: dir })
  git('add', 'f.txt')
  git('commit', '-m', 'primo')

  const added = JSON.parse(await tillerctl(['project-add', '--path', dir]))
  await tillerctl(['worktree-create', '--project', added.projectId, '--branch', 'fase2'])
  const listed = JSON.parse(await tillerctl(['worktree-list']))
  const worktree = listed.worktrees.find((w: { branch: string }) => w.branch === 'fase2')
  return { worktreeId: worktree.id, path: worktree.path }
}

/** `run` stampa il solo paneId, senza JSON attorno. */
async function creaPane(extra: string[] = []): Promise<string> {
  return tillerctl(['run', '--cmd', 'true', '--cwd', scratch, ...extra])
}

async function attendi(condizione: () => Promise<boolean>, timeoutMs = 10_000): Promise<void> {
  const scadenza = Date.now() + timeoutMs
  while (Date.now() < scadenza) {
    if (await condizione()) return
    await new Promise((resolve) => setTimeout(resolve, 150))
  }
  throw new Error('condizione non soddisfatta entro il timeout')
}

test('criterio 1: lo stato del terminale vive nel main', async () => {
  // Arrange
  const paneId = await creaPane()

  // Act — scrive nel pane, aspetta l'output, poi chiude la finestra.
  await tillerctl(['send', '--pane', paneId, '--data', 'echo impronta-unica\n'])
  await attendi(async () =>
    (await tillerctl(['read', '--pane', paneId])).includes('impronta-unica')
  )
  for (const window of app.windows()) await window.close()

  // Assert — a finestra chiusa il contenuto è ancora leggibile: se lo stato
  // vivesse nel renderer, qui sarebbe sparito insieme alla finestra.
  expect(await tillerctl(['read', '--pane', paneId])).toContain('impronta-unica')
})

test('criterio 2: lo scrollback sopravvive al riavvio', async () => {
  // Arrange
  const { worktreeId, path } = await worktreeReale()
  const paneId = await tillerctl([
    'run',
    '--cmd',
    'true',
    '--cwd',
    path,
    '--worktree',
    worktreeId
  ])
  await tillerctl(['send', '--pane', paneId, '--data', 'echo sopravvissuto\n'])
  await attendi(async () =>
    (await tillerctl(['read', '--pane', paneId])).includes('sopravvissuto')
  )

  // Act — chiudere il pane forza il salvataggio; poi l'app si riavvia.
  await tillerctl(['close', '--pane', paneId])
  await app.close()
  await launch()

  // Assert — ricreato con lo stesso id, ritrova la propria cronologia.
  const ricreato = await tillerctl([
    'run',
    '--cmd',
    'true',
    '--cwd',
    path,
    '--worktree',
    worktreeId,
    '--pane-id',
    paneId
  ])
  expect(ricreato).toBe(paneId)
  expect(await tillerctl(['read', '--pane', paneId])).toContain('sopravvissuto')
})

test('criterio 3: il titolo OSC arriva nello stato del main', async () => {
  // Arrange
  const paneId = await creaPane()

  // Act — \033]0;…\007 in ottale è il formato che Claude scrive davvero;
  // \342\234\263 è ✳ in UTF-8.
  await tillerctl([
    'send',
    '--pane',
    paneId,
    '--data',
    "printf '\\033]0;\\342\\234\\263 lavoro\\007'\n"
  ])

  // Assert — è la fondazione del Layer B della Fase 3.
  await attendi(async () => {
    const state = JSON.parse(await tillerctl(['state']))
    const pane = state.panes.find((p: { id: string }) => p.id === paneId)
    return pane?.title?.includes('lavoro') === true
  })
})

test('criterio 4: venti pane, nessuno che sbianca', async () => {
  // Arrange — misura di partenza prima di creare qualsiasi pane.
  const memoriaMB = async (): Promise<number> => {
    const metrics = await app.evaluate(({ app: electronApp }) => electronApp.getAppMetrics())
    return metrics.reduce((sum, entry) => sum + entry.memory.workingSetSize, 0) / 1024
  }
  const partenza = await memoriaMB()

  const paneIds: string[] = []
  for (let i = 0; i < 20; i++) {
    const paneId = await creaPane()
    paneIds.push(paneId)
    await tillerctl(['send', '--pane', paneId, '--data', `echo marcatore-${i}\n`])
  }

  // Act & Assert — percorrendoli tutti, ognuno mostra il proprio contenuto.
  // È il modo in cui si manifesta l'eviction dei contesti WebGL: il pane più
  // vecchio perderebbe il proprio, e con esso il contenuto.
  for (let i = 0; i < paneIds.length; i++) {
    await attendi(async () =>
      (await tillerctl(['read', '--pane', paneIds[i]])).includes(`marcatore-${i}`)
    )
  }

  // Venti pane non devono costare quanto costavano gli IOSurface in Swift.
  // Sforare questa soglia è il segnale per smontare del tutto i pane
  // nascosti (alternativa già progettata nella spec, decisione D5).
  expect((await memoriaMB()) - partenza).toBeLessThan(100)
})
```

- [ ] **Passo 2: costruisci ed esegui le e2e**

```bash
pnpm exec electron-vite build && pnpm exec playwright test e2e/fase-2-terminale.spec.ts
```

Atteso: 4 test verdi.

- [ ] **Passo 3: valida per mutazione il criterio 1**

In `src/main/control/dispatch.ts`, dentro `case 'pane.read'`, sostituisci
`terminal.readText(request.params.lines)` con `''`. Poi:

```bash
pnpm exec electron-vite build && pnpm exec playwright test e2e/fase-2-terminale.spec.ts
```

Atteso: il criterio 1 diventa ROSSO. Ripristina.

- [ ] **Passo 4: valida per mutazione il criterio 3**

In `src/main/control/dispatch.ts`, commenta il blocco
`deps.terminals.onTitle(...)`. Poi:

```bash
pnpm exec electron-vite build && pnpm exec playwright test e2e/fase-2-terminale.spec.ts
```

Atteso: il criterio 3 diventa ROSSO. Ripristina.

- [ ] **Passo 5: esegui il gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add e2e/
git commit -m "test: criteri end-to-end della fase due"
```

---

## Chiusura

- [ ] **Copia spec e piano nel repo che li implementa**

```bash
cp ~/Desktop/Progetti/tiller/.claude/worktrees/commit-push-8ecc39/docs/superpowers/specs/2026-08-01-electron-fase-2-motore-terminale-design.md ~/Desktop/Progetti/tiller-electron/docs/fase-2-design.md
cp ~/Desktop/Progetti/tiller/.claude/worktrees/commit-push-8ecc39/docs/superpowers/plans/2026-08-01-electron-fase-2-motore-terminale.md ~/Desktop/Progetti/tiller-electron/docs/fase-2-piano.md
cd ~/Desktop/Progetti/tiller-electron && git add docs/ && git commit -m "docs: spec e piano della fase due"
```

- [ ] **Verifica che il repo Swift non sia stato toccato**

```bash
cd ~/Desktop/Progetti/tiller && git status --porcelain
```

Atteso: nessun file sorgente modificato.
