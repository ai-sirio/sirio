# Fase 6b-2 — Scrivere: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Le caselle `- [ ]`
> servono a tenere il segno. Il gate completo (`bash scripts/ci.sh`) lo esegue
> **Claude**, mai chi implementa.

**Obiettivo:** modificare e salvare qualsiasi file di testo del worktree, senza
mai perdere una modifica in silenzio.

**Architettura:** il buffer vive in un archivio del renderer chiavato per
percorso, fuori dai componenti, perché `PaneGrid` monta solo il tab attivo. La
scrittura è una richiesta nuova sul socket di controllo, atomica via `rename`. Il
conflitto si rileva riusando l'evento `files.changed` già emesso dal watcher, e
si distingue dal proprio salvataggio confrontando l'`mtime`.

**Spec:** `docs/superpowers/specs/2026-08-02-electron-fase-6b2-scrittura-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`. Il repo Swift `~/Desktop/Progetti/tiller/`
  è **sola lettura**, si consulta per la parità e non si modifica mai.
- `.tokensave/**` non si committa mai.
- Test con `vitest` (unità) e `@playwright/test` (e2e). Test prima, sempre.
- Stringhe rivolte all'utente **in inglese**. Commenti e nomi interni in italiano,
  come il resto del repo.
- Conventional Commits, messaggio in inglese minuscolo imperativo.
- `npm run lint` deve dare **0 errori**. Negli ultimi lotti il typecheck era verde
  e eslint rosso tre volte di fila: sono domande diverse.
- Nessuna dipendenza nuova. CodeMirror c'è dalla 6b-1, la scrittura usa
  `node:fs/promises`, già in uso in `src/main/files/`.

## Tre correzioni alla spec, trovate esplorando il codice

Vanno lette prima di iniziare: cambiano il contenuto di tre task.

1. **`files.read` non restituisce l'`mtime`.** `readFileForView`
   (`src/main/files/read.ts`) produce `{ text, isBinary, truncated }`. La guardia
   anti-eco della spec §3 confronta «l'mtime restituito da `files.write` con
   quello riletto» — ma non c'è niente da rileggere. Senza il Task 1 il criterio
   5 è impossibile, non difficile.
2. **`FileTab.svelte:58` ricarica già su `files.changed`**, incondizionatamente e
   per l'intero worktree. Oggi è innocuo perché il tab è in sola lettura; dal
   Task 5 in poi è **la perdita di dati descritta dalla spec**, scritta nel
   codice. Va sostituito, non affiancato.
3. **`⌘W` non esiste.** `closeTab` esiste nel riduttore
   (`src/shared/workspace/layout-engine.ts:197`) ma nessuna interfaccia lo
   invoca: non c'è pulsante di chiusura e non c'è scorciatoia. La spec §4 dà per
   scontato `⌘W` e ci appende una guardia. Il Task 8 deve quindi introdurre
   **anche** la scorciatoia, non solo la guardia.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/shared/files/types.ts` | *modifica*: `mtimeMs` su `FileContent` |
| `src/main/files/read.ts` | *modifica*: leggere e riportare l'`mtime` |
| `src/main/files/write.ts` | *nuovo*: scrittura atomica confinata alla radice |
| `src/shared/protocol.ts` | *modifica*: membro `files.write` |
| `src/main/control/dispatch.ts` | *modifica*: il caso `files.write` |
| `src/renderer/src/lib/workspace/document-state.ts` | *nuovo*: **riduttore puro** del documento |
| `src/renderer/src/lib/workspace/documents.svelte.ts` | *nuovo*: archivio a rune, sottile |
| `src/renderer/src/lib/workspace/FileTab.svelte` | *modifica*: editor, banner |
| `src/renderer/src/lib/workspace/TabSegment.svelte` | *modifica*: pallino di modifica |
| `src/renderer/src/App.svelte` | *modifica*: `⌘S`, `⌘W`, guardia di chiusura |
| `src/main/index.ts` | *modifica*: `window.on('close')` |
| `e2e/fase-6b2-scrittura.spec.ts` | *nuovo*: gli otto criteri |

**Perché il riduttore è separato dall'archivio.** La parte con i casi — pulito o
sporco, file sparito, eco del proprio salvataggio — è la parte che va testata, e
si testa meglio come funzione pura che come modulo a rune. È lo stesso taglio già
usato dalla Fase 4a: `layout-engine.ts` puro, componenti sottili.

---

## Task 1: L'`mtime` sul contenuto letto

**File:**
- Modifica: `src/shared/files/types.ts`
- Modifica: `src/main/files/read.ts`
- Test: `src/main/files/read.test.ts`, `src/shared/files/types.test.ts`

**Interfacce:**
- Produce: `FileContent.mtimeMs: number | null`. Lo consumano i Task 3, 4 e 6.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
// src/main/files/read.test.ts
test('il contenuto letto porta l mtime del file', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-read-'))
  await writeFile(join(dir, 'a.txt'), 'ciao\n')
  const atteso = (await stat(join(dir, 'a.txt'))).mtimeMs

  const contenuto = await readFileForView(dir, 'a.txt')

  expect(contenuto.mtimeMs).toBe(atteso)
})

test('anche un binario porta l mtime', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-read-'))
  await writeFile(join(dir, 'b.bin'), Buffer.from([0x41, 0x00]))

  const contenuto = await readFileForView(dir, 'b.bin')

  expect(contenuto.isBinary).toBe(true)
  expect(contenuto.mtimeMs).not.toBeNull()
})
```

