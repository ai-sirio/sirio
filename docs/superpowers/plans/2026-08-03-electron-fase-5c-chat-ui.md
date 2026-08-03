# Fase 5c — L'interfaccia della chat: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** rendere visibile il trascritto e collegare i driver della 5a al
riduttore della 5b.

**Architettura:** un solo derivato per cambiamento — lo scatto di presentazione —
consumato da schede che non riscandiscono mai il trascritto; il composer sopra; e
un task di cablaggio esplicito, con criteri propri.

**Spec:** `docs/superpowers/specs/2026-08-03-electron-fase-5c-chat-ui-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- Stringhe rivolte all'utente **in inglese**; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo.
- `npm run lint` deve dare **0 errori**.
- Nessuna dipendenza nuova: il rendering del Markdown è già in casa dalla 6b-3.
- **Ogni colore e ogni misura passano dai token** di `assets/tokens.css`
  (`--t-surface-*`, `--t-text-*`, `--t-space-*`, `--t-radius-*`,
  `--t-text-size-*`). Un esadecimale letterale o un `px` fuori scala in
  `padding`, `margin`, `gap`, `border-radius` o `font-size` fa fallire
  `src/renderer/src/lib/design/scale-guard.test.ts`. Non è una convenzione da
  ricordare: è un test che gira nel gate. Le superfici della chat sono
  `--t-surface-content` per il fondo e `--t-surface-card` per le schede.

## L'ordine dei task, e perché

Il cablaggio (**Task 2**) viene quasi per primo, prima delle schede e del
composer. È deliberato: la Fase 4a è stata un motore corretto che **nessuno
chiamava**, e se ne sono accorti solo i criteri end-to-end, a quindici task di
distanza. Qui il collegamento driver → riduttore → vista si stabilisce presto e i
task successivi lo trovano già vivo.

Regola che ne discende: **ogni voce in "dipende da" deve poter essere puntata a un
task che la consuma.** Se alla fine della fase resta qualcosa che nessuno chiama,
è un difetto, non un avanzo.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/renderer/src/lib/chat/presentation.ts` | *nuovo*: lo scatto, con la sua cache |
| `src/renderer/src/lib/chat/segmenter.ts` | *nuovo*: prosa contro riquadri `★ Insight` |
| `src/renderer/src/lib/chat/chat-store.svelte.ts` | *nuovo*: stato vivo per worktree |
| `src/renderer/src/lib/chat/ChatPane.svelte` | *nuovo*: la vista intera |
| `src/renderer/src/lib/chat/Transcript.svelte` | *nuovo*: la lista e lo scorrimento |
| `src/renderer/src/lib/chat/cards/*.svelte` | *nuovo*: una scheda per genere |
| `src/renderer/src/lib/chat/Composer.svelte` | *nuovo*: testo, chip, invio |
| `src/renderer/src/lib/chat/ComposerControlBar.svelte` | *nuovo*: agente, modello, modalità, sforzo |
| `src/main/chat/session-manager.ts` | *modifica*: emette gli eventi verso il renderer |
| `e2e/fase-5c-chat.spec.ts` | *nuovo*: gli otto criteri |

---

## Task 1: Lo scatto di presentazione

**File:**
- Crea: `src/renderer/src/lib/chat/presentation.ts`, `src/renderer/src/lib/chat/segmenter.ts`
- Test: `presentation.test.ts`, `segmenter.test.ts`

**Interfacce:**
- Consuma: `raggruppa` e gli elementi della 5b.
- Produce: `costruisciScatto(items, cachePrecedente) -> { scatto, cache }`,
  `segmenta(markdown) -> Segmento[]` con `Segmento = { kind: 'prosa' | 'insight'; text: string }`.
  Li consumano i Task 3 e 4.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('un riquadro Insight diventa un segmento a se', () => {
  const testo = 'prima\n\n`★ Insight ─────`\nil contenuto\n`─────`\n\ndopo'
  const segmenti = segmenta(testo)

  expect(segmenti.map((s) => s.kind)).toEqual(['prosa', 'insight', 'prosa'])
  expect(segmenti[1].text).toContain('il contenuto')
})

