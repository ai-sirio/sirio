# Fase 1 — Fondamenta del main: piano di implementazione

> **Per esecutori agentici:** SOTTO-SKILL RICHIESTA: usare
> superpowers:subagent-driven-development (consigliata) o
> superpowers:executing-plans per implementare questo piano un task alla volta.
> Gli step usano checkbox (`- [ ]`) per il tracciamento.

**Obiettivo:** costruire il processo main completo — persistenza, git, adapter
agenti — pilotabile interamente da `tillerctl`, senza una riga di interfaccia.

**Architettura:** `better-sqlite3` come driver e `kysely` come query builder e
migratore; `simple-git` per i comandi git, con il parsing dei worktree scritto
da noi perché nessuna libreria lo copre; cinque adapter agenti che producono
righe di comando. Tutto è consumato dal dispatcher già esistente
(`src/main/control/dispatch.ts`), che resta l'unico punto di ingresso e non
importa né `electron` né `net`.

**Stack:** TypeScript 5.9, Electron 43.2 (Node 24.18 interno), better-sqlite3
13, kysely 0.29, simple-git 3.36, smol-toml 1.7 (solo nei test), Vitest 4,
Playwright.

**Spec:** `docs/superpowers/specs/2026-08-01-electron-fase-1-fondamenta-main-design.md`

## Vincoli globali

- Repo di lavoro: `~/Desktop/Progetti/tiller-electron/`. **Il repo Swift
  `~/Desktop/Progetti/tiller/` non va mai modificato**, solo letto.
- `src/main/control/dispatch.ts` non deve importare `electron` né `net`. È il
  confine "un protocollo, due trasporti" e va verificato dai test esistenti.
- Gate unico: `bash scripts/ci.sh` deve stampare `CI OK`. Include typecheck
  (`tsconfig.node.json`, `tsconfig.cli.json`, `svelte-check`), lint, unit test,
  build ed e2e.
- Test con `vitest` (`expect`, `test`), file `*.test.ts` accanto al codice.
  Nomi dei test in italiano, come i già presenti.
- `@types/node` resta a `^24`: è il pavimento, cioè il Node interno a Electron.
  Non allinearlo al Node di sistema.
- Node 24 richiede l'estensione `.ts` esplicita sugli import locali dentro
  `cli/`, che gira senza bundler.
- Messaggi di commit in Conventional Commits, soggetto minuscolo all'imperativo.
- Gli errori del dispatcher non si lanciano mai fuori: diventano
  `{ ok: false, error }`.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/shared/paths.ts` | *(modificato)* aggiunge `resolveDatabasePath()` |
| `src/shared/protocol.ts` | *(modificato)* nuovi metodi nella union `ControlRequest` |
| `src/main/db/schema.ts` | tipi TypeScript delle 11 tabelle per kysely |
| `src/main/db/database.ts` | apertura, WAL, foreign key, migratore |
| `src/main/db/migrations/001-initial.ts` | creazione delle 11 tabelle |
| `src/main/db/repository.ts` | query su progetti e worktree |
| `src/main/import/swift-import.ts` | travaso una tantum dal database Swift |
| `src/main/git/runner.ts` | istanza `simple-git` configurata |
| `src/main/git/worktrees.ts` | parsing `--porcelain`, add, remove |
| `src/main/git/repo.ts` | rilevamento repository, root, branch corrente |
| `src/main/git/status.ts` | `.status()` → modello di dominio |
| `src/main/git/actions.ts` | stage, unstage, discard, con validazione |
| `src/main/agents/quote.ts` | `shellQuote`, `tomlStringLiteral` |
| `src/main/agents/catalog.ts` | interfaccia `AgentAdapter`, elenco dei cinque |
| `src/main/agents/adapters/*.ts` | un file per agente |
| `src/main/control/dispatch.ts` | *(modificato)* nuovi metodi, diventa async |
| `cli/tillerctl.ts` | *(modificato)* nuovi comandi |

### Nota di raffinamento rispetto alla spec

La spec prevede di estendere `src/main/state/` con progetti e worktree. In fase
di piano questa scelta viene realizzata diversamente: **progetti e worktree non
vengono duplicati in memoria**, il dispatcher li legge dal database a ogni
richiesta tramite `repository.ts`.

Motivo: `AppState` esiste per i pane vivi, che sono effimeri e non persistiti.
Progetti e worktree sono persistiti, quindi il database *è già* la loro sorgente
di verità — tenerne una copia in RAM introdurrebbe due verità da riconciliare
senza che nulla, in Fase 1, ne tragga vantaggio. Gli eventi `project.added` per
il renderer serviranno in Fase 4, quando esisterà una UI che deve aggiornarsi da
sola; aggiungerli ora sarebbe codice senza lettori.

---

### Task 1: driver, query builder e apertura del database

**File:**
- Modifica: `package.json` (dipendenze), `pnpm-workspace.yaml`
- Modifica: `src/shared/paths.ts`
- Crea: `src/main/db/schema.ts`
- Crea: `src/main/db/database.ts`
- Test: `src/main/db/database.test.ts`, `src/main/db/better-sqlite3-packaging.test.ts`

**Interfacce:**
- Consuma: `resolveSocketPath()` da `src/shared/paths.ts` (pattern da imitare).
- Produce: `resolveDatabasePath(): string`;
  `openDatabase(path: string): Kysely<TillerDatabase>`;
  `closeDatabase(db: Kysely<TillerDatabase>): Promise<void>`;
  l'interfaccia `TillerDatabase` con le 11 tabelle.

- [ ] **Step 1: installare le dipendenze**

```bash
cd ~/Desktop/Progetti/tiller-electron
pnpm add better-sqlite3 kysely simple-git
pnpm add -D @types/better-sqlite3 smol-toml
```

`pnpm` chiederà di approvare gli script di build di `better-sqlite3`. Aggiungere
la voce in `pnpm-workspace.yaml`, nella lista già presente accanto a
`node-pty: true`:

```yaml
  better-sqlite3: true
```

Poi rieseguire `pnpm install`.

- [ ] **Step 2: scrivere il test di guardia sul packaging**

`better-sqlite3` 13 usa N-API con prebuild per piattaforma: nessun rebuild per
Electron. Il rischio non è la compilazione, è che il binario non finisca nel
pacchetto — la stessa classe di bug di `spawn-helper` in Fase 0.

Creare `src/main/db/better-sqlite3-packaging.test.ts`:

```ts
import { expect, test } from 'vitest'
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { dirname, join } from 'node:path'

const require = createRequire(import.meta.url)

/**
 * Guardia di packaging, non test di logica.
 *
 * better-sqlite3 carica un binario nativo prebuilt. Se il pacchetto viene
 * installato senza, o il binario non finisce nell'app impacchettata, il
 * fallimento arriva a runtime con un messaggio che non nomina la causa.
 * Questo test lo trasforma in un fallimento di CI.
 */
test('il binario nativo di better-sqlite3 esiste', () => {
  const packageJson = require.resolve('better-sqlite3/package.json')
  const root = dirname(packageJson)
  const candidates = [
    join(root, 'build', 'Release', 'better_sqlite3.node'),
    join(root, 'prebuilds', `${process.platform}-${process.arch}.node`)
  ]
  expect(candidates.some((p) => existsSync(p))).toBe(true)
})

test('better-sqlite3 apre un database in memoria', () => {
  const Database = require('better-sqlite3')
  const db = new Database(':memory:')
  db.exec('CREATE TABLE t(a TEXT)')
  db.prepare('INSERT INTO t VALUES(?)').run('x')
  expect(db.prepare('SELECT a FROM t').get()).toEqual({ a: 'x' })
  db.close()
})
```

- [ ] **Step 3: eseguire il test e verificare che fallisca**

Comando: `pnpm test:unit -- src/main/db/better-sqlite3-packaging.test.ts`
Atteso: FAIL, `Cannot find module 'better-sqlite3'` se lo step 1 non è stato
completato. Se passa già, lo step 1 è a posto e si prosegue.

- [ ] **Step 4: scrivere il test di apertura del database**

Creare `src/main/db/database.test.ts`:

```ts
import { expect, test } from 'vitest'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { sql } from 'kysely'
import { openDatabase, closeDatabase } from './database'
import { resolveDatabasePath } from '../../shared/paths'

function tempDbPath(): string {
  return join(mkdtempSync(join(tmpdir(), 'tiller-db-')), 'tiller.sqlite')
}

test('il database si apre in modalita WAL', async () => {
  const path = tempDbPath()
  const db = openDatabase(path)
  const result = await sql<{ journal_mode: string }>`PRAGMA journal_mode`.execute(db)
  expect(result.rows[0]?.journal_mode).toBe('wal')
  await closeDatabase(db)
  rmSync(path, { force: true })
})

test('le foreign key sono attive', async () => {
  const path = tempDbPath()
  const db = openDatabase(path)
  const result = await sql<{ foreign_keys: number }>`PRAGMA foreign_keys`.execute(db)
  expect(result.rows[0]?.foreign_keys).toBe(1)
  await closeDatabase(db)
  rmSync(path, { force: true })
})

test('TILLER_DB sovrascrive il percorso del database', () => {
  const previous = process.env.TILLER_DB
  process.env.TILLER_DB = '/tmp/custom.sqlite'
  expect(resolveDatabasePath()).toBe('/tmp/custom.sqlite')
  if (previous === undefined) delete process.env.TILLER_DB
  else process.env.TILLER_DB = previous
})

test('senza override il database sta accanto al socket di controllo', () => {
  const previous = process.env.TILLER_DB
  delete process.env.TILLER_DB
  expect(resolveDatabasePath()).toMatch(/tiller-electron\/tiller\.sqlite$/)
  if (previous !== undefined) process.env.TILLER_DB = previous
})
```

- [ ] **Step 5: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/db/database.test.ts`
Atteso: FAIL, `Cannot find module './database'`.

- [ ] **Step 6: aggiungere `resolveDatabasePath` a `src/shared/paths.ts`**

Aggiungere in fondo al file, senza toccare `resolveSocketPath`:

```ts
/**
 * Path del database, con la stessa logica del socket: nessun uso di
 * `app.getPath`, così tillerctl lo calcola senza importare `electron`.
 * Override con $TILLER_DB (usato dai test per isolarsi).
 */
export function resolveDatabasePath(): string {
  const override = process.env.TILLER_DB
  if (override && override.length > 0) return override

  if (process.platform === 'darwin') {
    return join(homedir(), 'Library', 'Application Support', APP_DIR_NAME, 'tiller.sqlite')
  }
  return join(homedir(), '.local', 'state', APP_DIR_NAME, 'tiller.sqlite')
}
```

- [ ] **Step 7: scrivere `src/main/db/schema.ts`**

```ts
/**
 * Forma delle tabelle per kysely.
 *
 * Rispecchia lo schema finale estratto dal database di produzione dell'app
 * Swift (17 migrazioni GRDB già collassate). I DATETIME di GRDB sono stringhe
 * ISO e restano tali: nessuna conversione, nessuna perdita di fuso orario.
 */
export interface ProjectTable {
  id: string
  name: string
  rootPath: string
  createdAt: string
  colorHex: string | null
  displayName: string | null
  iconKind: string
  iconValue: string | null
  avatarImage: Uint8Array | null
  defaultWorktreeBase: string | null
  worktreeLocationOverride: string | null
  orderIdx: number
}

export interface WorktreeTable {
  id: string
  projectId: string
  branch: string
  path: string
  createdAt: string
  comment: string | null
  commentUpdatedAt: string | null
  isPrimary: number
  orderIdx: number
}

export interface TerminalContentTable {
  id: string
  worktreeId: string
  launchKind: string
  agentId: string | null
  commandJSON: string | null
  createdAt: string
}

export interface PaneScrollbackTable {
  terminalContentId: string
  worktreeId: string
  data: Uint8Array
  updatedAt: string
}

export interface AgentAccountTable {
  id: string
  provider: string
  configDirPath: string
  label: string
  orgName: string | null
  createdAt: string
  lastAuthenticatedAt: string
}

export interface AgentSessionTable {
  terminalContentId: string
  worktreeId: string
  agentId: string
  sessionRef: string
  capturedAt: string
}

export interface ChatSessionTable {
  id: string
  worktreeId: string
  agentId: string
  acpSessionId: string | null
  createdAt: string
  lastActivityAt: string
  contextUsageUsed: number | null
  contextUsageSize: number | null
  permissionMode: string | null
  selectedModel: string | null
  selectedEffort: string | null
  transportKind: string
  title: string | null
}

export interface ChatItemTable {
  sessionId: string
  ordinal: number
  kind: string
  payload: Uint8Array
}

export interface WorkspaceLayoutTable {
  worktreeId: string
  schemaVersion: number
  revision: number
  payload: string
  checksum: string
  updatedAt: string
}

export interface WorkspaceTabTable {
  id: string
  worktreeId: string
  title: string
  titleIsAutoNamed: number
  contentKind: string
  contentId: string
  viewStateJSON: string | null
  viewStateVersion: number
  createdAt: string
}

export interface WorkspaceLayoutQuarantineTable {
  id: string
  worktreeId: string
  payload: string
  reason: string
  createdAt: string
}

export interface TillerDatabase {
  project: ProjectTable
  worktree: WorktreeTable
  terminalContent: TerminalContentTable
  paneScrollback: PaneScrollbackTable
  agentAccount: AgentAccountTable
  agentSession: AgentSessionTable
  chatSession: ChatSessionTable
  chatItem: ChatItemTable
  workspaceLayout: WorkspaceLayoutTable
  workspaceTab: WorkspaceTabTable
  workspaceLayoutQuarantine: WorkspaceLayoutQuarantineTable
}

/** Ordine di inserimento che rispetta le foreign key. */
export const TABLES_IN_DEPENDENCY_ORDER = [
  'project',
  'worktree',
  'terminalContent',
  'paneScrollback',
  'agentAccount',
  'agentSession',
  'chatSession',
  'chatItem',
  'workspaceLayout',
  'workspaceTab',
  'workspaceLayoutQuarantine'
] as const satisfies readonly (keyof TillerDatabase)[]
```

- [ ] **Step 8: scrivere `src/main/db/database.ts`**

```ts
import BetterSqlite3 from 'better-sqlite3'
import { mkdirSync } from 'node:fs'
import { dirname } from 'node:path'
import { Kysely, SqliteDialect } from 'kysely'
import type { TillerDatabase } from './schema'

/**
 * Apre il database applicando i pragma che devono valere per ogni connessione.
 *
 * WAL: lettori e scrittore non si bloccano a vicenda — serve perché il socket
 * di controllo e il renderer leggono mentre l'app scrive.
 * foreign_keys: SQLite le lascia disattive per default, e senza di esse i
 * vincoli ON DELETE CASCADE dichiarati nello schema non farebbero nulla.
 */
export function openDatabase(path: string): Kysely<TillerDatabase> {
  if (path !== ':memory:') mkdirSync(dirname(path), { recursive: true })
  const sqlite = new BetterSqlite3(path)
  sqlite.pragma('journal_mode = WAL')
  sqlite.pragma('foreign_keys = ON')
  return new Kysely<TillerDatabase>({ dialect: new SqliteDialect({ database: sqlite }) })
}

export async function closeDatabase(db: Kysely<TillerDatabase>): Promise<void> {
  await db.destroy()
}
```

