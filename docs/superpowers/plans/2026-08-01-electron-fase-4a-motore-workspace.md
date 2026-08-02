# Fase 4a — Motore del workspace: piano di implementazione

> **Per esecutori agentici:** SOTTO-SKILL RICHIESTA: usare
> superpowers:subagent-driven-development (consigliato) o
> superpowers:executing-plans per implementare un task alla volta.

**Obiettivo:** portare modello, riduttore, geometria e persistenza del layout
del workspace, verificabili interamente da `tillerctl` senza interfaccia.

**Architettura:** otto moduli di logica pura in `src/shared/workspace/` più uno
store in `src/main/workspace/`. Il riduttore è una funzione:
`apply(comando, layout) → transizione | errore`. Il renderer anticipa
localmente con le funzioni pure, il main decide.

**Stack:** TypeScript, Zod, kysely, vitest, Playwright.

**Repo:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
`~/Desktop/Progetti/tiller` è **sola lettura**.

**Spec:** `docs/2026-08-01-electron-fase-4a-motore-workspace-design.md`.

## Vincoli globali

- **`src/shared/` non importa mai da `src/main/`.** È il livello comune a main,
  renderer e CLI. L'inversione è già costata un difetto (commit `c78052f`).
- **Gli import relativi dentro `src/shared/` portano l'estensione `.ts`.**
  `tillerctl` carica questi file sotto Node senza bundler, e Node risolve alla
  lettera. vitest, `tsc` ed eslint risolvono alla maniera dei bundler e **non
  se ne accorgono**: un import senza estensione passa tutti e tre e rompe la
  CLI a runtime.
- Nessun file sotto `src/shared/workspace/` importa `electron`, `node:net` o
  `node:fs`.
- **Mai indebolire un tipo di produzione per far compilare un finto di test.**
  Se un finto non soddisfa più un'interfaccia, si aggiorna il finto.
- `ControlRequest` e `StateEvent` sono unioni discriminate su cui dispatcher e
  renderer fanno `switch` esaustivo: le aggiunte al protocollo e i rami che le
  gestiscono stanno **nello stesso task**.
- Commenti e messaggi in italiano.
- Commit in stile Conventional Commits, oggetto minuscolo all'imperativo.
- Prima di ogni commit: `pnpm exec prettier --write` sui file toccati.
- **Prova di ogni task: `bash scripts/ci.sh` deve stampare `CI OK`.** Non i
  sotto-comandi: solo il gate completo esegue gli e2e, che invocano
  `node cli/tillerctl.ts` davvero e sono l'unico controllo che accorge se la
  CLI si rompe a runtime.
- Baseline all'inizio della fase: **303 test unitari, 13 e2e** (9 delle Fasi
  0–2 più i 4 della Fase 3), gate verde.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/shared/workspace/layout-ids.ts` | `PaneGroupID`, `SplitID`, `WorkspaceTabID` brandizzati |
| `src/shared/workspace/layout-types.ts` | `LayoutNode`, `PaneGroup`, `WorkspaceTab`, `viewState` |
| `src/shared/workspace/layout-invariants.ts` | `validate`, `makeLayout`, `emptyLayout` |
| `src/shared/workspace/layout-commands.ts` | i nove comandi, `isStructuralCommand` |
| `src/shared/workspace/layout-engine.ts` | `apply(comando, layout)` |
| `src/shared/workspace/layout-metrics.ts` | le costanti misurate |
| `src/shared/workspace/layout-geometry.ts` | `groupRects(layout, contenitore)`, `dividerRects(layout, contenitore)` |
| `src/shared/workspace/drop-target.ts` | `resolveDropTarget(punto, rects)` |
| `src/shared/workspace/spatial-neighbors.ts` | `neighbor(da, direzione, rects)` |
| `src/main/workspace/layout-store.ts` | carica, salva, quarantena |
| `src/main/workspace/persistence-policy.ts` | strutturale subito, resto ritardato |
| `e2e/fase-4a-workspace.spec.ts` | i quattro criteri |

Modificati: `src/shared/protocol.ts`, `src/main/control/dispatch.ts`,
`src/main/index.ts`, `cli/args.ts`.

---

## Task 1: identificatori e tipi del layout

**File:**
- Crea: `src/shared/workspace/layout-ids.ts`
- Crea: `src/shared/workspace/layout-ids.test.ts`
- Crea: `src/shared/workspace/layout-types.ts`
- Crea: `src/shared/workspace/layout-types.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `PaneGroupID`, `SplitID`, `WorkspaceTabID` (stringhe brandizzate) con
  `newPaneGroupID()`, `newSplitID()`, `newWorkspaceTabID()` e i corrispondenti
  schemi Zod `PaneGroupIDSchema` ecc.; `LayoutNode`, `PaneGroup`,
  `WorkspaceTab`, `WorkspaceTabViewState`, `WorkspaceContentRef`,
  `WorkspaceSplitAxis` come tipi e schemi Zod.

- [ ] **Passo 1: scrivere il test che fallisce**

`layout-ids.test.ts`:

```ts
import { expect, test } from 'vitest'
import { newPaneGroupID, newSplitID, PaneGroupIDSchema } from './layout-ids.ts'

test('gli identificatori generati sono uuid distinti', () => {
  const a = newPaneGroupID()
  const b = newPaneGroupID()
  expect(a).not.toBe(b)
  expect(PaneGroupIDSchema.safeParse(a).success).toBe(true)
})

test('lo schema rifiuta una stringa vuota', () => {
  expect(PaneGroupIDSchema.safeParse('').success).toBe(false)
})

test('i tre tipi non si mescolano', () => {
  // Prova di tipo, non di runtime: se questa riga compilasse, il branding
  // non starebbe facendo il suo lavoro.
  const split = newSplitID()
  // @ts-expect-error un SplitID non e un PaneGroupID
  const gruppo: ReturnType<typeof newPaneGroupID> = split
  expect(gruppo).toBe(split)
})
```

`layout-types.test.ts`:

```ts
import { expect, test } from 'vitest'
import { LayoutNodeSchema, WorkspaceTabSchema, emptyViewState } from './layout-types.ts'
import { newPaneGroupID, newSplitID, newWorkspaceTabID } from './layout-ids.ts'

test('un nodo foglia e un nodo split si validano', () => {
  const gruppo = newPaneGroupID()
  expect(LayoutNodeSchema.safeParse({ kind: 'group', id: gruppo }).success).toBe(true)
  expect(
    LayoutNodeSchema.safeParse({
      kind: 'split',
      id: newSplitID(),
      axis: 'horizontal',
      fraction: 0.5,
      first: { kind: 'group', id: gruppo },
      second: { kind: 'group', id: newPaneGroupID() }
    }).success
  ).toBe(true)
})

test('una frazione fuori da (0,1) e rifiutata dallo schema', () => {
  const nodo = {
    kind: 'split',
    id: newSplitID(),
    axis: 'horizontal',
    fraction: 1.5,
    first: { kind: 'group', id: newPaneGroupID() },
    second: { kind: 'group', id: newPaneGroupID() }
  }
  expect(LayoutNodeSchema.safeParse(nodo).success).toBe(false)
})

test('lo stato di vista vuoto si valida e porta i campi delle fasi future', () => {
  const tab = {
    id: newWorkspaceTabID(),
    title: 'Terminale 1',
    titleIsAutoNamed: true,
    content: { kind: 'terminal', id: 'term-1' },
    viewState: emptyViewState()
  }
  const esito = WorkspaceTabSchema.safeParse(tab)
  expect(esito.success).toBe(true)
  // I campi di Fase 5 e 6 esistono gia, cosi non servira migrare lo schema.
  expect(emptyViewState()).toHaveProperty('chatComposerDraft')
  expect(emptyViewState()).toHaveProperty('documentCaretOffset')
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- src/shared/workspace`
Atteso: FAIL, moduli inesistenti.

- [ ] **Passo 3: implementare `layout-ids.ts`**

```ts
import { z } from 'zod'

/**
 * Tre identificatori distinti viaggiano nelle stesse firme (`moveTab(tabID,
 * to: group(groupID))`). Il branding li rende non intercambiabili: scambiarli
 * e un errore che deve prendere il compilatore, non un test.
 */
declare const marchio: unique symbol

export type PaneGroupID = string & { readonly [marchio]: 'PaneGroupID' }
export type SplitID = string & { readonly [marchio]: 'SplitID' }
export type WorkspaceTabID = string & { readonly [marchio]: 'WorkspaceTabID' }

export const PaneGroupIDSchema = z.string().min(1).transform((v) => v as PaneGroupID)
export const SplitIDSchema = z.string().min(1).transform((v) => v as SplitID)
export const WorkspaceTabIDSchema = z.string().min(1).transform((v) => v as WorkspaceTabID)

export function newPaneGroupID(): PaneGroupID {
  return crypto.randomUUID() as PaneGroupID
}

export function newSplitID(): SplitID {
  return crypto.randomUUID() as SplitID
}

export function newWorkspaceTabID(): WorkspaceTabID {
  return crypto.randomUUID() as WorkspaceTabID
}
```

- [ ] **Passo 4: implementare `layout-types.ts`**

```ts
import { z } from 'zod'
import {
  PaneGroupIDSchema,
  SplitIDSchema,
  WorkspaceTabIDSchema,
  type PaneGroupID,
  type SplitID,
  type WorkspaceTabID
} from './layout-ids.ts'

export const WorkspaceSplitAxisSchema = z.enum(['horizontal', 'vertical'])
export type WorkspaceSplitAxis = z.infer<typeof WorkspaceSplitAxisSchema>

/**
 * Cosa mostra un tab. In Fase 4a esiste il solo caso `terminal`; le Fasi 5 e 6
 * aggiungeranno `chat` e `document`. Unione discriminata proprio perche
 * aggiungerne uno faccia fallire i punti che vanno aggiornati.
 */
export const WorkspaceContentRefSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('terminal'), id: z.string().min(1) })
])
export type WorkspaceContentRef = z.infer<typeof WorkspaceContentRefSchema>

/**
 * Stato di vista per tab. Porta gia i campi delle Fasi 5 e 6 (decisione
 * dell'utente): restano nulli finche non servono, ma non richiederanno una
 * migrazione dello schema quando serviranno.
 */
export const WorkspaceTabViewStateSchema = z.object({
  documentCaretOffset: z.number().int().nullable(),
  documentSelectionLength: z.number().int().nullable(),
  documentScrollAnchor: z.number().nullable(),
  documentFoldedRanges: z.array(z.tuple([z.number().int(), z.number().int()])),
  editorMode: z.string().nullable(),
  chatComposerDraft: z.string().nullable(),
  chatAttachmentReferences: z.array(z.string()),
  chatTranscriptAnchor: z.number().nullable(),
  followsTail: z.boolean(),
  terminalViewportAnchor: z.number().nullable()
})
export type WorkspaceTabViewState = z.infer<typeof WorkspaceTabViewStateSchema>

export function emptyViewState(): WorkspaceTabViewState {
  return {
    documentCaretOffset: null,
    documentSelectionLength: null,
    documentScrollAnchor: null,
    documentFoldedRanges: [],
    editorMode: null,
    chatComposerDraft: null,
    chatAttachmentReferences: [],
    chatTranscriptAnchor: null,
    followsTail: false,
    terminalViewportAnchor: null
  }
}

export const WorkspaceTabSchema = z.object({
  id: WorkspaceTabIDSchema,
  title: z.string(),
  titleIsAutoNamed: z.boolean(),
  content: WorkspaceContentRefSchema,
  viewState: WorkspaceTabViewStateSchema
})
export type WorkspaceTab = z.infer<typeof WorkspaceTabSchema>

/**
 * La foglia dell'albero NON e un riquadro singolo: e un gruppo che contiene
 * piu tab. Da qui discendono la barra dei tab per riquadro e il terzo livello
 * dell'albero in sidebar, che appiattisce i tab di tutti i gruppi.
 */
export interface PaneGroup {
  id: PaneGroupID
  tabs: WorkspaceTab[]
  activeTabId: WorkspaceTabID | null
}

export const PaneGroupSchema: z.ZodType<PaneGroup> = z.object({
  id: PaneGroupIDSchema,
  tabs: z.array(WorkspaceTabSchema),
  activeTabId: WorkspaceTabIDSchema.nullable()
})

export type LayoutNode =
  | { kind: 'group'; id: PaneGroupID }
  | {
      kind: 'split'
      id: SplitID
      axis: WorkspaceSplitAxis
      fraction: number
      first: LayoutNode
      second: LayoutNode
    }

export const LayoutNodeSchema: z.ZodType<LayoutNode> = z.lazy(() =>
  z.discriminatedUnion('kind', [
    z.object({ kind: z.literal('group'), id: PaneGroupIDSchema }),
    z.object({
      kind: z.literal('split'),
      id: SplitIDSchema,
      axis: WorkspaceSplitAxisSchema,
      // Estremi esclusi: una frazione 0 o 1 e un riquadro invisibile.
      fraction: z.number().gt(0).lt(1),
      first: LayoutNodeSchema,
      second: LayoutNodeSchema
    })
  ])
)
```

- [ ] **Passo 5: eseguire e verificare che passi**

Comando: `pnpm test:unit -- src/shared/workspace`
Atteso: PASS.