test('l involucro del riquadro e tollerato ma non preteso', () => {
  // Alcuni CLI avvolgono il riquadro in un blocco di codice, altri in un apice
  // singolo, altri in niente. Solo le righe di intestazione e chiusura contano.
  for (const apertura of ['```\n★ Insight ─────', '`★ Insight ─────', '★ Insight ─────']) {
    expect(segmenta(`${apertura}\nx\n─────`).some((s) => s.kind === 'insight')).toBe(true)
  }
})

test('la segmentazione si riusa quando id e valore non cambiano', () => {
  // Durante lo streaming cambia il testo dell'ULTIMO messaggio: risegmentare
  // tutto il trascritto a ogni consegna e' il modo piu' semplice di riprodurre
  // il blocco che questo progetto ha gia' curato due volte.
  let contatore = 0
  const spia = (t: string) => { contatore += 1; return segmenta(t) }

  const primo = costruisciScatto(items, nuovaCache(), spia)
  contatore = 0
  costruisciScatto(itemsConSoloLUltimoCambiato, primo.cache, spia)

  expect(contatore).toBe(1)
})
```

Il terzo test è il senso del task: misura il **costo**, non il risultato. Senza,
la ricostruzione eccessiva non fa diventare rosso niente e ricompare fra sei mesi
come "la chat si impunta".

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.
- [ ] **Passo 3: implementare.** La cache è chiavata su id **e** valore.
- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: derive one presentation snapshot per transcript change"
```

---

## Task 2: Il cablaggio

**File:**
- Modifica: `src/main/chat/session-manager.ts`
- Crea: `src/renderer/src/lib/chat/chat-store.svelte.ts`
- Crea: `e2e/fase-5c-chat.spec.ts` (i criteri 1 e 7)

**Interfacce:**
- Consuma: gli eventi dei driver (5a), `applica` e `statoIniziale` (5b).
- Produce: `chatStore` con `sessione(worktreeId)`, `manda(worktreeId, blocchi)`,
  `rispondiPermesso(...)`. Lo consumano i Task 3, 4 e 5.

**Questo è il task che nella 4a mancava.** Un motore e delle viste che non si
parlano compilano, passano i test unitari, e falliscono solo davanti a un utente.

- [ ] **Passo 1: scrivere i criteri e2e 1 e 7, e lasciarli rossi**

1. Scrivere un prompt e inviarlo mostra il messaggio dell'utente e poi la
   risposta dell'agente.
7. Riaprire un worktree mostra la conversazione precedente.

Il 7 è quello che misura davvero il cablaggio: coinvolge driver, riduttore,
persistenza e vista in un gesto solo.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.

- [ ] **Passo 3: implementare**

Gli eventi del driver arrivano al renderer **già coalescenti**: la 5a ha due
canali e un criterio che verifica che le consegne siano molto meno numerose dei
pezzi. Non aggiungere qui un secondo meccanismo di accumulo — sarebbero due
verità sullo stesso numero, e la seconda diverge.

Lo stato vivo è per **worktree**, come tutto il resto dell'applicazione.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: connect the chat drivers to the transcript and the view"
```

---

## Task 3: La lista e lo scorrimento

**File:**
- Crea: `src/renderer/src/lib/chat/ChatPane.svelte`, `Transcript.svelte`
- Modifica: `e2e/fase-5c-chat.spec.ts` (criteri 2 e 6)

- [ ] **Passo 1: scrivere i criteri 2 e 6, e lasciarli rossi**

2. La risposta appare **mentre arriva**, non tutta insieme alla fine.
6. Scorrere indietro durante lo streaming **non** riporta la vista in fondo.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

La conversazione si segue automaticamente **finché l'utente non scorre
indietro**. Riportarlo in fondo mentre legge qualcosa più su è il modo più rapido
di rendere inutilizzabile una chat lunga.

**Nessuna virtualizzazione.** Il blocco che questo progetto ha già vissuto veniva
dalla frequenza degli aggiornamenti, non dalla lunghezza della lista, ed è già
affrontato dalla coalescenza della 5a e dallo scatto del Task 1. Una finestra
scorrevole adesso curerebbe la malattia sbagliata, e romperebbe la selezione
continua del testo — che in Swift è costata un lavoro apposta per essere
mantenuta.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: show the transcript and follow it while it streams"
```