```ts
// src/shared/files/types.test.ts
test('un contenuto senza mtime resta valido', () => {
  const parsed = FileContentSchema.parse({ text: '', isBinary: false, truncated: false })
  expect(parsed.mtimeMs).toBeNull()
})
```

- [ ] **Passo 2: eseguire e vedere fallire**

`npx vitest run src/main/files/ src/shared/files/` → FAIL.

- [ ] **Passo 3: implementare**

```ts
// src/shared/files/types.ts, dentro FileContentSchema
  /**
   * Serve alla guardia anti-eco della 6b-2: dopo un salvataggio il watcher
   * scatta su di noi, e l'unico modo per riconoscere il proprio evento e'
   * confrontare questo numero. `null` significa "non lo so", e chi decide deve
   * trattarlo come modifica esterna — un falso conflitto si chiude con un
   * click, una ricarica sbagliata cancella il lavoro.
   */
  mtimeMs: z.number().nullable().default(null)
```

Il `.default(null)` **non è opzionale**: lo schema viaggia sul socket di
controllo. È la stessa lezione di `isPreview` in Fase 6a e di `oldText` in 6b-1.

In `read.ts` si legge dallo **stesso descrittore** che si è già aperto:

```ts
import { open } from 'node:fs/promises'
// ...
  const target = resolveInsideRoot(rootPath, relativePath)
  const handle = await open(target, 'r')
  let data: Buffer
  let mtimeMs: number
  try {
    // fstat sul descrittore, non stat sul percorso: fra un `stat` e una `readFile`
    // separati il file puo' cambiare, e si finirebbe con l'mtime di una versione
    // e il testo di un'altra. Resta una finestra fra fstat e read, e nessuna API
    // di Node la chiude: e' il limite gia' dichiarato nella spec.
    mtimeMs = (await handle.stat()).mtimeMs
    data = await handle.readFile()
  } finally {
    await handle.close()
  }
```

