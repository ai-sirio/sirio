# Fase 6b-1 — Leggere: piano di implementazione

> **Per chi esegue:** SKILL RICHIESTA: usare `superpowers:subagent-driven-development`
> per implementare questo piano un task alla volta. I passi usano caselle
> (`- [ ]`) per il tracciamento.

**Obiettivo:** navigare e leggere qualsiasi file del worktree con evidenziazione
sintattica, e portare la stessa evidenziazione dentro il diff della Fase 6a.

**Architettura:** la logica pura (tabelle icone, risoluzione linguaggio) sta in
`src/shared/files/`; l'accesso al disco in `src/main/files/`; la tokenizzazione
nel renderer, unico consumatore. CodeMirror 6 è il motore unico: `readOnly` per
il visualizzatore, `highlightTree` per il diff senza istanziare un editor.

**Stack:** TypeScript, Svelte 5 (rune), zod, CodeMirror 6, chokidar, vitest,
Playwright.

**Spec:** `docs/superpowers/specs/2026-08-02-electron-fase-6b1-lettura-design.md`

## Vincoli globali

- **Repo di lavoro:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
  `~/Desktop/Progetti/tiller` si **legge soltanto**, mai si modifica.
- **`.tokensave/` non si committa mai** e non si ripristina.
- **Non toccare** `~/.zshrc` né l'installazione di pyenv. Gli aggiustamenti di
  ambiente vanno nei test.
- **Non eseguire `bash scripts/ci.sh`.** Il gate completo lo esegue Claude. La
  prova richiesta a fine task è: `npx vitest run <percorsi del task>`,
  `pnpm typecheck`, `pnpm svelte-check`.
- **Committare prima di mutare.** Le mutazioni di verifica si annullano con
  `git checkout --`, che riporta a HEAD: se il lavoro non è committato, si perde.
- **Ogni campo nuovo su uno schema zod che viaggia sul socket** deve avere
  `.default(...)`. Lo schema è un contratto di rete oltre che un tipo: un campo
  obbligatorio nuovo rifiuta ogni chiamante esistente, e il compilatore non lo
  vede perché i payload sono JSON. Aggiungere un **membro** a un'unione
  discriminata è invece additivo e sicuro.
- **Stringhe rivolte all'utente in inglese**, sempre, anche se i commenti e
  questo piano sono in italiano.