- [ ] **Passo 6: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: identificatori brandizzati e tipi del layout del workspace"
```

---

## Task 2: invarianti e costruzione

**File:**
- Crea: `src/shared/workspace/layout-invariants.ts`
- Crea: `src/shared/workspace/layout-invariants.test.ts`

**Interfacce:**
- Consuma: i tipi del Task 1.
- Produce: `type WorkspaceLayout` (brandizzato, senza costruttore esportato);
  `type LayoutError` (unione discriminata su `kind` con gli undici casi);
  `validate(root, groups, activeGroupId): LayoutError | null`;
  `makeLayout(root, groups, activeGroupId): { ok: true; layout } | { ok: false; error }`;
  `emptyLayout(groupId?): WorkspaceLayout`;
  letture `groupOf`, `tabOf`, `orderedGroupIds`, `allTabs`, `splitIds`,
  `fractionOf`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { makeLayout, emptyLayout, allTabs, orderedGroupIds } from './layout-invariants.ts'
import { newPaneGroupID, newSplitID, newWorkspaceTabID } from './layout-ids.ts'
// I tipi marchiati degli id stanno in `layout-ids.ts`, insieme alle factory che
// li producono; `layout-types.ts` esporta le forme del layout, non gli id.
import type { PaneGroupID } from './layout-ids.ts'
import { emptyViewState } from './layout-types.ts'
import type { PaneGroup } from './layout-types.ts'

const gruppo = (id = newPaneGroupID()): [typeof id, PaneGroup] => [
  id,
  { id, tabs: [], activeTabId: null }
]

test('il layout vuoto e valido e ha un gruppo solo', () => {
  const l = emptyLayout()
  expect(orderedGroupIds(l)).toHaveLength(1)
  expect(allTabs(l)).toEqual([])
})

test('un gruppo referenziato dall albero ma assente dal registro e orfano', () => {
  const [id] = gruppo()
  const esito = makeLayout({ kind: 'group', id }, new Map(), id)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('unresolvedGroupLeaf')
})

test('un gruppo nel registro ma assente dall albero e orfano', () => {
  const [idA, gA] = gruppo()
  const [idB, gB] = gruppo()
  const esito = makeLayout({ kind: 'group', id: idA }, new Map([[idA, gA], [idB, gB]]), idA)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('orphanGroup')
})

test('un id di gruppo ripetuto nell albero riporta L ID, non una frase', () => {
  // Il campo si chiama `id`: se ci finisce dentro una descrizione, chi mostra
  // l errore stampa prosa al posto di un identificatore.
  const [idA, gA] = gruppo()
  const esito = makeLayout(
    { kind: 'split', id: newSplitID(), axis: 'vertical', fraction: 0.5,
      first: { kind: 'group', id: idA }, second: { kind: 'group', id: idA } },
    new Map([[idA, gA]]),
    idA
  )
  expect(esito.ok).toBe(false)
  if (!esito.ok) {
    expect(esito.error.kind).toBe('duplicateID')
    if (esito.error.kind === 'duplicateID') expect(esito.error.id).toBe(idA)
  }
})

test('un tab attivo che non sta nel gruppo e rifiutato', () => {
  const [idA, gA] = gruppo()
  const estraneo = newWorkspaceTabID()
  const esito = makeLayout(
    { kind: 'group', id: idA },
    new Map([[idA, { ...gA, activeTabId: estraneo }]]),
    idA
  )
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('activeTabNotInGroup')
})

test('una frazione fuori dai limiti e rifiutata', () => {
  // I gruppi DEVONO avere un tab: sotto uno split un gruppo vuoto fa scattare
  // `emptyNonRootGroup`, che nel corpo di `validate` viene prima delle
  // frazioni. Con `gruppo()` nudo questo test proverebbe l invariante
  // sbagliata e passerebbe lo stesso.
  const conTab = (nome: string): [PaneGroupID, PaneGroup] => {
    const [id, g] = gruppo()
    const tab = {
      id: newWorkspaceTabID(),
      title: nome,
      titleIsAutoNamed: true,
      content: { kind: 'terminal' as const, id: `term-${nome}` },
      viewState: emptyViewState()
    }
    return [id, { ...g, tabs: [tab], activeTabId: tab.id }]
  }
  const [idA, gA] = conTab('a')
  const [idB, gB] = conTab('b')
  const splitId = newSplitID()
  const esito = makeLayout(
    { kind: 'split', id: splitId, axis: 'vertical', fraction: 1.5,
      first: { kind: 'group', id: idA }, second: { kind: 'group', id: idB } },
    new Map([[idA, gA], [idB, gB]]),
    idA
  )
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('invalidFraction')
})

test('un gruppo attivo sconosciuto e rifiutato', () => {
  const [idA, gA] = gruppo()
  const esito = makeLayout({ kind: 'group', id: idA }, new Map([[idA, gA]]), newPaneGroupID())
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('unknownActiveGroup')
})

test('un gruppo non radice senza tab e rifiutato', () => {
  const [idA, gA] = gruppo()
  const [idB, gB] = gruppo()
  const root = {
    kind: 'split' as const,
    id: newSplitID(),
    axis: 'horizontal' as const,
    fraction: 0.5,
    first: { kind: 'group' as const, id: idA },
    second: { kind: 'group' as const, id: idB }
  }
  const esito = makeLayout(root, new Map([[idA, gA], [idB, gB]]), idA)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('emptyNonRootGroup')
})

test('un gruppo radice senza tab e invece lecito: e il workspace appena aperto', () => {
  const [idA, gA] = gruppo()
  expect(makeLayout({ kind: 'group', id: idA }, new Map([[idA, gA]]), idA).ok).toBe(true)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-invariants`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { LayoutNode, PaneGroup, WorkspaceTab } from './layout-types.ts'
import type { PaneGroupID, SplitID, WorkspaceTabID } from './layout-ids.ts'
import { newPaneGroupID } from './layout-ids.ts'

declare const marchioLayout: unique symbol

/**
 * Un layout valido per costruzione. Non esiste un costruttore esportato: si
 * ottiene solo da `makeLayout` o `emptyLayout`, che validano. Uno stato
 * illegale non e "respinto a runtime": non e rappresentabile.
 */
export type WorkspaceLayout = {
  readonly root: LayoutNode
  readonly groups: ReadonlyMap<PaneGroupID, PaneGroup>
  readonly activeGroupId: PaneGroupID
  readonly [marchioLayout]: true
}

export type LayoutError =
  | { kind: 'orphanGroup'; groupId: PaneGroupID }
  | { kind: 'unresolvedGroupLeaf'; groupId: PaneGroupID }
  | { kind: 'duplicateID'; id: string }
  | { kind: 'emptyNonRootGroup'; groupId: PaneGroupID }
  | { kind: 'unknownActiveGroup'; groupId: PaneGroupID }
  | { kind: 'activeTabNotInGroup'; groupId: PaneGroupID }
  | { kind: 'invalidFraction'; splitId: SplitID; fraction: number }
  | { kind: 'duplicateContentOwnership'; contentId: string }
  | { kind: 'unknownGroup'; groupId: PaneGroupID }
  | { kind: 'unknownTab'; tabId: WorkspaceTabID }
  | { kind: 'illegalSplitOfSoleTab'; groupId: PaneGroupID }

function walkGroups(node: LayoutNode, out: PaneGroupID[]): void {
  if (node.kind === 'group') {
    out.push(node.id)
    return
  }
  walkGroups(node.first, out)
  walkGroups(node.second, out)
}

function walkSplits(node: LayoutNode, out: SplitID[]): void {
  if (node.kind === 'group') return
  out.push(node.id)
  walkSplits(node.first, out)
  walkSplits(node.second, out)
}

export function validate(
  root: LayoutNode,
  groups: ReadonlyMap<PaneGroupID, PaneGroup>,
  activeGroupId: PaneGroupID
): LayoutError | null {
  // Nessun controllo su `groups.size === 0`: ogni LayoutNode contiene almeno
  // una foglia di gruppo, quindi un registro vuoto fa comunque scattare
  // `unresolvedGroupLeaf` qui sotto. Un errore dedicato sarebbe un ramo che
  // non si raggiunge mai, e nasconderebbe quello vero.
  const nell_albero: PaneGroupID[] = []
  walkGroups(root, nell_albero)

  // Ogni foglia deve risolvere a un gruppo del registro...
  for (const id of nell_albero) {
    if (!groups.has(id)) return { kind: 'unresolvedGroupLeaf', groupId: id }
  }
  // ...e ogni gruppo del registro deve comparire nell'albero.
  const insieme = new Set<string>(nell_albero)
  for (const id of groups.keys()) {
    if (!insieme.has(id)) return { kind: 'orphanGroup', groupId: id }
  }
  if (insieme.size !== nell_albero.length) {
    // Il campo si chiama `id` e deve contenere UN ID: una frase descrittiva
    // qui arriverebbe intatta a chi mostra l errore, che si aspetta un
    // identificatore da cercare nel layout.
    const ripetuto = nell_albero.find((id, i) => nell_albero.indexOf(id) !== i)
    return { kind: 'duplicateID', id: ripetuto as string }
  }

  const splits: SplitID[] = []
  walkSplits(root, splits)
  if (new Set<string>(splits).size !== splits.length) {
    const ripetuto = splits.find((id, i) => splits.indexOf(id) !== i)
    return { kind: 'duplicateID', id: ripetuto as string }
  }

  if (!groups.has(activeGroupId)) {
    return { kind: 'unknownActiveGroup', groupId: activeGroupId }
  }

  const eRadiceSola = root.kind === 'group'
  const contenuti = new Set<string>()
  const tabVisti = new Set<string>()

  for (const gruppo of groups.values()) {
    // Un gruppo vuoto e lecito SOLO se e l'unico: e il workspace appena
    // aperto. Un riquadro vuoto accanto a uno pieno e spazio sprecato che
    // l'utente non puo chiudere.
    if (gruppo.tabs.length === 0 && !eRadiceSola) {
      return { kind: 'emptyNonRootGroup', groupId: gruppo.id }
    }
    if (
      gruppo.activeTabId !== null &&
      !gruppo.tabs.some((t) => t.id === gruppo.activeTabId)
    ) {
      return { kind: 'activeTabNotInGroup', groupId: gruppo.id }
    }
    for (const tab of gruppo.tabs) {
      if (tabVisti.has(tab.id)) return { kind: 'duplicateID', id: tab.id }
      tabVisti.add(tab.id)
      // Due tab non possono mostrare lo stesso terminale: chiuderne uno
      // ucciderebbe il processo sotto l'altro.
      if (contenuti.has(tab.content.id)) {
        return { kind: 'duplicateContentOwnership', contentId: tab.content.id }
      }
      contenuti.add(tab.content.id)
    }
  }

  for (const frazione of fractions(root)) {
    if (!(frazione.value > 0 && frazione.value < 1)) {
      return { kind: 'invalidFraction', splitId: frazione.id, fraction: frazione.value }
    }
  }

  return null
}

function fractions(node: LayoutNode): { id: SplitID; value: number }[] {
  if (node.kind === 'group') return []
  return [
    { id: node.id, value: node.fraction },
    ...fractions(node.first),
    ...fractions(node.second)
  ]
}

export function makeLayout(
  root: LayoutNode,
  groups: ReadonlyMap<PaneGroupID, PaneGroup>,
  activeGroupId: PaneGroupID
): { ok: true; layout: WorkspaceLayout } | { ok: false; error: LayoutError } {
  const errore = validate(root, groups, activeGroupId)
  if (errore !== null) return { ok: false, error: errore }
  return {
    ok: true,
    layout: { root, groups, activeGroupId } as WorkspaceLayout
  }
}

export function emptyLayout(groupId: PaneGroupID = newPaneGroupID()): WorkspaceLayout {
  const esito = makeLayout(
    { kind: 'group', id: groupId },
    new Map([[groupId, { id: groupId, tabs: [], activeTabId: null }]]),
    groupId
  )
  if (!esito.ok) throw new Error('il layout vuoto deve essere valido')
  return esito.layout
}

// --- Letture ---

export function orderedGroupIds(layout: WorkspaceLayout): PaneGroupID[] {
  const out: PaneGroupID[] = []
  walkGroups(layout.root, out)
  return out
}

export function groupOf(layout: WorkspaceLayout, id: PaneGroupID): PaneGroup | undefined {
  return layout.groups.get(id)
}

/** I tab di tutti i gruppi, in ordine di visita: e cio che mostra la sidebar. */
export function allTabs(layout: WorkspaceLayout): WorkspaceTab[] {
  return orderedGroupIds(layout).flatMap((id) => layout.groups.get(id)?.tabs ?? [])
}

export function tabOf(layout: WorkspaceLayout, id: WorkspaceTabID): WorkspaceTab | undefined {
  return allTabs(layout).find((t) => t.id === id)
}

export function groupContainingTab(
  layout: WorkspaceLayout,
  id: WorkspaceTabID
): PaneGroupID | undefined {
  return orderedGroupIds(layout).find((g) =>
    layout.groups.get(g)?.tabs.some((t) => t.id === id)
  )
}

export function splitIds(layout: WorkspaceLayout): SplitID[] {
  const out: SplitID[] = []
  walkSplits(layout.root, out)
  return out
}

