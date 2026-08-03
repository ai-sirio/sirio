# Il sistema di design — piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** sostituire la palette parziale e scaduta con un sistema — colori
corretti, scale esplicite — e renderlo impossibile da aggirare in silenzio.

**Architettura:** tre task in sequenza obbligata. Il primo porta i colori giusti,
il secondo scrive la scansione che li rende obbligatori (e resta rossa), il terzo
migra i componenti e la fa diventare verde.

**Spec:** `docs/superpowers/specs/2026-08-03-electron-design-system-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo.
- `npm run lint`: **0 errori**; i warning sono 26 e **non devono salire**.
- Non committare `.tokensave/**`.
- **Non eseguire `bash scripts/ci.sh`.**
- `__dirname` **funziona** nei test vitest di questo progetto — verificato con una
  sonda, non supposto. Vale anche `import.meta.dirname`, e `process.cwd()` è la
  radice del repo. Usa `__dirname` come nei blocchi qui sotto e non cercare
  alternative.

## Fuori scopo, dichiarato

**La sidebar a tre livelli e la rimozione di `AgentsPanel` non sono in questo
piano**, pur essendo decise nella spec. La sequenza scelta mette i token prima
delle fasi 5c e 7, e la ristrutturazione dopo: è un cambio di funzionalità, non
di token, e non blocca nessuna delle due fasi. Avrà il suo piano.

Anche fuori: ombre, temi dell'utente, resa del trascritto chat (appartiene alla
5c).

---

## Task 1: La palette, e il test che la difende

**File:**
- Crea: `src/renderer/src/lib/design/palette.ts`
- Crea: `src/renderer/src/lib/design/palette.test.ts`
- Crea: `src/renderer/src/assets/tokens.css`
- Modifica: `src/renderer/src/assets/main.css` (importa `tokens.css`)

**Interfacce:**
- Produce: `PALETTE_SWIFT` — `Record<string, string>` da nome token a valore
  esadecimale maiuscolo; `OPACITA_TRASLUCIDA` — `number`.

- [ ] **Passo 1: scrivere il test che fallisce**

`src/renderer/src/lib/design/palette.test.ts`:

```ts
import { describe, expect, test } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'
import { OPACITA_TRASLUCIDA, PALETTE_SWIFT } from './palette.ts'

/**
 * Legge i token dichiarati nel blocco `:root` del foglio.
 *
 * Il test confronta il CSS con i valori estratti dalla versione Swift invece di
 * ricontrollare una copia in TypeScript: e' il CSS che l applicazione usa
 * davvero, e la copia sarebbe libera di divergere da cio' che si vede.
 */
function tokenDichiarati(): Record<string, string> {
  const css = readFileSync(join(__dirname, '../../assets/tokens.css'), 'utf8')
  const trovati: Record<string, string> = {}
  for (const riga of css.split('\n')) {
    const m = riga.match(/^\s*(--t-[a-z0-9-]+):\s*([^;]+);/)
    if (m) trovati[m[1]] = m[2].trim()
  }
  return trovati
}

