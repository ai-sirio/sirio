# Fase 5b — Il trascritto: design

Data: 2026-08-03
Stato: proposto

## Obiettivo

Trasformare il flusso di eventi che la 5a produce in un trascritto renderizzabile
e persistibile: una macchina a stati pura, i suoi derivati, e il salvataggio degli
elementi.

## Il confine con le fasi vicine

La 5a **produce** eventi e persiste la *sessione* — agente, modello, modalità,
`acpSessionId`. La 5b **consuma** quegli eventi e persiste il *contenuto*. La 5c
lo disegna.

Il taglio è netto e regge una prova semplice: la 5b non importa nulla dal
processo main e non contiene una riga di Svelte. Se per testarla servisse far
partire un driver o montare un componente, il confine sarebbe nel posto sbagliato.

## La decisione portante: una macchina a stati pura

Il riduttore Swift (`TranscriptReducer.swift`, 266 righe) si descrive da solo:
«No I/O, no concurrency — fully unit-testable». Si porta con la stessa proprietà,
e non è una preferenza stilistica: è ciò che rende la fase verificabile **senza
CLI, senza processi e senza quota consumata**.

In TypeScript la forma naturale è una funzione `applica(stato, evento) -> stato`
con stato immutabile, non una classe con metodi mutanti. La versione Swift muta
una `struct` — che è comunque un valore, quindi la semantica è la stessa; in
TypeScript la stessa semantica si ottiene solo restituendo un oggetto nuovo.

## Gli elementi del trascritto

Unione discriminata, un membro per genere:

```
userMessage   { id, blocks }
agentMessage  { id, text, isComplete }
thought       { id, text }
toolCall      ToolCallItem
plan          { id, entries }
editSummary   { id, paths }
turnDivider   { id, at }
```

Lo stato del riduttore, oltre agli elementi: `currentModeId`,
`availableCommands`, `contextUsage`, e `turnDurations` — che è **stato di
interfaccia e non si persiste mai**, perché è la durata di un turno già chiuso,
utile a mostrarla e inutile a ricostruirla.

## I comportamenti che non sono ovvi

Sono la ragione per cui questa fase esiste come fase, e ciascuno diventerà un
test.

**1. I pezzi si accumulano nell'elemento aperto, e aprirne uno chiude gli altri.**
Il riduttore tiene l'indice del messaggio dell'agente aperto, del pensiero aperto,
del messaggio utente aperto. Un pezzo di testo dell'agente si concatena a quello
aperto se c'è; altrimenti ne apre uno nuovo. Un pensiero **chiude** il messaggio
dell'agente. Senza questa regola il trascritto diventa un elemento per token.

**2. `userMessageChunk` non arriva mai in un turno dal vivo.** Arriva solo quando
`session/load` ripete la conversazione. Il commento Swift lo dice esplicitamente,
ed è il tipo di informazione che si perde riscrivendo: chi non lo sa scrive un
ramo "impossibile" e poi lo cancella per pulizia, e il ripristino della
conversazione smette di funzionare senza che nessun test lo noti.

**3. L'upsert di una chiamata a strumento conserva ciò che l'aggiornamento non
porta.** Un `toolCall` per un id già visto **non** deve azzerare permesso, output
del terminale, stato d'uscita e identità di presentazione: quei campi arrivano da
altri canali. Sovrascrivere l'elemento intero è la scorciatoia ovvia, e cancella
il permesso in attesa mentre l'utente lo sta guardando.

**4. Un aggiornamento per una chiamata mai vista crea una scheda minima.** Succede
alla riconnessione a metà flusso. La regola scritta in Swift è «materialize a
minimal card rather than dropping information»: meglio una scheda povera che
un'informazione persa.

**5. Gli id evitano le collisioni con quelli già persistiti.** Il riduttore parte
con l'insieme degli id esistenti e non li riusa. Senza, ricaricare una
conversazione e continuarla produce due elementi con lo stesso id, e
l'interfaccia ne mostra uno solo.

**6. La fine del turno annulla i permessi rimasti in attesa.** Un permesso che
nessuno ha risposto prima della fine del turno non è più rispondibile: lasciarlo
in attesa mostra dei pulsanti che non fanno nulla. Il commento di `ChatQuestion`
lo dice meglio di come lo direi io: «Offering the buttons again would be a lie.»

**7. La fine del turno produce il riepilogo delle modifiche.** I percorsi dei
`toolCall` di genere `edit` completati nel turno diventano un `editSummary`, e la
lista si svuota all'inizio e alla fine del turno.

**8. L'uso del contesto si ripristina prima degli aggiornamenti dal vivo.** Al
rimontaggio di un worktree l'anello dell'uso deve mostrare subito l'ultimo valore
noto, invece di restare vuoto fino al prossimo `usage_update`.

## I derivati

Due funzioni pure che l'interfaccia consuma senza mai riscandire il trascritto.

**L'albero delle chiamate.** Divide gli elementi in radici più i figli di ogni
task, **in un passaggio solo**, e raccoglie al volo il permesso di approvazione
del piano. Il commento Swift dice che va chiamata una volta per cambiamento e
«never per rendered card» — ed è il genere di dettaglio che decide se una chat
lunga scorre o si impunta.

Regola da non perdere: un genitore sconosciuto **non** nasconde il figlio, che
resta al suo posto nel flusso. L'informazione non sparisce perché manca un
elemento.

**La domanda.** Normalizza in una forma sola due sorgenti diverse: un input di
strumento a forma di `AskUserQuestion`, e una richiesta di permesso semplice. Le
viste leggono questa e non toccano mai i payload grezzi.

Il campo `isStructured` porta una cicatrice che va portata con lui: **non si
deduce dal fatto che il prompt sia non vuoto**, perché `ui/select` di Pi ripete lì
la propria intestazione e il prompt viene scartato. Dedurlo è un errore che
funziona su tre agenti su quattro.

## Persistenza

La 5b scrive `chatItem`. La 5a ha già la sua tabella `chatSession` e non si tocca.

Si persistono gli elementi; **non** si persistono `turnDurations` né lo stato
transitorio di apertura dei flussi. Ricaricando, il riduttore riparte con gli id
già usati (punto 5) e i flussi chiusi.

Un elemento che non si riesce a interpretare si **scarta singolarmente**, e non
manda in quarantena l'intera conversazione. È la lezione opposta a quella del
codificatore del layout della 6b-1, dove un contenuto sconosciuto rifiuta tutto:
lì il layout è una struttura e perderne un pezzo la rompe, qui il trascritto è una
lista e un elemento illeggibile è un buco, non una rottura.

## Come si verifica

Test unitari, tutti. Non c'è un criterio end-to-end in questa fase perché non c'è
niente da gesticolare: nessuna interfaccia, nessun processo. I criteri arrivano
con la 5c, che è dove il trascritto diventa visibile.

Gli otto comportamenti sopra sono otto famiglie di test. Le mutazioni previste:

| Mutazione | Rosso atteso |
| --- | --- |
| i pezzi non si accumulano, ogni pezzo è un elemento | i test del punto 1 |
| l'upsert sovrascrive l'elemento intero | i test del punto 3 |
| gli id non consultano l'insieme esistente | i test del punto 5 |
| la fine turno lascia i permessi in attesa | i test del punto 6 |
| l'albero non gestisce il genitore sconosciuto | i test del derivato |
| `isStructured` si deduce dal prompt non vuoto | i test della domanda |

## Fuori scope

- Qualunque componente Svelte, il composer, le menzioni `@file` — **5c**.
- I driver — **5a-2**.
- Registry e installazione degli agenti — **Fase 7**.
