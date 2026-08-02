# Fase 4b — Le viste del workspace: piano di implementazione

> **Per esecutori agentici:** SOTTO-SKILL RICHIESTA: usare
> superpowers:subagent-driven-development (consigliato) o
> superpowers:executing-plans per implementare un task alla volta.

**Obiettivo:** disegnare il modello di layout prodotto dalla Fase 4a —
albero della sidebar, barra superiore con i tab, riquadri, barra inferiore,
pannello agenti, stato vuoto, temi.

**Architettura:** tre viste sullo stesso modello. La geometria arriva dalla 4a
(`groupRects`, `dividerRects`) e **non viene mai ricalcolata**: nessun
`getBoundingClientRect` per sapere dove si trova qualcosa.

**Stack:** Svelte 5 (rune), TypeScript, `paneforge`, `svelte-dnd-action`,
vitest, Playwright.

**Repo:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
`~/Desktop/Progetti/tiller` è **sola lettura**.

**Spec:** `docs/2026-08-01-electron-fase-4b-viste-workspace-design.md`.

**Dipende da:** Fase 4a completa (modello, geometria, protocollo
`workspace.get` / `workspace.apply` / evento `workspace.layout`) e Fase 3
(`AgentStatus`, `PaneSnapshot.agentId`, `PaneSnapshot.status`).

## Vincoli globali

- **`src/shared/` non importa mai da `src/main/`.** È il livello comune a main,
  renderer e CLI. L'inversione è già costata un difetto (commit `c78052f`).
- **Import relativi in `src/shared/` con estensione `.ts` esplicita.** Quel
  livello lo carica anche `tillerctl` sotto Node senza bundler.
- **Il gate è `bash scripts/ci.sh`, e stampa `CI OK`.** Non sostituirlo con i
  singoli sotto-comandi: esiste una classe di difetti che passa vitest, tsc ed
  eslint e rompe `tillerctl` a runtime, e solo gli e2e la prende.
- **Niente libreria di test per componenti.** Il repo non ne ha una e non se ne
  aggiunge: la logica sta in funzioni pure testate con vitest, l'interazione si
  verifica con Playwright, come già fa `Terminal.svelte` dalla Fase 2.
- **Niente confronto di screenshot.** I diff di immagine falliscono per
  antialiasing e insegnano a ignorare il gate.
- **Svelte 5 con le rune** (`$state`, `$derived`, `$props`), come il codice
  esistente in `src/renderer/src/lib/app-model.svelte.ts`.
- **Nei file `.svelte` e `.svelte.ts`, collezioni da `svelte/reactivity`**:
  `SvelteMap`, `SvelteSet`, `SvelteDate`, mai le native. `$state` rende
  osservabile il RIFERIMENTO, non il contenuto: una `Map` nativa dentro
  `$state` non notifica il cambio di una chiave, e la vista che la legge non si
  aggiorna. La regola `svelte/prefer-svelte-reactivity` fa fallire il gate.
  Nei file `.ts` e `.test.ts` normali restano le collezioni native.
- **Stringhe dell'interfaccia in inglese**, anche quando il piano e i commenti
  sono in italiano.
- Baseline all'inizio della fase: gate verde con la Fase 4a completa.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/shared/status-rollup.ts` | riduzione di molti valori a uno, con precedenza dichiarata |
| `src/renderer/src/lib/workspace/TabSegment.svelte` | un segmento = un gruppo |
| `src/renderer/src/lib/workspace/TopBar.svelte` | barra titolo + segmenti a `y = 0` |
| `src/renderer/src/lib/workspace/PaneGrid.svelte` | riquadri assoluti + divisori |
| `src/renderer/src/lib/workspace/SidebarTree.svelte` | albero a tre livelli |
| `src/renderer/src/lib/workspace/TreeRow.svelte` | una riga dell'albero |
| `src/renderer/src/lib/workspace/StatusBar.svelte` | striscia a tutta larghezza |
| `src/renderer/src/lib/workspace/QuotaBar.svelte` | icona + barra + percentuale |
| `src/renderer/src/lib/workspace/AgentsPanel.svelte` | agenti di tutti i worktree |
| `src/renderer/src/lib/workspace/EmptyState.svelte` | stato vuoto della finestra |
| `src/renderer/src/lib/theme.svelte.ts` | tema corrente e token |
| `e2e/fase-4b-viste.spec.ts` | i sei criteri |

Modificati: `src/main/index.ts` (chrome della finestra, titolo, tema nativo),
`src/renderer/src/App.svelte`, `src/renderer/src/assets/` (token CSS).

---

## Task 1: chrome della finestra e titolo

**File:**
- Modifica: `src/main/index.ts`
- Crea: `src/shared/window-title.ts`
- Crea: `src/shared/window-title.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `windowTitle(progetto: string | null, worktree: string | null): string`.

**Contesto:** `titleBarStyle: 'hiddenInset'` libera la riga superiore perché ci
vivano i tab (V2). I semafori restano ma vanno riposizionati: con una barra da
34 px, la posizione predefinita li lascia troppo in alto.

- [ ] **Passo 1: scrivere il test del titolo**

`src/shared/window-title.test.ts`:

```ts
import { test, expect } from 'vitest'
import { windowTitle } from './window-title.ts'

test('progetto e worktree compaiono separati da un punto medio', () => {
  expect(windowTitle('tiller', 'main')).toBe('tiller · main')
})

test('senza selezione ripiega sul nome dell applicazione', () => {
  expect(windowTitle(null, null)).toBe('Tiller')
})

test('progetto senza worktree mostra il solo progetto', () => {
  // Succede fra la selezione del progetto e quella del worktree: un titolo
  // `tiller · ` con il separatore penzolante sembra un difetto di rendering.
  expect(windowTitle('tiller', null)).toBe('tiller')
})
```

- [ ] **Passo 2: eseguire il test e verificare che fallisca**

Comando: `pnpm test:unit -- window-title`
Atteso: FAIL, `Cannot find module './window-title.ts'`.

- [ ] **Passo 3: implementare**

`src/shared/window-title.ts`:

```ts
/** Titolo della finestra: `<progetto> · <worktree>`, con ripieghi (P3). */
export function windowTitle(progetto: string | null, worktree: string | null): string {
  if (progetto === null) return 'Tiller'
  if (worktree === null) return progetto
  return `${progetto} · ${worktree}`
}
```

- [ ] **Passo 4: eseguire il test e verificare che passi**

Comando: `pnpm test:unit -- window-title`
Atteso: PASS, 3 test.

- [ ] **Passo 5: applicare il chrome in `src/main/index.ts`**

Nelle opzioni di `new BrowserWindow(...)` aggiungere:

```ts
    titleBarStyle: 'hiddenInset',
    // Con una barra da 34px la posizione predefinita dei semafori li lascia
    // troppo in alto. 12/10 li centra verticalmente su quella riga.
    trafficLightPosition: { x: 12, y: 10 },
```

- [ ] **Passo 6: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add src/shared/window-title.ts src/shared/window-title.test.ts src/main/index.ts
git commit -m "feat: barra del titolo integrata e titolo finestra con contesto"
```

---

## Task 2: riduzione di molti valori a uno

**File:**
- Crea: `src/shared/status-rollup.ts`
- Crea: `src/shared/status-rollup.test.ts`

**Interfacce:**
- Consuma: `AgentStatus` da `src/shared/agent-status.ts` (Fase 3).
- Produce: `rollupStatus(stati: readonly (AgentStatus | null)[]): AgentStatus | null`;
  `type QuotaWindow = { label: string; used: number; resetsAt: number | null }`;
  `mostConstrained(finestre: readonly QuotaWindow[]): QuotaWindow | null`.

**Contesto:** due riduzioni con la stessa forma — molti valori, un indicatore,
una precedenza dichiarata. Stanno nello stesso file perché tenerle separate le
farebbe divergere in silenzio.

Nota di provenienza: `highestPriority` / `statusForPanes` erano state scritte
nel piano della Fase 3 e cancellate durante l'auto-revisione, perché in Fase 3
nessuno le chiamava. Tornano qui con l'ordinamento dettato dal consumatore reale.

- [ ] **Passo 1: scrivere i test**

`src/shared/status-rollup.test.ts`:

