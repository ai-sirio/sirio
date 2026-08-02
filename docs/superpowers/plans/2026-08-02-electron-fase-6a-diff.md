# Fase 6a — Diff, piano di implementazione

> **Per chi esegue:** SOTTO-SKILL RICHIESTA: usa
> `superpowers:subagent-driven-development` (consigliata) o
> `superpowers:executing-plans` per eseguire questo piano task per task. I passi
> usano caselle (`- [ ]`) per il tracciamento.

**Obiettivo:** il pannello destro elenca i file cambiati e il click su un file
apre una tab con il suo diff.

**Architettura:** git resta l'autorità su cosa è cambiato — `parse-diff` legge
l'unified diff prodotto da git, non lo ricalcola. Le funzioni pure (accoppiamento
side-by-side, ritardo adattivo, invarianti) vivono in `src/shared/`; il dialogo
con git e col filesystem in `src/main/`; il disegno in `src/renderer/`. Il diff è
contenuto di tab, non modalità di pannello, quindi eredita split, persistenza e
ripristino dalle Fasi 4a/4b.

**Stack:** TypeScript, Svelte 5, zod, simple-git, `parse-diff` (nuova),
`chokidar` (nuova), vitest, Playwright.

**Spec:** `docs/superpowers/specs/2026-08-02-electron-fase-6a-diff-design.md`

## Vincoli globali

- Repo di lavoro: `~/Desktop/Progetti/tiller-electron`. Il repo Swift
  `~/Desktop/Progetti/tiller` si **legge soltanto**, mai si modifica.
- Non committare mai nulla sotto `.tokensave/`.
- Tutto il codice dell'app è TypeScript; i `.svelte` usano `lang="ts"`.
- Gate unico: `bash scripts/ci.sh` deve stampare `CI OK`.
- Zero warning ESLint **sui file toccati dal task**: verificare con
  `npx eslint <i file del task>`, non solo col lint globale.
- I commenti nel codice sono in italiano, come il resto del repo, e senza
  lettere accentate (la convenzione esistente scrive `perche'`, `e'`).
- Test con vitest (`describe`/`test`/`expect`), struttura Arrange-Act-Assert.
- Per costruire durante le mutazioni usare `npx electron-vite build`, **non**
  `pnpm build`: quest'ultimo include il typecheck, e una mutazione che non
  compila lascerebbe Playwright sul `out/` vecchio, che passa a vuoto.
- Mai eseguire una mutazione su una base rossa: il rosso non sarebbe
  attribuibile.

---

## Struttura dei file

**Nuovi:**

| File | Responsabilità |
|---|---|
| `src/shared/diff/types.ts` | `DiffLine`, `FileDiff`, `ChangedFile` e i loro schemi |
| `src/shared/diff/side-by-side.ts` | `rows()` — accoppia le righe in coppie sinistra/destra |
| `src/shared/adaptive-delay.ts` | ritardo che cresce sotto raffica |
| `src/main/git/diff-parse.ts` | `parse-diff` → `DiffLine[]` |
| `src/main/git/diff.ts` | `loadFileDiff` — comandi git, limiti, binari, submodule |
| `src/main/git/merge-base.ts` | `resolveBase` — `origin/HEAD`, ripiego, merge-base |
| `src/main/git/changes.ts` | `listChanged` — le due viste in una firma |
| `src/main/git/directory-status.ts` | rollup dello stato per cartella |
| `src/main/git/watch.ts` | chokidar + debounce adattivo |
| `src/renderer/lib/RightPanel.svelte` | selettore `Files`/`Changes` |
| `src/renderer/lib/ChangesList.svelte` | lista file, selettore sorgente, azioni |
| `src/renderer/lib/DiffTab.svelte` | resa side-by-side del diff |
| `e2e/fase-6a-diff.spec.ts` | i sei criteri |

**Modificati:**

| File | Modifica |
|---|---|
| `src/shared/workspace/layout-types.ts` | membro `diff` in `WorkspaceContentRef`; `isPreview` su `WorkspaceTab` |
| `src/shared/workspace/layout-invariants.ts` | `contentKey()`, errore `multiplePreviewTabs` |
| `src/shared/workspace/layout-codec.ts` | decodifica del contenuto `diff` |
| `src/shared/workspace/layout-engine.ts` | comandi `openPreview` e `promoteTab` |
| `src/shared/protocol.ts` | richieste `git.changes`, `git.diff`, `git.bases`; evento `git.changed` |
| `src/main/control/dispatch.ts` | gestori delle richieste sopra |

---

## Task 1: Tipi del diff

**File:**
- Crea: `src/shared/diff/types.ts`
- Test: `src/shared/diff/types.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `DiffLineKind`, `DiffLine`, `FileDiff`, `ChangedFile`,
  `DiffSource`, e gli schemi zod `DiffLineSchema`, `FileDiffSchema`,
  `ChangedFileSchema`. Tutti i task successivi usano questi nomi.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/shared/diff/types.test.ts
import { describe, expect, test } from 'vitest'
import { DiffLineSchema, FileDiffSchema, ChangedFileSchema } from './types.ts'

describe('schemi del diff', () => {
  test('una riga di contesto ha entrambi i numeri di riga', () => {
    const riga = {
      kind: 'context',
      oldLineNumber: 12,
      newLineNumber: 12,
      text: 'const x = 1'
    }
    expect(DiffLineSchema.parse(riga)).toEqual(riga)
  })

  test('una aggiunta non ha numero di riga vecchio', () => {
    const riga = { kind: 'addition', oldLineNumber: null, newLineNumber: 7, text: '+nuovo' }
    expect(DiffLineSchema.parse(riga)).toEqual(riga)
  })

  test('un genere sconosciuto viene respinto', () => {
    const riga = { kind: 'inventato', oldLineNumber: null, newLineNumber: 1, text: '' }
    expect(DiffLineSchema.safeParse(riga).success).toBe(false)
  })

  test('un diff binario non porta righe', () => {
    const diff = {
      path: 'logo.png',
      lines: [],
      additions: 0,
      deletions: 0,
      isBinary: true,
      isSubmodule: false
    }
    expect(FileDiffSchema.parse(diff)).toEqual(diff)
  })

  test('un file cambiato porta i conteggi e lo stato', () => {
    const file = {
      path: 'src/a.ts',
      state: 'modified',
      staged: false,
      additions: 3,
      deletions: 1
    }
    expect(ChangedFileSchema.parse(file)).toEqual(file)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/shared/diff/types.test.ts`
Atteso: FAIL, `Cannot find module './types.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/shared/diff/types.ts
import { z } from 'zod'

/**
 * I generi di riga dell unified diff, presi da GitDiffLineKind in Swift
 * (Packages/TillerGit/Sources/TillerGit/GitDiff.swift). `metadata` copre le
 * intestazioni `diff --git`, `index`, `---`, `+++`: si conservano nel modello
 * ma non si mostrano, perche' servono a distinguere un file vuoto da un file
 * senza modifiche.
 */
export const DiffLineKindSchema = z.enum([
  'metadata',
  'hunk',
  'context',
  'addition',
  'deletion'
])
export type DiffLineKind = z.infer<typeof DiffLineKindSchema>

export const DiffLineSchema = z.object({
  kind: DiffLineKindSchema,
  oldLineNumber: z.number().int().nullable(),
  newLineNumber: z.number().int().nullable(),
  text: z.string()
})
export type DiffLine = z.infer<typeof DiffLineSchema>

export const FileDiffSchema = z.object({
  path: z.string().min(1),
  lines: z.array(DiffLineSchema),
  additions: z.number().int().nonnegative(),
  deletions: z.number().int().nonnegative(),
  isBinary: z.boolean(),
  isSubmodule: z.boolean()
})
export type FileDiff = z.infer<typeof FileDiffSchema>

/**
 * Da dove viene il confronto. `worktree` e' il non committato (git status +
 * git diff); `branch` e' il lavoro del ramo (git diff base...HEAD). Le azioni
 * di stage esistono solo per `worktree`: sulla vista del ramo quelle righe
 * sono gia' committate.
 */
export const DiffSourceSchema = z.enum(['worktree', 'branch'])
export type DiffSource = z.infer<typeof DiffSourceSchema>

export const ChangedFileSchema = z.object({
  path: z.string().min(1),
  state: z.enum(['modified', 'added', 'deleted', 'renamed', 'untracked', 'conflicted']),
  staged: z.boolean(),
  additions: z.number().int().nonnegative(),
  deletions: z.number().int().nonnegative()
})
export type ChangedFile = z.infer<typeof ChangedFileSchema>
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/shared/diff/types.test.ts`
Atteso: PASS, 5 test.

- [ ] **Passo 5: lint e commit**

```bash
npx eslint src/shared/diff/
git add src/shared/diff/
git commit -m "feat: diff domain types"
```

---

## Task 2: Accoppiamento side-by-side

**File:**
- Crea: `src/shared/diff/side-by-side.ts`
- Test: `src/shared/diff/side-by-side.test.ts`

**Interfacce:**
- Consuma: `DiffLine` da `src/shared/diff/types.ts`.
- Produce: `SideBySideRow` (`{ left: DiffLine | null; right: DiffLine | null }`)
  e `rows(lines: DiffLine[]): SideBySideRow[]`. Il Task 14 la usa per disegnare.

Porta `GitDiffSideBySide.rows` da
`Packages/TillerGit/Sources/TillerGit/GitDiffSideBySide.swift`. È il pezzo più
denso della fase e anche il più facile da verificare: pura, un tipo in ingresso
e uno in uscita.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/shared/diff/side-by-side.test.ts
import { describe, expect, test } from 'vitest'
import type { DiffLine } from './types.ts'
import { rows } from './side-by-side.ts'

function ctx(n: number, t: string): DiffLine {
  return { kind: 'context', oldLineNumber: n, newLineNumber: n, text: t }
}
function del(n: number, t: string): DiffLine {
  return { kind: 'deletion', oldLineNumber: n, newLineNumber: null, text: t }
}
function add(n: number, t: string): DiffLine {
  return { kind: 'addition', oldLineNumber: null, newLineNumber: n, text: t }
}
function hunk(t: string): DiffLine {
  return { kind: 'hunk', oldLineNumber: null, newLineNumber: null, text: t }
}
function meta(t: string): DiffLine {
  return { kind: 'metadata', oldLineNumber: null, newLineNumber: null, text: t }
}

