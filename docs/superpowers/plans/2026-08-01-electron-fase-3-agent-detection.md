# Fase 3 — Agent detection: piano di implementazione

> **Per esecutori agentici:** SOTTO-SKILL RICHIESTA: usare
> superpowers:subagent-driven-development (consigliato) o
> superpowers:executing-plans per implementare un task alla volta.

**Obiettivo:** portare i quattro layer di rilevamento dell'attività degli agenti
nel main Electron, con lo stato leggibile da `tillerctl state` senza alcuna UI.

**Architettura:** nove moduli di logica pura in `src/main/agents/detection/`
(funzioni su stringhe, testabili senza PTY né tempo) più un coordinatore che è
l'unico a conoscere timer e processi. L'identità dell'agente viene dalla tabella
dei processi; il titolo OSC la fornisce solo quando è inequivocabile.

**Stack:** TypeScript, Zod, vitest, Playwright, node-pty, `ps` di sistema.

**Repo:** `~/Desktop/Progetti/tiller-electron`. Il repo Swift
`~/Desktop/Progetti/tiller` è **sola lettura**: non modificarlo mai.

**Spec:** `docs/2026-08-01-electron-fase-3-agent-detection-design.md` (nel repo
Electron).

## Vincoli globali

- Ogni file sotto `src/main/agents/detection/` **non deve importare** `electron`
  né `node:net` né `node:child_process`. Sono funzioni pure. `process-table.ts`
  è l'unica eccezione parziale: riceve l'esecutore di comando iniettato, non lo
  importa.
- **Mai indebolire un tipo di produzione per far compilare un finto di test.**
  Se un finto non soddisfa più un'interfaccia, si aggiorna il finto.
- Commenti e messaggi in italiano; stringhe rivolte all'utente in inglese solo
  se compaiono nella UI (in Fase 3 non ce ne sono).
- Commit in stile Conventional Commits, oggetto minuscolo all'imperativo.
- Gate unico: `bash scripts/ci.sh` deve stampare `CI OK`.
- Baseline all'inizio della fase: **244 test unitari, 9 e2e, gate verde.**
- Le stringhe usate come fixture sono **registrazioni reali** in
  `docs/catture-agenti-2026-08-01/*.json`. Non inventarne di nuove: se serve un
  caso non registrato, dichiararlo nel commento del test.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `src/main/agents/detection/agent-status.ts` | vocabolario `AgentStatus` + priorità |
| `src/main/agents/detection/signal-merger.ts` | finestra di debounce Layer A → B |
| `src/main/agents/detection/title-normalize.ts` | collassa i fotogrammi animati |
| `src/main/agents/detection/title-identity.ts` | identità dal titolo, solo se inequivocabile |
| `src/main/agents/detection/title-status.ts` | stato dal titolo, identità già nota |
| `src/main/agents/detection/content-status.ts` | Layer C: stato dal contenuto |
| `src/main/agents/detection/process-table.ts` | una `ps` → mappa pid, figli di un pid |
| `src/main/agents/detection/process-identity.ts` | `comm`/`args` → agentId |
| `src/main/agents/detection/activity-model.ts` | macchina a stati, proprietà del pane |
| `src/main/agents/detection-coordinator.ts` | inneschi: titolo, quiete, cambio processo |
| `src/main/agents/tillerctl-shim.ts` | shim a path stabile per gli hook |
| `e2e/fixtures/fake-agents/*` | agenti finti costruiti dalle catture |
| `e2e/fase-3-detection.spec.ts` | i quattro criteri |

Modificati: `src/shared/protocol.ts`, `src/main/state/app-state.ts`,
`src/main/control/dispatch.ts`, `src/main/pty/pty-spawner.ts`,
`src/main/pty/pty-manager.ts`, `src/main/index.ts`, `cli/args.ts`,
`src/main/agents/catalog.ts`, `src/main/agents/adapters/*.ts`,
`src/renderer/src/lib/app-model.svelte.ts`.

---

## Task 1: vocabolario di stato e debounce fra layer

**File:**
- Crea: `src/main/agents/detection/agent-status.ts`
- Crea: `src/main/agents/detection/signal-merger.ts`
- Crea: `src/main/agents/detection/signal-merger.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `type AgentStatus = 'running' | 'needs-input' | 'done' | 'error'`;
  `shouldApplyTitleSignal(lastHookAt: number | null, now: number, debounceMs?: number): boolean`;
  `HOOK_DEBOUNCE_MS = 1500`.

**Nota sull'ambito:** l'aggregazione per worktree (priorità fra pane di uno
stesso worktree) **non** fa parte della Fase 3: il suo unico consumatore è il
badge in sidebar, che arriva in Fase 4. `agent-status.ts` qui contiene solo il
tipo, senza test propri — un alias di tipo non ha comportamento da provare.

- [ ] **Passo 1: scrivere il test che fallisce**

`src/main/agents/detection/signal-merger.test.ts`:

```ts
import { expect, test } from 'vitest'
import { shouldApplyTitleSignal, HOOK_DEBOUNCE_MS } from './signal-merger'

test('senza hook precedenti il titolo si applica sempre', () => {
  expect(shouldApplyTitleSignal(null, 10_000)).toBe(true)
})

test('un hook recente sopprime il titolo', () => {
  expect(shouldApplyTitleSignal(10_000, 10_000 + HOOK_DEBOUNCE_MS - 1)).toBe(false)
})