- [ ] **Step 9: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/db/`
Atteso: PASS, 6 test.

- [ ] **Step 10: commit**

```bash
git add package.json pnpm-lock.yaml pnpm-workspace.yaml src/shared/paths.ts src/main/db/
git commit -m "feat: apertura database con better-sqlite3 e kysely"
```

---

### Task 2: migrazione iniziale delle undici tabelle

**File:**
- Crea: `src/main/db/migrations/001-initial.ts`
- Crea: `src/main/db/migrations/index.ts`
- Modifica: `src/main/db/database.ts`
- Test: `src/main/db/migrations/migrations.test.ts`

**Interfacce:**
- Consuma: `openDatabase(path)`, `TillerDatabase`, `TABLES_IN_DEPENDENCY_ORDER`.
- Produce: `migrateToLatest(db: Kysely<TillerDatabase>): Promise<void>`;
  `MIGRATIONS: Record<string, Migration>`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/db/migrations/migrations.test.ts`:

```ts
import { expect, test } from 'vitest'
import { sql } from 'kysely'
import { openDatabase, closeDatabase, migrateToLatest } from '../database'
import { TABLES_IN_DEPENDENCY_ORDER } from '../schema'

async function migratedDb() {
  const db = openDatabase(':memory:')
  await migrateToLatest(db)
  return db
}

test('la migrazione crea tutte e undici le tabelle', async () => {
  const db = await migratedDb()
  const rows = await sql<{ name: string }>`
    SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'kysely%'
  `.execute(db)
  const names = rows.rows.map((r) => r.name).sort()
  expect(names).toEqual([...TABLES_IN_DEPENDENCY_ORDER].sort())
  await closeDatabase(db)
})

test('cancellare un worktree cancella in cascata cio che gli appartiene', async () => {
  const db = await migratedDb()
  await db.insertInto('project').values({
    id: 'p1', name: 'Tiller', rootPath: '/tmp/tiller', createdAt: '2026-01-01T00:00:00Z',
    colorHex: null, displayName: null, iconKind: 'icon', iconValue: null,
    avatarImage: null, defaultWorktreeBase: null, worktreeLocationOverride: null, orderIdx: 0
  }).execute()
  await db.insertInto('worktree').values({
    id: 'w1', projectId: 'p1', branch: 'main', path: '/tmp/tiller',
    createdAt: '2026-01-01T00:00:00Z', comment: null, commentUpdatedAt: null,
    isPrimary: 1, orderIdx: 0
  }).execute()
  await db.insertInto('terminalContent').values({
    id: 't1', worktreeId: 'w1', launchKind: 'shell', agentId: null,
    commandJSON: null, createdAt: '2026-01-01T00:00:00Z'
  }).execute()

  await db.deleteFrom('worktree').where('id', '=', 'w1').execute()

  const left = await db.selectFrom('terminalContent').selectAll().execute()
  expect(left).toEqual([])
  await closeDatabase(db)
})

test('un worktree senza progetto viene rifiutato', async () => {
  const db = await migratedDb()
  await expect(
    db.insertInto('worktree').values({
      id: 'w1', projectId: 'inesistente', branch: 'main', path: '/tmp/x',
      createdAt: '2026-01-01T00:00:00Z', comment: null, commentUpdatedAt: null,
      isPrimary: 0, orderIdx: 0
    }).execute()
  ).rejects.toThrow(/FOREIGN KEY/)
  await closeDatabase(db)
})

test('migrare due volte non fallisce', async () => {
  const db = await migratedDb()
  await expect(migrateToLatest(db)).resolves.toBeUndefined()
  await closeDatabase(db)
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/db/migrations/`
Atteso: FAIL, `migrateToLatest is not exported`.

- [ ] **Step 3: scrivere `src/main/db/migrations/001-initial.ts`**

```ts
import { type Kysely, sql } from 'kysely'

/**
 * Crea lo schema nella sua forma finale.
 *
 * Le 17 migrazioni GRDB dell'app Swift non vengono replicate: la loro storia
 * (fra cui il re-keying paneId → terminalContentId di v13/v15/v16) è già
 * assorbita in questo risultato. Le migrazioni Electron ripartono da 1.
 */
export const migration001Initial = {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async up(db: Kysely<any>): Promise<void> {
    await db.schema
      .createTable('project')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('name', 'text', (c) => c.notNull())
      .addColumn('rootPath', 'text', (c) => c.notNull())
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .addColumn('colorHex', 'text')
      .addColumn('displayName', 'text')
      .addColumn('iconKind', 'text', (c) => c.notNull().defaultTo('icon'))
      .addColumn('iconValue', 'text')
      .addColumn('avatarImage', 'blob')
      .addColumn('defaultWorktreeBase', 'text')
      .addColumn('worktreeLocationOverride', 'text')
      .addColumn('orderIdx', 'integer', (c) => c.notNull().defaultTo(0))
      .execute()

    await db.schema
      .createTable('worktree')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('projectId', 'text', (c) =>
        c.notNull().references('project.id').onDelete('cascade')
      )
      .addColumn('branch', 'text', (c) => c.notNull())
      .addColumn('path', 'text', (c) => c.notNull())
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .addColumn('comment', 'text')
      .addColumn('commentUpdatedAt', 'text')
      .addColumn('isPrimary', 'integer', (c) => c.notNull().defaultTo(0))
      .addColumn('orderIdx', 'integer', (c) => c.notNull().defaultTo(0))
      .execute()

    await db.schema
      .createTable('terminalContent')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('launchKind', 'text', (c) => c.notNull())
      .addColumn('agentId', 'text')
      .addColumn('commandJSON', 'text')
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .execute()

    await db.schema
      .createTable('paneScrollback')
      .addColumn('terminalContentId', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('data', 'blob', (c) => c.notNull())
      .addColumn('updatedAt', 'text', (c) => c.notNull())
      .execute()

    await db.schema
      .createTable('agentAccount')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('provider', 'text', (c) => c.notNull())
      .addColumn('configDirPath', 'text', (c) => c.notNull())
      .addColumn('label', 'text', (c) => c.notNull())
      .addColumn('orgName', 'text')
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .addColumn('lastAuthenticatedAt', 'text', (c) => c.notNull())
      .execute()

    await db.schema
      .createTable('agentSession')
      .addColumn('terminalContentId', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('agentId', 'text', (c) => c.notNull())
      .addColumn('sessionRef', 'text', (c) => c.notNull())
      .addColumn('capturedAt', 'text', (c) => c.notNull())
      .execute()

    await db.schema
      .createTable('chatSession')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('agentId', 'text', (c) => c.notNull())
      .addColumn('acpSessionId', 'text')
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .addColumn('lastActivityAt', 'text', (c) => c.notNull())
      .addColumn('contextUsageUsed', 'integer')
      .addColumn('contextUsageSize', 'integer')
      .addColumn('permissionMode', 'text')
      .addColumn('selectedModel', 'text')
      .addColumn('selectedEffort', 'text')
      .addColumn('transportKind', 'text', (c) => c.notNull().defaultTo('acp'))
      .addColumn('title', 'text')
      .execute()

    await db.schema
      .createTable('chatItem')
      .addColumn('sessionId', 'text', (c) =>
        c.notNull().references('chatSession.id').onDelete('cascade')
      )
      .addColumn('ordinal', 'integer', (c) => c.notNull())
      .addColumn('kind', 'text', (c) => c.notNull())
      .addColumn('payload', 'blob', (c) => c.notNull())
      .addPrimaryKeyConstraint('chatItem_pk', ['sessionId', 'ordinal'])
      .execute()

    await db.schema
      .createTable('workspaceLayout')
      .addColumn('worktreeId', 'text', (c) =>
        c.primaryKey().notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('schemaVersion', 'integer', (c) => c.notNull())
      .addColumn('revision', 'integer', (c) => c.notNull())
      .addColumn('payload', 'text', (c) => c.notNull())
      .addColumn('checksum', 'text', (c) => c.notNull())
      .addColumn('updatedAt', 'text', (c) => c.notNull())
      .execute()

    await db.schema
      .createTable('workspaceTab')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('title', 'text', (c) => c.notNull())
      .addColumn('titleIsAutoNamed', 'integer', (c) => c.notNull())
      .addColumn('contentKind', 'text', (c) => c.notNull())
      .addColumn('contentId', 'text', (c) => c.notNull())
      .addColumn('viewStateJSON', 'text')
      .addColumn('viewStateVersion', 'integer', (c) => c.notNull())
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .addUniqueConstraint('workspaceTab_unique', ['worktreeId', 'contentKind', 'contentId'])
      .execute()

    await db.schema
      .createTable('workspaceLayoutQuarantine')
      .addColumn('id', 'text', (c) => c.primaryKey().notNull())
      .addColumn('worktreeId', 'text', (c) =>
        c.notNull().references('worktree.id').onDelete('cascade')
      )
      .addColumn('payload', 'text', (c) => c.notNull())
      .addColumn('reason', 'text', (c) => c.notNull())
      .addColumn('createdAt', 'text', (c) => c.notNull())
      .execute()

    await sql`PRAGMA foreign_keys = ON`.execute(db)
  },

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async down(db: Kysely<any>): Promise<void> {
    for (const table of [
      'workspaceLayoutQuarantine', 'workspaceTab', 'workspaceLayout', 'chatItem',
      'chatSession', 'agentSession', 'agentAccount', 'paneScrollback',
      'terminalContent', 'worktree', 'project'
    ]) {
      await db.schema.dropTable(table).execute()
    }
  }
}
```

- [ ] **Step 4: scrivere `src/main/db/migrations/index.ts`**

```ts
import type { Migration } from 'kysely/migration'
import { migration001Initial } from './001-initial'

/**
 * Provider statico, non FileMigrationProvider.
 *
 * kysely offre FileMigrationProvider, che elenca i file di una cartella a
 * runtime. In un'app Electron impacchettata quella cartella non esiste: il
 * main viene messo in bundle da electron-vite in un file solo. Un elenco
 * esplicito è l'unica forma che sopravvive al bundling, e costa una riga per
 * migrazione.
 */
export const MIGRATIONS: Record<string, Migration> = {
  '001-initial': migration001Initial
}
```

- [ ] **Step 5: aggiungere `migrateToLatest` a `src/main/db/database.ts`**

Aggiungere gli import in cima e la funzione in fondo:

```ts
import { Kysely, SqliteDialect } from 'kysely'
import { Migrator } from 'kysely/migration'
import { MIGRATIONS } from './migrations'

/**
 * Applica le migrazioni mancanti. Idempotente: kysely tiene traccia di quelle
 * gia' eseguite nella propria tabella e salta il resto.
 */
export async function migrateToLatest(db: Kysely<TillerDatabase>): Promise<void> {
  const migrator = new Migrator({
    db,
    provider: { getMigrations: async () => MIGRATIONS }
  })
  const { error } = await migrator.migrateToLatest()
  if (error) throw error instanceof Error ? error : new Error(String(error))
}
```

- [ ] **Step 6: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/db/`
Atteso: PASS, 10 test.

- [ ] **Step 7: commit**

```bash
git add src/main/db/
git commit -m "feat: migrazione iniziale con le undici tabelle"
```

---

### Task 3: repository di progetti e worktree

**File:**
- Crea: `src/main/db/repository.ts`
- Test: `src/main/db/repository.test.ts`

**Interfacce:**
- Consuma: `openDatabase`, `migrateToLatest`, `TillerDatabase`.
- Produce:
  `addProject(db, input: { id: string; name: string; rootPath: string; createdAt: string }): Promise<void>`;
  `listProjects(db): Promise<ProjectSummary[]>`;
  `removeProject(db, projectId: string): Promise<boolean>`;
  `addWorktree(db, input: { id: string; projectId: string; branch: string; path: string; createdAt: string; isPrimary: boolean }): Promise<void>`;
  `listWorktrees(db, projectId?: string): Promise<WorktreeSummary[]>`;
  `removeWorktree(db, worktreeId: string): Promise<boolean>`;
  `findWorktree(db, worktreeId: string): Promise<WorktreeSummary | undefined>`.
  I tipi `ProjectSummary` e `WorktreeSummary`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/db/repository.test.ts`:

```ts
import { beforeEach, expect, test } from 'vitest'
import type { Kysely } from 'kysely'
import { openDatabase, closeDatabase, migrateToLatest } from './database'
import type { TillerDatabase } from './schema'
import {
  addProject, listProjects, removeProject,
  addWorktree, listWorktrees, removeWorktree, findWorktree
} from './repository'

let db: Kysely<TillerDatabase>

beforeEach(async () => {
  if (db) await closeDatabase(db)
  db = openDatabase(':memory:')
  await migrateToLatest(db)
})

const project = { id: 'p1', name: 'Tiller', rootPath: '/tmp/tiller', createdAt: '2026-01-01T00:00:00Z' }

test('un database vuoto non ha progetti', async () => {
  expect(await listProjects(db)).toEqual([])
})

test('un progetto aggiunto compare nella lista', async () => {
  await addProject(db, project)
  expect(await listProjects(db)).toEqual([
    { id: 'p1', name: 'Tiller', rootPath: '/tmp/tiller', orderIdx: 0 }
  ])
})

test('rimuovere un progetto rimuove i suoi worktree', async () => {
  await addProject(db, project)
  await addWorktree(db, {
    id: 'w1', projectId: 'p1', branch: 'main', path: '/tmp/tiller',
    createdAt: '2026-01-01T00:00:00Z', isPrimary: true
  })
  expect(await removeProject(db, 'p1')).toBe(true)
  expect(await listWorktrees(db)).toEqual([])
})

test('rimuovere un progetto inesistente restituisce false', async () => {
  expect(await removeProject(db, 'assente')).toBe(false)
})

test('i worktree si filtrano per progetto', async () => {
  await addProject(db, project)
  await addProject(db, { ...project, id: 'p2', name: 'Altro', rootPath: '/tmp/altro' })
  await addWorktree(db, {
    id: 'w1', projectId: 'p1', branch: 'main', path: '/tmp/tiller',
    createdAt: '2026-01-01T00:00:00Z', isPrimary: true
  })
  await addWorktree(db, {
    id: 'w2', projectId: 'p2', branch: 'main', path: '/tmp/altro',
    createdAt: '2026-01-01T00:00:00Z', isPrimary: true
  })
  const only = await listWorktrees(db, 'p1')
  expect(only.map((w) => w.id)).toEqual(['w1'])
})

test('isPrimary sopravvive al giro in database come booleano', async () => {
  await addProject(db, project)
  await addWorktree(db, {
    id: 'w1', projectId: 'p1', branch: 'main', path: '/tmp/tiller',
    createdAt: '2026-01-01T00:00:00Z', isPrimary: true
  })
  const found = await findWorktree(db, 'w1')
  expect(found?.isPrimary).toBe(true)
})

test('un worktree inesistente non si trova', async () => {
  expect(await findWorktree(db, 'assente')).toBeUndefined()
})

test('rimuovere un worktree restituisce true solo la prima volta', async () => {
  await addProject(db, project)
  await addWorktree(db, {
    id: 'w1', projectId: 'p1', branch: 'main', path: '/tmp/tiller',
    createdAt: '2026-01-01T00:00:00Z', isPrimary: false
  })
  expect(await removeWorktree(db, 'w1')).toBe(true)
  expect(await removeWorktree(db, 'w1')).toBe(false)
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/db/repository.test.ts`
Atteso: FAIL, `Cannot find module './repository'`.

