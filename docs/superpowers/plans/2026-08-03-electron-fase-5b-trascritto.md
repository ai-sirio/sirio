# Fase 5b — Il trascritto: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** trasformare il flusso di eventi della 5a in un trascritto
renderizzabile e persistibile.

**Architettura:** una macchina a stati **pura** — `applica(stato, evento)` che
restituisce uno stato nuovo — più due derivati puri e la persistenza degli
elementi. Nessun import dal processo main, nessuna riga di Svelte.

**Spec:** `docs/superpowers/specs/2026-08-03-electron-fase-5b-trascritto-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- `src/shared/chat/` non importa **mai** da `src/main/`.
- **Stato immutabile.** Ogni funzione restituisce un oggetto nuovo. La versione
  Swift muta una `struct`, che è comunque un valore: in TypeScript la stessa
  semantica si ottiene solo restituendo una copia.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo.
- `npm run lint` deve dare **0 errori**.
- Nessuna dipendenza nuova.

## Struttura dei file

| File | Responsabilità |
| --- | --- |
| `src/shared/chat/transcript-item.ts` | *nuovo*: l'unione degli elementi e `ToolCallItem` |
| `src/shared/chat/transcript-reducer.ts` | *nuovo*: la macchina a stati |
| `src/shared/chat/tool-call-tree.ts` | *nuovo*: radici e figli, in un passaggio |
| `src/shared/chat/chat-question.ts` | *nuovo*: la domanda normalizzata |
| `src/main/chat/transcript-store.ts` | *nuovo*: lettura/scrittura di `chatItem` |
| `src/main/db/migrations/` | *modifica*: la tabella `chatItem` |

**Perché niente criteri e2e in questa fase.** Non c'è nulla da gesticolare:
nessuna interfaccia, nessun processo. I criteri arrivano con la 5c, dove il
trascritto diventa visibile. Qui la verifica è per intero nei test unitari, ed è
il motivo per cui la fase può essere scritta e provata in un pomeriggio.

---

## Task 1: Gli elementi del trascritto

**File:**
- Crea: `src/shared/chat/transcript-item.ts`
- Test: `src/shared/chat/transcript-item.test.ts`

**Interfacce:**
- Produce: `TranscriptItem` (unione discriminata su `kind`), `ToolCallItem`,
  `PermissionState`. Li consumano tutti i task successivi.

```ts
export type TranscriptItem =
  | { kind: 'userMessage'; id: string; blocks: ContentBlock[] }
  | { kind: 'agentMessage'; id: string; text: string; isComplete: boolean }
  | { kind: 'thought'; id: string; text: string }
  | { kind: 'toolCall'; id: string; call: ToolCallItem }
  | { kind: 'plan'; id: string; entries: PlanEntry[] }
  | { kind: 'editSummary'; id: string; paths: string[] }
  | { kind: 'turnDivider'; id: string; at: number }
```

`at` è un numero di millisecondi, non una `Date`: attraversa il socket di
controllo e finisce in SQLite, e una `Date` in JSON è già una stringa a cui
qualcuno dovrà ridare un tipo.

- [ ] **Passo 1: scrivere i test che falliscono**

Schemi Zod con andata e ritorno per ogni membro, e — come in tutte le fasi
precedenti — un test che un payload **senza i campi facoltativi** resta valido.
`ToolCallItem` ha `toolCallId` obbligatorio e tutto il resto facoltativo: è la
regola della versione Swift, e serve perché gli aggiornamenti arrivano parziali.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.
- [ ] **Passo 3: implementare.**
- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: model the items of a chat transcript"
```

---

## Task 2: Il riduttore, parte prima — i flussi

**File:**
- Crea: `src/shared/chat/transcript-reducer.ts`
- Test: `src/shared/chat/transcript-reducer.test.ts`

**Interfacce:**
- Produce: `statoIniziale(idEsistenti: string[]): TranscriptState`,
  `applica(stato, evento): TranscriptState`,
  `utentePrompt(stato, blocks, quando): TranscriptState`.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('i pezzi dell agente si accumulano in un solo elemento', () => {
  let s = statoIniziale([])
  s = applica(s, { kind: 'agentMessageChunk', block: { type: 'text', text: 'ciao ' } })
  s = applica(s, { kind: 'agentMessageChunk', block: { type: 'text', text: 'mondo' } })

  expect(s.items).toHaveLength(1)
  expect(s.items[0]).toMatchObject({ kind: 'agentMessage', text: 'ciao mondo', isComplete: false })
})

