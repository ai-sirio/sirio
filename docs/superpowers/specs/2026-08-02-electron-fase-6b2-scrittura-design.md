# Fase 6b-2 — Scrivere: design

Data: 2026-08-02
Stato: approvato dall'utente, pronto per la pianificazione implementativa

## Obiettivo

Modificare e salvare qualsiasi file di testo del worktree, con lo stesso
contratto di documento della versione Swift: stato sporco, salvataggio
esplicito, conflitto con il disco, file eliminato, e nessuna modifica persa in
silenzio.

## Da dove si parte

La Fase 6b-1 lascia in piedi tutto il lato lettura: l'albero dei file nel
pannello destro, `files.list` e `files.read` sul socket di controllo, il
confinamento alla radice (`resolveInsideRoot`), il watcher chokidar che emette
`files.changed` con le **cartelle** toccate, la tokenizzazione proiettata sulle
righe, e il tab file con CodeMirror in sola lettura.

Questa fase aggiunge il verbo mancante: scrivere.

## Decisioni dell'utente

1. **Solo l'editor generico.** Gli `.md` si aprono e si modificano come testo.
   L'anteprima Markdown e la toolbar di formattazione sono una capacità
   separata, con una dipendenza propria e criteri di successo propri: vanno in
   una Fase 6b-3 breve, non qui.
2. **Il conflitto si rileva riusando l'evento di cartella.** Nessun secondo
   meccanismo di sorveglianza.

## La divergenza dalla versione Swift, e perché

Swift rileva la modifica esterna con un `DispatchSource` **sul descrittore del
file aperto**: sa esattamente quando *quel* file cambia.

Il watcher costruito in 6b-1 è chokidar e notifica **cartelle**: `files.changed`
porta un elenco di directory, non di file. È una granularità diversa, quindi il
meccanismo di conflitto non si porta uno a uno.

La scelta è riusare quel canale: quando `files.changed` include la cartella di un
file aperto, si rilegge quel file e si confronta. Costa qualche rilettura inutile
quando nella stessa cartella si scrive altro — una lettura di file, non un
meccanismo nuovo da avviare, fermare e testare.

## Architettura

### 1. Dove vive il buffer

`PaneGrid.svelte` monta **solo il tab attivo** (`PaneGrid.svelte:102`: il
gruppo rende `attivo`, non tutti i suoi tab). Un buffer tenuto dentro il
componente dell'editor viene quindi distrutto al cambio di tab, e le modifiche
non salvate spariscono **senza alcun errore**: è la forma peggiore di perdita di
dati, perché non lascia traccia.

Il buffer sta perciò fuori dai componenti, in un archivio nel renderer chiavato
per percorso:

```
src/renderer/src/lib/workspace/documents.svelte.ts
  path -> { text, savedText, mtimeMs, conflict, deleted }
```

`isDirty` è **derivato** (`text !== savedText`), non un flag da tenere in
sincronia: è il contratto di `CodeDocument.swift`, dove `isDirty` è una proprietà
calcolata. Un flag separato è una seconda fonte di verità che diverge al primo
percorso di errore.

Le due alternative scartate:

| Alternativa | Perché no |
| --- | --- |
| stato dentro `FileTab.svelte` | il montaggio selettivo di `PaneGrid` lo distrugge al cambio tab |
| buffer nel processo main | il testo attraverserebbe il socket a ogni tasto |

### 2. Scrivere: `files.write`

Un **membro nuovo** dell'unione discriminata delle richieste, come `files.list` e
`files.read` in 6b-1: aggiungere un membro è additivo e non rifiuta i chiamanti
esistenti. Non si aggiungono campi obbligatori a schemi esistenti.

```
files.write { worktreeId, path, text } -> { mtimeMs }
```

Riusa `resolveInsideRoot`: il confine di fiducia esiste già ed è già coperto da
test, non si riscrive.

La scrittura è **atomica**: file temporaneo nella stessa cartella, poi `rename`.
Un salvataggio interrotto a metà non deve lasciare un file troncato al posto
dell'originale — e la cartella dev'essere la stessa, perché `rename` è atomico
solo all'interno dello stesso filesystem.

Restituisce l'`mtime` risultante, che serve alla guardia anti-eco.

### 3. Conflitto con il disco, e la guardia anti-eco

Su `files.changed`, per ogni file aperto la cui cartella compare nell'elenco, si
rilegge e si confronta:

| Stato del buffer | Comportamento |
| --- | --- |
| pulito | ricarica automatica, silenziosa |
| sporco | banner *File changed on disk — Reload / Keep* |
| file sparito | banner *File deleted on disk*; `⌘S` lo ricrea |

**La guardia anti-eco è il punto delicato.** Dopo il nostro stesso salvataggio il
watcher scatta su di noi: senza guardia ogni salvataggio si auto-segnala come
modifica esterna, e l'utente vede un conflitto con sé stesso.

Si confronta l'`mtime` restituito da `files.write` con quello riletto: uguale
significa che l'evento è il nostro, e si ignora. È un **confronto di stato**, non
una finestra temporale: un timer sbaglia sotto carico, ed è il tipo di difetto
che si manifesta solo sulla macchina lenta di qualcun altro.