- [ ] **Step 3: scrivere `src/main/db/repository.ts`**

```ts
import type { Kysely } from 'kysely'
import type { TillerDatabase } from './schema'

export interface ProjectSummary {
  id: string
  name: string
  rootPath: string
  orderIdx: number
}

export interface WorktreeSummary {
  id: string
  projectId: string
  branch: string
  path: string
  isPrimary: boolean
  orderIdx: number
}

export interface AddProjectInput {
  id: string
  name: string
  rootPath: string
  createdAt: string
}

export interface AddWorktreeInput {
  id: string
  projectId: string
  branch: string
  path: string
  createdAt: string
  isPrimary: boolean
}

export async function addProject(
  db: Kysely<TillerDatabase>,
  input: AddProjectInput
): Promise<void> {
  await db
    .insertInto('project')
    .values({
      id: input.id,
      name: input.name,
      rootPath: input.rootPath,
      createdAt: input.createdAt,
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
}

export async function listProjects(db: Kysely<TillerDatabase>): Promise<ProjectSummary[]> {
  return db
    .selectFrom('project')
    .select(['id', 'name', 'rootPath', 'orderIdx'])
    .orderBy('orderIdx')
    .execute()
}

export async function removeProject(
  db: Kysely<TillerDatabase>,
  projectId: string
): Promise<boolean> {
  const result = await db.deleteFrom('project').where('id', '=', projectId).executeTakeFirst()
  return Number(result.numDeletedRows ?? 0) > 0
}

export async function addWorktree(
  db: Kysely<TillerDatabase>,
  input: AddWorktreeInput
): Promise<void> {
  await db
    .insertInto('worktree')
    .values({
      id: input.id,
      projectId: input.projectId,
      branch: input.branch,
      path: input.path,
      createdAt: input.createdAt,
      comment: null,
      commentUpdatedAt: null,
      // SQLite non ha un tipo booleano: 0/1 dentro, boolean fuori.
      isPrimary: input.isPrimary ? 1 : 0,
      orderIdx: 0
    })
    .execute()
}

export async function listWorktrees(
  db: Kysely<TillerDatabase>,
  projectId?: string
): Promise<WorktreeSummary[]> {
  let query = db
    .selectFrom('worktree')
    .select(['id', 'projectId', 'branch', 'path', 'isPrimary', 'orderIdx'])
    .orderBy('orderIdx')
  if (projectId !== undefined) query = query.where('projectId', '=', projectId)
  const rows = await query.execute()
  return rows.map((row) => ({ ...row, isPrimary: row.isPrimary === 1 }))
}

export async function findWorktree(
  db: Kysely<TillerDatabase>,
  worktreeId: string
): Promise<WorktreeSummary | undefined> {
  const row = await db
    .selectFrom('worktree')
    .select(['id', 'projectId', 'branch', 'path', 'isPrimary', 'orderIdx'])
    .where('id', '=', worktreeId)
    .executeTakeFirst()
  return row === undefined ? undefined : { ...row, isPrimary: row.isPrimary === 1 }
}

export async function removeWorktree(
  db: Kysely<TillerDatabase>,
  worktreeId: string
): Promise<boolean> {
  const result = await db.deleteFrom('worktree').where('id', '=', worktreeId).executeTakeFirst()
  return Number(result.numDeletedRows ?? 0) > 0
}
```

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/db/repository.test.ts`
Atteso: PASS, 8 test.

- [ ] **Step 5: commit**

```bash
git add src/main/db/repository.ts src/main/db/repository.test.ts
git commit -m "feat: repository di progetti e worktree"
```

---

### Task 4: import dal database dell'app Swift

**File:**
- Crea: `src/main/import/swift-import.ts`
- Test: `src/main/import/swift-import.test.ts`

**Interfacce:**
- Consuma: `openDatabase`, `migrateToLatest`, `TABLES_IN_DEPENDENCY_ORDER`, `TillerDatabase`.
- Produce: `importFromSwift(db, sourcePath: string): Promise<ImportReport>`;
  `ImportReport = { counts: Record<string, { source: number; target: number }> }`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/import/swift-import.test.ts`:

```ts
import { beforeEach, expect, test } from 'vitest'
import BetterSqlite3 from 'better-sqlite3'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Kysely } from 'kysely'
import { openDatabase, closeDatabase, migrateToLatest } from '../db/database'
import type { TillerDatabase } from '../db/schema'
import { importFromSwift } from './swift-import'

let db: Kysely<TillerDatabase>

beforeEach(async () => {
  if (db) await closeDatabase(db)
  db = openDatabase(':memory:')
  await migrateToLatest(db)
})

/**
 * Costruisce una sorgente con lo schema Swift: stesse tabelle, stessi nomi di
 * colonna, tipi DATETIME come stringhe. Non si usa il database di produzione
 * nei test — contiene percorsi personali — ma la forma e' la sua.
 */
function makeSwiftDb(): string {
  const path = join(mkdtempSync(join(tmpdir(), 'tiller-swift-')), 'tiller.sqlite')
  const src = new BetterSqlite3(path)
  src.exec(`
    CREATE TABLE "project" ("id" TEXT PRIMARY KEY NOT NULL, "name" TEXT NOT NULL,
      "rootPath" TEXT NOT NULL, "createdAt" DATETIME NOT NULL, "colorHex" TEXT,
      "displayName" TEXT, "iconKind" TEXT NOT NULL DEFAULT 'icon', "iconValue" TEXT,
      "avatarImage" BLOB, "defaultWorktreeBase" TEXT, "worktreeLocationOverride" TEXT,
      "orderIdx" INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE "worktree" ("id" TEXT PRIMARY KEY NOT NULL, "projectId" TEXT NOT NULL,
      "branch" TEXT NOT NULL, "path" TEXT NOT NULL, "createdAt" DATETIME NOT NULL,
      "comment" TEXT, "commentUpdatedAt" DATETIME, "isPrimary" BOOLEAN NOT NULL DEFAULT 0,
      "orderIdx" INTEGER NOT NULL DEFAULT 0);
    CREATE TABLE "terminalContent" ("id" TEXT PRIMARY KEY NOT NULL, "worktreeId" TEXT NOT NULL,
      "launchKind" TEXT NOT NULL, "agentId" TEXT, "commandJSON" TEXT, "createdAt" DATETIME NOT NULL);
    CREATE TABLE "paneScrollback" ("terminalContentId" TEXT PRIMARY KEY NOT NULL,
      "worktreeId" TEXT NOT NULL, "data" BLOB NOT NULL, "updatedAt" DATETIME NOT NULL);
    CREATE TABLE "agentAccount" ("id" TEXT PRIMARY KEY NOT NULL, "provider" TEXT NOT NULL,
      "configDirPath" TEXT NOT NULL, "label" TEXT NOT NULL, "orgName" TEXT,
      "createdAt" DATETIME NOT NULL, "lastAuthenticatedAt" DATETIME NOT NULL);
    CREATE TABLE "agentSession" ("terminalContentId" TEXT PRIMARY KEY NOT NULL,
      "worktreeId" TEXT NOT NULL, "agentId" TEXT NOT NULL, "sessionRef" TEXT NOT NULL,
      "capturedAt" DATETIME NOT NULL);
    CREATE TABLE "chatSession" ("id" TEXT PRIMARY KEY NOT NULL, "worktreeId" TEXT NOT NULL,
      "agentId" TEXT NOT NULL, "acpSessionId" TEXT, "createdAt" DATETIME NOT NULL,
      "lastActivityAt" DATETIME NOT NULL, "contextUsageUsed" INTEGER, "contextUsageSize" INTEGER,
      "permissionMode" TEXT, "selectedModel" TEXT, "selectedEffort" TEXT,
      "transportKind" TEXT NOT NULL DEFAULT 'acp', "title" TEXT);
    CREATE TABLE "chatItem" ("sessionId" TEXT NOT NULL, "ordinal" INTEGER NOT NULL,
      "kind" TEXT NOT NULL, "payload" BLOB NOT NULL, PRIMARY KEY ("sessionId","ordinal"));
    CREATE TABLE "workspaceLayout" ("worktreeId" TEXT PRIMARY KEY NOT NULL,
      "schemaVersion" INTEGER NOT NULL, "revision" INTEGER NOT NULL, "payload" TEXT NOT NULL,
      "checksum" TEXT NOT NULL, "updatedAt" DATETIME NOT NULL);
    CREATE TABLE "workspaceTab" ("id" TEXT PRIMARY KEY NOT NULL, "worktreeId" TEXT NOT NULL,
      "title" TEXT NOT NULL, "titleIsAutoNamed" BOOLEAN NOT NULL, "contentKind" TEXT NOT NULL,
      "contentId" TEXT NOT NULL, "viewStateJSON" TEXT, "viewStateVersion" INTEGER NOT NULL,
      "createdAt" DATETIME NOT NULL);
    CREATE TABLE "workspaceLayoutQuarantine" ("id" TEXT PRIMARY KEY NOT NULL,
      "worktreeId" TEXT NOT NULL, "payload" TEXT NOT NULL, "reason" TEXT NOT NULL,
      "createdAt" DATETIME NOT NULL);
    INSERT INTO project VALUES('p1','Tiller','/tmp/tiller','2026-01-01T00:00:00Z',
      NULL,NULL,'icon',NULL,NULL,NULL,NULL,0);
    INSERT INTO worktree VALUES('w1','p1','main','/tmp/tiller','2026-01-01T00:00:00Z',
      NULL,NULL,1,0);
    INSERT INTO chatSession VALUES('s1','w1','claude',NULL,'2026-01-01T00:00:00Z',
      '2026-01-01T00:00:00Z',NULL,NULL,NULL,NULL,NULL,'acp','Prima chat');
  `)
  src.prepare('INSERT INTO paneScrollback VALUES(?,?,?,?)').run(
    't1', 'w1', Buffer.from('ciao dal terminale'), '2026-01-01T00:00:00Z'
  )
  src.prepare('INSERT INTO terminalContent VALUES(?,?,?,?,?,?)').run(
    't1', 'w1', 'shell', null, null, '2026-01-01T00:00:00Z'
  )
  src.prepare('INSERT INTO chatItem VALUES(?,?,?,?)').run(
    's1', 0, 'userMessage', Buffer.from('{"userMessage":{"blocks":[]}}')
  )
  src.close()
  return path
}

test('import trasferisce tutte le righe e i conteggi coincidono', async () => {
  const source = makeSwiftDb()
  const report = await importFromSwift(db, source)
  expect(report.counts.project).toEqual({ source: 1, target: 1 })
  expect(report.counts.worktree).toEqual({ source: 1, target: 1 })
  expect(report.counts.chatItem).toEqual({ source: 1, target: 1 })
  rmSync(source, { force: true })
})

test('import e idempotente: due esecuzioni non duplicano', async () => {
  const source = makeSwiftDb()
  await importFromSwift(db, source)
  const second = await importFromSwift(db, source)
  expect(second.counts.project).toEqual({ source: 1, target: 1 })
  expect(second.counts.chatItem).toEqual({ source: 1, target: 1 })
  rmSync(source, { force: true })
})

test('i BLOB arrivano intatti', async () => {
  const source = makeSwiftDb()
  await importFromSwift(db, source)
  const row = await db.selectFrom('paneScrollback').selectAll().executeTakeFirstOrThrow()
  expect(Buffer.from(row.data).toString('utf8')).toBe('ciao dal terminale')
  rmSync(source, { force: true })
})

test('import non modifica il file sorgente', async () => {
  const source = makeSwiftDb()
  const before = new BetterSqlite3(source, { readonly: true })
  const countBefore = before.prepare('SELECT count(*) c FROM project').get() as { c: number }
  before.close()

  await importFromSwift(db, source)

  const after = new BetterSqlite3(source, { readonly: true })
  const countAfter = after.prepare('SELECT count(*) c FROM project').get() as { c: number }
  after.close()
  expect(countAfter.c).toBe(countBefore.c)
  rmSync(source, { force: true })
})

test('un percorso inesistente fallisce con un messaggio chiaro', async () => {
  await expect(importFromSwift(db, '/tmp/non-esiste-affatto.sqlite')).rejects.toThrow(
    /non trovato/
  )
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/import/`
Atteso: FAIL, `Cannot find module './swift-import'`.

- [ ] **Step 3: scrivere `src/main/import/swift-import.ts`**

```ts
import BetterSqlite3 from 'better-sqlite3'
import { copyFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import type { Kysely } from 'kysely'
import { TABLES_IN_DEPENDENCY_ORDER, type TillerDatabase } from '../db/schema'

export interface ImportReport {
  counts: Record<string, { source: number; target: number }>
}

/**
 * Travasa i dati dal database dell'app Swift.
 *
 * Il file sorgente non viene mai aperto in scrittura ne' bloccato: l'app Swift
 * puo' essere in esecuzione. Si copia in una cartella temporanea e si legge da
 * li' in sola lettura.
 *
 * L'inserimento e' un upsert per chiave primaria, quindi rieseguire l'import
 * non duplica nulla — utile perche' l'app Swift continua a essere usata durante
 * la migrazione e l'import va rilanciato.
 */
export async function importFromSwift(
  db: Kysely<TillerDatabase>,
  sourcePath: string
): Promise<ImportReport> {
  if (!existsSync(sourcePath)) {
    throw new Error(`database sorgente non trovato: ${sourcePath}`)
  }

  const scratch = mkdtempSync(join(tmpdir(), 'tiller-import-'))
  const copy = join(scratch, 'source.sqlite')
  copyFileSync(sourcePath, copy)

  try {
    const source = new BetterSqlite3(copy, { readonly: true })
    const counts: ImportReport['counts'] = {}

    await db.transaction().execute(async (tx) => {
      for (const table of TABLES_IN_DEPENDENCY_ORDER) {
        const rows = source.prepare(`SELECT * FROM "${table}"`).all() as Record<
          string,
          unknown
        >[]
        counts[table] = { source: rows.length, target: 0 }
        if (rows.length === 0) continue

        for (const row of rows) {
          await tx
            .insertInto(table)
            // I nomi di colonna della sorgente coincidono con quelli di
            // destinazione: lo schema e' lo stesso, collassato.
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            .values(row as any)
            .onConflict((oc) => oc.doNothing())
            .execute()
        }
      }
    })

    for (const table of TABLES_IN_DEPENDENCY_ORDER) {
      const result = await db
        .selectFrom(table)
        .select((eb) => eb.fn.countAll<number>().as('c'))
        .executeTakeFirstOrThrow()
      counts[table].target = Number(result.c)
    }

    source.close()

    const mismatched = Object.entries(counts).filter(
      ([, c]) => c.source !== c.target
    )
    if (mismatched.length > 0) {
      const detail = mismatched
        .map(([table, c]) => `${table}: ${c.source} in sorgente, ${c.target} importate`)
        .join('; ')
      throw new Error(`import incompleto — ${detail}`)
    }

    return { counts }
  } finally {
    rmSync(scratch, { recursive: true, force: true })
  }
}
```

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/import/`
Atteso: PASS, 5 test.

- [ ] **Step 5: commit**

```bash
git add src/main/import/
git commit -m "feat: import una tantum dal database dell'app Swift"
```

---

### Task 5: runner git e rilevamento repository

**File:**
- Crea: `src/main/git/runner.ts`
- Crea: `src/main/git/repo.ts`
- Test: `src/main/git/repo.test.ts`
- Crea: `src/main/git/test-repo.ts` (aiuto per i test)

**Interfacce:**
- Consuma: niente dai task precedenti.
- Produce: `gitIn(repoPath: string): SimpleGit`;
  `isGitRepository(path: string): Promise<boolean>`;
  `repositoryRoot(path: string): Promise<string | null>`;
  `currentBranch(repoPath: string): Promise<string>`;
  e l'aiuto `createTestRepo(): Promise<string>` usato dai task 6, 7 e 8.

- [ ] **Step 1: scrivere l'aiuto per i test**

Creare `src/main/git/test-repo.ts`:

```ts
import { execFileSync } from 'node:child_process'
import { mkdtempSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

/**
 * Crea un repository git vero in una cartella temporanea.
 *
 * I test di questo modulo non usano mock: simple-git va verificato contro git
 * vero, non contro le nostre aspettative su git. Un mock confermerebbe solo
 * che sappiamo cosa ci aspettiamo.
 */
export function createTestRepo(): string {
  const dir = mkdtempSync(join(tmpdir(), 'tiller-git-'))
  const run = (...args: string[]): void => {
    execFileSync('git', args, { cwd: dir, stdio: 'pipe' })
  }
  run('init', '--initial-branch=main')
  run('config', 'user.email', 'test@example.com')
  run('config', 'user.name', 'Test')
  writeFileSync(join(dir, 'README.md'), '# test\n')
  run('add', 'README.md')
  run('commit', '-m', 'primo commit')
  return dir
}
```

- [ ] **Step 2: scrivere il test**

Creare `src/main/git/repo.test.ts`:

```ts
import { expect, test } from 'vitest'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { createTestRepo } from './test-repo'
import { isGitRepository, repositoryRoot, currentBranch } from './repo'

test('una cartella con git e riconosciuta come repository', async () => {
  const repo = createTestRepo()
  expect(await isGitRepository(repo)).toBe(true)
  rmSync(repo, { recursive: true, force: true })
})

test('una cartella qualsiasi non e un repository', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'tiller-plain-'))
  expect(await isGitRepository(dir)).toBe(false)
  rmSync(dir, { recursive: true, force: true })
})