Tutti i rami di ritorno esistenti (binario, troncato, normale) riportano
`mtimeMs`.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/files/types.ts src/main/files/read.ts src/main/files/read.test.ts src/shared/files/types.test.ts
git commit -m "feat: report the modification time with a file read"
```

---

## Task 2: Scrittura atomica confinata alla radice

**File:**
- Crea: `src/main/files/write.ts`
- Test: `src/main/files/write.test.ts`

**Interfacce:**
- Consuma: `resolveInsideRoot` da `./paths.ts`.
- Produce: `writeFileFromEditor(rootPath: string, relativePath: string, text: string): Promise<{ mtimeMs: number }>`.
  Lo consuma il Task 3.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('il testo finisce sul disco e torna il nuovo mtime', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-write-'))
  await writeFile(join(dir, 'a.txt'), 'vecchio\n')

  const esito = await writeFileFromEditor(dir, 'a.txt', 'nuovo\n')

  expect(await readFile(join(dir, 'a.txt'), 'utf8')).toBe('nuovo\n')
  expect(esito.mtimeMs).toBe((await stat(join(dir, 'a.txt'))).mtimeMs)
})

test('un percorso fuori dalla radice viene rifiutato', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-write-'))
  await expect(writeFileFromEditor(dir, '../fuori.txt', 'x')).rejects.toThrow()
})

test('non resta nessun file temporaneo dopo una scrittura riuscita', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-write-'))
  await writeFileFromEditor(dir, 'a.txt', 'contenuto\n')

  expect(await readdir(dir)).toEqual(['a.txt'])
})

test('un file che non esisteva viene creato', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-write-'))
  await writeFileFromEditor(dir, 'nuovo.txt', 'ciao\n')

  expect(await readFile(join(dir, 'nuovo.txt'), 'utf8')).toBe('ciao\n')
})

test('una cartella inesistente fallisce senza lasciare residui', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'tiller-write-'))
  await expect(writeFileFromEditor(dir, 'assente/a.txt', 'x')).rejects.toThrow()

  expect(await readdir(dir)).toEqual([])
})
```

Il terzo e il quinto test sono quelli che contano: misurano che il temporaneo non
sopravviva né al successo né al fallimento. L'atomicità vera — che cosa resta su
disco se il processo muore a metà `write` — **non è testabile in modo
deterministico**, e la spec lo dichiara. Questi test coprono ciò che è coperbile.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL, `writeFileFromEditor` non esiste.

- [ ] **Passo 3: implementare**

```ts
import { rename, unlink, writeFile, stat } from 'node:fs/promises'
import { dirname, join, basename } from 'node:path'
import { randomBytes } from 'node:crypto'
import { resolveInsideRoot } from './paths.ts'

/**
 * Scrive il file passando da un temporaneo e poi `rename`.
 *
 * Il temporaneo sta nella **stessa cartella** del bersaglio, non in `/tmp`:
 * `rename` e' atomico solo dentro lo stesso filesystem, e una cartella
 * temporanea di sistema puo' benissimo starne su un altro. Con `/tmp` la
 * `rename` diventa una copia, e la copia non e' atomica: e' esattamente la
 * garanzia che si voleva.
 */
export async function writeFileFromEditor(
  rootPath: string,
  relativePath: string,
  text: string
): Promise<{ mtimeMs: number }> {
  const target = resolveInsideRoot(rootPath, relativePath)
  const temporaneo = join(
    dirname(target),
    `.${basename(target)}.${randomBytes(6).toString('hex')}.tmp`
  )

  try {
    await writeFile(temporaneo, text, 'utf8')
    await rename(temporaneo, target)
  } catch (errore) {
    // Il temporaneo non deve sopravvivere all'errore. `unlink` puo' a sua volta
    // fallire (non e' mai stato creato): quel fallimento non deve nascondere
    // l'errore vero, che e' l'unico che l'utente puo' capire.
    await unlink(temporaneo).catch(() => {})
    throw errore
  }

  return { mtimeMs: (await stat(target)).mtimeMs }
}
```

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/files/` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/main/files/write.ts src/main/files/write.test.ts
git commit -m "feat: write a file atomically inside the worktree root"
```

---

## Task 3: `files.write` sul socket di controllo

**File:**
- Modifica: `src/shared/protocol.ts` (accanto a `files.read`, riga ~132)
- Modifica: `src/main/control/dispatch.ts` (accanto al caso `files.read`, riga ~454)
- Test: `src/main/control/dispatch-files.test.ts`