Swift risolve lo stesso problema con lo stesso principio (guard anti-eco dopo
`save()`, documentata nella spec dell'editor markdown).

**Limite noto del confronto:** l'`mtime` ha granularità finita. Due scritture
molto ravvicinate possono condividere lo stesso valore, e in quel caso una
modifica esterna verrebbe scambiata per la nostra. Il rimedio, se il caso si
presenta, è confrontare anche la dimensione o un digest del contenuto — non si
adotta subito perché il caso richiede una scrittura esterna nella stessa
frazione di millisecondo del nostro salvataggio. Va scritto qui perché è una
scelta consapevole, non una svista.

### 4. Segnalare lo sporco, e non perderlo

- Pallino di modifica nel tab, come in `TabBarView` di Swift.
- `⌘S` salva il documento del tab attivo.
- `⌘W` su un tab sporco chiede *Save / Don't save / Cancel*.
- **Chiusura della finestra**: in Electron chiudere la finestra non chiude
  l'app, ma distrugge il renderer. Senza guardia i buffer sporchi evaporano.
  È la stessa asimmetria già documentata per i PTY in `CLAUDE.md` — chiudere la
  finestra non termina gli agenti, ma qui il renderer è proprio dove vive il
  buffer, quindi la conseguenza è opposta e va gestita.

### 5. Cosa non si persiste

I tab file si ripristinano al riavvio **rileggendo dal disco**. I buffer sporchi
non si persistono: in Swift `isDirty` è stato di runtime e al riavvio il file si
rilegge. Un tab il cui file non esiste più si scarta.

**Questa sezione dipende dalla 6b-1, non da questa fase.** Il ripristino passa da
`encodeLayout`/`decodeLayout` in `layout-codec.ts`, che serializzano il contenuto
del tab come **stringa** (`'terminal'`, `'diff:worktree'`) e la riconoscono con
confronti letterali, chiudendo con `return { ok: false, reason: 'contenuto
sconosciuto' }`. Il compilatore forza solo il lato encode — dove l'accesso a
`.source` non esiste sul membro `file` — mentre il decode è invisibile a `tsc`.
Se i due lati restano disallineati, `decodeLayout` rifiuta l'**intero** layout e
lo manda in quarantena: si apre un file, si riavvia, e si perdono anche i
terminali. La verifica è un test di andata e ritorno in 6b-1, non un criterio di
questa fase.

## Errori

Il boundary di scrittura è nuovo, quindi va trattato come tale.

| Caso | Comportamento |
| --- | --- |
| percorso fuori dalla radice | rifiutato con il motivo, come in `files.list` |
| permessi insufficienti | il salvataggio fallisce, **il buffer resta intatto**, l'errore è visibile e l'utente può riprovare |
| cartella sparita sotto il file | stesso trattamento dei permessi |
| disco pieno | idem: mai svuotare il buffer su un salvataggio fallito |

La regola comune: **un salvataggio fallito non deve mai far perdere il testo in
memoria.** Il buffer è l'unica copia della modifica.

## Fuori scope, dichiarato

- Creazione, rinomina, eliminazione di file. La versione Swift non li ha: il
  menu contestuale di `FileExplorerView.swift` espone solo *Show in Finder*.
- Anteprima e toolbar Markdown → Fase 6b-3.
- Modifica dentro la vista diff.
- Autosave. Il salvataggio è esplicito, come in Swift.

## Criteri end-to-end

Tutti partono dal **gesto**, mai da una chiamata al protocollo.

1. Scrivere nell'editor fa comparire il pallino di modifica sul tab.
2. `⌘S` cambia il file **sul disco** (verificato leggendo il file, non l'interfaccia).
3. Un file modificato da fuori con buffer **pulito** si aggiorna da solo.
4. Un file modificato da fuori con buffer **sporco** mostra il banner di conflitto.
5. Dopo un salvataggio nostro il banner **non** compare (la guardia anti-eco).
6. *Reload* scarta le modifiche locali; *Keep* le mantiene e toglie il banner.
7. Cambiare tab e tornare indietro **conserva** il buffer sporco.
8. Un salvataggio fallito lascia il buffer intatto e mostra l'errore.

Il criterio 7 è quello che misura la decisione architetturale del §1: se il
buffer finisse dentro il componente, sarebbe l'unico a diventare rosso.

## Mutazioni di verifica previste

| Mutazione | Criterio atteso rosso |
| --- | --- |
| la guardia anti-eco viene rimossa | 5 |
| il buffer si sposta dentro `FileTab.svelte` | 7 |
| `files.write` scrive direttamente invece che via `rename` | nessuno — **è il limite noto**: l'atomicità non si prova a mano, si argomenta |
| un salvataggio fallito azzera il buffer | 8 |

La riga senza criterio è deliberata e va scritta, non nascosta: un test che
simuli l'interruzione a metà scrittura richiederebbe di uccidere il processo nel
mezzo di una `write`, il che non è riproducibile in modo deterministico. La
scelta di `rename` si giustifica per costruzione, non per test.

## Dipendenze

Nessuna nuova. CodeMirror è già montato dalla 6b-1; la scrittura usa
`node:fs/promises`, già in uso in `src/main/files/`.
