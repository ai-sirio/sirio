# V1 — L'albero dei progetti nella sidebar: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** far vedere nella sidebar i progetti e i worktree che esistono
davvero, come richiede il vincolo V1 di `docs/fase-4-requisiti-ui.md`.

**Architettura:** non si costruisce niente di nuovo. Il motore c'è, il canale c'è,
i componenti ci sono. Manca la chiamata.

## Il difetto, per intero

`docs/fase-4-requisiti-ui.md`, vincolo **V1**, marcato «non negoziabile»:

```
▼ progetto
   ⑂ main  (worktree aperto, selezionato)      ⋯
       ▭ Terminale 1                            (tab)
       ✳ Chat                                   (tab)
     ⑂ fix/chat-card-ui  (worktree chiuso)
     + New Worktree…
```

Con la nota: «un progetto con un solo worktree resta comunque espandibile…
Proposta di collassarlo in una riga sola: **respinta esplicitamente**».

Quello che il codice fa oggi, in `src/renderer/src/App.svelte:277`:

```ts
const projectRows = $derived.by((): ProjectRow[] => {
  if (worktreeAttivo === null) return []
  const pane = model.panes.find((candidate) => worktreeKey(candidate) === worktreeAttivo)
  const branch = pane === undefined ? labelForPath(worktreeAttivo) : labelForPath(pane.cwd)
  return [{ id: 'project:current', name: 'Workspace', worktrees: [{ id: worktreeAttivo, branch }] }]
})
```

Un progetto finto chiamato «Workspace», contenente il solo worktree attivo,
derivato dai pane vivi. I progetti aggiunti non compaiono; i worktree chiusi non
esistono; `+ New Worktree…` non c'è.

**Prova:** con `tillerctl project-add` seguito da `worktree-create` e un ricarico,
la finestra continua a dire «No projects». Sessanta criteri end-to-end restano
verdi, perché nessuno di essi attraversa il ponte fra «ho aggiunto un progetto» e
«lo vedo».

## Quello che invece funziona già, e non va toccato

- `window.tiller.request(...)` è un canale **generico**: inoltra qualunque
  `ControlRequest`. Il renderer può già chiedere tutto.
- Il dispatch espone `project.list`, `project.add`, `project.remove`,
  `worktree.list`, `worktree.create`, `worktree.remove`.
- `buildTree` (`tree-model.ts`) e `SidebarTree.svelte` sono **generici e
  corretti**: N progetti, N worktree, N tab, con espansione e riepilogo di stato.
  Erano stati scritti sulla V1.

Il difetto è per intero nel dato che li alimenta.

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo.
- `npm run lint`: **0 errori**; i warning sono 26 e **non devono salire**.
- **Ogni colore e ogni misura passano dai token** di `assets/tokens.css`. Un
  esadecimale letterale o un `px` fuori scala fa fallire
  `src/renderer/src/lib/design/scale-guard.test.ts`.
- `__dirname` funziona nei test vitest di questo progetto.
- **Non eseguire `bash scripts/ci.sh`.**

---

## Task 1: Lo store dei progetti

**File:**
- Crea: `src/renderer/src/lib/workspace/projects.svelte.ts`
- Crea: `src/renderer/src/lib/workspace/projects.test.ts`

**Interfacce:**
- Produce: `class ProjectsStore` con
  `constructor(richiedi: Richiedi)`, `readonly rows: ProjectRow[]`,
  `aggiorna(): Promise<void>`, `creaWorktree(projectId: string, branch: string): Promise<void>`.
- `type Richiedi = (request: unknown) => Promise<{ ok: boolean; result?: unknown }>` —
  iniettata, così il test non ha bisogno né di Electron né del socket.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
import { expect, test } from 'vitest'
import { ProjectsStore } from './projects.svelte.ts'

function richiediFinto(progetti: unknown, worktree: unknown) {
  return async (request: unknown): Promise<{ ok: boolean; result?: unknown }> => {
    const metodo = (request as { method: string }).method
    if (metodo === 'project.list') return { ok: true, result: { projects: progetti } }
    if (metodo === 'worktree.list') return { ok: true, result: { worktrees: worktree } }
    return { ok: false }
  }
}

test('unisce i progetti ai loro worktree', async () => {
  const store = new ProjectsStore(
    richiediFinto(
      [{ id: 'p1', name: 'tiller', path: '/a' }],
      [
        { id: 'w1', projectId: 'p1', branch: 'main', path: '/a' },
        { id: 'w2', projectId: 'p1', branch: 'fix/x', path: '/a-fix' }
      ]
    )
  )
  await store.aggiorna()
  expect(store.rows).toEqual([
    {
      id: 'p1',
      name: 'tiller',
      worktrees: [
        { id: 'w1', branch: 'main' },
        { id: 'w2', branch: 'fix/x' }
      ]
    }
  ])
})

test('un progetto senza worktree resta visibile', async () => {
  const store = new ProjectsStore(richiediFinto([{ id: 'p1', name: 'vuoto', path: '/v' }], []))
  await store.aggiorna()
  expect(store.rows).toEqual([{ id: 'p1', name: 'vuoto', worktrees: [] }])
})