describe('la palette scura riproduce quella della versione Swift', () => {
  const dichiarati = tokenDichiarati()

  for (const [nome, atteso] of Object.entries(PALETTE_SWIFT)) {
    test(`${nome} vale ${atteso}`, () => {
      expect(dichiarati[nome]?.toUpperCase()).toBe(atteso)
    })
  }

  test('ogni token dichiarato ha un riferimento Swift', () => {
    const senzaRiferimento = Object.keys(dichiarati).filter(
      (nome) => !(nome in PALETTE_SWIFT) && !nome.startsWith('--t-space-') &&
        !nome.startsWith('--t-radius-') && !nome.startsWith('--t-text-size-')
    )
    expect(senzaRiferimento).toEqual([])
  })

  test('le superfici traslucide condividono una sola opacita', () => {
    expect(dichiarati['--t-translucency']).toBe(String(OPACITA_TRASLUCIDA))
  })
})
```

Il secondo test è quello che conta più del primo: senza, si potrebbe aggiungere
un colore nuovo che non risponde a nessuno.

- [ ] **Passo 2: verificare il rosso**

Run: `npx vitest run src/renderer/src/lib/design/`
Expected: FAIL — `Cannot find module './palette.ts'`

- [ ] **Passo 3: scrivere il riferimento**

`src/renderer/src/lib/design/palette.ts`:

```ts
/**
 * I valori della palette scura della versione Swift, alla data del porting.
 *
 * Estratti da `App/AppTheme.swift` e `TillerCore/AppSurfaceColor.swift`. Sono
 * dati, non stile: servono al test di fedelta'. La migrazione aveva copiato
 * `#1F1F26`, valore vero fino al commit Swift `a0f5fcf` e sostituito da
 * `8368529`; nessun test poteva accorgersene, perche' un colore scuro plausibile
 * supera qualunque criterio che non lo confronti con la fonte.
 */
export const PALETTE_SWIFT: Record<string, string> = {
  '--t-surface-chrome': '#1A1A1E',
  '--t-surface-content': '#121216',
  '--t-surface-card': '#282A35',
  '--t-surface-field': '#14151B',
  '--t-surface-pill': '#33343D',
  '--t-row-hover': '#20232D',
  '--t-row-selected': '#2B2F3A',
  '--t-row-ring': '#3A4050',
  '--t-hairline': '#40424F',
  '--t-text-selected': '#FFFFFF',
  '--t-text-title': '#D9DBE3',
  '--t-text-subtitle': '#B8BDD1',
  '--t-text-meta': '#A8ADC4',
  '--t-git-staged': '#8CD1A1',
  '--t-git-modified': '#E8B05C',
  '--t-git-untracked': '#6EADE8',
  '--t-git-conflict': '#E69499',
  '--t-diff-add-bg': '#143D24',
  '--t-diff-del-bg': '#45141A',
  '--t-diff-hunk-bg': '#1A2B47',
  '--t-rail-task': '#7D6BD6',
  '--t-rail-tool': '#666B80'
}

/** `AppSurfaceColor.translucentSurfaceOpacity`. Era 0.88, sbagliato. */
export const OPACITA_TRASLUCIDA = 0.96
```

- [ ] **Passo 4: scrivere il foglio**

`src/renderer/src/assets/tokens.css`, blocco `:root`, con esattamente i 22 token
sopra più `--t-translucency: 0.96`. Valori esadecimali **maiuscoli**, per far
combaciare il confronto senza normalizzazioni che nascondono differenze.

Importarlo da `main.css` **prima** di `base.css`.

- [ ] **Passo 5: verificare il verde**

Run: `npx vitest run src/renderer/src/lib/design/`
Expected: PASS

- [ ] **Passo 6: committare**

```bash
git add src/renderer/src/lib/design/ src/renderer/src/assets/tokens.css src/renderer/src/assets/main.css
git commit -m "feat: restore the swift dark palette as tokens"
```

---

## Task 2: Le scale, e la scansione che le rende obbligatorie

**File:**
- Modifica: `src/renderer/src/assets/tokens.css` (aggiunge le tre scale)
- Crea: `src/renderer/src/lib/design/scale-guard.test.ts`

**Interfacce:**
- Consuma: `tokens.css` dal Task 1.
- Produce: nulla di importabile. La scansione è il deliverable.

**Questo task finisce ROSSO, ed è previsto.** La scansione trova i valori
letterali che i componenti usano oggi; il Task 3 li toglie. Un task che
consegnasse verde qui avrebbe scritto una scansione che non guarda niente.

- [ ] **Passo 1: aggiungere le scale a `tokens.css`**

```css
  /* Spaziature. Il 10 della versione Swift si risolve in 8 dentro un elemento
     e in 12 fra elementi: tenerli entrambi ripeterebbe la deriva 6/7. */
  --t-space-1: 2px;
  --t-space-2: 4px;
  --t-space-3: 6px;
  --t-space-4: 8px;
  --t-space-5: 12px;
  --t-space-6: 16px;
  --t-space-7: 24px;
  --t-space-8: 32px;

  /* Raggi. Il 7 confluisce nel 6: 41 usi diventano un valore solo. */
  --t-radius-1: 4px;
  --t-radius-2: 6px;
  --t-radius-3: 8px;
  --t-radius-4: 12px;

  /* Testo. Base 11, il piu' frequente nella versione Swift (27 usi). */
  --t-text-size-1: 10px;
  --t-text-size-2: 11px;
  --t-text-size-3: 12px;
  --t-text-size-4: 13px;
  --t-text-size-5: 15px;
  --t-text-size-6: 17px;
