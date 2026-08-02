# Fase 6b-3 — Markdown: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** anteprima Markdown, passaggio anteprima/codice e barra di
formattazione, sopra l'editor che la 6b-2 ha già consegnato.

**Architettura:** le trasformazioni del testo sono funzioni pure e si testano
senza interfaccia; il rendering passa per due strati di difesa, perché il testo
reso viene da file che un agente ha appena scritto.

**Spec:** `docs/superpowers/specs/2026-08-02-electron-fase-6b3-markdown-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo.
- `npm run lint` deve dare **0 errori**.
- Due dipendenze nuove, e solo due: `markdown-it` e `dompurify`.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/renderer/src/lib/workspace/markdown-format.ts` | *nuovo*: trasformazioni pure sulla selezione |
| `src/renderer/src/lib/workspace/markdown-render.ts` | *nuovo*: sorgente → HTML ripulito |
| `src/renderer/src/lib/workspace/MarkdownToolbar.svelte` | *nuovo*: i pulsanti |
| `src/renderer/src/lib/workspace/FileTab.svelte` | *modifica*: modalità, anteprima, barra |
| `e2e/fase-6b3-markdown.spec.ts` | *nuovo*: i sei criteri |

---

## Task 1: Le trasformazioni del testo

**File:**
- Crea: `src/renderer/src/lib/workspace/markdown-format.ts`
- Test: `src/renderer/src/lib/workspace/markdown-format.test.ts`

**Interfacce:**
- Produce: `avvolgi`, `intestazione`, `elenco`, `collegamento`. Tutte con firma
  `(text: string, selezione: Selezione, ...) => Risultato`, dove
  `Selezione = { from: number; to: number }` e
  `Risultato = { text: string; selection: Selezione }`.
  Le consuma il Task 3.

Funzioni **pure**: nessun accesso a CodeMirror, nessun DOM. È il motivo per cui
sono un task a sé — i casi limite stanno qui, e qui si testano senza far partire
un'applicazione.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('avvolgere una selezione piena la mette fra i marcatori', () => {
  const esito = avvolgi('ciao mondo', { from: 5, to: 10 }, '**')
  expect(esito.text).toBe('ciao **mondo**')
  expect(esito.selection).toEqual({ from: 7, to: 12 })
})

test('avvolgere una selezione vuota lascia il cursore in mezzo', () => {
  const esito = avvolgi('ciao ', { from: 5, to: 5 }, '**')
  expect(esito.text).toBe('ciao ****')
  expect(esito.selection).toEqual({ from: 7, to: 7 })
})

test('avvolgere due volte toglie il grassetto invece di annidarlo', () => {
  const esito = avvolgi('ciao **mondo**', { from: 7, to: 12 }, '**')
  expect(esito.text).toBe('ciao mondo')
  expect(esito.selection).toEqual({ from: 5, to: 10 })
})

test('l intestazione si applica a ogni riga della selezione', () => {
  const esito = intestazione('uno\ndue', { from: 0, to: 7 }, 2)
  expect(esito.text).toBe('## uno\n## due')
})

test('l intestazione sostituisce un livello gia presente', () => {
  const esito = intestazione('# uno', { from: 0, to: 5 }, 3)
  expect(esito.text).toBe('### uno')
})

test('l elenco marca ogni riga selezionata', () => {
  const esito = elenco('uno\ndue', { from: 0, to: 7 })
  expect(esito.text).toBe('- uno\n- due')
})

test('il collegamento avvolge il testo e lascia il cursore nell url', () => {
  const esito = collegamento('vai qui', { from: 4, to: 7 })
  expect(esito.text).toBe('vai [qui]()')
  expect(esito.selection).toEqual({ from: 10, to: 10 })
})
```

Il terzo test è quello che si dimentica: **premere grassetto due volte deve
togliere il grassetto.** Una barra che annida `****testo****` a ogni click è
peggio di una barra assente, perché sembra funzionare.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.
- [ ] **Passo 3: implementare.**
- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git add src/renderer/src/lib/workspace/markdown-format.ts src/renderer/src/lib/workspace/markdown-format.test.ts
git commit -m "feat: transform a markdown selection"
```

---

## Task 2: Il rendering, con due strati di difesa

**File:**
- Crea: `src/renderer/src/lib/workspace/markdown-render.ts`
- Test: `src/renderer/src/lib/workspace/markdown-render.test.ts`
- Modifica: `package.json` (`markdown-it`, `dompurify`)