test('un pensiero chiude il messaggio dell agente aperto', () => {
  let s = statoIniziale([])
  s = applica(s, { kind: 'agentMessageChunk', block: { type: 'text', text: 'ciao' } })
  s = applica(s, { kind: 'agentThoughtChunk', block: { type: 'text', text: 'penso' } })
  s = applica(s, { kind: 'agentMessageChunk', block: { type: 'text', text: 'altro' } })

  // Tre elementi, non due: il messaggio chiuso non riapre.
  expect(s.items.map((i) => i.kind)).toEqual(['agentMessage', 'thought', 'agentMessage'])
  expect(s.items[0]).toMatchObject({ isComplete: true })
})

test('i pezzi del messaggio utente ricostruiscono il messaggio ripetuto', () => {
  // Non arrivano MAI in un turno dal vivo: arrivano solo quando session/load
  // ripete la conversazione. Il ramo esiste per quello, e cancellarlo "perche'
  // non si verifica mai" rompe il ripristino senza rompere nessun altro test.
  let s = statoIniziale([])
  s = applica(s, { kind: 'userMessageChunk', block: { type: 'text', text: 'una ' } })
  s = applica(s, { kind: 'userMessageChunk', block: { type: 'text', text: 'domanda' } })

  expect(s.items).toHaveLength(1)
  expect(s.items[0]).toMatchObject({ kind: 'userMessage' })
})

test('applica non muta lo stato che riceve', () => {
  const prima = statoIniziale([])
  const dopo = applica(prima, { kind: 'agentMessageChunk', block: { type: 'text', text: 'x' } })

  expect(prima.items).toHaveLength(0)
  expect(dopo).not.toBe(prima)
})

test('gli id non collidono con quelli gia persistiti', () => {
  let s = statoIniziale(['agent-0'])
  s = applica(s, { kind: 'agentMessageChunk', block: { type: 'text', text: 'x' } })

  expect(s.items[0].id).not.toBe('agent-0')
})

test('un evento sconosciuto non cambia nulla e non alza', () => {
  const prima = statoIniziale([])
  expect(applica(prima, { kind: 'unknown' }).items).toEqual([])
})
```

Il quarto test è quello che protegge l'architettura: se qualcuno rende il
riduttore mutante «per efficienza», è l'unico che se ne accorge.

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.
- [ ] **Passo 3: implementare.**

Lo stato tiene gli indici dei tre flussi aperti — messaggio dell'agente,
pensiero, messaggio utente. Aprirne uno chiude gli altri. Senza questa regola il
trascritto diventa un elemento per token.

- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: fold streaming chunks into transcript items"
```

---

## Task 3: Il riduttore, parte seconda — le chiamate a strumento

**File:**
- Modifica: `src/shared/chat/transcript-reducer.ts`
- Test: `src/shared/chat/transcript-reducer.test.ts`

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('un toolCall per un id gia visto non azzera il permesso in attesa', () => {
  // La scorciatoia ovvia — sostituire l'elemento intero — cancella il permesso
  // mentre l'utente lo sta guardando. Permesso, output del terminale, stato
  // d'uscita e identita' di presentazione arrivano da altri canali.
  let s = statoIniziale([])
  s = applica(s, { kind: 'toolCall', call: { toolCallId: 't1', title: 'Read', status: 'pending' } })
  s = applica(s, {
    kind: 'permissionRequested',
    requestId: 1,
    toolCallId: 't1',
    options: [{ id: 'allow_once', label: 'Allow' }]
  })
  s = applica(s, { kind: 'toolCall', call: { toolCallId: 't1', title: 'Read', status: 'inProgress' } })

  const call = s.items.find((i) => i.kind === 'toolCall')
  expect(call?.call.permission?.isPending).toBe(true)
})

test('un aggiornamento per una chiamata mai vista crea una scheda minima', () => {
  // Riconnessione a metà flusso: meglio una scheda povera che un'informazione
  // persa.
  let s = statoIniziale([])
  s = applica(s, { kind: 'toolCallUpdate', update: { toolCallId: 'ignoto', status: 'completed' } })

  expect(s.items).toHaveLength(1)
  expect(s.items[0]).toMatchObject({ kind: 'toolCall' })
})

