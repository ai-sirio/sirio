# Sidebar a tre livelli — piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** l'albero della sidebar mostra identità e stato degli agenti di
tutti i worktree, e `AgentsPanel` viene rimosso senza perdere la sua proprietà.

**Architettura:** nessuna gerarchia nuova. Si arricchisce `buildTree`
(`tree-model.ts`) e le righe di `TreeRow.svelte`; la sorgente dei tab si
generalizza da «solo worktree attivo» a «tutti», usando `model.panes` che è già
globale. Spec: `docs/superpowers/specs/2026-08-03-electron-sidebar-tre-livelli-design.md`.

**Stack:** Svelte 5 (rune), vitest, Playwright e2e con `tillerctl`.

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- Stringhe UI in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo. Non committare `.tokensave/**`.
- **Colori e misure solo dai token** di `assets/tokens.css`
  (`scale-guard.test.ts` fa fallire esadecimali e px fuori scala).
  I colori di stato restano gli alias git già usati da `StatusDot`. **Nessun
  token nuovo.**
- `npm run lint`: 0 errori, warning non oltre il baseline che trovi.
- La verifica «rosso prima» sul singolo criterio: `npx playwright test e2e/sidebar-tre-livelli.spec.ts -g "criterio N"`. L'intero file: due volte in tutto (fine lotto + una per mutazione). `typecheck`/`lint` a fine lotto, una volta se verdi; se rossi, correggi e riesegui finché verdi.
- **Non eseguire `bash scripts/ci.sh`.**

## Punti di realtà (verificati sul codice il 2026-08-03)

- `TabRow` = `{ id, title, status }` in `tree-model.ts`; `buildTree` produce
  nodi `project|worktree|tab|action` con `rollupStatus`.
- `App.svelte:315` costruisce `tabRows` **solo per `worktreeAttivo`**; i pane
  con `agentId`/`status` vivono in `model.panes` (`lib/app-model.svelte.ts`,
  aggiornati dagli eventi: riga ~36).
- `AgentsPanel.svelte` è montato in `App.svelte:628` con `agentRows`
  (`App.svelte:363`) e `revealAgent` (`App.svelte:494`); la colonna 220px entra
  nel calcolo larghezza a `App.svelte:386`.
- `e2e/fase-4b-viste.spec.ts:328` asserisce su `[data-agents-worktree]`.
- `StatusDot.svelte` rende già lo stato con forma+colore su ogni riga.

---

## Task 1: le righe tab di tutti i worktree

**File:**
- Modifica: `src/renderer/src/lib/workspace/tree-model.ts`
- Modifica: `src/renderer/src/lib/workspace/tree-model.test.ts`
- Modifica: `src/renderer/src/App.svelte` (la `$derived` di `tabRows`)