```

- [ ] **Passo 2: scrivere la scansione**

`src/renderer/src/lib/design/scale-guard.test.ts`:

```ts
import { describe, expect, test } from 'vitest'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { join } from 'node:path'

const RADICE = join(__dirname, '../..')

/** Proprieta' governate dal sistema. La larghezza dei bordi non lo e'. */
const PROPRIETA = /(padding|margin|gap|border-radius|font-size)[a-z-]*:\s*([^;{}]+)/g
const ESADECIMALE = /#[0-9a-fA-F]{3,8}\b/

/**
 * `tokens.css` dichiara i valori: e' l unico posto dove i letterali sono la
 * definizione invece che una scorciatoia. `terminal-theme.ts` non compare
 * perche' non e' ne' `.svelte` ne' `.css` — il terminale ha i propri colori e
 * non passa dai token, ed e' un esclusione voluta, non una dimenticanza.
 */
const ESCLUSI = new Set(['tokens.css'])

function fogli(dir: string, out: string[] = []): string[] {
  for (const voce of readdirSync(dir)) {
    const percorso = join(dir, voce)
    if (statSync(percorso).isDirectory()) fogli(percorso, out)
    else if (/\.(svelte|css)$/.test(voce) && !ESCLUSI.has(voce)) out.push(percorso)
  }
  return out
}

describe('nessuna scorciatoia intorno ai token', () => {
  const file = fogli(RADICE)

  test('ci sono fogli da controllare', () => {
    expect(file.length).toBeGreaterThan(0)
  })

  for (const percorso of file) {
    const nome = percorso.slice(RADICE.length + 1)
    const testo = readFileSync(percorso, 'utf8')

    test(`${nome} non usa colori letterali`, () => {
      const righe = testo
        .split('\n')
        .map((riga, i) => ({ riga, n: i + 1 }))
        .filter(({ riga }) => ESADECIMALE.test(riga))
        .map(({ riga, n }) => `${n}: ${riga.trim()}`)
      expect(righe).toEqual([])
    })

    test(`${nome} non usa misure fuori scala`, () => {
      const fuori: string[] = []
      for (const m of testo.matchAll(PROPRIETA)) {
        const valore = m[2]
        if (valore.includes('var(--t-')) continue
        for (const px of valore.matchAll(/\b(\d+)px\b/g)) {
          if (px[1] !== '0') fuori.push(`${m[1]}: ${valore.trim()}`)
        }
      }
      expect(fuori).toEqual([])
    })
  }
})
```

Il primo test — «ci sono fogli da controllare» — non è di contorno: senza,
una scansione che non trova nessun file passerebbe verde su zero controlli, ed è
il modo più silenzioso che una verifica ha di sparire.

- [ ] **Passo 3: verificare il rosso, e contarlo**

Run: `npx vitest run src/renderer/src/lib/design/scale-guard.test.ts`
Expected: FAIL su più file. **Annotare quanti**: è la misura del lavoro del
Task 3, e serve a sapere che alla fine sono stati sistemati tutti.

- [ ] **Passo 4: committare**

```bash
git add src/renderer/src/assets/tokens.css src/renderer/src/lib/design/scale-guard.test.ts
git commit -m "feat: add the spacing radius and type scales with a guard"
```

Il commit lascia la suite rossa. È voluto e va scritto nel messaggio esteso.

---

## Task 3: Migrare i componenti, e togliere i token morti

**File:**
- Modifica: ogni `.svelte` e `.css` che la scansione del Task 2 segnala
- Modifica: `src/renderer/src/assets/base.css` (toglie i `--ev-*` e i vecchi `--tiller-*`)
- Crea: `src/renderer/src/lib/design/dead-tokens.test.ts`

- [ ] **Passo 1: scrivere il test dei token morti**

```ts
import { expect, test } from 'vitest'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const base = readFileSync(join(__dirname, '../../assets/base.css'), 'utf8')