test('la root del repository e la cartella stessa', async () => {
  const repo = createTestRepo()
  const root = await repositoryRoot(repo)
  // macOS mette /tmp dietro un symlink verso /private/tmp: git risponde con il
  // percorso risolto, quindi si confronta la fine.
  expect(root?.endsWith(repo.replace('/private', ''))).toBe(true)
  rmSync(repo, { recursive: true, force: true })
})

test('la root di una cartella non-git e null', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'tiller-plain-'))
  expect(await repositoryRoot(dir)).toBeNull()
  rmSync(dir, { recursive: true, force: true })
})

test('il branch corrente di un repo appena creato e main', async () => {
  const repo = createTestRepo()
  expect(await currentBranch(repo)).toBe('main')
  rmSync(repo, { recursive: true, force: true })
})
```

- [ ] **Step 3: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/git/repo.test.ts`
Atteso: FAIL, `Cannot find module './repo'`.

- [ ] **Step 4: scrivere `src/main/git/runner.ts`**

```ts
import { simpleGit, type SimpleGit } from 'simple-git'

/** Tetto all'output di un singolo comando git, in byte. */
const MAX_OUTPUT_BYTES = 10 * 1024 * 1024

/**
 * Istanza di simple-git legata a una cartella.
 *
 * maxConcurrentProcesses a 1 serializza i comandi sullo stesso repository:
 * git prende lock sull'index, e due comandi in parallelo sullo stesso repo
 * falliscono in modo intermittente e difficile da riprodurre.
 */
export function gitIn(repoPath: string): SimpleGit {
  return simpleGit({
    baseDir: repoPath,
    binary: 'git',
    maxConcurrentProcesses: 1,
    trimmed: true,
    config: [`core.bigFileThreshold=${MAX_OUTPUT_BYTES}`]
  })
}
```

- [ ] **Step 5: scrivere `src/main/git/repo.ts`**

```ts
import { gitIn } from './runner'

export async function isGitRepository(path: string): Promise<boolean> {
  try {
    return await gitIn(path).checkIsRepo()
  } catch {
    // simple-git lancia se la cartella non esiste o git non e' installato:
    // in entrambi i casi la risposta utile e' "non e' un repository".
    return false
  }
}

export async function repositoryRoot(path: string): Promise<string | null> {
  try {
    const root = await gitIn(path).revparse(['--show-toplevel'])
    return root.length > 0 ? root : null
  } catch {
    return null
  }
}

export async function currentBranch(repoPath: string): Promise<string> {
  return gitIn(repoPath).revparse(['--abbrev-ref', 'HEAD'])
}
```

- [ ] **Step 6: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/git/repo.test.ts`
Atteso: PASS, 5 test.

- [ ] **Step 7: commit**

```bash
git add src/main/git/
git commit -m "feat: runner git e rilevamento repository"
```

---

### Task 6: worktree git

**File:**
- Crea: `src/main/git/worktrees.ts`
- Test: `src/main/git/worktrees.test.ts`

**Interfacce:**
- Consuma: `gitIn(repoPath)`, `createTestRepo()`.
- Produce: `GitWorktreeInfo = { path: string; branch: string | null; isMain: boolean }`;
  `parseWorktreeList(output: string): GitWorktreeInfo[]`;
  `listWorktrees(repoPath: string): Promise<GitWorktreeInfo[]>`;
  `addWorktree(repoPath: string, options: { branch: string; path: string; base?: string }): Promise<void>`;
  `removeWorktree(repoPath: string, worktreePath: string): Promise<void>`.

> Attenzione ai nomi: `listWorktrees`/`addWorktree`/`removeWorktree` esistono
> anche in `src/main/db/repository.ts` con firme diverse (quelle lavorano sul
> database, queste su git). Chi le importa entrambe deve rinominarle
> all'import, per esempio `import { listWorktrees as listGitWorktrees }`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/git/worktrees.test.ts`:

```ts
import { expect, test } from 'vitest'
import { execFileSync } from 'node:child_process'
import { existsSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { createTestRepo } from './test-repo'
import { parseWorktreeList, listWorktrees, addWorktree, removeWorktree } from './worktrees'

test('il parser legge il blocco del worktree principale', () => {
  const output = [
    'worktree /tmp/repo',
    'HEAD abc123',
    'branch refs/heads/main',
    ''
  ].join('\n')
  expect(parseWorktreeList(output)).toEqual([
    { path: '/tmp/repo', branch: 'main', isMain: true }
  ])
})

test('il parser distingue il principale dai secondari', () => {
  const output = [
    'worktree /tmp/repo',
    'HEAD abc123',
    'branch refs/heads/main',
    '',
    'worktree /tmp/repo-feature',
    'HEAD def456',
    'branch refs/heads/feature',
    ''
  ].join('\n')
  expect(parseWorktreeList(output)).toEqual([
    { path: '/tmp/repo', branch: 'main', isMain: true },
    { path: '/tmp/repo-feature', branch: 'feature', isMain: false }
  ])
})

test('il parser gestisce un worktree in stato detached', () => {
  const output = ['worktree /tmp/repo', 'HEAD abc123', 'detached', ''].join('\n')
  expect(parseWorktreeList(output)).toEqual([
    { path: '/tmp/repo', branch: null, isMain: true }
  ])
})

test('il parser accetta un ultimo blocco senza riga vuota finale', () => {
  const output = ['worktree /tmp/repo', 'HEAD abc123', 'branch refs/heads/main'].join('\n')
  expect(parseWorktreeList(output)).toEqual([
    { path: '/tmp/repo', branch: 'main', isMain: true }
  ])
})

test('un repo appena creato ha un solo worktree, il principale', async () => {
  const repo = createTestRepo()
  const list = await listWorktrees(repo)
  expect(list).toHaveLength(1)
  expect(list[0]?.isMain).toBe(true)
  expect(list[0]?.branch).toBe('main')
  rmSync(repo, { recursive: true, force: true })
})

test('aggiungere un worktree lo crea su disco e git lo elenca', async () => {
  const repo = createTestRepo()
  const target = join(repo, '..', `wt-${Date.now()}`)

  await addWorktree(repo, { branch: 'feature/uno', path: target })

  expect(existsSync(target)).toBe(true)
  // Oracolo esterno: si interroga git direttamente, non il nostro parser.
  const raw = execFileSync('git', ['worktree', 'list', '--porcelain'], {
    cwd: repo,
    encoding: 'utf8'
  })
  expect(raw).toContain('refs/heads/feature/uno')

  rmSync(target, { recursive: true, force: true })
  rmSync(repo, { recursive: true, force: true })
})

test('rimuovere un worktree lo toglie da disco e dalla lista', async () => {
  const repo = createTestRepo()
  const target = join(repo, '..', `wt-rm-${Date.now()}`)
  await addWorktree(repo, { branch: 'feature/due', path: target })

  await removeWorktree(repo, target)

  expect(existsSync(target)).toBe(false)
  expect(await listWorktrees(repo)).toHaveLength(1)
  rmSync(repo, { recursive: true, force: true })
})

test('aggiungere un worktree su un branch gia esistente fallisce', async () => {
  const repo = createTestRepo()
  const target = join(repo, '..', `wt-dup-${Date.now()}`)
  await expect(addWorktree(repo, { branch: 'main', path: target })).rejects.toThrow()
  rmSync(repo, { recursive: true, force: true })
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/git/worktrees.test.ts`
Atteso: FAIL, `Cannot find module './worktrees'`.

- [ ] **Step 3: scrivere `src/main/git/worktrees.ts`**

```ts
import { gitIn } from './runner'

export interface GitWorktreeInfo {
  path: string
  branch: string | null
  isMain: boolean
}

/**
 * Legge l'output di `git worktree list --porcelain`.
 *
 * Il formato e' a blocchi separati da riga vuota; il primo blocco e' sempre il
 * worktree principale. Un blocco puo' dichiarare `detached` invece di `branch`.
 *
 * Scritto a mano perche' simple-git non espone alcuna API per i worktree, e su
 * npm non esiste un pacchetto per essi con un'adozione non trascurabile.
 */
export function parseWorktreeList(output: string): GitWorktreeInfo[] {
  const result: GitWorktreeInfo[] = []
  let path: string | null = null
  let branch: string | null = null
  let isFirst = true

  const flush = (): void => {
    if (path === null) return
    result.push({ path, branch, isMain: isFirst })
    isFirst = false
    path = null
    branch = null
  }

  for (const line of output.split('\n')) {
    if (line.startsWith('worktree ')) {
      path = line.slice('worktree '.length)
    } else if (line.startsWith('branch refs/heads/')) {
      branch = line.slice('branch refs/heads/'.length)
    } else if (line.length === 0) {
      flush()
    }
  }
  flush()

  return result
}

export async function listWorktrees(repoPath: string): Promise<GitWorktreeInfo[]> {
  const output = await gitIn(repoPath).raw(['worktree', 'list', '--porcelain'])
  return parseWorktreeList(output)
}

export async function addWorktree(
  repoPath: string,
  options: { branch: string; path: string; base?: string }
): Promise<void> {
  const args = ['worktree', 'add', '-b', options.branch, options.path]
  if (options.base !== undefined) args.push(options.base)
  await gitIn(repoPath).raw(args)
}

export async function removeWorktree(repoPath: string, worktreePath: string): Promise<void> {
  await gitIn(repoPath).raw(['worktree', 'remove', '--force', worktreePath])
}
```

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/git/worktrees.test.ts`
Atteso: PASS, 8 test.

- [ ] **Step 5: commit**

```bash
git add src/main/git/worktrees.ts src/main/git/worktrees.test.ts
git commit -m "feat: gestione dei worktree git"
```

---

### Task 7: stato git, branch, remote e clone

**File:**
- Crea: `src/main/git/status.ts`
- Crea: `src/main/git/branches.ts`
- Crea: `src/main/git/remote.ts`
- Crea: `src/main/git/clone.ts`
- Test: `src/main/git/status.test.ts`, `src/main/git/remote.test.ts`

**Interfacce:**
- Consuma: `gitIn(repoPath)`, `createTestRepo()`.
- Produce:
  `GitFileState = 'modified' | 'added' | 'deleted' | 'renamed' | 'untracked' | 'conflicted'`;
  `GitStatusEntry = { path: string; state: GitFileState; staged: boolean }`;
  `GitStatusSnapshot = { branch: string | null; entries: GitStatusEntry[] }`;
  `readStatus(repoPath: string): Promise<GitStatusSnapshot>`;
  `listBranches(repoPath: string): Promise<string[]>`;
  `githubOwner(repoPath: string): Promise<string | null>`;
  `projectNameFromCloneUrl(url: string): string`;
  `clone(url: string, target: string, onProgress?: (fraction: number) => void): Promise<void>`.

- [ ] **Step 1: scrivere il test dello stato**

Creare `src/main/git/status.test.ts`:

```ts
import { expect, test } from 'vitest'
import { execFileSync } from 'node:child_process'
import { rmSync, writeFileSync, unlinkSync } from 'node:fs'
import { join } from 'node:path'
import { createTestRepo } from './test-repo'
import { readStatus } from './status'
import { listBranches } from './branches'