**Interfacce:**
- Produce: `export function righeTabDaPanes(panes: readonly PaneRow[], worktreeId: string): TabRow[]` in `tree-model.ts`, con `interface PaneRow { id: string; worktreeId: string | null; cwd: string; title: string; agentId: string | null; status: AgentStatus | null }` (allineala ai campi veri dei pane di `AppModel` — se differiscono, adegua l'interfaccia e dichiaralo).
- I task 2-5 assumono che `tabPerWorktree` abbia righe per **ogni** worktree con pane vivi, non solo l'attivo.

- [ ] **Passo 1: test rosso**

```ts
test('un worktree non attivo ha comunque le sue righe tab', () => {
  const panes = [
    { id: 'p1', worktreeId: 'w1', cwd: '/a', title: 'claude', agentId: 'claude', status: 'running' as const },
    { id: 'p2', worktreeId: 'w2', cwd: '/b', title: 'shell', agentId: null, status: null }
  ]
  expect(righeTabDaPanes(panes, 'w2')).toEqual([
    { id: 'p2', title: 'shell', status: null, agentId: null }
  ])
})
```

**`TabRow` guadagna `agentId: string | null` in QUESTO task** (il layout senza
pane vivo lo lascia `null`); il Task 2 lo consuma senza ritoccare i tipi.

- [ ] **Passo 2: rosso verificato** (`npx vitest run src/renderer/src/lib/workspace/tree-model.test.ts` — funzione assente)
- [ ] **Passo 3: implementare.** In `App.svelte`, `tabRows` diventa: per il worktree attivo la logica attuale (layout se c'è, fallback pane); per ogni altro worktree con pane vivi, `righeTabDaPanes(model.panes, worktree.id)`.
- [ ] **Passo 4: verde.**
- [ ] **Passo 5:**

```bash
git commit -m "feat: build tab rows for every worktree in the sidebar tree"
```

---

## Task 2: identità dell'agente sulla riga

**File:**
- Modifica: `tree-model.ts` (`TreeNode` + `agentId: string | null`; `TabRow` ce l'ha dal Task 1)
- Crea: `src/renderer/src/lib/workspace/AgentIcon.svelte`
- Modifica: `src/renderer/src/lib/workspace/TreeRow.svelte`
- Crea: `e2e/sidebar-tre-livelli.spec.ts` (criterio 1)

**Interfacce:**
- `AgentIcon.svelte`: `const { agentId }: { agentId: string | null } = $props()`. `agentId === null` → non rende nulla. Monogramma: prima lettera maiuscola (`pi` → `π`, `omp` → `Ω` se vuoi, altrimenti lettera semplice — dichiara la mappa), badge tondo `--t-radius-4`, sfondo `--t-surface-pill`, testo `--t-text-subtitle`, `data-agent={agentId}`.
- `TreeRow` sulla riga `kind === 'tab'`: `<AgentIcon agentId={node.agentId} />` prima del label.

- [ ] **Passo 1: criterio 1 rosso**

```
criterio 1: la riga del tab di un pane agente mostra il monogramma del SUO agente
```

Fixture: `tillerctl` crea worktree + pane, poi registra l'agente sul pane (usa
il comando già usato da `e2e/fase-3-agenti.spec.ts` per assegnare
agentId/status — leggi lì la forma esatta). Asserzione sull'identità:

```ts
await expect(
  window.locator(`[data-tab-id="${paneId}"] [data-agent="claude"]`)
).toBeVisible()
```

(Se le righe non hanno `data-tab-id`, aggiungilo in `TreeRow` — serve comunque
ai criteri 3-4.)

- [ ] **Passo 2: rosso verificato** (elemento assente)
- [ ] **Passo 3: implementare** (plumbing `agentId` da `righeTabDaPanes`/layout a `buildTree` a `TreeRow`).
- [ ] **Passo 4: verde sul singolo criterio.**
- [ ] **Passo 5:**

```bash
git commit -m "feat: show the agent identity on tab rows"
```

---

## Task 3: il badge di inattività

**File:**
- Crea: `src/renderer/src/lib/workspace/idle-badge.ts` + `idle-badge.test.ts`
- Modifica: `lib/app-model.svelte.ts` (registrare `changedAt` alla transizione, riga ~36)
- Modifica: `TreeRow.svelte` (badge testo `Zzz`, `data-idle="true"`, colore `--t-text-meta`)

**Interfacce:**
- `export const IDLE_BADGE_MS = 10 * 60 * 1000`
- `export function idleBadge(status: AgentStatus | null, changedAt: number | null, now: number): boolean` — `true` solo se `status === 'done'` e `now - changedAt >= IDLE_BADGE_MS`.
- `AppModel`: i pane guadagnano `statusChangedAt: number | null`, scritto con `Date.now()` **solo quando `status` cambia davvero** (un evento che ribadisce lo stesso stato non azzera l'orologio).

- [ ] **Passo 1: test rossi**

```ts
test('done da oltre la soglia accende il badge', () => {
  expect(idleBadge('done', 0, IDLE_BADGE_MS)).toBe(true)
})
test('done fresco no', () => {
  expect(idleBadge('done', 0, IDLE_BADGE_MS - 1)).toBe(false)
})
test('running non accende mai, comunque vecchio', () => {
  expect(idleBadge('running', 0, IDLE_BADGE_MS * 2)).toBe(false)
})
test('senza orologio niente badge', () => {
  expect(idleBadge('done', null, IDLE_BADGE_MS * 2)).toBe(false)
})
```

- [ ] **Passo 2: rosso.** - [ ] **Passo 3: implementare.** - [ ] **Passo 4: verde.**
- [ ] **Passo 5: nessun criterio e2e, ed è dichiarato in spec** (aspettare 10 minuti in Playwright non verifica niente). Il rendering in `TreeRow` usa `idleBadge(...)` con `Date.now()` aggiornato da un tick lento (`setInterval` 60s, cancellato in teardown del componente).

```bash
git commit -m "feat: show an idle badge on long-quiet agent rows"
```

---

## Task 4: il contatore sull'intestazione collassata

**File:**
- Modifica: `tree-model.ts` (conteggio needs-input per nodo)
- Modifica: `tree-model.test.ts`
- Modifica: `TreeRow.svelte` / `SidebarTree.svelte` (badge visibile solo su nodo collassato)
- Modifica: `e2e/sidebar-tre-livelli.spec.ts` (criteri 2 e 3)

**Interfacce:**
- `TreeNode` guadagna `needsInputCount: number` (0 sui nodi `tab`/`action`; somma dei discendenti su `worktree`/`project`).
- `TreeRow` mostra `<span class="attention-count" data-attention-count={n}>` accanto al `StatusDot` **solo se il nodo è collassato e `n > 0`** (lo stato di espansione vive già in `SidebarTree` — passa `expanded` a `TreeRow` se non arriva già).

- [ ] **Passo 1: unit rosso** (`buildTree` con due tab `needs-input` in un worktree → `needsInputCount === 2` sul worktree e sul progetto), **criteri 2-3 rossi**

```
criterio 2: needs-input in un worktree collassato fa comparire il contatore, e due lo portano a 2
criterio 3: espandere il worktree fa sparire il contatore e mostra il pallino sulla riga giusta
```

- [ ] **Passo 2: rossi verificati.** - [ ] **Passo 3: implementare.** - [ ] **Passo 4: verdi.**
- [ ] **Passo 5:**

```bash
git commit -m "feat: count waiting agents on collapsed tree rows"
```

---

## Task 5: rimozione di AgentsPanel

**File:**
- Elimina: `src/renderer/src/lib/workspace/AgentsPanel.svelte`
- Modifica: `App.svelte` (via `agentRows` la colonna a :386 e il mount a :627-628; `revealAgent` **resta** e diventa l'azione del click sulla riga tab dell'albero per worktree non attivi)
- Modifica: `e2e/fase-4b-viste.spec.ts` (il criterio a :328 migra sull'albero)
- Modifica: `e2e/sidebar-tre-livelli.spec.ts` (criterio 4)

- [ ] **Passo 1: criterio 4 rosso**

```
criterio 4: un pane agente in un worktree non attivo compare nell'albero col suo
stato, il click porta a quel worktree e quel pane, e [aria-label="Agents"] non
esiste più nel DOM
```

- [ ] **Passo 2: rosso verificato** (oggi il pannello esiste ancora).
- [ ] **Passo 3: implementare.** Il criterio migrato in `fase-4b-viste.spec.ts` conserva la **proprietà** (agente altrove visibile e raggiungibile), misurata sull'albero: riscrivi l'asserzione, non cancellarla.
- [ ] **Passo 4: verdi (criterio 4 + fase-4b intero file).**
- [ ] **Passo 5:**

```bash
git commit -m "feat: absorb the agents panel into the sidebar tree"
```

---

## Task 6: chiusura del lotto

- [ ] `npx vitest run src/renderer/` — exit 0, **zero `Errors`** nel riepilogo.
- [ ] `npx electron-vite build && npx playwright test e2e/sidebar-tre-livelli.spec.ts` — una volta.
- [ ] **Mutazioni** (una alla volta, ripristinando dopo ognuna; e2e completo del file per ciascuna):
  - (a) `agentId` non propagato in `buildTree` (forzalo a `null`) → **solo** criterio 1 rosso.
  - (b) `needsInputCount` forzato a 0 → criterio 2 rosso (e il 3 nella metà contatore).
  - (c) mutazione unit, senza e2e: `idleBadge` che ignora la soglia → i suoi test rossi.
- [ ] `npm run typecheck` e `npm run lint` — una volta se verdi; se rossi, correggi e riesegui finché verdi.
- [ ] **Cattura schermata** (file temporaneo `.spec.ts` in `e2e/`, poi cancellato): albero con 2 progetti, un worktree collassato col contatore, un tab con monogramma. Allegala al report.

## Report finale

I commit; output verbatim delle verifiche; la prova che ogni criterio era rosso
prima; l'esito delle tre mutazioni; ogni punto del piano trovato sbagliato,
**detto invece che aggirato**; cosa non sei riuscito a verificare.

**Fuori scopo:** icone vere degli agenti, selezione a card, tab bar con icone,
respiro tipografico (rifinitura visiva, fase a parte); riordino per stato
(respinto); qualunque secondo albero per stato.