```ts
import { test, expect } from 'vitest'
import { rollupStatus, mostConstrained } from './status-rollup.ts'

test('needs-input batte running', () => {
  expect(rollupStatus(['running', 'needs-input'])).toBe('needs-input')
})

test('needs-input batte error', () => {
  // `error` e' gia' concluso; `needs-input` e' fermo sull utente adesso.
  expect(rollupStatus(['error', 'needs-input'])).toBe('needs-input')
})

test('error batte running', () => {
  expect(rollupStatus(['running', 'error'])).toBe('error')
})

test('running batte done', () => {
  expect(rollupStatus(['done', 'running'])).toBe('running')
})

test('nessuno stato riduce a null', () => {
  expect(rollupStatus([])).toBe(null)
  expect(rollupStatus([null, null])).toBe(null)
})

test('i null non oscurano uno stato presente', () => {
  expect(rollupStatus([null, 'running', null])).toBe('running')
})

test('la quota mostrata e quella piu vicina all esaurimento', () => {
  const finestre = [
    { label: '5h', used: 0.37, resetsAt: null },
    { label: 'wk', used: 0.83, resetsAt: null }
  ]
  expect(mostConstrained(finestre)?.label).toBe('wk')
})

test('senza finestre non c e quota da mostrare', () => {
  // Non una barra a zero: si leggerebbe come "quota esaurita", il contrario.
  expect(mostConstrained([])).toBe(null)
})
```

- [ ] **Passo 2: eseguire i test e verificare che falliscano**

Comando: `pnpm test:unit -- status-rollup`
Atteso: FAIL, `Cannot find module './status-rollup.ts'`.

- [ ] **Passo 3: implementare**

`src/shared/status-rollup.ts`:

```ts
import type { AgentStatus } from './agent-status.ts'

/**
 * Precedenza degli stati, dalla piu urgente. `needs-input` batte `error`
 * perche e' l unico stato in cui il lavoro e' fermo sull utente ADESSO;
 * l errore e' gia' concluso e resta li.
 */
const PRECEDENZA: readonly AgentStatus[] = ['needs-input', 'error', 'running', 'done']

/** Stato da mostrare su una riga che ne raggruppa molti (P6). */
export function rollupStatus(stati: readonly (AgentStatus | null)[]): AgentStatus | null {
  for (const candidato of PRECEDENZA) {
    if (stati.includes(candidato)) return candidato
  }
  return null
}

export interface QuotaWindow {
  label: string
  /** Frazione consumata, da 0 a 1. */
  used: number
  /** Epoch ms del ripristino, o null se sconosciuto. */
  resetsAt: number | null
}

/**
 * La finestra temporale piu vicina all esaurimento (V4).
 *
 * Una barra di avanzamento comunica una cosa sola: quanto manca alla fine.
 * Mostrarne due affiancate obbliga chi guarda a calcolare il massimo, che e'
 * l unica operazione che poi compie.
 */
export function mostConstrained(finestre: readonly QuotaWindow[]): QuotaWindow | null {
  let peggiore: QuotaWindow | null = null
  for (const finestra of finestre) {
    if (peggiore === null || finestra.used > peggiore.used) peggiore = finestra
  }
  return peggiore
}
```

- [ ] **Passo 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- status-rollup`
Atteso: PASS, 8 test.

- [ ] **Passo 5: commit**

```bash
git add src/shared/status-rollup.ts src/shared/status-rollup.test.ts
git commit -m "feat: riduzione degli stati agente e delle finestre di quota"
```

---

## Task 3: segmento di tab e regioni di trascinamento

**File:**
- Crea: `src/renderer/src/lib/workspace/TabSegment.svelte`
- Crea: `src/renderer/src/lib/workspace/tab-segment-layout.ts`
- Crea: `src/renderer/src/lib/workspace/tab-segment-layout.test.ts`

**Interfacce:**
- Consuma: `Rect` e `PaneGroupID` da `src/shared/workspace/`; `WorkspaceTab` da
  `layout-types.ts`.
- Produce: componente `TabSegment` con props
  `{ groupId: PaneGroupID; rect: Rect; tabs: WorkspaceTab[]; activeTabId: WorkspaceTabID | null; inTitleBar: boolean; onSelect: (id: WorkspaceTabID) => void }`;
  `segmentInset(rect: Rect, inTitleBar: boolean): { paddingLeft: number }`.

**Contesto:** questo è il task della trappola. La barra è
`-webkit-app-region: drag` perché ci si trascini la finestra; **ogni figlio
interattivo va marcato `no-drag`**, altrimenti i tab si vedono, l'hover
funziona e il click no — sembra un bug di gestione eventi ed è una regione di
trascinamento.

- [ ] **Passo 1: scrivere il test della riserva per i semafori**

`src/renderer/src/lib/workspace/tab-segment-layout.test.ts`:

```ts
import { test, expect } from 'vitest'
import { segmentInset, TRAFFIC_LIGHTS_WIDTH } from './tab-segment-layout.ts'

test('il segmento a sinistra nella barra del titolo lascia posto ai semafori', () => {
  expect(segmentInset({ x: 0, y: 0, w: 800, h: 34 }, true).paddingLeft).toBe(
    TRAFFIC_LIGHTS_WIDTH
  )
})

test('un segmento non a filo di sinistra non riserva nulla', () => {
  // Il secondo riquadro di uno split verticale: i semafori sono lontani.
  expect(segmentInset({ x: 400, y: 0, w: 400, h: 34 }, true).paddingLeft).toBe(0)
})

test('fuori dalla barra del titolo non si riserva mai', () => {
  expect(segmentInset({ x: 0, y: 200, w: 800, h: 34 }, false).paddingLeft).toBe(0)
})
```

- [ ] **Passo 2: eseguire il test e verificare che fallisca**

Comando: `pnpm test:unit -- tab-segment-layout`
Atteso: FAIL, modulo assente.

- [ ] **Passo 3: implementare la funzione pura**

`src/renderer/src/lib/workspace/tab-segment-layout.ts`:

```ts
import type { Rect } from '../../../../shared/workspace/layout-geometry'

/**
 * Larghezza occupata dai semafori piu il respiro a destra. Misurata su macOS
 * 15: tre pallini da 12px con 8px di passo, a partire da x = 12.
 */
export const TRAFFIC_LIGHTS_WIDTH = 78

/** Riserva a sinistra per i semafori, solo per il segmento a filo di bordo. */
export function segmentInset(rect: Rect, inTitleBar: boolean): { paddingLeft: number } {
  if (!inTitleBar) return { paddingLeft: 0 }
  if (rect.x > 0) return { paddingLeft: 0 }
  return { paddingLeft: TRAFFIC_LIGHTS_WIDTH }
}
```

- [ ] **Passo 4: eseguire il test e verificare che passi**

Comando: `pnpm test:unit -- tab-segment-layout`
Atteso: PASS, 3 test.

- [ ] **Passo 5: scrivere il componente**

`src/renderer/src/lib/workspace/TabSegment.svelte`:

```svelte
<script lang="ts">
  import type { Rect } from '../../../../shared/workspace/layout-geometry'
  import type { PaneGroupID, WorkspaceTabID } from '../../../../shared/workspace/layout-ids'
  import type { WorkspaceTab } from '../../../../shared/workspace/layout-types'
  import { segmentInset } from './tab-segment-layout.ts'

  const {
    groupId,
    rect,
    tabs,
    activeTabId,
    inTitleBar,
    onSelect
  }: {
    groupId: PaneGroupID
    rect: Rect
    tabs: WorkspaceTab[]
    activeTabId: WorkspaceTabID | null
    inTitleBar: boolean
    onSelect: (id: WorkspaceTabID) => void
  } = $props()

  const inset = $derived(segmentInset(rect, inTitleBar))
</script>

<div
  class="segment"
  data-group-id={groupId}
  style:left="{rect.x}px"
  style:width="{rect.w}px"
  style:padding-left="{inset.paddingLeft}px"