**Interfacce:**
- Produce: `renderMarkdown(sorgente: string): string`, che restituisce HTML
  **già ripulito**. Lo consuma il Task 3.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('un titolo diventa un titolo', () => {
  expect(renderMarkdown('# Ciao')).toContain('<h1>Ciao</h1>')
})

test('l html grezzo nel sorgente non arriva nel risultato', () => {
  const esito = renderMarkdown('<img src=x onerror="alert(1)">')
  expect(esito).not.toContain('onerror')
})

test('nemmeno con l html abilitato passa un gestore di evento', () => {
  // Stessa sorgente, ma costruendo il renderer con html: true.
  // Il secondo strato deve reggere da solo.
  const esito = renderMarkdownConHtmlAbilitato('<img src=x onerror="alert(1)">')
  expect(esito).not.toContain('onerror')
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

`markdown-it` con `html: false`, poi `DOMPurify.sanitize` sul risultato.

**Il terzo test è il senso del task.** Il testo reso viene da file del worktree,
cioè — in questa applicazione — da file che un agente ha appena scritto. Un
`README.md` con `<img src=x onerror=...>` diventerebbe esecuzione di script
dentro il renderer di Electron, che ha accesso al bridge del preload.

Due strati perché il primo si disattiva con una riga di configurazione dall'aria
innocua, e il secondo è quello che regge quando qualcuno la scrive. Il terzo test
verifica proprio il secondo strato da solo, e per questo il modulo deve esporre
una via per costruire il renderer con `html: true` **a soli fini di test**.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: committare**

```bash
git add package.json pnpm-lock.yaml src/renderer/src/lib/workspace/markdown-render.ts src/renderer/src/lib/workspace/markdown-render.test.ts
git commit -m "feat: render markdown without letting html through"
```

---

## Task 3: Il tab Markdown

**File:**
- Crea: `src/renderer/src/lib/workspace/MarkdownToolbar.svelte`
- Modifica: `src/renderer/src/lib/workspace/FileTab.svelte`
- Crea: `e2e/fase-6b3-markdown.spec.ts`

- [ ] **Passo 1: scrivere i criteri e2e 1-6** e lasciarli rossi.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

- Il file è Markdown se l'estensione è `.md` o `.markdown`, senza distinzione fra
  maiuscole e minuscole.
- Un `.md` si apre in **anteprima**; l'intestazione ha il passaggio anteprima ⇄
  codice. La modalità è stato **locale del componente**: non è una proprietà del
  documento ma della vista, e due tab sullo stesso file possono legittimamente
  mostrarne uno per parte.
- La barra è visibile **solo** in modalità codice.
- I pulsanti leggono la selezione dalla vista CodeMirror, chiamano la funzione
  pura del Task 1, e applicano il risultato con una **transazione** — la stessa
  strada che la 6b-2 usa già per la ricarica esterna. Nessun accesso al DOM, e
  soprattutto niente ricreazione della vista: perderebbe selezione e scorrimento.
- Oltre **2 MB** il file si apre in codice, con un avviso nell'intestazione, e il
  passaggio ad anteprima resta disponibile ma non è il valore iniziale. La misura
  è in byte: `new TextEncoder().encode(text).length`, come già in `DiffTab`.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.

- [ ] **Passo 5: mutazioni**

| Mutazione | Rosso atteso |
| --- | --- |
| `html: false` diventa `html: true` | **nessuno** — è il punto: il secondo strato regge |
| si toglie **anche** `DOMPurify` | **solo** 5 |
| `avvolgi` restituisce la selezione vecchia | **solo** 4 |
| il limite morbido viene ignorato | **solo** 6 |

La prima riga è deliberata e va riportata così: un solo strato che regge non prova
che l'altro sia inutile, prova che gli strati sono due.

- [ ] **Passo 6: committare**

```bash
git add src/renderer/src/lib/workspace/MarkdownToolbar.svelte src/renderer/src/lib/workspace/FileTab.svelte e2e/fase-6b3-markdown.spec.ts
git commit -m "feat: preview and format markdown in its tab"
```

---

## Chiusura

Riportare: i commit; `npx vitest run src/renderer/src/lib/workspace/`;
`npm run typecheck` e `npm run lint` (0 errori); l'esito di ogni mutazione col
criterio diventato rosso; ogni punto del piano trovato sbagliato; e ciò che non si
è riusciti a verificare.

Fuori scope: creazione di nuovi file Markdown, WYSIWYG, apertura da `⌘O` o drag &
drop, persistenza della modalità fra riavvii.