export function fractionOf(layout: WorkspaceLayout, id: SplitID): number | null {
  return fractions(layout.root).find((f) => f.id === id)?.value ?? null
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-invariants`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: invarianti del layout e costruzione validata"
```

---

## Task 3: comandi e transizione

**File:**
- Crea: `src/shared/workspace/layout-commands.ts`
- Crea: `src/shared/workspace/layout-commands.test.ts`

**Interfacce:**
- Consuma: i tipi dei Task 1–2.
- Produce: `type WorkspaceLayoutCommand` (nove casi, discriminati su `kind`);
  `type SplitPlacementSide = 'left' | 'right' | 'above' | 'below'` con
  `axisOf(side)` e `anchorIsFirstChild(side)`;
  `type SplitContentPayload`, `type MoveDestination`;
  `type LayoutDelta`, `emptyDelta()`, `type FocusIntent`,
  `type LayoutTransition`; `isStructuralCommand(comando): boolean`;
  gli schemi Zod corrispondenti (servono al protocollo nel Task 12).

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { isStructuralCommand, axisOf, anchorIsFirstChild } from './layout-commands.ts'
import { newPaneGroupID, newSplitID, newWorkspaceTabID } from './layout-ids.ts'

test('sono strutturali solo i quattro comandi che cambiano cosa esiste', () => {
  const tabId = newWorkspaceTabID()
  expect(isStructuralCommand({ kind: 'closeTab', tabId })).toBe(true)
  expect(isStructuralCommand({ kind: 'activateTab', tabId })).toBe(false)
  expect(
    isStructuralCommand({ kind: 'setPreferredFraction', splitId: newSplitID(), fraction: 0.4 })
  ).toBe(false)
  expect(
    isStructuralCommand({ kind: 'renameTab', tabId, title: 'x', isAutoNamed: false })
  ).toBe(false)
})

test('i quattro lati non collassano in due', () => {
  // Collassare sinistra in destra dividerebbe dal lato sbagliato dell ancora.
  expect(axisOf('left')).toBe('horizontal')
  expect(axisOf('above')).toBe('vertical')
  expect(anchorIsFirstChild('right')).toBe(true)
  expect(anchorIsFirstChild('left')).toBe(false)
  expect(anchorIsFirstChild('below')).toBe(true)
  expect(anchorIsFirstChild('above')).toBe(false)
})

test('lo schema Zod accetta un comando ben formato e rifiuta uno sconosciuto', async () => {
  const { WorkspaceLayoutCommandSchema } = await import('./layout-commands.ts')
  expect(
    WorkspaceLayoutCommandSchema.safeParse({
      kind: 'activateGroup',
      groupId: newPaneGroupID()
    }).success
  ).toBe(true)
  expect(WorkspaceLayoutCommandSchema.safeParse({ kind: 'esplodi' }).success).toBe(false)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-commands`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'
import {
  PaneGroupIDSchema,
  SplitIDSchema,
  WorkspaceTabIDSchema,
  type PaneGroupID,
  type SplitID,
  type WorkspaceTabID
} from './layout-ids.ts'
import { WorkspaceTabSchema, WorkspaceTabViewStateSchema } from './layout-types.ts'
import type { WorkspaceTab, WorkspaceTabViewState, WorkspaceSplitAxis } from './layout-types.ts'

/**
 * Quattro lati, non due. Il menu "Sposta tab in…" espone destinazioni esatte e
 * l'anteprima di bordo risolve il piu vicino dei quattro: collassare sinistra
 * in destra dividerebbe silenziosamente dal lato sbagliato dell'ancora.
 */
export const SplitPlacementSideSchema = z.enum(['left', 'right', 'above', 'below'])
export type SplitPlacementSide = z.infer<typeof SplitPlacementSideSchema>

export function axisOf(side: SplitPlacementSide): WorkspaceSplitAxis {
  return side === 'left' || side === 'right' ? 'horizontal' : 'vertical'
}

/** Vero quando il contenuto dell'ancora resta primo figlio dello split. */
export function anchorIsFirstChild(side: SplitPlacementSide): boolean {
  return side === 'right' || side === 'below'
}

export const SplitContentPayloadSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('newTab'), tab: WorkspaceTabSchema }),
  z.object({ kind: z.literal('existingTab'), tabId: WorkspaceTabIDSchema })
])
export type SplitContentPayload = z.infer<typeof SplitContentPayloadSchema>

export const MoveDestinationSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('group'), groupId: PaneGroupIDSchema, index: z.number().int().min(0) }),
  z.object({
    kind: z.literal('newSplit'),
    anchor: PaneGroupIDSchema,
    placement: SplitPlacementSideSchema,
    newGroup: PaneGroupIDSchema,
    newSplit: SplitIDSchema
  })
])
export type MoveDestination = z.infer<typeof MoveDestinationSchema>

export const WorkspaceLayoutCommandSchema = z.discriminatedUnion('kind', [
  z.object({
    kind: z.literal('insertTab'),
    tab: WorkspaceTabSchema,
    into: PaneGroupIDSchema,
    index: z.number().int().min(0).nullable(),
    activate: z.boolean()
  }),
  z.object({
    kind: z.literal('splitGroup'),
    anchor: PaneGroupIDSchema,
    placement: SplitPlacementSideSchema,
    newGroup: PaneGroupIDSchema,
    newSplit: SplitIDSchema,
    content: SplitContentPayloadSchema
  }),
  z.object({ kind: z.literal('moveTab'), tabId: WorkspaceTabIDSchema, to: MoveDestinationSchema }),
  z.object({ kind: z.literal('closeTab'), tabId: WorkspaceTabIDSchema }),
  z.object({ kind: z.literal('activateTab'), tabId: WorkspaceTabIDSchema }),
  z.object({ kind: z.literal('activateGroup'), groupId: PaneGroupIDSchema }),
  z.object({
    kind: z.literal('setPreferredFraction'),
    splitId: SplitIDSchema,
    fraction: z.number().gt(0).lt(1)
  }),
  z.object({
    kind: z.literal('updateViewState'),
    tabId: WorkspaceTabIDSchema,
    viewState: WorkspaceTabViewStateSchema
  }),
  z.object({
    kind: z.literal('renameTab'),
    tabId: WorkspaceTabIDSchema,
    title: z.string(),
    isAutoNamed: z.boolean()
  })
])
export type WorkspaceLayoutCommand = z.infer<typeof WorkspaceLayoutCommandSchema>

/**
 * L'UNICA fonte di verita sulle classi rilevanti per la durabilita.
 *
 * Strutturale significa "cambia cosa esiste": perderlo a un crash costa
 * lavoro, quindi si scrive subito. Il resto cambia come le cose sono
 * presentate — e trascinare un divisore ne produce centinaia al secondo.
 */
export function isStructuralCommand(command: WorkspaceLayoutCommand): boolean {
  switch (command.kind) {
    case 'insertTab':
    case 'splitGroup':
    case 'moveTab':
    case 'closeTab':
      return true
    case 'activateTab':
    case 'activateGroup':
    case 'setPreferredFraction':
    case 'updateViewState':
    case 'renameTab':
      return false
  }
}

export interface LayoutDelta {
  insertedTabs: WorkspaceTabID[]
  removedTabs: WorkspaceTabID[]
  movedTabs: WorkspaceTabID[]
  insertedGroups: PaneGroupID[]
  removedGroups: PaneGroupID[]
  insertedSplits: SplitID[]
  collapsedSplits: SplitID[]
  activeGroupChanged: PaneGroupID | null
  activeTabChanges: [PaneGroupID, WorkspaceTabID | null][]
  preferredFractionChanges: [SplitID, number][]
  isStructural: boolean
}

export function emptyDelta(): LayoutDelta {
  return {
    insertedTabs: [],
    removedTabs: [],
    movedTabs: [],
    insertedGroups: [],
    removedGroups: [],
    insertedSplits: [],
    collapsedSplits: [],
    activeGroupChanged: null,
    activeTabChanges: [],
    preferredFractionChanges: [],
    isStructural: false
  }
}

/**
 * Cosa deve prendere il fuoco dopo il comando. Lo decide il riduttore, non la
 * vista: chiudendo un tab e il riduttore a sapere quale resta. Sparpagliare
 * questa logica nei gestori di evento significa vederla contraddirsi.
 */
export type FocusIntent =
  | { kind: 'none' }
  | { kind: 'focusTab'; tabId: WorkspaceTabID }
  | { kind: 'focusDivider'; splitId: SplitID }

export interface LayoutTransition {
  layout: import('./layout-invariants.ts').WorkspaceLayout
  delta: LayoutDelta
  focusIntent: FocusIntent
}

export type { WorkspaceTab, WorkspaceTabViewState }
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-commands`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: comandi del layout, delta e classificazione strutturale"
```

---

## Task 4: riduttore — i cinque comandi non strutturali

**File:**
- Crea: `src/shared/workspace/layout-engine.ts`
- Crea: `src/shared/workspace/layout-engine.test.ts`

**Interfacce:**
- Consuma: Task 1–3.
- Produce:
  `apply(command, layout): { ok: true } & LayoutTransition | { ok: false; error: LayoutError }`.
  In questo task gestisce `activateTab`, `activateGroup`, `renameTab`,
  `updateViewState`, `setPreferredFraction`; gli altri quattro **lanciano**
  temporaneamente (`throw new Error(...)`) e vengono completati nei Task 5 e 6.

**Nota sul taglio:** `apply` fa uno `switch` esaustivo su un'unione
discriminata. I quattro comandi strutturali devono comparire fin dal primo
task, altrimenti il compilatore rifiuta il file. Sono presenti come rami che
**lanciano**, e i Task 5 e 6 li **sostituiscono**.

Lanciano, invece di restituire un `LayoutError`, perche' un errore di dominio
inventato — `unknownTab` per un comando che non ha nulla di sbagliato — non
descrive niente e puo' essere soddisfatto per caso da un test che si limita ad
asserire `ok === false`, facendo sembrare finito un lavoro mai iniziato.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { apply } from './layout-engine.ts'
import { emptyLayout, makeLayout, groupOf, fractionOf } from './layout-invariants.ts'
import { newPaneGroupID, newSplitID, newWorkspaceTabID } from './layout-ids.ts'
import { emptyViewState } from './layout-types.ts'
import type { PaneGroup, WorkspaceTab } from './layout-types.ts'

function tab(titolo: string, contenuto: string): WorkspaceTab {
  return {
    id: newWorkspaceTabID(),
    title: titolo,
    titleIsAutoNamed: true,
    content: { kind: 'terminal', id: contenuto },
    viewState: emptyViewState()
  }
}

/** Layout con un gruppo solo e due tab dentro. */
function conDueTab(): { layout: ReturnType<typeof emptyLayout>; t1: WorkspaceTab; t2: WorkspaceTab } {
  const id = newPaneGroupID()
  const t1 = tab('Terminale 1', 'term-1')
  const t2 = tab('Terminale 2', 'term-2')
  const gruppo: PaneGroup = { id, tabs: [t1, t2], activeTabId: t1.id }
  const esito = makeLayout({ kind: 'group', id }, new Map([[id, gruppo]]), id)
  if (!esito.ok) throw new Error('layout di prova non valido')
  return { layout: esito.layout, t1, t2 }
}

test('activateTab cambia il tab attivo e chiede il fuoco su di esso', () => {
  const { layout, t2 } = conDueTab()
  const esito = apply({ kind: 'activateTab', tabId: t2.id }, layout)
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  const gruppoId = esito.layout.activeGroupId
  expect(groupOf(esito.layout, gruppoId)?.activeTabId).toBe(t2.id)
  expect(esito.focusIntent).toEqual({ kind: 'focusTab', tabId: t2.id })
  expect(esito.delta.isStructural).toBe(false)
})

test('activateTab su un tab inesistente e respinto e non tocca il layout', () => {
  const { layout } = conDueTab()
  const esito = apply({ kind: 'activateTab', tabId: newWorkspaceTabID() }, layout)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('unknownTab')
})

test('renameTab cambia titolo e marca il delta come non strutturale', () => {
  const { layout, t1 } = conDueTab()
  const esito = apply({ kind: 'renameTab', tabId: t1.id, title: 'build', isAutoNamed: false }, layout)
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  const rinominato = groupOf(esito.layout, esito.layout.activeGroupId)?.tabs[0]
  expect(rinominato?.title).toBe('build')
  expect(rinominato?.titleIsAutoNamed).toBe(false)
  expect(esito.delta.isStructural).toBe(false)
})

test('updateViewState sostituisce lo stato di vista del solo tab indicato', () => {
  const { layout, t1, t2 } = conDueTab()
  const nuovo = { ...emptyViewState(), followsTail: true }
  const esito = apply({ kind: 'updateViewState', tabId: t1.id, viewState: nuovo }, layout)
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  const tabs = groupOf(esito.layout, esito.layout.activeGroupId)?.tabs ?? []
  expect(tabs.find((t) => t.id === t1.id)?.viewState.followsTail).toBe(true)
  expect(tabs.find((t) => t.id === t2.id)?.viewState.followsTail).toBe(false)
})

test('setPreferredFraction su uno split inesistente e respinto', () => {
  const { layout } = conDueTab()
  const esito = apply(
    { kind: 'setPreferredFraction', splitId: newSplitID(), fraction: 0.3 },
    layout
  )
  expect(esito.ok).toBe(false)
})

test('activateGroup su un gruppo sconosciuto e respinto', () => {
  const esito = apply({ kind: 'activateGroup', groupId: newPaneGroupID() }, emptyLayout())
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('unknownGroup')
})

test('un comando respinto lascia il layout identico', () => {
  const { layout } = conDueTab()
  const prima = JSON.stringify([...layout.groups.entries()])
  apply({ kind: 'activateTab', tabId: newWorkspaceTabID() }, layout)
  expect(JSON.stringify([...layout.groups.entries()])).toBe(prima)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-engine`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import {
  makeLayout,
  groupContainingTab,
  fractionOf,
  type WorkspaceLayout,
  type LayoutError
} from './layout-invariants.ts'
import {
  emptyDelta,
  isStructuralCommand,
  type FocusIntent,
  type LayoutDelta,
  type WorkspaceLayoutCommand
} from './layout-commands.ts'
import type { LayoutNode, PaneGroup } from './layout-types.ts'
import type { PaneGroupID, SplitID } from './layout-ids.ts'

export type ApplyResult =
  | { ok: true; layout: WorkspaceLayout; delta: LayoutDelta; focusIntent: FocusIntent }
  | { ok: false; error: LayoutError }

function ricostruisci(
  root: LayoutNode,
  groups: ReadonlyMap<PaneGroupID, PaneGroup>,
  activeGroupId: PaneGroupID,
  delta: LayoutDelta,
  focusIntent: FocusIntent
): ApplyResult {
  const esito = makeLayout(root, groups, activeGroupId)
  // Il layout nuovo passa dagli invarianti come qualunque altro: il riduttore
  // non ha il permesso di produrre uno stato che il costruttore rifiuterebbe.
  if (!esito.ok) return esito
  return { ok: true, layout: esito.layout, delta, focusIntent }
}

/** Sostituisce la frazione di uno split, lasciando intatto il resto. */
function conFrazione(node: LayoutNode, splitId: SplitID, fraction: number): LayoutNode {
  if (node.kind === 'group') return node
  if (node.id === splitId) return { ...node, fraction }
  return {
    ...node,
    first: conFrazione(node.first, splitId, fraction),
    second: conFrazione(node.second, splitId, fraction)
  }
}

export function apply(
  command: WorkspaceLayoutCommand,
  layout: WorkspaceLayout
): ApplyResult {
  switch (command.kind) {
    case 'activateGroup': {
      if (!layout.groups.has(command.groupId)) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: command.groupId } }
      }
      const delta = { ...emptyDelta(), activeGroupChanged: command.groupId }
      const attivo = layout.groups.get(command.groupId)?.activeTabId ?? null
      return ricostruisci(
        layout.root,
        layout.groups,
        command.groupId,
        delta,
        attivo === null ? { kind: 'none' } : { kind: 'focusTab', tabId: attivo }
      )
    }

    case 'activateTab': {
      const gruppoId = groupContainingTab(layout, command.tabId)
      if (gruppoId === undefined) {
        return { ok: false, error: { kind: 'unknownTab', tabId: command.tabId } }
      }
      const gruppo = layout.groups.get(gruppoId)
      if (gruppo === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: gruppoId } }
      }
      const groups = new Map(layout.groups)
      groups.set(gruppoId, { ...gruppo, activeTabId: command.tabId })
      const delta = {
        ...emptyDelta(),
        activeTabChanges: [[gruppoId, command.tabId] as [PaneGroupID, typeof command.tabId]],
        activeGroupChanged: gruppoId === layout.activeGroupId ? null : gruppoId
      }
      return ricostruisci(layout.root, groups, gruppoId, delta, {
        kind: 'focusTab',
        tabId: command.tabId
      })
    }

    case 'renameTab':
    case 'updateViewState': {
      const gruppoId = groupContainingTab(layout, command.tabId)
      if (gruppoId === undefined) {
        return { ok: false, error: { kind: 'unknownTab', tabId: command.tabId } }
      }
      const gruppo = layout.groups.get(gruppoId)
      if (gruppo === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: gruppoId } }
      }
      const tabs = gruppo.tabs.map((t) =>
        t.id !== command.tabId
          ? t
          : command.kind === 'renameTab'
            ? { ...t, title: command.title, titleIsAutoNamed: command.isAutoNamed }
            : { ...t, viewState: command.viewState }
      )
      const groups = new Map(layout.groups)
      groups.set(gruppoId, { ...gruppo, tabs })
      return ricostruisci(
        layout.root,
        groups,
        layout.activeGroupId,
        emptyDelta(),
        { kind: 'none' }
      )
    }

    case 'setPreferredFraction': {
      if (fractionOf(layout, command.splitId) === null) {
        return {
          ok: false,
          error: { kind: 'invalidFraction', splitId: command.splitId, fraction: command.fraction }
        }
      }
      const delta = {
        ...emptyDelta(),
        preferredFractionChanges: [[command.splitId, command.fraction] as [SplitID, number]]
      }
      return ricostruisci(
        conFrazione(layout.root, command.splitId, command.fraction),
        layout.groups,
        layout.activeGroupId,
        delta,
        { kind: 'focusDivider', splitId: command.splitId }
      )
    }

    // I quattro comandi strutturali arrivano nei Task 5 e 6. Sono qui perche
    // lo switch su un'unione discriminata deve essere esaustivo.
    //
    // Un `throw`, non un errore di dominio: restituire per esempio
    // `unknownGroup` per un comando perfettamente valido significa inventare
    // un errore che non descrive niente, e un test potrebbe soddisfarlo per
    // caso — facendo sembrare finito un lavoro che non e' nemmeno iniziato.
    // Questi quattro rami vengono SOSTITUITI nei Task 5 e 6.
    case 'insertTab':
    case 'closeTab':
    case 'splitGroup':
    case 'moveTab':
      throw new Error(`comando non ancora implementato: ${command.kind}`)
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-engine`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: riduttore del layout per i comandi non strutturali"
```

---

## Task 5: riduttore — `insertTab` e `closeTab`

**File:**
- Modifica: `src/shared/workspace/layout-engine.ts` (sostituisce i rami
  `insertTab` e `closeTab` del Task 4)
- Modifica: `src/shared/workspace/layout-engine.test.ts`

**Interfacce:**
- Consuma: Task 4.
- Produce: gli stessi tipi; `insertTab` e `closeTab` funzionanti. Chiudere
  l'ultimo tab di un gruppo non radice **collassa** il gruppo e il suo split.

- [ ] **Passo 1: scrivere il test che fallisce**

Aggiungere a `layout-engine.test.ts` (le funzioni `tab` e `conDueTab` del
Task 4 restano):