test('un repo pulito non ha voci di stato', async () => {
  const repo = createTestRepo()
  const status = await readStatus(repo)
  expect(status.entries).toEqual([])
  expect(status.branch).toBe('main')
  rmSync(repo, { recursive: true, force: true })
})

test('un file nuovo risulta untracked e non staged', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'nuovo.txt'), 'contenuto\n')
  const status = await readStatus(repo)
  expect(status.entries).toEqual([{ path: 'nuovo.txt', state: 'untracked', staged: false }])
  rmSync(repo, { recursive: true, force: true })
})

test('un file modificato e messo in stage risulta staged', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'README.md'), '# cambiato\n')
  execFileSync('git', ['add', 'README.md'], { cwd: repo })
  const status = await readStatus(repo)
  expect(status.entries).toEqual([{ path: 'README.md', state: 'modified', staged: true }])
  rmSync(repo, { recursive: true, force: true })
})

test('un file cancellato risulta deleted', async () => {
  const repo = createTestRepo()
  unlinkSync(join(repo, 'README.md'))
  const status = await readStatus(repo)
  expect(status.entries).toEqual([{ path: 'README.md', state: 'deleted', staged: false }])
  rmSync(repo, { recursive: true, force: true })
})

test('i branch locali vengono elencati', async () => {
  const repo = createTestRepo()
  execFileSync('git', ['branch', 'altro'], { cwd: repo })
  expect((await listBranches(repo)).sort()).toEqual(['altro', 'main'])
  rmSync(repo, { recursive: true, force: true })
})
```

- [ ] **Step 2: scrivere il test del remote**

Creare `src/main/git/remote.test.ts`:

```ts
import { expect, test } from 'vitest'
import { execFileSync } from 'node:child_process'
import { rmSync } from 'node:fs'
import { createTestRepo } from './test-repo'
import { githubOwner, projectNameFromCloneUrl } from './remote'

test('il nome progetto si ricava da un URL https', () => {
  expect(projectNameFromCloneUrl('https://github.com/stablyai/orca.git')).toBe('orca')
})

test('il nome progetto si ricava da un URL ssh', () => {
  expect(projectNameFromCloneUrl('git@github.com:stablyai/orca.git')).toBe('orca')
})

test('il nome progetto tollera uno slash finale', () => {
  expect(projectNameFromCloneUrl('https://github.com/stablyai/orca/')).toBe('orca')
})

test('senza remote non c e owner', async () => {
  const repo = createTestRepo()
  expect(await githubOwner(repo)).toBeNull()
  rmSync(repo, { recursive: true, force: true })
})

test("l owner si legge dal remote origin", async () => {
  const repo = createTestRepo()
  execFileSync('git', ['remote', 'add', 'origin', 'git@github.com:stablyai/orca.git'], {
    cwd: repo
  })
  expect(await githubOwner(repo)).toBe('stablyai')
  rmSync(repo, { recursive: true, force: true })
})
```

- [ ] **Step 3: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/git/status.test.ts src/main/git/remote.test.ts`
Atteso: FAIL, moduli non trovati.

- [ ] **Step 4: scrivere `src/main/git/status.ts`**

```ts
import { gitIn } from './runner'

export type GitFileState =
  | 'modified'
  | 'added'
  | 'deleted'
  | 'renamed'
  | 'untracked'
  | 'conflicted'

export interface GitStatusEntry {
  path: string
  state: GitFileState
  staged: boolean
}

export interface GitStatusSnapshot {
  branch: string | null
  entries: GitStatusEntry[]
}

/**
 * Traduce i codici a due lettere di `git status --porcelain` nel modello di
 * dominio. La prima lettera descrive l'index (staged), la seconda l'albero di
 * lavoro. simple-git fa gia' il parsing; qui si normalizza la forma.
 */
function toState(index: string, workingDir: string): GitFileState {
  if (index === 'U' || workingDir === 'U') return 'conflicted'
  if (index === '?' || workingDir === '?') return 'untracked'
  if (index === 'R' || workingDir === 'R') return 'renamed'
  if (index === 'D' || workingDir === 'D') return 'deleted'
  if (index === 'A' || workingDir === 'A') return 'added'
  return 'modified'
}

export async function readStatus(repoPath: string): Promise<GitStatusSnapshot> {
  const status = await gitIn(repoPath).status()
  const entries: GitStatusEntry[] = status.files.map((file) => ({
    path: file.path,
    state: toState(file.index, file.working_dir),
    // Lo spazio nell'index significa "nessuna modifica messa in stage".
    staged: file.index !== ' ' && file.index !== '?'
  }))
  return { branch: status.current, entries }
}
```

- [ ] **Step 5: scrivere `src/main/git/branches.ts`**

```ts
import { gitIn } from './runner'

export async function listBranches(repoPath: string): Promise<string[]> {
  const summary = await gitIn(repoPath).branchLocal()
  return summary.all
}
```

- [ ] **Step 6: scrivere `src/main/git/remote.ts`**

```ts
import { gitIn } from './runner'

/** Estrae `owner` da `https://github.com/owner/repo.git` o `git@github.com:owner/repo.git`. */
export async function githubOwner(repoPath: string): Promise<string | null> {
  try {
    const remotes = await gitIn(repoPath).getRemotes(true)
    const origin = remotes.find((r) => r.name === 'origin')
    const url = origin?.refs.fetch
    if (url === undefined || url.length === 0) return null

    const match = /github\.com[:/]([^/]+)\//.exec(url)
    return match?.[1] ?? null
  } catch {
    return null
  }
}

export function projectNameFromCloneUrl(url: string): string {
  const withoutTrailingSlash = url.replace(/\/+$/, '')
  const last = withoutTrailingSlash.split(/[:/]/).pop() ?? ''
  return last.replace(/\.git$/, '')
}
```

- [ ] **Step 7: scrivere `src/main/git/clone.ts`**

```ts
import { simpleGit } from 'simple-git'

/**
 * Clona riportando l'avanzamento come frazione fra 0 e 1.
 *
 * simple-git espone il progresso attraverso l'opzione `progress`, che riceve
 * gli eventi che git scrive su stderr. Non emette un evento finale a 1: chi
 * mostra una barra deve chiuderla quando la promise si risolve.
 */
export async function clone(
  url: string,
  target: string,
  onProgress?: (fraction: number) => void
): Promise<void> {
  const git = simpleGit({
    progress:
      onProgress === undefined
        ? undefined
        : ({ progress }) => onProgress(Math.min(1, Math.max(0, progress / 100)))
  })
  await git.clone(url, target, ['--progress'])
}
```

- [ ] **Step 8: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/git/`
Atteso: PASS, tutti i test git (18).

- [ ] **Step 9: commit**

```bash
git add src/main/git/
git commit -m "feat: stato git, branch, remote e clone"
```

---

### Task 8: azioni git distruttive con validazione

**File:**
- Crea: `src/main/git/actions.ts`
- Test: `src/main/git/actions.test.ts`

**Interfacce:**
- Consuma: `gitIn(repoPath)`, `GitStatusEntry`, `readStatus`, `createTestRepo()`.
- Produce: `stage(repoPath, entries: GitStatusEntry[]): Promise<void>`;
  `unstage(repoPath, entries: GitStatusEntry[]): Promise<void>`;
  `discardChanges(repoPath, entries: GitStatusEntry[]): Promise<void>`;
  `discardUntracked(repoPath, entries: GitStatusEntry[]): Promise<void>`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/git/actions.test.ts`:

```ts
import { expect, test } from 'vitest'
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { join } from 'node:path'
import { createTestRepo } from './test-repo'
import { readStatus } from './status'
import { stage, unstage, discardChanges, discardUntracked } from './actions'

test('mettere in stage un file lo rende staged', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'README.md'), '# cambiato\n')
  const before = await readStatus(repo)
  await stage(repo, before.entries)
  const after = await readStatus(repo)
  expect(after.entries[0]?.staged).toBe(true)
  rmSync(repo, { recursive: true, force: true })
})

test('togliere dallo stage riporta il file a non staged', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'README.md'), '# cambiato\n')
  await stage(repo, (await readStatus(repo)).entries)
  await unstage(repo, (await readStatus(repo)).entries)
  const after = await readStatus(repo)
  expect(after.entries[0]?.staged).toBe(false)
  rmSync(repo, { recursive: true, force: true })
})

test('scartare le modifiche riporta il file al contenuto committato', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'README.md'), '# cambiato\n')
  await discardChanges(repo, (await readStatus(repo)).entries)
  expect(readFileSync(join(repo, 'README.md'), 'utf8')).toBe('# test\n')
  rmSync(repo, { recursive: true, force: true })
})

test('scartare gli untracked cancella solo i file non tracciati', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'usa-e-getta.txt'), 'x\n')
  const untracked = (await readStatus(repo)).entries.filter((e) => e.state === 'untracked')
  await discardUntracked(repo, untracked)
  expect(existsSync(join(repo, 'usa-e-getta.txt'))).toBe(false)
  expect(existsSync(join(repo, 'README.md'))).toBe(true)
  rmSync(repo, { recursive: true, force: true })
})

test('un percorso assoluto viene rifiutato', async () => {
  const repo = createTestRepo()
  await expect(
    discardChanges(repo, [{ path: '/etc/passwd', state: 'modified', staged: false }])
  ).rejects.toThrow(/percorso non valido/)
  rmSync(repo, { recursive: true, force: true })
})

test('un percorso che risale fuori dal repository viene rifiutato', async () => {
  const repo = createTestRepo()
  await expect(
    discardUntracked(repo, [{ path: '../fuori.txt', state: 'untracked', staged: false }])
  ).rejects.toThrow(/percorso non valido/)
  rmSync(repo, { recursive: true, force: true })
})

test('un percorso che inizia con un trattino viene rifiutato', async () => {
  const repo = createTestRepo()
  await expect(
    discardChanges(repo, [{ path: '--force', state: 'modified', staged: false }])
  ).rejects.toThrow(/percorso non valido/)
  rmSync(repo, { recursive: true, force: true })
})

test('un elenco vuoto non esegue nulla', async () => {
  const repo = createTestRepo()
  writeFileSync(join(repo, 'intatto.txt'), 'x\n')
  await discardUntracked(repo, [])
  expect(existsSync(join(repo, 'intatto.txt'))).toBe(true)
  rmSync(repo, { recursive: true, force: true })
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/git/actions.test.ts`
Atteso: FAIL, `Cannot find module './actions'`.

- [ ] **Step 3: scrivere `src/main/git/actions.ts`**

```ts
import { isAbsolute, normalize } from 'node:path'
import { gitIn } from './runner'
import type { GitStatusEntry } from './status'

/**
 * Valida i percorsi prima di passarli a git.
 *
 * `discardChanges` e `discardUntracked` sono le uniche operazioni di Tiller che
 * cancellano lavoro non committato in modo irreversibile, e ricevono percorsi
 * che a regime arrivano dall'interfaccia. Tre categorie vanno fermate qui:
 * percorsi assoluti e percorsi che risalgono (`..`) uscirebbero dal
 * repository; percorsi che iniziano con `-` verrebbero letti da git come
 * opzioni, non come file.
 */
function validatedPaths(entries: GitStatusEntry[]): string[] {
  return entries.map((entry) => {
    const path = entry.path
    const invalid =
      path.length === 0 ||
      isAbsolute(path) ||
      path.startsWith('-') ||
      normalize(path).startsWith('..')
    if (invalid) throw new Error(`percorso non valido: ${path}`)
    return path
  })
}

export async function stage(repoPath: string, entries: GitStatusEntry[]): Promise<void> {
  const paths = validatedPaths(entries)
  if (paths.length === 0) return
  await gitIn(repoPath).raw(['add', '--', ...paths])
}

export async function unstage(repoPath: string, entries: GitStatusEntry[]): Promise<void> {
  const paths = validatedPaths(entries)
  if (paths.length === 0) return
  await gitIn(repoPath).raw(['restore', '--staged', '--', ...paths])
}

export async function discardChanges(
  repoPath: string,
  entries: GitStatusEntry[]
): Promise<void> {
  const paths = validatedPaths(entries)
  if (paths.length === 0) return
  await gitIn(repoPath).raw(['restore', '--worktree', '--', ...paths])
}

export async function discardUntracked(
  repoPath: string,
  entries: GitStatusEntry[]
): Promise<void> {
  const paths = validatedPaths(entries)
  if (paths.length === 0) return
  await gitIn(repoPath).raw(['clean', '-fd', '--', ...paths])
}
```

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/git/actions.test.ts`
Atteso: PASS, 8 test.

- [ ] **Step 5: commit**

```bash
git add src/main/git/actions.ts src/main/git/actions.test.ts
git commit -m "feat: azioni git con validazione dei percorsi"
```

---

### Task 9: quoting shell e letterali TOML

**File:**
- Crea: `src/main/agents/quote.ts`
- Test: `src/main/agents/quote.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `shellQuote(value: string): string`;
  `tomlStringLiteral(value: string): string`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/agents/quote.test.ts`:

```ts
import { expect, test } from 'vitest'
import { execFileSync } from 'node:child_process'
import { parse as parseToml } from 'smol-toml'
import { shellQuote, tomlStringLiteral } from './quote'

test('una stringa semplice viene racchiusa fra apici', () => {
  expect(shellQuote('ciao')).toBe("'ciao'")
})

test("un apice interno viene neutralizzato", () => {
  expect(shellQuote("l'agente")).toBe("'l'\\''agente'")
})

test('il quoting sopravvive alla shell vera', () => {
  const tricky = `spazi e 'apici' e "virgolette" e $VAR e \`backtick\``
  const output = execFileSync('sh', ['-c', `printf %s ${shellQuote(tricky)}`], {
    encoding: 'utf8'
  })
  // Oracolo esterno: la shell e' l'unica autorita' su cosa sia una parola sola.
  expect(output).toBe(tricky)
})

test('il letterale TOML non scappa le barre', () => {
  expect(tomlStringLiteral('/Users/x/bin/tillerctl')).toBe('"/Users/x/bin/tillerctl"')
})

test('il letterale TOML si riparsa come TOML valido', () => {
  const value = '/Users/x/bin/tillerctl'
  const parsed = parseToml(`v = ${tomlStringLiteral(value)}`)
  expect(parsed.v).toBe(value)
})