test('la fine del turno annulla i permessi rimasti in attesa', () => {
  // Un permesso non risposto entro la fine del turno non e' piu' rispondibile:
  // lasciarlo in attesa mostra pulsanti che non fanno nulla.
  let s = statoIniziale([])
  s = applica(s, { kind: 'toolCall', call: { toolCallId: 't1', title: 'Write', status: 'pending' } })
  s = applica(s, { kind: 'permissionRequested', requestId: 1, toolCallId: 't1', options: [] })
  s = applica(s, { kind: 'turnEnded', reason: 'endTurn', at: 1000 })

  const call = s.items.find((i) => i.kind === 'toolCall')
  expect(call?.call.permission?.resolution).toEqual({ kind: 'cancelled' })
})

test('la fine del turno riepiloga i file modificati nel turno', () => {
  let s = utentePrompt(statoIniziale([]), [{ type: 'text', text: 'vai' }], 0)
  s = applica(s, {
    kind: 'toolCall',
    call: { toolCallId: 't1', title: 'Edit', kind: 'edit', status: 'completed',
            locations: [{ path: 'src/a.ts' }] }
  })
  s = applica(s, { kind: 'turnEnded', reason: 'endTurn', at: 1000 })

  const riepilogo = s.items.find((i) => i.kind === 'editSummary')
  expect(riepilogo).toMatchObject({ paths: ['src/a.ts'] })
})

test('la lista dei file modificati si svuota fra un turno e l altro', () => {
  // Due turni di fila: il secondo riepilogo non ripete i file del primo.
})

test('la durata del turno esiste ma non e parte degli elementi', () => {
  // turnDurations e' stato di interfaccia: si mostra e non si ricostruisce.
})
```

- [ ] **Passo 2: eseguire e vedere fallire** → FAIL.
- [ ] **Passo 3: implementare.**
- [ ] **Passo 4: eseguire e vedere passare** → PASS.
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: fold tool calls and turn boundaries into the transcript"
```

---

## Task 4: L'albero delle chiamate

**File:**
- Crea: `src/shared/chat/tool-call-tree.ts`
- Test: `src/shared/chat/tool-call-tree.test.ts`

**Interfacce:**
- Produce: `raggruppa(items): { roots, children, pendingPlanApproval }`.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('i figli finiscono sotto il loro genitore', () => { /* ... */ })

test('un genitore sconosciuto lascia il figlio al suo posto nel flusso', () => {
  // L'informazione non sparisce perche' manca un elemento.
  const g = raggruppa([toolCall('t2', { parentToolCallId: 'mai-visto' })])
  expect(g.roots).toHaveLength(1)
})

test('l approvazione del piano in attesa si raccoglie durante il passaggio', () => {
  // Raccolta qui perche' le viste non devono riscandire il trascritto.
  const g = raggruppa([toolCall('t1', { kind: 'switchMode', permission: inAttesa })])
  expect(g.pendingPlanApproval).not.toBeNull()
})

test('raggruppa fa un solo passaggio sugli elementi', () => {
  // Un contatore su un getter, oppure una lista lunga con un tetto di tempo:
  // il punto e' che la funzione si chiama una volta per cambiamento, mai per
  // scheda renderizzata. E' la differenza fra una chat lunga che scorre e una
  // che si impunta.
})
```

- [ ] **Passo 2: rosso.** - [ ] **Passo 3: implementare.** - [ ] **Passo 4: verde.**
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: group tool calls into roots and children"
```

---

## Task 5: La domanda normalizzata

**File:**
- Crea: `src/shared/chat/chat-question.ts`
- Test: `src/shared/chat/chat-question.test.ts`

**Interfacce:**
- Produce: `domandaDa(call: ToolCallItem): ChatQuestion | null`.

Normalizza **due sorgenti diverse** in una forma sola: un input di strumento a
forma di `AskUserQuestion`, e una richiesta di permesso semplice. Le viste leggono
questa e non toccano mai i payload grezzi.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('un permesso semplice diventa una domanda con le sue opzioni', () => { /* ... */ })