```ts
test('insertTab in coda e attivazione opzionale', () => {
  const { layout, t1 } = conDueTab()
  const t3 = tab('Terminale 3', 'term-3')
  const esito = apply(
    { kind: 'insertTab', tab: t3, into: layout.activeGroupId, index: null, activate: true },
    layout
  )
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  const gruppo = groupOf(esito.layout, layout.activeGroupId)
  expect(gruppo?.tabs.map((t) => t.id)).toEqual([t1.id, expect.any(String), t3.id])
  expect(gruppo?.activeTabId).toBe(t3.id)
  expect(esito.delta.insertedTabs).toEqual([t3.id])
  expect(esito.delta.isStructural).toBe(true)
})

test('insertTab a un indice preciso', () => {
  const { layout, t1 } = conDueTab()
  const t3 = tab('Terminale 3', 'term-3')
  const esito = apply(
    { kind: 'insertTab', tab: t3, into: layout.activeGroupId, index: 0, activate: false },
    layout
  )
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  expect(groupOf(esito.layout, layout.activeGroupId)?.tabs[0].id).toBe(t3.id)
  // activate:false non sposta il tab attivo.
  expect(groupOf(esito.layout, layout.activeGroupId)?.activeTabId).toBe(t1.id)
})

test('insertTab di un contenuto gia mostrato altrove e respinto', () => {
  const { layout } = conDueTab()
  const doppione = tab('Doppione', 'term-1')
  const esito = apply(
    { kind: 'insertTab', tab: doppione, into: layout.activeGroupId, index: null, activate: false },
    layout
  )
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('duplicateContentOwnership')
})

test('closeTab rimuove il tab e sposta il fuoco su quello rimasto', () => {
  const { layout, t1, t2 } = conDueTab()
  const esito = apply({ kind: 'closeTab', tabId: t1.id }, layout)
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  expect(groupOf(esito.layout, layout.activeGroupId)?.tabs.map((t) => t.id)).toEqual([t2.id])
  expect(esito.focusIntent).toEqual({ kind: 'focusTab', tabId: t2.id })
  expect(esito.delta.removedTabs).toEqual([t1.id])
})

test('chiudere l ultimo tab del gruppo radice lascia un workspace vuoto valido', () => {
  const { layout, t1, t2 } = conDueTab()
  const uno = apply({ kind: 'closeTab', tabId: t1.id }, layout)
  expect(uno.ok).toBe(true)
  if (!uno.ok) return
  const due = apply({ kind: 'closeTab', tabId: t2.id }, uno.layout)
  expect(due.ok).toBe(true)
  if (!due.ok) return
  expect(groupOf(due.layout, due.layout.activeGroupId)?.tabs).toEqual([])
  expect(due.focusIntent).toEqual({ kind: 'none' })
})

test('closeTab su un tab inesistente e respinto', () => {
  const { layout } = conDueTab()
  const esito = apply({ kind: 'closeTab', tabId: newWorkspaceTabID() }, layout)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('unknownTab')
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-engine`
Atteso: FAIL — i rami restituiscono ancora l'errore segnaposto del Task 4.

- [ ] **Passo 3: sostituire i due rami**

In `layout-engine.ts`, togliere `insertTab` e `closeTab` dal ramo comune in
fondo e aggiungere prima di esso:

```ts
    case 'insertTab': {
      const gruppo = layout.groups.get(command.into)
      if (gruppo === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: command.into } }
      }
      const indice = command.index ?? gruppo.tabs.length
      const tabs = [...gruppo.tabs]
      tabs.splice(Math.min(indice, tabs.length), 0, command.tab)
      const groups = new Map(layout.groups)
      groups.set(command.into, {
        ...gruppo,
        tabs,
        activeTabId: command.activate ? command.tab.id : gruppo.activeTabId
      })
      const delta = { ...emptyDelta(), insertedTabs: [command.tab.id], isStructural: true }
      return ricostruisci(
        layout.root,
        groups,
        command.activate ? command.into : layout.activeGroupId,
        delta,
        command.activate ? { kind: 'focusTab', tabId: command.tab.id } : { kind: 'none' }
      )
    }

    case 'closeTab': {
      const gruppoId = groupContainingTab(layout, command.tabId)
      if (gruppoId === undefined) {
        return { ok: false, error: { kind: 'unknownTab', tabId: command.tabId } }
      }
      const gruppo = layout.groups.get(gruppoId)
      if (gruppo === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: gruppoId } }
      }
      const indice = gruppo.tabs.findIndex((t) => t.id === command.tabId)
      const tabs = gruppo.tabs.filter((t) => t.id !== command.tabId)
      const delta: LayoutDelta = {
        ...emptyDelta(),
        removedTabs: [command.tabId],
        isStructural: true
      }

      // Gruppo svuotato e non unico: il gruppo sparisce e il suo split
      // collassa sul fratello. Un riquadro vuoto accanto a uno pieno sarebbe
      // spazio che l'utente non puo recuperare.
      if (tabs.length === 0 && layout.root.kind !== 'group') {
        const { root, collapsed } = rimuoviGruppo(layout.root, gruppoId)
        if (root === null) {
          return { ok: false, error: { kind: 'unknownGroup', groupId: gruppoId } }
        }
        const groups = new Map(layout.groups)
        groups.delete(gruppoId)
        delta.removedGroups = [gruppoId]
        delta.collapsedSplits = collapsed
        const nuovoAttivo =
          layout.activeGroupId === gruppoId
            ? (primoGruppo(root) ?? layout.activeGroupId)
            : layout.activeGroupId
        const attivo = groups.get(nuovoAttivo)?.activeTabId ?? null
        return ricostruisci(root, groups, nuovoAttivo, delta, {
          ...(attivo === null ? { kind: 'none' } : { kind: 'focusTab', tabId: attivo })
        } as FocusIntent)
      }

      // Il tab attivo era quello chiuso: passa al successivo, o al precedente
      // se non c'era successivo.
      const nuovoAttivoTab =
        gruppo.activeTabId !== command.tabId
          ? gruppo.activeTabId
          : (tabs[indice]?.id ?? tabs[indice - 1]?.id ?? null)
      const groups = new Map(layout.groups)
      groups.set(gruppoId, { ...gruppo, tabs, activeTabId: nuovoAttivoTab })
      delta.activeTabChanges = [[gruppoId, nuovoAttivoTab]]
      return ricostruisci(layout.root, groups, layout.activeGroupId, delta, {
        ...(nuovoAttivoTab === null
          ? { kind: 'none' }
          : { kind: 'focusTab', tabId: nuovoAttivoTab })
      } as FocusIntent)
    }
```

e aggiungere in fondo al file le due funzioni di supporto:

```ts
/**
 * Toglie una foglia dall'albero facendo collassare il suo split sul fratello.
 * Restituisce anche gli split spariti, perche il delta li riporti.
 */
function rimuoviGruppo(
  node: LayoutNode,
  target: PaneGroupID
): { root: LayoutNode | null; collapsed: SplitID[] } {
  if (node.kind === 'group') {
    return node.id === target ? { root: null, collapsed: [] } : { root: node, collapsed: [] }
  }
  const a = rimuoviGruppo(node.first, target)
  if (a.root === null) return { root: node.second, collapsed: [node.id, ...a.collapsed] }
  const b = rimuoviGruppo(node.second, target)
  if (b.root === null) return { root: node.first, collapsed: [node.id, ...b.collapsed] }
  return { root: { ...node, first: a.root, second: b.root }, collapsed: [] }
}

function primoGruppo(node: LayoutNode): PaneGroupID | null {
  return node.kind === 'group' ? node.id : primoGruppo(node.first)
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-engine`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: inserimento e chiusura dei tab con collasso del gruppo svuotato"
```

---

## Task 6: riduttore — `splitGroup` e `moveTab`

**File:**
- Modifica: `src/shared/workspace/layout-engine.ts` (sostituisce gli ultimi due
  rami segnaposto)
- Modifica: `src/shared/workspace/layout-engine.test.ts`

**Interfacce:**
- Consuma: Task 5.
- Produce: `apply` completo su tutti e nove i comandi. Il ramo segnaposto in
  fondo allo `switch` sparisce.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
test('splitGroup a destra mette l ancora come primo figlio', () => {
  const { layout, t1 } = conDueTab()
  const nuovoGruppo = newPaneGroupID()
  const nuovoSplit = newSplitID()
  const t3 = tab('Terminale 3', 'term-3')
  const esito = apply(
    {
      kind: 'splitGroup',
      anchor: layout.activeGroupId,
      placement: 'right',
      newGroup: nuovoGruppo,
      newSplit: nuovoSplit,
      content: { kind: 'newTab', tab: t3 }
    },
    layout
  )
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  expect(esito.layout.root.kind).toBe('split')
  if (esito.layout.root.kind !== 'split') return
  expect(esito.layout.root.axis).toBe('horizontal')
  expect(esito.layout.root.fraction).toBe(0.5)
  expect(esito.layout.root.first).toEqual({ kind: 'group', id: layout.activeGroupId })
  expect(esito.layout.root.second).toEqual({ kind: 'group', id: nuovoGruppo })
  expect(groupOf(esito.layout, nuovoGruppo)?.tabs.map((t) => t.id)).toEqual([t3.id])
  expect(esito.delta.insertedGroups).toEqual([nuovoGruppo])
  expect(esito.delta.insertedSplits).toEqual([nuovoSplit])
  expect(esito.delta.isStructural).toBe(true)
})

test('splitGroup a sinistra mette il nuovo contenuto come primo figlio', () => {
  const { layout } = conDueTab()
  const nuovoGruppo = newPaneGroupID()
  const esito = apply(
    {
      kind: 'splitGroup',
      anchor: layout.activeGroupId,
      placement: 'left',
      newGroup: nuovoGruppo,
      newSplit: newSplitID(),
      content: { kind: 'newTab', tab: tab('Terminale 3', 'term-3') }
    },
    layout
  )
  expect(esito.ok).toBe(true)
  if (!esito.ok) return
  if (esito.layout.root.kind !== 'split') return
  // Se questo fosse `layout.activeGroupId`, il riquadro nuovo comparirebbe
  // dal lato sbagliato dell ancora.
  expect(esito.layout.root.first).toEqual({ kind: 'group', id: nuovoGruppo })
})

test('splitGroup che sposta l unico tab dell ancora e respinto', () => {
  const id = newPaneGroupID()
  const solo = tab('Unico', 'term-1')
  const esito0 = makeLayout(
    { kind: 'group', id },
    new Map([[id, { id, tabs: [solo], activeTabId: solo.id }]]),
    id
  )
  if (!esito0.ok) throw new Error('layout di prova non valido')
  const esito = apply(
    {
      kind: 'splitGroup',
      anchor: id,
      placement: 'right',
      newGroup: newPaneGroupID(),
      newSplit: newSplitID(),
      content: { kind: 'existingTab', tabId: solo.id }
    },
    esito0.layout
  )
  // Lasciare il gruppo ancora vuoto violerebbe `emptyNonRootGroup`: meglio un
  // errore con nome che un layout che il costruttore rifiuterebbe.
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.error.kind).toBe('illegalSplitOfSoleTab')
})

test('moveTab fra gruppi esistenti sposta e non duplica', () => {
  const { layout, t1, t2 } = conDueTab()
  const nuovoGruppo = newPaneGroupID()
  const diviso = apply(
    {
      kind: 'splitGroup',
      anchor: layout.activeGroupId,
      placement: 'right',
      newGroup: nuovoGruppo,
      newSplit: newSplitID(),
      content: { kind: 'existingTab', tabId: t2.id }
    },
    layout
  )
  expect(diviso.ok).toBe(true)
  if (!diviso.ok) return

  const spostato = apply(
    { kind: 'moveTab', tabId: t1.id, to: { kind: 'group', groupId: nuovoGruppo, index: 0 } },
    diviso.layout
  )
  // t1 era l ultimo tab del gruppo di origine: spostarlo lo svuota e il suo
  // split collassa.
  expect(spostato.ok).toBe(true)
  if (!spostato.ok) return
  expect(groupOf(spostato.layout, nuovoGruppo)?.tabs.map((t) => t.id)).toEqual([t1.id, t2.id])
  expect(spostato.layout.root.kind).toBe('group')
  expect(spostato.delta.movedTabs).toEqual([t1.id])
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-engine`
Atteso: FAIL.

- [ ] **Passo 3: sostituire i due rami**

Togliere il ramo comune segnaposto e aggiungere:

```ts
    case 'splitGroup': {
      const ancora = layout.groups.get(command.anchor)
      if (ancora === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: command.anchor } }
      }

      let tabNuovo
      let ancoraAggiornata = ancora
      if (command.content.kind === 'newTab') {
        tabNuovo = command.content.tab
      } else {
        const esistente = ancora.tabs.find((t) => t.id === command.content.tabId)
        if (esistente === undefined) {
          return { ok: false, error: { kind: 'unknownTab', tabId: command.content.tabId } }
        }
        // Spostare l'unico tab dell'ancora la lascerebbe vuota accanto a un
        // riquadro pieno: e proprio cio che `emptyNonRootGroup` vieta.
        if (ancora.tabs.length === 1) {
          return { ok: false, error: { kind: 'illegalSplitOfSoleTab', groupId: command.anchor } }
        }
        tabNuovo = esistente
        const rimasti = ancora.tabs.filter((t) => t.id !== esistente.id)
        ancoraAggiornata = {
          ...ancora,
          tabs: rimasti,
          activeTabId:
            ancora.activeTabId === esistente.id ? (rimasti[0]?.id ?? null) : ancora.activeTabId
        }
      }

      const groups = new Map(layout.groups)
      groups.set(command.anchor, ancoraAggiornata)
      groups.set(command.newGroup, {
        id: command.newGroup,
        tabs: [tabNuovo],
        activeTabId: tabNuovo.id
      })

      const foglia: LayoutNode = { kind: 'group', id: command.newGroup }
      const primo = anchorIsFirstChild(command.placement)
      const root = sostituisciFoglia(layout.root, command.anchor, {
        kind: 'split',
        id: command.newSplit,
        axis: axisOf(command.placement),
        // Meta e meta: la posizione la aggiusta l'utente trascinando.
        fraction: 0.5,
        first: primo ? { kind: 'group', id: command.anchor } : foglia,
        second: primo ? foglia : { kind: 'group', id: command.anchor }
      })

      const delta: LayoutDelta = {
        ...emptyDelta(),
        insertedGroups: [command.newGroup],
        insertedSplits: [command.newSplit],
        ...(command.content.kind === 'newTab'
          ? { insertedTabs: [tabNuovo.id] }
          : { movedTabs: [tabNuovo.id] }),
        isStructural: true
      }
      return ricostruisci(root, groups, command.newGroup, delta, {
        kind: 'focusTab',
        tabId: tabNuovo.id
      })
    }

    case 'moveTab': {
      const origine = groupContainingTab(layout, command.tabId)
      if (origine === undefined) {
        return { ok: false, error: { kind: 'unknownTab', tabId: command.tabId } }
      }
      if (command.to.kind === 'newSplit') {
        // Un rilascio su un bordo e uno split che porta con se un tab
        // esistente: stessa operazione, stesso codice.
        return apply(
          {
            kind: 'splitGroup',
            anchor: command.to.anchor,
            placement: command.to.placement,
            newGroup: command.to.newGroup,
            newSplit: command.to.newSplit,
            content: { kind: 'existingTab', tabId: command.tabId }
          },
          layout
        )
      }

      const gruppoOrigine = layout.groups.get(origine)
      const gruppoDest = layout.groups.get(command.to.groupId)
      if (gruppoOrigine === undefined || gruppoDest === undefined) {
        return { ok: false, error: { kind: 'unknownGroup', groupId: command.to.groupId } }
      }
      const spostato = gruppoOrigine.tabs.find((t) => t.id === command.tabId)
      if (spostato === undefined) {
        return { ok: false, error: { kind: 'unknownTab', tabId: command.tabId } }
      }

      const groups = new Map(layout.groups)
      const rimasti = gruppoOrigine.tabs.filter((t) => t.id !== command.tabId)
      const destTabs =
        origine === command.to.groupId ? [...rimasti] : [...gruppoDest.tabs]
      destTabs.splice(Math.min(command.to.index, destTabs.length), 0, spostato)
      groups.set(command.to.groupId, {
        ...gruppoDest,
        tabs: destTabs,
        activeTabId: command.tabId
      })

      const delta: LayoutDelta = { ...emptyDelta(), movedTabs: [command.tabId], isStructural: true }
      let root = layout.root
      if (origine !== command.to.groupId) {
        if (rimasti.length === 0 && layout.root.kind !== 'group') {
          const esito = rimuoviGruppo(layout.root, origine)
          if (esito.root === null) {
            return { ok: false, error: { kind: 'unknownGroup', groupId: origine } }
          }
          root = esito.root
          groups.delete(origine)
          delta.removedGroups = [origine]
          delta.collapsedSplits = esito.collapsed
        } else {
          groups.set(origine, {
            ...gruppoOrigine,
            tabs: rimasti,
            activeTabId:
              gruppoOrigine.activeTabId === command.tabId
                ? (rimasti[0]?.id ?? null)
                : gruppoOrigine.activeTabId
          })
        }
      }

      return ricostruisci(root, groups, command.to.groupId, delta, {
        kind: 'focusTab',
        tabId: command.tabId
      })
    }
```