- **Commit** in [Conventional Commits](https://www.conventionalcommits.org/),
  soggetto imperativo minuscolo, in inglese.
- Helper di test per repo git temporanei: **`createTestRepo`** (non
  `makeTestRepo`), già presente in `src/main/git/*.test.ts`.
- `rtk` intercetta `grep` e `rg` in questo ambiente: usare `awk`/`sed`.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/shared/files/types.ts` | schemi zod `FileNode`, `FileContent` |
| `src/shared/files/icon-key.ts` | nome/estensione → chiave icona (porta `FileIconKey.swift`) |
| `src/shared/files/language.ts` | estensione → linguaggio CodeMirror (porta `CodeLanguageResolver.swift`) |
| `src/main/files/paths.ts` | confinamento alla radice, errori tipizzati |
| `src/main/files/tree.ts` | elenco dei figli di **una** cartella |
| `src/main/files/read.ts` | lettura con limiti e rilevamento binario |
| `src/main/files/watch.ts` | chokidar sulla radice → evento `files.changed` |
| `src/renderer/src/lib/workspace/highlight.ts` | `highlightTree` → intervalli per riga |
| `src/renderer/src/lib/workspace/FilesTree.svelte` | albero, espansione pigra, icone |
| `src/renderer/src/lib/workspace/FileTab.svelte` | CodeMirror in sola lettura |
| `src/renderer/src/lib/workspace/file-icon.ts` | chiave icona → nome Iconify |

---

## Task 1: Schemi condivisi dei file

**File:**
- Crea: `src/shared/files/types.ts`
- Test: `src/shared/files/types.test.ts`

**Interfacce:**
- Produce: `FileNodeSchema`, `FileNode`, `FileNodeKind`, `FileContentSchema`,
  `FileContent`. Li consumano i Task 4, 5, 7, 10, 13.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { describe, expect, test } from 'vitest'
import { FileContentSchema, FileNodeSchema } from './types.ts'

describe('FileNodeSchema', () => {
  test('accetta una cartella con percorso relativo annidato', () => {
    const parsed = FileNodeSchema.parse({
      relativePath: 'src/main',
      name: 'main',
      kind: 'directory'
    })
    expect(parsed.kind).toBe('directory')
  })

  test('rifiuta un genere sconosciuto', () => {
    expect(() => FileNodeSchema.parse({ relativePath: 'a', name: 'a', kind: 'socket' })).toThrow()
  })

  test('rifiuta un nome vuoto', () => {
    expect(() => FileNodeSchema.parse({ relativePath: '', name: '', kind: 'file' })).toThrow()
  })
})

describe('FileContentSchema', () => {
  test('un file binario arriva senza testo ma non troncato', () => {
    const parsed = FileContentSchema.parse({ text: '', isBinary: true, truncated: false })
    expect(parsed.isBinary).toBe(true)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

`npx vitest run src/shared/files/types.test.ts`
Atteso: FAIL, `Cannot find module './types.ts'`.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'

/**
 * Porta FileTreeNodeKind da Packages/TillerCore/Sources/TillerCore/FileTree.swift.
 * Un collegamento simbolico e' un genere a se': si mostra, ma non si segue
 * durante l'elenco, altrimenti un anello di collegamenti farebbe girare
 * l'albero all'infinito.
 */
export const FileNodeKindSchema = z.enum(['directory', 'file', 'symlink'])
export type FileNodeKind = z.infer<typeof FileNodeKindSchema>

export const FileNodeSchema = z.object({
  /**
   * Relativo alla radice del worktree, mai vuoto: la radice non e' un nodo,
   * e' il contenitore. La stringa vuota si usa come PARAMETRO per chiedere i
   * figli della radice, non come valore di un nodo.
   */
  relativePath: z.string().min(1),
  name: z.string().min(1),
  kind: FileNodeKindSchema
})
export type FileNode = z.infer<typeof FileNodeSchema>

export const FileContentSchema = z.object({
  text: z.string(),
  isBinary: z.boolean(),
  /** Il contenuto e' stato tagliato per limite di byte o di righe. */
  truncated: z.boolean()
})
export type FileContent = z.infer<typeof FileContentSchema>
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

`npx vitest run src/shared/files/types.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/files/
git commit -m "feat: shared schemas for file nodes and contents"
```

---

## Task 2: Tabelle delle icone

**File:**
- Crea: `src/shared/files/icon-key.ts`
- Test: `src/shared/files/icon-key.test.ts`
- Legge (sola lettura): `~/Desktop/Progetti/tiller/Packages/TillerCore/Sources/TillerCore/FileIconKey.swift`

**Interfacce:**
- Produce: `FileIconKey` (unione di stringhe), `iconKeyForFileName(name)`,
  `iconKeyForDirectoryName(name)`. Li consuma il Task 11.

Porta le tabelle **così come sono**. Sono dati, non meccanica: la ricerca è a
due stadi (nome esatto prima, poi estensione) proprio perché aggiungere un tema
costi una tabella e non una voce per estensione.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { describe, expect, test } from 'vitest'
import { iconKeyForDirectoryName, iconKeyForFileName } from './icon-key.ts'

describe('iconKeyForFileName', () => {
  test('il nome esatto vince sull estensione', () => {
    expect(iconKeyForFileName('Dockerfile')).toBe('docker')
    expect(iconKeyForFileName('.gitignore')).toBe('git')
  })

  test('l estensione non distingue maiuscole', () => {
    expect(iconKeyForFileName('App.TS')).toBe('typescript')
    expect(iconKeyForFileName('main.swift')).toBe('swift')
  })

  test('un estensione sconosciuta ripiega su file', () => {
    expect(iconKeyForFileName('note.qwerty')).toBe('file')
  })

  test('un nome senza estensione ripiega su file', () => {
    expect(iconKeyForFileName('CHANGELOG')).toBe('file')
  })
})

describe('iconKeyForDirectoryName', () => {
  test('riconosce le cartelle note senza distinguere maiuscole', () => {
    expect(iconKeyForDirectoryName('src')).toBe('folderSrc')
    expect(iconKeyForDirectoryName('Tests')).toBe('folderTests')
  })

  test('una cartella sconosciuta ripiega su folder', () => {
    expect(iconKeyForDirectoryName('qwerty')).toBe('folder')
  })
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/shared/files/icon-key.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

Aprire `FileIconKey.swift` e portare le tre tabelle (`exactFileNames`,
`fileExtensions`, `directoryNames`) e i due casi `.enum` in unioni TypeScript.
Struttura:

```ts
export type FileIconKey =
  | 'swift' | 'c' | 'cpp' | 'csharp' | 'java' | 'kotlin' | 'python' | 'ruby'
  | 'rust' | 'go' | 'javascript' | 'typescript' | 'react' | 'vue'
  | 'html' | 'css' | 'sass' | 'json' | 'yaml' | 'toml' | 'xml' | 'markdown'
  | 'text' | 'pdf' | 'image' | 'video' | 'audio' | 'font' | 'archive'
  | 'shell' | 'sql' | 'database' | 'docker' | 'git' | 'log' | 'env'
  | 'settings' | 'lock' | 'makefile'
  | 'folder' | 'folderSrc' | 'folderTests' | 'folderDocs' | 'folderGithub'
  | 'folderNodeModules' | 'folderDist' | 'folderScripts' | 'folderConfig'
  | 'folderAssets' | 'folderPublic' | 'folderPackages' | 'folderVscode'
  | 'folderGit' | 'folderLib' | 'folderTools'
  | 'file' | 'symlink'

const EXACT_FILE_NAMES: Record<string, FileIconKey> = {
  dockerfile: 'docker',
  makefile: 'makefile',
  '.gitignore': 'git',
  '.gitattributes': 'git',
  '.gitmodules': 'git',
  '.env': 'env',
  license: 'text'
  // ... completare dalla tabella Swift
}

const FILE_EXTENSIONS: Record<string, FileIconKey> = {
  swift: 'swift',
  c: 'c', h: 'c',
  cpp: 'cpp', cc: 'cpp', cxx: 'cpp', hpp: 'cpp',
  cs: 'csharp',
  java: 'java',
  kt: 'kotlin', kts: 'kotlin'
  // ... completare dalla tabella Swift
}

const DIRECTORY_NAMES: Record<string, FileIconKey> = {
  // ... completare dalla tabella Swift
}

export function iconKeyForFileName(name: string): FileIconKey {
  const lower = name.toLowerCase()
  const exact = EXACT_FILE_NAMES[lower]
  if (exact !== undefined) return exact
  const dot = lower.lastIndexOf('.')
  // Un nome che inizia con il punto e non ne ha altri (".env" gia' preso sopra)
  // non ha estensione: e' un nome intero.
  if (dot <= 0) return 'file'
  return FILE_EXTENSIONS[lower.slice(dot + 1)] ?? 'file'
}

export function iconKeyForDirectoryName(name: string): FileIconKey {
  return DIRECTORY_NAMES[name.toLowerCase()] ?? 'folder'
}
```

**Attenzione:** `pathExtension` di Swift su `".gitignore"` restituisce stringa
vuota, non `"gitignore"`. La guardia `dot <= 0` riproduce quel comportamento; una
porta ingenua con `split('.').pop()` no, e assegnerebbe icone sbagliate a tutti i
file che iniziano con un punto.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/shared/files/icon-key.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/files/icon-key.ts src/shared/files/icon-key.test.ts
git commit -m "feat: icon keys for file and directory names"
```

---

## Task 3: Risoluzione del linguaggio

**File:**
- Crea: `src/shared/files/language.ts`
- Test: `src/shared/files/language.test.ts`
- Legge: `~/Desktop/Progetti/tiller/Packages/TillerCode/Sources/TillerCode/CodeLanguageResolver.swift`

**Interfacce:**
- Produce: `CodeLanguageId` (unione), `languageForFileName(name): CodeLanguageId | null`.
  Lo consumano i Task 9, 13, 16.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { describe, expect, test } from 'vitest'
import { languageForFileName } from './language.ts'

test('riconosce le estensioni note', () => {
  expect(languageForFileName('app.ts')).toBe('typescript')
  expect(languageForFileName('Component.tsx')).toBe('typescript')
  expect(languageForFileName('main.py')).toBe('python')
  expect(languageForFileName('lib.rs')).toBe('rust')
})

test('non distingue maiuscole', () => {
  expect(languageForFileName('APP.TS')).toBe('typescript')
})

test('un estensione ignota non ha linguaggio', () => {
  expect(languageForFileName('note.qwerty')).toBeNull()
})

test('un file senza estensione non ha linguaggio', () => {
  expect(languageForFileName('CHANGELOG')).toBeNull()
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/shared/files/language.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

```ts
/** I linguaggi per cui esiste un pacchetto @codemirror/lang-*. */
export type CodeLanguageId =
  | 'javascript' | 'typescript' | 'python' | 'rust' | 'go' | 'cpp' | 'java'
  | 'php' | 'html' | 'css' | 'json' | 'markdown' | 'sql' | 'xml' | 'yaml'

const BY_EXTENSION: Record<string, CodeLanguageId> = {
  js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript',
  ts: 'typescript', tsx: 'typescript', mts: 'typescript', cts: 'typescript',
  py: 'python',
  rs: 'rust',
  go: 'go',
  c: 'cpp', h: 'cpp', cpp: 'cpp', cc: 'cpp', hpp: 'cpp',
  java: 'java',
  php: 'php',
  html: 'html', htm: 'html',
  css: 'css', scss: 'css', sass: 'css',
  json: 'json',
  md: 'markdown', markdown: 'markdown',
  sql: 'sql',
  xml: 'xml', svg: 'xml',
  yml: 'yaml', yaml: 'yaml'
}

export function languageForFileName(name: string): CodeLanguageId | null {
  const lower = name.toLowerCase()
  const dot = lower.lastIndexOf('.')
  if (dot <= 0) return null
  return BY_EXTENSION[lower.slice(dot + 1)] ?? null
}
```

**Questo task copre 15 linguaggi. Non è parità: Swift ne copre 42.** Il Task 3b
colma il divario. Non allargare qui la tabella: prima va verificato che i modi
legacy funzionino con `highlightTree`.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/shared/files/language.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/files/language.ts src/shared/files/language.test.ts
git commit -m "feat: resolve a code language from a file name"
```

---

## Task 3b: Colmare il divario di linguaggi

**File:**
- Modifica: `src/shared/files/language.ts`, `src/shared/files/language.test.ts`
- Modifica: `package.json`

**Perché esiste:** il Task 3 copre 15 linguaggi, Tiller Swift ne copre **42** via
`CodeEditLanguages`. Mancano bash, ruby, kotlin, toml, dockerfile, lua, perl,
scala, dart, elixir, haskell, objc e Swift stesso. Un visualizzatore che non
colora uno script bash è una perdita visibile al primo uso.

- [ ] **Passo 1: verificare l'assunzione, prima di costruirci sopra**

```bash
pnpm add @codemirror/legacy-modes
```

Scrivere una sonda usa e getta che tokenizza uno script bash con
`StreamLanguage.define(shell)` e `highlightTree`, e **conta gli intervalli
prodotti**:

```ts
import { StreamLanguage } from '@codemirror/language'
import { shell } from '@codemirror/legacy-modes/mode/shell'
import { classHighlighter, highlightTree } from '@lezer/highlight'

const parser = StreamLanguage.define(shell).parser
const albero = parser.parse('#!/bin/bash\nif [ -f x ]; then echo "ciao"; fi\n')
let intervalli = 0
highlightTree(albero, classHighlighter, () => { intervalli++ })
// intervalli deve essere > 0
```

**Se gli intervalli sono zero, FERMARSI e segnalarlo.** I modi legacy sono a
flusso e non alberi Lezer: la compatibilità con `highlightTree` è plausibile ma
**non verificata**. Non aggirare il problema con un secondo motore.

- [ ] **Passo 2: estendere la tabella e l'unione**

Aggiungere a `CodeLanguageId` e a `BY_EXTENSION` i linguaggi coperti dai modi
legacy: `sh`/`bash`/`zsh` → shell, `rb` → ruby, `kt`/`kts` → kotlin, `toml` →
toml, `lua`, `pl` → perl, `scala`/`sc`, `dart`, `ex`/`exs` → elixir,
`hs` → haskell, `m`/`mm` → objc, `swift`. Il nome esatto `Dockerfile` va gestito
dove si gestiscono i nomi esatti, non fra le estensioni.

- [ ] **Passo 3: estendere i test** con un caso per ogni linguaggio aggiunto, più
  la verifica che i 15 originali non siano cambiati.

- [ ] **Passo 4: eseguire**

`npx vitest run src/shared/files/` → PASS.

- [ ] **Passo 5: committare**

```bash
git add package.json pnpm-lock.yaml src/shared/files/language.ts src/shared/files/language.test.ts
git commit -m "feat: cover the languages missing from the official grammars"
```

**Resta scoperto** ciò che nessuna delle due strade offre (agda, verilog, zig,
julia, ocaml): marginale, si accetta.

**Si perde comunque** il rilevamento dal contenuto che Swift fa sui primi e
ultimi 4 KB (uno shebang in un file senza estensione). La porta è basata solo
sull'estensione: scriverlo, non nasconderlo.

---

## Task 4: Confinamento alla radice

**File:**
- Crea: `src/main/files/paths.ts`
- Test: `src/main/files/paths.test.ts`

**Interfacce:**
- Produce: `FileAccessError` (con `kind: 'pathOutsideRoot' | 'notDirectory'`),
  `resolveInsideRoot(rootPath, relativePath): string`. Lo consumano i Task 5 e 6.

Questa è una **frontiera di fiducia**: il percorso arriva dal renderer o dalla
CLI. Va isolata in un file suo e testata da sola, non annegata dentro `tree.ts`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { describe, expect, test } from 'vitest'
import { FileAccessError, resolveInsideRoot } from './paths.ts'

const ROOT = '/tmp/una-radice'

test('un percorso relativo normale resta dentro', () => {
  expect(resolveInsideRoot(ROOT, 'src/main')).toBe('/tmp/una-radice/src/main')
})

test('la stringa vuota e la radice stessa', () => {
  expect(resolveInsideRoot(ROOT, '')).toBe(ROOT)
})

test('la risalita con .. viene rifiutata', () => {
  expect(() => resolveInsideRoot(ROOT, '../altrove')).toThrow(FileAccessError)
})

test('la risalita mascherata dentro il percorso viene rifiutata', () => {
  expect(() => resolveInsideRoot(ROOT, 'src/../../altrove')).toThrow(FileAccessError)
})

test('un percorso assoluto fuori radice viene rifiutato', () => {
  expect(() => resolveInsideRoot(ROOT, '/etc/passwd')).toThrow(FileAccessError)
})

test('un fratello con lo stesso prefisso non e dentro', () => {
  // "/tmp/una-radice-bis" inizia per "/tmp/una-radice" ma e' un altra cartella.
  expect(() => resolveInsideRoot('/tmp/una-radice', '../una-radice-bis/x')).toThrow(FileAccessError)
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/main/files/paths.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

```ts
import { resolve, sep } from 'node:path'

export type FileAccessErrorKind = 'pathOutsideRoot' | 'notDirectory'

/** Porta FileTreeError da FileTree.swift, messaggi inclusi. */
export class FileAccessError extends Error {
  constructor(
    readonly kind: FileAccessErrorKind,
    readonly path: string
  ) {
    super(
      kind === 'pathOutsideRoot'
        ? `Path escapes the worktree root: ${path}`
        : `Path is not a directory: ${path}`
    )
    this.name = 'FileAccessError'
  }
}

export function resolveInsideRoot(rootPath: string, relativePath: string): string {
  const root = resolve(rootPath)
  const target = resolve(root, relativePath)
  // Il confronto sul separatore non e' pedanteria: senza, "/tmp/radice-bis"
  // passerebbe il controllo su radice "/tmp/radice" per semplice prefisso.
  if (target !== root && !target.startsWith(root + sep)) {
    throw new FileAccessError('pathOutsideRoot', relativePath)
  }
  return target
}
```

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/files/paths.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/files/
git commit -m "feat: confine file access to the worktree root"
```

- [ ] **Passo 6: mutazione di verifica**

Sostituire `!target.startsWith(root + sep)` con `!target.startsWith(root)`.
Atteso: **rosso** su "un fratello con lo stesso prefisso non e dentro".
Annullare con `git checkout -- src/main/files/paths.ts`.

---

## Task 5: Elenco dei figli di una cartella

**File:**
- Crea: `src/main/files/tree.ts`
- Test: `src/main/files/tree.test.ts`

**Interfacce:**
- Consuma: `resolveInsideRoot`, `FileAccessError` (Task 4); `FileNode` (Task 1).
- Produce: `listChildren(rootPath, relativePath): Promise<FileNode[]>`.
  Lo consuma il Task 7.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { mkdtemp, mkdir, writeFile, symlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test } from 'vitest'
import { listChildren } from './tree.ts'

async function albero(): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'tiller-tree-'))
  await mkdir(join(root, 'src', 'annidata'), { recursive: true })
  await mkdir(join(root, '.git'))
  await writeFile(join(root, 'src', 'app.ts'), 'export {}\n')
  await writeFile(join(root, 'radice.txt'), 'ciao\n')
  await symlink(join(root, 'radice.txt'), join(root, 'collegamento'))
  return root
}

