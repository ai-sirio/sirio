# Fase 5c — L'interfaccia della chat: design

Data: 2026-08-03
Stato: proposto

## Obiettivo

Rendere visibile il trascritto: la vista della conversazione, le schede, il
composer, e il cablaggio che collega i driver della 5a al riduttore della 5b.

## Il rischio principale di questa fase, dichiarato subito

La Fase 4a è stata **un motore corretto che nessuno chiamava**: quindici task
costruivano viste che consumavano un layout, e nessuno lo procurava dal main. Se
ne sono accorti solo i criteri end-to-end.

La 5c ha la stessa forma: un motore (5b) e dei driver (5a) che qualcuno deve
mettere in comunicazione. Perciò il cablaggio è **un task esplicito con criteri
propri**, non un dettaglio dentro il task della vista, e ogni voce in "dipende
da" deve poter essere puntata a un task che la consuma.

## Il secondo rischio: il blocco dell'interfaccia

In Swift la chat si è bloccata due volte, e la causa vera è stata trovata solo la
seconda: **una coda di transazioni prodotta da eventi per-token** dei driver
nativi. Il rimedio è stato la coalescenza, e nella 5a esiste già — con un criterio
end-to-end che verifica che il numero di consegne sia molto minore del numero di
pezzi prodotti.

Questa fase eredita quel criterio e ne aggiunge il complemento: **tutti i valori
derivati dal trascritto si costruiscono una volta per cambiamento**, mai una volta
per scheda renderizzata. In Swift lo fa `ChatPresentationSnapshot`, che li calcola
tutti in un colpo e riusa la segmentazione dei messaggi quando id e valore non
sono cambiati.

## Architettura

### Lo scatto di presentazione

Un derivato solo, ricostruito quando cambiano gli elementi:

```
items, grouped (l'albero della 5b), composerPermissions,
hasPendingPermission, activeSubagentTasks, currentActivity,
agentMessageSegments
```

Con una **cache per la segmentazione**, chiavata su id **e** valore
dell'elemento: durante uno streaming il testo dell'ultimo messaggio cambia a ogni
consegna, ma tutti gli altri no, e risegmentare l'intero trascritto a ogni
consegna è il modo più semplice di riprodurre il blocco che si è appena finito di
curare.

### La segmentazione dei messaggi

I riquadri `★ Insight ───` diventano **schede proprie**, non prosa in linea.
Si porta la tolleranza del riconoscitore Swift così com'è: alcuni CLI avvolgono il
riquadro in un blocco di codice, altri in un apice singolo, altri in niente. **Solo
le righe di intestazione e chiusura contano**; l'involucro si tollera e non si
pretende.

La ragione per cui il taglio avviene prima del renderer, e non dentro: gli apici
annidati nel corpo del riquadro rompono il parsing del Markdown, e separarlo prima
lascia che ogni metà usi il rendering che le serve davvero.

### Il rendering del Markdown

**Si riusa `renderMarkdown` della 6b-3**, con i suoi due strati — `html: false` e
la ripulitura del risultato.

Qui il motivo è ancora più diretto che nei file: il testo non è "scritto da un
agente" in senso lato, è **la risposta dell'agente**, e passa per una catena in cui
l'agente può aver letto contenuti di terze parti. Un secondo renderer, senza i due
strati, sarebbe la stessa mappa duplicata che nella 6b-1 ha spento cinque
linguaggi — con la differenza che qui la copia stantia non è un colore mancante,
è l'esecuzione di script nel renderer.

### Le schede

Una per genere di elemento: chiamata a strumento, domanda, piano, task,
riepilogo delle modifiche, output di terminale, anteprima del diff, riquadro di
approfondimento. Le schede **leggono i valori normalizzati della 5b** e non
toccano mai i payload grezzi: `chat-question.ts` esiste apposta.

### Il composer

Testo, allegati come chip, barra di controllo con agente, modello, modalità e
livello di sforzo. Le menzioni `@file` usano l'indice dei file — che è di questa
fase, perché è del composer.

Chip e allegati vengono dalla loro storia: in Swift il composer ha un documento
proprio (`ComposerDocument`) invece di una semplice stringa, ed è quello che rende
possibile un allegato in mezzo al testo.

## Decisioni

**Nessuna virtualizzazione della lista, per ora.** Il blocco che questo progetto
ha già vissuto non veniva dalla lunghezza della lista ma dalla **frequenza degli
aggiornamenti**, ed è già affrontato dalla coalescenza della 5a e dallo scatto
unico qui. Aggiungere una finestra scorrevole adesso significherebbe curare la
malattia sbagliata, e le finestre scorrevoli rompono la selezione continua del
testo — che in Swift è costata un lavoro apposta per essere mantenuta.

Il criterio che farebbe cambiare idea, scritto qui perché sia una decisione e non
una dimenticanza: se una conversazione da mille elementi non scorre fluida,
la finestra scorrevole torna in discussione, con la selezione come vincolo.

**La conversazione si segue automaticamente finché l'utente non scorre indietro.**
Riportare l'utente in fondo mentre sta leggendo qualcosa più su è il modo più
rapido di rendere inutilizzabile una chat lunga.

## Come si verifica

Criteri end-to-end, tutti dal **gesto**: si scrive nel composer, si preme invio,
si clicca un pulsante di permesso. Mai da una chiamata al protocollo.

1. Scrivere un prompt e inviarlo mostra il messaggio dell'utente e poi la risposta
   dell'agente.
2. La risposta appare **mentre arriva**, non tutta insieme alla fine.
3. Una richiesta di permesso mostra la scheda con le sue opzioni; cliccarne una la
   risolve e la conversazione prosegue.
4. Un riquadro `★ Insight` diventa una scheda propria, separata dalla prosa.
5. Markdown pericoloso nella risposta dell'agente viene mostrato **senza** essere
   eseguito.
6. Scorrere indietro durante lo streaming **non** riporta la vista in fondo.
7. Riaprire un worktree mostra la conversazione precedente.
8. Il composer manda un allegato insieme al testo.

Il criterio 5 è quello meno appariscente e più importante, come nella 6b-3.
Il criterio 7 è quello che misura il cablaggio: è il criterio che nella 4b
mancava.

## Mutazioni previste

| Mutazione | Rosso atteso |
| --- | --- |
| il cablaggio driver → riduttore viene tolto | 1, 2, 3, 7 |
| lo scatto si ricostruisce per scheda invece che per cambiamento | **nessuno** — è il limite noto |
| la segmentazione dei riquadri viene disattivata | 4 |
| il rendering perde la ripulitura | 5 |
| la vista torna sempre in fondo | 6 |

La seconda riga è la parte onesta: ricostruire troppo spesso **non cambia il
risultato**, cambia il costo, e un criterio che dipende da una soglia di tempo su
una macchina condivisa è un generatore di falsi rossi. Si copre con un test che
conta le ricostruzioni, non con un criterio end-to-end.

## Fuori scope

- Registry e installazione degli agenti — **Fase 7**.
- Le impostazioni degli account e delle quote — **Fase 7**.
- Il pannello degli agenti nella barra laterale destra: dipende dai subagent, e
  arriva dopo che la chat funziona.