e aggiungere la funzione di supporto:

```ts
/** Sostituisce la foglia di un gruppo con un sottoalbero. */
function sostituisciFoglia(
  node: LayoutNode,
  target: PaneGroupID,
  sostituto: LayoutNode
): LayoutNode {
  if (node.kind === 'group') return node.id === target ? sostituto : node
  return {
    ...node,
    first: sostituisciFoglia(node.first, target, sostituto),
    second: sostituisciFoglia(node.second, target, sostituto)
  }
}
```

Aggiungere in testa al file l'import `axisOf, anchorIsFirstChild` da
`./layout-commands.ts`.

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-engine`
Atteso: PASS. Il riduttore ora copre tutti e nove i comandi.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: divisione dei gruppi e spostamento dei tab"
```

---

## Task 7: metriche e geometria

**File:**
- Crea: `src/shared/workspace/layout-metrics.ts`
- Crea: `src/shared/workspace/layout-geometry.ts`
- Crea: `src/shared/workspace/layout-geometry.test.ts`

**Interfacce:**
- Consuma: Task 2.
- Produce: `WORKSPACE_METRICS` con `minGroupWidth: 240`, `minGroupHeight: 160`,
  `dividerBand: 6`, `dividerHairline: 1`, `tabStripHeight: 32`,
  `edgeBandFraction: 0.22`, `dragThreshold: 4`;
  `type Rect = { x: number; y: number; w: number; h: number }`;
  `groupRects(layout, container): Map<PaneGroupID, Rect>`;
  `dividerRects(layout, container): { splitId: SplitID; axis: WorkspaceSplitAxis; band: Rect; hairline: Rect }[]`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { groupRects, dividerRects } from './layout-geometry.ts'
import { WORKSPACE_METRICS } from './layout-metrics.ts'
import { makeLayout } from './layout-invariants.ts'
import { newPaneGroupID, newSplitID } from './layout-ids.ts'
import type { PaneGroup } from './layout-types.ts'

const CONTENITORE = { x: 0, y: 0, w: 1000, h: 600 }

function dueRiquadri(fraction = 0.5): {
  layout: ReturnType<typeof makeLayout>
  a: ReturnType<typeof newPaneGroupID>
  b: ReturnType<typeof newPaneGroupID>
  s: ReturnType<typeof newSplitID>
} {
  const a = newPaneGroupID()
  const b = newPaneGroupID()
  const s = newSplitID()
  const g = (id: typeof a): PaneGroup => ({
    id,
    tabs: [
      {
        id: crypto.randomUUID() as never,
        title: 't',
        titleIsAutoNamed: true,
        content: { kind: 'terminal', id: `term-${id}` },
        viewState: {
          documentCaretOffset: null,
          documentSelectionLength: null,
          documentScrollAnchor: null,
          documentFoldedRanges: [],
          editorMode: null,
          chatComposerDraft: null,
          chatAttachmentReferences: [],
          chatTranscriptAnchor: null,
          followsTail: false,
          terminalViewportAnchor: null
        }
      }
    ],
    activeTabId: null
  })
  // `g` genera un id di tab nuovo a ogni chiamata: va invocata UNA volta per
  // gruppo. Chiamarla due volte — una per i tab, una per l'id attivo — darebbe
  // un `activeTabId` che punta a un tab non presente, e `makeLayout`
  // respingerebbe il fixture con `activeTabNotInGroup` prima ancora che la
  // geometria venga calcolata.
  const ga = g(a)
  const gb = g(b)
  const layout = makeLayout(
    {
      kind: 'split',
      id: s,
      axis: 'horizontal',
      fraction,
      first: { kind: 'group', id: a },
      second: { kind: 'group', id: b }
    },
    new Map([
      [a, { ...ga, activeTabId: ga.tabs[0].id }],
      [b, { ...gb, activeTabId: gb.tabs[0].id }]
    ]),
    a
  )
  return { layout, a, b, s }
}

test('un gruppo solo occupa tutto il contenitore', () => {
  const id = newPaneGroupID()
  const l = makeLayout(
    { kind: 'group', id },
    new Map([[id, { id, tabs: [], activeTabId: null }]]),
    id
  )
  if (!l.ok) throw new Error('layout non valido')
  expect(groupRects(l.layout, CONTENITORE).get(id)).toEqual(CONTENITORE)
})

test('uno split orizzontale divide la larghezza lasciando la banda del divisore', () => {
  const { layout, a, b } = dueRiquadri(0.5)
  if (!layout.ok) throw new Error('layout non valido')
  const rects = groupRects(layout.layout, CONTENITORE)
  const meta = WORKSPACE_METRICS.dividerBand / 2
  expect(rects.get(a)).toEqual({ x: 0, y: 0, w: 500 - meta, h: 600 })
  expect(rects.get(b)).toEqual({ x: 500 + meta, y: 0, w: 500 - meta, h: 600 })
})

test('la frazione sposta il confine', () => {
  const { layout, a } = dueRiquadri(0.25)
  if (!layout.ok) throw new Error('layout non valido')
  expect(groupRects(layout.layout, CONTENITORE).get(a)?.w).toBe(250 - WORKSPACE_METRICS.dividerBand / 2)
})