---

## Task 4: Le schede

**File:**
- Crea: `src/renderer/src/lib/chat/cards/` — chiamata a strumento, domanda, piano,
  task, riepilogo modifiche, output di terminale, anteprima diff, riquadro insight
- Modifica: `e2e/fase-5c-chat.spec.ts` (criteri 3, 4, 5)

- [ ] **Passo 1: scrivere i criteri 3, 4 e 5, e lasciarli rossi**

3. Una richiesta di permesso mostra la scheda con le sue opzioni; cliccarne una
   la risolve e la conversazione prosegue.
4. Un riquadro `★ Insight` diventa una scheda propria, separata dalla prosa.
5. Markdown pericoloso nella risposta dell'agente viene mostrato **senza** essere
   eseguito.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

- Le schede leggono i **valori normalizzati** della 5b — `domandaDa`, l'albero — e
  non toccano mai i payload grezzi.
- Il Markdown si rende con **`renderMarkdown` della 6b-3**, quello con i due
  strati. Non scriverne un secondo: qui il testo non è genericamente "di un
  agente", è **la risposta dell'agente**, che può aver letto contenuti di terze
  parti. Una seconda mappa stantia nella 6b-1 costava cinque linguaggi spenti;
  qui costerebbe l'esecuzione di script nel renderer.
- Il criterio 5 misura la proprietà vera — **nessun elemento con il gestore di
  evento, e lo script non parte** — non l'assenza di una parola nell'HTML. Cercare
  la parola sbaglia in entrambe le direzioni: fallisce su testo scappato, che è
  innocuo, e tace su un markup pericoloso che quella parola non ce l'ha.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: render one card per transcript item kind"
```

---

## Task 5: Il composer

**File:**
- Crea: `Composer.svelte`, `ComposerControlBar.svelte`
- Modifica: `e2e/fase-5c-chat.spec.ts` (criterio 8)

- [ ] **Passo 1: scrivere il criterio 8, e lasciarlo rosso**

8. Il composer manda un allegato insieme al testo.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

Il composer ha un **documento**, non una stringa: è quello che rende possibile un
allegato in mezzo al testo, ed è la forma che la versione Swift ha adottato dopo
averci provato con una stringa.

Le menzioni `@file` usano l'indice dei file — che appartiene a questa fase perché
appartiene al composer.

La barra di controllo espone agente, modello, modalità e livello di sforzo, e i
loro valori vengono dalla sessione della 5a: **non** ricostruirli da un catalogo
locale. I quattro driver espongono capacità diverse, e alcuni non hanno affatto un
catalogo di modelli: un valore mancante si mostra come mancante.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: compose a prompt with text and attachments"
```

---

## Task 6: Le mutazioni

| Mutazione | Rosso atteso |
| --- | --- |
| il cablaggio driver → riduttore viene tolto | 1, 2, 3, 7 |
| lo scatto si ricostruisce per scheda invece che per cambiamento | **nessun criterio** — lo prende il test del Task 1 |
| la segmentazione dei riquadri viene disattivata | **solo** 4 |
| il rendering perde la ripulitura | **solo** 5 |
| la vista torna sempre in fondo | **solo** 6 |

La seconda riga è la parte onesta: ricostruire troppo spesso non cambia il
risultato, cambia il costo, e un criterio end-to-end che dipende da una soglia di
tempo su una macchina condivisa è un generatore di falsi rossi. Per questo il
Task 1 ha un test che **conta** le ricostruzioni.

- [ ] **Passo 1: eseguire le cinque mutazioni, riportando l'output di ciascuna.**

---

## Chiusura

Riportare: i commit; `npx vitest run src/renderer/src/lib/chat/`;
`npm run typecheck` e `npm run lint` (0 errori); l'esito di ogni mutazione; ogni
punto del piano trovato sbagliato; e ciò che non si è riusciti a verificare.

Fuori scope: registro e installazione degli agenti, impostazioni degli account e
quote (**Fase 7**); il pannello degli agenti nella barra laterale destra, che
dipende dai subagent e arriva dopo che la chat funziona.