**Interfacce:**
- Consuma: `writeFileFromEditor` (Task 2).
- Produce: richiesta `files.write { worktreeId, path, text }` → `{ mtimeMs }`.
  La consuma il Task 5.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('files.write salva il file e torna il nuovo mtime', async () => {
  // ... stesso allestimento di 'files.read restituisce il contenuto'
  const risposta = await dispatch({
    id: '1',
    method: 'files.write',
    params: { worktreeId: worktree.id, path: 'a.txt', text: 'nuovo\n' }
  })

  expect(risposta.ok).toBe(true)
  expect(await readFile(join(rootPath, 'a.txt'), 'utf8')).toBe('nuovo\n')
})

test('files.write rifiuta un percorso fuori dalla radice', async () => {
  const risposta = await dispatch({
    id: '2',
    method: 'files.write',
    params: { worktreeId: worktree.id, path: '../fuori.txt', text: 'x' }
  })

  expect(risposta.ok).toBe(false)
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

```ts
// src/shared/protocol.ts, subito dopo il membro files.read
  z.object({
    id: z.string().min(1),
    method: z.literal('files.write'),
    params: z.object({
      worktreeId: z.string().min(1),
      path: z.string().min(1),
      text: z.string()
    })
  }),
```

`text: z.string()` senza `.min(1)`: svuotare un file è una modifica legittima, e
un minimo qui trasformerebbe «cancella tutto e salva» in un errore incomprensibile.

```ts
// src/main/control/dispatch.ts, subito dopo il caso files.read
        case 'files.write': {
          const rootPath = await percorsoWorktree(deps.db, request.params.worktreeId)
          const esito = await writeFileFromEditor(
            rootPath,
            request.params.path,
            request.params.text
          )
          return { id: request.id, ok: true, result: esito }
        }
```

Il confinamento non si riscrive qui: `writeFileFromEditor` chiama già
`resolveInsideRoot`, che è il confine di fiducia esistente e già coperto da test.
Un secondo controllo in questo punto sarebbe una seconda verità da tenere
allineata.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/main/control/` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/shared/protocol.ts src/main/control/dispatch.ts src/main/control/dispatch-files.test.ts
git commit -m "feat: add files.write to the control protocol"
```

---

## Task 4: Il riduttore puro del documento

**File:**
- Crea: `src/renderer/src/lib/workspace/document-state.ts`
- Test: `src/renderer/src/lib/workspace/document-state.test.ts`

**Interfacce:**
- Produce:

```ts
export type DocumentState = {
  readonly path: string
  readonly text: string
  readonly savedText: string
  readonly savedMtimeMs: number | null
  readonly conflict: 'none' | 'changedOnDisk' | 'deletedOnDisk'
}
export function isDirty(stato: DocumentState): boolean
export function apriDocumento(path: string, text: string, mtimeMs: number | null): DocumentState
export function modifica(stato: DocumentState, text: string): DocumentState
export function salvato(stato: DocumentState, text: string, mtimeMs: number): DocumentState
export function daDisco(stato: DocumentState, disco: DiscoLetto): DocumentState
export function risolviConRicarica(stato: DocumentState, text: string, mtimeMs: number | null): DocumentState
export function risolviTenendo(stato: DocumentState): DocumentState
export type DiscoLetto = { presente: false } | { presente: true; text: string; mtimeMs: number | null }
```

Lo consuma il Task 5. Tutte le funzioni restituiscono un **nuovo** stato: nessuna
muta l'argomento.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
const base = apriDocumento('a.ts', 'uno\n', 100)

test('un documento appena aperto non e sporco', () => {
  expect(isDirty(base)).toBe(false)
})

test('modificare rende sporco, e tornare al testo salvato lo pulisce', () => {
  const sporco = modifica(base, 'due\n')
  expect(isDirty(sporco)).toBe(true)
  expect(isDirty(modifica(sporco, 'uno\n'))).toBe(false)
})

test('salvare pulisce e aggiorna l mtime di riferimento', () => {
  const dopo = salvato(modifica(base, 'due\n'), 'due\n', 200)
  expect(isDirty(dopo)).toBe(false)
  expect(dopo.savedMtimeMs).toBe(200)
})

test('l eco del proprio salvataggio non e un conflitto', () => {
  const dopo = salvato(modifica(base, 'due\n'), 'due\n', 200)
  const eco = daDisco(dopo, { presente: true, text: 'due\n', mtimeMs: 200 })
  expect(eco.conflict).toBe('none')
  expect(eco.text).toBe('due\n')
})

test('un buffer pulito si ricarica da solo, senza conflitto', () => {
  const dopo = daDisco(base, { presente: true, text: 'esterno\n', mtimeMs: 300 })
  expect(dopo.conflict).toBe('none')
  expect(dopo.text).toBe('esterno\n')
  expect(isDirty(dopo)).toBe(false)
})

test('un buffer sporco tiene le sue modifiche e segnala il conflitto', () => {
  const sporco = modifica(base, 'mio\n')
  const dopo = daDisco(sporco, { presente: true, text: 'esterno\n', mtimeMs: 300 })
  expect(dopo.conflict).toBe('changedOnDisk')
  expect(dopo.text).toBe('mio\n')
})

test('un mtime sconosciuto vale come modifica esterna, non come eco', () => {
  const dopo = salvato(modifica(base, 'due\n'), 'due\n', 200)
  const sporco = modifica(dopo, 'ancora\n')
  const esito = daDisco(sporco, { presente: true, text: 'due\n', mtimeMs: null })
  expect(esito.conflict).toBe('changedOnDisk')
})

test('il file sparito si dichiara e non svuota il buffer', () => {
  const sporco = modifica(base, 'mio\n')
  const dopo = daDisco(sporco, { presente: false })
  expect(dopo.conflict).toBe('deletedOnDisk')
  expect(dopo.text).toBe('mio\n')
})

test('ricaricare scarta le modifiche locali', () => {
  const sporco = modifica(base, 'mio\n')
  const conflitto = daDisco(sporco, { presente: true, text: 'esterno\n', mtimeMs: 300 })
  const risolto = risolviConRicarica(conflitto, 'esterno\n', 300)
  expect(risolto.text).toBe('esterno\n')
  expect(isDirty(risolto)).toBe(false)
  expect(risolto.conflict).toBe('none')
})

test('tenere le modifiche toglie il banner e lascia il buffer sporco', () => {
  const sporco = modifica(base, 'mio\n')
  const conflitto = daDisco(sporco, { presente: true, text: 'esterno\n', mtimeMs: 300 })
  const risolto = risolviTenendo(conflitto)
  expect(risolto.conflict).toBe('none')
  expect(isDirty(risolto)).toBe(true)
  expect(risolto.text).toBe('mio\n')
})
```

Il test sull'`mtime` sconosciuto è quello che fissa la direzione dell'errore: nel
dubbio si mostra un conflitto, non si ricarica. Un falso conflitto costa un
click; una ricarica sbagliata costa il lavoro.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

`isDirty` è **calcolato**, `text !== savedText`, mai un campo. È il contratto di
`CodeDocument.swift`, e un flag separato è una seconda verità che diverge al primo
percorso di errore.

`daDisco` decide in quest'ordine:

1. disco assente → `deletedOnDisk`, buffer intatto;
2. `disco.mtimeMs !== null && disco.mtimeMs === stato.savedMtimeMs` → è il nostro
   eco, `none`, nessun cambiamento;
3. buffer pulito → ricarica silenziosa (`text` e `savedText` dal disco, `none`);
4. altrimenti → `changedOnDisk`, buffer intatto.

- [ ] **Passo 4: eseguire e vedere passare**

`npx vitest run src/renderer/src/lib/workspace/document-state.test.ts` → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/document-state.ts src/renderer/src/lib/workspace/document-state.test.ts
git commit -m "feat: decide the document state from the buffer and the disk"
```

---

## Task 5: L'archivio dei documenti

**File:**
- Crea: `src/renderer/src/lib/workspace/documents.svelte.ts`
- Modifica: `src/renderer/src/lib/workspace/FileTab.svelte` (**togliere** la
  ricarica incondizionata alla riga ~58)

**Interfacce:**
- Consuma: il riduttore del Task 4, `files.read` e `files.write` dal socket.
- Produce: `documentStore` con `apri(worktreeId, path)`, `modifica(path, text)`,
  `salva(path)`, `stato(path)`, `sporchi()`, `chiudi(path)`,
  `applicaEventoCartelle(worktreeId, directories)`. Lo consumano i Task 6, 7, 8, 9.

Il modulo è **sottile**: tiene una `SvelteMap<string, DocumentState>` in `$state`
e delega ogni decisione al riduttore. Se qui compare un `if` sul conflitto, sta
nel posto sbagliato.

- [ ] **Passo 1: togliere la ricarica che distrugge il buffer**

Oggi `FileTab.svelte` fa così:

```svelte
    const unsubscribe = window.tiller.onStateEvent((event) => {
      if (event.type !== 'files.changed' || event.worktreeId !== id) return
      reload()
    })
```

Ricarica per **qualunque** cartella toccata nel worktree, scartando il contenuto
in memoria. In sola lettura non si nota; con un buffer modificabile è la perdita
di dati che questa fase esiste per evitare. La sottoscrizione si sposta
nell'archivio e diventa condizionata: solo le cartelle che contengono un file
aperto, e la decisione la prende `daDisco`.

- [ ] **Passo 2: scrivere il test che fallisce**

```ts
test('un evento su una cartella senza file aperti non tocca nessun documento', () => {
  // apri 'src/a.ts', poi applicaEventoCartelle(id, ['docs'])
  // -> nessuna rilettura, nessun cambiamento di stato
})

test('un evento sulla cartella di un file aperto lo rilegge', () => {
  // apri 'src/a.ts', poi applicaEventoCartelle(id, ['src'])
  // -> una rilettura di 'src/a.ts'
})
```

`window.tiller` si simula con un finto che conta le chiamate a `files.read`.

- [ ] **Passo 3: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 4: implementare**

La cartella di un percorso è tutto ciò che precede l'ultimo `/`, o `''` per un
file in radice — la stessa convenzione di `containingDirectory` in
`src/main/files/watch.ts`. **Non reimplementarla con una regex diversa:** se le
due convenzioni divergono, i file in radice smettono di aggiornarsi e nessun test
di unità se ne accorge.

- [ ] **Passo 5: eseguire e vedere passare** → PASS.

- [ ] **Passo 6: committare**

```bash
git add src/renderer/src/lib/workspace/documents.svelte.ts src/renderer/src/lib/workspace/FileTab.svelte
git commit -m "feat: keep file buffers outside the tab components"
```

---

## Task 6: Il tab diventa un editor

**File:**
- Modifica: `src/renderer/src/lib/workspace/FileTab.svelte`

**Interfacce:**
- Consuma: `documentStore` (Task 5).

- [ ] **Passo 1: scrivere il criterio e2e 1** (scrivere fa comparire il pallino)
  e **lasciarlo rosso**. Se è già verde, il criterio è scritto male.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Tre punti da non sbagliare:

- `EditorState.readOnly.of(true)` e `EditorView.editable.of(false)` **vanno via**:
  sono ciò che oggi impedisce la scrittura.
- Il testo iniziale viene dall'archivio, non da `files.read` diretto: tornando su
  un tab il buffer sporco dev'essere quello di prima, non il disco. È il criterio 7.
- Un `EditorView.updateListener` che a ogni `docChanged` chiama
  `documentStore.modifica(path, testo)`. **Solo su `docChanged`**: senza quel
  controllo anche lo spostamento del cursore riscriverebbe il buffer, e con un
  `$effect` che rilegge l'archivio si ottiene un giro infinito.

Quando l'archivio riporta un testo diverso da quello nella vista — ricarica
silenziosa, oppure *Reload* — la vista si aggiorna con una transazione che
sostituisce il documento, **non** ricreando l'editor: ricrearlo perde selezione e
posizione di scorrimento a ogni salvataggio esterno.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/FileTab.svelte e2e/fase-6b2-scrittura.spec.ts
git commit -m "feat: edit a file in its tab"
```

---

## Task 7: Salvare, e dirlo

**File:**
- Modifica: `src/renderer/src/lib/workspace/TabSegment.svelte` (il pallino)
- Modifica: `src/renderer/src/App.svelte` (`⌘S`)

- [ ] **Passo 1: scrivere i criteri e2e 2 e 8** (`⌘S` cambia il file sul disco; un
  salvataggio fallito lascia il buffer intatto e mostra l'errore) e lasciarli rossi.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Il pallino sta in `TabSegment.svelte`, accanto a `{tab.title}` (riga ~84). Il tab
sa il proprio contenuto: `tab.content.kind === 'file'` dà il percorso, e il
percorso è la chiave dell'archivio. Serve un attributo stabile per il criterio —
`data-dirty="true"` sul pulsante `.tab` — perché un pallino reso con uno
pseudo-elemento CSS non è osservabile da Playwright.

`⌘S` si aggiunge nel `keydown` globale di `App.svelte`. **Attenzione:**
`Terminal.svelte:112` ha già un ascoltatore globale per `⌘F`. Convivono, ma il
salvataggio deve applicarsi al **tab attivo del gruppo attivo**, e non fare nulla
se quel tab non è un file: premere `⌘S` con un terminale davanti non deve salvare
il file di un altro pane.

Sul fallimento: `salva` **non tocca** `savedText`. Il buffer resta sporco e
l'errore compare. È la regola comune della spec — il buffer è l'unica copia della
modifica.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/TabSegment.svelte src/renderer/src/App.svelte e2e/fase-6b2-scrittura.spec.ts
git commit -m "feat: save the active file and show unsaved changes"
```

---

## Task 8: Non perdere lo sporco

**File:**
- Modifica: `src/renderer/src/App.svelte` (`⌘W` e la risposta alla chiusura)
- Modifica: `src/main/index.ts` (`window.on('close')`)
- Modifica: `src/shared/protocol.ts` (evento verso il renderer)

`⌘W` **non esiste oggi**: `closeTab` è nel riduttore ma nessuna interfaccia lo
invoca. Questo task introduce la scorciatoia e la sua guardia insieme — una
scorciatoia che chiude senza chiedere sarebbe un modo nuovo di perdere dati,
aggiunto dalla fase che doveva impedirlo.

- [ ] **Passo 1: scrivere il test del percorso di chiusura**

Il main oggi ha solo `window.on('closed')` (riga ~103), che scatta a finestra già
chiusa: troppo tardi per chiedere qualcosa. Serve `window.on('close')` con
`event.preventDefault()` quando il renderer dichiara di avere buffer sporchi.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

`⌘W` sul tab attivo: se il documento è sporco, una finestra di dialogo
*Save / Don't save / Cancel* (`dialog.showMessageBox` di Electron, dal main);
altrimenti `closeTab` diretto. Chiudendo il tab si toglie il documento
dall'archivio, altrimenti i buffer si accumulano per tutta la sessione.

Alla chiusura della finestra: il main chiede al renderer se ci sono documenti
sporchi, e se sì mostra la stessa dialog. In Electron chiudere la finestra non
chiude l'app ma **distrugge il renderer**, cioè proprio dove vive il buffer.
È l'asimmetria opposta a quella dei PTY documentata in `CLAUDE.md`: là chiudere
la finestra non termina gli agenti, qui cancellerebbe le modifiche.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/App.svelte src/main/index.ts src/shared/protocol.ts
git commit -m "feat: guard unsaved changes on close"
```

---

## Task 9: Il conflitto, e come si risolve

**File:**
- Modifica: `src/renderer/src/lib/workspace/FileTab.svelte` (il banner)

- [ ] **Passo 1: scrivere i criteri e2e 3, 4, 5 e 6** e lasciarli rossi.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Un banner sopra l'editor, visibile solo con `conflict !== 'none'`:

- `changedOnDisk` → *File changed on disk* con *Reload* e *Keep*;
- `deletedOnDisk` → *File deleted on disk*; `⌘S` lo ricrea, perché
  `writeFileFromEditor` non distingue creazione da sovrascrittura.

Il banner **non** blocca l'editor: la modifica in corso resta modificabile. Un
banner modale trasformerebbe un avviso in un ostacolo.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/FileTab.svelte e2e/fase-6b2-scrittura.spec.ts
git commit -m "feat: resolve a conflict with the file on disk"
```

---

## Task 10: I criteri end-to-end e le mutazioni

**File:**
- Completa: `e2e/fase-6b2-scrittura.spec.ts`

Tutti i criteri partono da un **gesto** — scrivere nell'editor, premere `⌘S`,
scrivere sul disco da fuori — mai da `window.tiller.request` e mai da
`workspace.apply`. Un criterio che parte dal comando verifica il comando, non la
funzione.

- [ ] **Passo 1: completare gli otto criteri**

1. Scrivere nell'editor fa comparire il pallino sul tab.
2. `⌘S` cambia il file **sul disco**, verificato leggendo il file con `readFile`,
   non l'interfaccia.
3. Un file modificato da fuori con buffer **pulito** si aggiorna da solo.
4. Un file modificato da fuori con buffer **sporco** mostra il banner.
5. Dopo un salvataggio nostro il banner **non** compare.
6. *Reload* scarta le modifiche locali; *Keep* le mantiene e toglie il banner.
7. Cambiare tab e tornare indietro **conserva** il buffer sporco.
8. Un salvataggio fallito lascia il buffer intatto e mostra l'errore.

Per il criterio 8 il fallimento si provoca **dal filesystem**, non con un finto:
si toglie il permesso di scrittura alla cartella con `chmod 0o500` prima di
premere `⌘S`. Un finto verificherebbe il finto.

- [ ] **Passo 2: eseguire tutti i criteri**

```bash
npx electron-vite build && npx playwright test e2e/fase-6b2-scrittura.spec.ts
```
Atteso: 8 passed.

- [ ] **Passo 3: mutazioni di verifica**

Una per volta, ricostruendo ogni volta e annullando con `git checkout --`:

| Mutazione | Rosso atteso |
| --- | --- |
| in `daDisco`, il confronto degli `mtime` diventa sempre `false` | **solo** 5 |
| l'archivio si sposta dentro `FileTab.svelte` | **solo** 7 |
| `salva` azzera il buffer quando la scrittura fallisce | **solo** 8 |
| `writeFileFromEditor` scrive diretta invece che via `rename` | **nessuno** — è il limite noto |
| `daDisco` ricarica anche col buffer sporco | **solo** 4 e 6 |

"**solo**" è la parte che conta. Un criterio che diventa rosso insieme ad altri
misura qualcosa di più largo di ciò che dichiara; una mutazione che non fa
diventare rosso il suo criterio significa che il criterio non misura niente. In
entrambi i casi si riscrive il criterio **prima** di proseguire.

La riga senza criterio è deliberata e va riportata, non nascosta: provare
l'atomicità richiederebbe di uccidere il processo dentro una `write`, che non è
riproducibile in modo deterministico. `rename` si giustifica per costruzione.

- [ ] **Passo 4: committare**

```bash
git add e2e/fase-6b2-scrittura.spec.ts
git commit -m "test: end-to-end criteria for writing files"
```

---

## Chiusura

Al termine dell'ultimo task, riportare:

- l'elenco dei commit,
- l'esito di `npx vitest run` sui percorsi toccati,
- l'esito di `npm run typecheck` e `npm run lint` (0 errori, non solo 0 errori di tipo),
- l'esito di **ogni** mutazione, con quali criteri sono diventati rossi,
- ogni punto del piano trovato sbagliato, dicendolo invece di adeguarsi,
- ciò che **non** si è riusciti a verificare, dicendolo invece di ometterlo.

Fuori scope, già deciso nella spec: creazione, rinomina ed eliminazione di file
(la versione Swift non li ha); anteprima e toolbar Markdown, che vanno in una
Fase 6b-3; la modifica dentro la vista diff; l'autosave.