test('il letterale TOML sopravvive a virgolette e barre rovesciate', () => {
  const value = 'ha "virgolette" e \\ barre'
  const parsed = parseToml(`v = ${tomlStringLiteral(value)}`)
  expect(parsed.v).toBe(value)
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/agents/quote.test.ts`
Atteso: FAIL, `Cannot find module './quote'`.

- [ ] **Step 3: scrivere `src/main/agents/quote.ts`**

```ts
/**
 * Racchiude una stringa in apici singoli POSIX, sostituendo ogni apice interno
 * con '\'' — il risultato e' una parola sola per sh, zsh e bash.
 */
export function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'\\''`)}'`
}

/**
 * Letterale stringa TOML (basic string).
 *
 * Serve per l'override `-c notify=[...]` di Codex, che viene interpretato come
 * TOML e non come JSON. Nell'app Swift questo punto era una trappola:
 * JSONEncoder scappa `/` come `\/` per default, che in TOML non e' una
 * sequenza di escape valida, e Codex moriva al caricamento della
 * configurazione prima ancora di mostrare la sua interfaccia.
 *
 * In JavaScript il problema non esiste: JSON.stringify non tocca le barre, e
 * ogni escape che produce (\" \\ \n \t \uXXXX) e' valido anche in TOML. La
 * verifica non e' affidata a questo commento: il test riparsa il risultato con
 * un parser TOML vero.
 */
export function tomlStringLiteral(value: string): string {
  return JSON.stringify(value)
}
```

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/agents/quote.test.ts`
Atteso: PASS, 6 test.

- [ ] **Step 5: commit**

```bash
git add src/main/agents/quote.ts src/main/agents/quote.test.ts
git commit -m "feat: quoting shell e letterali TOML"
```

---

### Task 10: i cinque adapter agenti

**File:**
- Crea: `src/main/agents/catalog.ts`
- Crea: `src/main/agents/adapters/claude.ts`, `codex.ts`, `opencode.ts`, `pi.ts`, `omp.ts`
- Test: `src/main/agents/catalog.test.ts`

**Interfacce:**
- Consuma: `shellQuote`, `tomlStringLiteral`.
- Produce: l'interfaccia `AgentAdapter` con
  `id`, `displayName`, `hasNativeHooks`,
  `prepare(options: PrepareOptions): Promise<void>`,
  `command(options: LaunchOptions): string`,
  `resumeCommand(options: LaunchOptions & { sessionRef: string }): string | null`,
  `summarizerCommand(prompt: string): string | null`;
  `LaunchOptions = { worktreePath: string; paneId: string; tillerctlPath: string }`;
  `PrepareOptions = LaunchOptions`;
  `AGENT_CATALOG: readonly AgentAdapter[]`;
  `findAdapter(id: string): AgentAdapter | undefined`.

- [ ] **Step 1: scrivere il test**

Creare `src/main/agents/catalog.test.ts`:

```ts
import { expect, test } from 'vitest'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, readdirSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parse as parseToml } from 'smol-toml'
import { AGENT_CATALOG, findAdapter } from './catalog'

const launch = {
  worktreePath: '/tmp/wt',
  paneId: '11111111-2222-3333-4444-555555555555',
  tillerctlPath: '/Users/x/bin/tillerctl'
}

test('il catalogo contiene i cinque agenti attesi', () => {
  expect(AGENT_CATALOG.map((a) => a.id)).toEqual(['claude', 'codex', 'opencode', 'pi', 'omp'])
})

test('gli id sono unici', () => {
  const ids = AGENT_CATALOG.map((a) => a.id)
  expect(new Set(ids).size).toBe(ids.length)
})

test('un id sconosciuto non trova adapter', () => {
  expect(findAdapter('inesistente')).toBeUndefined()
})

test('solo codex e omp dichiarano hook nativi', () => {
  const withHooks = AGENT_CATALOG.filter((a) => a.hasNativeHooks).map((a) => a.id)
  expect(withHooks.sort()).toEqual(['codex', 'omp'])
})

test('claude si lancia con il comando nudo e riprende per riferimento', () => {
  const claude = findAdapter('claude')
  expect(claude?.command(launch)).toBe('claude')
  expect(claude?.resumeCommand({ ...launch, sessionRef: 'abc' })).toBe("claude --resume 'abc'")
})

test('pi non ha hook nativi e riprende con --session', () => {
  const pi = findAdapter('pi')
  expect(pi?.hasNativeHooks).toBe(false)
  expect(pi?.resumeCommand({ ...launch, sessionRef: 'abc' })).toBe("pi --session 'abc'")
})

test('opencode riprende con --session', () => {
  const opencode = findAdapter('opencode')
  expect(opencode?.resumeCommand({ ...launch, sessionRef: 'abc' })).toBe(
    "opencode --session 'abc'"
  )
})

test('omp punta al proprio file di hook nel worktree', () => {
  const omp = findAdapter('omp')
  expect(omp?.command(launch)).toBe("omp --hook '/tmp/wt/.tiller/omp-hook.ts'")
})

test("l override notify di codex e TOML valido che si riparsa", () => {
  const codex = findAdapter('codex')
  const command = codex?.command(launch) ?? ''
  // Estrae il contenuto fra apici singoli dopo `-c `.
  const match = /-c '(.+)'$/.exec(command)
  expect(match).not.toBeNull()

  const parsed = parseToml(match?.[1] ?? '')
  expect(parsed.notify).toEqual([
    '/Users/x/bin/tillerctl',
    'notify',
    '--session',
    launch.paneId,
    '--status',
    'needs-input'
  ])
})

test("l override di codex sopravvive alla shell come parola sola", () => {
  const codex = findAdapter('codex')
  const command = codex?.command(launch) ?? ''
  const match = /-c ('.+')$/.exec(command)
  const output = execFileSync('sh', ['-c', `printf %s ${match?.[1]}`], { encoding: 'utf8' })
  expect(output.startsWith('notify=[')).toBe(true)
})

test('prepare non scrive nulla nel worktree', async () => {
  // Guardia sulla decisione D4 della spec: in Fase 1 gli hook non si scrivono,
  // perche' l'app Swift e' ancora in uso e scriverebbero sugli stessi file.
  // Se qualcuno riattiva la scrittura senza volerlo, questo test lo dice prima
  // che se ne accorgano i worktree veri.
  const worktree = mkdtempSync(join(tmpdir(), 'tiller-prepare-'))
  const before = readdirSync(worktree)

  for (const adapter of AGENT_CATALOG) {
    await adapter.prepare({ ...launch, worktreePath: worktree })
  }

  expect(readdirSync(worktree)).toEqual(before)
  rmSync(worktree, { recursive: true, force: true })
})

test('i comandi di riassunto citano il prompt in modo sicuro', () => {
  const claude = findAdapter('claude')
  expect(claude?.summarizerCommand("riassumi 'questo'")).toBe(
    "claude -p 'riassumi '\\''questo'\\'''"
  )
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/agents/catalog.test.ts`
Atteso: FAIL, `Cannot find module './catalog'`.

- [ ] **Step 3: scrivere `src/main/agents/catalog.ts`**

```ts
export interface LaunchOptions {
  worktreePath: string
  paneId: string
  tillerctlPath: string
}

export type PrepareOptions = LaunchOptions

export interface AgentAdapter {
  /** Identificatore stabile, per esempio "claude" o "codex". */
  readonly id: string
  /** Nome leggibile, per esempio "Claude Code". */
  readonly displayName: string
  /**
   * True quando l'agente notifica da solo gli eventi di ciclo di vita
   * chiamando `tillerctl notify`. False → Tiller guarda il codice di uscita.
   */
  readonly hasNativeHooks: boolean

  /**
   * Scriverebbe la configurazione degli hook dentro il worktree.
   *
   * In Fase 1 e' deliberatamente un no-op per ogni adapter: l'app Swift e'
   * ancora in uso e scrive negli stessi file (.claude/settings.local.json,
   * .tiller/omp-hook.ts, .opencode/plugin/tiller-session.js). Chi apre il
   * worktree per ultimo vincerebbe, e l'altra app smetterebbe di ricevere
   * notifiche senza segnalare nulla. La scrittura arriva in Fase 3, insieme
   * al Layer A della detection che e' l'unico consumatore di questi hook.
   */
  prepare(options: PrepareOptions): Promise<void>

  /** Comando di shell completo da eseguire nel pane. */
  command(options: LaunchOptions): string

  /**
   * Comando che rilancia l'agente riprendendo una sessione nativa, oppure null
   * quando l'agente non sa riprendere per riferimento.
   */
  resumeCommand(options: LaunchOptions & { sessionRef: string }): string | null

  /**
   * Comando che esegue la CLI dell'agente in modalita' non interattiva per
   * riassumere `prompt`, oppure null quando non esiste un'invocazione sicura.
   */
  summarizerCommand(prompt: string): string | null
}

import { claudeAdapter } from './adapters/claude'
import { codexAdapter } from './adapters/codex'
import { opencodeAdapter } from './adapters/opencode'
import { piAdapter } from './adapters/pi'
import { ompAdapter } from './adapters/omp'

/** Gli adapter supportati, in ordine di visualizzazione. */
export const AGENT_CATALOG: readonly AgentAdapter[] = [
  claudeAdapter,
  codexAdapter,
  opencodeAdapter,
  piAdapter,
  ompAdapter
]

export function findAdapter(id: string): AgentAdapter | undefined {
  return AGENT_CATALOG.find((adapter) => adapter.id === id)
}
```

- [ ] **Step 4: scrivere i cinque adapter**

`src/main/agents/adapters/claude.ts`:

```ts
import { shellQuote } from '../quote'
import type { AgentAdapter } from '../catalog'

export const claudeAdapter: AgentAdapter = {
  id: 'claude',
  displayName: 'Claude Code',
  hasNativeHooks: true,

  async prepare() {},

  command() {
    return 'claude'
  },

  resumeCommand({ sessionRef }) {
    return `claude --resume ${shellQuote(sessionRef)}`
  },

  summarizerCommand(prompt) {
    return `claude -p ${shellQuote(prompt)}`
  }
}
```

`src/main/agents/adapters/codex.ts`:

```ts
import { shellQuote, tomlStringLiteral } from '../quote'
import type { AgentAdapter, LaunchOptions } from '../catalog'

/**
 * L'override `-c notify=[...]` viene letto da Codex come TOML, non come JSON:
 * ogni elemento dell'array deve essere un letterale stringa TOML valido.
 */
function notifyOverride({ paneId, tillerctlPath }: LaunchOptions): string {
  const args = [tillerctlPath, 'notify', '--session', paneId, '--status', 'needs-input']
  return shellQuote(`notify=[${args.map(tomlStringLiteral).join(',')}]`)
}

export const codexAdapter: AgentAdapter = {
  id: 'codex',
  displayName: 'Codex',
  hasNativeHooks: true,

  async prepare() {},

  command(options) {
    return `codex -c ${notifyOverride(options)}`
  },

  resumeCommand(options) {
    return `codex -c ${notifyOverride(options)} resume ${shellQuote(options.sessionRef)}`
  },

  summarizerCommand(prompt) {
    return `codex exec --output-last-message /dev/stdout ${shellQuote(prompt)}`
  }
}
```

`src/main/agents/adapters/opencode.ts`:

```ts
import { shellQuote } from '../quote'
import type { AgentAdapter } from '../catalog'

export const opencodeAdapter: AgentAdapter = {
  id: 'opencode',
  displayName: 'OpenCode',
  hasNativeHooks: false,

  async prepare() {},

  command() {
    return 'opencode'
  },

  resumeCommand({ sessionRef }) {
    return `opencode --session ${shellQuote(sessionRef)}`
  },

  summarizerCommand(prompt) {
    return `opencode run --pure ${shellQuote(prompt)}`
  }
}
```

`src/main/agents/adapters/pi.ts`:

```ts
import { shellQuote } from '../quote'
import type { AgentAdapter } from '../catalog'

/** Pi non ha un meccanismo di hook: Tiller guarda il codice di uscita del pane. */
export const piAdapter: AgentAdapter = {
  id: 'pi',
  displayName: 'Pi',
  hasNativeHooks: false,

  async prepare() {},

  command() {
    return 'pi'
  },

  resumeCommand({ sessionRef }) {
    return `pi --session ${shellQuote(sessionRef)}`
  },

  summarizerCommand(prompt) {
    return `pi --print --no-tools ${shellQuote(prompt)}`
  }
}
```

`src/main/agents/adapters/omp.ts`:

```ts
import { shellQuote } from '../quote'
import type { AgentAdapter, LaunchOptions } from '../catalog'

function hookPath({ worktreePath }: LaunchOptions): string {
  return shellQuote(`${worktreePath}/.tiller/omp-hook.ts`)
}

export const ompAdapter: AgentAdapter = {
  id: 'omp',
  displayName: 'Oh-My-Pi',
  hasNativeHooks: true,

  async prepare() {},

  command(options) {
    return `omp --hook ${hookPath(options)}`
  },

  resumeCommand(options) {
    return `omp --hook ${hookPath(options)} --resume=${shellQuote(options.sessionRef)}`
  },

  summarizerCommand(prompt) {
    return `omp --print --no-tools ${shellQuote(prompt)}`
  }
}
```

- [ ] **Step 5: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- src/main/agents/`
Atteso: PASS, 18 test.

- [ ] **Step 6: commit**

```bash
git add src/main/agents/
git commit -m "feat: i cinque adapter agenti con prepare no-op"
```

---

### Task 11: estensione del protocollo e del dispatcher

**File:**
- Modifica: `src/shared/protocol.ts`
- Modifica: `src/main/control/dispatch.ts`
- Modifica: `src/main/control/socket-server.ts`, `src/main/control/ipc-transport.ts`, `src/main/index.ts`
- Test: `src/main/control/dispatch.test.ts` (esistente, esteso)

**Interfacce:**
- Consuma: tutto quanto prodotto dai task 1–10.
- Produce: `createDispatcher(deps)` che ora restituisce
  `(request: ControlRequest) => Promise<ControlResponse>`;
  `DispatchDeps` esteso con `db: Kysely<TillerDatabase>` e `newId(): string`.

> **Cambio di firma con effetti a catena.** Le operazioni git e le query sono
> asincrone, quindi il dispatcher passa da sincrono a `Promise`. Chi lo chiama
> — `socket-server.ts`, `ipc-transport.ts` — deve attendere il risultato. È il
> punto più delicato del task: se un chiamante dimentica l'`await`, serializza
> una promise al posto della risposta e il client riceve `{}`.

- [ ] **Step 1: estendere `src/shared/protocol.ts`**

Aggiungere alla union `ControlRequest`, dopo `window.focus`:

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('project.add'),
    params: z.object({ rootPath: z.string().min(1), name: z.string().optional() })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('project.list'),
    params: z.object({})
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('project.remove'),
    params: z.object({ projectId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('worktree.list'),
    params: z.object({ projectId: z.string().optional() })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('worktree.create'),
    params: z.object({
      projectId: z.string().min(1),
      branch: z.string().min(1),
      base: z.string().optional(),
      path: z.string().optional()
    })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('worktree.remove'),
    params: z.object({ worktreeId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('git.status'),
    params: z.object({ worktreeId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('agent.list'),
    params: z.object({})
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('db.import'),
    params: z.object({ fromPath: z.string().min(1) })
  })
```

Ed estendere i parametri di `pane.create` con l'agente:

```ts
    params: z.object({
      cwd: z.string().min(1),
      focus: z.boolean().default(false),
      agent: z.string().optional(),
      worktreeId: z.string().optional()
    })
```

- [ ] **Step 2: scrivere i test del dispatcher**

Aggiungere a `src/main/control/dispatch.test.ts`. Il file esistente costruisce
già delle `deps` finte: estenderle con un database vero in memoria.

```ts
import { expect, test } from 'vitest'
import { mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { execFileSync } from 'node:child_process'
import { openDatabase, closeDatabase, migrateToLatest } from '../db/database'
import { createDispatcher } from './dispatch'
import { AppState } from '../state/app-state'

function makeGitRepo(): string {
  const dir = mkdtempSync(join(tmpdir(), 'tiller-disp-'))
  const run = (...args: string[]): void => {
    execFileSync('git', args, { cwd: dir, stdio: 'pipe' })
  }
  run('init', '--initial-branch=main')
  run('config', 'user.email', 'test@example.com')
  run('config', 'user.name', 'Test')
  execFileSync('sh', ['-c', 'echo x > f.txt'], { cwd: dir })
  run('add', 'f.txt')
  run('commit', '-m', 'primo')
  return dir
}

async function makeDispatcher() {
  const db = openDatabase(':memory:')
  await migrateToLatest(db)
  let counter = 0
  const dispatch = createDispatcher({
    state: new AppState(),
    pty: { spawn: () => {}, write: () => {}, kill: () => {} } as never,
    window: { focus: () => {}, isOpen: () => true },
    now: () => 1000,
    newPaneId: () => `pane-${++counter}`,
    newId: () => `id-${++counter}`,
    defaultShell: '/bin/sh',
    db
  })
  return { db, dispatch }
}

test('project.add registra un progetto e lo restituisce in lista', async () => {
  const { db, dispatch } = await makeDispatcher()
  const repo = makeGitRepo()

  const added = await dispatch({
    id: 'r1', method: 'project.add', params: { rootPath: repo }
  })
  expect(added.ok).toBe(true)

  const listed = await dispatch({ id: 'r2', method: 'project.list', params: {} })
  expect(listed.ok).toBe(true)
  const result = (listed as { result: { projects: { rootPath: string }[] } }).result
  expect(result.projects).toHaveLength(1)

  await closeDatabase(db)
  rmSync(repo, { recursive: true, force: true })
})

test('project.add rifiuta una cartella che non e un repository', async () => {
  const { db, dispatch } = await makeDispatcher()
  const plain = mkdtempSync(join(tmpdir(), 'tiller-plain-'))

  const response = await dispatch({
    id: 'r1', method: 'project.add', params: { rootPath: plain }
  })
  expect(response.ok).toBe(false)
  expect((response as { error: string }).error).toMatch(/non e un repository/)

  await closeDatabase(db)
  rmSync(plain, { recursive: true, force: true })
})

test('worktree.create crea il worktree su disco e lo persiste', async () => {
  const { db, dispatch } = await makeDispatcher()
  const repo = makeGitRepo()
  const added = await dispatch({ id: 'r1', method: 'project.add', params: { rootPath: repo } })
  const projectId = (added as { result: { projectId: string } }).result.projectId

  const created = await dispatch({
    id: 'r2', method: 'worktree.create', params: { projectId, branch: 'feature/x' }
  })
  expect(created.ok).toBe(true)

  const listed = await dispatch({ id: 'r3', method: 'worktree.list', params: {} })
  const worktrees = (listed as { result: { worktrees: unknown[] } }).result.worktrees
  expect(worktrees).toHaveLength(1)

  await closeDatabase(db)
  rmSync(repo, { recursive: true, force: true })
})

test('agent.list restituisce i cinque agenti', async () => {
  const { db, dispatch } = await makeDispatcher()
  const response = await dispatch({ id: 'r1', method: 'agent.list', params: {} })
  const agents = (response as { result: { agents: { id: string }[] } }).result.agents
  expect(agents.map((a) => a.id)).toEqual(['claude', 'codex', 'opencode', 'pi', 'omp'])
  await closeDatabase(db)
})

test('git.status su un worktree sconosciuto risponde con errore, non lancia', async () => {
  const { db, dispatch } = await makeDispatcher()
  const response = await dispatch({
    id: 'r1', method: 'git.status', params: { worktreeId: 'assente' }
  })
  expect(response.ok).toBe(false)
  expect((response as { error: string }).error).toMatch(/worktree sconosciuto/)
  await closeDatabase(db)
})
```

- [ ] **Step 3: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- src/main/control/dispatch.test.ts`
Atteso: FAIL, i nuovi metodi non sono gestiti.

- [ ] **Step 4: estendere `src/main/control/dispatch.ts`**

Aggiungere gli import di tipo e i nuovi campi in `DispatchDeps`:

```ts
import type { Kysely } from 'kysely'
import type { TillerDatabase } from '../db/schema'
import {
  addProject, listProjects, removeProject,
  addWorktree, listWorktrees, removeWorktree, findWorktree
} from '../db/repository'
import { isGitRepository } from '../git/repo'
import {
  addWorktree as addGitWorktree,
  removeWorktree as removeGitWorktree
} from '../git/worktrees'
import { readStatus } from '../git/status'
import { AGENT_CATALOG, findAdapter } from '../agents/catalog'
import { importFromSwift } from '../import/swift-import'
import { basename, join } from 'node:path'

export interface DispatchDeps {
  state: AppState
  pty: PtyManager
  window: WindowController
  now(): number
  newPaneId(): string
  newId(): string
  defaultShell: string
  db: Kysely<TillerDatabase>
}
```

Cambiare la firma restituita e aggiungere i casi:

```ts
export function createDispatcher(
  deps: DispatchDeps
): (request: ControlRequest) => Promise<ControlResponse> {
  return async (request: ControlRequest): Promise<ControlResponse> => {
    try {
      switch (request.method) {
        // … i casi esistenti restano invariati, tranne pane.create (sotto)

        case 'project.add': {
          const rootPath = request.params.rootPath
          if (!(await isGitRepository(rootPath))) {
            return { id: request.id, ok: false, error: `non e un repository git: ${rootPath}` }
          }
          const projectId = deps.newId()
          await addProject(deps.db, {
            id: projectId,
            name: request.params.name ?? basename(rootPath),
            rootPath,
            createdAt: new Date(deps.now()).toISOString()
          })
          return { id: request.id, ok: true, result: { projectId } }
        }

        case 'project.list': {
          return { id: request.id, ok: true, result: { projects: await listProjects(deps.db) } }
        }

        case 'project.remove': {
          const removed = await removeProject(deps.db, request.params.projectId)
          return { id: request.id, ok: true, result: { removed } }
        }

        case 'worktree.list': {
          const worktrees = await listWorktrees(deps.db, request.params.projectId)
          return { id: request.id, ok: true, result: { worktrees } }
        }

        case 'worktree.create': {
          const projects = await listProjects(deps.db)
          const project = projects.find((p) => p.id === request.params.projectId)
          if (project === undefined) {
            return {
              id: request.id, ok: false,
              error: `progetto sconosciuto: ${request.params.projectId}`
            }
          }
          const path =
            request.params.path ??
            join(project.rootPath, '..', `${basename(project.rootPath)}-${deps.newId()}`)

          // Prima git, poi il database: se git fallisce non resta un record
          // che punta a un worktree inesistente.
          await addGitWorktree(project.rootPath, {
            branch: request.params.branch,
            path,
            base: request.params.base
          })
          const worktreeId = deps.newId()
          await addWorktree(deps.db, {
            id: worktreeId,
            projectId: project.id,
            branch: request.params.branch,
            path,
            createdAt: new Date(deps.now()).toISOString(),
            isPrimary: false
          })
          return { id: request.id, ok: true, result: { worktreeId, path } }
        }

        case 'worktree.remove': {
          const worktree = await findWorktree(deps.db, request.params.worktreeId)
          if (worktree === undefined) {
            return {
              id: request.id, ok: false,
              error: `worktree sconosciuto: ${request.params.worktreeId}`
            }
          }
          const projects = await listProjects(deps.db)
          const project = projects.find((p) => p.id === worktree.projectId)
          if (project !== undefined) {
            await removeGitWorktree(project.rootPath, worktree.path)
          }
          const removed = await removeWorktree(deps.db, worktree.id)
          return { id: request.id, ok: true, result: { removed } }
        }

        case 'git.status': {
          const worktree = await findWorktree(deps.db, request.params.worktreeId)
          if (worktree === undefined) {
            return {
              id: request.id, ok: false,
              error: `worktree sconosciuto: ${request.params.worktreeId}`
            }
          }
          return { id: request.id, ok: true, result: await readStatus(worktree.path) }
        }

        case 'agent.list': {
          const agents = AGENT_CATALOG.map((a) => ({
            id: a.id,
            displayName: a.displayName,
            hasNativeHooks: a.hasNativeHooks
          }))
          return { id: request.id, ok: true, result: { agents } }
        }

        case 'db.import': {
          const report = await importFromSwift(deps.db, request.params.fromPath)
          return { id: request.id, ok: true, result: report }
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

Modificare il caso `pane.create` esistente per usare l'adapter quando richiesto:

```ts
        case 'pane.create': {
          const pane = {
            id: deps.newPaneId(),
            cwd: request.params.cwd,
            createdAt: deps.now()
          }

          // Con --agent il pane esegue il comando dell'adapter invece della
          // shell interattiva. tillerctlPath resta il binario corrente: gli
          // hook non vengono scritti in Fase 1, ma il comando lo cita
          // comunque (Codex lo passa come override, non come file).
          const adapterId = request.params.agent
          let args: string[] = []
          if (adapterId !== undefined) {
            const adapter = findAdapter(adapterId)
            if (adapter === undefined) {
              return { id: request.id, ok: false, error: `agente sconosciuto: ${adapterId}` }
            }
            const command = adapter.command({
              worktreePath: pane.cwd,
              paneId: pane.id,
              tillerctlPath: process.argv[1] ?? 'tillerctl'
            })
            args = ['-lc', command]
          }

          deps.pty.spawn(pane.id, {
            cwd: pane.cwd,
            shell: deps.defaultShell,
            args,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS
          })
          deps.state.addPane(pane)
          if (request.params.focus) deps.window.focus()
          return { id: request.id, ok: true, result: { paneId: pane.id } }
        }
```

- [ ] **Step 5: aggiornare i chiamanti del dispatcher**

In `src/main/control/socket-server.ts` e `src/main/control/ipc-transport.ts`, il
risultato di `dispatch(...)` va atteso. Cercare ogni chiamata e anteporre
`await`, rendendo `async` il callback che la contiene. In `src/main/index.ts`
aggiungere l'apertura del database e la migrazione prima di costruire le
dipendenze:

```ts
import { openDatabase, migrateToLatest } from './db/database'
import { resolveDatabasePath } from '../shared/paths'

const db = openDatabase(resolveDatabasePath())
await migrateToLatest(db)
```

e passare `db` e `newId: () => randomUUID()` a `createDispatcher`.

- [ ] **Step 6: eseguire l'intero gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`. Se il typecheck segnala una promise non attesa in
`socket-server.ts` o `ipc-transport.ts`, è esattamente il caso avvertito sopra:
aggiungere l'`await` mancante.

- [ ] **Step 7: commit**

```bash
git add src/shared/protocol.ts src/main/control/ src/main/index.ts
git commit -m "feat: metodi di controllo per progetti, worktree, git e agenti"
```

---

### Task 12: comandi di tillerctl

**File:**
- Modifica: `cli/args.ts`, `cli/tillerctl.ts`
- Test: `cli/args.test.ts` (esistente, esteso)

**Interfacce:**
- Consuma: la union `ControlRequest` estesa nel task 11.
- Produce: i comandi `project-add`, `project-list`, `project-remove`,
  `worktree-list`, `worktree-create`, `worktree-remove`, `status`, `agents`,
  `import`.

- [ ] **Step 1: scrivere i test degli argomenti**

Aggiungere a `cli/args.test.ts`:

```ts
import { expect, test } from 'vitest'
import { parseCommand } from './args.ts'

test('project-add costruisce la richiesta con il percorso', () => {
  const request = parseCommand(['project-add', '--path', '/tmp/repo'])
  expect(request.method).toBe('project.add')
  expect(request.params).toEqual({ rootPath: '/tmp/repo' })
})

test('project-add accetta un nome facoltativo', () => {
  const request = parseCommand(['project-add', '--path', '/tmp/repo', '--name', 'Tiller'])
  expect(request.params).toEqual({ rootPath: '/tmp/repo', name: 'Tiller' })
})

test('worktree-create richiede progetto e branch', () => {
  const request = parseCommand([
    'worktree-create', '--project', 'p1', '--branch', 'feature/x'
  ])
  expect(request.method).toBe('worktree.create')
  expect(request.params).toEqual({ projectId: 'p1', branch: 'feature/x' })
})

test('worktree-create senza branch fallisce con un messaggio utile', () => {
  expect(() => parseCommand(['worktree-create', '--project', 'p1'])).toThrow(/--branch/)
})

test('agents non richiede argomenti', () => {
  expect(parseCommand(['agents']).method).toBe('agent.list')
})

test('import richiede il percorso sorgente', () => {
  const request = parseCommand(['import', '--from', '/tmp/tiller.sqlite'])
  expect(request.method).toBe('db.import')
  expect(request.params).toEqual({ fromPath: '/tmp/tiller.sqlite' })
})

test('run accetta --agent', () => {
  const request = parseCommand(['run', '--cwd', '/tmp/wt', '--agent', 'codex'])
  expect(request.method).toBe('pane.create')
  expect(request.params).toMatchObject({ cwd: '/tmp/wt', agent: 'codex' })
})

test('un comando sconosciuto elenca quelli disponibili', () => {
  expect(() => parseCommand(['inventato'])).toThrow(/comando sconosciuto/)
})
```

- [ ] **Step 2: eseguire e verificare il fallimento**

Comando: `pnpm test:unit -- cli/args.test.ts`
Atteso: FAIL, i nuovi comandi non sono riconosciuti.

- [ ] **Step 3: estendere `cli/args.ts`**

Aggiungere i comandi alla funzione di parsing esistente, usando `parseArgs` di
`node:util` come già fa il file. Ogni comando dichiara le proprie opzioni e
solleva un errore che nomina l'opzione mancante:

```ts
// Estratto: lo schema per i nuovi comandi. Va integrato nella struttura
// gia' presente in args.ts, non sostituito.
const COMMANDS = {
  'project-add': {
    method: 'project.add',
    options: { path: { type: 'string' }, name: { type: 'string' } },
    required: ['path'],
    build: (v: Record<string, string>) => ({
      rootPath: v.path,
      ...(v.name === undefined ? {} : { name: v.name })
    })
  },
  'project-list': { method: 'project.list', options: {}, required: [], build: () => ({}) },
  'project-remove': {
    method: 'project.remove',
    options: { project: { type: 'string' } },
    required: ['project'],
    build: (v: Record<string, string>) => ({ projectId: v.project })
  },
  'worktree-list': {
    method: 'worktree.list',
    options: { project: { type: 'string' } },
    required: [],
    build: (v: Record<string, string>) =>
      v.project === undefined ? {} : { projectId: v.project }
  },
  'worktree-create': {
    method: 'worktree.create',
    options: {
      project: { type: 'string' }, branch: { type: 'string' },
      base: { type: 'string' }, path: { type: 'string' }
    },
    required: ['project', 'branch'],
    build: (v: Record<string, string>) => ({
      projectId: v.project,
      branch: v.branch,
      ...(v.base === undefined ? {} : { base: v.base }),
      ...(v.path === undefined ? {} : { path: v.path })
    })
  },
  'worktree-remove': {
    method: 'worktree.remove',
    options: { worktree: { type: 'string' } },
    required: ['worktree'],
    build: (v: Record<string, string>) => ({ worktreeId: v.worktree })
  },
  status: {
    method: 'git.status',
    options: { worktree: { type: 'string' } },
    required: ['worktree'],
    build: (v: Record<string, string>) => ({ worktreeId: v.worktree })
  },
  agents: { method: 'agent.list', options: {}, required: [], build: () => ({}) },
  import: {
    method: 'db.import',
    options: { from: { type: 'string' } },
    required: ['from'],
    build: (v: Record<string, string>) => ({ fromPath: v.from })
  }
} as const
```

Il comando `run` esistente guadagna l'opzione `agent: { type: 'string' }`, che
finisce nei params solo quando presente.

Gli errori per opzione mancante devono citare l'opzione con il trattino
davanti — `--branch` e non `branch` — perché il messaggio è ciò che l'utente
legge nel terminale. Il comando sconosciuto elenca i comandi disponibili.

- [ ] **Step 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- cli/`
Atteso: PASS.

- [ ] **Step 5: verifica manuale a mano libera**

```bash
cd ~/Desktop/Progetti/tiller-electron
pnpm tillerctl agents
```

Con l'app spenta deve rispondere `Tiller non è in ascolto su …`, non con uno
stack trace.

- [ ] **Step 6: commit**

```bash
git add cli/
git commit -m "feat: comandi tillerctl per progetti, worktree e agenti"
```

---

### Task 13: criteri di successo end-to-end

**File:**
- Crea: `e2e/fase-1-fondamenta.spec.ts`
- Test: sé stesso

**Interfacce:**
- Consuma: tutto. Nessuna produzione per task successivi.

- [ ] **Step 1: scrivere i tre criteri**

Creare `e2e/fase-1-fondamenta.spec.ts`:

```ts
import { test, expect, _electron as electron, type ElectronApplication } from '@playwright/test'
import { execFile, execFileSync } from 'node:child_process'
import { promisify } from 'node:util'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { parse as parseToml } from 'smol-toml'

const run = promisify(execFile)

let app: ElectronApplication
let scratch: string
let socketPath: string
let dbPath: string

test.beforeEach(async () => {
  scratch = await mkdtemp(join(tmpdir(), 'tiller-f1-'))
  socketPath = join(scratch, 'control.sock')
  dbPath = join(scratch, 'tiller.sqlite')
  app = await electron.launch({
    args: ['.'],
    env: { ...process.env, TILLER_SOCKET: socketPath, TILLER_DB: dbPath }
  })
  await app.firstWindow()
})

test.afterEach(async () => {
  await app.close()
  await rm(scratch, { recursive: true, force: true })
})

async function tillerctl(args: string[]): Promise<string> {
  const { stdout } = await run('node', ['cli/tillerctl.ts', ...args], {
    env: { ...process.env, TILLER_SOCKET: socketPath, TILLER_DB: dbPath }
  })
  return stdout.trim()
}

function makeRepo(): string {
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
  return dir
}

test('criterio 1: import fedele e ripetibile', async () => {
  // Costruisce una sorgente con lo schema Swift e due righe correlate.
  const source = join(scratch, 'swift.sqlite')
  execFileSync('node', ['-e', `
    const D = require('better-sqlite3');
    const db = new D(${JSON.stringify(source)});
    db.exec(\`
      CREATE TABLE "project" ("id" TEXT PRIMARY KEY NOT NULL,"name" TEXT NOT NULL,
        "rootPath" TEXT NOT NULL,"createdAt" DATETIME NOT NULL,"colorHex" TEXT,
        "displayName" TEXT,"iconKind" TEXT NOT NULL DEFAULT 'icon',"iconValue" TEXT,
        "avatarImage" BLOB,"defaultWorktreeBase" TEXT,"worktreeLocationOverride" TEXT,
        "orderIdx" INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE "worktree" ("id" TEXT PRIMARY KEY NOT NULL,"projectId" TEXT NOT NULL,
        "branch" TEXT NOT NULL,"path" TEXT NOT NULL,"createdAt" DATETIME NOT NULL,
        "comment" TEXT,"commentUpdatedAt" DATETIME,"isPrimary" BOOLEAN NOT NULL DEFAULT 0,
        "orderIdx" INTEGER NOT NULL DEFAULT 0);
      INSERT INTO project VALUES('p1','Tiller','/tmp/t','2026-01-01T00:00:00Z',
        NULL,NULL,'icon',NULL,NULL,NULL,NULL,0);
      INSERT INTO worktree VALUES('w1','p1','main','/tmp/t','2026-01-01T00:00:00Z',
        NULL,NULL,1,0);
    \`);
    db.close();
  `], { cwd: process.cwd() })

  const first = await tillerctl(['import', '--from', source])
  expect(first).toContain('project')

  const listed = JSON.parse(await tillerctl(['project-list']))
  expect(listed.projects).toHaveLength(1)

  // Rieseguito, non duplica.
  await tillerctl(['import', '--from', source])
  const again = JSON.parse(await tillerctl(['project-list']))
  expect(again.projects).toHaveLength(1)
})

test('criterio 2: worktree reale, verificato da git stesso', async () => {
  const repo = makeRepo()
  const added = JSON.parse(await tillerctl(['project-add', '--path', repo]))
  const created = JSON.parse(
    await tillerctl(['worktree-create', '--project', added.projectId, '--branch', 'feature/e2e'])
  )

  // Oracolo esterno: git, non il nostro parser. Se lo stato vivesse solo in
  // memoria o il worktree non fosse creato davvero, questa riga cadrebbe.
  const raw = execFileSync('git', ['worktree', 'list', '--porcelain'], {
    cwd: repo,
    encoding: 'utf8'
  })
  expect(raw).toContain('refs/heads/feature/e2e')
  expect(created.path.length).toBeGreaterThan(0)

  // Il record sopravvive al riavvio dell'app: si chiude e si riapre sullo
  // stesso database.
  await app.close()
  app = await electron.launch({
    args: ['.'],
    env: { ...process.env, TILLER_SOCKET: socketPath, TILLER_DB: dbPath }
  })
  await app.firstWindow()

  const listed = JSON.parse(await tillerctl(['worktree-list']))
  expect(listed.worktrees).toHaveLength(1)
  expect(listed.worktrees[0].branch).toBe('feature/e2e')
})

test('criterio 3: il comando di Codex contiene TOML che si riparsa', async () => {
  const agents = JSON.parse(await tillerctl(['agents']))
  expect(agents.agents.map((a: { id: string }) => a.id)).toContain('codex')

  // Il comando si verifica direttamente sull'adapter: e' la stessa funzione
  // che il dispatcher usa per lanciare il pane.
  const { codexAdapter } = await import('../src/main/agents/adapters/codex')
  const command = codexAdapter.command({
    worktreePath: '/tmp/wt',
    paneId: 'abc-123',
    tillerctlPath: '/Users/x/bin/tillerctl'
  })
  const match = /-c '(.+)'$/.exec(command)
  expect(match).not.toBeNull()
  const parsed = parseToml(match?.[1] ?? '') as { notify: string[] }
  expect(parsed.notify[0]).toBe('/Users/x/bin/tillerctl')
  expect(parsed.notify).toContain('abc-123')
})
```

- [ ] **Step 2: eseguire e verificare il fallimento su un criterio**

Comando: `pnpm test:e2e -- e2e/fase-1-fondamenta.spec.ts`
Se tutti e tre passano al primo colpo, verificare che siano davvero
falsificabili prima di proseguire: vedi step 3.

- [ ] **Step 3: validare per mutazione**

Rompere temporaneamente `tomlStringLiteral` in `src/main/agents/quote.ts`
facendogli scappare le barre:

```ts
export function tomlStringLiteral(value: string): string {
  return JSON.stringify(value).replaceAll('/', '\\/')
}
```

Rieseguire il criterio 3: deve fallire. Poi ripristinare l'originale e
verificare che torni verde. Un test che non fallisce quando si rompe il codice
che sorveglia non sta sorvegliando nulla.

- [ ] **Step 4: eseguire il gate completo**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Step 5: commit**

```bash
git add e2e/fase-1-fondamenta.spec.ts
git commit -m "test: criteri end-to-end della Fase 1"
```

---

## Autorevisione

**Copertura della spec.** Ogni requisito ha il suo task: driver e query builder
(1), migrazione (2), repository (3), import con idempotenza e conteggi (4), git
completo (5–8), quoting TOML (9), i cinque adapter con `prepare()` no-op (10),
superficie di controllo (11), tillerctl (12), i tre criteri di successo (13).

**Scostamenti dalla spec, deliberati e motivati nel testo:**

1. `src/main/state/` non viene esteso con progetti e worktree; il dispatcher li
   legge dal database. Motivo nella sezione "Nota di raffinamento".
2. `smol-toml` è una dipendenza di sviluppo, non di produzione:
   `JSON.stringify` di JavaScript non scappa le barre, quindi il letterale TOML
   è una riga di stdlib e la libreria serve come oracolo nei test. È un
   miglioramento rispetto alla spec, che la prevedeva in produzione.

**Nomi che collidono.** `listWorktrees`, `addWorktree` e `removeWorktree`
esistono sia in `db/repository.ts` sia in `git/worktrees.ts` con firme diverse.
Il task 11 le importa entrambe rinominando quelle git (`addGitWorktree`,
`removeGitWorktree`); l'avvertenza è ripetuta nel task 6.

---

## Esito dell'esecuzione (2026-08-01)

**13/13 task, 19 commit, 175 test unitari + 5 e2e, `bash scripts/ci.sh` → `CI OK`.**

Eseguito delegando a codex `gpt-5.6-luna` un task alla volta, con revisione del
diff — non del report — fra un task e l'altro.

### Correzioni al piano trovate eseguendolo

1. **`Migrator` non si importa da `kysely`.** kysely 0.29 lo espone dal
   sottopercorso `./migration`, insieme a `Migration` e `FileMigrationProvider`.
   Il piano indicava la radice e i quattro test della migrazione erano rossi.
2. **`simple-git` non veniva installato da nessun task.** L'unico `pnpm add`
   stava nel Task 1, scritto pensando alla sola persistenza. Il Task 5 falliva
   con `Cannot find package`.
3. **`hasNativeHooks` di Claude è `true`**, non `false`: l'asserzione del piano
   ne attendeva due (codex, omp) invece di tre.
4. **Escape sbagliato nel valore atteso del comando di riassunto.** Scritto come
   `"claude -p 'riassumi '\''questo'\'''"` in una stringa a doppi apici, dove
   `\'` non è un escape di JavaScript e collassa in `'`. Risolto con
   `String.raw`. È lo stesso errore di livello che il codice sotto test esiste
   per evitare, commesso nel test che lo verifica.
5. **La fixture e2e dell'import creava due tabelle su undici.** L'import legge
   ogni tabella di `TABLES_IN_DEPENDENCY_ORDER`, quindi moriva con
   `no such table: terminalContent`. Corretta la fixture, non l'import:
   verificato che il database di produzione e il suo backup più vecchio
   contengono tutte le tabelle, quindi una sorgente incompleta deve fallire
   forte anziché saltare righe in silenzio.
6. **Il codice trascritto dal piano non era formattato** e `migratedDb` non
   dichiarava il tipo di ritorno: 48 avvisi prettier e un errore
   `explicit-function-return-type`. Il lint è rimasto rosso per cinque task
   perché i comandi di prova dei task 2–10 sono mirati alla propria area e solo
   il Task 11 guarda il gate intero.
7. **I cinque `prepare()` vuoti violano `no-empty-function`.** Annotati con
   `eslint-disable-next-line` e un commento che rimanda alla decisione D4,
   invece di riempirli di codice finto.

### Verifiche eseguite in proprio, oltre ai report

- **Import contro il database di produzione reale** (12,1 MB): conteggi identici
  su tutte e 11 le tabelle — 10 progetti, 9 worktree, 121 scrollback, 62
  sessioni di chat, 1000 chatItem, 9 layout, 8 tab, 2 terminalContent — e la
  seconda esecuzione non duplica. La tabella `legacyTerminalTab_v15`, presente
  nella sorgente ma non nel nostro schema, viene ignorata senza inciampi.
- **Validazione per mutazione** di quattro guardie: il test di packaging di
  `better-sqlite3` (binario rimosso → rosso), la validazione dei percorsi in
  `git/actions.ts` (guardia disattivata → cadono i 3 test sui percorsi), la
  guardia su `prepare()` (fatto scrivere un file → rosso), il criterio 2 e2e
  (creazione git del worktree disattivata → rosso).
- **Confine architetturale**: `control/dispatch.ts` continua a non importare né
  `electron` né `net` dopo l'aggiunta di nove metodi.

### Trappola da ricordare

**La validazione per mutazione dei test e2e richiede una ricompilazione.**
Playwright avvia l'app da `out/main/index.js`, il bundle prodotto da
electron-vite, non dai sorgenti: mutare `src/` senza `pnpm exec electron-vite
build` fa girare i test sul codice vecchio, tutto resta verde e si conclude a
torto che il test non sorvegli nulla. Il gate `ci.sh` è immune perché costruisce
prima di eseguire l'e2e.

### Debito aperto

Il warning Node `MODULE_TYPELESS_PACKAGE_JSON` a ogni invocazione di
`tillerctl` (tre righe su stderr, cosmetico), ereditato dalla Fase 0.