test('i token del template electron-vite sono spariti', () => {
  expect(base).not.toMatch(/--ev-/)
})

test('i vecchi token --tiller-* sono spariti', () => {
  expect(base).not.toMatch(/--tiller-surface|--tiller-text|--tiller-status/)
})
```

- [ ] **Passo 2: verificare il rosso**

Run: `npx vitest run src/renderer/src/lib/design/dead-tokens.test.ts`
Expected: FAIL, entrambi.

- [ ] **Passo 3: migrare**

Sostituire ogni valore segnalato col token corrispondente. Regole di conversione,
da applicare senza inventare:

| Trovato | Diventa |
|---|---|
| `2px` | `var(--t-space-1)` |
| `3px`, `4px`, `5px` | `var(--t-space-2)` (spaziatura) o `var(--t-radius-1)` (raggio) |
| `6px`, `7px` | `var(--t-space-3)` / `var(--t-radius-2)` |
| `8px`, `9px`, `10px` dentro un elemento | `var(--t-space-4)` / `var(--t-radius-3)` |
| `10px`, `12px`, `14px` fra elementi | `var(--t-space-5)` |
| `16px` | `var(--t-space-6)` |
| `--tiller-surface-chrome` | `--t-surface-chrome` |
| `--tiller-surface-workspace` | `--t-surface-content` |
| `--tiller-text-primary` | `--t-text-title` |
| `--tiller-text-secondary` | `--t-text-subtitle` |
| `--tiller-text-muted` | `--t-text-meta` |
| `--tiller-border` | `--t-hairline` |

`--tiller-topbar-height` e `--tiller-statusbar-height` restano: sono altezze di
componenti, non spaziature, e non appartengono alla scala.

**Se una conversione non è ovvia, non tirare a indovinare: dichiarala nel
report.** Un valore convertito male è peggio del valore originale, perché adesso
sembra deliberato.

**Il tema chiaro va tenuto coerente**, non riprogettato: ogni token scuro nuovo
ha la sua controparte chiara in `[data-theme='light']`, ricavata dai valori chiari
di `AppTheme.swift`.

- [ ] **Passo 4: verificare il verde**

```bash
npx vitest run src/renderer/src/lib/design/
```
Expected: PASS — palette, scansione e token morti, tutti e tre.

- [ ] **Passo 5: committare**

```bash
git add -A
git commit -m "refactor: move every surface onto the design tokens"
```

---

## Chiusura

Riportare: i commit; l'output di `npx vitest run src/renderer/`, `npm run
typecheck`, `npm run lint` (0 errori, warning non oltre 26); **quanti file la
scansione segnalava al Task 2 e quanti ne restano**; ogni conversione non ovvia
e come è stata decisa; ogni punto del piano trovato sbagliato, detto invece che
aggirato.

**Nessun criterio end-to-end in questo piano, ed è dichiarato.** Un token non ha
comportamento: non c'è niente da gesticolare. Quello che i test non coprono — che
il risultato sia bello, e che le proporzioni reggano — si vede guardando, ed è la
verifica manuale che segue. Nessuno ha ancora guardato questa applicazione con i
propri occhi.