test('elenca una sola cartella, non ricorsivamente', async () => {
  const root = await albero()
  const figli = await listChildren(root, 'src')
  expect(figli.map((n) => n.name).sort()).toEqual(['annidata', 'app.ts'])
})

test('esclude .git', async () => {
  const root = await albero()
  const nomi = (await listChildren(root, '')).map((n) => n.name)
  expect(nomi).not.toContain('.git')
})

test('distingue cartella, file e collegamento', async () => {
  const root = await albero()
  const perNome = new Map((await listChildren(root, '')).map((n) => [n.name, n.kind]))
  expect(perNome.get('src')).toBe('directory')
  expect(perNome.get('radice.txt')).toBe('file')
  expect(perNome.get('collegamento')).toBe('symlink')
})

test('i percorsi relativi dei figli sono annidati', async () => {
  const root = await albero()
  const figli = await listChildren(root, 'src')
  expect(figli.map((n) => n.relativePath).sort()).toEqual(['src/annidata', 'src/app.ts'])
})

test('le cartelle vengono prima dei file', async () => {
  const root = await albero()
  const generi = (await listChildren(root, '')).map((n) => n.kind)
  expect(generi.indexOf('directory')).toBeLessThan(generi.lastIndexOf('file'))
})

test('un file al posto di una cartella e un errore con motivo', async () => {
  const root = await albero()
  await expect(listChildren(root, 'radice.txt')).rejects.toThrow(/not a directory/i)
})