describe('rows', () => {
  test('il contesto compare su entrambi i lati', () => {
    const out = rows([ctx(1, 'a')])
    expect(out).toEqual([{ left: ctx(1, 'a'), right: ctx(1, 'a') }])
  })

  test('cancellazioni e aggiunte della stessa lunghezza si affiancano', () => {
    const out = rows([del(1, '-a'), del(2, '-b'), add(1, '+A'), add(2, '+B')])
    expect(out).toEqual([
      { left: del(1, '-a'), right: add(1, '+A') },
      { left: del(2, '-b'), right: add(2, '+B') }
    ])
  })

  test('piu cancellazioni che aggiunte lasciano il lato destro vuoto', () => {
    const out = rows([del(1, '-a'), del(2, '-b'), add(1, '+A')])
    expect(out).toEqual([
      { left: del(1, '-a'), right: add(1, '+A') },
      { left: del(2, '-b'), right: null }
    ])
  })

  test('piu aggiunte che cancellazioni lasciano il lato sinistro vuoto', () => {
    const out = rows([del(1, '-a'), add(1, '+A'), add(2, '+B')])
    expect(out).toEqual([
      { left: del(1, '-a'), right: add(1, '+A') },
      { left: null, right: add(2, '+B') }
    ])
  })

  test('una intestazione di hunk occupa il lato sinistro e chiude le sequenze', () => {
    const out = rows([del(1, '-a'), hunk('@@ -1 +1 @@')])
    expect(out).toEqual([
      { left: del(1, '-a'), right: null },
      { left: hunk('@@ -1 +1 @@'), right: null }
    ])
  })

  test('i metadata si scartano ma chiudono le sequenze aperte', () => {
    const out = rows([del(1, '-a'), meta('diff --git a/x b/x'), add(1, '+A')])
    expect(out).toEqual([
      { left: del(1, '-a'), right: null },
      { left: null, right: add(1, '+A') }
    ])
  })

  test('un ingresso vuoto produce zero righe', () => {
    expect(rows([])).toEqual([])
  })

  test('una sequenza in coda viene chiusa comunque', () => {
    const out = rows([ctx(1, 'a'), del(2, '-b')])
    expect(out).toEqual([
      { left: ctx(1, 'a'), right: ctx(1, 'a') },
      { left: del(2, '-b'), right: null }
    ])
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/shared/diff/side-by-side.test.ts`
Atteso: FAIL, `Cannot find module './side-by-side.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/shared/diff/side-by-side.ts
import type { DiffLine } from './types.ts'

export interface SideBySideRow {
  left: DiffLine | null
  right: DiffLine | null
}

/**
 * Accoppia le righe dell unified diff in righe visive: il contesto su
 * entrambi i lati, ogni sequenza di cancellazioni azzeccata con la sequenza di
 * aggiunte che la SEGUE, le intestazioni di hunk da sole a sinistra, i
 * metadata scartati.
 *
 * L accoppiamento per sequenze e non riga-per-riga e' il punto: git emette
 * tutte le cancellazioni di un blocco e poi tutte le aggiunte, quindi
 * appaiarle nell ordine di arrivo metterebbe la prima aggiunta accanto alla
 * prima cancellazione solo per caso.
 */
export function rows(lines: DiffLine[]): SideBySideRow[] {
  const out: SideBySideRow[] = []
  let cancellate: DiffLine[] = []
  let aggiunte: DiffLine[] = []

  function chiudi(): void {
    const quante = Math.max(cancellate.length, aggiunte.length)
    for (let i = 0; i < quante; i++) {
      out.push({
        left: i < cancellate.length ? cancellate[i] : null,
        right: i < aggiunte.length ? aggiunte[i] : null
      })
    }
    cancellate = []
    aggiunte = []
  }

  for (const riga of lines) {
    switch (riga.kind) {
      case 'deletion':
        cancellate.push(riga)
        break
      case 'addition':
        aggiunte.push(riga)
        break
      case 'context':
        chiudi()
        out.push({ left: riga, right: riga })
        break
      case 'hunk':
        chiudi()
        out.push({ left: riga, right: null })
        break
      case 'metadata':
        chiudi()
        break
    }
  }
  chiudi()
  return out
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/shared/diff/side-by-side.test.ts`
Atteso: PASS, 8 test.

- [ ] **Passo 5: verificare per mutazione**

Sostituire `Math.max` con `Math.min` in `chiudi()`. Rieseguire il test.
Atteso: ROSSO su "piu cancellazioni che aggiunte" e su "piu aggiunte che
cancellazioni" — sono i due test che esistono per questa riga. Ripristinare con
`git checkout -- src/shared/diff/side-by-side.ts` e riverificare il verde.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/shared/diff/
git add src/shared/diff/
git commit -m "feat: pair unified diff lines side by side"
```

---

## Task 3: Lettura dell'unified diff

**File:**
- Crea: `src/main/git/diff-parse.ts`
- Test: `src/main/git/diff-parse.test.ts`
- Modifica: `package.json` (dipendenza `parse-diff`)

**Interfacce:**
- Consuma: `DiffLine`, `FileDiff` da `src/shared/diff/types.ts`.
- Produce: `parseUnifiedDiff(testo: string): FileDiff[]`. Il Task 4 la usa.

- [ ] **Passo 1: installare la dipendenza**

```bash
pnpm add parse-diff
```

- [ ] **Passo 2: scrivere il test che fallisce**

```ts
// src/main/git/diff-parse.test.ts
import { describe, expect, test } from 'vitest'
import { parseUnifiedDiff } from './diff-parse.ts'

const SEMPLICE = `diff --git a/src/a.ts b/src/a.ts
index 111..222 100644
--- a/src/a.ts
+++ b/src/a.ts
@@ -1,3 +1,3 @@
 const x = 1
-const y = 2
+const y = 3
 const z = 4
`

describe('parseUnifiedDiff', () => {
  test('conta aggiunte e cancellazioni', () => {
    const [diff] = parseUnifiedDiff(SEMPLICE)
    expect(diff.path).toBe('src/a.ts')
    expect(diff.additions).toBe(1)
    expect(diff.deletions).toBe(1)
  })

  test('assegna i numeri di riga ai due lati', () => {
    const [diff] = parseUnifiedDiff(SEMPLICE)
    const cancellata = diff.lines.find((r) => r.kind === 'deletion')
    const aggiunta = diff.lines.find((r) => r.kind === 'addition')
    expect(cancellata?.oldLineNumber).toBe(2)
    expect(cancellata?.newLineNumber).toBeNull()
    expect(aggiunta?.oldLineNumber).toBeNull()
    expect(aggiunta?.newLineNumber).toBe(2)
  })

  test('conserva la intestazione di hunk', () => {
    const [diff] = parseUnifiedDiff(SEMPLICE)
    expect(diff.lines.some((r) => r.kind === 'hunk')).toBe(true)
  })

  test('riconosce un file binario e non produce righe', () => {
    const testo = `diff --git a/logo.png b/logo.png
index 111..222 100644
Binary files a/logo.png and b/logo.png differ
`
    const [diff] = parseUnifiedDiff(testo)
    expect(diff.isBinary).toBe(true)
    expect(diff.lines).toEqual([])
  })

  test('riconosce un submodule', () => {
    const testo = `diff --git a/vendor/lib b/vendor/lib
index 1111111..2222222 160000
--- a/vendor/lib
+++ b/vendor/lib
@@ -1 +1 @@
-Subproject commit 1111111
+Subproject commit 2222222
`
    const [diff] = parseUnifiedDiff(testo)
    expect(diff.isSubmodule).toBe(true)
  })

  test('usa il nuovo percorso per un file rinominato', () => {
    const testo = `diff --git a/vecchio.ts b/nuovo.ts
similarity index 90%
rename from vecchio.ts
rename to nuovo.ts
index 111..222 100644
--- a/vecchio.ts
+++ b/nuovo.ts
@@ -1 +1 @@
-a
+b
`
    const [diff] = parseUnifiedDiff(testo)
    expect(diff.path).toBe('nuovo.ts')
  })

  test('un file cancellato conserva il suo percorso', () => {
    const testo = `diff --git a/andato.ts b/andato.ts
deleted file mode 100644
index 111..000
--- a/andato.ts
+++ /dev/null
@@ -1 +0,0 @@
-addio
`
    const [diff] = parseUnifiedDiff(testo)
    expect(diff.path).toBe('andato.ts')
    expect(diff.deletions).toBe(1)
  })

  test('un testo vuoto produce zero diff', () => {
    expect(parseUnifiedDiff('')).toEqual([])
  })

  // I tre casi qui sotto sono indistinguibili da un binario guardando la
  // struttura: parse-diff produce per tutti zero chunk e zero conteggi. Solo
  // il testo grezzo di git li separa, ed e' li' che va cercato il marchio.
  test('una rinomina pura non e un file binario', () => {
    const testo = `diff --git a/vecchio.ts b/nuovo.ts
similarity index 100%
rename from vecchio.ts
rename to nuovo.ts
`
    const [diff] = parseUnifiedDiff(testo)
    expect(diff.isBinary).toBe(false)
    expect(diff.path).toBe('nuovo.ts')
  })

  test('un cambio di permessi non e un file binario', () => {
    const testo = `diff --git a/script.sh b/script.sh
old mode 100644
new mode 100755
`
    expect(parseUnifiedDiff(testo)[0].isBinary).toBe(false)
  })

  test('un file nuovo vuoto non e un file binario', () => {
    const testo = `diff --git a/vuoto.txt b/vuoto.txt
new file mode 100644
index 0000000..e69de29
`
    expect(parseUnifiedDiff(testo)[0].isBinary).toBe(false)
  })

  test('riconosce anche la forma GIT binary patch', () => {
    const testo = `diff --git a/logo.png b/logo.png
index 111..222 100644
GIT binary patch
delta 42
zcmV
`
    expect(parseUnifiedDiff(testo)[0].isBinary).toBe(true)
  })

  test('distingue il binario dalla rinomina nello stesso diff', () => {
    const testo = `diff --git a/logo.png b/logo.png
index 111..222 100644
Binary files a/logo.png and b/logo.png differ
diff --git a/vecchio.ts b/nuovo.ts
similarity index 100%
rename from vecchio.ts
rename to nuovo.ts
`
    expect(parseUnifiedDiff(testo).map((d) => d.isBinary)).toEqual([true, false])
  })
})
```

**Trappola registrata:** dedurre "binario" dall'assenza di chunk sembra
naturale e passa i test ovvi, perché il binario *ha* davvero zero chunk. Ma
l'implicazione non si inverte: hanno zero chunk anche la rinomina pura, il
cambio di permessi e il file nuovo vuoto. Il primo giro di questo task ha
prodotto esattamente quel difetto, e nessuno dei test originari lo vedeva.

- [ ] **Passo 3: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/diff-parse.test.ts`
Atteso: FAIL, `Cannot find module './diff-parse.ts'`.

- [ ] **Passo 4: scrivere l'implementazione minima**

```ts
// src/main/git/diff-parse.ts
import parse from 'parse-diff'
import type { DiffLine, FileDiff } from '../../shared/diff/types.ts'

/** Il modo di git di dire "questo percorso non esiste da questo lato". */
const ASSENTE = '/dev/null'

/** Il modo di git di segnare un gitlink, cioe' un submodule. */
const MODO_SUBMODULE = '160000'

/**
 * Le due forme in cui git dichiara un contenuto binario: la prima e' il
 * comportamento predefinito, la seconda arriva con `--binary`.
 */
const MARCHIO_BINARIO = /^(Binary files .* differ|GIT binary patch)$/m

/**
 * Il testo grezzo di ciascun file, nell ordine in cui parse-diff li restituisce.
 *
 * Serve perche' parse-diff NON distingue un binario da una rinomina pura, da un
 * cambio di permessi o da un file nuovo vuoto: per tutti e quattro produce zero
 * chunk e conteggi a zero. Dedurre "binario" da quella struttura marcherebbe
 * come illeggibili tre casi perfettamente leggibili.
 *
 * Lo spezzettamento su `diff --git` a inizio riga e' sicuro: dentro il corpo di
 * un diff ogni riga porta un prefisso (` `, `+`, `-`), quindi una riga di
 * contenuto che dicesse `diff --git` non comincerebbe mai a colonna zero.
 */
function sezioni(testo: string): string[] {
  const out: string[] = []
  let corrente: string[] | null = null
  for (const riga of testo.split('\n')) {
    if (riga.startsWith('diff --git ')) {
      if (corrente !== null) out.push(corrente.join('\n'))
      corrente = [riga]
      continue
    }
    corrente?.push(riga)
  }
  if (corrente !== null) out.push(corrente.join('\n'))
  return out
}

function percorso(file: parse.File): string {
  // Sul rinominato interessa il nome NUOVO: e' quello che l utente clicchera'
  // nell elenco e che esiste su disco.
  const nuovo = file.to ?? ''
  if (nuovo !== '' && nuovo !== ASSENTE) return nuovo
  const vecchio = file.from ?? ''
  return vecchio === ASSENTE ? '' : vecchio
}

export function parseUnifiedDiff(testo: string): FileDiff[] {
  const grezze = sezioni(testo)
  return parse(testo).map((file, indice) => {
    const righe: DiffLine[] = []
    let submodule = false

    for (const chunk of file.chunks) {
      righe.push({
        kind: 'hunk',
        oldLineNumber: null,
        newLineNumber: null,
        text: chunk.content
      })
      for (const change of chunk.changes) {
        if (change.content.includes('Subproject commit')) submodule = true
        if (change.type === 'add') {
          righe.push({
            kind: 'addition',
            oldLineNumber: null,
            newLineNumber: change.ln,
            text: change.content.slice(1)
          })
          continue
        }
        if (change.type === 'del') {
          righe.push({
            kind: 'deletion',
            oldLineNumber: change.ln,
            newLineNumber: null,
            text: change.content.slice(1)
          })
          continue
        }
        righe.push({
          kind: 'context',
          oldLineNumber: change.ln1,
          newLineNumber: change.ln2,
          text: change.content.slice(1)
        })
      }
    }

    // Il marchio nel testo, non l assenza di chunk: senza chunk stanno anche
    // rinomine, cambi di permessi e file vuoti, che binari non sono.
    const sezione = grezze[indice]
    const binario = sezione !== undefined && MARCHIO_BINARIO.test(sezione)
    const gitlink = submodule || (file.newMode ?? file.oldMode ?? '') === MODO_SUBMODULE

    return {
      path: percorso(file),
      // Il binario non ha righe da mostrare: git stesso non le emette.
      lines: binario ? [] : righe,
      additions: file.additions ?? 0,
      deletions: file.deletions ?? 0,
      isBinary: binario,
      isSubmodule: gitlink
    }
  })
}
```

- [ ] **Passo 5: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/diff-parse.test.ts`
Atteso: PASS, 8 test. Se il rilevamento del binario o del submodule non
corrisponde, aggiustare l'implementazione **non i test**: sono i formati che
git emette davvero.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/main/git/diff-parse.ts src/main/git/diff-parse.test.ts
git add package.json pnpm-lock.yaml src/main/git/diff-parse.ts src/main/git/diff-parse.test.ts
git commit -m "feat: read git unified diff into typed lines"
```

---

## Task 4: Caricamento del diff di un file

**File:**
- Crea: `src/main/git/diff.ts`
- Test: `src/main/git/diff.test.ts`

**Interfacce:**
- Consuma: `gitIn` da `./runner.ts`, `parseUnifiedDiff` da `./diff-parse.ts`,
  `FileDiff` e `DiffSource` da `../../shared/diff/types.ts`.
- Produce:

```ts
type DiffLoad =
  | { ok: true; diff: FileDiff }
  | { ok: false; reason: string }

async function loadFileDiff(opts: {
  repoPath: string
  path: string
  source: DiffSource
  base: string | null
  isUntracked: boolean
}): Promise<DiffLoad>
```

I Task 12 e 15 la usano. `base` è richiesto quando `source === 'branch'`.

- [ ] **Passo 1: scrivere il test che fallisce**

I test usano `makeTestRepo` di `src/main/git/test-repo.ts`, già presente e usato
da `status.test.ts`. Leggerlo per la firma esatta prima di scrivere.

```ts
// src/main/git/diff.test.ts
import { describe, expect, test } from 'vitest'
import { writeFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { makeTestRepo } from './test-repo.ts'
import { gitIn } from './runner.ts'
import { loadFileDiff, LIMITE_BYTE, LIMITE_RIGHE } from './diff.ts'

describe('loadFileDiff', () => {
  test('legge il diff di un file modificato non committato', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'uno\ndue\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    await writeFile(join(repo, 'a.txt'), 'uno\nTRE\n')

    const esito = await loadFileDiff({
      repoPath: repo,
      path: 'a.txt',
      source: 'worktree',
      base: null,
      isUntracked: false
    })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.diff.additions).toBe(1)
    expect(esito.diff.deletions).toBe(1)
  })

  test('un file non tracciato si confronta con il vuoto', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'x\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    await writeFile(join(repo, 'nuovo.txt'), 'riga uno\nriga due\n')

    const esito = await loadFileDiff({
      repoPath: repo,
      path: 'nuovo.txt',
      source: 'worktree',
      base: null,
      isUntracked: true
    })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.diff.additions).toBe(2)
    expect(esito.diff.deletions).toBe(0)
  })

  test('la vista del ramo non mostra il lavoro fatto sulla base dopo il fork', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'comune.txt'), 'base\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')

    await gitIn(repo).raw(['checkout', '-b', 'lavoro'])
    await writeFile(join(repo, 'mio.txt'), 'mio\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('sul ramo')

    // La base avanza DOPO il fork: non deve comparire nel diff del ramo.
    const principale = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD~1']))
    void principale
    await gitIn(repo).raw(['checkout', '-'])
    await writeFile(join(repo, 'altrui.txt'), 'altrui\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('sulla base')
    const nomeBase = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()
    await gitIn(repo).raw(['checkout', 'lavoro'])

    const mio = await loadFileDiff({
      repoPath: repo,
      path: 'mio.txt',
      source: 'branch',
      base: nomeBase,
      isUntracked: false
    })
    expect(mio.ok).toBe(true)
    if (mio.ok) expect(mio.diff.additions).toBe(1)

    const altrui = await loadFileDiff({
      repoPath: repo,
      path: 'altrui.txt',
      source: 'branch',
      base: nomeBase,
      isUntracked: false
    })
    expect(altrui.ok).toBe(true)
    if (altrui.ok) expect(altrui.diff.lines).toEqual([])
  })

  test('un diff oltre il limite di righe viene rifiutato con il motivo', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'x\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    const enorme = Array.from({ length: LIMITE_RIGHE + 10 }, (_, i) => `riga ${i}`).join('\n')
    await writeFile(join(repo, 'a.txt'), enorme)

    const esito = await loadFileDiff({
      repoPath: repo,
      path: 'a.txt',
      source: 'worktree',
      base: null,
      isUntracked: false
    })

    expect(esito.ok).toBe(false)
    if (esito.ok) return
    expect(esito.reason).toMatch(/troppo grande/)
  })

  test('i limiti sono quelli di Tiller Swift', () => {
    expect(LIMITE_BYTE).toBe(5 * 1024 * 1024)
    expect(LIMITE_RIGHE).toBe(20_000)
  })

  test('un percorso fuori dal repo viene rifiutato', async () => {
    const repo = await makeTestRepo()
    const esito = await loadFileDiff({
      repoPath: repo,
      path: '../fuori.txt',
      source: 'worktree',
      base: null,
      isUntracked: false
    })
    expect(esito.ok).toBe(false)
    if (esito.ok) return
    expect(esito.reason).toMatch(/fuori dal repository/)
  })

  test('senza base la vista del ramo si rifiuta invece di indovinare', async () => {
    const repo = await makeTestRepo()
    const esito = await loadFileDiff({
      repoPath: repo,
      path: 'a.txt',
      source: 'branch',
      base: null,
      isUntracked: false
    })
    expect(esito.ok).toBe(false)
    if (esito.ok) return
    expect(esito.reason).toMatch(/base/)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/diff.test.ts`
Atteso: FAIL, `Cannot find module './diff.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/main/git/diff.ts
import { isAbsolute, join, normalize, relative } from 'node:path'
import { gitIn } from './runner.ts'
import { parseUnifiedDiff } from './diff-parse.ts'
import type { DiffSource, FileDiff } from '../../shared/diff/types.ts'

/** I limiti di GitDiff.outputLimits in Swift. Oltre, la UI si bloccherebbe. */
export const LIMITE_BYTE = 5 * 1024 * 1024
export const LIMITE_RIGHE = 20_000

export type DiffLoad = { ok: true; diff: FileDiff } | { ok: false; reason: string }

/**
 * Un percorso relativo che non esce dal repository. Il percorso arriva dalla
 * lista, ma la lista arriva dal renderer, e un renderer e' un processo che
 * riceve dati da fuori: `../../etc/passwd` non deve diventare un argomento di
 * git.
 */
function dentroIlRepo(repoPath: string, path: string): boolean {
  if (isAbsolute(path)) return false
  const assoluto = normalize(join(repoPath, path))
  const rel = relative(repoPath, assoluto)
  return rel !== '' && !rel.startsWith('..')
}

function vuoto(path: string): FileDiff {
  return {
    path,
    lines: [],
    additions: 0,
    deletions: 0,
    isBinary: false,
    isSubmodule: false
  }
}

export async function loadFileDiff(opts: {
  repoPath: string
  path: string
  source: DiffSource
  base: string | null
  isUntracked: boolean
}): Promise<DiffLoad> {
  if (!dentroIlRepo(opts.repoPath, opts.path)) {
    return { ok: false, reason: `percorso fuori dal repository: ${opts.path}` }
  }
  if (opts.source === 'branch' && opts.base === null) {
    return { ok: false, reason: 'nessuna base con cui confrontare il ramo' }
  }

  const argomenti = costruisciArgomenti(opts)
  let testo: string
  try {
    testo = await gitIn(opts.repoPath).raw(argomenti)
  } catch (errore) {
    // `git diff --no-index` esce con 1 quando ci SONO differenze: simple-git
    // lo tratta da errore, ma l uscita utile e' nel messaggio.
    const messaggio = errore instanceof Error ? errore.message : String(errore)
    if (!opts.isUntracked) return { ok: false, reason: messaggio }
    testo = messaggio
  }

  if (testo.length > LIMITE_BYTE) {
    return { ok: false, reason: `diff troppo grande: oltre ${LIMITE_BYTE} byte` }
  }
  const righe = testo.split('\n').length
  if (righe > LIMITE_RIGHE) {
    return { ok: false, reason: `diff troppo grande: oltre ${LIMITE_RIGHE} righe` }
  }

  const diffs = parseUnifiedDiff(testo)
  // Nessun diff non e' un errore: e' un file che, per questa sorgente, non e'
  // cambiato. Il caso si vede sulla vista del ramo, su un file toccato solo
  // dopo il fork della base.
  return { ok: true, diff: diffs[0] ?? vuoto(opts.path) }
}

function costruisciArgomenti(opts: {
  repoPath: string
  path: string
  source: DiffSource
  base: string | null
  isUntracked: boolean
}): string[] {
  const comuni = ['diff', '--no-color', '--no-ext-diff', '--unified=3']
  if (opts.source === 'branch') {
    // I TRE punti: confronta col merge-base, non con la punta della base.
    // Coi due punti comparirebbe, invertito, anche il lavoro finito sulla base
    // dopo il fork, attribuito a questo ramo.
    return [...comuni, `${opts.base as string}...HEAD`, '--', opts.path]
  }
  if (opts.isUntracked) {
    return [...comuni, '--no-index', '--', '/dev/null', join(opts.repoPath, opts.path)]
  }
  return [...comuni, 'HEAD', '--', opts.path]
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/diff.test.ts`
Atteso: PASS, 7 test.

- [ ] **Passo 5: verificare per mutazione**

Cambiare i tre punti in due (`${opts.base}..HEAD`). Rieseguire.
Atteso: ROSSO su "la vista del ramo non mostra il lavoro fatto sulla base dopo
il fork" — è l'unico test che distingue le due forme. Ripristinare con
`git checkout -- src/main/git/diff.ts` e riverificare il verde.

Seconda mutazione: far restituire `true` a `dentroIlRepo`. Atteso: ROSSO su
"un percorso fuori dal repo viene rifiutato". Ripristinare.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/main/git/diff.ts src/main/git/diff.test.ts
git add src/main/git/diff.ts src/main/git/diff.test.ts
git commit -m "feat: load a file diff for worktree and branch sources"
```

---

## Task 5: Risoluzione della base

**File:**
- Crea: `src/main/git/merge-base.ts`
- Test: `src/main/git/merge-base.test.ts`

**Interfacce:**
- Consuma: `gitIn` da `./runner.ts`.
- Produce:

```ts
type BaseResolution =
  | { ok: true; base: string }
  | { ok: false; reason: string }

async function resolveBase(repoPath: string): Promise<BaseResolution>
async function hasMergeBase(repoPath: string, base: string): Promise<boolean>
```

I Task 6 e 12 le usano.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/main/git/merge-base.test.ts
import { describe, expect, test } from 'vitest'
import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { makeTestRepo } from './test-repo.ts'
import { gitIn } from './runner.ts'
import { resolveBase, hasMergeBase } from './merge-base.ts'

async function commitVuoto(repo: string, nome: string): Promise<void> {
  await writeFile(join(repo, `${nome}.txt`), nome)
  await gitIn(repo).add('.')
  await gitIn(repo).commit(nome)
}

describe('resolveBase', () => {
  test('ripiega sul ramo principale locale quando non c e un remoto', async () => {
    const repo = await makeTestRepo()
    await commitVuoto(repo, 'primo')
    const corrente = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()

    const esito = await resolveBase(repo)

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.base).toBe(corrente)
  })

  test('si rifiuta invece di indovinare quando non trova nessuna base', async () => {
    const repo = await makeTestRepo()
    // Repository senza commit: nessun ramo esiste ancora.
    const esito = await resolveBase(repo)
    expect(esito.ok).toBe(false)
    if (esito.ok) return
    expect(esito.reason).toMatch(/nessun ramo di riferimento/)
  })
})

describe('hasMergeBase', () => {
  test('e vero fra un ramo e la sua base', async () => {
    const repo = await makeTestRepo()
    await commitVuoto(repo, 'primo')
    const base = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()
    await gitIn(repo).raw(['checkout', '-b', 'lavoro'])
    await commitVuoto(repo, 'secondo')

    expect(await hasMergeBase(repo, base)).toBe(true)
  })

  test('e falso verso un ramo orfano', async () => {
    const repo = await makeTestRepo()
    await commitVuoto(repo, 'primo')
    const base = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()
    await gitIn(repo).raw(['checkout', '--orphan', 'orfano'])
    await gitIn(repo).raw(['rm', '-rf', '.'])
    await commitVuoto(repo, 'solo')

    expect(await hasMergeBase(repo, base)).toBe(false)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/merge-base.test.ts`
Atteso: FAIL, `Cannot find module './merge-base.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/main/git/merge-base.ts
import { gitIn } from './runner.ts'

export type BaseResolution = { ok: true; base: string } | { ok: false; reason: string }

/** L ordine di ripiego: cosa dice il remoto, poi le convenzioni locali. */
const CANDIDATI_LOCALI = ['main', 'master']

/**
 * La base non si persiste: la si chiede a git ogni volta. Un campo salvato
 * invecchierebbe in silenzio quando la storia cambia, e non coprirebbe
 * comunque i worktree creati fuori da Tiller.
 */
export async function resolveBase(repoPath: string): Promise<BaseResolution> {
  const daRemoto = await ramoDiDefaultDelRemoto(repoPath)
  if (daRemoto !== null) return { ok: true, base: daRemoto }

  for (const nome of CANDIDATI_LOCALI) {
    if (await esisteRamo(repoPath, nome)) return { ok: true, base: nome }
  }
  return { ok: false, reason: 'nessun ramo di riferimento: ne origin/HEAD, ne main, ne master' }
}

async function ramoDiDefaultDelRemoto(repoPath: string): Promise<string | null> {
  try {
    const uscita = await gitIn(repoPath).raw(['symbolic-ref', '--short', 'refs/remotes/origin/HEAD'])
    const nome = uscita.trim()
    // `origin/main` -> `origin/main`: si tiene il riferimento completo, che e'
    // cio' che git accetta come argomento di diff.
    return nome === '' ? null : nome
  } catch {
    return null
  }
}

async function esisteRamo(repoPath: string, nome: string): Promise<boolean> {
  try {
    await gitIn(repoPath).raw(['rev-parse', '--verify', `refs/heads/${nome}`])
    return true
  } catch {
    return false
  }
}

export async function hasMergeBase(repoPath: string, base: string): Promise<boolean> {
  try {
    const uscita = await gitIn(repoPath).raw(['merge-base', base, 'HEAD'])
    return uscita.trim() !== ''
  } catch {
    return false
  }
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/merge-base.test.ts`
Atteso: PASS, 4 test.

- [ ] **Passo 5: lint e commit**

```bash
npx eslint src/main/git/merge-base.ts src/main/git/merge-base.test.ts
git add src/main/git/merge-base.ts src/main/git/merge-base.test.ts
git commit -m "feat: resolve the comparison base from git"
```

---

## Task 6: Elenco dei file cambiati

**File:**
- Crea: `src/main/git/changes.ts`
- Test: `src/main/git/changes.test.ts`

**Interfacce:**
- Consuma: `readStatus` e `GitStatusEntry` da `./status.ts`, `gitIn` da
  `./runner.ts`, `ChangedFile` e `DiffSource` da `../../shared/diff/types.ts`.
- Produce:

```ts
type ChangesList =
  | { ok: true; branch: string | null; base: string | null; files: ChangedFile[] }
  | { ok: false; reason: string }

async function listChanged(opts: {
  repoPath: string
  source: DiffSource
  base: string | null
}): Promise<ChangesList>
```

I Task 12 e 13 la usano.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/main/git/changes.test.ts
import { describe, expect, test } from 'vitest'
import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { makeTestRepo } from './test-repo.ts'
import { gitIn } from './runner.ts'
import { listChanged } from './changes.ts'

describe('listChanged', () => {
  test('la vista non committato elenca i file con i conteggi', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'uno\ndue\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    await writeFile(join(repo, 'a.txt'), 'uno\nTRE\nQUATTRO\n')

    const esito = await listChanged({ repoPath: repo, source: 'worktree', base: null })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    const file = esito.files.find((f) => f.path === 'a.txt')
    expect(file?.additions).toBe(2)
    expect(file?.deletions).toBe(1)
    expect(file?.state).toBe('modified')
  })

  test('la vista non committato include i file non tracciati', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'x\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    await writeFile(join(repo, 'nuovo.txt'), 'a\nb\n')

    const esito = await listChanged({ repoPath: repo, source: 'worktree', base: null })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.files.some((f) => f.path === 'nuovo.txt' && f.state === 'untracked')).toBe(true)
  })

  test('la vista del ramo elenca i file di tutti i suoi commit', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'base.txt'), 'base\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    const base = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()

    await gitIn(repo).raw(['checkout', '-b', 'lavoro'])
    await writeFile(join(repo, 'uno.txt'), 'uno\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('commit uno')
    await writeFile(join(repo, 'due.txt'), 'due\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('commit due')

    const esito = await listChanged({ repoPath: repo, source: 'branch', base })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    const percorsi = esito.files.map((f) => f.path).sort()
    expect(percorsi).toEqual(['due.txt', 'uno.txt'])
  })

  test('la vista del ramo esclude il lavoro fatto sulla base dopo il fork', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'base.txt'), 'base\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    const base = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()

    await gitIn(repo).raw(['checkout', '-b', 'lavoro'])
    await writeFile(join(repo, 'mio.txt'), 'mio\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('sul ramo')

    await gitIn(repo).raw(['checkout', base])
    await writeFile(join(repo, 'altrui.txt'), 'altrui\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('sulla base')
    await gitIn(repo).raw(['checkout', 'lavoro'])

    const esito = await listChanged({ repoPath: repo, source: 'branch', base })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.files.map((f) => f.path)).toEqual(['mio.txt'])
  })

  test('la vista del ramo senza base si rifiuta con il motivo', async () => {
    const repo = await makeTestRepo()
    const esito = await listChanged({ repoPath: repo, source: 'branch', base: null })
    expect(esito.ok).toBe(false)
    if (esito.ok) return
    expect(esito.reason).toMatch(/base/)
  })

  test('la vista del ramo riporta la base usata', async () => {
    const repo = await makeTestRepo()
    await writeFile(join(repo, 'a.txt'), 'x\n')
    await gitIn(repo).add('.')
    await gitIn(repo).commit('primo')
    const base = (await gitIn(repo).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()

    const esito = await listChanged({ repoPath: repo, source: 'branch', base })

    expect(esito.ok).toBe(true)
    if (!esito.ok) return
    expect(esito.base).toBe(base)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/changes.test.ts`
Atteso: FAIL, `Cannot find module './changes.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/main/git/changes.ts
import { gitIn } from './runner.ts'
import { readStatus } from './status.ts'
import type { ChangedFile, DiffSource } from '../../shared/diff/types.ts'

export type ChangesList =
  | { ok: true; branch: string | null; base: string | null; files: ChangedFile[] }
  | { ok: false; reason: string }

/**
 * Una sola firma per le due viste. Cambia il comando git dietro e la presenza
 * dello stato di stage: sulla vista del ramo quelle righe sono gia'
 * committate, quindi `staged` non ha significato e resta falso.
 */
export async function listChanged(opts: {
  repoPath: string
  source: DiffSource
  base: string | null
}): Promise<ChangesList> {
  if (opts.source === 'branch' && opts.base === null) {
    return { ok: false, reason: 'nessuna base con cui confrontare il ramo' }
  }
  try {
    return opts.source === 'branch'
      ? await delRamo(opts.repoPath, opts.base as string)
      : await nonCommittato(opts.repoPath)
  } catch (errore) {
    return { ok: false, reason: errore instanceof Error ? errore.message : String(errore) }
  }
}

async function nonCommittato(repoPath: string): Promise<ChangesList> {
  const stato = await readStatus(repoPath)
  const conteggi = await numstat(repoPath, ['diff', '--numstat', 'HEAD'])

  const files: ChangedFile[] = stato.entries.map((voce) => ({
    path: voce.path,
    state: voce.state,
    staged: voce.staged,
    // Un file non tracciato non compare in `git diff HEAD`: i suoi conteggi si
    // ottengono contando le righe del file, che e' quel che fa `--no-index`.
    additions: conteggi.get(voce.path)?.additions ?? 0,
    deletions: conteggi.get(voce.path)?.deletions ?? 0
  }))
  return { ok: true, branch: stato.branch, base: null, files }
}

async function delRamo(repoPath: string, base: string): Promise<ChangesList> {
  const conteggi = await numstat(repoPath, ['diff', '--numstat', `${base}...HEAD`])
  const stati = await nomiEStati(repoPath, base)
  const branch = (await gitIn(repoPath).raw(['rev-parse', '--abbrev-ref', 'HEAD'])).trim()

  const files: ChangedFile[] = [...conteggi.entries()].map(([path, c]) => ({
    path,
    state: stati.get(path) ?? 'modified',
    staged: false,
    additions: c.additions,
    deletions: c.deletions
  }))
  return { ok: true, branch, base, files }
}

async function numstat(
  repoPath: string,
  argomenti: string[]
): Promise<Map<string, { additions: number; deletions: number }>> {
  const uscita = await gitIn(repoPath).raw(argomenti)
  const out = new Map<string, { additions: number; deletions: number }>()
  for (const riga of uscita.split('\n')) {
    if (riga.trim() === '') continue
    const [agg, canc, ...resto] = riga.split('\t')
    const path = resto.join('\t')
    if (path === '') continue
    // Il trattino segna un file binario: nessun conteggio di righe.
    out.set(path, {
      additions: agg === '-' ? 0 : Number(agg),
      deletions: canc === '-' ? 0 : Number(canc)
    })
  }
  return out
}

async function nomiEStati(
  repoPath: string,
  base: string
): Promise<Map<string, ChangedFile['state']>> {
  const uscita = await gitIn(repoPath).raw(['diff', '--name-status', `${base}...HEAD`])
  const out = new Map<string, ChangedFile['state']>()
  for (const riga of uscita.split('\n')) {
    if (riga.trim() === '') continue
    const parti = riga.split('\t')
    const codice = parti[0]?.[0] ?? 'M'
    // Sul rinominato git emette vecchio E nuovo: interessa il nuovo.
    const path = codice === 'R' ? (parti[2] ?? '') : (parti[1] ?? '')
    if (path === '') continue
    if (codice === 'A') out.set(path, 'added')
    else if (codice === 'D') out.set(path, 'deleted')
    else if (codice === 'R') out.set(path, 'renamed')
    else out.set(path, 'modified')
  }
  return out
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/changes.test.ts`
Atteso: PASS, 6 test.

- [ ] **Passo 5: verificare per mutazione**

In `delRamo`, cambiare `${base}...HEAD` in `${base}..HEAD` in **entrambe** le
chiamate. Atteso: ROSSO su "la vista del ramo esclude il lavoro fatto sulla base
dopo il fork". Ripristinare con `git checkout -- src/main/git/changes.ts`.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/main/git/changes.ts src/main/git/changes.test.ts
git add src/main/git/changes.ts src/main/git/changes.test.ts
git commit -m "feat: list changed files for both diff sources"
```

---

## Task 7: Rollup dello stato per cartella

**File:**
- Crea: `src/main/git/directory-status.ts`
- Test: `src/main/git/directory-status.test.ts`

**Interfacce:**
- Consuma: `ChangedFile` da `../../shared/diff/types.ts`.
- Produce: `type DirectoryStatus = 'clean' | 'modified' | 'conflicted'` e
  `rollupDirectories(files: ChangedFile[]): Map<string, DirectoryStatus>`.
  Il Task 13 la usa per i pallini sulle cartelle.

Porta `DirectoryGitStatus.swift`. La chiave della mappa è il percorso della
cartella con la barra finale (`src/`, `src/main/`); la radice è `''`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/main/git/directory-status.test.ts
import { describe, expect, test } from 'vitest'
import type { ChangedFile } from '../../shared/diff/types.ts'
import { rollupDirectories } from './directory-status.ts'

function f(path: string, state: ChangedFile['state'] = 'modified'): ChangedFile {
  return { path, state, staged: false, additions: 1, deletions: 0 }
}

describe('rollupDirectories', () => {
  test('marca ogni cartella antenata di un file modificato', () => {
    const out = rollupDirectories([f('src/main/git/diff.ts')])
    expect(out.get('')).toBe('modified')
    expect(out.get('src/')).toBe('modified')
    expect(out.get('src/main/')).toBe('modified')
    expect(out.get('src/main/git/')).toBe('modified')
  })

  test('un conflitto vince sul modificato lungo tutta la catena', () => {
    const out = rollupDirectories([f('src/a.ts'), f('src/b.ts', 'conflicted')])
    expect(out.get('src/')).toBe('conflicted')
    expect(out.get('')).toBe('conflicted')
  })

  test('una cartella senza file cambiati non compare', () => {
    const out = rollupDirectories([f('src/a.ts')])
    expect(out.has('docs/')).toBe(false)
  })

  test('un file nella radice marca solo la radice', () => {
    const out = rollupDirectories([f('README.md')])
    expect(out.get('')).toBe('modified')
    expect(out.size).toBe(1)
  })

  test('nessun file cambiato produce una mappa vuota', () => {
    expect(rollupDirectories([]).size).toBe(0)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/directory-status.test.ts`
Atteso: FAIL, `Cannot find module './directory-status.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/main/git/directory-status.ts
import type { ChangedFile } from '../../shared/diff/types.ts'

export type DirectoryStatus = 'clean' | 'modified' | 'conflicted'

/** Piu alto vince: un conflitto sepolto in fondo deve risalire in cima. */
const PESO: Record<DirectoryStatus, number> = { clean: 0, modified: 1, conflicted: 2 }

/**
 * Lo stato di ogni cartella che contiene, a qualsiasi profondita', un file
 * cambiato. La chiave porta la barra finale; la radice e' la stringa vuota.
 */
export function rollupDirectories(files: ChangedFile[]): Map<string, DirectoryStatus> {
  const out = new Map<string, DirectoryStatus>()

  for (const file of files) {
    const stato: DirectoryStatus = file.state === 'conflicted' ? 'conflicted' : 'modified'
    for (const cartella of antenate(file.path)) {
      const attuale = out.get(cartella)
      if (attuale === undefined || PESO[stato] > PESO[attuale]) {
        out.set(cartella, stato)
      }
    }
  }
  return out
}

function antenate(path: string): string[] {
  const parti = path.split('/')
  // L ultimo pezzo e' il file: le antenate sono i prefissi che lo precedono,
  // radice inclusa.
  const out = ['']
  let accumulato = ''
  for (let i = 0; i < parti.length - 1; i++) {
    accumulato += `${parti[i]}/`
    out.push(accumulato)
  }
  return out
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/directory-status.test.ts`
Atteso: PASS, 5 test.

- [ ] **Passo 5: verificare per mutazione**

Cambiare `PESO[stato] > PESO[attuale]` in `PESO[stato] < PESO[attuale]`.
Atteso: ROSSO su "un conflitto vince sul modificato". Ripristinare.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/main/git/directory-status.ts src/main/git/directory-status.test.ts
git add src/main/git/directory-status.ts src/main/git/directory-status.test.ts
git commit -m "feat: roll git status up to directories"
```

---

## Task 8: Ritardo adattivo

**File:**
- Crea: `src/shared/adaptive-delay.ts`
- Test: `src/shared/adaptive-delay.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce:

```ts
interface AdaptiveDelay {
  readonly currentDelayMs: number
  recordEvent(nowMs: number): number
  reset(): void
}
function createAdaptiveDelay(opts?: {
  baseMs?: number
  maxMs?: number
  quietMs?: number
}): AdaptiveDelay
```

Il Task 9 lo usa.

Porta `AdaptiveDebounce` da `App/RightPanel/RightPanelModel.swift:14`. Il tempo
arriva come argomento, non da `Date.now()`: un orologio iniettato rende il test
deterministico senza timer finti.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
// src/shared/adaptive-delay.test.ts
import { describe, expect, test } from 'vitest'
import { createAdaptiveDelay } from './adaptive-delay.ts'

describe('createAdaptiveDelay', () => {
  test('parte dal ritardo di base', () => {
    const d = createAdaptiveDelay({ baseMs: 100, maxMs: 800, quietMs: 1000 })
    expect(d.currentDelayMs).toBe(100)
  })

  test('il ritardo cresce sotto raffica', () => {
    const d = createAdaptiveDelay({ baseMs: 100, maxMs: 800, quietMs: 1000 })
    d.recordEvent(0)
    const primo = d.recordEvent(50)
    const secondo = d.recordEvent(100)
    expect(secondo).toBeGreaterThan(primo)
  })

  test('il ritardo non supera il massimo', () => {
    const d = createAdaptiveDelay({ baseMs: 100, maxMs: 400, quietMs: 1000 })
    let ultimo = 0
    for (let t = 0; t < 2000; t += 10) ultimo = d.recordEvent(t)
    expect(ultimo).toBe(400)
  })

  test('dopo la quiete il ritardo torna alla base', () => {
    const d = createAdaptiveDelay({ baseMs: 100, maxMs: 800, quietMs: 1000 })
    for (let t = 0; t < 500; t += 10) d.recordEvent(t)
    expect(d.currentDelayMs).toBeGreaterThan(100)
    const dopoLaQuiete = d.recordEvent(500 + 1001)
    expect(dopoLaQuiete).toBe(100)
  })

  test('reset riporta alla base', () => {
    const d = createAdaptiveDelay({ baseMs: 100, maxMs: 800, quietMs: 1000 })
    for (let t = 0; t < 500; t += 10) d.recordEvent(t)
    d.reset()
    expect(d.currentDelayMs).toBe(100)
  })
})
```

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/shared/adaptive-delay.test.ts`
Atteso: FAIL, `Cannot find module './adaptive-delay.ts'`.

- [ ] **Passo 3: scrivere l'implementazione minima**

```ts
// src/shared/adaptive-delay.ts

const BASE_MS = 120
const MAX_MS = 900
const QUIETE_MS = 1500
const FATTORE = 1.6

export interface AdaptiveDelay {
  readonly currentDelayMs: number
  /** Registra un evento e restituisce il ritardo da usare adesso. */
  recordEvent(nowMs: number): number
  reset(): void
}

/**
 * Un ritardo che cresce finche' gli eventi continuano ad arrivare e si azzera
 * alla prima quiete. Serve al watcher: un `npm install` dentro il worktree
 * genera migliaia di eventi, e un debounce a ritardo fisso ricaricherebbe la
 * lista decine di volte mentre l installazione e' ancora in corso.
 *
 * Il tempo arriva come argomento invece che da `Date.now()`: cosi' il
 * comportamento si prova senza timer finti, passando semplicemente i tempi.
 */
export function createAdaptiveDelay(opts?: {
  baseMs?: number
  maxMs?: number
  quietMs?: number
}): AdaptiveDelay {
  const base = opts?.baseMs ?? BASE_MS
  const massimo = opts?.maxMs ?? MAX_MS
  const quiete = opts?.quietMs ?? QUIETE_MS

  let corrente = base
  let ultimoEvento: number | null = null

  return {
    get currentDelayMs() {
      return corrente
    },
    recordEvent(nowMs: number): number {
      const precedente = ultimoEvento
      ultimoEvento = nowMs
      if (precedente === null || nowMs - precedente > quiete) {
        corrente = base
        return corrente
      }
      corrente = Math.min(massimo, Math.round(corrente * FATTORE))
      return corrente
    },
    reset(): void {
      corrente = base
      ultimoEvento = null
    }
  }
}
```

- [ ] **Passo 4: eseguire il test e vederlo passare**

Comando: `npx vitest run src/shared/adaptive-delay.test.ts`
Atteso: PASS, 5 test.

- [ ] **Passo 5: verificare per mutazione**

Sostituire `Math.min(massimo, ...)` con `Math.round(corrente * FATTORE)`.
Atteso: ROSSO su "il ritardo non supera il massimo". Ripristinare.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/shared/adaptive-delay.ts src/shared/adaptive-delay.test.ts
git add src/shared/adaptive-delay.ts src/shared/adaptive-delay.test.ts
git commit -m "feat: delay that grows under bursts"
```

---

## Task 9: Watcher del worktree

**File:**
- Crea: `src/main/git/watch.ts`
- Test: `src/main/git/watch.test.ts`
- Modifica: `package.json` (dipendenza `chokidar`)

**Interfacce:**
- Consuma: `createAdaptiveDelay` da `../../shared/adaptive-delay.ts`.
- Produce:

```ts
interface WorktreeWatcher {
  close(): Promise<void>
}
function watchWorktree(opts: {
  repoPath: string
  onChange: () => void
}): WorktreeWatcher
function shouldIgnore(relativePath: string): boolean
```

Il Task 12 usa `watchWorktree`; `shouldIgnore` è esportata per provarla da sola.

- [ ] **Passo 1: installare la dipendenza**

```bash
pnpm add chokidar
```

- [ ] **Passo 2: scrivere il test che fallisce**

```ts
// src/main/git/watch.test.ts
import { describe, expect, test } from 'vitest'
import { writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { makeTestRepo } from './test-repo.ts'
import { shouldIgnore, watchWorktree } from './watch.ts'

describe('shouldIgnore', () => {
  test('ignora node_modules', () => {
    expect(shouldIgnore('node_modules/pacchetto/index.js')).toBe(true)
  })

  test('ignora gli oggetti di git', () => {
    expect(shouldIgnore('.git/objects/ab/cdef')).toBe(true)
  })

  test('NON ignora .git/HEAD, che segnala un commit', () => {
    expect(shouldIgnore('.git/HEAD')).toBe(false)
  })

  test('NON ignora .git/index, che segnala lo staging', () => {
    expect(shouldIgnore('.git/index')).toBe(false)
  })

  test('non ignora un file sorgente', () => {
    expect(shouldIgnore('src/main/git/diff.ts')).toBe(false)
  })
})

describe('watchWorktree', () => {
  test('segnala una scrittura entro un tempo ragionevole', async () => {
    const repo = await makeTestRepo()
    let segnalazioni = 0
    const watcher = watchWorktree({ repoPath: repo, onChange: () => { segnalazioni += 1 } })

    await new Promise((r) => setTimeout(r, 300))
    await writeFile(join(repo, 'nuovo.txt'), 'contenuto')
    await new Promise((r) => setTimeout(r, 1500))
    await watcher.close()

    expect(segnalazioni).toBeGreaterThan(0)
  })

  test('una raffica produce molte meno segnalazioni che eventi', async () => {
    const repo = await makeTestRepo()
    let segnalazioni = 0
    const watcher = watchWorktree({ repoPath: repo, onChange: () => { segnalazioni += 1 } })

    await new Promise((r) => setTimeout(r, 300))
    for (let i = 0; i < 60; i++) {
      await writeFile(join(repo, `f${i}.txt`), String(i))
    }
    await new Promise((r) => setTimeout(r, 2500))
    await watcher.close()

    expect(segnalazioni).toBeGreaterThan(0)
    expect(segnalazioni).toBeLessThan(10)
  })
})
```

- [ ] **Passo 3: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/git/watch.test.ts`
Atteso: FAIL, `Cannot find module './watch.ts'`.

- [ ] **Passo 4: scrivere l'implementazione minima**

```ts
// src/main/git/watch.ts
import chokidar from 'chokidar'
import { createAdaptiveDelay } from '../../shared/adaptive-delay.ts'

/**
 * Dentro `.git/` quasi tutto e' rumore — gli oggetti si riscrivono in
 * continuazione — ma due file sono segnali veri: `HEAD` cambia a ogni commit e
 * checkout, `index` a ogni stage. Ignorarli renderebbe la lista cieca proprio
 * ai due gesti che la cambiano di piu'.
 */
const DENTRO_GIT_DA_GUARDARE = new Set(['.git/HEAD', '.git/index'])

const CARTELLE_IGNORATE = ['node_modules/', 'dist/', 'out/', '.build/', 'DerivedData/']

export function shouldIgnore(relativePath: string): boolean {
  if (DENTRO_GIT_DA_GUARDARE.has(relativePath)) return false
  if (relativePath === '.git' || relativePath.startsWith('.git/')) return true
  return CARTELLE_IGNORATE.some(
    (c) => relativePath === c.slice(0, -1) || relativePath.startsWith(c)
  )
}

export interface WorktreeWatcher {
  close(): Promise<void>
}

export function watchWorktree(opts: { repoPath: string; onChange: () => void }): WorktreeWatcher {
  const ritardo = createAdaptiveDelay()
  let timer: ReturnType<typeof setTimeout> | null = null
  let chiuso = false

  const watcher = chokidar.watch(opts.repoPath, {
    ignoreInitial: true,
    cwd: opts.repoPath,
    ignored: (percorso: string) => shouldIgnore(percorso)
  })

  watcher.on('all', () => {
    if (chiuso) return
    const attesa = ritardo.recordEvent(Date.now())
    if (timer !== null) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = null
      if (!chiuso) opts.onChange()
    }, attesa)
  })

  return {
    async close(): Promise<void> {
      chiuso = true
      if (timer !== null) {
        clearTimeout(timer)
        timer = null
      }
      await watcher.close()
    }
  }
}
```

- [ ] **Passo 5: eseguire il test e vederlo passare**

Comando: `npx vitest run src/main/git/watch.test.ts`
Atteso: PASS, 7 test. I due test temporali sono i più lenti della suite: se
risultano instabili sulla macchina, allungare le attese, **non** allentare le
asserzioni.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/main/git/watch.ts src/main/git/watch.test.ts
git add package.json pnpm-lock.yaml src/main/git/watch.ts src/main/git/watch.test.ts
git commit -m "feat: watch a worktree with an adaptive debounce"
```

---

## Task 10: Invariante dell'anteprima e chiave di contenuto

**File:**
- Modifica: `src/shared/workspace/layout-invariants.ts`
- Test: `src/shared/workspace/layout-invariants.test.ts`

**Interfacce:**
- Consuma: `WorkspaceContentRef`, `WorkspaceTab` da `./layout-types.ts`.
- Produce: `contentKey(content: WorkspaceContentRef): string` e il nuovo membro
  `{ kind: 'multiplePreviewTabs'; groupId: PaneGroupID }` di `LayoutError`.
  Il Task 11 li consuma.

**Questo task viene PRIMA del campo `isPreview`, di proposito.** Nell'ordine
opposto esisterebbe una finestra in cui il layout può salvare due anteprime, e
quello stato finirebbe persistito su disco, dove nessun controllo successivo lo
troverebbe più.

Il task è eseguibile prima del campo perché `contentKey` si può introdurre
subito (oggi ha un solo caso) e l'invariante si può scrivere contro un campo che
il Task 11 aggiunge: fino ad allora il test dell'invariante resta rosso, ed è
il rosso che il Task 11 spegne. Per non lasciare la base rossa fra i due task,
**il campo `isPreview` si aggiunge qui**, in `layout-types.ts`, e il Task 11
si occupa di codec, motore e comandi.

- [ ] **Passo 1: scrivere il test che fallisce**

Aggiungere in `src/shared/workspace/layout-invariants.test.ts`:

```ts
test('due tab in anteprima nello stesso gruppo violano l invariante', () => {
  const gruppo = newPaneGroupID()
  const uno = { ...tabDiProva('a'), isPreview: true }
  const due = { ...tabDiProva('b'), isPreview: true }
  const esito = makeLayout(
    { kind: 'group', id: gruppo },
    new Map([[gruppo, { id: gruppo, tabs: [uno, due], activeTabId: uno.id }]]),
    gruppo
  )
  expect(esito.ok).toBe(false)
  if (esito.ok) return
  expect(esito.error).toEqual({ kind: 'multiplePreviewTabs', groupId: gruppo })
})

test('una sola anteprima per gruppo e lecita', () => {
  const gruppo = newPaneGroupID()
  const uno = { ...tabDiProva('a'), isPreview: true }
  const due = { ...tabDiProva('b'), isPreview: false }
  const esito = makeLayout(
    { kind: 'group', id: gruppo },
    new Map([[gruppo, { id: gruppo, tabs: [uno, due], activeTabId: uno.id }]]),
    gruppo
  )
  expect(esito.ok).toBe(true)
})

test('lo stesso file aperto nelle due sorgenti sono due contenuti distinti', () => {
  expect(contentKey({ kind: 'diff', path: 'a.ts', source: 'worktree' })).not.toBe(
    contentKey({ kind: 'diff', path: 'a.ts', source: 'branch' })
  )
})

test('lo stesso file nella stessa sorgente e lo stesso contenuto', () => {
  expect(contentKey({ kind: 'diff', path: 'a.ts', source: 'worktree' })).toBe(
    contentKey({ kind: 'diff', path: 'a.ts', source: 'worktree' })
  )
})
```

`tabDiProva` è l'helper già presente nel file; leggerlo e adattare il nome se
differisce. Aggiungere `contentKey` agli import.

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/shared/workspace/layout-invariants.test.ts`
Atteso: FAIL — `contentKey` non esiste e `isPreview` non è nel tipo.

- [ ] **Passo 3: aggiungere il campo e il membro del contenuto**

In `src/shared/workspace/layout-types.ts`:

```ts
export const WorkspaceContentRefSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('terminal'), id: z.string().min(1) }),
  // La sorgente fa parte dell identita': lo stesso file nel non committato e
  // nel ramo mostra due cose diverse, quindi sono due contenuti, non uno.
  z.object({
    kind: z.literal('diff'),
    path: z.string().min(1),
    source: DiffSourceSchema
  })
])
```

con `import { DiffSourceSchema } from '../diff/types.ts'` in cima, e sul tab:

```ts
export const WorkspaceTabSchema = z.object({
  id: WorkspaceTabIDSchema,
  title: z.string(),
  titleIsAutoNamed: z.boolean(),
  content: WorkspaceContentRefSchema,
  viewState: WorkspaceTabViewStateSchema,
  /**
   * Tab provvisoria: il click successivo la rimpiazza invece di aprirne una
   * nuova. NON si persiste — al ripristino tutte le tab tornano permanenti,
   * perche' una sessione appena riaperta non deve contenere una tab che
   * sparisce al primo click.
   */
  isPreview: z.boolean()
})
```

Aggiornare il commento sopra `WorkspaceContentRefSchema`, che oggi dice "le
Fasi 5 e 6 aggiungeranno `chat` e `document`".

- [ ] **Passo 4: aggiungere `contentKey` e l'invariante**

In `src/shared/workspace/layout-invariants.ts`:

```ts
import type { WorkspaceContentRef } from './layout-types.ts'

/**
 * La chiave di proprieta' del contenuto. Non e' piu' `content.id`: il
 * contenuto `diff` non ha un id, ha un percorso e una sorgente.
 *
 * A cosa serve: due tab non possono mostrare lo stesso contenuto. Per il
 * terminale la ragione e' la vita del processo — chiuderne uno ucciderebbe il
 * processo sotto l altro. Per il diff la ragione e' diversa ma la regola
 * conviene lo stesso: rende "clicco un file gia' aperto" per forza un
 * portarlo in primo piano, invece di un duplicato.
 */
export function contentKey(content: WorkspaceContentRef): string {
  switch (content.kind) {
    case 'terminal':
      return `terminal:${content.id}`
    case 'diff':
      return `diff:${content.source}:${content.path}`
  }
}
```

Nel tipo `LayoutError` aggiungere:

```ts
  | { kind: 'multiplePreviewTabs'; groupId: PaneGroupID }
```

Dentro `validate`, sostituire il blocco che oggi legge `tab.content.id`
(righe 112-115) e aggiungere il conteggio delle anteprime:

```ts
  for (const gruppo of groups.values()) {
    if (gruppo.tabs.length === 0 && !eRadiceSola) {
      return { kind: 'emptyNonRootGroup', groupId: gruppo.id }
    }
    if (gruppo.activeTabId !== null && !gruppo.tabs.some((t) => t.id === gruppo.activeTabId)) {
      return { kind: 'activeTabNotInGroup', groupId: gruppo.id }
    }
    let anteprime = 0
    for (const tab of gruppo.tabs) {
      if (tabVisti.has(tab.id)) return { kind: 'duplicateID', id: tab.id }
      tabVisti.add(tab.id)
      const chiave = contentKey(tab.content)
      if (contenuti.has(chiave)) {
        return { kind: 'duplicateContentOwnership', contentId: chiave }
      }
      contenuti.add(chiave)
      if (tab.isPreview) anteprime += 1
    }
    // Al massimo una anteprima per gruppo: due renderebbero indeterminato
    // quale rimpiazzare al click successivo.
    if (anteprime > 1) return { kind: 'multiplePreviewTabs', groupId: gruppo.id }
  }
```

- [ ] **Passo 5: aggiustare i punti che il compilatore fa fallire**

Comando: `pnpm typecheck`

Ogni costruzione di `WorkspaceTab` ora richiede `isPreview`. I punti attesi
sono i test del workspace, `src/main/control/dispatch.ts` (in
`bootstrapPaneTab`) e le fixture. Aggiungere `isPreview: false` a ciascuno: è
il valore giusto per ogni tab esistente, perché nessuna di esse è provvisoria.

- [ ] **Passo 6: eseguire i test e vederli passare**

Comando: `npx vitest run src/shared/workspace/`
Atteso: PASS, la suite del workspace più i 4 test nuovi.

- [ ] **Passo 7: verificare per mutazione**

Cambiare `anteprime > 1` in `anteprime > 2`. Atteso: ROSSO su "due tab in
anteprima nello stesso gruppo violano l invariante". Ripristinare.

Seconda mutazione: in `contentKey`, far restituire al caso `diff` solo
`` `diff:${content.path}` `` senza la sorgente. Atteso: ROSSO su "lo stesso file
aperto nelle due sorgenti sono due contenuti distinti". Ripristinare.

- [ ] **Passo 8: lint e commit**

```bash
npx eslint src/shared/workspace/
git add src/shared/workspace/ src/main/control/dispatch.ts
git commit -m "feat: preview tab invariant and content ownership key"
```

---

## Task 11: Codec e comandi dell'anteprima

**File:**
- Modifica: `src/shared/workspace/layout-codec.ts:108-117`
- Modifica: `src/shared/workspace/layout-engine.ts`
- Test: `src/shared/workspace/layout-codec.test.ts`,
  `src/shared/workspace/layout-engine.test.ts`

**Interfacce:**
- Consuma: `contentKey` dal Task 10, `WorkspaceContentRef` con il membro `diff`.
- Produce: i comandi `{ kind: 'openPreview'; tab: WorkspaceTab; into: PaneGroupID }`
  e `{ kind: 'promoteTab'; tabId: WorkspaceTabID }` nel motore. Il Task 15 li usa.

- [ ] **Passo 1: scrivere il test che fallisce**

In `layout-codec.test.ts`:

```ts
test('un tab di diff sopravvive al giro completo', () => {
  const gruppo = newPaneGroupID()
  const tab = {
    id: newWorkspaceTabID(),
    title: 'diff.ts',
    titleIsAutoNamed: true,
    content: { kind: 'diff' as const, path: 'src/main/git/diff.ts', source: 'worktree' as const },
    viewState: emptyViewState(),
    isPreview: false
  }
  const esito = makeLayout(
    { kind: 'group', id: gruppo },
    new Map([[gruppo, { id: gruppo, tabs: [tab], activeTabId: tab.id }]]),
    gruppo
  )
  expect(esito.ok).toBe(true)
  if (!esito.ok) return

  const scritto = serializza(esito.layout)
  const riletto = decodeLayout(scritto.payload, scritto.tabs)
  expect(riletto.ok).toBe(true)
  if (!riletto.ok) return
  expect(allTabs(riletto.layout)[0].content).toEqual({
    kind: 'diff',
    path: 'src/main/git/diff.ts',
    source: 'worktree'
  })
})

test('una anteprima si rilegge come permanente', () => {
  const gruppo = newPaneGroupID()
  const tab = {
    id: newWorkspaceTabID(),
    title: 'diff.ts',
    titleIsAutoNamed: true,
    content: { kind: 'diff' as const, path: 'a.ts', source: 'worktree' as const },
    viewState: emptyViewState(),
    isPreview: true
  }
  const esito = makeLayout(
    { kind: 'group', id: gruppo },
    new Map([[gruppo, { id: gruppo, tabs: [tab], activeTabId: tab.id }]]),
    gruppo
  )
  if (!esito.ok) throw new Error('layout non valido')
  const scritto = serializza(esito.layout)
  const riletto = decodeLayout(scritto.payload, scritto.tabs)
  expect(riletto.ok).toBe(true)
  if (!riletto.ok) return
  // Deliberato: una sessione ripristinata non deve avere una tab che sparisce
  // al primo click.
  expect(allTabs(riletto.layout)[0].isPreview).toBe(false)
})

test('un contenuto sconosciuto finisce ancora in quarantena', () => {
  const esito = decodeLayout('{}', [
    {
      id: 'x',
      title: 't',
      titleIsAutoNamed: 0,
      contentKind: 'inventato',
      contentId: 'y',
      viewStateJSON: null
    } as never
  ])
  expect(esito.ok).toBe(false)
})
```

Adattare i nomi (`serializza`, `decodeLayout`, la forma delle righe) leggendo il
file di test esistente: il codec ha già i suoi helper.

In `layout-engine.test.ts`:

```ts
test('openPreview rimpiazza la anteprima esistente nel gruppo', () => {
  // Arrange: un gruppo con una anteprima aperta.
  // Act: openPreview di un secondo tab.
  // Assert: il gruppo ha UNA sola tab, ed e la seconda.
})

test('openPreview non tocca le tab permanenti', () => {
  // Arrange: un gruppo con una tab permanente e una anteprima.
  // Act: openPreview di un terzo tab.
  // Assert: restano due tab, la permanente e la nuova anteprima.
})

test('promoteTab rende permanente e il click successivo apre una nuova tab', () => {
  // Arrange: un gruppo con una anteprima.
  // Act: promoteTab su quella, poi openPreview di un altro tab.
  // Assert: due tab, entrambe presenti, una sola con isPreview true.
})
```

Questi tre vanno scritti per intero seguendo lo stile dei test già presenti in
`layout-engine.test.ts`, che costruiscono il layout con `makeLayout` e
applicano i comandi con la funzione `apply` del motore. Leggere il file prima:
la firma di `apply` e la forma del risultato (`{ ok, layout, delta }`) sono già
fissate dalla Fase 4a e non vanno cambiate.

- [ ] **Passo 2: eseguire i test e vederli fallire**

Comando: `npx vitest run src/shared/workspace/`
Atteso: FAIL — il codec rifiuta `contentKind: 'diff'`, i comandi non esistono.

- [ ] **Passo 3: estendere il codec**

In `layout-codec.ts`, sostituire il blocco alle righe 108-117:

```ts
    if (riga.contentKind === 'terminal') {
      perId.set(riga.id, {
        id: riga.id as WorkspaceTabID,
        title: riga.title,
        titleIsAutoNamed: riga.titleIsAutoNamed === 1,
        content: { kind: 'terminal', id: riga.contentId },
        viewState: viewState.data,
        isPreview: false
      })
      continue
    }
    if (riga.contentKind === 'diff:worktree' || riga.contentKind === 'diff:branch') {
      perId.set(riga.id, {
        id: riga.id as WorkspaceTabID,
        title: riga.title,
        titleIsAutoNamed: riga.titleIsAutoNamed === 1,
        content: {
          kind: 'diff',
          path: riga.contentId,
          source: riga.contentKind === 'diff:branch' ? 'branch' : 'worktree'
        },
        viewState: viewState.data,
        // Deliberato: l anteprima non si persiste, si rilegge permanente.
        isPreview: false
      })
      continue
    }
    return { ok: false, reason: `contenuto sconosciuto: ${riga.contentKind}` }
```

La sorgente viaggia dentro `contentKind` invece di aggiungere una colonna:
`contentId` resta un identificatore singolo e non serve una migrazione dello
schema. Aggiornare di conseguenza il lato di scrittura (`serializza`), che oggi
scrive `contentKind: 'terminal'` fisso.

- [ ] **Passo 4: aggiungere i comandi al motore**

In `layout-engine.ts`, estendere l'unione dei comandi e gestirli:

```ts
  | { kind: 'openPreview'; tab: WorkspaceTab; into: PaneGroupID }
  | { kind: 'promoteTab'; tabId: WorkspaceTabID }
```

`openPreview` inserisce il tab con `isPreview: true` nel gruppo indicato,
rimuovendo prima l'eventuale anteprima già presente **in quel gruppo**;
`promoteTab` riscrive il tab indicato con `isPreview: false`.

Entrambi devono passare per lo stesso `makeLayout` degli altri comandi, così
l'invariante del Task 10 li controlla: un comando che costruisse il layout a
mano salterebbe il controllo.

- [ ] **Passo 5: eseguire i test e vederli passare**

Comando: `npx vitest run src/shared/workspace/`
Atteso: PASS.

- [ ] **Passo 6: verificare per mutazione**

In `openPreview`, togliere la rimozione dell'anteprima esistente. Atteso: ROSSO
su "openPreview rimpiazza la anteprima esistente nel gruppo" **e** su
"due tab in anteprima nello stesso gruppo violano l invariante" del Task 10 —
la seconda dimostra che l'invariante sta davvero facendo da rete. Ripristinare.

- [ ] **Passo 7: lint e commit**

```bash
npx eslint src/shared/workspace/
git add src/shared/workspace/
git commit -m "feat: persist diff tabs and add preview commands"
```

---

## Task 12: Protocollo e dispatcher

**File:**
- Modifica: `src/shared/protocol.ts`
- Modifica: `src/main/control/dispatch.ts`
- Test: `src/main/control/dispatch.test.ts`

**Interfacce:**
- Consuma: `listChanged`, `loadFileDiff`, `resolveBase`, `watchWorktree`.
- Produce: le richieste `git.changes`, `git.diff`, `git.bases` e l'evento
  `git.changed`. I Task 13, 14 e 15 li usano.

Protocollo e dispatcher stanno **nello stesso task**: l'unione dei
`ControlRequest` è un tipo somma esaustivo, e aggiungerne un membro senza il
ramo che lo gestisce lascia il progetto rosso fra i due task.

- [ ] **Passo 1: scrivere il test che fallisce**

In `src/main/control/dispatch.test.ts`, seguendo l'helper `makeChatDeps` già
presente per costruire le dipendenze finte:

```ts
test('git.changes elenca i file non committati del worktree', async () => {
  // Arrange: dispatch con un repo di prova che ha un file modificato.
  // Act: dispatch({ kind: 'git.changes', worktreeId, source: 'worktree' }).
  // Assert: ok true, e il file compare con i suoi conteggi.
})

test('git.changes su un worktree che non e un repo git risponde con il motivo', async () => {
  // Assert: ok false, motivo che nomina il repository.
})

test('git.diff rifiuta un percorso fuori dal repository', async () => {
  // Act: dispatch({ kind: 'git.diff', worktreeId, path: '../fuori', source: 'worktree' }).
  // Assert: ok false, motivo "fuori dal repository".
})

test('git.bases elenca i rami e segnala il default', async () => {
  // Assert: la base risolta compare nella lista.
})
```

Scriverli per intero seguendo i test già presenti nel file: `makeChatDeps`
mostra come si costruisce un `dispatch` provabile, e `makeTestRepo` come si
ottiene un repo vero.

- [ ] **Passo 2: eseguire il test e vederlo fallire**

Comando: `npx vitest run src/main/control/dispatch.test.ts`
Atteso: FAIL — le richieste non esistono nell'unione.

- [ ] **Passo 3: estendere il protocollo**

In `src/shared/protocol.ts`, aggiungere all'unione delle richieste:

```ts
  | { kind: 'git.changes'; worktreeId: string; source: DiffSource; base: string | null }
  | { kind: 'git.diff'; worktreeId: string; path: string; source: DiffSource; base: string | null }
  | { kind: 'git.bases'; worktreeId: string }
```

e all'unione degli eventi:

```ts
  | { type: 'git.changed'; worktreeId: string }
```

L'evento non porta la lista: porta solo la notizia che qualcosa è cambiato, e
chi la riceve richiede `git.changes`. Mandare la lista dentro l'evento
significherebbe calcolarla anche quando nessuno la sta guardando.

- [ ] **Passo 4: gestire le richieste nel dispatcher**

In `src/main/control/dispatch.ts`, aggiungere i tre rami allo `switch`. Il
percorso del repo si ottiene dal worktree già noto al dispatcher; il caso
"non è un repo git" si risolve con `isGitRepository` da `../git/repo.ts`.

Il watcher si avvia alla prima `git.changes` per un worktree e si chiude quando
il worktree si chiude, dentro lo stesso punto in cui oggi si chiudono i pane.
Tenerne al più uno per worktree in una `Map<string, WorktreeWatcher>`.

**Attenzione allo spegnimento:** i watcher vanno chiusi in `src/main/index.ts`
insieme alle altre risorse, prima di `closeDatabase(db)`, altrimenti un evento
in arrivo dopo la chiusura del database tocca una connessione chiusa.

- [ ] **Passo 5: eseguire i test e vederli passare**

Comando: `npx vitest run src/main/control/dispatch.test.ts`
Atteso: PASS.

- [ ] **Passo 6: lint e commit**

```bash
npx eslint src/shared/protocol.ts src/main/control/dispatch.ts src/main/control/dispatch.test.ts
git add src/shared/protocol.ts src/main/control/ src/main/index.ts
git commit -m "feat: git changes and diff over the control protocol"
```

---

## Task 13: Pannello e lista dei cambiati

**File:**
- Crea: `src/renderer/lib/RightPanel.svelte`
- Crea: `src/renderer/lib/ChangesList.svelte`
- Modifica: il componente che compone la finestra (leggere `App.svelte` per
  trovare dove montare il pannello)

**Interfacce:**
- Consuma: `git.changes`, `git.bases`, l'evento `git.changed` dal Task 12.
- Produce: la callback `onOpenDiff(path: string, source: DiffSource, permanent: boolean)`,
  che il Task 15 collega.

- [ ] **Passo 1: costruire `RightPanel.svelte`**

Il selettore ha due voci: `Files` e `Changes`. `Files` è **disabilitato** con
titolo "disponibile con l'editor" — la sua implementazione è Fase 6b. `Changes`
è disabilitato quando il worktree non è un repository git, con la ragione
visibile: il pannello non sparisce, si spiega.

Il pannello legge il worktree **selezionato nella sidebar**: è da lì che vengono
`worktreeId` e percorso.

- [ ] **Passo 2: costruire `ChangesList.svelte`**

Contiene:

- l'intestazione con il selettore di sorgente: `Non committato` / `Ramo`
- sulla sorgente `branch`, il selettore della base, popolato da `git.bases`,
  che mostra `<base> → <ramo corrente>`
- la lista dei file: percorso, e a destra `+N` in verde e `−M` in rosso
- sulla sorgente `worktree` soltanto, le azioni stage / unstage / scarta

La base scelta vive nello stato del componente per quel worktree e **non si
persiste**: torna al default a ogni avvio.

- [ ] **Passo 3: reagire all'evento**

Alla ricezione di `git.changed` per il worktree corrente, richiedere
`git.changes`. L'attenuazione l'ha già fatta il watcher nel main: il renderer
non deve aggiungere un secondo debounce, o i due si sommerebbero in un ritardo
che nessuno dei due dichiara.

- [ ] **Passo 4: verificare a mano**

```bash
pnpm dev
```

Aprire un worktree con file modificati: la lista li mostra con i conteggi.
Passare a `Ramo`: la lista cambia e le azioni spariscono.

- [ ] **Passo 5: lint e commit**

```bash
npx eslint src/renderer/
pnpm svelte-check
git add src/renderer/
git commit -m "feat: right panel with the changed files list"
```

---

## Task 14: Resa del diff

**File:**
- Crea: `src/renderer/lib/DiffTab.svelte`
- Modifica: il renderer dei contenuti di tab (dove oggi si sceglie fra i
  contenuti; leggere il componente che monta i terminali)

**Interfacce:**
- Consuma: `rows` da `src/shared/diff/side-by-side.ts`, `git.diff` dal Task 12.
- Produce: il componente montato quando `tab.content.kind === 'diff'`.

- [ ] **Passo 1: montare il contenuto giusto**

Nel punto che oggi monta il terminale, aggiungere il ramo `diff`. Essendo
`WorkspaceContentRef` un'unione discriminata, il compilatore indica già dove.

- [ ] **Passo 2: costruire `DiffTab.svelte`**

Richiede `git.diff` per il proprio `path` e `source`, poi disegna
`rows(diff.lines)` in due colonne:

- una riga di contesto: stesso testo a sinistra e a destra, numeri di riga a
  entrambi i lati
- una cancellazione a sinistra su fondo rosso tenue, una aggiunta a destra su
  fondo verde tenue
- un lato `null`: cella vuota, non una riga saltata — l'allineamento fra le due
  colonne è tutto il punto della vista
- una intestazione di hunk: una riga sola a tutta larghezza
- `isBinary`: una riga "file binario"
- `isSubmodule`: una riga con il vecchio e il nuovo commit
- errore da `git.diff` (compreso "troppo grande"): il motivo, leggibile

Font a spaziatura fissa e i token di colore esistenti; niente nuovi colori
inventati.

- [ ] **Passo 3: verificare a mano**

```bash
pnpm dev
```

Aprire il diff di un file con aggiunte e cancellazioni di lunghezza diversa:
le due colonne restano allineate e le celle mancanti sono vuote.

- [ ] **Passo 4: lint e commit**

```bash
npx eslint src/renderer/
pnpm svelte-check
git add src/renderer/
git commit -m "feat: side-by-side diff tab"
```

---

## Task 15: Il ponte fra il click e la tab

**File:**
- Modifica: `src/renderer/lib/ChangesList.svelte` (emissione del gesto)
- Modifica: il contenitore che possiede il layout del worktree (chi chiama
  `workspace.apply`)

**Interfacce:**
- Consuma: `openPreview` e `promoteTab` dal Task 11, `contentKey` dal Task 10.
- Produce: il comportamento che i criteri e2e del Task 16 verificano.

**Questo è il task che nella Fase 4b mancava.** Quindici task costruivano
consumatori e nessuno il produttore: il motore era completo, testato e mai
chiamato, e i test passavano lo stesso perché tutto ciò che esisteva era
coperto. Qui il produttore è questo task, e il Task 16 lo verifica partendo dal
click.

- [ ] **Passo 1: collegare il click singolo**

Il click su una riga della lista:

1. cerca fra le tab del worktree una con `contentKey` uguale a quella del file
   cliccato per la sorgente corrente;
2. se esiste, la attiva — niente duplicati;
3. se non esiste, applica `openPreview` con un tab nuovo il cui titolo è il
   nome del file (non il percorso, non un UUID), `titleIsAutoNamed: true`, e
   contenuto `{ kind: 'diff', path, source }`.

Il punto 3 merita attenzione: nella Fase 4b una funzione analoga intitolava le
tab con l'UUID del worktree invece del nome. Il titolo è `basename(path)`.

- [ ] **Passo 2: collegare il doppio click**

Il doppio click applica `promoteTab` sul tab appena aperto (o su quello già
esistente per quel file), rendendolo permanente.

- [ ] **Passo 3: verificare a mano**

```bash
pnpm dev
```

Tre click su tre file diversi: resta **una** tab, l'ultima. Doppio click, poi
un altro click: due tab.

- [ ] **Passo 4: lint e commit**

```bash
npx eslint src/renderer/
pnpm svelte-check
git add src/renderer/
git commit -m "feat: open a diff tab from the changes list"
```

---

## Task 16: Criteri end-to-end

**File:**
- Crea: `e2e/fase-6a-diff.spec.ts`

I criteri partono dal **gesto**, non dal comando. Un criterio che chiama
`workspace.apply` per aprire la tab e poi ne verifica il contenuto passerebbe
anche con la lista scollegata.

- [ ] **Passo 1: scrivere i sei criteri**

Seguire `e2e/fase-4b-viste.spec.ts` per gli helper (avvio dell'app,
`worktreeReale`, attese). I criteri:

1. **La lista elenca il file modificato con i conteggi giusti.** In un worktree
   con un file a cui si aggiungono due righe e se ne toglie una, la riga della
   lista mostra `+2` e `−1`.
2. **Il click apre una tab con il diff di quel file.** Click sulla riga; compare
   una tab il cui titolo è il nome del file e il cui contenuto mostra il testo
   della riga aggiunta.
3. **L'anteprima si rimpiazza, la promozione no.** Click su A, click su B: una
   tab sola, che mostra B. Doppio click, click su C: due tab.
4. **La vista Ramo esclude il lavoro fatto sulla base dopo il fork.** Costruire
   il caso con git nel repo di prova; la lista mostra il file del ramo e **non**
   quello aggiunto sulla base.
5. **Le azioni di stage esistono solo sul non committato.** Sulla vista Ramo il
   controllo di stage non è presente nel DOM.
6. **Una modifica da fuori dell'app compare senza interazione.** Scrivere un
   file con `fs` mentre l'app è aperta; entro pochi secondi la riga compare
   nella lista.

- [ ] **Passo 2: eseguire e vederli passare**

```bash
npx electron-vite build
npx playwright test e2e/fase-6a-diff.spec.ts
```

- [ ] **Passo 3: verificare per mutazione**

Tre mutazioni, una per volta, ricostruendo ogni volta con
`npx electron-vite build` (**non** `pnpm build`):

| Mutazione | Criterio atteso rosso |
|---|---|
| il click nella lista non chiama `openPreview` | 2 |
| `openPreview` non rimuove l'anteprima esistente | 3 |
| i tre punti diventano due in `changes.ts` | 4 |

Se una resta verde, il criterio non prova ciò che promette: correggere il
criterio, non la mutazione. Annullare ciascuna con `git checkout -- <file>` e
riverificare il verde prima della successiva.

- [ ] **Passo 4: gate completo**

```bash
bash scripts/ci.sh
```

Atteso: `CI OK`.

- [ ] **Passo 5: lint e commit**

```bash
npx eslint e2e/fase-6a-diff.spec.ts
git add e2e/fase-6a-diff.spec.ts
git commit -m "test: end-to-end criteria for the diff view"
```

---

## Autoverifica del piano

**Copertura della spec:**

| Requisito della spec | Task |
|---|---|
| diff come contenuto di tab | 10, 11, 14, 15 |
| `Diff`+`Status` fusi in `Changes` | 6, 13 |
| due sorgenti, una resa | 4, 6, 13 |
| tre punti / merge-base | 4, 5, 6 |
| base da `origin/HEAD` con ripiego, non persistita | 5, 13 |
| pannello allineato al worktree della sidebar | 13 |
| git autorità sul cambiamento (`parse-diff`) | 3 |
| `rows()` portata | 2 |
| limiti 5 MB / 20.000 righe | 4 |
| anteprima e promozione | 10, 11, 15 |
| invariante prima del campo | 10 |
| watcher con debounce adattivo | 8, 9 |
| rollup per cartella | 7 |
| casi d'errore | 4, 5, 6, 13, 14 |
| sei criteri e2e dal gesto | 16 |
| `Files` fuori perimetro | 13 (disabilitato con la ragione) |

**Coerenza dei tipi:** `DiffLine`, `FileDiff`, `ChangedFile`, `DiffSource`
sono definiti nel Task 1 e usati con quei nomi in 2, 3, 4, 6, 7, 10, 12.
`contentKey` nasce nel Task 10 ed è consumata in 11 e 15. `SideBySideRow`
nasce nel Task 2 ed è consumata nel 14. `listChanged` e `loadFileDiff` nascono
in 6 e 4 e sono consumate nel 12.

**Nota sull'ordine:** i Task 1-9 non toccano il workspace e si possono
eseguire in qualsiasi ordine fra loro. Dal 10 in poi l'ordine è vincolante:
l'invariante prima del campo, il campo prima del codec, il codec prima del
ponte, il ponte prima dei criteri.