test('un worktree chiuso compare come gli altri', async () => {
  // Il vincolo V1 elenca `fix/chat-card-ui (worktree chiuso)` accanto a quello
  // aperto: la sidebar dice cosa ESISTE, non cosa e' montato adesso. Filtrare
  // sui pane vivi e' esattamente il difetto che questo piano ripara.
  const store = new ProjectsStore(
    richiediFinto(
      [{ id: 'p1', name: 'tiller', path: '/a' }],
      [{ id: 'w9', projectId: 'p1', branch: 'chiuso', path: '/a-chiuso' }]
    )
  )
  await store.aggiorna()
  expect(store.rows[0].worktrees).toHaveLength(1)
})

test('una risposta non ok lascia le righe precedenti', async () => {
  // Un errore di rete non deve svuotare la sidebar sotto le mani dell'utente.
  let ok = true
  const store = new ProjectsStore(async (request) => {
    if (!ok) return { ok: false }
    const metodo = (request as { method: string }).method
    if (metodo === 'project.list') return { ok: true, result: { projects: [{ id: 'p1', name: 'x', path: '/x' }] } }
    return { ok: true, result: { worktrees: [] } }
  })
  await store.aggiorna()
  ok = false
  await store.aggiorna()
  expect(store.rows).toHaveLength(1)
})
```

- [ ] **Passo 2: verificare il rosso**

Run: `npx vitest run src/renderer/src/lib/workspace/projects.test.ts`
Expected: FAIL — modulo assente.

- [ ] **Passo 3: implementare**

Lo store chiama `project.list` e `worktree.list`, raggruppa i worktree per
`projectId`, e conserva le righe precedenti se una risposta non è `ok`. `rows` è
`$state`. Nessun filtro sui pane vivi.

- [ ] **Passo 4: verificare il verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: read the real projects and worktrees"
```

---

## Task 2: Il ponte — collegarlo alla sidebar

**File:**
- Modifica: `src/renderer/src/App.svelte` (sostituisce `projectRows`)
- Crea: `e2e/v1-sidebar-progetti.spec.ts`

**Interfacce:**
- Consuma: `ProjectsStore` dal Task 1.

- [ ] **Passo 1: scrivere il criterio 1, e lasciarlo rosso**

```
1. Un progetto aggiunto con `tillerctl project-add` compare nella sidebar
   col proprio nome, e il suo worktree compare sotto di esso.
```

**È il criterio che mancava.** I sessanta esistenti costruiscono lo stato e poi
verificano ciò che hanno costruito; nessuno attraversa il ponte fra «l'ho
aggiunto» e «lo vedo». Deve asserire il **nome vero** del progetto — non che
esista una riga, che passerebbe anche col «Workspace» finto.

- [ ] **Passo 2: verificare il rosso.** Atteso: la sidebar dice «No projects».

- [ ] **Passo 3: implementare**

`projectRows` diventa `store.rows`. Lo store si aggiorna al montaggio e a ogni
evento di stato che possa averlo cambiato. `tabRows` resta una mappa per
worktree: i tab del worktree attivo si popolano, gli altri restano vuoti — e
`buildTree` già prevede il caso, come dice il suo stesso commento.

**Il worktree attivo va evidenziato, non filtrato:** `activeWorktreeId` esiste
già ed è ciò che `SidebarTree` usa per espandere gli antenati.

- [ ] **Passo 4: verificare il verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: show the real project tree in the sidebar"
```

---

## Task 3: `+ New Worktree…`

**File:**
- Modifica: `src/renderer/src/lib/workspace/tree-model.ts` (nodo `action`)
- Modifica: `src/renderer/src/lib/workspace/TreeRow.svelte`
- Modifica: `e2e/v1-sidebar-progetti.spec.ts` (criterio 2)

- [ ] **Passo 1: scrivere il criterio 2, e lasciarlo rosso**

```
2. `+ New Worktree…` sotto un progetto crea un worktree, che compare
   nella sidebar senza ricaricare la finestra.
```

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

Un `TreeNode` di genere `action` come ultimo figlio di ogni progetto. È un genere
nuovo, non un worktree con un nome speciale: un finto worktree finirebbe nei
conteggi, nel riepilogo di stato e nella selezione, e ognuno di quei posti
andrebbe poi insegnato a ignorarlo.

Il branch lo chiede l'utente. Se `worktree.create` fallisce, l'errore si mostra:
il vincolo del progetto è che gli errori non si inghiottono mai in silenzio.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: create a worktree from the sidebar"
```

---

## Chiusura

Riportare: i commit; `npx vitest run src/renderer/`; `npm run typecheck`;
`npm run lint` (0 errori, warning non oltre 26); la prova che ogni test era rosso
prima; ogni punto del piano trovato sbagliato, detto invece che aggirato.

**Fuori scopo:** la sidebar a tre livelli con le sessioni degli agenti e la
rimozione di `AgentsPanel` (piano a parte); rimuovere progetti dall'interfaccia;
qualunque ridisegno visivo.