>
  {#each tabs as tab (tab.id)}
    <!--
      `no-drag` non e' decorativo. Il contenitore e' una regione di
      trascinamento della finestra: senza questa riga il tab si vede, l hover
      funziona e il click NON arriva mai. Il criterio 1 degli e2e esiste per
      questo, e la sua validazione per mutazione toglie proprio questa classe.
    -->
    <button
      class="tab no-drag"
      class:active={tab.id === activeTabId}
      data-tab-id={tab.id}
      onclick={() => onSelect(tab.id)}
    >
      {tab.title}
    </button>
  {/each}
</div>

<style>
  .segment {
    position: absolute;
    top: 0;
    height: var(--tiller-topbar-height);
    display: flex;
    align-items: center;
    gap: 2px;
    -webkit-app-region: drag;
  }
  .no-drag {
    -webkit-app-region: no-drag;
  }
  .tab {
    height: 24px;
    padding: 0 10px;
    border: 0;
    background: transparent;
    color: var(--tiller-text-secondary);
    border-radius: 5px;
    font-size: 12px;
    white-space: nowrap;
    cursor: default;
  }
  .tab.active {
    background: var(--tiller-surface-raised);
    color: var(--tiller-text-primary);
  }
</style>
```

- [ ] **Passo 6: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: segmento di tab con regione di trascinamento corretta"
```

---

## Task 4: barra superiore montata sulla geometria

**File:**
- Crea: `src/renderer/src/lib/workspace/TopBar.svelte`
- Crea: `src/renderer/src/lib/workspace/topbar-segments.ts`
- Crea: `src/renderer/src/lib/workspace/topbar-segments.test.ts`

**Interfacce:**
- Consuma: `groupRects` da `src/shared/workspace/layout-geometry.ts`;
  `TabSegment` dal Task 3.
- Produce: `segmentsInTitleBar(rects: Map<PaneGroupID, Rect>): PaneGroupID[]`;
  componente `TopBar`.

**Contesto:** la regola di collocazione della spec — ogni gruppo ha la sua
striscia sul proprio bordo superiore; se quel bordo è a `y = 0` la striscia sta
nella barra del titolo, altrimenti inline. Questo task fa il primo caso, il
Task 5 il secondo.

- [ ] **Passo 1: scrivere il test della selezione dei segmenti**

`src/renderer/src/lib/workspace/topbar-segments.test.ts`:

```ts
import { test, expect } from 'vitest'
import { segmentsInTitleBar } from './topbar-segments.ts'
import type { PaneGroupID } from '../../../../shared/workspace/layout-ids'

const g = (s: string): PaneGroupID => s as PaneGroupID

test('gruppo unico: la sua striscia sta nella barra del titolo', () => {
  const rects = new Map([[g('a'), { x: 0, y: 0, w: 800, h: 600 }]])
  expect(segmentsInTitleBar(rects)).toEqual([g('a')])
})

test('split verticale: entrambi i gruppi toccano il bordo superiore', () => {
  const rects = new Map([
    [g('a'), { x: 0, y: 0, w: 400, h: 600 }],
    [g('b'), { x: 400, y: 0, w: 400, h: 600 }]
  ])
  expect(segmentsInTitleBar(rects)).toEqual([g('a'), g('b')])
})

test('split orizzontale: solo il gruppo superiore', () => {
  const rects = new Map([
    [g('sopra'), { x: 0, y: 0, w: 800, h: 300 }],
    [g('sotto'), { x: 0, y: 300, w: 800, h: 300 }]
  ])
  expect(segmentsInTitleBar(rects)).toEqual([g('sopra')])
})

test('i segmenti escono ordinati da sinistra a destra', () => {
  // L ordine della mappa non e' garantito dal riduttore; quello visivo si'.
  const rects = new Map([
    [g('destro'), { x: 400, y: 0, w: 400, h: 600 }],
    [g('sinistro'), { x: 0, y: 0, w: 400, h: 600 }]
  ])
  expect(segmentsInTitleBar(rects)).toEqual([g('sinistro'), g('destro')])
})
```

- [ ] **Passo 2: eseguire il test e verificare che fallisca**

Comando: `pnpm test:unit -- topbar-segments`
Atteso: FAIL, modulo assente.

- [ ] **Passo 3: implementare**

`src/renderer/src/lib/workspace/topbar-segments.ts`:

```ts
import type { PaneGroupID } from '../../../../shared/workspace/layout-ids'
import type { Rect } from '../../../../shared/workspace/layout-geometry'

/**
 * I gruppi la cui striscia di tab va disegnata DENTRO la barra del titolo:
 * quelli il cui bordo superiore e' a y = 0. Gli altri la ricevono inline
 * (Task 5).
 *
 * Nota su cosa questo NON e': non e' "il gruppo attivo". Quella regola
 * sembrerebbe piu semplice ma introdurrebbe uno stato che il modello non ha —
 * quale gruppo possiede la barra — da tenere poi in sincronia con il focus,
 * con la chiusura dei gruppi e con gli split. Stato derivato che si puo'
 * calcolare e che, se memorizzato, prima o poi diverge.
 */
export function segmentsInTitleBar(rects: ReadonlyMap<PaneGroupID, Rect>): PaneGroupID[] {
  return [...rects.entries()]
    .filter(([, rect]) => rect.y === 0)
    .sort(([, a], [, b]) => a.x - b.x)
    .map(([id]) => id)
}
```

- [ ] **Passo 4: eseguire il test e verificare che passi**

Comando: `pnpm test:unit -- topbar-segments`
Atteso: PASS, 4 test.

- [ ] **Passo 5: scrivere il componente**

`src/renderer/src/lib/workspace/TopBar.svelte`:

```svelte
<script lang="ts">
  import TabSegment from './TabSegment.svelte'
  import { segmentsInTitleBar } from './topbar-segments.ts'
  import { groupRects } from '../../../../shared/workspace/layout-geometry'
  import { groupOf } from '../../../../shared/workspace/layout-invariants'
  import type { WorkspaceLayout } from '../../../../shared/workspace/layout-invariants'
  import type { Rect } from '../../../../shared/workspace/layout-geometry'
  import type { WorkspaceTabID } from '../../../../shared/workspace/layout-ids'

  const {
    layout,
    container,
    onSelect
  }: {
    layout: WorkspaceLayout
    container: Rect
    onSelect: (id: WorkspaceTabID) => void
  } = $props()

  // La geometria arriva dalla 4a. Non si misura il DOM: interrogarlo con
  // getBoundingClientRect significherebbe un reflow sincrono proprio durante
  // il trascinamento di un divisore.
  const rects = $derived(groupRects(layout, container))
  const inTitleBar = $derived(segmentsInTitleBar(rects))
</script>

<div class="topbar">
  {#each inTitleBar as groupId (groupId)}
    {@const gruppo = groupOf(layout, groupId)}
    {#if gruppo !== undefined}
      <TabSegment
        {groupId}
        rect={rects.get(groupId)!}
        tabs={gruppo.tabs}
        activeTabId={gruppo.activeTabId}
        inTitleBar={true}
        {onSelect}
      />
    {/if}
  {/each}
</div>

<style>
  .topbar {
    position: relative;
    height: var(--tiller-topbar-height);
    flex: 0 0 auto;
    background: var(--tiller-surface-chrome);
    -webkit-app-region: drag;
  }
</style>
```

- [ ] **Passo 6: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: barra superiore con segmenti allineati ai divisori"
```

---

## Task 5: strisce inline per i gruppi che non toccano il bordo

**File:**
- Modifica: `src/renderer/src/lib/workspace/topbar-segments.ts`
- Modifica: `src/renderer/src/lib/workspace/topbar-segments.test.ts`
- Crea: `src/renderer/src/lib/workspace/InlineSegments.svelte`

**Interfacce:**
- Consuma: `TabSegment` (Task 3), `groupRects`.
- Produce: `segmentsInline(rects): PaneGroupID[]`; componente `InlineSegments`.

**Contesto:** completa la regola della Sezione 2 della spec. Con uno split
orizzontale il gruppo inferiore non tocca la barra del titolo, e la sua striscia
va disegnata sul proprio bordo superiore.

- [ ] **Passo 1: aggiungere i test**

In coda a `topbar-segments.test.ts`:

```ts
import { segmentsInline } from './topbar-segments.ts'

test('gruppo unico: nessuna striscia inline', () => {
  const rects = new Map([[g('a'), { x: 0, y: 0, w: 800, h: 600 }]])
  expect(segmentsInline(rects)).toEqual([])
})

test('split orizzontale: il gruppo inferiore ha la sua striscia inline', () => {
  const rects = new Map([
    [g('sopra'), { x: 0, y: 0, w: 800, h: 300 }],
    [g('sotto'), { x: 0, y: 300, w: 800, h: 300 }]
  ])
  expect(segmentsInline(rects)).toEqual([g('sotto')])
})

test('ogni gruppo sta in una lista sola', () => {
  // Le due funzioni partizionano: un gruppo che finisse in entrambe avrebbe
  // due strisce, e la seconda coprirebbe il contenuto del riquadro.
  const rects = new Map([
    [g('a'), { x: 0, y: 0, w: 800, h: 300 }],
    [g('b'), { x: 0, y: 300, w: 400, h: 300 }],
    [g('c'), { x: 400, y: 300, w: 400, h: 300 }]
  ])
  const insieme = [...segmentsInTitleBar(rects), ...segmentsInline(rects)]
  expect(new Set(insieme).size).toBe(3)
  expect(insieme).toHaveLength(3)
})
```

- [ ] **Passo 2: eseguire i test e verificare che falliscano**

Comando: `pnpm test:unit -- topbar-segments`
Atteso: FAIL, `segmentsInline is not a function`.

- [ ] **Passo 3: implementare**

In coda a `topbar-segments.ts`:

```ts
/**
 * I gruppi la cui striscia va disegnata inline, sopra il proprio riquadro:
 * il complemento esatto di `segmentsInTitleBar`. Le due funzioni partizionano
 * i gruppi — un gruppo in entrambe avrebbe due strisce.
 */
export function segmentsInline(rects: ReadonlyMap<PaneGroupID, Rect>): PaneGroupID[] {
  return [...rects.entries()]
    .filter(([, rect]) => rect.y !== 0)
    .sort(([, a], [, b]) => a.y - b.y || a.x - b.x)
    .map(([id]) => id)
}
```

- [ ] **Passo 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- topbar-segments`
Atteso: PASS, 7 test.

- [ ] **Passo 5: scrivere il componente**

`src/renderer/src/lib/workspace/InlineSegments.svelte`: identico a `TopBar` nel
corpo, con `segmentsInline` al posto di `segmentsInTitleBar`, `inTitleBar={false}`
passato a `TabSegment`, e il contenitore posizionato in assoluto sul rettangolo
del gruppo invece che in cima alla finestra:

```svelte
<script lang="ts">
  import TabSegment from './TabSegment.svelte'
  import { segmentsInline } from './topbar-segments.ts'
  import { groupRects } from '../../../../shared/workspace/layout-geometry'
  import { groupOf } from '../../../../shared/workspace/layout-invariants'
  import type { WorkspaceLayout } from '../../../../shared/workspace/layout-invariants'
  import type { Rect } from '../../../../shared/workspace/layout-geometry'
  import type { WorkspaceTabID } from '../../../../shared/workspace/layout-ids'

  const {
    layout,
    container,
    onSelect
  }: {
    layout: WorkspaceLayout
    container: Rect
    onSelect: (id: WorkspaceTabID) => void
  } = $props()

  const rects = $derived(groupRects(layout, container))
  const inline = $derived(segmentsInline(rects))
</script>

{#each inline as groupId (groupId)}
  {@const gruppo = groupOf(layout, groupId)}
  {@const rect = rects.get(groupId)}
  {#if gruppo !== undefined && rect !== undefined}
    <div class="inline-host" style:top="{rect.y}px">
      <TabSegment
        {groupId}
        {rect}
        tabs={gruppo.tabs}
        activeTabId={gruppo.activeTabId}
        inTitleBar={false}
        {onSelect}
      />
    </div>
  {/if}
{/each}

<style>
  .inline-host {
    position: absolute;
    left: 0;
    right: 0;
    height: var(--tiller-topbar-height);
    /* Non e' chrome della finestra: qui trascinare deve selezionare testo,
       non spostare la finestra. */
    -webkit-app-region: no-drag;
  }
</style>
```

- [ ] **Passo 6: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 7: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: strisce di tab inline per i gruppi sotto il bordo superiore"
```

---

## Task 6: riquadri e divisori

**File:**
- Crea: `src/renderer/src/lib/workspace/PaneGrid.svelte`
- Modifica: `package.json` (dipendenza `paneforge`)

**Interfacce:**
- Consuma: `groupRects`, `dividerRects`, comando `setSplitFraction` della 4a.
- Produce: componente `PaneGrid` con props `{ layout, container, onFraction }`.

**Contesto:** i riquadri sono posizionati in assoluto sui rettangoli della 4a
(V6). `paneforge` prende in carico il trascinamento dei divisori — vincoli di
dimensione, tastiera, accessibilità — senza possedere il modello: la frazione
risultante torna al riduttore.

- [ ] **Passo 1: installare la dipendenza**

```bash
pnpm add paneforge@1.0.2
```

- [ ] **Passo 2: scrivere il componente**

`src/renderer/src/lib/workspace/PaneGrid.svelte`:

```svelte
<script lang="ts">
  import { groupRects, dividerRects } from '../../../../shared/workspace/layout-geometry'
  import { groupOf } from '../../../../shared/workspace/layout-invariants'
  import Terminal from '../../components/Terminal.svelte'
  import type { WorkspaceLayout } from '../../../../shared/workspace/layout-invariants'
  import type { Rect } from '../../../../shared/workspace/layout-geometry'
  import type { SplitID } from '../../../../shared/workspace/layout-ids'

  const {
    layout,
    container,
    onFraction
  }: {
    layout: WorkspaceLayout
    container: Rect
    onFraction: (splitId: SplitID, frazione: number) => void
  } = $props()

  const rects = $derived(groupRects(layout, container))
  const divisori = $derived(dividerRects(layout, container))
</script>

<div class="grid">
  {#each [...rects.entries()] as [groupId, rect] (groupId)}
    {@const gruppo = groupOf(layout, groupId)}
    {@const attivo = gruppo?.tabs.find((t) => t.id === gruppo.activeTabId)}
    {#if attivo !== undefined}
      <div
        class="pane"
        data-group-id={groupId}
        style:left="{rect.x}px"
        style:top="{rect.y}px"
        style:width="{rect.w}px"
        style:height="{rect.h}px"
      >
        <!--
          `activeTabId` NON e' un paneId: il tab referenzia il contenuto con
          `content: { kind: 'terminal', id }`, e quell `id` e' il pane. Sono
          due spazi di identificatori distinti, ed entrambi sono stringhe —
          scambiarli compila e fallisce a runtime con un pane vuoto.
          L unione e' discriminata: quando le Fasi 5 e 6 aggiungeranno `chat` e
          `document`, questo `switch` deve fallire la compilazione.
        -->
        {#if attivo.content.kind === 'terminal'}
          <Terminal paneId={attivo.content.id} />
        {/if}
      </div>
    {/if}
  {/each}

  {#each divisori as divisore (divisore.splitId)}
    <!--
      La banda e' la zona sensibile al mouse, la hairline la riga visibile:
      sono due rettangoli distinti perche' una riga da 1px non si azzecca col
      cursore. Entrambe vengono dalla 4a, non si calcolano qui.
    -->
    <div
      class="divider-band"
      role="separator"
      tabindex="0"
      data-split-id={divisore.splitId}
      style:left="{divisore.band.x}px"
      style:top="{divisore.band.y}px"
      style:width="{divisore.band.w}px"
      style:height="{divisore.band.h}px"
      onpointerdown={(e) => avviaTrascinamento(e, divisore.splitId)}
    >
      <div
        class="divider-hairline"
        style:left="{divisore.hairline.x - divisore.band.x}px"
        style:top="{divisore.hairline.y - divisore.band.y}px"
        style:width="{divisore.hairline.w}px"
        style:height="{divisore.hairline.h}px"
      ></div>
    </div>
  {/each}
</div>
```

Nel blocco `<script>` dello stesso file:

```ts
  /**
   * Converte il puntatore in una frazione e la manda al riduttore.
   *
   * `setPointerCapture` non e' un dettaglio: senza, il puntatore che esce dal
   * divisore durante il trascinamento smette di consegnare eventi, e il
   * divisore resta incollato al mouse finche' non si clicca altrove.
   */
  // L asse arriva da `dividerRects` della 4a, non da una ricerca nell albero:
  // chi ha calcolato la geometria lo sapeva gia.
  function avviaTrascinamento(evento: PointerEvent, splitId: SplitID): void {
    const bersaglio = evento.currentTarget as HTMLElement
    bersaglio.setPointerCapture(evento.pointerId)
    const divisore = divisori.find((d) => d.splitId === splitId)
    if (divisore === undefined) return

    const muovi = (e: PointerEvent): void => {
      const frazione =
        divisore.axis === 'vertical'
          ? (e.clientX - container.x) / container.w
          : (e.clientY - container.y) / container.h
      // Nessun clamp qui: i vincoli di dimensione minima sono un invariante
      // della 4a, e il riduttore RIFIUTA la transizione fuori soglia. Un clamp
      // locale nasconderebbe il rifiuto e i due limiti divergerebbero.
      onFraction(splitId, frazione)
    }

    const finisci = (): void => {
      bersaglio.removeEventListener('pointermove', muovi)
      bersaglio.removeEventListener('pointerup', finisci)
    }
    bersaglio.addEventListener('pointermove', muovi)
    bersaglio.addEventListener('pointerup', finisci)
  }
```

Nota su `paneforge`: la libreria è installata per i suoi vincoli di dimensione e
la sua gestione da tastiera, che questo trascinamento a mano **non** copre. Se
alla verifica del Passo 3 il gesto risulta già adeguato e la tastiera è l'unica
lacuna, sostituire questo blocco con `PaneResizer` di `paneforge` invece di
scrivere anche la parte da tastiera. Decidere lì, con il gesto sotto mano, non
qui.

- [ ] **Passo 3: verificare a mano che i riquadri compaiano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: un riquadro con il terminale; dopo uno split, due riquadri affiancati
con un divisore trascinabile.

- [ ] **Passo 4: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 5: commit**

```bash
git add package.json pnpm-lock.yaml src/renderer/src/lib/workspace/
git commit -m "feat: riquadri posizionati sulla geometria e divisori trascinabili"
```

---

## Task 7: trascinamento dei tab fra gruppi

**File:**
- Modifica: `src/renderer/src/lib/workspace/TabSegment.svelte`
- Modifica: `package.json` (dipendenza `svelte-dnd-action`)

**Interfacce:**
- Consuma: `resolveDropTarget` da `src/shared/workspace/drop-target.ts` (4a);
  comando `moveTab`.
- Produce: niente verso altri task.

**Contesto:** `svelte-dnd-action` non usa il drag&drop nativo del browser, che
su Electron è la fonte principale di grattacapi (immagini fantasma, eventi che
non arrivano, cursori sbagliati). Il **dove** cade il tab lo decide la 4a con
`resolveDropTarget`, non la libreria: la libreria fornisce il gesto, il modello
fornisce la risposta.

- [ ] **Passo 1: installare la dipendenza**

```bash
pnpm add svelte-dnd-action@0.9.77
```

- [ ] **Passo 2: applicare l azione alla lista dei tab**

In `TabSegment.svelte`, avvolgere il `{#each}` in un contenitore con
`use:dndzone`, e aggiungere alle props `onMoveTab`:

```svelte
<script lang="ts">
  import { dndzone, type DndEvent } from 'svelte-dnd-action'

  // …props esistenti, piu:
  //   onMoveTab: (tabId: WorkspaceTabID, versoGruppo: PaneGroupID, indice: number) => void

  /**
   * `svelte-dnd-action` vuole poter riordinare la lista che gli si passa, ma
   * l ordine vero e' del riduttore. Si tiene una copia locale per la sola
   * durata del gesto e si scarta al `finalize`: quello che torna dal main
   * sovrascrive comunque.
   */
  let durante = $state<WorkspaceTab[] | null>(null)
  const mostrati = $derived(durante ?? tabs)

  function consider(e: CustomEvent<DndEvent<WorkspaceTab>>): void {
    durante = e.detail.items
  }

  function finalize(e: CustomEvent<DndEvent<WorkspaceTab>>): void {
    durante = null
    const caduto = e.detail.info.id as WorkspaceTabID
    const indice = e.detail.items.findIndex((t) => t.id === caduto)
    if (indice < 0) return
    // Il riduttore e' la sola autorita' sull ordine: si emette il comando e
    // si aspetta il layout di ritorno, non si muta la lista qui.
    onMoveTab(caduto, groupId, indice)
  }
</script>

<div
  class="tabs"
  use:dndzone={{ items: mostrati, type: 'workspace-tab', flipDurationMs: 0 }}
  onconsider={consider}
  onfinalize={finalize}
>
  {#each mostrati as tab (tab.id)}
    <!-- …bottone del Task 3, invariato, `no-drag` compreso… -->
  {/each}
</div>
```

`type: 'workspace-tab'` è ciò che permette a un tab di attraversare i segmenti:
zone con lo stesso tipo si accettano fra loro.

Il rilascio **sul bordo di un riquadro** — che crea uno split invece di
spostare — non passa di qui: è `resolveDropTarget(punto, rects)` della 4a,
chiamato dal `PaneGrid` sul proprio `ondragover`. La libreria fornisce il gesto,
il modello fornisce la risposta.

- [ ] **Passo 3: verificare a mano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: un tab trascinato da un segmento all'altro cambia gruppo; rilasciandolo
sul bordo di un riquadro nasce uno split.

- [ ] **Passo 4: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 5: commit**

```bash
git add package.json pnpm-lock.yaml src/renderer/src/lib/workspace/
git commit -m "feat: trascinamento dei tab fra gruppi"
```

---

## Task 8: albero della sidebar

**File:**
- Crea: `src/renderer/src/lib/workspace/SidebarTree.svelte`
- Crea: `src/renderer/src/lib/workspace/TreeRow.svelte`
- Crea: `src/renderer/src/lib/workspace/tree-model.ts`
- Crea: `src/renderer/src/lib/workspace/tree-model.test.ts`

**Interfacce:**
- Consuma: `allTabs` da `layout-invariants.ts`; lo stato dei progetti e dei
  worktree da `app-model.svelte.ts`.
- Produce: `type TreeNode = { kind: 'project' | 'worktree' | 'tab'; id: string;
  label: string; depth: number; children: TreeNode[] }`;
  `buildTree(progetti, layoutPerWorktree): TreeNode[]`.

**Contesto:** V1 — tre livelli a forma fissa. **Un progetto con un solo worktree
resta espandibile**, con la sua riga figlia: la proposta di collassarlo in una
riga sola è stata respinta esplicitamente.

- [ ] **Passo 1: scrivere i test**

`src/renderer/src/lib/workspace/tree-model.test.ts`:

```ts
import { test, expect } from 'vitest'
import { buildTree } from './tree-model.ts'

test('un progetto con un solo worktree resta a tre livelli', () => {
  // V1: la forma non cambia in base al contenuto. Collassare il caso a un
  // worktree e' stato respinto esplicitamente dall utente.
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [{ id: 'w1', branch: 'main' }] }],
    new Map([['w1', [{ id: 't1', title: 'Terminal 1' }]]])
  )
  expect(albero[0].kind).toBe('project')
  expect(albero[0].children[0].kind).toBe('worktree')
  expect(albero[0].children[0].children[0].kind).toBe('tab')
})

test('il terzo livello e allTabs appiattito, non un gruppo per volta', () => {
  // I tab di gruppi diversi finiscono nella stessa lista: l albero e' l unica
  // vista indipendente dagli split.
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [{ id: 'w1', branch: 'main' }] }],
    new Map([['w1', [{ id: 't1', title: 'Terminal 1' }, { id: 't2', title: 'Chat' }]]])
  )
  expect(albero[0].children[0].children.map((n) => n.label)).toEqual([
    'Terminal 1',
    'Chat'
  ])
})

test('un worktree senza layout caricato non ha figli, ma esiste', () => {
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [{ id: 'w1', branch: 'main' }] }],
    new Map()
  )
  expect(albero[0].children[0].children).toEqual([])
})
```

- [ ] **Passo 2: eseguire i test e verificare che falliscano**

Comando: `pnpm test:unit -- tree-model`
Atteso: FAIL, modulo assente.

- [ ] **Passo 3: implementare `tree-model.ts` e i due componenti**

`src/renderer/src/lib/workspace/tree-model.ts`:

```ts
export interface TreeNode {
  kind: 'project' | 'worktree' | 'tab'
  id: string
  label: string
  depth: number
  children: TreeNode[]
}

export interface ProjectRow {
  id: string
  name: string
  worktrees: { id: string; branch: string }[]
}

export interface TabRow {
  id: string
  title: string
}

/**
 * Progetti -> worktree -> tab, profondita 0/1/2.
 *
 * La forma non cambia in base al contenuto (V1): un progetto con un solo
 * worktree resta comunque un nodo con un figlio, perche' collassarlo e' stato
 * respinto esplicitamente. Non aggiungere qui una scorciatoia "se ne ha uno
 * solo".
 */
export function buildTree(
  progetti: readonly ProjectRow[],
  tabPerWorktree: ReadonlyMap<string, readonly TabRow[]>
): TreeNode[] {
  return progetti.map((progetto) => ({
    kind: 'project' as const,
    id: progetto.id,
    label: progetto.name,
    depth: 0,
    children: progetto.worktrees.map((worktree) => ({
      kind: 'worktree' as const,
      id: worktree.id,
      label: worktree.branch,
      depth: 1,
      // Un worktree il cui layout non e' ancora caricato non ha figli, ma
      // esiste: la riga non deve sparire mentre il layout arriva.
      children: (tabPerWorktree.get(worktree.id) ?? []).map((tab) => ({
        kind: 'tab' as const,
        id: tab.id,
        label: tab.title,
        depth: 2,
        children: []
      }))
    }))
  }))
}
```

`TreeRow.svelte` disegna una riga sola, con freccia di espansione per
`kind !== 'tab'` e nessuna per il terzo livello. **Gli attributi `data-` non
sono decorativi: il criterio 4 degli e2e li seleziona.**

```svelte
<div
  class="riga"
  data-row-kind={node.kind}
  data-row-id={node.id}
  style:padding-left="{node.depth * 14 + 6}px"
>
  {#if node.kind !== 'tab'}
    <button class="freccia" onclick={toggle} aria-label="Toggle">
      {espanso ? '▾' : '▸'}
    </button>
  {/if}
  <span class="etichetta">{node.label}</span>
  <StatusDot status={node.status} />
</div>
```

`SidebarTree.svelte` ricorre su `children`, montando `TreeRow` per ogni nodo e
sé stesso per i figli quando il nodo è espanso.

- [ ] **Passo 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- tree-model`
Atteso: PASS, 3 test.

- [ ] **Passo 5: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: albero della sidebar a tre livelli"
```

---

## Task 9: stato degli agenti sulle righe dell albero

**File:**
- Modifica: `src/renderer/src/lib/workspace/tree-model.ts`
- Modifica: `src/renderer/src/lib/workspace/tree-model.test.ts`
- Modifica: `src/renderer/src/lib/workspace/TreeRow.svelte`
- Crea: `src/renderer/src/lib/workspace/StatusDot.svelte`

**Interfacce:**
- Consuma: `rollupStatus` dal Task 2; `PaneSnapshot.status` e `.agentId` dalla
  Fase 3.
- Produce: `TreeNode` guadagna `status: AgentStatus | null`.

**Contesto:** P6. Il pallino sta su **ogni riga**, non solo sui tab: worktree e
progetto mostrano il rollup dei discendenti, sempre, anche da espansi. Una regola
sola, nessuna dipendenza dallo stato di apertura.

- [ ] **Passo 1: aggiungere i test**

```ts
test('la riga del worktree mostra il rollup dei suoi tab', () => {
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [{ id: 'w1', branch: 'main' }] }],
    new Map([['w1', [
      { id: 't1', title: 'Terminal 1', status: 'running' },
      { id: 't2', title: 'Chat', status: 'needs-input' }
    ]]])
  )
  expect(albero[0].children[0].status).toBe('needs-input')
})

test('il rollup risale fino al progetto', () => {
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [
      { id: 'w1', branch: 'main' },
      { id: 'w2', branch: 'fix' }
    ] }],
    new Map([
      ['w1', [{ id: 't1', title: 'Terminal 1', status: 'done' }]],
      ['w2', [{ id: 't2', title: 'Chat', status: 'error' }]]
    ])
  )
  expect(albero[0].status).toBe('error')
})

test('un albero senza agenti non mostra pallini', () => {
  const albero = buildTree(
    [{ id: 'p1', name: 'tiller', worktrees: [{ id: 'w1', branch: 'main' }] }],
    new Map([['w1', [{ id: 't1', title: 'Terminal 1', status: null }]]])
  )
  expect(albero[0].status).toBe(null)
})
```

- [ ] **Passo 2: eseguire i test e verificare che falliscano**

Comando: `pnpm test:unit -- tree-model`
Atteso: FAIL, `status` è `undefined`.

- [ ] **Passo 3: implementare**

`TabRow` guadagna `status: AgentStatus | null`, `TreeNode` pure, e `buildTree`
risale con `rollupStatus`:

```ts
import { rollupStatus } from '../../../../shared/status-rollup'
import type { AgentStatus } from '../../../../shared/agent-status'

// dentro buildTree, per ogni worktree:
const figliTab = (tabPerWorktree.get(worktree.id) ?? []).map((tab) => ({
  kind: 'tab' as const,
  id: tab.id,
  label: tab.title,
  depth: 2,
  status: tab.status,
  children: []
}))
const nodoWorktree = {
  kind: 'worktree' as const,
  id: worktree.id,
  label: worktree.branch,
  depth: 1,
  status: rollupStatus(figliTab.map((n) => n.status)),
  children: figliTab
}

// e per il progetto, sullo stesso schema:
//   status: rollupStatus(nodiWorktree.map((n) => n.status))
```

Il rollup si calcola **sempre**, non solo per le righe collassate: una regola
sola, nessuna dipendenza dallo stato di apertura.

`StatusDot.svelte` disegna **forma distinta per stato**, non solo colore:

```svelte
<!--
  Forma oltre al colore. Con dieci worktree aperti l albero e' esattamente
  l informazione che serve a chi non distingue i colori, e un pallino che varia
  solo di tinta non gliela da'.
-->
```

| Stato | Forma | Token colore |
|---|---|---|
| `needs-input` | anello vuoto pulsante | `--tiller-status-attention` |
| `error` | triangolo | `--tiller-status-error` |
| `running` | cerchio pieno | `--tiller-status-running` |
| `done` | trattino | `--tiller-status-muted` |
| `null` | niente | — |

`StatusDot.svelte` posa `data-status`, che il criterio 4 legge:

```svelte
<script lang="ts">
  import type { AgentStatus } from '../../../../shared/agent-status'
  const { status }: { status: AgentStatus | null } = $props()
</script>

{#if status !== null}
  <span class="dot" class:needs-input={status === 'needs-input'} data-status={status}
  ></span>
{/if}
```

- [ ] **Passo 4: eseguire i test e verificare che passino**

Comando: `pnpm test:unit -- tree-model`
Atteso: PASS, 6 test.

- [ ] **Passo 5: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: stato degli agenti sulle righe dell albero"
```

---

## Task 10: barra inferiore a tutta larghezza

**File:**
- Crea: `src/renderer/src/lib/workspace/StatusBar.svelte`
- Modifica: `src/renderer/src/App.svelte`

**Interfacce:**
- Consuma: `windowTitle` non serve qui; il contesto arriva da `app-model`.
- Produce: componente `StatusBar`.

**Contesto:** V3. Una striscia continua da bordo a bordo, ~24 px, **sotto tutto,
sidebar compresa**. Oggi in Tiller si ferma al bordo destro del pannello
centrale, e la sidebar ha una riga inferiore separata con ingranaggio e aiuto:
quella riga confluisce qui a destra e sparisce dalla sidebar.

- [ ] **Passo 1: ristrutturare `App.svelte`**

La finestra diventa una colonna di tre righe, non due colonne con chrome
interno:

```
┌──────────────────────────────────────────┐
│ TopBar (34px)                            │
├──────────┬───────────────────────────────┤
│ Sidebar  │ PaneGrid + InlineSegments     │
├──────────┴───────────────────────────────┤
│ StatusBar (24px, bordo a bordo)          │
└──────────────────────────────────────────┘
```

La striscia sta **fuori** dal contenitore che ospita i riquadri: altrimenti la
geometria della 4a, che copre l'area dei pane, dovrebbe conoscerne l'altezza.
La striscia è chrome della finestra, non del workspace.

- [ ] **Passo 2: scrivere il componente**

`src/renderer/src/lib/workspace/StatusBar.svelte`:

```svelte
<script lang="ts">
  import type { Snippet } from 'svelte'

  const {
    project,
    worktree,
    quotas
  }: {
    project: string | null
    worktree: string | null
    quotas?: Snippet
  } = $props()
</script>

<div class="statusbar">
  <span class="context">
    {#if project !== null}{project}{#if worktree !== null} · {worktree}{/if}{/if}
  </span>
  <span class="spacer"></span>
  {@render quotas?.()}
  <button class="chrome-btn" title="Settings" aria-label="Settings">⚙</button>
  <button class="chrome-btn" title="Help" aria-label="Help">?</button>
</div>

<style>
  /*
    `width: 100%` qui vale quanto il contenitore: va montata come terza riga
    della colonna di finestra, MAI dentro il pannello centrale. Montarla
    dentro e' esattamente il difetto di V3 che questo task chiude.
  */
  .statusbar {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: var(--tiller-statusbar-height);
    padding: 0 8px;
    flex: 0 0 auto;
    background: var(--tiller-surface-chrome);
    font-size: 11px;
    color: var(--tiller-text-secondary);
  }
  .spacer {
    flex: 1 1 auto;
  }
  .chrome-btn {
    border: 0;
    background: transparent;
    color: inherit;
    cursor: default;
  }
</style>
```

La riga inferiore che oggi la sidebar ha per sé (ingranaggio e aiuto) va
**rimossa** da lì: i due bottoni ora vivono qui.

- [ ] **Passo 3: icone della barra superiore distinguibili (P4)**

Sostituire i tre rettangoli quasi identici con simboli distinti e aggiungere
`title` a ciascuno. Nessuna scelta di design: è un difetto da chiudere.

- [ ] **Passo 4: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 5: commit**

```bash
git add src/renderer/src/
git commit -m "feat: barra inferiore a tutta larghezza e icone distinguibili"
```

---

## Task 11: quote come barre di avanzamento

**File:**
- Crea: `src/renderer/src/lib/workspace/QuotaBar.svelte`
- Modifica: `src/renderer/src/lib/workspace/StatusBar.svelte`

**Interfacce:**
- Consuma: `mostConstrained` e `QuotaWindow` dal Task 2.
- Produce: componente `QuotaBar` con props
  `{ agentId: string; windows: QuotaWindow[] }`.

**Contesto:** V4. Un widget per agente — icona, una barra, percentuale — e la
barra mostra la finestra temporale più vicina all'esaurimento. Il dettaglio
completo, entrambe le finestre più l'ora di ripristino, sta nel `title`.

**Da dove arrivano i dati:** fuori ambito. La 4b disegna quello che il main le
passa. Se il main non pubblica quote per un agente, **il widget non compare** —
nessun segnaposto, nessuna barra a zero, che si leggerebbe come «quota
esaurita», cioè il contrario del vero.

- [ ] **Passo 1: scrivere il componente**

```svelte
<script lang="ts">
  import { mostConstrained, type QuotaWindow } from '../../../../shared/status-rollup'

  const { agentId, windows }: { agentId: string; windows: QuotaWindow[] } = $props()

  const peggiore = $derived(mostConstrained(windows))
  const dettaglio = $derived(
    windows.map((w) => `${w.label}: ${Math.round(w.used * 100)}%`).join(' · ')
  )
</script>

<!-- Nessuna quota: niente widget. Una barra a zero direbbe il contrario. -->
{#if peggiore !== null}
  <div class="quota" title={dettaglio} data-agent-id={agentId}>
    <span class="icon"></span>
    <div class="track"><div class="fill" style:width="{peggiore.used * 100}%"></div></div>
    <span class="pct">{Math.round(peggiore.used * 100)}%</span>
  </div>
{/if}
```

- [ ] **Passo 2: verificare a mano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: con `37% 5h` e `83% wk` la barra mostra `83%`, e il tooltip entrambe.

- [ ] **Passo 3: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 4: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: quote come barre di avanzamento"
```

---

## Task 12: stato vuoto

**File:**
- Crea: `src/renderer/src/lib/workspace/EmptyState.svelte`
- Modifica: `src/renderer/src/App.svelte`

**Interfacce:**
- Consuma: numero di progetti e worktree selezionato da `app-model`.
- Produce: componente `EmptyState` con prop `{ hasProjects: boolean }`.

**Contesto:** P1 e P2. Senza worktree selezionato, oggi tre pannelli dicono la
stessa cosa. Pannello destro e pannello agenti vanno **nascosti**, non lasciati
vuoti, e resta un solo messaggio al centro.

Due varianti, perché le situazioni sono due:

| Situazione | Messaggio | Azione |
|---|---|---|
| nessun progetto | `No projects` | bottone **Add project** |
| progetti sì, worktree non selezionato | `Select a worktree` | nessuna |

Nel secondo caso non c'è bottone perché l'azione è nella sidebar, già davanti
agli occhi. Nel primo il `+` è in alto a destra, a oltre mille pixel di
distanza, in mezzo ad altre tre icone simili: lì il bottone serve.

**Terzo caso — worktree selezionato ma senza tab.** Non si disegna. La 4a non lo
produce: chiudere l'ultimo tab di un gruppo chiude il gruppo, e un worktree
appena aperto riceve un tab terminale iniziale. Se dovesse comparire è un
difetto della 4a, e uno stato vuoto qui lo nasconderebbe dietro una schermata
gentile.

- [ ] **Passo 1: scrivere il componente e nascondere i pannelli**

`src/renderer/src/lib/workspace/EmptyState.svelte`:

```svelte
<script lang="ts">
  const {
    hasProjects,
    onAddProject
  }: { hasProjects: boolean; onAddProject: () => void } = $props()
</script>

<div class="vuoto">
  {#if hasProjects}
    <!-- Nessun bottone: l azione e' nella sidebar, gia' davanti agli occhi. -->
    <p>Select a worktree</p>
  {:else}
    <!-- Qui il bottone serve: il `+` sta in alto a destra, a oltre mille
         pixel di distanza, in mezzo ad altre tre icone simili. -->
    <p>No projects</p>
    <button onclick={onAddProject}>Add project</button>
  {/if}
</div>
```

In `App.svelte`, rendere condizionale il **montaggio** — non la visibilità — del
pannello destro e del pannello agenti su `worktreeSelezionato !== null`.
Nasconderli con `visibility` lascerebbe lo spazio occupato, che è il difetto di
partenza.

- [ ] **Passo 2: verificare a mano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: senza progetti, un solo messaggio con il bottone; con progetti ma senza
selezione, un solo messaggio senza bottone; in nessuno dei due casi compaiono
pannello destro e pannello agenti.

- [ ] **Passo 3: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 4: commit**

```bash
git add src/renderer/src/
git commit -m "feat: uno stato vuoto solo, con l azione dentro"
```

---

## Task 13: pannello agenti globale

**File:**
- Crea: `src/renderer/src/lib/workspace/AgentsPanel.svelte`

**Interfacce:**
- Consuma: `PaneSnapshot.agentId` e `.status` (Fase 3) per **tutti** i pane, non
  solo quelli del worktree corrente.
- Produce: componente `AgentsPanel`.

**Contesto:** P5. La proposta di fondere il pannello nell'albero è stata
respinta: l'utente usa entrambi. Divisione del lavoro, da qui in avanti
esplicita:

- **albero** — il worktree che si ha davanti, stato in contesto;
- **pannello** — tutti i worktree insieme, compresi i collassati e i chiusi.

Il difetto da chiudere non era il pannello di troppo: era che il pannello si
limitava al worktree corrente, cioè faceva il lavoro dell'albero invece del
proprio.

Cliccare una riga porta a quel tab **anche se il worktree è chiuso**: lo apre e
ci va.

- [ ] **Passo 1: scrivere il componente**

```svelte
<script lang="ts">
  import StatusDot from './StatusDot.svelte'
  import type { AgentStatus } from '../../../../shared/agent-status'

  interface AgentRow {
    paneId: string
    worktreeId: string
    worktreeBranch: string
    agentId: string
    status: AgentStatus | null
    tabTitle: string
  }

  const {
    rows,
    onReveal
  }: {
    rows: AgentRow[]
    onReveal: (worktreeId: string, paneId: string) => void
  } = $props()

  // Raggruppate per worktree: e' la dimensione che l albero non copre.
  const perWorktree = $derived(
    rows.reduce<Map<string, AgentRow[]>>((acc, riga) => {
      const lista = acc.get(riga.worktreeId) ?? []
      lista.push(riga)
      return acc.set(riga.worktreeId, lista)
    }, new Map())
  )
</script>

{#each [...perWorktree.entries()] as [worktreeId, righe] (worktreeId)}
  <div class="gruppo" data-agents-worktree={worktreeId}>
    <div class="intestazione">{righe[0].worktreeBranch}</div>
    {#each righe as riga (riga.paneId)}
      <button class="riga" onclick={() => onReveal(riga.worktreeId, riga.paneId)}>
        <StatusDot status={riga.status} />
        <span class="agente">{riga.agentId}</span>
        <span class="titolo">{riga.tabTitle}</span>
      </button>
    {/each}
  </div>
{/each}
```

`onReveal` apre il worktree se non è aperto e poi seleziona il tab: le righe
riguardano anche worktree chiusi, quindi non si può assumere che il worktree sia
già montato.

```svelte
<!--
  Perche' questo pannello puo' essere globale, e in Swift non poteva.
  Nell app Swift `openWorktreeIds` decide quali host terminale restano montati,
  e smontarne uno termina i suoi PTY: un pannello che elencasse agenti di
  worktree chiusi elencherebbe processi morti.
  La Fase 2 della migrazione ha spostato lo stato del terminale nel main — i
  pane vivono li' e il renderer tiene solo quello visibile — quindi un agente in
  un worktree chiuso e' ancora vivo e ancora interrogabile, e la riga che lo
  mostra e' vera. Non e' una funzione aggiunta: e' una restrizione
  dell architettura precedente che e' caduta.
-->
```

- [ ] **Passo 2: verificare a mano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: aperti due worktree con un agente ciascuno, chiuderne uno; il pannello
continua a elencare entrambi, e cliccare quello chiuso lo riapre sul tab giusto.

- [ ] **Passo 3: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 4: commit**

```bash
git add src/renderer/src/lib/workspace/
git commit -m "feat: pannello agenti su tutti i worktree"
```

---

## Task 14: temi e trasparenza

**File:**
- Crea: `src/renderer/src/lib/theme.svelte.ts`
- Modifica: `src/renderer/src/assets/` (token CSS)
- Modifica: `src/main/index.ts`

**Interfacce:**
- Consuma: impostazione utente `appearance: 'system' | 'light' | 'dark'`.
- Produce: token CSS custom; `nativeTheme.themeSource` allineato.

**Contesto:** **due meccanismi, non uno.** `nativeTheme.themeSource` governa il
chrome nativo — semafori, menu contestuali — e l'attributo sul `<html>` governa
i token. Impostando solo il secondo, in tema chiaro restano i semafori scuri e
sembra un glitch di rendering.

- [ ] **Passo 1: definire i token**

Un blocco `:root` e uno `[data-theme='light']`, con almeno:
`--tiller-topbar-height: 34px`, `--tiller-statusbar-height: 24px`,
`--tiller-surface-chrome`, `--tiller-surface-raised`, `--tiller-text-primary`,
`--tiller-text-secondary`, `--tiller-status-attention`,
`--tiller-status-error`, `--tiller-status-running`, `--tiller-status-muted`.

- [ ] **Passo 2: allineare il tema nativo**

In `src/main/index.ts`, alla lettura dell'impostazione:

```ts
nativeTheme.themeSource = appearance // 'system' | 'light' | 'dark'
```

- [ ] **Passo 3: vibrancy solo sul chrome**

Sidebar, barra superiore e barra inferiore ricevono la superficie traslucida.
**L'area dei riquadri no.**

```
// xterm.js disegna su <canvas>, che e' opaco: una vibrancy dietro un canvas
// opaco costa il blur della finestra senza che si veda nulla. Nell app Swift
// `#1F1F26` con blur 20 aveva senso perche' li' il terminale e' libghostty su
// Metal e il compositing e' diverso. Si porta la decisione, non la costante.
```

- [ ] **Passo 4: verificare a mano**

Comando: `pnpm exec electron-vite build && ./node_modules/.bin/electron .`
Atteso: passando a tema chiaro cambiano **sia** i token **sia** i semafori.

- [ ] **Passo 5: gate**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add src/renderer/src/ src/main/index.ts
git commit -m "feat: temi chiaro e scuro con chrome nativo allineato"
```

---

## Task 15: i sei criteri end-to-end

**File:**
- Crea: `e2e/fase-4b-viste.spec.ts`

**Interfacce:**
- Consuma: `tillerctl` e i `data-*` posati dai task precedenti
  (`data-group-id`, `data-tab-id`, `data-split-id`, `data-agent-id`).
- Produce: niente verso altri task.

**Contesto:** stesso stampo di `e2e/fase-2-terminale.spec.ts`, con socket **e
database** in cartella temporanea. `TILLER_DB` va sovrascritto quanto
`TILLER_SOCKET`: ometterlo fa girare i test sul database reale dell'utente, ed è
già costato una diagnosi lunga (commit `862e79c`).

Asserzioni sul DOM e sulla geometria. **Niente confronto di screenshot.**

- [ ] **Passo 1: scrivere i sei criteri**

```ts
test('criterio 1: un tab nella barra del titolo si clicca', async () => {
  // Arrange — due tab nello stesso gruppo.
  const window = await app.firstWindow()
  await creaDueTab()

  // Act — click sul secondo.
  await window.locator('[data-tab-id]').nth(1).click()

  // Assert — la trappola che prende: se manca `no-drag`, il click non arriva
  // mai perche' il contenitore e' una regione di trascinamento della finestra.
  await expect(window.locator('[data-tab-id].active')).toHaveCount(1)
  await expect(window.locator('[data-tab-id]').nth(1)).toHaveClass(/active/)
})

test('criterio 2: i confini dei segmenti coincidono con i divisori', async () => {
  // Arrange — split verticale.
  const window = await app.firstWindow()
  await splitVerticale()

  // Act — geometria dichiarata dai due lati.
  const xSegmento = await window.locator('.segment').nth(1).evaluate(
    (el) => Number.parseFloat((el as HTMLElement).style.left)
  )
  const xDivisore = await window.locator('[data-split-id]').first().evaluate(
    (el) => Number.parseFloat((el as HTMLElement).style.left)
  )

  // Assert — entrambi vengono da groupRects/dividerRects della 4a. Se un
  // giorno la barra si misurasse il DOM da sola, qui divergerebbero.
  expect(Math.abs(xSegmento - xDivisore)).toBeLessThan(2)
})

test('criterio 3: lo split orizzontale da una striscia inline al gruppo sotto', async () => {
  const window = await app.firstWindow()
  await splitOrizzontale()

  // Il gruppo inferiore non tocca la barra del titolo: la sua striscia sta
  // sul proprio bordo superiore.
  await expect(window.locator('.inline-host')).toHaveCount(1)
  const top = await window.locator('.inline-host').evaluate(
    (el) => Number.parseFloat((el as HTMLElement).style.top)
  )
  expect(top).toBeGreaterThan(0)
})

test('criterio 4: il rollup mostra needs-input sopra running', async () => {
  // Arrange — due agenti finti nello stesso worktree, uno per stato.
  const window = await app.firstWindow()
  await agenteConStato('running')
  await agenteConStato('needs-input')

  // Assert — sulla riga del worktree, non su quella del tab.
  await expect(
    window.locator('[data-row-kind="worktree"] [data-status]')
  ).toHaveAttribute('data-status', 'needs-input')
})

test('criterio 5: la barra inferiore e larga quanto la finestra', async () => {
  const window = await app.firstWindow()

  const larghezze = await window.evaluate(() => ({
    barra: document.querySelector('.statusbar')!.getBoundingClientRect().width,
    finestra: document.documentElement.clientWidth
  }))

  // V3: oggi in Tiller si ferma al bordo destro del pannello centrale.
  expect(Math.abs(larghezze.barra - larghezze.finestra)).toBeLessThan(2)
})

test('criterio 6: il pannello agenti elenca un worktree chiuso e lo riapre', async () => {
  // Arrange — agente in un secondo worktree, poi il worktree si chiude.
  const window = await app.firstWindow()
  const { worktreeId } = await secondoWorktreeConAgente()
  await tillerctl(['worktree-close', '--worktree', worktreeId])

  // Assert — la riga c e' ancora: il pane vive nel main, non nel renderer.
  const riga = window.locator(`[data-agents-worktree="${worktreeId}"]`)
  await expect(riga).toHaveCount(1)

  // Act — cliccarla riapre il worktree e ci va.
  await riga.click()
  await expect(window.locator('[data-tab-id].active')).toHaveCount(1)
})
```

- [ ] **Passo 2: eseguire gli e2e**

Comando: `pnpm exec electron-vite build && pnpm test:e2e`
Atteso: tutti i criteri passano, più quelli delle fasi precedenti.

- [ ] **Passo 3: validare per mutazione il criterio 1**

Prova che il criterio sa fallire. In `TabSegment.svelte`, togliere `no-drag`
dalla classe del bottone:

```svelte
    <button
      class="tab"
```

Comando: `pnpm exec electron-vite build && pnpm test:e2e -- --grep "criterio 1"`
Atteso: **FAIL** — il click non arriva. Se passa, il criterio sta provando la
presenza dell'elemento e non il click, ed è vacuo: va riscritto.

Poi annullare la mutazione e riverificare:

```bash
git checkout src/renderer/src/lib/workspace/TabSegment.svelte
pnpm exec electron-vite build && pnpm test:e2e
```

- [ ] **Passo 4: validare per mutazione il criterio 4**

In `src/shared/status-rollup.ts`, invertire i primi due elementi della
precedenza:

```ts
const PRECEDENZA: readonly AgentStatus[] = ['error', 'needs-input', 'running', 'done']
```

Comando: `pnpm test:unit -- status-rollup`
Atteso: **FAIL** sul test «needs-input batte error». Poi annullare.

- [ ] **Passo 5: gate completo**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add e2e/
git commit -m "test: i sei criteri end-to-end delle viste del workspace"
```

---

## Checklist manuale (una volta, con la finestra vera)

Gli e2e provano la logica e la geometria; solo l'occhio prova l'aspetto.

- [ ] la finestra ha **una riga sola** in alto, ~34 px, con i tab accanto ai
      semafori e nessuna seconda riga
- [ ] trascinando la barra superiore in un punto vuoto la finestra si sposta
- [ ] cliccando un tab il pane cambia, e il tab **non** trascina la finestra
- [ ] split verticale: i confini dei segmenti cadono esattamente sui divisori
- [ ] split orizzontale: il gruppo inferiore ha la sua striscia, quello
      superiore resta nella barra del titolo
- [ ] la barra inferiore arriva a **entrambi** i bordi della finestra, e la
      sidebar non ha più una riga inferiore propria
- [ ] con dieci worktree aperti si vede a colpo d'occhio quale agente aspetta
- [ ] in tema chiaro cambiano **anche i semafori**, non solo i colori dei
      pannelli
- [ ] la traslucenza si vede su sidebar e barre, e l'area dei terminali è opaca