test('un percorso fuori radice e un errore con motivo', async () => {
  const root = await albero()
  await expect(listChildren(root, '../fuori')).rejects.toThrow(/escapes the worktree root/i)
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/main/files/tree.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

```ts
import { readdir } from 'node:fs/promises'
import { FileAccessError, resolveInsideRoot } from './paths.ts'
import type { FileNode } from '../../shared/files/types.ts'

/**
 * Elenca i figli di UNA cartella, mai ricorsivamente: porta
 * FileTreeLoader.children(at:rootURL:). L'espansione pigra non e' un
 * ottimizzazione: un worktree con node_modules non deve essere percorso per
 * intero solo per mostrare la radice.
 */
export async function listChildren(rootPath: string, relativePath: string): Promise<FileNode[]> {
  const target = resolveInsideRoot(rootPath, relativePath)

  let voci
  try {
    voci = await readdir(target, { withFileTypes: true })
  } catch (errore) {
    const codice = (errore as NodeJS.ErrnoException).code
    if (codice === 'ENOTDIR') throw new FileAccessError('notDirectory', relativePath)
    throw errore
  }

  return voci
    .filter((voce) => voce.name !== '.git')
    .map((voce) => ({
      relativePath: relativePath === '' ? voce.name : `${relativePath}/${voce.name}`,
      name: voce.name,
      // L'ordine dei controlli conta: un collegamento a una cartella risponde
      // true a entrambi, e va mostrato come collegamento.
      kind: voce.isSymbolicLink() ? 'symlink' : voce.isDirectory() ? 'directory' : 'file'
    }))
    .sort(ordine)
}

function ordine(a: FileNode, b: FileNode): number {
  const aCartella = a.kind === 'directory'
  const bCartella = b.kind === 'directory'
  if (aCartella !== bCartella) return aCartella ? -1 : 1
  return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
}
```

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/files/tree.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/files/tree.ts src/main/files/tree.test.ts
git commit -m "feat: list the children of one directory"
```

---

## Task 6: Lettura del file con limiti

**File:**
- Crea: `src/main/files/read.ts`
- Test: `src/main/files/read.test.ts`
- Modifica: il file della Fase 6a che definisce i limiti del diff, per
  **esportarli** se sono privati.

**Interfacce:**
- Consuma: `resolveInsideRoot` (Task 4); `FileContent` (Task 1).
- Produce: `readFileForView(rootPath, relativePath): Promise<FileContent>`.
  Lo consuma il Task 7.

**Prima di implementare:** cercare dove la Fase 6a ha messo i limiti 5 MB /
20.000 righe (`awk '/5 \* 1024|20_000|20000/' src/main/git/*.ts`). Se sono
costanti private, esportarle e importarle qui. **Non duplicare i numeri:** due
copie divergono, e il diff e il visualizzatore direbbero cose diverse sullo
stesso file.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { mkdtemp, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { expect, test } from 'vitest'
import { readFileForView } from './read.ts'

async function radice(): Promise<string> {
  return mkdtemp(join(tmpdir(), 'tiller-read-'))
}

test('legge un file di testo intero', async () => {
  const root = await radice()
  await writeFile(join(root, 'a.txt'), 'prima\nseconda\n')
  const contenuto = await readFileForView(root, 'a.txt')
  expect(contenuto).toEqual({ text: 'prima\nseconda\n', isBinary: false, truncated: false })
})

test('un file binario torna senza testo', async () => {
  const root = await radice()
  await writeFile(join(root, 'b.bin'), Buffer.from([0x50, 0x00, 0x51]))
  const contenuto = await readFileForView(root, 'b.bin')
  expect(contenuto.isBinary).toBe(true)
  expect(contenuto.text).toBe('')
})

test('oltre il limite di righe torna troncato', async () => {
  const root = await radice()
  await writeFile(join(root, 'lungo.txt'), 'x\n'.repeat(25_000))
  const contenuto = await readFileForView(root, 'lungo.txt')
  expect(contenuto.truncated).toBe(true)
  expect(contenuto.text.split('\n').length).toBeLessThanOrEqual(20_000)
})

test('un file vuoto non e binario ne troncato', async () => {
  const root = await radice()
  await writeFile(join(root, 'vuoto.txt'), '')
  expect(await readFileForView(root, 'vuoto.txt')).toEqual({
    text: '',
    isBinary: false,
    truncated: false
  })
})

test('un file inesistente propaga l errore', async () => {
  const root = await radice()
  await expect(readFileForView(root, 'assente.txt')).rejects.toThrow()
})

test('un percorso fuori radice e rifiutato', async () => {
  const root = await radice()
  await expect(readFileForView(root, '../fuori.txt')).rejects.toThrow(/escapes the worktree root/i)
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/main/files/read.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

```ts
import { readFile } from 'node:fs/promises'
import { resolveInsideRoot } from './paths.ts'
import type { FileContent } from '../../shared/files/types.ts'
// Importare i due limiti dalla Fase 6a invece di riscriverli.
import { MAX_DIFF_BYTES, MAX_DIFF_LINES } from '../git/diff.ts'

/** La finestra su cui git decide se un contenuto e' binario. */
const FINESTRA_BINARIO = 8000

export async function readFileForView(
  rootPath: string,
  relativePath: string
): Promise<FileContent> {
  const target = resolveInsideRoot(rootPath, relativePath)
  const dati = await readFile(target)

  // Stessa euristica di changes.ts, che e' quella di git: un byte nullo nella
  // prima finestra. Un binario non si mostra e non si tronca: si dichiara.
  if (dati.subarray(0, FINESTRA_BINARIO).includes(0)) {
    return { text: '', isBinary: true, truncated: false }
  }

  let troncato = false
  let porzione = dati
  if (porzione.byteLength > MAX_DIFF_BYTES) {
    porzione = porzione.subarray(0, MAX_DIFF_BYTES)
    troncato = true
  }

  let testo = porzione.toString('utf8')
  const righe = testo.split('\n')
  if (righe.length > MAX_DIFF_LINES) {
    testo = righe.slice(0, MAX_DIFF_LINES).join('\n')
    troncato = true
  }

  return { text: testo, isBinary: false, truncated: troncato }
}
```

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/files/read.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/files/read.ts src/main/files/read.test.ts src/main/git/diff.ts
git commit -m "feat: read a file for viewing, with the diff limits"
```

---

## Task 7: Le due richieste di protocollo

**File:**
- Modifica: `src/main/control/dispatch.ts` (accanto ai `case 'git.*'`, riga ~422)
- Test: `src/main/control/dispatch-files.test.ts`

**Interfacce:**
- Consuma: `listChildren` (Task 5), `readFileForView` (Task 6).
- Produce: i metodi `files.list` e `files.read`. Li consumano i Task 10 e 13.

Seguire **esattamente** la forma dei `case 'git.changes'` e `'git.diff'` già
presenti: stessa validazione dei parametri, stessa forma di risposta
`{ ok, result | error }`, stessa risoluzione da `worktreeId` a percorso su disco.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// Modellare su src/main/control/*.test.ts esistenti: stesso costruttore di
// DispatchDeps, stesso createTestRepo per il worktree temporaneo.
test('files.list elenca i figli di una cartella', async () => {
  // ... arrange con createTestRepo
  const risposta = await dispatcher.handle({
    id: 'x', method: 'files.list', params: { worktreeId, path: '' }
  })
  expect(risposta.ok).toBe(true)
})

test('files.list rifiuta un percorso fuori radice con il motivo', async () => {
  const risposta = await dispatcher.handle({
    id: 'x', method: 'files.list', params: { worktreeId, path: '../fuori' }
  })
  expect(risposta.ok).toBe(false)
  expect(risposta.error).toMatch(/escapes the worktree root/i)
})

test('files.read restituisce il contenuto', async () => { /* ... */ })

test('files.read su un binario lo dichiara invece di fallire', async () => { /* ... */ })
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/main/control/dispatch-files.test.ts` → FAIL.

- [ ] **Passo 3: implementare i due `case`**

Un percorso fuori radice deve tornare `{ ok: false, error }` con il messaggio di
`FileAccessError`, **non** propagare l'eccezione: il socket serve anche la CLI, e
un'eccezione non gestita chiuderebbe la connessione invece di rispondere.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/control/` → PASS (tutti i file).

- [ ] **Passo 5: committare**

```bash
git add src/main/control/
git commit -m "feat: files.list and files.read over the control protocol"
```

---

## Task 8: Il watcher del filesystem

**File:**
- Crea: `src/main/files/watch.ts`
- Test: `src/main/files/watch.test.ts`
- Modello: `src/main/git/watch.ts` (stessa forma, chokidar già in uso)

**Interfacce:**
- Produce: `watchFiles(rootPath, onChanged: (directories: string[]) => void): () => void`.
  Lo consuma il Task 10 tramite l'evento `files.changed`.

L'evento porta le **cartelle toccate**, non i file: il consumatore ricarica una
cartella per volta, e un albero espanso in profondità non si richiude perché
qualcuno ha salvato altrove.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
test('una scrittura segnala la cartella che la contiene', async () => {
  const root = await mkdtemp(join(tmpdir(), 'tiller-watch-'))
  await mkdir(join(root, 'src'))
  const viste: string[][] = []
  const stop = watchFiles(root, (cartelle) => viste.push(cartelle))
  await attesaPronto()
  await writeFile(join(root, 'src', 'nuovo.ts'), 'x\n')
  await vi.waitFor(() => expect(viste.flat()).toContain('src'))
  stop()
})

test('.git non genera segnalazioni', async () => {
  // scrivere dentro .git non deve svegliare l albero
})

test('fermare il watcher smette di segnalare', async () => { /* ... */ })
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare** con chokidar, `ignored` su `.git`, debounce
  coerente con `src/main/git/watch.ts`.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/files/watch.test.ts` → PASS.

- [ ] **Passo 5: collegare l'evento `files.changed`** nel punto in cui
  `git.changed` è già emesso, e committare.

```bash
git add src/main/files/watch.ts src/main/files/watch.test.ts src/main/
git commit -m "feat: watch the worktree and report changed directories"
```

---

## Task 9: Tokenizzazione proiettata sulle righe

**File:**
- Crea: `src/renderer/src/lib/workspace/highlight.ts`
- Test: `src/renderer/src/lib/workspace/highlight.test.ts`
- Modifica: `package.json` (dipendenze CodeMirror)

**Interfacce:**
- Consuma: `CodeLanguageId`, `languageForFileName` (Task 3).
- Produce: `TokenRange { from, to, className }`, `lineTokenRanges(text, language): TokenRange[][]`.
  Lo consumano i Task 14 e 16.

**Questo è il cuore della fase.** È il porting di `LineHighlightMap` da
`DiffHighlighter.swift`, e il caso che una porta ingenua sbaglia è il token che
attraversa più righe: una stringa multilinea o un commento a blocchi deve
comparire su **tutte** le righe che tocca, con gli offset rimappati sull'inizio
di ciascuna.

**`@codemirror/language` e `@lezer/highlight` vanno aggiunti esplicitamente,
anche se compaiono già in `pnpm-lock.yaml`.** Ci sono finiti come peer di
`@codemirror/legacy-modes` (Task 3b), ma pnpm non li espone in
`node_modules/@codemirror/`: un `import` diretto fallisce con
`Cannot find package '@codemirror/language'`. Verificato: sono nello store
`.pnpm` ma non collegati. Dipendere da una risoluzione transitiva qui non è una
scorciatoia, è una build che si rompe.

Installare prima:

```bash
pnpm add codemirror @codemirror/state @codemirror/view @codemirror/language @lezer/highlight \
  @codemirror/lang-javascript @codemirror/lang-python @codemirror/lang-rust \
  @codemirror/lang-go @codemirror/lang-cpp @codemirror/lang-java \
  @codemirror/lang-php @codemirror/lang-html @codemirror/lang-css \
  @codemirror/lang-json @codemirror/lang-markdown @codemirror/lang-sql \
  @codemirror/lang-xml @codemirror/lang-yaml
```

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { lineTokenRanges } from './highlight.ts'

test('una parola chiave riceve una classe', () => {
  const righe = lineTokenRanges('const x = 1\n', 'typescript')
  const classi = righe[0].map((r) => r.className).join(' ')
  expect(classi).toMatch(/keyword/)
})

test('una stringa e una parola chiave hanno classi diverse', () => {
  const righe = lineTokenRanges('const s = "ciao"\n', 'typescript')
  const classi = new Set(righe[0].map((r) => r.className))
  expect(classi.size).toBeGreaterThan(1)
})

test('un token su piu righe compare su tutte, con offset per riga', () => {
  const testo = 'const s = `prima\nseconda\nterza`\n'
  const righe = lineTokenRanges(testo, 'typescript')
  // La riga 1 (indice 1) e' interamente dentro il template literal.
  expect(righe[1].length).toBeGreaterThan(0)
  // Gli offset sono relativi all inizio della riga, non del documento.
  expect(righe[1][0].from).toBe(0)
  expect(righe[1][0].to).toBeLessThanOrEqual('seconda'.length)
})

test('il numero di righe restituite combacia con quelle del testo', () => {
  const testo = 'a\nb\nc\n'
  expect(lineTokenRanges(testo, 'typescript')).toHaveLength(testo.split('\n').length)
})

test('un linguaggio nullo non produce intervalli', () => {
  expect(lineTokenRanges('qualsiasi cosa\n', null)).toEqual([])
})

test('il testo vuoto non esplode', () => {
  expect(() => lineTokenRanges('', 'typescript')).not.toThrow()
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/renderer/src/lib/workspace/highlight.test.ts` → FAIL.

- [ ] **Passo 3: implementare**

```ts
import { classHighlighter, highlightTree } from '@lezer/highlight'
import { javascript } from '@codemirror/lang-javascript'
import { python } from '@codemirror/lang-python'
// ... gli altri lang-* come sopra
import type { Parser } from '@lezer/common'
import type { CodeLanguageId } from '../../../../shared/files/language.ts'

export interface TokenRange {
  /** Offset dall inizio DELLA RIGA, non del documento. */
  from: number
  to: number
  className: string
}

function parserFor(language: CodeLanguageId): Parser {
  switch (language) {
    case 'typescript':
      return javascript({ typescript: true }).language.parser
    case 'javascript':
      return javascript().language.parser
    case 'python':
      return python().language.parser
    // ... un caso per linguaggio; l unione esaustiva fa fallire il compilatore
    //     se il Task 3 aggiunge un linguaggio e questo file non lo segue.
  }
}

/** Offset di inizio di ogni riga, piu' la lunghezza totale come sentinella. */
function lineStarts(text: string): number[] {
  const starts = [0]
  for (let i = 0; i < text.length; i++) {
    if (text[i] === '\n') starts.push(i + 1)
  }
  return starts
}

/**
 * Porta LineHighlightMap da DiffHighlighter.swift: tokenizza il testo INTERO e
 * proietta gli intervalli sulle righe. Tokenizzare riga per riga darebbe
 * risultati sbagliati, perche' una riga isolata e' ambigua: `}` da sola non
 * dice se chiude una funzione, una stringa o un commento.
 */
export function lineTokenRanges(text: string, language: CodeLanguageId | null): TokenRange[][] {
  if (language === null) return []

  const starts = lineStarts(text)
  const righe: TokenRange[][] = starts.map(() => [])
  const albero = parserFor(language).parse(text)

  highlightTree(albero, classHighlighter, (from, to, className) => {
    if (to <= from) return
    for (let i = indiceRiga(starts, from); i < starts.length; i++) {
      const inizio = starts[i]
      if (inizio >= to) break
      // Fine del contenuto della riga: l inizio della successiva meno il \n,
      // oppure la fine del testo per l ultima riga.
      const fine = i + 1 < starts.length ? starts[i + 1] - 1 : text.length
      const a = Math.max(from, inizio)
      const b = Math.min(to, fine)
      if (b > a) righe[i].push({ from: a - inizio, to: b - inizio, className })
    }
  })

  return righe
}

/** Ricerca binaria della riga che contiene un offset del documento. */
function indiceRiga(starts: number[], offset: number): number {
  let basso = 0
  let alto = starts.length - 1
  while (basso < alto) {
    const mezzo = Math.ceil((basso + alto) / 2)
    if (starts[mezzo] <= offset) basso = mezzo
    else alto = mezzo - 1
  }
  return basso
}
```

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/renderer/src/lib/workspace/highlight.test.ts` → PASS.

- [ ] **Passo 5: committare, poi mutare**

```bash
git add package.json pnpm-lock.yaml src/renderer/src/lib/workspace/highlight.ts src/renderer/src/lib/workspace/highlight.test.ts
git commit -m "feat: project syntax tokens onto individual lines"
```

- [ ] **Passo 6: mutazione di verifica**

Sostituire il ciclo `for` con un solo inserimento sulla riga di `from`
(cioè trattare ogni token come se stesse su una riga sola).
Atteso: **rosso** su "un token su piu righe compare su tutte".
Annullare con `git checkout -- src/renderer/src/lib/workspace/highlight.ts`.

---

## Task 10: Nomi delle icone

**File:**
- Crea: `src/renderer/src/lib/workspace/file-icon.ts`
- Test: `src/renderer/src/lib/workspace/file-icon.test.ts`
- Legge: `~/Desktop/Progetti/tiller/Packages/TillerCore/Sources/TillerCore/FileIconTheme.swift`

**Interfacce:**
- Consuma: `FileIconKey` (Task 2).
- Produce: `iconNameFor(key: FileIconKey): string`. Lo consuma il Task 11.

La tabella `materialAssets` in Swift **è già** una tabella di nomi Iconify: gli
asset seguono `mat-<nome-iconify>` dal set `material-icon-theme`. Il porting
consiste nel togliere il prefisso `mat-`, non nello scegliere icone nuove.

`symlink` è deliberatamente assente dalla tabella Material anche in Swift e
ripiega sull'icona generica: mantenere l'assenza.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { iconNameFor } from './file-icon.ts'
import type { FileIconKey } from '../../../../shared/files/icon-key.ts'

test('ogni chiave ha un nome di icona', () => {
  const chiavi: FileIconKey[] = ['typescript', 'swift', 'docker', 'folder', 'file']
  for (const chiave of chiavi) expect(iconNameFor(chiave)).not.toBe('')
})

test('i nomi non portano il prefisso mat- di Swift', () => {
  expect(iconNameFor('typescript').startsWith('mat-')).toBe(false)
})

test('symlink ripiega sull icona generica', () => {
  expect(iconNameFor('symlink')).toBe(iconNameFor('file'))
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare** portando la tabella e aggiungendo
  `@iconify-json/material-icon-theme@1.2.70` come dipendenza di sviluppo, con le
  icone **impacchettate a build-time**. L'app non deve contattare la rete per
  disegnare un albero di file.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add package.json pnpm-lock.yaml src/renderer/src/lib/workspace/file-icon.ts src/renderer/src/lib/workspace/file-icon.test.ts
git commit -m "feat: map icon keys to bundled material icons"
```

---

## Task 11: L'albero nel pannello

**File:**
- Crea: `src/renderer/src/lib/workspace/FilesTree.svelte`
- Modifica: `src/renderer/src/lib/workspace/RightPanel.svelte` (righe 21–36:
  il pulsante `Files` è oggi `disabled` con la scritta "Available with the editor")

**Interfacce:**
- Consuma: `files.list` (Task 7), `iconNameFor` (Task 10), `iconKeyForFileName`
  e `iconKeyForDirectoryName` (Task 2), evento `files.changed` (Task 8).
- Produce: proprietà `onOpenFile(path: string, permanent: boolean)`.
  La collega il Task 15.

Modellare su `ChangesList.svelte`: stessa gestione di `loadGeneration` contro le
risposte in ritardo, stessa forma degli stati caricamento/errore/vuoto, stessi
attributi `data-*` per i test.

Attributi richiesti dai criteri e2e: `data-file-path` su ogni riga file,
`data-directory-path` su ogni riga cartella, `data-expanded` sulle cartelle.

- [ ] **Passo 1: scrivere il test che fallisce** — criterio e2e 1 del Task 17,
  scritto ora e lasciato rosso.

- [ ] **Passo 2: eseguire e vedere fallire**

`npx playwright test e2e/fase-6b1-files.spec.ts -g "espandere"` → FAIL.

- [ ] **Passo 3: implementare l'albero**

Espansione pigra: alla prima apertura di una cartella si chiama `files.list` per
**quella** cartella, e il risultato si tiene in una mappa
`percorso -> FileNode[]`. Riaprire una cartella già vista non richiama il main.

Su `files.changed`, invalidare **solo** le cartelle segnalate che risultano
espanse, e ricaricarle. Le altre restano come sono: un albero aperto in
profondità non deve richiudersi perché qualcuno ha salvato un file altrove.

- [ ] **Passo 4: abilitare il pulsante `Files`** in `RightPanel.svelte`,
  togliendo `disabled` e la scritta "Available with the editor", e rendendo i due
  pulsanti un vero interruttore fra `Files` e `Changes`.

- [ ] **Passo 5: eseguire e vedere passare** → PASS.

- [ ] **Passo 6: committare**

```bash
git add src/renderer/src/lib/workspace/FilesTree.svelte src/renderer/src/lib/workspace/RightPanel.svelte e2e/fase-6b1-files.spec.ts
git commit -m "feat: browse the worktree from the files panel"
```

---

## Task 12: Il tab file nel modello di layout

**File:**
- Modifica: `src/shared/workspace/layout-types.ts` (`WorkspaceContentRefSchema`, riga ~19)
- Modifica: `src/shared/workspace/layout-invariants.ts` (`contentKey`)
- Test: `src/shared/workspace/layout-types.test.ts`, `layout-invariants.test.ts`

**Interfacce:**
- Produce: il membro `{ kind: 'file', path }` di `WorkspaceContentRef`.
  Lo consumano i Task 14 e 15.

Aggiungere un **membro** a un'unione discriminata è additivo: i payload esistenti
continuano a validare. È il contrario del campo `isPreview` della Fase 6a, che
era nuovo e obbligatorio e ha rifiutato tre criteri e2e della Fase 4b finché non
ha ricevuto `.default(false)`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
test('un contenuto file valida', () => {
  const parsed = WorkspaceContentRefSchema.parse({ kind: 'file', path: 'src/app.ts' })
  expect(parsed.kind).toBe('file')
})

test('i contenuti terminal e diff continuano a validare', () => {
  expect(WorkspaceContentRefSchema.parse({ kind: 'terminal', id: 'p1' }).kind).toBe('terminal')
  expect(
    WorkspaceContentRefSchema.parse({ kind: 'diff', path: 'a.ts', source: 'worktree' }).kind
  ).toBe('diff')
})

test('contentKey distingue un file da un diff sullo stesso percorso', () => {
  const file = contentKey({ kind: 'file', path: 'a.ts' })
  const diff = contentKey({ kind: 'diff', path: 'a.ts', source: 'worktree' })
  expect(file).not.toBe(diff)
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

```ts
// in WorkspaceContentRefSchema
z.object({ kind: z.literal('file'), path: z.string().min(1) })

// in contentKey
case 'file':
  return `file:${content.path}`
```

- [ ] **Passo 4: seguire il compilatore.** L'unione è discriminata proprio
  perché aggiungere un membro faccia fallire i punti da aggiornare. Eseguire
  `pnpm typecheck` **e** `pnpm svelte-check`: il secondo vede gli errori nei
  `.svelte` che il primo salta. Aggiornare ogni `switch` che ora non è esaustivo.

  **Nella Fase 6a il punto dimenticato è stato `App.svelte`**, dove TypeScript
  perde il restringimento su `tab.content` dentro una callback. Il rimedio è
  legare il contenuto a una costante prima di entrare nella callback:

  ```ts
  const contenuto = tab.content
  if (contenuto.kind !== 'terminal') return []
  const pane = model.panes.find((candidate) => candidate.id === contenuto.id)
  ```

- [ ] **Passo 5: eseguire e vedere passare**

`npx vitest run src/shared/workspace/` → PASS, `pnpm typecheck` e
`pnpm svelte-check` puliti.

- [ ] **Passo 6: committare**

```bash
git add src/shared/workspace/ src/renderer/
git commit -m "feat: a file tab in the workspace layout"
```

---

## Task 13: Il tab file con CodeMirror

**File:**
- Crea: `src/renderer/src/lib/workspace/FileTab.svelte`

**Interfacce:**
- Consuma: `files.read` (Task 7), `languageForFileName` (Task 3),
  il membro `file` di `WorkspaceContentRef` (Task 12).

Modellare su `DiffTab.svelte`: stessa gestione di `loadGeneration`, stessi stati
caricamento/errore, stessa intestazione con il percorso.

- [ ] **Passo 1: scrivere il criterio e2e 2** (click apre il tab con il
  contenuto) e lasciarlo rosso.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

```ts
import { EditorState } from '@codemirror/state'
import { EditorView } from '@codemirror/view'

// Sola lettura: 6b-1 legge, non scrive. L editing arriva in 6b-2.
const stato = EditorState.create({
  doc: contenuto.text,
  extensions: [EditorState.readOnly.of(true), EditorView.editable.of(false), estensioneLinguaggio]
})
```

L'istanza va creata in un `$effect` e **distrutta** nel suo ritorno
(`view.destroy()`), altrimenti ogni riapertura del tab lascia una vista viva
attaccata al DOM.

Casi da rendere espliciti nella vista:
- `isBinary` → `This file is binary and cannot be shown.`
- `truncated` → avviso che il contenuto è tagliato, con il limite raggiunto
- linguaggio `null` → testo senza colore, nessun errore

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/FileTab.svelte e2e/fase-6b1-files.spec.ts
git commit -m "feat: read a file in a tab with codemirror"
```

---

## Task 14: Collegare il click all'apertura del tab

**File:**
- Modifica: `src/renderer/src/App.svelte`

**Interfacce:**
- Consuma: `onOpenFile` (Task 11), `openPreview` del motore di layout,
  `FileTab.svelte` (Task 13).

**Questo task esiste perché la Fase 4b ha consegnato una UI completa e scollegata,
e la 6a ha consegnato pulsanti `disabled`.** Un componente che espone
`onOpenFile` e un motore che espone `openPreview` non si collegano da soli, e
nessun test unitario se ne accorge: entrambi i lati passano.

- [ ] **Passo 1: verificare che i criteri 2 e 3 siano rossi** prima di scrivere
  il collegamento. Se sono già verdi, il criterio è scritto male.

- [ ] **Passo 2: collegare** `FilesTree.onOpenFile(path, permanent)` a
  `openPreview` / promozione, esattamente come `onOpenDiff` della Fase 6a, e
  rendere `FileTab` per i tab con `content.kind === 'file'`.

- [ ] **Passo 3: eseguire e vedere passare**

`npx playwright test e2e/fase-6b1-files.spec.ts` → criteri 2 e 3 PASS.

- [ ] **Passo 4: committare, poi mutare**

```bash
git add src/renderer/src/App.svelte
git commit -m "feat: connect file clicks to preview tabs"
```

- [ ] **Passo 5: mutazione di verifica**

Svuotare il produttore del click in `FilesTree.svelte`
(`onclick={() => {}}`), ricostruire con `npx electron-vite build`, rieseguire.
Atteso: **rosso solo** sui criteri 2 e 3; gli altri restano verdi.
Annullare con `git checkout --` e ricostruire.

---

## Task 15: I testi completi nel diff

**File:**
- Modifica: `src/shared/diff/types.ts` (`FileDiffSchema`)
- Modifica: `src/main/git/diff.ts` (produrre i due testi)
- Test: `src/main/git/diff.test.ts`

**Interfacce:**
- Produce: `oldText` e `newText` su `FileDiff`. Li consuma il Task 16.

In Fase 6a questi campi erano stati deliberatamente non portati, con la
motivazione scritta che servivano solo all'evidenziatore. È il punto previsto in
cui rientrano.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
test('il diff di un file modificato porta i due testi completi', async () => {
  const repo = await createTestRepo()
  // ... scrivere, committare, modificare
  const risultato = await fileDiff({ repoPath: repo, path: 'a.ts', source: 'worktree' })
  expect(risultato.oldText).toContain('versione precedente')
  expect(risultato.newText).toContain('versione nuova')
})

test('un file nuovo non ha testo precedente', async () => {
  // oldText === null, newText valorizzato
})

test('un file binario non porta testi', async () => {
  // oldText === null && newText === null
})

test('un payload senza i due campi resta valido', () => {
  // La compatibilita' del socket: il default deve reggere.
  const parsed = FileDiffSchema.parse({
    path: 'a.ts', lines: [], additions: 0, deletions: 0, isBinary: false, isSubmodule: false
  })
  expect(parsed.oldText).toBeNull()
  expect(parsed.newText).toBeNull()
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

```ts
// in FileDiffSchema
oldText: z.string().nullable().default(null),
newText: z.string().nullable().default(null)
```

Il `.default(null)` **non è opzionale**: lo schema viaggia sul socket di
controllo, quindi è un contratto di rete. Senza, ogni chiamante esistente viene
rifiutato — è esattamente ciò che è successo con `isPreview` in Fase 6a.

I testi si prendono con `git show <rev>:<path>` per il lato precedente e dal
disco (o `git show HEAD:<path>`) per quello nuovo, applicando gli stessi limiti
del Task 6. Oltre i limiti, o su binario, restano `null`.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/git/ src/shared/diff/` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/diff/types.ts src/main/git/diff.ts src/main/git/diff.test.ts
git commit -m "feat: carry the full old and new texts in a file diff"
```

---

## Task 16: Il diff prende colore

**File:**
- Modifica: `src/renderer/src/lib/workspace/DiffTab.svelte` (riga ~152 e ~158:
  oggi ogni riga è un solo `<span class="line-text">{row.left.text}</span>`)

**Interfacce:**
- Consuma: `lineTokenRanges` (Task 9), `oldText`/`newText` (Task 15),
  `languageForFileName` (Task 3).

La resa è **affiancata**: la colonna sinistra usa la mappa costruita su
`oldText`, la destra quella su `newText`. Sono due file diversi e due
tokenizzazioni diverse — usare una sola mappa per entrambe le colonne colorerebbe
le righe cancellate con i token del file nuovo.

Le mappe si calcolano **una volta per diff** in un `$derived`, non per riga:
tokenizzare a ogni riga rifà l'intero albero sintattico per ogni riga del file.

- [ ] **Passo 1: scrivere il criterio e2e 6** (il diff di un `.ts` mostra token
  colorati) e lasciarlo rosso.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Il singolo span diventa una sequenza di span, uno per `TokenRange`, con il testo
non coperto da alcun intervallo reso senza classe. Il numero di riga da usare per
consultare la mappa è `oldLineNumber` a sinistra e `newLineNumber` a destra.

**Quando i testi sono `null`** (binario, oltre i limiti, submodulo) il diff resta
esattamente come oggi: righe non colorate, struttura intatta. L'evidenziazione è
un miglioramento, non una dipendenza.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/DiffTab.svelte e2e/fase-6b1-files.spec.ts
git commit -m "feat: highlight syntax inside the diff"
```

---

## Task 17: I criteri end-to-end

**File:**
- Completa: `e2e/fase-6b1-files.spec.ts`

Tutti i criteri partono dal **gesto dell'utente** — un click, una scrittura sul
disco — e mai da una chiamata al protocollo o da `workspace.apply`. Un criterio
che parte dal comando verifica il comando, non la funzione.

- [ ] **Passo 1: completare i sette criteri**

1. Espandere una cartella mostra i suoi figli, e **non** quelli delle
   sottocartelle (verifica che l'elenco sia di una cartella sola).
2. Click su un file apre un tab in anteprima con il contenuto del file.
3. Doppio click promuove il tab; un click singolo successivo su un altro file
   sostituisce l'anteprima e **lascia vivo** il tab promosso.
4. Scrivere un file sul disco aggiorna la sua cartella, e le altre cartelle
   espanse **restano espanse**.
5. Un file `.ts` aperto mostra token con classi diverse (parola chiave contro
   stringa), non un blocco monocromo.
6. Il diff di un file `.ts` modificato mostra gli stessi token colorati.
7. Un file binario apre un tab che ne spiega il motivo, senza byte grezzi.

- [ ] **Passo 2: eseguire tutti i criteri**

```bash
npx electron-vite build && npx playwright test e2e/fase-6b1-files.spec.ts
```
Atteso: 7 passed.

- [ ] **Passo 3: mutazioni di verifica**

Una per volta, ricostruendo ogni volta, annullando con `git checkout --`:

| Mutazione | Criterio atteso rosso |
| --- | --- |
| `listChildren` diventa ricorsivo | 1 |
| il produttore del click non chiama `onOpenFile` | 2 e 3 |
| `files.changed` ricarica tutto l'albero invece delle sole cartelle toccate | 4 |
| `lineTokenRanges` restituisce sempre `[]` | 5 e 6 |
| il controllo `isBinary` in `read.ts` è invertito | 7 |

Una mutazione che **non** fa diventare rosso il suo criterio è un criterio che
non misura niente: va riscritto prima di proseguire.

- [ ] **Passo 4: committare**

```bash
git add e2e/fase-6b1-files.spec.ts
git commit -m "test: end-to-end criteria for reading files"
```

---

## Chiusura

Il gate completo (`bash scripts/ci.sh`) lo esegue **Claude**, non chi implementa.
Al termine dell'ultimo task, riportare:

- l'elenco dei commit,
- l'esito di `npx vitest run` sui percorsi toccati,
- l'esito di `pnpm typecheck` e `pnpm svelte-check`,
- l'esito di ogni mutazione richiesta, con il criterio diventato rosso,
- ciò che **non** si è riusciti a verificare, dicendolo invece di ometterlo.