test('un input a forma di AskUserQuestion diventa una domanda strutturata', () => { /* ... */ })

test('isStructured NON si deduce da un prompt non vuoto', () => {
  // `ui/select` di Pi ripete la propria intestazione nel prompt e scarta il
  // prompt vero: dedurre `isStructured` da li' funziona su tre agenti su
  // quattro. Si deduce dalla FORMA dell'input, non dal testo.
  const domanda = domandaDa(toolCall('t1', {
    permission: inAttesa,
    rawInput: { header: 'Scegli', prompt: 'Scegli' }
  }))
  expect(domanda?.isStructured).toBe(false)
})

test('una domanda annullata dalla fine del turno non offre piu le opzioni', () => {
  // Rioffrire i pulsanti sarebbe una bugia: nessuno risponderebbe.
})
```

- [ ] **Passo 2: rosso.** - [ ] **Passo 3: implementare.** - [ ] **Passo 4: verde.**
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: normalise a question from either source"
```

---

## Task 6: La persistenza degli elementi

**File:**
- Crea: `src/main/chat/transcript-store.ts`
- Test: `src/main/chat/transcript-store.test.ts`
- Modifica: la migrazione che aggiunge `chatItem`

**Interfacce:**
- Produce: `salvaItems(db, sessionId, items)`, `leggiItems(db, sessionId)`.

- [ ] **Passo 1: scrivere i test che falliscono**

```ts
test('gli elementi tornano nell ordine in cui sono stati salvati', () => { /* ... */ })

test('un elemento illeggibile si scarta da solo, senza perdere gli altri', () => {
  // Politica OPPOSTA a quella del codificatore del layout, e deliberatamente:
  // il layout e' una struttura e perderne un pezzo la rompe, il trascritto e'
  // una lista e un elemento illeggibile e' un buco, non una rottura.
})

test('le durate dei turni non finiscono nel database', () => {
  // Sono stato d'interfaccia: si mostrano e non si ricostruiscono.
})

test('ricaricando, il riduttore riparte con gli id gia usati', () => {
  // Altrimenti continuare una conversazione ripristinata produce due elementi
  // con lo stesso id, e l'interfaccia ne mostra uno solo.
  const items = await leggiItems(db, 's1')
  const s = statoIniziale(items.map((i) => i.id))
  // ... un chunk nuovo non riusa un id esistente
})
```

- [ ] **Passo 2: rosso.** - [ ] **Passo 3: implementare.** - [ ] **Passo 4: verde.**
- [ ] **Passo 5: committare**

```bash
git commit -m "feat: persist and restore transcript items"
```

---

## Task 7: Le mutazioni

Non è un task di codice: è il controllo che i test dei Task 2-6 misurino qualcosa.
Una per volta, annullando con `git checkout --`.

| Mutazione | Rosso atteso |
| --- | --- |
| ogni pezzo diventa un elemento nuovo | i test del Task 2 sull'accumulo |
| `applica` muta lo stato invece di copiarlo | il test di immutabilità, **e solo quello** |
| l'upsert sovrascrive l'elemento intero | il test del permesso in attesa |
| gli id non consultano l'insieme esistente | il test delle collisioni |
| la fine turno lascia i permessi in attesa | il test dell'annullamento |
| l'albero scarta il figlio col genitore ignoto | il test corrispondente |
| `isStructured` si deduce dal prompt non vuoto | il test di Pi |

La seconda riga merita attenzione: se mutare lo stato fa diventare rossi **molti**
test, va bene lo stesso, ma vuol dire che il test di immutabilità non sta
misurando l'immutabilità — sta misurando l'effetto collaterale più vistoso.
Riportalo così com'è.

- [ ] **Passo 1: eseguire le sette mutazioni, riportando l'output di ciascuna.**

---

## Chiusura

Riportare: i commit; `npx vitest run src/shared/chat/ src/main/chat/`;
`npm run typecheck` e `npm run lint` (0 errori); l'esito di ogni mutazione; ogni
punto del piano trovato sbagliato; e ciò che non si è riusciti a verificare.

Fuori scope: qualunque componente Svelte, il composer, le menzioni `@file`
(**5c**); i driver (**5a-2**); registry e installazione degli agenti (**Fase 7**).