test('scaduta la finestra il titolo torna autorevole', () => {
  expect(shouldApplyTitleSignal(10_000, 10_000 + HOOK_DEBOUNCE_MS)).toBe(true)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- src/main/agents/detection`
Atteso: FAIL, moduli inesistenti.

- [ ] **Passo 3: implementare**

`src/main/agents/detection/agent-status.ts`:

```ts
/** Stato del ciclo di vita di un agente in un pane. */
export type AgentStatus = 'running' | 'needs-input' | 'done' | 'error'
```

`src/main/agents/detection/signal-merger.ts`:

```ts
/**
 * Finestra entro cui un hook esplicito (Layer A) resta autorevole e sopprime
 * un segnale derivato dal titolo (Layer B), cosi un glifo in ritardo non
 * sovrascrive un evento piu fresco. Stesso valore dell'app Swift.
 */
export const HOOK_DEBOUNCE_MS = 1500

export function shouldApplyTitleSignal(
  lastHookAt: number | null,
  now: number,
  debounceMs: number = HOOK_DEBOUNCE_MS
): boolean {
  if (lastHookAt === null) return true
  return now - lastHookAt >= debounceMs
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- src/main/agents/detection`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/agent-status.ts src/main/agents/detection/signal-merger.ts src/main/agents/detection/signal-merger.test.ts
git commit -m "feat: vocabolario di stato agente e debounce fra layer"
```

---

## Task 2: normalizzazione dei titoli animati

**File:**
- Crea: `src/main/agents/detection/title-normalize.ts`
- Crea: `src/main/agents/detection/title-normalize.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce: `BRAILLE_RANGE = /[⠀-⣿]/`;
  `containsBrailleSpinner(title: string): boolean`;
  `startsWithBrailleSpinner(title: string): boolean`;
  `normalizeTitle(title: string): string`.

**Contesto:** omp riscrive il titolo a ogni fotogramma dello spinner (`π ⠼`,
`π ⠹`, `π ⠦`, `π ⠸`, `π ⠋`, `π ⠴` osservati in 12 secondi). Sostituendo il
carattere braille con uno fisso, i fotogrammi diventano titoli identici e
`AppState.setPaneTitle` — che gia scarta i titoli invariati — assorbe la raffica
senza codice nuovo.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { containsBrailleSpinner, startsWithBrailleSpinner, normalizeTitle } from './title-normalize'

// Fotogrammi reali registrati da omp il 2026-08-01.
const FOTOGRAMMI_OMP = [
  'π ⠼ Crea file prova.txt con ciao',
  'π ⠹ Crea file prova.txt con ciao',
  'π ⠦ Crea file prova.txt con ciao',
  'π ⠸ Crea file prova.txt con ciao'
]

test('fotogrammi diversi dello stesso lavoro normalizzano allo stesso titolo', () => {
  const normalizzati = new Set(FOTOGRAMMI_OMP.map(normalizeTitle))
  expect(normalizzati.size).toBe(1)
})

test('la normalizzazione preserva il testo del compito', () => {
  expect(normalizeTitle('π ⠼ Crea file prova.txt con ciao')).toContain('Crea file prova.txt con ciao')
})

test('un titolo senza spinner passa invariato', () => {
  expect(normalizeTitle('✳ Claude Code')).toBe('✳ Claude Code')
  expect(normalizeTitle('π > cap-omp-EafzYT')).toBe('π > cap-omp-EafzYT')
})

test('riconosce lo spinner in posizione iniziale e altrove', () => {
  // Claude al lavoro: lo spinner apre il titolo.
  expect(startsWithBrailleSpinner('⠂ Claude Code')).toBe(true)
  // omp al lavoro: lo spinner segue il glifo pi.
  expect(startsWithBrailleSpinner('π ⠼ Crea file')).toBe(false)
  expect(containsBrailleSpinner('π ⠼ Crea file')).toBe(true)
  expect(containsBrailleSpinner('OpenCode')).toBe(false)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- title-normalize`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
/** Blocco Unicode Braille Patterns: ogni CLI ci pesca i propri spinner. */
export const BRAILLE_RANGE = /[⠀-⣿]/u

/** Fotogramma fisso su cui collassare tutti gli altri. */
const FOTOGRAMMA_STABILE = '⣿'

export function containsBrailleSpinner(title: string): boolean {
  return BRAILLE_RANGE.test(title)
}

/**
 * Vero quando il primo carattere non-spazio del titolo e uno spinner: e la
 * convenzione di Claude al lavoro (`⠂ Claude Code`), distinta da omp che lo
 * mette dopo il proprio glifo (`π ⠼ …`).
 */
export function startsWithBrailleSpinner(title: string): boolean {
  const primo = title.trimStart()[0]
  return primo !== undefined && BRAILLE_RANGE.test(primo)
}

/**
 * Collassa i fotogrammi dello spinner in uno solo. omp cambia titolo a ogni
 * fotogramma: senza questo, ogni fotogramma diventerebbe un evento di stato.
 */
export function normalizeTitle(title: string): string {
  return title.replace(new RegExp(BRAILLE_RANGE.source, 'gu'), FOTOGRAMMA_STABILE)
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- title-normalize`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/title-normalize.ts src/main/agents/detection/title-normalize.test.ts
git commit -m "feat: normalizzazione dei titoli agente animati"
```

---

## Task 3: identità dal titolo

**File:**
- Crea: `src/main/agents/detection/title-identity.ts`
- Crea: `src/main/agents/detection/title-identity.test.ts`

**Interfacce:**
- Consuma: `startsWithBrailleSpinner` da `./title-normalize`.
- Produce: `identifyFromTitle(title: string): string | null`;
  `isPiFamilyTitle(title: string): boolean`.

**Contesto — la decisione che conta:** un titolo che inizia con `π` dichiara la
*famiglia* pi, non quale membro. L'app Swift prova a distinguere omp con
`contains("π:")`, ma omp scrive `π > `: il controllo fallisce e omp viene
classificato come pi. Orca rinuncia e fonde i due in un'unica etichetta. Qui
`identifyFromTitle` restituisce **`null`** per la famiglia pi e lascia decidere
al Layer D, che legge il nome dell'eseguibile e non sbaglia.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { identifyFromTitle, isPiFamilyTitle } from './title-identity'

// Tutti i titoli qui sotto sono registrazioni reali del 2026-08-01.

test('i prefissi di Claude sono inequivocabili', () => {
  expect(identifyFromTitle('✳ Claude Code')).toBe('claude')
  expect(identifyFromTitle('✳ Creare file prova.txt con parola ciao')).toBe('claude')
  expect(identifyFromTitle('⠂ Claude Code')).toBe('claude')
  expect(identifyFromTitle('✳')).toBe('claude')
})

test('OpenCode si riconosce sia fermo sia al lavoro', () => {
  expect(identifyFromTitle('OpenCode')).toBe('opencode')
  // Al lavoro il nome sparisce e resta la sigla: senza questo ramo,
  // OpenCode perde identita' proprio mentre sta lavorando.
  expect(identifyFromTitle('OC | Creazione file prova.txt con parola ciao')).toBe('opencode')
})

test('un titolo della famiglia pi non assegna identita: decide il Layer D', () => {
  expect(identifyFromTitle('π - cap-pi-0aPG6D')).toBeNull()
  expect(identifyFromTitle('π > cap-omp-EafzYT')).toBeNull()
  expect(identifyFromTitle('π ⠼ Crea file prova.txt con ciao')).toBeNull()
  expect(isPiFamilyTitle('π > cap-omp-EafzYT')).toBe(true)
  expect(isPiFamilyTitle('✳ Claude Code')).toBe(false)
})

test('il nome di un agente conta solo come parola intera', () => {
  expect(identifyFromTitle('codex')).toBe('codex')
  // Un titolo di cwd non e un agente.
  expect(identifyFromTitle('~/codex-notes')).toBeNull()
  expect(identifyFromTitle('opencode-experiment')).toBeNull()
})

test('un titolo vuoto o di shell non identifica nulla', () => {
  expect(identifyFromTitle('')).toBeNull()
  expect(identifyFromTitle('zsh in tmp')).toBeNull()
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- title-identity`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import { startsWithBrailleSpinner } from './title-normalize'

const CLAUDE_IDLE = '✳' // ✳
const PI_GLYPH = 'π' // π

/** Vero quando il titolo appartiene alla famiglia pi (pi o un suo fork). */
export function isPiFamilyTitle(title: string): boolean {
  return title.trimStart().startsWith(PI_GLYPH)
}

/** Corrispondenza su parola intera: `codex-notes` non e Codex. */
function hasWord(word: string, text: string): boolean {
  return new RegExp(`(^|[^a-z0-9])${word}([^a-z0-9]|$)`, 'i').test(text)
}

/**
 * Identita dell'agente ricavata dal solo titolo, senza registrazione
 * preventiva: serve a riconoscere un agente lanciato a mano.
 *
 * Restituisce un id SOLO quando il titolo e inequivocabile. La famiglia pi
 * non lo e: il separatore dopo il glifo e cosmetico ed e gia cambiato una
 * volta (`π:` → `π >`), quindi l'identita la assegna il Layer D leggendo il
 * nome dell'eseguibile.
 */
export function identifyFromTitle(title: string): string | null {
  if (title.length === 0) return null

  // 1. I prefissi di Claude non appartengono a nessun altro.
  if (title === CLAUDE_IDLE || title.startsWith(`${CLAUDE_IDLE} `)) return 'claude'
  if (startsWithBrailleSpinner(title)) return 'claude'

  // 2. Famiglia pi: riconosciuta ma non attribuita.
  if (isPiFamilyTitle(title)) return null

  // 3. OpenCode: nome intero da fermo, sigla mentre lavora.
  if (title.trim() === 'OpenCode' || title.startsWith('OC | ')) return 'opencode'

  // 4. Nomi letterali, su parola intera.
  for (const id of ['codex', 'opencode']) {
    if (hasWord(id, title)) return id
  }

  return null
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- title-identity`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/title-identity.ts src/main/agents/detection/title-identity.test.ts
git commit -m "feat: identita agente dal titolo, famiglia pi delegata al layer processi"
```

---

## Task 4: stato dal titolo

**File:**
- Crea: `src/main/agents/detection/title-status.ts`
- Crea: `src/main/agents/detection/title-status.test.ts`

**Interfacce:**
- Consuma: `AgentStatus` da `./agent-status`; `containsBrailleSpinner`,
  `startsWithBrailleSpinner` da `./title-normalize`; `isPiFamilyTitle` da
  `./title-identity`.
- Produce: `statusFromTitle(title: string, agentId: string): AgentStatus | null`.

**Contesto:** `null` significa "il titolo non ha un'opinione" e non deve **mai**
forzare una transizione. Il ramo `hasPrefix(". ")` dell'app Swift non compare in
nessuna cattura e non va portato.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { statusFromTitle } from './title-status'

test('Claude: glifo fermo = in attesa, spinner iniziale = al lavoro', () => {
  expect(statusFromTitle('✳ Claude Code', 'claude')).toBe('needs-input')
  expect(statusFromTitle('✳ Creare file prova.txt con parola ciao', 'claude')).toBe('needs-input')
  expect(statusFromTitle('⠂ Claude Code', 'claude')).toBe('running')
  expect(statusFromTitle('⠂ Creare file prova.txt con parola ciao', 'claude')).toBe('running')
})

test('famiglia pi: spinner ovunque = al lavoro, glifo nudo = in attesa', () => {
  expect(statusFromTitle('π ⠼ Crea file prova.txt con ciao', 'omp')).toBe('running')
  expect(statusFromTitle('π > Crea file prova.txt con ciao', 'omp')).toBe('needs-input')
  expect(statusFromTitle('π - cap-pi-0aPG6D', 'pi')).toBe('needs-input')
})

test('OpenCode: sigla = al lavoro, nome intero = in attesa', () => {
  expect(statusFromTitle('OC | Creazione file prova.txt con parola ciao', 'opencode')).toBe('running')
  expect(statusFromTitle('OpenCode', 'opencode')).toBe('needs-input')
})

test('Codex non emette titolo: nessuna opinione', () => {
  expect(statusFromTitle('', 'codex')).toBeNull()
  expect(statusFromTitle('qualsiasi cosa', 'codex')).toBeNull()
})

test('un titolo estraneo non forza nulla', () => {
  expect(statusFromTitle('zsh in tmp', 'claude')).toBeNull()
  expect(statusFromTitle('', 'claude')).toBeNull()
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- title-status`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { AgentStatus } from './agent-status'
import { containsBrailleSpinner, startsWithBrailleSpinner } from './title-normalize'
import { isPiFamilyTitle } from './title-identity'

const CLAUDE_IDLE = '✳' // ✳

/**
 * Stato ricavato dal titolo per un agente di identita gia nota.
 *
 * `null` significa "il titolo non ha un'opinione" e non deve mai forzare una
 * transizione: e sempre l'esito sicuro. Ogni riga qui sotto corrisponde a una
 * convenzione registrata dal vivo il 2026-08-01, non a una supposizione.
 */
export function statusFromTitle(title: string, agentId: string): AgentStatus | null {
  if (title.length === 0) return null

  switch (agentId) {
    case 'claude':
      // Nessun ramo `". "`: quella convenzione non compare nelle catture.
      if (title === CLAUDE_IDLE || title.startsWith(`${CLAUDE_IDLE} `)) return 'needs-input'
      if (startsWithBrailleSpinner(title)) return 'running'
      return null

    case 'pi':
    case 'omp':
      if (!isPiFamilyTitle(title)) return null
      return containsBrailleSpinner(title) ? 'running' : 'needs-input'

    case 'opencode':
      if (title.startsWith('OC | ')) return 'running'
      if (title.trim() === 'OpenCode') return 'needs-input'
      return null

    // Codex non scrive alcun titolo: il suo stato arriva dagli hook e dal
    // Layer D, mai da qui.
    default:
      return null
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- title-status`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/title-status.ts src/main/agents/detection/title-status.test.ts
git commit -m "feat: stato agente dal titolo osc"
```

---

## Task 5: stato dal contenuto (Layer C)

**File:**
- Crea: `src/main/agents/detection/content-status.ts`
- Crea: `src/main/agents/detection/content-status.test.ts`

**Interfacce:**
- Consuma: `AgentStatus` da `./agent-status`.
- Produce: `statusFromContent(tail: string, agentId: string): AgentStatus | null`;
  `CONTENT_TAIL_LINES = 40`.

**Contesto — il caso che giustifica l'astensione:** nessuna delle otto stringhe
cercate dallo `ScreenManifest` Swift compare nell'output reale di alcun agente.
La barra di stato di OpenCode contiene `esc interrupt` **sempre**, anche a
riposo: una regola generica lo marcherebbe come eternamente occupato, e nessun
altro segnale lo correggerebbe perche OpenCode non ha hook nativi. Le astensioni
sono parte della specifica.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { statusFromContent, CONTENT_TAIL_LINES } from './content-status'

// Righe reali registrate il 2026-08-01.
const PERMESSO_CLAUDE = [
  'Do you want to create prova.txt?',
  '1. Yes',
  '2. Yes, allow all edits during this session (shift+tab)',
  'Esc to cancel · Tab to amend'
].join('\n')

const BARRA_OPENCODE =
  '⬝⬝■■■■■■ esc interrupt tab agents ctrl+p commands • OpenCode 1.18.0'

const LAVORO_OMP = ['⠏ Working… ⟨esc⟩', '⠼ Creazione file prova.txt ⟨esc⟩'].join('\n')

test('Claude: la richiesta di permesso e attesa, qualunque sia l azione', () => {
  expect(statusFromContent(PERMESSO_CLAUDE, 'claude')).toBe('needs-input')
  expect(statusFromContent('Do you want to run `ls`?', 'claude')).toBe('needs-input')
})

test('Claude: senza richiesta non c e opinione', () => {
  expect(statusFromContent('Model: Sonnet 5  Context: 0/1.0M', 'claude')).toBeNull()
})

test('omp: la riga di lavoro con spinner e parentesi esc', () => {
  expect(statusFromContent(LAVORO_OMP, 'omp')).toBe('running')
  expect(statusFromContent('│ │ No recent sessions │', 'omp')).toBeNull()
})

test('OpenCode: nessuna regola, perche la barra dice sempre esc interrupt', () => {
  // Se questo test diventasse `running`, OpenCode risulterebbe occupato per
  // sempre: non ha hook nativi che possano correggere lo stato.
  expect(statusFromContent(BARRA_OPENCODE, 'opencode')).toBeNull()
})

test('codex e pi non hanno regole di contenuto', () => {
  expect(statusFromContent('qualsiasi output', 'codex')).toBeNull()
  expect(statusFromContent('qualsiasi output', 'pi')).toBeNull()
})

test('la finestra e larga abbastanza da superare la barra di stato', () => {
  // Con 6 righe la richiesta di Claude e invisibile: sotto c e solo la barra.
  expect(CONTENT_TAIL_LINES).toBeGreaterThanOrEqual(30)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- content-status`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { AgentStatus } from './agent-status'

/**
 * Righe di coda da esaminare. Non e un numero scelto a occhio: con 6 righe la
 * richiesta di permesso di Claude e invisibile, perche sotto di essa c e solo
 * la barra di stato fissa della TUI; con 30 compare. 40 e quel confine
 * misurato piu margine.
 */
export const CONTENT_TAIL_LINES = 40

/** Claude chiede permesso nominando l azione: "Do you want to create X?". */
const CLAUDE_PERMESSO = /Do you want to .+\?/

/** omp segna il lavoro con spinner in testa e ⟨esc⟩ in coda alla riga. */
const OMP_LAVORO = /^[⠀-⣿]\s.*⟨esc⟩\s*$/u

/**
 * Stato ricavato dal contenuto di schermo (Layer C).
 *
 * Solo regole ancorate per agente, mai un matcher generico su testo libero.
 * Un match sbagliato e peggio di nessun match: `null` non forza mai una
 * transizione, mentre uno stato errato su un agente senza hook nativi non
 * viene corretto da nessuno.
 */
export function statusFromContent(tail: string, agentId: string): AgentStatus | null {
  if (tail.length === 0) return null

  switch (agentId) {
    case 'claude':
      return CLAUDE_PERMESSO.test(tail) ? 'needs-input' : null

    case 'omp':
      return tail.split('\n').some((riga) => OMP_LAVORO.test(riga.trim())) ? 'running' : null

    // Astensioni deliberate, non lacune:
    // - opencode: la sua barra di stato contiene `esc interrupt` in ogni
    //   istante, anche a riposo. Qualsiasi regola su quel testo lo bloccherebbe
    //   su "al lavoro" per sempre.
    // - codex, pi: comportamento non osservato nelle catture del 2026-08-01.
    default:
      return null
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- content-status`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/content-status.ts src/main/agents/detection/content-status.test.ts
git commit -m "feat: stato agente dal contenuto di schermo con astensioni dichiarate"
```

---

## Task 6: tabella dei processi e identità dal processo

**File:**
- Crea: `src/main/agents/detection/process-table.ts`
- Crea: `src/main/agents/detection/process-table.test.ts`
- Crea: `src/main/agents/detection/process-identity.ts`
- Crea: `src/main/agents/detection/process-identity.test.ts`

**Interfacce:**
- Consuma: niente.
- Produce:
  `interface ProcessRow { pid: number; ppid: number; comm: string; args: string }`;
  `parseProcessTable(output: string): ProcessRow[]`;
  `childrenOf(rows: readonly ProcessRow[], pid: number): ProcessRow[]`;
  `type ProcessLister = () => Promise<string>`;
  `createPsLister(exec: (file: string, args: string[]) => Promise<string>): ProcessLister`;
  `PS_ARGS: readonly string[]`;
  `recognizeAgent(comm: string, args: string): string | null`.

**Contesto — perche una sola scansione:** in Swift `proc_listchildpids` costa
microsecondi, quindi scansionare per pane e gratis. In Node ogni `ps` e un fork
da 24 ms misurati su 794 processi, ma quella singola tabella contiene gia tutti
i pane. Portare la struttura Swift alla lettera darebbe 20 pane × 24 ms a ogni
raffica di output.

- [ ] **Passo 1: scrivere il test che fallisce**

`src/main/agents/detection/process-table.test.ts`:

```ts
import { expect, test } from 'vitest'
import { parseProcessTable, childrenOf, createPsLister, PS_ARGS } from './process-table'

// Forma reale dell output di `ps -axo pid=,ppid=,comm=,args=` su macOS.
const OUTPUT = [
  '  501     1 /bin/zsh zsh',
  '  502   501 /Users/x/.bun/bin/bun bun /Users/x/.bun/bin/omp',
  '  503   502 /usr/bin/uv uv run mcp',
  ' 1000     1 /usr/local/bin/claude claude'
].join('\n')

test('interpreta pid, ppid, nome eseguibile e riga di comando', () => {
  const righe = parseProcessTable(OUTPUT)
  expect(righe).toHaveLength(4)
  expect(righe[1]).toEqual({
    pid: 502,
    ppid: 501,
    comm: '/Users/x/.bun/bin/bun',
    args: 'bun /Users/x/.bun/bin/omp'
  })
})

test('righe malformate vengono scartate senza far cadere il resto', () => {
  expect(parseProcessTable('spazzatura\n  501     1 /bin/zsh zsh')).toHaveLength(1)
})

test('i figli diretti si trovano per ppid', () => {
  const righe = parseProcessTable(OUTPUT)
  expect(childrenOf(righe, 501).map((r) => r.pid)).toEqual([502])
  expect(childrenOf(righe, 999)).toEqual([])
})

test('il lister chiama ps una volta sola e restituisce il suo output', async () => {
  const chiamate: string[][] = []
  const lister = createPsLister(async (file, args) => {
    chiamate.push([file, ...args])
    return OUTPUT
  })
  expect(await lister()).toBe(OUTPUT)
  expect(chiamate).toEqual([['ps', ...PS_ARGS]])
})
```

`src/main/agents/detection/process-identity.test.ts`:

```ts
import { expect, test } from 'vitest'
import { recognizeAgent } from './process-identity'

// Coppie (comm, args) misurate il 2026-08-01 lanciando ogni CLI in un pty.
test('gli agenti con eseguibile omonimo si riconoscono dal nome', () => {
  expect(recognizeAgent('/usr/local/bin/claude', 'claude')).toBe('claude')
  expect(recognizeAgent('/opt/homebrew/bin/codex', 'codex')).toBe('codex')
  expect(recognizeAgent('/opt/homebrew/bin/opencode', 'opencode')).toBe('opencode')
  expect(recognizeAgent('/Users/x/.local/bin/pi', 'pi')).toBe('pi')
})

test('omp gira dentro bun e si riconosce dalla riga di comando', () => {
  // Senza questo ramo omp resterebbe "bun" e finirebbe non riconosciuto.
  expect(recognizeAgent('/Users/x/.bun/bin/bun', 'bun /Users/x/.bun/bin/omp')).toBe('omp')
})

test('la riga di comando ha precedenza sul nome eseguibile', () => {
  // `bun` da solo non e un agente; `bun …/omp` lo e.
  expect(recognizeAgent('/Users/x/.bun/bin/bun', 'bun server.ts')).toBeNull()
})

test('un processo qualunque non e un agente', () => {
  expect(recognizeAgent('/bin/zsh', 'zsh')).toBeNull()
  expect(recognizeAgent('/usr/bin/node', 'node index.js')).toBeNull()
  // Un path che contiene il nome ma non lo esegue non basta.
  expect(recognizeAgent('/bin/cat', 'cat /Users/x/codex-notes/appunti.md')).toBeNull()
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- process-`
Atteso: FAIL, moduli inesistenti.

- [ ] **Passo 3: implementare**

`src/main/agents/detection/process-table.ts`:

```ts
export interface ProcessRow {
  pid: number
  ppid: number
  /** Path dell eseguibile come lo riporta `ps`. */
  comm: string
  /** Riga di comando completa. */
  args: string
}

/**
 * `comm` da il nome vero dell eseguibile, `args` la riga di comando: servono
 * entrambi, perche omp gira dentro `bun` e solo `args` lo rivela.
 */
export const PS_ARGS: readonly string[] = ['-axo', 'pid=,ppid=,comm=,args=']

const RIGA = /^\s*(\d+)\s+(\d+)\s+(\S+)\s+(.*)$/

export function parseProcessTable(output: string): ProcessRow[] {
  const righe: ProcessRow[] = []
  for (const linea of output.split('\n')) {
    const m = RIGA.exec(linea)
    // Una riga malformata si scarta: una tabella parziale e comunque utile.
    if (m === null) continue
    righe.push({ pid: Number(m[1]), ppid: Number(m[2]), comm: m[3], args: m[4].trim() })
  }
  return righe
}

export function childrenOf(rows: readonly ProcessRow[], pid: number): ProcessRow[] {
  return rows.filter((riga) => riga.ppid === pid)
}

export type ProcessLister = () => Promise<string>

/**
 * Una sola invocazione di `ps` per l intera tabella: la stessa risposta serve
 * tutti i pane. L esecutore e iniettato perche questo modulo resti puro e i
 * test non forkino processi.
 */
export function createPsLister(
  exec: (file: string, args: string[]) => Promise<string>
): ProcessLister {
  return () => exec('ps', [...PS_ARGS])
}
```

`src/main/agents/detection/process-identity.ts`:

```ts
/** Agenti il cui eseguibile porta il loro stesso nome. */
const ESEGUIBILI_OMONIMI = new Set(['claude', 'codex', 'opencode', 'pi'])

/** Agenti ospitati da un runtime, riconoscibili solo dalla riga di comando. */
const OSPITATI: readonly { id: string; pattern: RegExp }[] = [
  // `bun /Users/…/.bun/bin/omp` — misurato il 2026-08-01.
  { id: 'omp', pattern: /(^|\s|\/)omp(\s|$)/ }
]

function basename(path: string): string {
  return path.split('/').pop() ?? path
}

/**
 * Identita dell agente che gira in un processo (Layer D).
 *
 * L ordine conta: omp va riconosciuto dalla riga di comando PRIMA che `comm`
 * (che dice `bun`) porti fuori strada.
 */
export function recognizeAgent(comm: string, args: string): string | null {
  const eseguibile = basename(args.split(/\s+/)[0] ?? '')
  for (const { id, pattern } of OSPITATI) {
    // Solo il primo token conta come eseguibile ospitante; il resto della riga
    // deve nominare l agente come segmento di path o parola a se.
    const resto = args.slice(eseguibile.length)
    if (pattern.test(basename(resto.trim()))) return id
  }

  const nome = basename(comm)
  if (ESEGUIBILI_OMONIMI.has(nome)) return nome
  return null
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- process-`
Atteso: PASS. Se il ramo `OSPITATI` non distingue `bun server.ts` da
`bun …/omp`, correggere l'implementazione — **non il test**.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/process-table.ts src/main/agents/detection/process-table.test.ts src/main/agents/detection/process-identity.ts src/main/agents/detection/process-identity.test.ts
git commit -m "feat: tabella processi unica e identita agente da comm e riga di comando"
```

---

## Task 7: macchina a stati dell'attività agente

**File:**
- Crea: `src/main/agents/detection/activity-model.ts`
- Crea: `src/main/agents/detection/activity-model.test.ts`

**Interfacce:**
- Consuma: `AgentStatus`, `highestPriority` da `./agent-status`;
  `shouldApplyTitleSignal` da `./signal-merger`; `identifyFromTitle` da
  `./title-identity`; `statusFromTitle` da `./title-status`.
- Produce: `class AgentActivityModel` con:
  - `notify(paneId: string, status: AgentStatus, now: number): void`
  - `agentSpawned(paneId: string, agentId: string, now: number): void`
  - `handleTitle(paneId: string, title: string, now: number): void`
  - `applyContentSignal(paneId: string, status: AgentStatus, now: number): void`
  - `processIdentified(paneId: string, agentId: string): void`
  - `processGone(paneId: string): void`
  - `applyExit(paneId: string, exitCode: number): void`
  - `paneClosed(paneId: string): void`
  - `agentId(paneId: string): string | null`
  - `status(paneId: string): AgentStatus | null`

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { AgentActivityModel } from './activity-model'

test('un hook e autorevole e sopprime il titolo per la finestra di debounce', () => {
  const m = new AgentActivityModel()
  m.agentSpawned('p1', 'claude', 0)
  m.notify('p1', 'running', 1000)
  // Titolo "fermo" in ritardo, entro la finestra: non deve vincere.
  m.handleTitle('p1', '✳ Claude Code', 1400)
  expect(m.status('p1')).toBe('running')
  // Scaduta la finestra il titolo torna autorevole.
  m.handleTitle('p1', '✳ Claude Code', 3000)
  expect(m.status('p1')).toBe('needs-input')
})

test('un pane sconosciuto acquista identita da un titolo inequivocabile', () => {
  const m = new AgentActivityModel()
  m.handleTitle('p1', '✳ Claude Code', 0)
  expect(m.agentId('p1')).toBe('claude')
  expect(m.status('p1')).toBe('needs-input')
})

test('un titolo della famiglia pi non assegna identita: aspetta il layer processi', () => {
  const m = new AgentActivityModel()
  m.handleTitle('p1', 'π > cap-omp-EafzYT', 0)
  expect(m.agentId('p1')).toBeNull()
  m.processIdentified('p1', 'omp')
  expect(m.agentId('p1')).toBe('omp')
  m.handleTitle('p1', 'π ⠼ Crea file', 1000)
  expect(m.status('p1')).toBe('running')
})

test('il layer processi non declassa un pane gia noto a un altro layer', () => {
  const m = new AgentActivityModel()
  m.agentSpawned('p1', 'claude', 0)
  m.notify('p1', 'running', 100)
  m.processIdentified('p1', 'claude')
  expect(m.status('p1')).toBe('running')
})

test('un pane process-owned si azzera solo quando il processo sparisce', () => {
  const m = new AgentActivityModel()
  m.processIdentified('p1', 'codex')
  expect(m.status('p1')).toBe('running')
  // Codex non emette titolo: un titolo estraneo non deve cancellarlo.
  m.handleTitle('p1', 'zsh in tmp', 1000)
  expect(m.agentId('p1')).toBe('codex')
  m.processGone('p1')
  expect(m.agentId('p1')).toBeNull()
  expect(m.status('p1')).toBeNull()
})

test('un titolo che sparisce mentre lavora e transitorio, mentre e fermo significa uscito', () => {
  const m = new AgentActivityModel()
  m.handleTitle('p1', '⠂ Claude Code', 0) // title-owned, al lavoro
  m.handleTitle('p1', 'zsh in tmp', 1000) // titolo estraneo durante il lavoro
  expect(m.agentId('p1')).toBe('claude')

  m.handleTitle('p1', '✳ Claude Code', 2000) // torna fermo
  m.handleTitle('p1', 'zsh in tmp', 3000) // ora sparisce da fermo: uscito
  expect(m.agentId('p1')).toBeNull()
})

test('il contenuto non passa dal debounce e puo correggere uno stato stantio', () => {
  const m = new AgentActivityModel()
  m.agentSpawned('p1', 'claude', 0)
  m.notify('p1', 'running', 1000)
  m.applyContentSignal('p1', 'needs-input', 1100)
  expect(m.status('p1')).toBe('needs-input')
})

test('il contenuto non inventa agenti su pane non riconosciuti', () => {
  const m = new AgentActivityModel()
  m.applyContentSignal('p1', 'needs-input', 0)
  expect(m.agentId('p1')).toBeNull()
  expect(m.status('p1')).toBeNull()
})

test('il codice di uscita e l unica via a done per gli agenti senza hook', () => {
  const m = new AgentActivityModel()
  m.agentSpawned('p1', 'pi', 0)
  m.applyExit('p1', 0)
  expect(m.status('p1')).toBe('done')

  const n = new AgentActivityModel()
  n.agentSpawned('p2', 'pi', 0)
  n.applyExit('p2', 1)
  expect(n.status('p2')).toBe('error')
})

test('un pane mai riconosciuto come agente non acquista stato morendo', () => {
  const m = new AgentActivityModel()
  m.applyExit('p1', 0)
  expect(m.status('p1')).toBeNull()
})

test('chiudere un pane rimuove ogni traccia', () => {
  const m = new AgentActivityModel()
  m.agentSpawned('p1', 'claude', 0)
  m.paneClosed('p1')
  expect(m.agentId('p1')).toBeNull()
  expect(m.status('p1')).toBeNull()
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- activity-model`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { AgentStatus } from './agent-status'
import { shouldApplyTitleSignal } from './signal-merger'
import { identifyFromTitle } from './title-identity'
import { statusFromTitle } from './title-status'

/** Chi ha diritto di azzerare lo stato di un pane. */
type Proprieta = 'spawn' | 'title' | 'process'

interface Pane {
  agentId: string
  status: AgentStatus | null
  proprieta: Proprieta
  lastHookAt: number | null
}

/**
 * Macchina a stati dell attivita degli agenti: fonde i quattro layer di
 * evidenza in un solo stato per pane.
 *
 * Il concetto centrale e la PROPRIETA del pane, cioe chi ha diritto di
 * azzerarne lo stato. Mescolarle reintroduce i bug in cui il segnale (o
 * l assenza di segnale) di un layer cancella cio su cui un altro layer sta
 * ancora contando.
 */
export class AgentActivityModel {
  readonly #panes = new Map<string, Pane>()

  // --- Layer A: hook esplicito ---

  notify(paneId: string, status: AgentStatus, now: number): void {
    const pane = this.#panes.get(paneId)
    if (pane === undefined) return
    this.#panes.set(paneId, { ...pane, status, lastHookAt: now })
  }

  agentSpawned(paneId: string, agentId: string, now: number): void {
    this.#panes.set(paneId, {
      agentId,
      status: 'running',
      proprieta: 'spawn',
      lastHookAt: now
    })
  }

  // --- Layer B: titolo ---

  handleTitle(paneId: string, title: string, now: number): void {
    const pane = this.#panes.get(paneId)

    if (pane === undefined) {
      // Pane sconosciuto: solo un titolo inequivocabile assegna identita.
      const identificato = identifyFromTitle(title)
      if (identificato === null) return
      this.#panes.set(paneId, {
        agentId: identificato,
        status: statusFromTitle(title, identificato) ?? 'running',
        proprieta: 'title',
        lastHookAt: null
      })
      return
    }

    const status = statusFromTitle(title, pane.agentId)
    if (status === null) {
      // Il titolo non somiglia piu a questo agente. Solo un pane title-owned
      // puo sparire cosi, e solo se NON stava lavorando: mentre lavora, una
      // riscrittura momentanea del titolo e transitoria.
      if (pane.proprieta === 'title' && pane.status !== 'running') {
        this.#panes.delete(paneId)
      }
      return
    }

    if (!shouldApplyTitleSignal(pane.lastHookAt, now)) return
    this.#panes.set(paneId, { ...pane, status })
  }

  // --- Layer C: contenuto ---

  applyContentSignal(paneId: string, status: AgentStatus, now: number): void {
    const pane = this.#panes.get(paneId)
    // Il contenuto non identifica: parla solo di pane gia riconosciuti.
    if (pane === undefined || pane.status === status) return
    // Nessun debounce: una corrispondenza di contenuto e piu vicina alla
    // realta di una convenzione di titolo e puo correggere uno stato stantio.
    // Un notify successivo la sovrascrive comunque senza condizioni.
    this.#panes.set(paneId, { ...pane, status, lastHookAt: now })
  }

  // --- Layer D: processo ---

  processIdentified(paneId: string, agentId: string): void {
    // L evidenza di processo dice solo "vivo": non deve mai declassare uno
    // stato piu ricco gia stabilito da un altro layer.
    if (this.#panes.has(paneId)) return
    this.#panes.set(paneId, {
      agentId,
      status: 'running',
      proprieta: 'process',
      lastHookAt: null
    })
  }

  processGone(paneId: string): void {
    const pane = this.#panes.get(paneId)
    if (pane?.proprieta !== 'process') return
    this.#panes.delete(paneId)
  }

  // --- Uscita del processo ---

  applyExit(paneId: string, exitCode: number): void {
    const pane = this.#panes.get(paneId)
    // Un pane mai riconosciuto come agente non acquista uno stato morendo.
    if (pane === undefined) return
    this.#panes.set(paneId, { ...pane, status: exitCode === 0 ? 'done' : 'error' })
  }

  paneClosed(paneId: string): void {
    this.#panes.delete(paneId)
  }

  // --- Letture ---

  agentId(paneId: string): string | null {
    return this.#panes.get(paneId)?.agentId ?? null
  }

  status(paneId: string): AgentStatus | null {
    return this.#panes.get(paneId)?.status ?? null
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- activity-model`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection/activity-model.ts src/main/agents/detection/activity-model.test.ts
git commit -m "feat: macchina a stati dell attivita agente con proprieta del pane"
```

---

## Task 8: il PTY espone pid, processo in primo piano e codice di uscita

**File:**
- Modifica: `src/main/pty/pty-spawner.ts`
- Modifica: `src/main/pty/pty-manager.ts`
- Crea: `src/main/pty/pty-manager.test.ts` (se assente; altrimenti estenderlo)
- Modifica: `src/main/control/dispatch.test.ts` (finto di `pty`)

**Interfacce:**
- Consuma: niente.
- Produce: `PtyHandle` guadagna `readonly pid: number`,
  `foregroundProcess(): string`, e `onExit(cb: (exitCode: number) => void)`.
  `PtyManager` guadagna `pid(paneId): number | null`,
  `foregroundProcess(paneId): string | null`, e `onExit` passa il codice.

**Contesto:** `pty.process` di node-pty non basta come identita (Claude riscrive
il proprio titolo di processo nella versione: misurato `2.1.220`), ma cambia da
`zsh` a qualcos altro all avvio di un agente — ed e un rilevatore di
cambiamento **gratuito** che dice quando vale la pena pagare i 24 ms di `ps`.

- [ ] **Passo 1: scrivere il test che fallisce**

`src/main/pty/pty-manager.test.ts`:

```ts
import { expect, test } from 'vitest'
import { PtyManager } from './pty-manager'
import type { PtyHandle, PtySpawner } from './pty-spawner'

function fakeSpawner(pid: number, processo: string): {
  spawner: PtySpawner
  exit: (code: number) => void
} {
  let onExit: ((code: number) => void) | null = null
  const handle: PtyHandle = {
    pid,
    foregroundProcess: () => processo,
    write: () => {},
    onData: () => {},
    onExit: (cb) => {
      onExit = cb
    },
    kill: () => {},
    resize: () => {}
  }
  return { spawner: () => handle, exit: (code) => onExit?.(code) }
}

const opzioni = { cwd: '/tmp', shell: '/bin/zsh', args: [], cols: 80, rows: 24 }

test('espone pid e processo in primo piano del pane', () => {
  const { spawner } = fakeSpawner(4242, 'claude')
  const m = new PtyManager(spawner)
  m.spawn('p1', opzioni)
  expect(m.pid('p1')).toBe(4242)
  expect(m.foregroundProcess('p1')).toBe('claude')
})

test('un pane sconosciuto non ha pid ne processo', () => {
  const m = new PtyManager(fakeSpawner(1, 'zsh').spawner)
  expect(m.pid('assente')).toBeNull()
  expect(m.foregroundProcess('assente')).toBeNull()
})

test('l uscita porta con se il codice', () => {
  const { spawner, exit } = fakeSpawner(1, 'zsh')
  const m = new PtyManager(spawner)
  const visti: [string, number][] = []
  m.onExit((paneId, code) => visti.push([paneId, code]))
  m.spawn('p1', opzioni)
  exit(3)
  expect(visti).toEqual([['p1', 3]])
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- pty-manager`
Atteso: FAIL — `pid` e `foregroundProcess` non esistono su `PtyHandle`.

- [ ] **Passo 3: implementare**

In `src/main/pty/pty-spawner.ts`, sostituire l'interfaccia `PtyHandle` e il
corpo del ritorno di `createNodePtySpawner`:

```ts
export interface PtyHandle {
  /** Pid del processo esterno del pty (la shell). */
  readonly pid: number
  /**
   * Nome del processo in primo piano secondo node-pty. Non e un identita
   * affidabile — Claude riscrive il proprio titolo di processo nella versione
   * (`2.1.220` misurato) — ma cambia quando il primo piano cambia, ed e
   * gratuito: serve da innesco per la scansione vera.
   */
  foregroundProcess(): string
  write(data: string): void
  onData(cb: (data: string) => void): void
  onExit(cb: (exitCode: number) => void): void
  kill(): void
  resize(cols: number, rows: number): void
}
```

```ts
    return {
      pid: proc.pid,
      foregroundProcess: () => proc.process,
      write: (data) => proc.write(data),
      onData: (cb) => {
        proc.onData(cb)
      },
      onExit: (cb) => {
        proc.onExit(({ exitCode }) => cb(exitCode))
      },
      kill: () => proc.kill(),
      // node-pty chiama il primo parametro `columns`; è posizionale.
      resize: (cols, rows) => proc.resize(cols, rows)
    }
```

In `src/main/pty/pty-manager.ts`, cambiare il tipo dei listener di uscita e
aggiungere due letture:

```ts
type ExitListener = (paneId: string, exitCode: number) => void
```

```ts
    handle.onExit((exitCode) => {
      this.#handles.delete(paneId)
      for (const listener of this.#exitListeners) listener(paneId, exitCode)
    })
```

```ts
  /** Pid della shell del pane, o null se il pane non e attivo. */
  pid(paneId: string): number | null {
    return this.#handles.get(paneId)?.pid ?? null
  }

  /** Nome del processo in primo piano, o null se il pane non e attivo. */
  foregroundProcess(paneId: string): string | null {
    return this.#handles.get(paneId)?.foregroundProcess() ?? null
  }
```

- [ ] **Passo 4: aggiornare il finto in `dispatch.test.ts`**

Il finto di `pty` in `makeDispatcher()` (`src/main/control/dispatch.test.ts`,
intorno alla riga 43) va esteso. **Non rendere opzionali i metodi di
produzione per farlo compilare** — si aggiorna il finto:

```ts
    pty: {
      spawn: () => {},
      write: () => {},
      kill: () => {},
      resize: () => {},
      pid: () => null,
      foregroundProcess: () => null,
      onData: () => () => {},
      onExit: () => () => {}
    } as never,
```

Controllare anche il secondo finto piu completo nello stesso file (il commento
"Dipendenze complete per i test del motore terminale", intorno alla riga 64) e
aggiungervi gli stessi metodi.

- [ ] **Passo 5: eseguire l'intera suite**

Comando: `pnpm test:unit`
Atteso: PASS, nessuna regressione sui 244 test esistenti.

- [ ] **Passo 6: commit**

```bash
git add src/main/pty/ src/main/control/dispatch.test.ts
git commit -m "feat: il pty espone pid, processo in primo piano e codice di uscita"
```

---

## Task 9: protocollo, stato e dispatcher — `notify` e `pane.agent`

**File:**
- Modifica: `src/shared/protocol.ts`
- Modifica: `src/main/state/app-state.ts`
- Modifica: `src/main/control/dispatch.ts`
- Modifica: `src/main/state/app-state.test.ts`
- Modifica: `src/main/control/dispatch.test.ts`
- Modifica: `src/renderer/src/lib/app-model.svelte.ts`

**Interfacce:**
- Consuma: `AgentStatus`, `AGENT_STATUSES` da `../agents/detection/agent-status`;
  `AgentActivityModel` da `../agents/detection/activity-model`.
- Produce: `ControlRequest` guadagna il metodo `notify`;
  `StateEvent` guadagna `pane.agent`;
  `PaneSnapshot` guadagna `agentId: string | null` e `status: AgentStatus | null`;
  `AppState.setPaneAgent(paneId, agentId, status)`;
  `DispatchDeps.detection: AgentActivityModel` (**obbligatorio**).

**Contesto — perche protocollo e dispatcher stanno nello stesso task:**
`ControlRequest` e una `z.discriminatedUnion` e `dispatch.ts` ci fa sopra uno
`switch` esaustivo. Aggiungere un membro all'unione senza aggiungere il ramo
rompe la compilazione. In Fase 2 questi due pezzi erano stati separati in task
diversi e il compilatore ha rifiutato il taglio.

- [ ] **Passo 1: scrivere il test che fallisce**

In `src/main/state/app-state.test.ts` aggiungere:

```ts
test('setPaneAgent emette un evento e aggiorna lo snapshot', () => {
  const state = new AppState()
  const eventi: StateEvent[] = []
  state.subscribe((e) => eventi.push(e))
  state.addPane({ id: 'p1', cwd: '/tmp', createdAt: 0, title: '', agentId: null, status: null })

  state.setPaneAgent('p1', 'omp', 'running')

  expect(eventi.at(-1)).toEqual({
    type: 'pane.agent',
    paneId: 'p1',
    agentId: 'omp',
    status: 'running'
  })
  expect(state.snapshot()[0].agentId).toBe('omp')
  expect(state.snapshot()[0].status).toBe('running')
})

test('setPaneAgent non emette nulla se niente e cambiato', () => {
  const state = new AppState()
  state.addPane({ id: 'p1', cwd: '/tmp', createdAt: 0, title: '', agentId: 'omp', status: 'running' })
  const eventi: StateEvent[] = []
  state.subscribe((e) => eventi.push(e))
  state.setPaneAgent('p1', 'omp', 'running')
  expect(eventi).toEqual([])
})

test('setPaneAgent su un pane inesistente non fa nulla', () => {
  const state = new AppState()
  const eventi: StateEvent[] = []
  state.subscribe((e) => eventi.push(e))
  state.setPaneAgent('assente', 'omp', 'running')
  expect(eventi).toEqual([])
})
```

In `src/main/control/dispatch.test.ts` aggiungere:

```ts
test('notify registra lo stato dell agente sul pane', async () => {
  const { dispatch } = await makeDispatcher()
  const creato = await dispatch({
    id: 'r1',
    method: 'pane.create',
    params: { cwd: '/tmp', focus: false, agent: 'claude' }
  })
  expect(creato.ok).toBe(true)
  const paneId = (creato as { result: { paneId: string } }).result.paneId

  const risposta = await dispatch({
    id: 'r2',
    method: 'notify',
    params: { paneId, status: 'needs-input' }
  })
  expect(risposta.ok).toBe(true)

  const stato = await dispatch({ id: 'r3', method: 'state.get', params: {} })
  const pane = (stato as { result: { panes: { id: string; status: string }[] } })
    .result.panes.find((p) => p.id === paneId)
  expect(pane?.status).toBe('needs-input')
})

test('notify su un pane inesistente risponde con errore', async () => {
  const { dispatch } = await makeDispatcher()
  const risposta = await dispatch({
    id: 'r1',
    method: 'notify',
    params: { paneId: 'assente', status: 'running' }
  })
  expect(risposta.ok).toBe(false)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- app-state dispatch`
Atteso: FAIL — `setPaneAgent` e il metodo `notify` non esistono.

- [ ] **Passo 3: estendere il protocollo**

In `src/shared/protocol.ts`, aggiungere lo schema dello stato e i tre campi:

```ts
export const AgentStatusSchema = z.enum(['running', 'needs-input', 'done', 'error'])

export const PaneSnapshot = z.object({
  id: z.string().min(1),
  cwd: z.string().min(1),
  createdAt: z.number().int().nonnegative(),
  /** Titolo OSC riportato dal terminale; vuoto finché non ne arriva uno. */
  title: z.string().default(''),
  /** Agente riconosciuto nel pane, o null per una shell semplice. */
  agentId: z.string().nullable().default(null),
  /** Stato del ciclo di vita dell'agente, o null se non ce n'è uno. */
  status: AgentStatusSchema.nullable().default(null)
})
```

Nell'unione `ControlRequest`, aggiungere un membro:

```ts
  z.object({
    id: z.string().min(1),
    method: z.literal('notify'),
    params: z.object({
      paneId: z.string().min(1),
      status: AgentStatusSchema
    })
  }),
```

Nell'unione `StateEvent`, aggiungere:

```ts
  z.object({
    type: z.literal('pane.agent'),
    paneId: z.string().min(1),
    agentId: z.string().nullable(),
    status: AgentStatusSchema.nullable()
  })
```

- [ ] **Passo 4: estendere `AppState`**

In `src/main/state/app-state.ts`, dopo `setPaneTitle`:

```ts
  setPaneAgent(paneId: string, agentId: string | null, status: AgentStatus | null): void {
    const pane = this.#panes.get(paneId)
    if (pane === undefined) return
    if (pane.agentId === agentId && pane.status === status) return
    this.#panes.set(paneId, { ...pane, agentId, status })
    this.#emit({ type: 'pane.agent', paneId, agentId, status })
  }
```

con l'import `import type { AgentStatus } from '../agents/detection/agent-status'`.

- [ ] **Passo 5: cablare il dispatcher**

In `src/main/control/dispatch.ts`:

1. aggiungere alle `DispatchDeps` il campo **obbligatorio**
   `detection: AgentActivityModel` (import da
   `../agents/detection/activity-model`);
2. nel punto in cui `pane.create` costruisce il pane, aggiungere i due campi
   nuovi (`agentId: null`, `status: null`) e, quando `adapterId` è definito,
   chiamare subito dopo l'inserimento del pane:
   `deps.detection.agentSpawned(pane.id, adapterId, deps.now())` seguito da
   `deps.state.setPaneAgent(pane.id, adapterId, 'running')`;
3. aggiungere il ramo dello `switch`:

```ts
        case 'notify': {
          const { paneId, status } = request.params
          if (!deps.state.hasPane(paneId)) {
            return { id: request.id, ok: false, error: `pane sconosciuto: ${paneId}` }
          }
          deps.detection.notify(paneId, status, deps.now())
          deps.state.setPaneAgent(paneId, deps.detection.agentId(paneId), deps.detection.status(paneId))
          return { id: request.id, ok: true, result: {} }
        }
```

- [ ] **Passo 6: aggiornare i finti e il renderer**

In `makeDispatcher()` di `dispatch.test.ts` aggiungere
`detection: new AgentActivityModel()` alle dipendenze (e lo stesso al secondo
finto piu completo). In `src/renderer/src/lib/app-model.svelte.ts`, accanto al
ramo `case 'pane.title'`, aggiungere:

```ts
      case 'pane.agent':
        this.panes = this.panes.map((pane) =>
          pane.id === event.paneId
            ? { ...pane, agentId: event.agentId, status: event.status }
            : pane
        )
        return
```

Lo stile del file e immutabile (`this.panes = this.panes.map(...)`) e i rami
usano `return`, non `break`: seguirlo, non introdurre mutazione in luogo.

- [ ] **Passo 7: eseguire l'intera suite**

Comando: `pnpm test:unit && pnpm typecheck`
Atteso: PASS.

- [ ] **Passo 8: commit**

```bash
git add src/shared/protocol.ts src/main/state/ src/main/control/ src/renderer/src/lib/app-model.svelte.ts
git commit -m "feat: metodo notify ed evento pane.agent nel protocollo"
```

---

## Task 10: coordinatore degli inneschi

**File:**
- Crea: `src/main/agents/detection-coordinator.ts`
- Crea: `src/main/agents/detection-coordinator.test.ts`

**Interfacce:**
- Consuma: `AgentActivityModel`, `ProcessLister`, `parseProcessTable`,
  `childrenOf`, `recognizeAgent`, `statusFromContent`, `CONTENT_TAIL_LINES`,
  `normalizeTitle`.
- Produce:

```ts
export interface CoordinatorDeps {
  detection: AgentActivityModel
  listProcesses: ProcessLister
  /** Pid della shell di ogni pane vivo, per id. */
  shellPids(): ReadonlyMap<string, number>
  /** Processo in primo piano riportato dal pty, o null. */
  foregroundProcess(paneId: string): string | null
  /** Coda di schermo interpretata del pane. */
  readTail(paneId: string, lines: number): Promise<string>
  /** Chiamata a ogni cambio di stato o identita di un pane. */
  onChange(paneId: string, agentId: string | null, status: AgentStatus | null): void
  now(): number
}

export function createDetectionCoordinator(deps: CoordinatorDeps): {
  handleTitle(paneId: string, title: string): void
  handleOutputSettled(paneId: string): Promise<void>
  scanProcesses(): Promise<void>
  needsScan(): boolean
}
```

**Contesto:** `scanProcesses()` esegue **una sola** `ps` e ne risolve tutti i
pane. `needsScan()` confronta il processo in primo piano corrente con l'ultimo
visto: e il rilevatore gratuito che dice quando pagare i 24 ms.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { createDetectionCoordinator } from './detection-coordinator'
import { AgentActivityModel } from './detection/activity-model'

const TABELLA = [
  '  501     1 /bin/zsh zsh',
  '  502   501 /Users/x/.bun/bin/bun bun /Users/x/.bun/bin/omp',
  '  601     1 /bin/zsh zsh',
  '  602   601 /opt/homebrew/bin/codex codex'
].join('\n')

function setup(overrides: Partial<Parameters<typeof createDetectionCoordinator>[0]> = {}) {
  const detection = new AgentActivityModel()
  const cambi: [string, string | null, string | null][] = []
  let chiamateDiPs = 0
  const coord = createDetectionCoordinator({
    detection,
    listProcesses: async () => {
      chiamateDiPs++
      return TABELLA
    },
    shellPids: () => new Map([['pane-omp', 501], ['pane-codex', 601]]),
    foregroundProcess: (paneId) => (paneId === 'pane-omp' ? 'bun' : 'codex'),
    readTail: async () => '',
    onChange: (paneId, agentId, status) => cambi.push([paneId, agentId, status]),
    now: () => 1000,
    ...overrides
  })
  return { coord, detection, cambi, psCount: () => chiamateDiPs }
}

test('una sola ps risolve tutti i pane', async () => {
  const { coord, detection, psCount } = setup()
  await coord.scanProcesses()
  expect(psCount()).toBe(1)
  expect(detection.agentId('pane-omp')).toBe('omp')
  expect(detection.agentId('pane-codex')).toBe('codex')
})

test('omp non viene scambiato per pi', async () => {
  const { coord, detection } = setup()
  // Il titolo dice solo "famiglia pi"; il processo dice quale.
  coord.handleTitle('pane-omp', 'π > cap-omp-EafzYT')
  await coord.scanProcesses()
  expect(detection.agentId('pane-omp')).toBe('omp')
})

test('una ps fallita non cancella lo stato gia noto', async () => {
  const { coord, detection } = setup({
    listProcesses: async () => {
      throw new Error('ps non disponibile')
    }
  })
  detection.processIdentified('pane-codex', 'codex')
  await coord.scanProcesses()
  // "Non ho potuto guardare" non e "non c e".
  expect(detection.agentId('pane-codex')).toBe('codex')
  expect(detection.status('pane-codex')).toBe('running')
})

test('solo una scansione riuscita che non trova nulla azzera', async () => {
  const { coord, detection } = setup({
    listProcesses: async () => '  501     1 /bin/zsh zsh'
  })
  detection.processIdentified('pane-omp', 'omp')
  await coord.scanProcesses()
  expect(detection.agentId('pane-omp')).toBeNull()
})

test('needsScan e vero solo quando il primo piano cambia', async () => {
  const { coord } = setup()
  expect(coord.needsScan()).toBe(true) // primo giro: nulla di noto
  await coord.scanProcesses()
  expect(coord.needsScan()).toBe(false) // niente e cambiato
})

test('il contenuto arriva alla macchina a stati alla quiete dell output', async () => {
  const { coord, detection } = setup({
    readTail: async () => 'Do you want to create prova.txt?'
  })
  detection.processIdentified('pane-omp', 'claude')
  await coord.handleOutputSettled('pane-omp')
  expect(detection.status('pane-omp')).toBe('needs-input')
})

test('onChange scatta solo quando qualcosa cambia davvero', async () => {
  const { coord, cambi } = setup()
  await coord.scanProcesses()
  const dopoPrimo = cambi.length
  await coord.scanProcesses()
  expect(cambi.length).toBe(dopoPrimo)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- detection-coordinator`
Atteso: FAIL, modulo inesistente.

- [ ] **Passo 3: implementare**

```ts
import type { AgentStatus } from './detection/agent-status'
import type { AgentActivityModel } from './detection/activity-model'
import type { ProcessLister } from './detection/process-table'
import { parseProcessTable, childrenOf } from './detection/process-table'
import { recognizeAgent } from './detection/process-identity'
import { statusFromContent, CONTENT_TAIL_LINES } from './detection/content-status'
import { normalizeTitle } from './detection/title-normalize'

export interface CoordinatorDeps {
  detection: AgentActivityModel
  listProcesses: ProcessLister
  shellPids(): ReadonlyMap<string, number>
  foregroundProcess(paneId: string): string | null
  readTail(paneId: string, lines: number): Promise<string>
  onChange(paneId: string, agentId: string | null, status: AgentStatus | null): void
  now(): number
}

/**
 * Decide QUANDO guardare. I moduli in `detection/` decidono COSA significa
 * cio che si vede: sono funzioni pure e si testano senza tempo ne processi.
 * Qui vive tutto il resto — ed e per questo che questo file e piccolo.
 */
export function createDetectionCoordinator(deps: CoordinatorDeps): {
  handleTitle(paneId: string, title: string): void
  handleOutputSettled(paneId: string): Promise<void>
  scanProcesses(): Promise<void>
  needsScan(): boolean
} {
  /** Ultimo processo in primo piano visto per pane: il rilevatore gratuito. */
  const ultimoPrimoPiano = new Map<string, string | null>()

  const pubblica = (paneId: string): void => {
    deps.onChange(paneId, deps.detection.agentId(paneId), deps.detection.status(paneId))
  }

  /** Pubblica solo se identita o stato sono davvero cambiati. */
  const pubblicaSeCambiato = (paneId: string, prima: [string | null, AgentStatus | null]): void => {
    const dopo: [string | null, AgentStatus | null] = [
      deps.detection.agentId(paneId),
      deps.detection.status(paneId)
    ]
    if (prima[0] === dopo[0] && prima[1] === dopo[1]) return
    pubblica(paneId)
  }

  return {
    handleTitle(paneId, title): void {
      const prima: [string | null, AgentStatus | null] = [
        deps.detection.agentId(paneId),
        deps.detection.status(paneId)
      ]
      deps.detection.handleTitle(paneId, normalizeTitle(title), deps.now())
      pubblicaSeCambiato(paneId, prima)
    },

    async handleOutputSettled(paneId): Promise<void> {
      const agentId = deps.detection.agentId(paneId)
      if (agentId === null) return
      const prima: [string | null, AgentStatus | null] = [agentId, deps.detection.status(paneId)]
      const coda = await deps.readTail(paneId, CONTENT_TAIL_LINES)
      const status = statusFromContent(coda, agentId)
      if (status === null) return
      deps.detection.applyContentSignal(paneId, status, deps.now())
      pubblicaSeCambiato(paneId, prima)
    },

    needsScan(): boolean {
      for (const paneId of deps.shellPids().keys()) {
        const corrente = deps.foregroundProcess(paneId)
        if (!ultimoPrimoPiano.has(paneId)) return true
        if (ultimoPrimoPiano.get(paneId) !== corrente) return true
      }
      return false
    },

    async scanProcesses(): Promise<void> {
      let righe
      try {
        righe = parseProcessTable(await deps.listProcesses())
      } catch {
        // Una scansione fallita significa "non ho potuto guardare", non
        // "non c e": azzerare qui cancellerebbe lo stato di un agente vivo.
        return
      }

      for (const [paneId, shellPid] of deps.shellPids()) {
        const prima: [string | null, AgentStatus | null] = [
          deps.detection.agentId(paneId),
          deps.detection.status(paneId)
        ]

        let trovato: string | null = null
        for (const figlio of childrenOf(righe, shellPid)) {
          trovato = recognizeAgent(figlio.comm, figlio.args)
          if (trovato !== null) break
        }

        if (trovato === null) {
          deps.detection.processGone(paneId)
        } else {
          deps.detection.processIdentified(paneId, trovato)
        }

        ultimoPrimoPiano.set(paneId, deps.foregroundProcess(paneId))
        pubblicaSeCambiato(paneId, prima)
      }
    }
  }
}
```

- [ ] **Passo 4: eseguire e verificare che passi**

Comando: `pnpm test:unit -- detection-coordinator`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/agents/detection-coordinator.ts src/main/agents/detection-coordinator.test.ts
git commit -m "feat: coordinatore della detection con scansione unica per tutti i pane"
```

---

## Task 11: cablaggio nel main

**File:**
- Modifica: `src/main/index.ts`

**Interfacce:**
- Consuma: `AgentActivityModel`, `createDetectionCoordinator`,
  `createPsLister`, `CONTENT_TAIL_LINES`.
- Produce: niente di nuovo verso altri task.

- [ ] **Passo 1: costruire modello e coordinatore**

In `src/main/index.ts`, accanto alle istanze globali gia presenti
(`state`, `pty`, `terminals`, `db`):

```ts
import { promisify } from 'node:util'
import { execFile } from 'node:child_process'
import { AgentActivityModel } from './agents/detection/activity-model'
import { createDetectionCoordinator } from './agents/detection-coordinator'
import { createPsLister } from './agents/detection/process-table'
import { CONTENT_TAIL_LINES } from './agents/detection/content-status'
import { createDebouncer, type Debouncer } from './terminal/debounce'

const execFileAsync = promisify(execFile)
const detection = new AgentActivityModel()
```

Dentro `app.whenReady()`, dopo la creazione del dispatcher:

```ts
  const coordinator = createDetectionCoordinator({
    detection,
    listProcesses: createPsLister(async (file, args) => {
      const { stdout } = await execFileAsync(file, args, { maxBuffer: 8 << 20 })
      return stdout
    }),
    shellPids: () => {
      const mappa = new Map<string, number>()
      for (const pane of state.snapshot()) {
        const pid = pty.pid(pane.id)
        if (pid !== null) mappa.set(pane.id, pid)
      }
      return mappa
    },
    foregroundProcess: (paneId) => pty.foregroundProcess(paneId),
    readTail: async (paneId, lines) => {
      const terminal = terminals.get(paneId)
      if (terminal === undefined) return ''
      await terminal.flush()
      return terminal.readText(lines)
    },
    onChange: (paneId, agentId, status) => state.setPaneAgent(paneId, agentId, status),
    now: () => Date.now()
  })
```

Passare `detection` fra le dipendenze del dispatcher (campo obbligatorio
aggiunto nel Task 9).

- [ ] **Passo 2: collegare i tre inneschi**

```ts
  terminals.onTitle((paneId, title) => coordinator.handleTitle(paneId, title))

  // Quiete dell'output: un debouncer per pane, cosi due pane rumorosi non si
  // annullano a vicenda. 400 ms e il valore di partenza della spec.
  // `Debouncer<T>.push` vuole un valore: qui non c'e nulla da trasportare,
  // quindi il tipo e `null`.
  const quiete = new Map<string, Debouncer<null>>()
  pty.onData((paneId) => {
    let debouncer = quiete.get(paneId)
    if (debouncer === undefined) {
      debouncer = createDebouncer<null>({
        delayMs: 400,
        onSettle: () => {
          void coordinator.handleOutputSettled(paneId)
          // Il primo piano cambia proprio quando un agente parte o esce:
          // controllarlo qui e gratis, la ps si paga solo se serve davvero.
          if (coordinator.needsScan()) void coordinator.scanProcesses()
        },
        schedule: (fn, ms) => setTimeout(fn, ms),
        cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>)
      })
      quiete.set(paneId, debouncer)
    }
    debouncer.push(null)
  })

  pty.onExit((paneId, exitCode) => {
    detection.applyExit(paneId, exitCode)
    state.setPaneAgent(paneId, detection.agentId(paneId), detection.status(paneId))
    quiete.get(paneId)?.cancel()
    quiete.delete(paneId)
  })
```

Verificare la firma reale di `createDebouncer` in
`src/main/terminal/debounce.ts` e adattare `push`/`cancel` ai nomi effettivi.

- [ ] **Passo 3: verificare a mano che funzioni davvero**

```bash
pnpm exec electron-vite build
```

Poi, in due terminali distinti (il primo tiene l'app viva):

```bash
cd ~/Desktop/Progetti/tiller-electron && ./node_modules/.bin/electron .
```

```bash
cd ~/Desktop/Progetti/tiller-electron && node cli/tillerctl.ts run --agent omp --cwd /tmp
```

Attendere ~10 s, poi:

```bash
cd ~/Desktop/Progetti/tiller-electron && node cli/tillerctl.ts state
```

Atteso: il pane ha `"agentId":"omp"` — **non** `"pi"` — e uno `status` non nullo.

- [ ] **Passo 4: eseguire il gate**

Comando: `pnpm test:unit && pnpm typecheck`
Atteso: PASS.

- [ ] **Passo 5: commit**

```bash
git add src/main/index.ts
git commit -m "feat: cablaggio della detection agenti nel processo main"
```

---

## Task 12: `tillerctl notify` e shim a path stabile

**File:**
- Modifica: `cli/args.ts`
- Modifica: `cli/args.test.ts`
- Crea: `src/main/agents/tillerctl-shim.ts`
- Crea: `src/main/agents/tillerctl-shim.test.ts`
- Modifica: `src/main/index.ts`

**Interfacce:**
- Consuma: niente.
- Produce: comando CLI `notify --pane <id> --status <s>`;
  `stableTillerctlPath(appDir: string): string`;
  `writeTillerctlShim(appDir: string, target: string, write: (path: string, content: string, mode: number) => Promise<void>): Promise<string>`.

**Contesto:** nel Tiller Swift gli hook incorporavano un path assoluto a
`tillerctl`; l app si e spostata e i worktree hanno continuato a chiamare un
binario inesistente, in silenzio. Un hook e una scrittura che sopravvive a chi
l ha scritta: nessun compilatore e nessun test attraversa quel confine, quindi
il testo scritto non deve contenere informazioni volatili.

- [ ] **Passo 1: scrivere il test che fallisce**

In `cli/args.test.ts`:

```ts
test('notify costruisce la richiesta con pane e stato', () => {
  expect(parseCommand(['notify', '--pane', 'p1', '--status', 'needs-input'])).toEqual({
    method: 'notify',
    params: { paneId: 'p1', status: 'needs-input' }
  })
})

test('notify senza pane e un errore', () => {
  expect(() => parseCommand(['notify', '--status', 'running'])).toThrow('--pane è obbligatorio')
})

test('notify senza status e un errore', () => {
  expect(() => parseCommand(['notify', '--pane', 'p1'])).toThrow('--status è obbligatorio')
})
```

La funzione esportata si chiama `parseCommand(argv: string[]): ParsedCommand`
(`cli/args.ts:207`), e i messaggi d'errore seguono la convenzione gia in uso:
`--<opzione> è obbligatorio`.

`src/main/agents/tillerctl-shim.test.ts`:

```ts
import { expect, test } from 'vitest'
import { stableTillerctlPath, writeTillerctlShim } from './tillerctl-shim'

test('il path stabile non dipende dalla posizione del binario', () => {
  expect(stableTillerctlPath('/Users/x/Library/Application Support/tiller-electron')).toBe(
    '/Users/x/Library/Application Support/tiller-electron/bin/tillerctl'
  )
})

test('lo shim e eseguibile e inoltra al bersaglio corrente', async () => {
  const scritti: { path: string; content: string; mode: number }[] = []
  const path = await writeTillerctlShim('/app-dir', '/versioni/1.2.3/tillerctl', async (p, c, m) => {
    scritti.push({ path: p, content: c, mode: m })
  })
  expect(path).toBe('/app-dir/bin/tillerctl')
  expect(scritti[0].content).toContain('/versioni/1.2.3/tillerctl')
  expect(scritti[0].content.startsWith('#!')).toBe(true)
  // 0o755: gli hook lo eseguono, non lo leggono.
  expect(scritti[0].mode).toBe(0o755)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- tillerctl-shim args`
Atteso: FAIL.

- [ ] **Passo 3: implementare**

In `cli/args.ts`, dentro la tabella `COMMANDS`:

```ts
  notify: {
    method: 'notify',
    options: { pane: { type: 'string' }, status: { type: 'string' } },
    required: ['pane', 'status'],
    build: (v: Record<string, string>) => ({ paneId: v.pane, status: v.status })
  },
```

`src/main/agents/tillerctl-shim.ts`:

```ts
import { join } from 'node:path'

/**
 * Path che gli hook citeranno per sempre. Deliberatamente indipendente da dove
 * si trova il binario oggi: un hook resta nel worktree quando l app cambia
 * versione o si sposta, e un path volatile lo farebbe fallire in silenzio.
 */
export function stableTillerctlPath(appDir: string): string {
  return join(appDir, 'bin', 'tillerctl')
}

/**
 * Rigenera lo shim in modo che punti al binario corrente. Va chiamato a ogni
 * avvio: e il momento in cui l informazione volatile viene rifrescata in un
 * posto solo, invece di essere copiata in ogni worktree.
 */
export async function writeTillerctlShim(
  appDir: string,
  target: string,
  write: (path: string, content: string, mode: number) => Promise<void>
): Promise<string> {
  const path = stableTillerctlPath(appDir)
  const contenuto = `#!/bin/sh\nexec ${JSON.stringify(target)} "$@"\n`
  await write(path, contenuto, 0o755)
  return path
}
```

- [ ] **Passo 4: rigenerare lo shim all'avvio**

In `src/main/index.ts`, dentro `app.whenReady()`, prima della creazione del
dispatcher:

```ts
  // Lo shim va rigenerato a ogni avvio: e l unico posto in cui il path reale
  // del binario viene aggiornato, invece di essere copiato in ogni worktree.
  const appDir = dirname(resolveSocketPath())
  const tillerctlPath = await writeTillerctlShim(
    appDir,
    process.execPath,
    async (path, content, mode) => {
      await mkdir(dirname(path), { recursive: true })
      await writeFile(path, content, { mode })
      await chmod(path, mode)
    }
  )
```

con gli import `import { mkdir, writeFile, chmod } from 'node:fs/promises'` e
`import { dirname } from 'node:path'`. Passare `tillerctlPath` al dispatcher
come nuova dipendenza **obbligatoria** `tillerctlPath: string` e usarla in
`pane.create` al posto della riga attuale (`dispatch.ts:130`):

```ts
tillerctlPath: process.argv[1] ?? 'tillerctl'   // ← sostituire
tillerctlPath: deps.tillerctlPath               // ← con questa
```

Aggiornare di conseguenza il commento sopra (`dispatch.ts:117`), che oggi dice
"tillerctlPath resta il binario corrente": ora e il path stabile. Nei finti di
`dispatch.test.ts` aggiungere `tillerctlPath: '/finto/bin/tillerctl'`.

- [ ] **Passo 5: eseguire il gate**

Comando: `pnpm test:unit && pnpm typecheck`
Atteso: PASS.

- [ ] **Passo 6: commit**

```bash
git add cli/args.ts cli/args.test.ts src/main/agents/tillerctl-shim.ts src/main/agents/tillerctl-shim.test.ts src/main/index.ts src/main/control/dispatch.ts src/main/control/dispatch.test.ts
git commit -m "feat: comando notify in tillerctl e shim a path stabile per gli hook"
```

---

## Task 13: `prepare()` reale nei cinque adapter

**File:**
- Modifica: `src/main/agents/catalog.ts`
- Modifica: `src/main/agents/adapters/claude.ts`
- Modifica: `src/main/agents/adapters/codex.ts`
- Modifica: `src/main/agents/adapters/opencode.ts`
- Modifica: `src/main/agents/adapters/omp.ts`
- Modifica: `src/main/agents/adapters/pi.ts`
- Crea: `src/main/agents/adapters/prepare.test.ts`

**Interfacce:**
- Consuma: `shellQuote`, `jsonStringLiteral` da `../quote`.
- Produce: `prepare()` che scrive davvero; il commento in `catalog.ts` che
  dichiara il no-op di Fase 1 va **rimosso**.

**Contesto:** l app Swift e dismessa, quindi si scrive negli stessi path senza
namespace separati ne coesistenza da gestire. Mai la configurazione globale
dell utente.

- [ ] **Passo 1: scrivere il test che fallisce**

```ts
import { expect, test } from 'vitest'
import { mkdtemp, readFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { claudeAdapter } from './claude'
import { ompAdapter } from './omp'
import { opencodeAdapter } from './opencode'
import { codexAdapter } from './codex'
import { piAdapter } from './pi'

async function scratch(): Promise<string> {
  return mkdtemp(join(tmpdir(), 'prepare-'))
}

const opzioni = (worktreePath: string) => ({
  worktreePath,
  paneId: 'pane-1',
  tillerctlPath: '/stabile/bin/tillerctl'
})

test('claude scrive i cinque array di hook preservando le altre chiavi', async () => {
  const dir = await scratch()
  const { writeFile, mkdir } = await import('node:fs/promises')
  await mkdir(join(dir, '.claude'), { recursive: true })
  await writeFile(
    join(dir, '.claude/settings.local.json'),
    JSON.stringify({ model: 'sonnet', hooks: { PreToolUse: ['mio'] } })
  )

  await claudeAdapter.prepare(opzioni(dir))

  const scritto = JSON.parse(
    await readFile(join(dir, '.claude/settings.local.json'), 'utf8')
  )
  // Le chiavi dell utente sopravvivono.
  expect(scritto.model).toBe('sonnet')
  expect(scritto.hooks.PreToolUse).toEqual(['mio'])
  // I cinque array nostri ci sono.
  for (const evento of ['Stop', 'Notification', 'SessionStart', 'UserPromptSubmit', 'SessionEnd']) {
    expect(scritto.hooks[evento]).toBeDefined()
  }
  // Gli hook citano il path stabile e il paneId.
  const testo = JSON.stringify(scritto.hooks)
  expect(testo).toContain('/stabile/bin/tillerctl')
  expect(testo).toContain('pane-1')
})

test('omp scrive il file di hook e il comando lo carica', async () => {
  const dir = await scratch()
  await ompAdapter.prepare(opzioni(dir))
  const hook = await readFile(join(dir, '.tiller/omp-hook.ts'), 'utf8')
  expect(hook).toContain('/stabile/bin/tillerctl')
  expect(hook).toContain('pane-1')
  expect(hook).toContain('turn_end')
  expect(ompAdapter.command(opzioni(dir))).toContain('--hook')
})

test('opencode scrive il plugin che riporta solo l id di sessione', async () => {
  const dir = await scratch()
  await opencodeAdapter.prepare(opzioni(dir))
  const plugin = await readFile(join(dir, '.opencode/plugin/tiller-session.js'), 'utf8')
  expect(plugin).toContain('session-ref')
  // OpenCode non ha hook di stato: il plugin non deve toccare lo stato.
  expect(plugin).not.toContain('--status')
})

test('codex non scrive nulla: la sua config passa dalla riga di comando', async () => {
  const dir = await scratch()
  await codexAdapter.prepare(opzioni(dir))
  const { readdir } = await import('node:fs/promises')
  expect(await readdir(dir)).toEqual([])
  expect(codexAdapter.command(opzioni(dir))).toContain('notify=')
})

test('pi non ha meccanismo di hook e non scrive nulla', async () => {
  const dir = await scratch()
  await piAdapter.prepare(opzioni(dir))
  const { readdir } = await import('node:fs/promises')
  expect(await readdir(dir)).toEqual([])
  expect(piAdapter.hasNativeHooks).toBe(false)
})
```

- [ ] **Passo 2: eseguire e verificare che fallisca**

Comando: `pnpm test:unit -- prepare`
Atteso: FAIL — `prepare` e ancora un no-op.

- [ ] **Passo 3: implementare `claude.ts`**

```ts
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { join } from 'node:path'
import { shellQuote } from '../quote'
import type { AgentAdapter, PrepareOptions } from '../catalog'

/**
 * Semantica di fusione: si sostituiscono SOLO i cinque array di hook, ogni
 * altra chiave del file resta com era. E la configurazione di progetto
 * dell utente, non nostra.
 */
async function scriviHookClaude({
  worktreePath,
  paneId,
  tillerctlPath
}: PrepareOptions): Promise<void> {
  const dir = join(worktreePath, '.claude')
  await mkdir(dir, { recursive: true })
  const file = join(dir, 'settings.local.json')

  const comando = (status: string): string =>
    `${shellQuote(tillerctlPath)} notify --pane ${paneId} --status ${status}`
  const uno = (status: string): unknown => [
    { matcher: '', hooks: [{ type: 'command', command: comando(status) }] }
  ]

  const nostri: Record<string, unknown> = {
    Stop: uno('needs-input'),
    Notification: uno('needs-input'),
    // Un avvio di sessione lascia Claude fermo al prompt: sta aspettando, non
    // lavorando. UserPromptSubmit lo porta a running.
    SessionStart: uno('needs-input'),
    UserPromptSubmit: uno('running'),
    SessionEnd: uno('done')
  }

  let radice: Record<string, unknown> = {}
  try {
    radice = JSON.parse(await readFile(file, 'utf8')) as Record<string, unknown>
  } catch {
    // File assente o illeggibile: si riparte da zero senza perdere l avvio.
  }

  const esistenti = (radice.hooks ?? {}) as Record<string, unknown>
  radice.hooks = { ...esistenti, ...nostri }
  await writeFile(file, `${JSON.stringify(radice, null, 2)}\n`)
}

export const claudeAdapter: AgentAdapter = {
  id: 'claude',
  displayName: 'Claude Code',
  hasNativeHooks: true,
  prepare: scriviHookClaude,
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

- [ ] **Passo 4: implementare `omp.ts` e `opencode.ts`**

`omp.ts` — `prepare` scrive `<worktree>/.tiller/omp-hook.ts` e `command`
lo carica con `--hook`:

```ts
async function scriviHookOmp({
  worktreePath,
  paneId,
  tillerctlPath
}: PrepareOptions): Promise<void> {
  const dir = join(worktreePath, '.tiller')
  await mkdir(dir, { recursive: true })
  const hook = `// Generato da Tiller — guida il badge di questo pane.
export default function (pi) {
    const notify = (status) => {
        void pi.exec(${JSON.stringify(tillerctlPath)}, ["notify", "--pane", "${paneId}", "--status", status]);
    };
    pi.on("session_start", async () => { notify("running"); });
    pi.on("turn_start", async () => { notify("running"); });
    pi.on("turn_end", async () => { notify("needs-input"); });
    pi.on("session_shutdown", async () => { notify("done"); });
}
`
  await writeFile(join(dir, 'omp-hook.ts'), hook)
}
```

e `command({ worktreePath })` diventa
`` `omp --hook ${shellQuote(join(worktreePath, '.tiller/omp-hook.ts'))}` ``
(lo stesso per `resumeCommand`).

`opencode.ts` — `prepare` scrive
`<worktree>/.opencode/plugin/tiller-session.js`. OpenCode **non ha hook di
stato**: il plugin riporta solo l id di sessione, e non deve contenere
`--status`.

```ts
async function scriviPluginOpenCode({
  worktreePath,
  paneId,
  tillerctlPath
}: PrepareOptions): Promise<void> {
  const dir = join(worktreePath, '.opencode/plugin')
  await mkdir(dir, { recursive: true })
  const plugin = `// Generato da Tiller — riporta l'id di sessione per il restore.
export const TillerSession = async ({ $ }) => {
  let riportato = ""
  const tillerctl = ${JSON.stringify(tillerctlPath)}
  const pane = "${paneId}"
  return {
    event: async ({ event }) => {
      const id = event?.properties?.info?.id
      if (event?.type === "session.updated" && id && id !== riportato) {
        riportato = id
        await $\`\${tillerctl} session-ref --pane \${pane} --ref \${id}\`.quiet().nothrow()
      }
    },
  }
}
`
  await writeFile(join(dir, 'tiller-session.js'), plugin)
}
```

Nota: `session-ref` non e un comando di `tillerctl` in Fase 3 — il plugin lo
scrive perche il ripristino di sessione arriva in Fase 5, e `.nothrow()` fa si
che un comando ancora inesistente non rompa OpenCode.

- [ ] **Passo 5: lasciare `codex.ts` e `pi.ts` senza scrittura**

`codex.ts`: `prepare` resta vuoto ma il **commento cambia** — non e piu "in
Fase 1 non si scrive", e "Codex non ha configurazione di progetto: l override
passa dalla riga di comando". Verificare che `command()` contenga gia
`-c notify=[…]` costruito con `jsonStringLiteral` (senza escape delle barre,
perche l override `-c` di Codex e interpretato come TOML).

`pi.ts`: `prepare` resta vuoto con il commento "pi non ha meccanismo di hook;
lo stato arriva dai layer titolo/contenuto/processo e dal codice di uscita".

- [ ] **Passo 6: rimuovere il commento obsoleto in `catalog.ts`**

Sostituire il blocco di commento su `prepare` che dichiara il no-op di Fase 1
con:

```ts
  /**
   * Scrive la configurazione degli hook DENTRO il worktree — mai la
   * configurazione globale dell'utente (`~/.claude/settings.json`,
   * `~/.codex/config.toml`). Gli hook citano un path stabile a `tillerctl`,
   * non il binario corrente: un file di hook sopravvive a chi l'ha scritto.
   */
```

- [ ] **Passo 7: eseguire il gate**

Comando: `pnpm test:unit && pnpm typecheck && pnpm lint`
Atteso: PASS.

- [ ] **Passo 8: commit**

```bash
git add src/main/agents/
git commit -m "feat: prepare scrive davvero gli hook worktree-locali"
```

---

## Task 14: agenti finti e criteri end-to-end

**File:**
- Crea: `e2e/fixtures/fake-agents/claude`
- Crea: `e2e/fixtures/fake-agents/codex`
- Crea: `e2e/fixtures/fake-agents/opencode`
- Crea: `e2e/fixtures/fake-agents/pi`
- Crea: `e2e/fixtures/fake-agents/omp`
- Crea: `e2e/fase-3-detection.spec.ts`

**Interfacce:**
- Consuma: `tillerctl run`, `send`, `read`, `state`, `close`.
- Produce: niente verso altri task.

**Contesto:** `scripts/ci.sh` gira di continuo; chiamare LLM reali
consumerebbe quota a ogni esecuzione. Gli agenti finti riproducono le sequenze
OSC registrate dal vivo il 2026-08-01 — sono registrazioni, non invenzioni.

- [ ] **Passo 1: scrivere gli agenti finti**

`e2e/fixtures/fake-agents/omp` (rendere eseguibile con `chmod +x`):

```sh
#!/bin/sh
# Riproduce i titoli reali registrati da omp il 2026-08-01:
# fermo `π > <cwd>`, al lavoro `π ⠼ <task>`.
printf '\033]0;\317\200 > cap-omp-EafzYT\007'
sleep 2
printf '\033]0;\317\200 \342\240\274 Crea file prova.txt con ciao\007'
printf '\342\240\217 Working\342\200\246 \342\237\250esc\342\237\251\n'
sleep 60
```

`e2e/fixtures/fake-agents/claude`:

```sh
#!/bin/sh
# `✳ ` fermo, spinner braille iniziale al lavoro.
printf '\033]0;\342\234\263 Claude Code\007'
sleep 2
printf '\033]0;\342\240\202 Claude Code\007'
sleep 60
```

`e2e/fixtures/fake-agents/codex`:

```sh
#!/bin/sh
# Codex non emette ALCUN titolo: e l intero punto del Layer D.
sleep 60
```

`e2e/fixtures/fake-agents/opencode`:

```sh
#!/bin/sh
printf '\033]0;OpenCode\007'
# La barra di stato dice `esc interrupt` anche a riposo: e la trappola che il
# Layer C deve ignorare per questo agente.
printf '\342\254\235\342\254\235 esc interrupt tab agents ctrl+p commands \342\200\242 OpenCode 1.18.0\n'
sleep 60
```

`e2e/fixtures/fake-agents/pi`:

```sh
#!/bin/sh
printf '\033]0;\317\200 - cap-pi-0aPG6D\007'
sleep 60
```

- [ ] **Passo 2: scrivere i quattro criteri**

`e2e/fase-3-detection.spec.ts`, sullo stampo di `e2e/fase-2-terminale.spec.ts`
(stesso `beforeEach`/`afterEach` con socket e db in una cartella temporanea).
Il `PATH` dell'app deve avere davanti la cartella degli agenti finti:

```ts
app = await electron.launch({
  args: ['.'],
  env: {
    ...process.env,
    TILLER_SOCKET: socketPath,
    TILLER_DB: dbPath,
    PATH: `${join(process.cwd(), 'e2e/fixtures/fake-agents')}:${process.env.PATH}`
  }
})
```

```ts
async function statoPane(paneId: string): Promise<{ agentId: string | null; status: string | null }> {
  const stato = JSON.parse(await tillerctl(['state']))
  const pane = stato.panes.find((p: { id: string }) => p.id === paneId)
  return { agentId: pane?.agentId ?? null, status: pane?.status ?? null }
}

test('criterio 1: omp e omp, non pi', async () => {
  // Arrange & Act
  const paneId = await tillerctl(['run', '--agent', 'omp', '--cwd', scratch])

  // Assert — il titolo dice solo "famiglia pi"; il nome dell eseguibile dice
  // quale. L app Swift qui sbaglia, perche cerca il separatore `π:` che omp
  // non usa piu.
  await attendi(async () => (await statoPane(paneId)).agentId === 'omp')
})

test('criterio 2: codex, che non emette titolo, risulta comunque attivo', async () => {
  // Arrange & Act
  const paneId = await tillerctl(['run', '--agent', 'codex', '--cwd', scratch])

  // Assert — l unica prova del Layer D: nessun titolo, nessun hook ancora
  // partito, eppure il pane sa chi ci gira dentro.
  await attendi(async () => {
    const s = await statoPane(paneId)
    return s.agentId === 'codex' && s.status !== null
  })
})

test('criterio 3: venti pane costano una scansione, non venti', async () => {
  // Arrange — un finto `ps` che conta le proprie invocazioni.
  const contatore = join(scratch, 'ps-chiamate')
  const fintoPs = join(scratch, 'bin')
  await mkdir(fintoPs, { recursive: true })
  await writeFile(
    join(fintoPs, 'ps'),
    `#!/bin/sh\necho x >> ${contatore}\nexec /bin/ps "$@"\n`,
    { mode: 0o755 }
  )
  await app.close()
  app = await electron.launch({
    args: ['.'],
    env: {
      ...process.env,
      TILLER_SOCKET: socketPath,
      TILLER_DB: dbPath,
      PATH: `${fintoPs}:${join(process.cwd(), 'e2e/fixtures/fake-agents')}:${process.env.PATH}`
    }
  })
  await app.firstWindow()

  // Act — venti pane con lo stesso agente.
  for (let i = 0; i < 20; i++) {
    await tillerctl(['run', '--agent', 'codex', '--cwd', scratch])
  }
  await new Promise((r) => setTimeout(r, 3000))

  // Assert — il numero di scansioni deve restare nell ordine dei cambi di
  // primo piano, non moltiplicarsi per il numero di pane. Con una scansione
  // per pane si supererebbe abbondantemente questa soglia.
  const chiamate = (await readFile(contatore, 'utf8')).split('\n').filter(Boolean).length
  expect(chiamate).toBeLessThan(20)
})

test('criterio 4: una scansione fallita non cancella lo stato', async () => {
  // Arrange — pane riconosciuto.
  const paneId = await tillerctl(['run', '--agent', 'codex', '--cwd', scratch])
  await attendi(async () => (await statoPane(paneId)).agentId === 'codex')

  // Act — `ps` sostituito da un comando che fallisce sempre.
  const rotto = join(scratch, 'rotto')
  await mkdir(rotto, { recursive: true })
  await writeFile(join(rotto, 'ps'), '#!/bin/sh\nexit 1\n', { mode: 0o755 })
  await tillerctl(['send', '--pane', paneId, '--data', 'x'])
  await new Promise((r) => setTimeout(r, 2000))

  // Assert — "non ho potuto guardare" non e "non c e".
  expect((await statoPane(paneId)).agentId).toBe('codex')
})
```

- [ ] **Passo 3: eseguire gli e2e**

Comando: `pnpm exec electron-vite build && pnpm test:e2e`
Atteso: 13 test passati (9 delle fasi precedenti + 4 nuovi).

Se il criterio 4 non riesce a sostituire `ps` perche il `PATH` viene risolto
una volta sola all avvio, riavviare l app con il `PATH` rotto invece di
sostituire il file a caldo. Adattare il test, **non** rilassare l asserzione.

- [ ] **Passo 4: validare per mutazione il criterio 1**

Questo passo prova che il criterio sa fallire. In
`src/main/agents/detection/title-identity.ts`, sostituire temporaneamente il
ramo della famiglia pi con l'errore dell'app Swift:

```ts
  // MUTAZIONE TEMPORANEA: la regola sbagliata dell app Swift.
  if (title.includes('π:')) return 'omp'
  if (isPiFamilyTitle(title)) return 'pi'
```

Comando: `pnpm exec electron-vite build && pnpm test:e2e -- --grep "criterio 1"`
Atteso: **FAIL** (`agentId` risulta `pi`). Se passa, il criterio e vacuo e va
riscritto.

Poi **annullare la mutazione** e riverificare:

```bash
git checkout src/main/agents/detection/title-identity.ts
pnpm exec electron-vite build && pnpm test:e2e
```

- [ ] **Passo 5: gate completo**

Comando: `bash scripts/ci.sh`
Atteso: `CI OK`.

- [ ] **Passo 6: commit**

```bash
git add e2e/
git commit -m "test: agenti finti dalle catture e quattro criteri e2e della detection"
```

---

## Checklist manuale (una volta, con le CLI vere)

Da eseguire dopo il Task 14. Gli agenti finti provano la logica; solo le CLI
vere provano le convenzioni.

```bash
cd ~/Desktop/Progetti/tiller-electron && pnpm exec electron-vite build && ./node_modules/.bin/electron .
```

- [ ] `tillerctl run --agent claude --cwd /tmp` → `state` mostra
      `agentId: "claude"`, `status: "needs-input"`
- [ ] inviato un compito a Claude → `status` passa a `running` e torna a
      `needs-input`
- [ ] chiesto a Claude di creare un file → `status: "needs-input"` mentre la
      richiesta di permesso e a schermo (Layer C)
- [ ] `--agent codex` → `agentId: "codex"` pur senza alcun titolo
- [ ] `--agent opencode` → riconosciuto, e **non** bloccato su `running`
      quando e fermo
- [ ] `--agent pi` → `agentId: "pi"`, mai `omp`
- [ ] `--agent omp` → `agentId: "omp"`, mai `pi`
- [ ] con Claude al lavoro, `cat <worktree>/.claude/settings.local.json`
      contiene il path stabile `…/tiller-electron/bin/tillerctl`
- [ ] **pi al lavoro** (rischio aperto della spec): verificare che il titolo
      contenga uno spinner braille e che `status` diventi `running`. Se la
      convenzione differisce, correggere `statusFromTitle` e aggiungere il caso
      registrato ai test del Task 4.
