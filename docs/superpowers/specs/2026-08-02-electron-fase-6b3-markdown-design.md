# Fase 6b-3 — Markdown: design

Data: 2026-08-02
Stato: proposto — le due decisioni marcate «da confermare» sono mie, non
dell'utente, e sono segnalate come tali.

## Obiettivo

Portare l'editor Markdown della versione Swift: anteprima renderizzata, passaggio
anteprima/codice, e una barra di formattazione che agisce sulla selezione.

## Da dove si parte

La 6b-2 lascia in piedi tutto il resto di un editor: buffer fuori dai componenti,
stato sporco, `⌘S` con scrittura atomica, `⌘W` con guardia, banner di conflitto,
pallino di modifica. Un `.md` **si apre e si modifica già oggi**, come qualunque
altro testo.

Questa fase aggiunge soltanto ciò che è specifico del Markdown. È deliberatamente
piccola: è stata staccata dalla 6b-2 perché ha una dipendenza propria e criteri
propri, non perché sia complicata.

## Cosa portare, dalla versione Swift

Da `App/MarkdownEditor/` e dalla spec del 2026-07-10:

- **Passaggio anteprima/codice** nell'intestazione del tab, icone `<>` e matita.
- **Anteprima** renderizzata con il tema dell'app.
- **Barra di formattazione**, visibile **solo in modalità codice**: grassetto,
  corsivo, H1-H3, elenco, collegamento.
- **Le trasformazioni di testo sono funzioni pure**, testate a parte da qualunque
  interfaccia — in Swift stanno in `TillerCore`, non nella vista.
- **Limite morbido di dimensione**: oltre ~2 MB il file si apre in modalità
  codice senza anteprima, con un avviso nell'intestazione. Il renderer Markdown
  rallenta molto sui documenti enormi.

## Decisione 1 — quale renderer *(da confermare)*

`markdown-it` più `DOMPurify`.

`markdown-it` è maturo, conforme a CommonMark, e ha il comportamento che serve
qui: `html: false` **disattiva l'HTML grezzo dentro il Markdown**. È la scelta
predefinita giusta, e va detto perché.

**Questo è un confine di fiducia, non una scelta estetica.** Il testo renderizzato
viene da file del worktree — cioè, in questa applicazione, da file che un **agente
AI ha appena scritto**. Un `README.md` che contiene `<img src=x onerror=...>`
diventerebbe esecuzione di script dentro il renderer di Electron, che ha accesso
al bridge del preload. Non è un rischio teorico: è il caso d'uso normale del
prodotto.

Quindi due strati, non uno: `html: false` in `markdown-it` **e** `DOMPurify` sul
risultato prima di inserirlo nel DOM. Due strati perché il primo si disattiva con
una riga di configurazione che sembra innocua, e il secondo è quello che regge se
qualcuno la scrive.

Alternative scartate: `marked` (stessa famiglia, nessun vantaggio qui); un
renderer scritto a mano (contro «non ricreare la ruota», e sarebbe proprio il
pezzo dove un errore diventa una vulnerabilità).

## Decisione 2 — cosa si vede aprendo un `.md` *(da confermare)*

**Anteprima**, come in Swift.

È una divergenza dal comportamento attuale della 6b-1/6b-2, dove ogni file si apre
in CodeMirror. La ragione della parità: in Swift l'anteprima è il modo normale di
guardare un Markdown, e il codice è la modalità in cui si entra per scrivere.

Il tab **ricorda** la modalità scelta finché resta aperto. Non si persiste fra
riavvii: sarebbe un campo nuovo in `viewState`, cioè nel codificatore del layout,
per un beneficio piccolo. Il costo di quel campo lo conosciamo — la 6b-1 ha
mostrato che il lato `decode` è invisibile al compilatore e che un disallineamento
manda in quarantena **l'intero** layout.

Se preferisci l'apertura in codice, è una riga: dillo e cambia il valore
iniziale.

## Architettura

### Dove sta la modalità

Stato locale del componente del tab, non nell'archivio dei documenti: non è una
proprietà del **documento**, è una proprietà della **vista**. Due tab sullo stesso
file — possibile in due gruppi — possono legittimamente mostrarne uno per parte.

### Le trasformazioni di testo

`src/renderer/src/lib/workspace/markdown-format.ts`, funzioni pure:

```ts
type Selezione = { from: number; to: number }
type Risultato = { text: string; selection: Selezione }

avvolgi(text, selezione, marcatore)        // grassetto, corsivo
intestazione(text, selezione, livello)     // H1-H3
elenco(text, selezione)                    // elenco puntato
collegamento(text, selezione)              // [testo](url)
```

Ognuna restituisce **testo nuovo e selezione nuova**: una barra che formatta e poi
perde la selezione costringe a riselezionare a ogni pulsante.

I casi che i test devono coprire, e che sono la ragione per cui queste funzioni
sono pure: selezione **vuota** (inserire i marcatori e mettere il cursore in
mezzo), selezione **piena**, selezione **multiriga** (l'intestazione va per riga,
non attorno al blocco), e selezione **già formattata** — premere grassetto due
volte deve togliere il grassetto, non annidarlo.

### Il legame con CodeMirror

I pulsanti leggono la selezione dalla vista, chiamano la funzione pura, e
applicano il risultato con una transazione — la stessa strada che la 6b-2 usa già
per la ricarica esterna. Nessun accesso al DOM.

## Errori

- **Markdown malformato**: `markdown-it` non fallisce, rende ciò che capisce. Non
  serve un ramo d'errore.
- **File oltre il limite morbido**: nessun errore, si apre in codice con l'avviso.
- Il resto — permessi, file sparito, salvataggio fallito — è già della 6b-2 e non
  si ripete qui.

## Criteri end-to-end

Tutti partono dal gesto.

1. Aprire un `.md` mostra il testo **renderizzato** (un `#` diventa un titolo, non
   resta un cancelletto).
2. Il passaggio a codice mostra il sorgente, e torna indietro.
3. La barra di formattazione **non** è visibile in anteprima.
4. Selezionare una parola e premere grassetto la avvolge in `**`, e la selezione
   resta.
5. Un `.md` che contiene HTML pericoloso viene reso **senza** eseguirlo.
6. Un `.md` oltre il limite morbido si apre in codice, con l'avviso.

Il criterio 5 è il più importante e il meno appariscente: verifica che il testo
compaia e che lo script non parta.

## Mutazioni di verifica

| Mutazione | Criterio atteso rosso |
| --- | --- |
| `html: false` diventa `html: true` | **nessuno**, se DOMPurify regge — ed è il punto: il secondo strato esiste per questo |
| si toglie **anche** DOMPurify | 5 |
| `avvolgi` restituisce la selezione vecchia | 4 |
| il limite morbido viene ignorato | 6 |

La prima riga è deliberata. Un solo strato che regge non prova che l'altro sia
inutile: prova che gli strati sono due.

## Fuori scope

- Creazione di nuovi file Markdown (non c'è nemmeno in Swift).
- Modifica dentro la vista renderizzata (WYSIWYG).
- Apertura da `⌘O`, da drag & drop, da click nel terminale: in Electron i file si
  aprono dall'albero della 6b-1, e i tre canali di Swift esistono perché lì
  l'albero è arrivato dopo.
- Persistenza della modalità fra riavvii.