test('la banda del divisore e afferrabile ma solo il filo si dipinge', () => {
  const { layout, s } = dueRiquadri(0.5)
  if (!layout.ok) throw new Error('layout non valido')
  const [divisore] = dividerRects(layout.layout, CONTENITORE)
  expect(divisore.splitId).toBe(s)
  expect(divisore.band.w).toBe(WORKSPACE_METRICS.dividerBand)
  expect(divisore.hairline.w).toBe(WORKSPACE_METRICS.dividerHairline)
  // Il filo sta al centro della banda.
  expect(divisore.hairline.x).toBe(divisore.band.x + (6 - 1) / 2)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-geometry`
Atteso: FAIL, moduli inesistenti.

- [ ] **Passo 3: implementare `layout-metrics.ts`**

```ts
/**
 * Costanti misurate sull'app Swift, non scelte a occhio.
 *
 * `dividerBand` e `dividerHairline` sono due numeri diversi per due scopi
 * diversi: la banda resta larga perche il divisore sia afferrabile col mouse,
 * ma si dipinge solo il filo — una banda piena si leggerebbe come un corridoio
 * fra i riquadri invece che come una giuntura.
 */
export const WORKSPACE_METRICS = {
  minGroupWidth: 240,
  minGroupHeight: 160,
  dividerBand: 6,
  dividerHairline: 1,
  tabStripHeight: 32,
  /** Frazione del riquadro, per lato, che conta come "bordo" nel rilascio. */
  edgeBandFraction: 0.22,
  /** Spostamento oltre il quale un premi-e-trascina diventa trascinamento. */
  dragThreshold: 4
} as const
```

`minGroupWidth`, `minGroupHeight` e `dragThreshold` sono definiti qui ma **non
usati in 4a**, ed è corretto così: il riduttore non conosce la dimensione della
finestra, quindi il rifiuto di uno split troppo stretto spetta a chi emette il
comando — cioè alla 4b. Stanno insieme alle altre perché sono la stessa
tabella di misure, e separarle significherebbe cercarle in due posti.

- [ ] **Passo 4: implementare `layout-geometry.ts`**

```ts
import { WORKSPACE_METRICS } from './layout-metrics.ts'
import type { WorkspaceLayout } from './layout-invariants.ts'
import type { LayoutNode } from './layout-types.ts'
import type { PaneGroupID, SplitID } from './layout-ids.ts'

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export interface DividerRects {
  splitId: SplitID
  /**
   * Asse dello split, portato qui e non lasciato da cercare.
   *
   * Chi trascina un divisore deve sapere se convertire la X o la Y del
   * puntatore in frazione, e lo sa gia guardando questo rettangolo. Senza
   * questo campo il consumatore dovrebbe risalire al nodo dello split
   * attraversando l albero, oppure indovinare l asse dalla forma della banda:
   * due modi per riscoprire una cosa che chi ha calcolato la geometria sapeva.
   */
  axis: WorkspaceSplitAxis
  /** Area afferrabile. */
  band: Rect
  /** Linea dipinta, al centro della banda. */
  hairline: Rect
}

function walk(
  node: LayoutNode,
  rect: Rect,
  groups: Map<PaneGroupID, Rect>,
  dividers: DividerRects[]
): void {
  if (node.kind === 'group') {
    groups.set(node.id, rect)
    return
  }

  const banda = WORKSPACE_METRICS.dividerBand
  const meta = banda / 2

  if (node.axis === 'horizontal') {
    const confine = rect.x + rect.w * node.fraction
    walk(node.first, { ...rect, w: rect.w * node.fraction - meta }, groups, dividers)
    walk(
      node.second,
      { ...rect, x: confine + meta, w: rect.w * (1 - node.fraction) - meta },
      groups,
      dividers
    )
    const band = { x: confine - meta, y: rect.y, w: banda, h: rect.h }
    dividers.push({
      splitId: node.id,
      band,
      hairline: {
        ...band,
        x: band.x + (banda - WORKSPACE_METRICS.dividerHairline) / 2,
        w: WORKSPACE_METRICS.dividerHairline
      }
    })
    return
  }

  const confine = rect.y + rect.h * node.fraction
  walk(node.first, { ...rect, h: rect.h * node.fraction - meta }, groups, dividers)
  walk(
    node.second,
    { ...rect, y: confine + meta, h: rect.h * (1 - node.fraction) - meta },
    groups,
    dividers
  )
  const band = { x: rect.x, y: confine - meta, w: rect.w, h: banda }
  dividers.push({
    splitId: node.id,
    band,
    hairline: {
      ...band,
      y: band.y + (banda - WORKSPACE_METRICS.dividerHairline) / 2,
      h: WORKSPACE_METRICS.dividerHairline
    }
  })
}

/**
 * Rettangolo di ogni gruppo. Sono gli stessi numeri che servono ai riquadri,
 * ai segmenti della barra superiore, ai bersagli di rilascio e alla
 * navigazione spaziale: calcolarli una volta evita quattro verita diverse.
 */
export function groupRects(layout: WorkspaceLayout, container: Rect): Map<PaneGroupID, Rect> {
  const groups = new Map<PaneGroupID, Rect>()
  walk(layout.root, container, groups, [])
  return groups
}

export function dividerRects(layout: WorkspaceLayout, container: Rect): DividerRects[] {
  const dividers: DividerRects[] = []
  walk(layout.root, container, new Map(), dividers)
  return dividers
}
```

- [ ] **Passo 5: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-geometry`
Atteso: PASS.

- [ ] **Passo 6: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: metriche misurate e geometria dei riquadri"
```

---

## Task 8: bersagli di rilascio e vicini spaziali

**File:**
- Crea: `src/shared/workspace/drop-target.ts`
- Crea: `src/shared/workspace/drop-target.test.ts`
- Crea: `src/shared/workspace/spatial-neighbors.ts`
- Crea: `src/shared/workspace/spatial-neighbors.test.ts`

**Interfacce:**
- Consuma: Task 7.
- Produce: `type DropTarget` (`tabStrip` | `center` | `edge` | `none`);
  `resolveDropTarget(point, rects, tabCounts): DropTarget`;
  `type FocusDirection = 'left' | 'right' | 'up' | 'down'`;
  `neighbor(from, direction, rects): PaneGroupID | null`.

- [ ] **Passo 1: scrivere il test che fallisce**

`drop-target.test.ts`:

```ts
import { expect, test } from 'vitest'
import { resolveDropTarget } from './drop-target.ts'
import { newPaneGroupID } from './layout-ids.ts'

const A = newPaneGroupID()
const rects = new Map([[A, { x: 0, y: 0, w: 1000, h: 600 }]])
const conteggi = new Map([[A, 3]])

test('la fascia alta e la barra dei tab, con indice dedotto dalla x', () => {
  // 32 px di altezza: sopra c e la barra, sotto il contenuto.
  const t = resolveDropTarget({ x: 500, y: 10 }, rects, conteggi)
  expect(t.kind).toBe('tabStrip')
  if (t.kind !== 'tabStrip') return
  expect(t.groupId).toBe(A)
  expect(t.insertionIndex).toBeGreaterThanOrEqual(0)
  expect(t.insertionIndex).toBeLessThanOrEqual(3)
})

test('entro il 22% da un bordo si divide da quel lato', () => {
  expect(resolveDropTarget({ x: 50, y: 300 }, rects, conteggi)).toEqual({
    kind: 'edge',
    groupId: A,
    placement: 'left'
  })
  expect(resolveDropTarget({ x: 950, y: 300 }, rects, conteggi)).toEqual({
    kind: 'edge',
    groupId: A,
    placement: 'right'
  })
  expect(resolveDropTarget({ x: 500, y: 580 }, rects, conteggi)).toEqual({
    kind: 'edge',
    groupId: A,
    placement: 'below'
  })
})

test('il centro accoglie il tab nel gruppo esistente', () => {
  expect(resolveDropTarget({ x: 500, y: 300 }, rects, conteggi)).toEqual({
    kind: 'center',
    groupId: A
  })
})

test('fuori da ogni riquadro non c e bersaglio', () => {
  expect(resolveDropTarget({ x: 2000, y: 300 }, rects, conteggi)).toEqual({ kind: 'none' })
})
```

`spatial-neighbors.test.ts`:

```ts
import { expect, test } from 'vitest'
import { neighbor } from './spatial-neighbors.ts'
import { newPaneGroupID } from './layout-ids.ts'

const A = newPaneGroupID()
const B = newPaneGroupID()
const C = newPaneGroupID()

// A | B  affiancati; C sotto B.
const rects = new Map([
  [A, { x: 0, y: 0, w: 500, h: 600 }],
  [B, { x: 500, y: 0, w: 500, h: 300 }],
  [C, { x: 500, y: 300, w: 500, h: 300 }]
])

test('trova il riquadro adiacente nella direzione richiesta', () => {
  expect(neighbor(A, 'right', rects)).toBe(B)
  expect(neighbor(B, 'left', rects)).toBe(A)
  expect(neighbor(B, 'down', rects)).toBe(C)
  expect(neighbor(C, 'up', rects)).toBe(B)
})

test('non c e vicino oltre il bordo', () => {
  expect(neighbor(A, 'left', rects)).toBeNull()
  expect(neighbor(A, 'up', rects)).toBeNull()
})

test('serve sovrapposizione sull asse perpendicolare', () => {
  // C sta a destra di A ma piu in basso: A→right deve dare B, non C.
  expect(neighbor(A, 'right', rects)).toBe(B)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- drop-target spatial-neighbors`
Atteso: FAIL, moduli inesistenti.

- [ ] **Passo 3: implementare `drop-target.ts`**

```ts
import { WORKSPACE_METRICS } from './layout-metrics.ts'
import type { Rect } from './layout-geometry.ts'
import type { PaneGroupID } from './layout-ids.ts'
import type { SplitPlacementSide } from './layout-commands.ts'

export type DropTarget =
  | { kind: 'tabStrip'; groupId: PaneGroupID; insertionIndex: number }
  | { kind: 'center'; groupId: PaneGroupID }
  | { kind: 'edge'; groupId: PaneGroupID; placement: SplitPlacementSide }
  | { kind: 'none' }

export interface Point {
  x: number
  y: number
}

function contiene(r: Rect, p: Point): boolean {
  return p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
}

/**
 * Dove finisce un tab rilasciato in `point`. Funzione pura su rettangoli: il
 * renderer la usa a ogni movimento del mouse per l'anteprima, senza
 * attraversare il confine IPC.
 *
 * `tabCounts` serve solo a limitare l'indice di inserimento al numero di tab
 * gia presenti nel gruppo bersaglio.
 */
export function resolveDropTarget(
  point: Point,
  rects: ReadonlyMap<PaneGroupID, Rect>,
  tabCounts: ReadonlyMap<PaneGroupID, number>
): DropTarget {
  for (const [groupId, rect] of rects) {
    if (!contiene(rect, point)) continue

    if (point.y < rect.y + WORKSPACE_METRICS.tabStripHeight) {
      const conteggio = tabCounts.get(groupId) ?? 0
      const larghezzaTab = conteggio === 0 ? rect.w : rect.w / conteggio
      const indice = Math.round((point.x - rect.x) / larghezzaTab)
      return {
        kind: 'tabStrip',
        groupId,
        insertionIndex: Math.max(0, Math.min(indice, conteggio))
      }
    }

    const bordoX = rect.w * WORKSPACE_METRICS.edgeBandFraction
    const bordoY = rect.h * WORKSPACE_METRICS.edgeBandFraction
    if (point.x < rect.x + bordoX) return { kind: 'edge', groupId, placement: 'left' }
    if (point.x > rect.x + rect.w - bordoX) return { kind: 'edge', groupId, placement: 'right' }
    if (point.y < rect.y + bordoY) return { kind: 'edge', groupId, placement: 'above' }
    if (point.y > rect.y + rect.h - bordoY) return { kind: 'edge', groupId, placement: 'below' }

    return { kind: 'center', groupId }
  }
  return { kind: 'none' }
}
```

- [ ] **Passo 4: implementare `spatial-neighbors.ts`**

```ts
import type { Rect } from './layout-geometry.ts'
import type { PaneGroupID } from './layout-ids.ts'

export type FocusDirection = 'left' | 'right' | 'up' | 'down'

/**
 * Il riquadro adiacente nella direzione richiesta: fra quelli che stanno da
 * quel lato E si sovrappongono sull'asse perpendicolare, il piu vicino.
 *
 * La sovrapposizione e la parte che conta: senza, un riquadro molto lontano ma
 * geometricamente "a destra" vincerebbe su quello che l'utente vede accanto.
 */
export function neighbor(
  from: PaneGroupID,
  direction: FocusDirection,
  rects: ReadonlyMap<PaneGroupID, Rect>
): PaneGroupID | null {
  const origine = rects.get(from)
  if (origine === undefined) return null

  let migliore: PaneGroupID | null = null
  let distanzaMigliore = Infinity

  for (const [id, r] of rects) {
    if (id === from) continue

    const orizzontale = direction === 'left' || direction === 'right'
    const sovrapposto = orizzontale
      ? r.y < origine.y + origine.h && origine.y < r.y + r.h
      : r.x < origine.x + origine.w && origine.x < r.x + r.w
    if (!sovrapposto) continue

    let distanza: number
    switch (direction) {
      case 'right':
        distanza = r.x - (origine.x + origine.w)
        break
      case 'left':
        distanza = origine.x - (r.x + r.w)
        break
      case 'down':
        distanza = r.y - (origine.y + origine.h)
        break
      case 'up':
        distanza = origine.y - (r.y + r.h)
        break
    }
    if (distanza < 0) continue
    if (distanza < distanzaMigliore) {
      distanzaMigliore = distanza
      migliore = id
    }
  }
  return migliore
}
```

- [ ] **Passo 5: eseguire e verificare che passi**

Comando: `pnpm test:unit -- drop-target spatial-neighbors`
Atteso: PASS.

- [ ] **Passo 6: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: risoluzione dei bersagli di rilascio e vicini spaziali"
```

---

## Task 9: serializzazione del layout

**File:**
- Crea: `src/shared/workspace/layout-codec.ts`
- Crea: `src/shared/workspace/layout-codec.test.ts`

**Interfacce:**
- Consuma: Task 1–2.
- Produce: `LAYOUT_SCHEMA_VERSION = 1`;
  `encodeLayout(layout): { payload: string; tabs: StoredTab[] }`;
  `decodeLayout(payload, tabs): { ok: true; layout } | { ok: false; reason: string }`;
  `type StoredTab` con i campi della tabella `workspaceTab`;
  `checksumOf(payload): string`.

**Contesto:** lo schema separa la struttura dai tab — `workspaceLayout.payload`
porta l'albero e la mappa gruppo → id dei tab; `workspaceTab` porta titolo,
contenuto e stato di vista. La codifica deve rispettare quella divisione.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { encodeLayout, decodeLayout, checksumOf, LAYOUT_SCHEMA_VERSION } from './layout-codec.ts'
import { makeLayout, type WorkspaceLayout } from './layout-invariants.ts'
import { newPaneGroupID, newWorkspaceTabID } from './layout-ids.ts'
import { emptyViewState } from './layout-types.ts'

// Tipo di ritorno esplicito: `@typescript-eslint/explicit-function-return-type`
// e attivo anche sui file di test, e il gate fallisce senza.
function layoutDiProva(): WorkspaceLayout {
  const id = newPaneGroupID()
  const tab = {
    id: newWorkspaceTabID(),
    title: 'Terminale 1',
    titleIsAutoNamed: true,
    content: { kind: 'terminal' as const, id: 'term-1' },
    viewState: { ...emptyViewState(), followsTail: true }
  }
  const esito = makeLayout(
    { kind: 'group', id },
    new Map([[id, { id, tabs: [tab], activeTabId: tab.id }]]),
    id
  )
  if (!esito.ok) throw new Error('layout di prova non valido')
  return esito.layout
}

test('codifica e decodifica restituiscono lo stesso layout', () => {
  const originale = layoutDiProva()
  const { payload, tabs } = encodeLayout(originale)
  const tornato = decodeLayout(payload, tabs)
  expect(tornato.ok).toBe(true)
  if (!tornato.ok) return
  expect([...tornato.layout.groups.keys()]).toEqual([...originale.groups.keys()])
  expect(tornato.layout.groups.get(originale.activeGroupId)?.tabs[0].viewState.followsTail).toBe(
    true
  )
})

test('il payload porta la versione dello schema', () => {
  const { payload } = encodeLayout(layoutDiProva())
  expect(JSON.parse(payload).schemaVersion).toBe(LAYOUT_SCHEMA_VERSION)
})

test('un payload di spazzatura viene rifiutato con un motivo, non con un errore', () => {
  const esito = decodeLayout('{non json', [])
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.reason.length).toBeGreaterThan(0)
})

test('un payload valido ma con un tab mancante viene rifiutato', () => {
  const { payload } = encodeLayout(layoutDiProva())
  const esito = decodeLayout(payload, [])
  expect(esito.ok).toBe(false)
})

// Gemello del test sul payload di spazzatura, per l'ALTRO JSON: il viewState
// dei tab e una seconda colonna, e si corrompe indipendentemente dal payload.
test('un viewState di spazzatura viene rifiutato con un motivo, non con un errore', () => {
  const { payload, tabs } = encodeLayout(layoutDiProva())
  const rovinati = tabs.map((t) => ({ ...t, viewStateJSON: '{non json' }))
  const esito = decodeLayout(payload, rovinati)
  expect(esito.ok).toBe(false)
  if (!esito.ok) expect(esito.reason).toContain('non interpretabile')
})

test('il checksum cambia se il payload cambia', () => {
  expect(checksumOf('a')).not.toBe(checksumOf('b'))
  expect(checksumOf('a')).toBe(checksumOf('a'))
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-codec`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import { z } from 'zod'
import { createHash } from 'node:crypto'
import { makeLayout, orderedGroupIds, type WorkspaceLayout } from './layout-invariants.ts'
import { LayoutNodeSchema, WorkspaceTabViewStateSchema } from './layout-types.ts'
import type { PaneGroup, WorkspaceTab } from './layout-types.ts'
import {
  PaneGroupIDSchema,
  WorkspaceTabIDSchema,
  type PaneGroupID,
  type WorkspaceTabID
} from './layout-ids.ts'

export const LAYOUT_SCHEMA_VERSION = 1

/** Una riga della tabella `workspaceTab`. */
export interface StoredTab {
  id: string
  title: string
  titleIsAutoNamed: number
  contentKind: string
  contentId: string
  viewStateJSON: string | null
  viewStateVersion: number
}

const PayloadSchema = z.object({
  schemaVersion: z.number().int(),
  root: LayoutNodeSchema,
  activeGroupId: PaneGroupIDSchema,
  groups: z.array(
    z.object({
      id: PaneGroupIDSchema,
      tabIds: z.array(WorkspaceTabIDSchema),
      activeTabId: WorkspaceTabIDSchema.nullable()
    })
  )
})

/**
 * La struttura e i tab si salvano separati, come vuole lo schema: il payload
 * porta l'albero e i soli id, la tabella `workspaceTab` porta il contenuto.
 */
export function encodeLayout(layout: WorkspaceLayout): { payload: string; tabs: StoredTab[] } {
  const gruppi = orderedGroupIds(layout).map((id) => {
    const g = layout.groups.get(id) as PaneGroup
    return { id, tabIds: g.tabs.map((t) => t.id), activeTabId: g.activeTabId }
  })
  const tabs: StoredTab[] = orderedGroupIds(layout).flatMap((id) =>
    (layout.groups.get(id)?.tabs ?? []).map((t) => ({
      id: t.id,
      title: t.title,
      titleIsAutoNamed: t.titleIsAutoNamed ? 1 : 0,
      contentKind: t.content.kind,
      contentId: t.content.id,
      viewStateJSON: JSON.stringify(t.viewState),
      viewStateVersion: LAYOUT_SCHEMA_VERSION
    }))
  )
  return {
    payload: JSON.stringify({
      schemaVersion: LAYOUT_SCHEMA_VERSION,
      root: layout.root,
      activeGroupId: layout.activeGroupId,
      groups: gruppi
    }),
    tabs
  }
}

export function decodeLayout(
  payload: string,
  tabs: readonly StoredTab[]
): { ok: true; layout: WorkspaceLayout } | { ok: false; reason: string } {
  let grezzo: unknown
  try {
    grezzo = JSON.parse(payload)
  } catch {
    return { ok: false, reason: 'payload non interpretabile come JSON' }
  }

  const interpretato = PayloadSchema.safeParse(grezzo)
  if (!interpretato.success) {
    return { ok: false, reason: `payload non conforme: ${interpretato.error.message}` }
  }
  if (interpretato.data.schemaVersion !== LAYOUT_SCHEMA_VERSION) {
    return {
      ok: false,
      reason: `versione di schema ${interpretato.data.schemaVersion}, attesa ${LAYOUT_SCHEMA_VERSION}`
    }
  }

  const perId = new Map<string, WorkspaceTab>()
  for (const riga of tabs) {
    // Anche questo `JSON.parse` va protetto, come quello del payload:
    // `decodeLayout` deve SEMPRE restituire un motivo, mai lanciare. Una
    // eccezione qui salterebbe la quarantena del Task 10 — che esiste
    // esattamente per il caso di un database con dentro spazzatura.
    let grezzoViewState: unknown
    try {
      grezzoViewState = riga.viewStateJSON === null ? {} : JSON.parse(riga.viewStateJSON)
    } catch {
      return { ok: false, reason: `stato di vista non interpretabile per il tab ${riga.id}` }
    }
    const viewState = WorkspaceTabViewStateSchema.safeParse(grezzoViewState)
    if (!viewState.success) {
      return { ok: false, reason: `stato di vista non conforme per il tab ${riga.id}` }
    }
    if (riga.contentKind !== 'terminal') {
      return { ok: false, reason: `contenuto sconosciuto: ${riga.contentKind}` }
    }
    perId.set(riga.id, {
      id: riga.id as WorkspaceTabID,
      title: riga.title,
      titleIsAutoNamed: riga.titleIsAutoNamed === 1,
      content: { kind: 'terminal', id: riga.contentId },
      viewState: viewState.data
    })
  }

  const groups = new Map<PaneGroupID, PaneGroup>()
  for (const g of interpretato.data.groups) {
    const raccolti: WorkspaceTab[] = []
    for (const id of g.tabIds) {
      const t = perId.get(id)
      // Un id nel payload senza riga corrispondente significa che le due
      // scritture sono divergenti: meglio la quarantena di un layout monco.
      if (t === undefined) return { ok: false, reason: `tab mancante: ${id}` }
      raccolti.push(t)
    }
    groups.set(g.id, { id: g.id, tabs: raccolti, activeTabId: g.activeTabId })
  }

  const esito = makeLayout(interpretato.data.root, groups, interpretato.data.activeGroupId)
  if (!esito.ok) return { ok: false, reason: `invariante violato: ${esito.error.kind}` }
  return { ok: true, layout: esito.layout }
}

export function checksumOf(payload: string): string {
  return createHash('sha256').update(payload).digest('hex')
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-codec`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/workspace/
git commit -m "feat: codifica e decodifica del layout con checksum"
```

**Nota:** `node:crypto` è ammesso qui — è un modulo di Node disponibile anche
nel renderer di Electron, e `createHash` non tocca il filesystem. Se il
bundler del renderer se ne lamentasse, sostituire con un hash in JavaScript
puro senza cambiare la firma.

---

## Task 10: store del layout con quarantena

**File:**
- Crea: `src/main/workspace/layout-store.ts`
- Crea: `src/main/workspace/layout-store.test.ts`

**Interfacce:**
- Consuma: Task 9.
- Produce:
  `loadLayout(db, worktreeId): Promise<{ layout: WorkspaceLayout; revision: number }>`
  — non fallisce mai: in caso di problema mette in quarantena e restituisce
  `emptyLayout()` con revisione 0;
  `saveLayout(db, worktreeId, layout, revision): Promise<'scritto' | 'obsoleto'>`;
  `QUARANTINE_KEEP = 10`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import type { Kysely } from 'kysely'
import { openDatabase, migrateToLatest } from '../db/database'
import type { TillerDatabase } from '../db/schema'
import { loadLayout, saveLayout } from './layout-store'
import { emptyLayout } from '../../shared/workspace/layout-invariants.ts'

/**
 * Database in memoria con il worktree padre gia' inserito.
 *
 * `workspaceLayout.worktreeId` ha una foreign key su `worktree.id`, che a sua
 * volta ne ha una su `project.id`: senza i due record padre ogni `saveLayout`
 * muore con `FOREIGN KEY constraint failed`. Migrare lo schema NON basta.
 * Stessa forma del fixture di `scrollback-store.test.ts`, che risolve lo
 * stesso problema per la sua tabella.
 */
async function db(): Promise<Kysely<TillerDatabase>> {
  const database = openDatabase(':memory:')
  await migrateToLatest(database)
  await database
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
  await database
    .insertInto('worktree')
    .values({
      id: 'wt-1',
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
  return database
}

test('un worktree senza layout salvato parte vuoto, non in errore', async () => {
  const database = await db()
  const { layout, revision } = await loadLayout(database, 'wt-1')
  expect(revision).toBe(0)
  expect(layout.groups.size).toBe(1)
})

test('un layout salvato si rilegge identico', async () => {
  const database = await db()
  const l = emptyLayout()
  expect(await saveLayout(database, 'wt-1', l, 1)).toBe('scritto')
  const { layout, revision } = await loadLayout(database, 'wt-1')
  expect(revision).toBe(1)
  expect([...layout.groups.keys()]).toEqual([...l.groups.keys()])
})

test('una scrittura con revisione non piu recente viene scartata', async () => {
  const database = await db()
  await saveLayout(database, 'wt-1', emptyLayout(), 5)
  expect(await saveLayout(database, 'wt-1', emptyLayout(), 5)).toBe('obsoleto')
  expect(await saveLayout(database, 'wt-1', emptyLayout(), 4)).toBe('obsoleto')
  expect((await loadLayout(database, 'wt-1')).revision).toBe(5)
})

test('un payload corrotto finisce in quarantena e il worktree riparte vuoto', async () => {
  const database = await db()
  await saveLayout(database, 'wt-1', emptyLayout(), 1)
  await database
    .updateTable('workspaceLayout')
    .set({ payload: '{spazzatura' })
    .where('worktreeId', '=', 'wt-1')
    .execute()

  const { layout, revision } = await loadLayout(database, 'wt-1')
  // L'app resta viva: e la differenza fra perdere una sessione e perdere tutto.
  expect(revision).toBe(0)
  expect(layout.groups.size).toBe(1)

  const quarantena = await database
    .selectFrom('workspaceLayoutQuarantine')
    .selectAll()
    .where('worktreeId', '=', 'wt-1')
    .execute()
  expect(quarantena).toHaveLength(1)
  expect(quarantena[0].reason).toContain('JSON')
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- layout-store`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { Kysely } from 'kysely'
import { randomUUID } from 'node:crypto'
import type { TillerDatabase } from '../db/schema'
import {
  encodeLayout,
  decodeLayout,
  checksumOf,
  LAYOUT_SCHEMA_VERSION
} from '../../shared/workspace/layout-codec.ts'
import { emptyLayout, type WorkspaceLayout } from '../../shared/workspace/layout-invariants.ts'

/** Quante righe di quarantena conservare per worktree. */
export const QUARANTINE_KEEP = 10

async function metti_in_quarantena(
  db: Kysely<TillerDatabase>,
  worktreeId: string,
  payload: string,
  reason: string
): Promise<void> {
  await db
    .insertInto('workspaceLayoutQuarantine')
    .values({
      id: randomUUID(),
      worktreeId,
      payload,
      reason,
      createdAt: new Date().toISOString()
    })
    .execute()

  const vecchie = await db
    .selectFrom('workspaceLayoutQuarantine')
    .select('id')
    .where('worktreeId', '=', worktreeId)
    .orderBy('createdAt', 'desc')
    .offset(QUARANTINE_KEEP)
    .limit(1000)
    .execute()
  if (vecchie.length > 0) {
    await db
      .deleteFrom('workspaceLayoutQuarantine')
      .where(
        'id',
        'in',
        vecchie.map((r) => r.id)
      )
      .execute()
  }

  await db.deleteFrom('workspaceLayout').where('worktreeId', '=', worktreeId).execute()
  await db.deleteFrom('workspaceTab').where('worktreeId', '=', worktreeId).execute()
}

/**
 * Il layout di un worktree. NON fallisce mai: un record illeggibile finisce in
 * quarantena con il motivo e il worktree riparte vuoto. Perdere una sessione e
 * accettabile; perdere l'app no.
 */
export async function loadLayout(
  db: Kysely<TillerDatabase>,
  worktreeId: string
): Promise<{ layout: WorkspaceLayout; revision: number }> {
  const record = await db
    .selectFrom('workspaceLayout')
    .selectAll()
    .where('worktreeId', '=', worktreeId)
    .executeTakeFirst()
  if (record === undefined) return { layout: emptyLayout(), revision: 0 }

  if (checksumOf(record.payload) !== record.checksum) {
    await metti_in_quarantena(db, worktreeId, record.payload, 'checksum non corrispondente')
    return { layout: emptyLayout(), revision: 0 }
  }

  const tabs = await db
    .selectFrom('workspaceTab')
    .selectAll()
    .where('worktreeId', '=', worktreeId)
    .execute()

  const esito = decodeLayout(record.payload, tabs)
  if (!esito.ok) {
    await metti_in_quarantena(db, worktreeId, record.payload, esito.reason)
    return { layout: emptyLayout(), revision: 0 }
  }
  return { layout: esito.layout, revision: record.revision }
}

/**
 * Scrive il layout. Una revisione non piu recente di quella memorizzata viene
 * scartata: le scritture non strutturali sono ritardate, quindi una puo
 * arrivare dopo una strutturale piu recente e riporterebbe indietro il layout.
 */
export async function saveLayout(
  db: Kysely<TillerDatabase>,
  worktreeId: string,
  layout: WorkspaceLayout,
  revision: number
): Promise<'scritto' | 'obsoleto'> {
  const esistente = await db
    .selectFrom('workspaceLayout')
    .select('revision')
    .where('worktreeId', '=', worktreeId)
    .executeTakeFirst()
  if (esistente !== undefined && esistente.revision >= revision) return 'obsoleto'

  const { payload, tabs } = encodeLayout(layout)
  const now = new Date().toISOString()

  await db.transaction().execute(async (tx) => {
    await tx
      .deleteFrom('workspaceLayout')
      .where('worktreeId', '=', worktreeId)
      .execute()
    await tx
      .insertInto('workspaceLayout')
      .values({
        worktreeId,
        schemaVersion: LAYOUT_SCHEMA_VERSION,
        revision,
        payload,
        checksum: checksumOf(payload),
        updatedAt: now
      })
      .execute()
    await tx.deleteFrom('workspaceTab').where('worktreeId', '=', worktreeId).execute()
    for (const t of tabs) {
      await tx
        .insertInto('workspaceTab')
        .values({ ...t, worktreeId, createdAt: now })
        .execute()
    }
  })
  return 'scritto'
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- layout-store`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/main/workspace/
git commit -m "feat: store del layout con quarantena e guardia sulla revisione"
```

---

## Task 11: politica di persistenza

**File:**
- Crea: `src/main/workspace/persistence-policy.ts`
- Crea: `src/main/workspace/persistence-policy.test.ts`

**Interfacce:**
- Consuma: Task 3 (`isStructuralCommand`), Task 10 (`saveLayout`).
- Produce: `createLayoutPersister(deps)` con
  `record(worktreeId, layout, command): void` e `flushAll(): Promise<void>`;
  `LAYOUT_DEBOUNCE_MS = 500`.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { createLayoutPersister, LAYOUT_DEBOUNCE_MS } from './persistence-policy'
import { emptyLayout } from '../../shared/workspace/layout-invariants.ts'
import { newSplitID, newWorkspaceTabID } from '../../shared/workspace/layout-ids.ts'

interface OrologioFinto {
  schedule: (fn: () => void) => number
  cancel: () => void
  esegui: () => void
}

// Tipo di ritorno esplicito, come in `layoutDiProva`: la regola ESLint vale
// anche qui.
function orologioFinto(): OrologioFinto {
  const lavori: (() => void)[] = []
  return {
    schedule: (fn: () => void) => {
      lavori.push(fn)
      return lavori.length - 1
    },
    cancel: () => {},
    esegui: () => {
      const da = [...lavori]
      lavori.length = 0
      for (const f of da) f()
    }
  }
}

test('un comando strutturale scrive subito', async () => {
  const scritture: number[] = []
  const clock = orologioFinto()
  const p = createLayoutPersister({
    save: async (_w, _l, revision) => {
      scritture.push(revision)
      return 'scritto'
    },
    schedule: clock.schedule,
    cancel: clock.cancel,
    delayMs: LAYOUT_DEBOUNCE_MS
  })
  p.record('wt-1', emptyLayout(), { kind: 'closeTab', tabId: newWorkspaceTabID() })
  await p.flushAll()
  expect(scritture).toEqual([1])
})

test('cento comandi non strutturali producono una scrittura sola', async () => {
  const scritture: number[] = []
  const clock = orologioFinto()
  const p = createLayoutPersister({
    save: async (_w, _l, revision) => {
      scritture.push(revision)
      return 'scritto'
    },
    schedule: clock.schedule,
    cancel: clock.cancel,
    delayMs: LAYOUT_DEBOUNCE_MS
  })
  for (let i = 0; i < 100; i++) {
    p.record('wt-1', emptyLayout(), {
      kind: 'setPreferredFraction',
      splitId: newSplitID(),
      fraction: 0.4
    })
  }
  clock.esegui()
  await p.flushAll()
  // Trascinare un divisore non deve produrre cento transazioni.
  expect(scritture).toHaveLength(1)
})

test('la revisione cresce a ogni scrittura, cosi la guardia dello store funziona', async () => {
  const scritture: number[] = []
  const clock = orologioFinto()
  const p = createLayoutPersister({
    save: async (_w, _l, revision) => {
      scritture.push(revision)
      return 'scritto'
    },
    schedule: clock.schedule,
    cancel: clock.cancel,
    delayMs: LAYOUT_DEBOUNCE_MS
  })
  const tabId = newWorkspaceTabID()
  p.record('wt-1', emptyLayout(), { kind: 'closeTab', tabId })
  p.record('wt-1', emptyLayout(), { kind: 'closeTab', tabId })
  await p.flushAll()
  expect(scritture).toEqual([1, 2])
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- persistence-policy`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import { isStructuralCommand } from '../../shared/workspace/layout-commands.ts'
import type { WorkspaceLayoutCommand } from '../../shared/workspace/layout-commands.ts'
import type { WorkspaceLayout } from '../../shared/workspace/layout-invariants.ts'

/** Attesa prima di scrivere un cambiamento non strutturale. */
export const LAYOUT_DEBOUNCE_MS = 500

export interface PersisterDeps {
  save(
    worktreeId: string,
    layout: WorkspaceLayout,
    revision: number
  ): Promise<'scritto' | 'obsoleto'>
  schedule(fn: () => void, ms: number): unknown
  cancel(handle: unknown): void
  delayMs?: number
}

/**
 * Traduce la classificazione dei comandi in politica di scrittura.
 *
 * Strutturale significa "cambia cosa esiste": si scrive subito, perche
 * perderlo a un crash costa lavoro. Il resto cambia come le cose appaiono, e
 * trascinare un divisore ne produce centinaia al secondo: si accumula e si
 * scrive l'ultimo stato, non tutti.
 */
export function createLayoutPersister(deps: PersisterDeps): {
  record(worktreeId: string, layout: WorkspaceLayout, command: WorkspaceLayoutCommand): void
  flushAll(): Promise<void>
} {
  const revisioni = new Map<string, number>()
  const inAttesa = new Map<string, { layout: WorkspaceLayout; handle: unknown }>()
  const scritture: Promise<unknown>[] = []

  const prossimaRevisione = (worktreeId: string): number => {
    const r = (revisioni.get(worktreeId) ?? 0) + 1
    revisioni.set(worktreeId, r)
    return r
  }

  const scrivi = (worktreeId: string, layout: WorkspaceLayout): void => {
    scritture.push(deps.save(worktreeId, layout, prossimaRevisione(worktreeId)))
  }

  return {
    record(worktreeId, layout, command): void {
      const pendente = inAttesa.get(worktreeId)

      if (isStructuralCommand(command)) {
        // Una scrittura strutturale ingloba i cambiamenti accumulati: il
        // layout che si salva e comunque quello corrente.
        if (pendente !== undefined) {
          deps.cancel(pendente.handle)
          inAttesa.delete(worktreeId)
        }
        scrivi(worktreeId, layout)
        return
      }

      if (pendente !== undefined) deps.cancel(pendente.handle)
      const handle = deps.schedule(() => {
        const corrente = inAttesa.get(worktreeId)
        inAttesa.delete(worktreeId)
        if (corrente !== undefined) scrivi(worktreeId, corrente.layout)
      }, deps.delayMs ?? LAYOUT_DEBOUNCE_MS)
      inAttesa.set(worktreeId, { layout, handle })
    },

    async flushAll(): Promise<void> {
      for (const [worktreeId, pendente] of [...inAttesa]) {
        deps.cancel(pendente.handle)
        inAttesa.delete(worktreeId)
        scrivi(worktreeId, pendente.layout)
      }
      await Promise.all(scritture)
    }
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- persistence-policy`
Atteso: PASS.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add src/main/workspace/
git commit -m "feat: politica di persistenza guidata dalla classe del comando"
```

---

## Task 12: protocollo e dispatcher

**File:**
- Modifica: `src/shared/protocol.ts`
- Modifica: `src/main/control/dispatch.ts`
- Modifica: `src/main/control/dispatch.test.ts`
- Modifica: `src/renderer/src/lib/app-model.svelte.ts`

**Interfacce:**
- Consuma: Task 3 (schemi dei comandi), Task 10–11.
- Produce: `ControlRequest` guadagna `workspace.get` e `workspace.apply`;
  `StateEvent` guadagna `workspace.layout`;
  `DispatchDeps.workspace` **obbligatorio**, con
  `load(worktreeId)`, `apply(worktreeId, command)`.

**Nota sul taglio:** `ControlRequest` e `StateEvent` sono unioni discriminate
con `switch` esaustivi nel dispatcher e nel renderer. Protocollo e rami vanno
nello stesso task, o il compilatore rifiuta il file.

**Attenzione ai livelli:** `src/shared/protocol.ts` importerà da
`src/shared/workspace/layout-commands.ts`. Gli import relativi dentro
`src/shared/` **devono portare l'estensione `.ts`** — vedi i vincoli globali.

- [ ] **Passo 1: scrivere il test che fallisce**

In `dispatch.test.ts`:

```ts
test('workspace.get su un worktree nuovo restituisce un layout vuoto', async () => {
  const { dispatch } = await makeDispatcher()
  const r = await dispatch({ id: 'r1', method: 'workspace.get', params: { worktreeId: 'wt-1' } })
  expect(r.ok).toBe(true)
  const risultato = (r as { result: { layout: { groups: unknown[] } } }).result
  expect(risultato.layout.groups).toHaveLength(1)
})

test('workspace.apply inserisce un tab e lo si rilegge', async () => {
  const { dispatch } = await makeDispatcher()
  const iniziale = await dispatch({
    id: 'r1',
    method: 'workspace.get',
    params: { worktreeId: 'wt-1' }
  })
  const groupId = (iniziale as { result: { layout: { activeGroupId: string } } }).result.layout
    .activeGroupId

  const applicato = await dispatch({
    id: 'r2',
    method: 'workspace.apply',
    params: {
      worktreeId: 'wt-1',
      command: {
        kind: 'insertTab',
        tab: {
          id: crypto.randomUUID(),
          title: 'Terminale 1',
          titleIsAutoNamed: true,
          content: { kind: 'terminal', id: 'term-1' },
          viewState: {
            documentCaretOffset: null,
            documentSelectionLength: null,
            documentScrollAnchor: null,
            documentFoldedRanges: [],
            editorMode: null,
            chatComposerDraft: null,
            chatAttachmentReferences: [],
            chatTranscriptAnchor: null,
            followsTail: false,
            terminalViewportAnchor: null
          }
        },
        into: groupId,
        index: null,
        activate: true
      }
    }
  })
  expect(applicato.ok).toBe(true)

  const riletto = await dispatch({
    id: 'r3',
    method: 'workspace.get',
    params: { worktreeId: 'wt-1' }
  })
  const groups = (riletto as { result: { layout: { groups: { tabs: unknown[] }[] } } }).result
    .layout.groups
  expect(groups[0].tabs).toHaveLength(1)
})

test('un comando che viola un invariante risponde con errore e non cambia il layout', async () => {
  const { dispatch } = await makeDispatcher()
  const r = await dispatch({
    id: 'r1',
    method: 'workspace.apply',
    params: {
      worktreeId: 'wt-1',
      command: { kind: 'activateTab', tabId: crypto.randomUUID() }
    }
  })
  expect(r.ok).toBe(false)
  if (!r.ok) expect(r.error).toContain('unknownTab')
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- dispatch`
Atteso: FAIL — i metodi non esistono.

- [ ] **Passo 3: estendere il protocollo**

In `src/shared/protocol.ts`:

```ts
import { WorkspaceLayoutCommandSchema } from './workspace/layout-commands.ts'
```

Nell'unione `ControlRequest`:

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('workspace.get'),
    params: z.object({ worktreeId: z.string().min(1) })
  }),
  z.object({
    id: z.string().min(1),
    method: z.literal('workspace.apply'),
    params: z.object({
      worktreeId: z.string().min(1),
      command: WorkspaceLayoutCommandSchema
    })
  }),
```

Nell'unione `StateEvent`:

```ts
  z.object({
    type: z.literal('workspace.layout'),
    worktreeId: z.string().min(1),
    // Serializzato: il renderer ricostruisce la mappa dei gruppi.
    layout: z.unknown(),
    delta: z.unknown()
  })
```

- [ ] **Passo 4: cablare il dispatcher**

Aggiungere a `DispatchDeps` il campo **obbligatorio**:

```ts
  workspace: {
    load(worktreeId: string): Promise<WorkspaceLayout>
    apply(
      worktreeId: string,
      command: WorkspaceLayoutCommand
    ): Promise<{ ok: true; layout: WorkspaceLayout; delta: LayoutDelta } | { ok: false; error: string }>
  }
```

e i due rami dello `switch`:

```ts
        case 'workspace.get': {
          const layout = await deps.workspace.load(request.params.worktreeId)
          return { id: request.id, ok: true, result: { layout: serializza(layout) } }
        }

        case 'workspace.apply': {
          const esito = await deps.workspace.apply(
            request.params.worktreeId,
            request.params.command
          )
          if (!esito.ok) return { id: request.id, ok: false, error: esito.error }
          return {
            id: request.id,
            ok: true,
            result: { layout: serializza(esito.layout), delta: esito.delta }
          }
        }
```

con, in fondo al file:

```ts
/**
 * `WorkspaceLayout` porta una `Map`, che JSON non sa rappresentare: sul filo
 * viaggia come lista ordinata di gruppi.
 */
function serializza(layout: WorkspaceLayout): unknown {
  return {
    root: layout.root,
    activeGroupId: layout.activeGroupId,
    groups: orderedGroupIds(layout).map((id) => layout.groups.get(id))
  }
}
```

- [ ] **Passo 5: aggiornare finti e renderer**

Nei **due** finti di `dispatch.test.ts` aggiungere la dipendenza `workspace`.
Non è un finto vuoto: usa il riduttore vero su una mappa in memoria, così i
test del dispatcher provano il percorso reale e non una simulazione.

```ts
function fakeWorkspace(): DispatchDeps['workspace'] {
  const layouts = new Map<string, WorkspaceLayout>()
  const corrente = (worktreeId: string): WorkspaceLayout => {
    const gia = layouts.get(worktreeId)
    if (gia !== undefined) return gia
    const nuovo = emptyLayout()
    layouts.set(worktreeId, nuovo)
    return nuovo
  }
  return {
    load: async (worktreeId) => corrente(worktreeId),
    apply: async (worktreeId, command) => {
      const esito = applyLayout(command, corrente(worktreeId))
      if (!esito.ok) return { ok: false, error: esito.error.kind }
      layouts.set(worktreeId, esito.layout)
      return { ok: true, layout: esito.layout, delta: esito.delta }
    }
  }
}
```

con gli import `emptyLayout`, `type WorkspaceLayout` da
`../../shared/workspace/layout-invariants.ts` e
`apply as applyLayout` da `../../shared/workspace/layout-engine.ts`. Aggiungere
`workspace: fakeWorkspace()` alle dipendenze di entrambi i finti.

In `src/renderer/src/lib/app-model.svelte.ts`, accanto agli altri rami:

```ts
      case 'workspace.layout':
        // Stile immutabile e `return`, come gli altri rami di questo switch.
        this.layouts = new Map(this.layouts).set(event.worktreeId, event.layout)
        return
```

con il campo `layouts = $state(new Map<string, unknown>())` accanto a `panes`.
Il tipo resta `unknown`: in Fase 4a nessuna vista lo legge ancora, e tipizzarlo
qui significherebbe decidere in anticipo la forma che serve alla 4b.

- [ ] **Passo 6: gate e commit**

```bash
bash scripts/ci.sh
git add src/shared/protocol.ts src/main/control/ src/renderer/src/lib/app-model.svelte.ts
git commit -m "feat: metodi workspace.get e workspace.apply nel protocollo"
```

---

## Task 13: `tillerctl workspace` e cablaggio nel main

**File:**
- Modifica: `cli/args.ts`
- Modifica: `cli/args.test.ts`
- Modifica: `src/main/index.ts`

**Interfacce:**
- Consuma: Task 12.
- Produce: comandi CLI `workspace-get --worktree <id>` e
  `workspace-apply --worktree <id> --command <json>`; la dipendenza
  `workspace` del dispatcher costruita sul database reale.

- [ ] **Passo 1: scrivere il test che fallisce**

In `cli/args.test.ts`:

```ts
test('workspace-get costruisce la richiesta', () => {
  expect(parseCommand(['workspace-get', '--worktree', 'wt-1'])).toEqual({
    method: 'workspace.get',
    params: { worktreeId: 'wt-1' }
  })
})

test('workspace-apply interpreta il comando come JSON', () => {
  const comando = '{"kind":"activateGroup","groupId":"g-1"}'
  expect(parseCommand(['workspace-apply', '--worktree', 'wt-1', '--command', comando])).toEqual({
    method: 'workspace.apply',
    params: { worktreeId: 'wt-1', command: { kind: 'activateGroup', groupId: 'g-1' } }
  })
})

test('un comando che non e JSON valido e un errore chiaro', () => {
  expect(() =>
    parseCommand(['workspace-apply', '--worktree', 'wt-1', '--command', '{rotto'])
  ).toThrow('--command deve essere JSON valido')
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- args`
Atteso: FAIL.

- [ ] **Passo 3: aggiungere i comandi CLI**

Nella tabella `COMMANDS` di `cli/args.ts`:

```ts
  'workspace-get': {
    method: 'workspace.get',
    options: { worktree: { type: 'string' } },
    required: ['worktree'],
    build: (v: Record<string, string>) => ({ worktreeId: v.worktree })
  },
  'workspace-apply': {
    method: 'workspace.apply',
    options: { worktree: { type: 'string' }, command: { type: 'string' } },
    required: ['worktree', 'command'],
    build: (v: Record<string, string>) => ({
      worktreeId: v.worktree,
      command: jsonValido(v.command, 'command')
    })
  },
```

e la funzione di supporto, accanto a `intPositivo`:

```ts
function jsonValido(raw: string, nome: string): unknown {
  try {
    return JSON.parse(raw)
  } catch {
    throw new Error(`--${nome} deve essere JSON valido`)
  }
}
```

Aggiornare anche `HELP_TEXT` con le due righe nuove.

- [ ] **Passo 4: cablare il main**

In `src/main/index.ts`, accanto alle altre istanze:

```ts
import { loadLayout, saveLayout } from './workspace/layout-store'
import { createLayoutPersister } from './workspace/persistence-policy'
import { apply as applyLayout } from '../shared/workspace/layout-engine.ts'
import type { WorkspaceLayout } from '../shared/workspace/layout-invariants.ts'

/** Layout vivi per worktree: il database e la copia durevole, questa e quella corrente. */
const layouts = new Map<string, WorkspaceLayout>()
const persister = createLayoutPersister({
  save: (worktreeId, layout, revision) => saveLayout(db, worktreeId, layout, revision),
  schedule: (fn, ms) => setTimeout(fn, ms),
  cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>)
})
```

e la dipendenza del dispatcher:

```ts
    workspace: {
      load: async (worktreeId) => {
        const gia = layouts.get(worktreeId)
        if (gia !== undefined) return gia
        const { layout } = await loadLayout(db, worktreeId)
        layouts.set(worktreeId, layout)
        return layout
      },
      apply: async (worktreeId, command) => {
        let corrente = layouts.get(worktreeId)
        if (corrente === undefined) {
          corrente = (await loadLayout(db, worktreeId)).layout
          layouts.set(worktreeId, corrente)
        }
        const esito = applyLayout(command, corrente)
        if (!esito.ok) return { ok: false, error: esito.error.kind }
        layouts.set(worktreeId, esito.layout)
        persister.record(worktreeId, esito.layout, command)
        return { ok: true, layout: esito.layout, delta: esito.delta }
      }
    }
```

Nel gestore di `before-quit`, prima di `closeDatabase(db)`, aggiungere
`await persister.flushAll()`: i cambiamenti ritardati non devono morire con
l'app.

- [ ] **Passo 5: gate e commit**

```bash
bash scripts/ci.sh
git add cli/ src/main/index.ts
git commit -m "feat: comandi workspace in tillerctl e cablaggio del layout nel main"
```

---

## Task 14: criteri end-to-end

**File:**
- Crea: `e2e/fase-4a-workspace.spec.ts`

**Interfacce:**
- Consuma: Task 13.
- Produce: niente verso altri task.

- [ ] **Passo 1: scrivere i quattro criteri**

Sullo stampo di `e2e/fase-2-terminale.spec.ts` (stesso `beforeEach`/`afterEach`
con socket e database in cartella temporanea, stessa `worktreeReale()`):

```ts
async function layoutDi(worktreeId: string): Promise<{
  root: { kind: string }
  activeGroupId: string
  groups: { id: string; tabs: { id: string }[] }[]
}> {
  const r = JSON.parse(await tillerctl(['workspace-get', '--worktree', worktreeId]))
  return r.layout
}

async function applica(worktreeId: string, command: unknown): Promise<string> {
  return tillerctl(['workspace-apply', '--worktree', worktreeId, '--command', JSON.stringify(command)])
}

const TAB_VUOTO = {
  documentCaretOffset: null,
  documentSelectionLength: null,
  documentScrollAnchor: null,
  documentFoldedRanges: [],
  editorMode: null,
  chatComposerDraft: null,
  chatAttachmentReferences: [],
  chatTranscriptAnchor: null,
  followsTail: false,
  terminalViewportAnchor: null
}

test('criterio 1: uno split sopravvive al riavvio', async () => {
  // Arrange
  const { worktreeId } = await worktreeReale()
  const iniziale = await layoutDi(worktreeId)
  const gruppo = iniziale.activeGroupId

  await applica(worktreeId, {
    kind: 'insertTab',
    tab: { id: crypto.randomUUID(), title: 'T1', titleIsAutoNamed: true,
           content: { kind: 'terminal', id: 'term-1' }, viewState: TAB_VUOTO },
    into: gruppo, index: null, activate: true
  })

  // Act
  await applica(worktreeId, {
    kind: 'splitGroup',
    anchor: gruppo,
    placement: 'right',
    newGroup: crypto.randomUUID(),
    newSplit: crypto.randomUUID(),
    content: { kind: 'newTab',
               tab: { id: crypto.randomUUID(), title: 'T2', titleIsAutoNamed: true,
                      content: { kind: 'terminal', id: 'term-2' }, viewState: TAB_VUOTO } }
  })
  await app.close()
  await launch()

  // Assert — l'albero e ancora diviso, con i due tab al loro posto.
  const dopo = await layoutDi(worktreeId)
  expect(dopo.root.kind).toBe('split')
  expect(dopo.groups.flatMap((g) => g.tabs)).toHaveLength(2)
})

test('criterio 2: cento trascinamenti del divisore non fanno cento scritture', async () => {
  // Arrange — un layout con uno split, cosi la frazione ha senso.
  const { worktreeId } = await worktreeReale()
  const gruppo = (await layoutDi(worktreeId)).activeGroupId
  await applica(worktreeId, {
    kind: 'insertTab',
    tab: { id: crypto.randomUUID(), title: 'T1', titleIsAutoNamed: true,
           content: { kind: 'terminal', id: 'term-1' }, viewState: TAB_VUOTO },
    into: gruppo, index: null, activate: true
  })
  const nuovoSplit = crypto.randomUUID()
  await applica(worktreeId, {
    kind: 'splitGroup', anchor: gruppo, placement: 'right',
    newGroup: crypto.randomUUID(), newSplit: nuovoSplit,
    content: { kind: 'newTab',
               tab: { id: crypto.randomUUID(), title: 'T2', titleIsAutoNamed: true,
                      content: { kind: 'terminal', id: 'term-2' }, viewState: TAB_VUOTO } }
  })

  const revisione = async (): Promise<number> => {
    const db = new Database(dbPath, { readonly: true })
    const r = db.prepare('SELECT revision FROM workspaceLayout WHERE worktreeId = ?').get(worktreeId)
    db.close()
    return (r as { revision: number }).revision
  }
  const prima = await revisione()

  // Act — cento frazioni diverse, come un trascinamento reale.
  for (let i = 1; i <= 100; i++) {
    await applica(worktreeId, {
      kind: 'setPreferredFraction', splitId: nuovoSplit, fraction: 0.2 + i * 0.005
    })
  }
  await new Promise((r) => setTimeout(r, 1500))

  // Assert — le scritture si contano dalla revisione, che cresce di uno per
  // scrittura. Cento transazioni per un trascinamento sarebbero un difetto.
  expect((await revisione()) - prima).toBeLessThan(10)
})

test('criterio 3: un layout corrotto finisce in quarantena e l app resta viva', async () => {
  // Arrange
  const { worktreeId } = await worktreeReale()
  const gruppo = (await layoutDi(worktreeId)).activeGroupId
  await applica(worktreeId, {
    kind: 'insertTab',
    tab: { id: crypto.randomUUID(), title: 'T1', titleIsAutoNamed: true,
           content: { kind: 'terminal', id: 'term-1' }, viewState: TAB_VUOTO },
    into: gruppo, index: null, activate: true
  })
  await app.close()

  // Act — spazzatura al posto del payload, poi riavvio.
  const db = new Database(dbPath)
  db.prepare('UPDATE workspaceLayout SET payload = ? WHERE worktreeId = ?')
    .run('{spazzatura', worktreeId)
  db.close()
  await launch()

  // Assert — l'app c'e, il worktree riparte vuoto, la riga e in quarantena.
  const dopo = await layoutDi(worktreeId)
  expect(dopo.groups.flatMap((g) => g.tabs)).toHaveLength(0)

  const verifica = new Database(dbPath, { readonly: true })
  const righe = verifica
    .prepare('SELECT reason FROM workspaceLayoutQuarantine WHERE worktreeId = ?')
    .all(worktreeId)
  verifica.close()
  expect(righe).toHaveLength(1)
})

test('criterio 4: un comando che viola un invariante non tocca il layout', async () => {
  // Arrange
  const { worktreeId } = await worktreeReale()
  const gruppo = (await layoutDi(worktreeId)).activeGroupId
  await applica(worktreeId, {
    kind: 'insertTab',
    tab: { id: crypto.randomUUID(), title: 'T1', titleIsAutoNamed: true,
           content: { kind: 'terminal', id: 'term-1' }, viewState: TAB_VUOTO },
    into: gruppo, index: null, activate: true
  })
  const prima = JSON.stringify(await layoutDi(worktreeId))

  // Act — attivare un tab che non esiste.
  await expect(
    applica(worktreeId, { kind: 'activateTab', tabId: crypto.randomUUID() })
  ).rejects.toThrow()

  // Assert
  expect(JSON.stringify(await layoutDi(worktreeId))).toBe(prima)
})
```

Aggiungere in testa al file
`import Database from 'better-sqlite3'` (già dipendenza del repo).

- [ ] **Passo 2: eseguire gli e2e**

Comando: `pnpm exec electron-vite build && pnpm test:e2e`
Atteso: 17 test passati (13 delle fasi precedenti + 4 nuovi).

- [ ] **Passo 3: validare per mutazione il criterio 2**

In `src/main/workspace/persistence-policy.ts`, rendere temporaneamente
`isStructuralCommand` sempre vero:

```ts
      if (true) {   // MUTAZIONE TEMPORANEA
```

Comando: `pnpm exec electron-vite build && pnpm test:e2e -- --grep "criterio 2"`
Atteso: **FAIL** (cento scritture). Se passa, il criterio è vacuo e va
riscritto.

Poi annullare la mutazione:

```bash
git checkout src/main/workspace/persistence-policy.ts
pnpm exec electron-vite build && pnpm test:e2e
```

- [ ] **Passo 4: gate e commit**

```bash
bash scripts/ci.sh
git add e2e/
git commit -m "test: quattro criteri e2e del motore del workspace"
```

---

## Checklist finale

- [ ] `bash scripts/ci.sh` stampa `CI OK`
- [ ] Nessun file di `src/shared/` importa da `src/main/`
- [ ] Ogni import relativo dentro `src/shared/` porta l'estensione `.ts`
- [ ] `node cli/tillerctl.ts help` funziona (prova che la CLI non si è rotta)
- [ ] Il criterio 2 è stato validato per mutazione e sa fallire
