# Il sistema di design — spec

**Obiettivo:** dare a Tiller Electron un sistema visivo — colori corretti, scale
esplicite, un modello di rilievo dichiarato — al posto della palette parziale e
scaduta che ha oggi.

**Non è** un porting: la versione Swift non ha scale da portare. È lavoro nuovo,
informato da dove lo Swift è di fatto convergiuto e da quattro applicazioni prese
a riferimento.

## Il problema

`src/renderer/src/assets/base.css`, 109 righe: **14 colori, chiaro e scuro. E
basta.**

Nessuna scala di spaziature, nessuna di raggi, nessuna tipografica, nessuna
ombra. Più una ventina di token `--ev-*` mai usati, avanzo del template
electron-vite.

Con solo i colori definiti, ogni spaziatura e ogni raggio è stato deciso dentro il
componente dove serviva — cioè da nessuno, molte volte.

E i colori che ci sono sono **superati**, non diversi:

| | Electron oggi | Swift oggi |
|---|---|---|
| chrome | `#1F1F26` @ 0.88 | `#1A1A1E` @ 0.96 |
| workspace | `#17171C` | `#121216` |

`#1F1F26` è il valore del commit Swift `a0f5fcf`, sostituito da `8368529 recolor
chrome to warm graphite`. La migrazione ha copiato una fotografia scaduta.

Nessun test poteva accorgersene: `#1F1F26` è un colore scuro valido, l'app si
vede, i criteri restano verdi. È lo stesso difetto che ha attraversato tutta la
migrazione — una cosa plausibile al posto giusto — nella sua forma visiva.

## Decisioni

### Tema scuro primario

Il chiaro resta, ma smette di essere un obiettivo di parità: si mantiene
coerente, non si progetta per primo.

### Palette: quella della versione Swift, valori attuali

Estratta da `App/AppTheme.swift` e
`Packages/TillerCore/Sources/TillerCore/AppSurfaceColor.swift`.

**Superfici**

| Token | Valore | Ruolo |
|---|---|---|
| `--t-surface-chrome` | `#1A1A1E` | sidebar, barre, titolo |
| `--t-surface-content` | `#121216` | terminale e chat |
| `--t-surface-card` | `#282A35` | card del trascritto |
| `--t-surface-field` | `#14151B` | campi di input |
| `--t-surface-pill` | `#33343D` | pillole |

**Stati riga**

| Token | Valore |
|---|---|
| `--t-row-hover` | `#20232D` |
| `--t-row-selected` | `#2B2F3A` |
| `--t-row-ring` | `#3A4050` |
| `--t-hairline` | `#40424F` |

**Testo**

| Token | Valore | Ruolo |
|---|---|---|
| `--t-text-selected` | `#FFFFFF` | titolo della riga scelta |
| `--t-text-title` | `#D9DBE3` | titolo |
| `--t-text-subtitle` | `#B8BDD1` | sottotitolo |
| `--t-text-meta` | `#A8ADC4` | didascalie |

**Git, diff, binari**

| Token | Valore | Ruolo |
|---|---|---|
| `--t-git-staged` | `#8CD1A1` | staged, aggiunta |
| `--t-git-modified` | `#E8B05C` | modificato |
| `--t-git-untracked` | `#6EADE8` | non tracciato, collegamenti a file |
| `--t-git-conflict` | `#E69499` | conflitto, rimozione |
| `--t-diff-add-bg` | `#143D24` | fondo riga aggiunta |
| `--t-diff-del-bg` | `#45141A` | fondo riga rimossa |
| `--t-diff-hunk-bg` | `#1A2B47` | fondo intestazione hunk |
| `--t-rail-task` | `#7D6BD6` | binario card «task» |
| `--t-rail-tool` | `#666B80` | binario card «strumento» |

**Opacità delle superfici traslucide: `0.96`**, non `0.88`. È un valore unico
condiviso da tutte le superfici traslucide, come in Swift
(`translucentSurfaceOpacity`).

### Modello di rilievo: l'inversione della versione Swift

Il chrome (`#1A1A1E`) è **più chiaro** del contenuto (`#121216`). Il contenuto è
la superficie recessiva; il chrome sta sopra.

È l'opposto di Superconductor, che solleva il contenuto in una card chiara con
ombra. La scelta è deliberata, per tre ragioni:

1. **Il terminale non è una superficie che si decide.** xterm dipinge il proprio
   fondo, e l'area di rendering è opaca. Un contenuto «sollevato» obbligherebbe a
   un tema terminale chiaro, o a una finestra la cui superficie più chiara
   contiene testo chiaro su fondo scuro.
2. **L'elevazione funziona solo se è scarsa.** Superconductor solleva una card,
   al centro. Tiller apre due, tre, quattro riquadri in split, tutti ugualmente
   importanti: sollevarli tutti spende il segnale senza comunicare niente.
3. **La scala è già tarata su questo verso.** `#282A35` su `#121216` sono
   quattordici punti di luminanza; su `#1A1A1E` diventano sei. Invertire non
   significa scambiare due colori, significa ritarare tutta la scala a mano.

**Limite dichiarato:** Superconductor è a sua volta un multiplexer di terminali,
e non sappiamo come renda i propri terminali — lo screenshot di riferimento mostra
la vista chat. Se li solleva, la ragione 1 perde forza. Non cambia la decisione,
ma va scritto invece che taciuto.

### Le scale: nuove, non portate

Misurazione del codice Swift:

- **padding**: 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14 — ogni intero fino a 10.
- **raggi**: 6 usato 21 volte, 7 usato 20 volte. Due valori a un pixel di
  distanza, 41 usi complessivi.
- **testo**: 7, 9, 10, 11, 12, 13, 14, 15, 16, 17.

Raggio 6 e raggio 7 usati quasi lo stesso numero di volte sono la firma di una
decisione mai presa. Non c'è una scala da portare; si definisce ora.

**Spaziature** — `2, 4, 6, 8, 12, 16, 24, 32`

Tiene i tre valori su cui lo Swift converge davvero (6, 8, 4) e toglie il rumore.
Il 10, usato 15 volte, si risolve in 8 o 12 secondo il ruolo: 8 dentro un
elemento, 12 fra elementi. Tenere sia 8 sia 10 riprodurrebbe la deriva 6/7 in una
scala nuova.

**Raggi** — `4, 6, 8, 12`

Il 7 confluisce nel 6: quarantuno usi diventano un valore solo. 5 → 4, 10 → 8,
14 → 12.

**Testo** — `10, 11, 12, 13, 15, 17`

Sono i sei valori che lo Swift usa davvero; 7, 9, 14 e 16 spariscono. Base 11,
come nello Swift (27 usi, il più frequente).

**Nessuna scala di ombre in questa fase.** Il modello è recessivo: la separazione
è per superficie e filetto. Un token d'ombra introdotto «perché serve un sistema
completo» sarebbe la definizione di speculativo.

### La sidebar: tre livelli, e un pannello in meno

**Oggi:** `SidebarTree` mostra progetto → worktree. Le sessioni degli agenti
vivono in `AgentsPanel`, a destra.

**Dopo:** progetto → worktree → sessioni agente, con icona dell'agente, pallino
di stato e badge di inattività sulla riga della sessione.

**`AgentsPanel` viene rimosso.** Non affiancato: rimosso. Due gerarchie sugli
stessi oggetti raddoppiano il lavoro senza raddoppiare l'informazione, e prima o
poi in una delle due manca qualcosa. Il pannello destro resta Files/Changes.

**Il rischio da sorvegliare:** tre livelli in una colonna da ~230px. Il rientro
mangia larghezza dove i nomi sono già lunghi — nello screenshot di Dirijor un
nome è troncato già al secondo livello. Le vie d'uscita (guide sottili invece di
rientri larghi; il worktree come intestazione invece che come riga) si valutano
guardando la cosa costruita, non adesso.

### Cosa si prende da ciascun riferimento

- **Superconductor** — la grammatica: scale, raggi, tipografia, respiro; la
  selezione come card staccata invece che come rettangolo colorato; la tab bar con
  le icone degli agenti e l'indicatore a sottolineatura.
- **Dirijor** — i segnali di stato sulla riga: pallino, badge di inattività,
  identità dell'agente.
- **Waku** — il separatore di ragionamento collassato («Worked for 18 seconds ›»)
  e la distinzione fra messaggio utente e risposta. **Da valutare nella 5c**, non
  qui: sono decisioni che il contenuto determina.
- **Briq** — la domanda «cosa aspetta me?». **Non** la sua risposta: niente
  secondo albero per stato. Se serve, è un ordinamento dell'albero che c'è già.

## Sequenza

I token vengono **prima** delle due fasi che restano (5c chat, 7 piattaforma), e
la rifinitura visiva viene **dopo**.

La ragione è una distinzione fra due generi di decisione:

- **Indipendenti dal contenuto** — palette, scale, modello di rilievo. Valgono
  uguali su qualunque schermata, e si possono fissare adesso.
- **Determinate dal contenuto** — densità del trascritto, gerarchia della card,
  dove va il respiro. Giudicarle prima che la schermata esista è indovinare.

Costruire la 5c sopra i token costa quanto costruirla senza, e risparmia di
rifarla. Ridisegnare la chat prima che esista, no.

## Come si verifica un sistema di design

Un token non ha comportamento: non si prova con un criterio end-to-end. Ma due
proprietà sono verificabili davvero, e non sono formalità.

**1. Fedeltà della palette.** Un test unitario confronta i token CSS con i valori
estratti dalla versione Swift, elencati sopra. Fallisce se qualcuno cambia una
tinta senza cambiare il riferimento. È il test che avrebbe preso `#1F1F26`.

**2. Nessuna scorciatoia.** Un test analizza i file `.svelte` e `.css` e fallisce
se trova un colore esadecimale letterale o un `px` fuori scala nelle proprietà
governate dal sistema (spaziature, raggi, dimensioni del testo).

È questo secondo test che rende il sistema un sistema. Senza, i token sono
un suggerimento: il primo componente che ha fretta scrive `padding: 7px` e la
deriva riparte da capo — esattamente come è successo nella versione Swift, dove
nessuno aveva torto perché non c'era niente da sbagliare.

**Le eccezioni si dichiarano.** Il terminale ha i propri colori (tema xterm) e
non passa dai token: è un'esclusione esplicita nel test, non un buco.

E i valori che la scala non contiene ma che non si possono arrotondare — un
rientro che deve allineare, il ritaglio `-1px` di `.sr-only` — vivono in
`tokens.css` col prefisso **`--t-except-*`**. Il prefisso è il punto: rende le
eccezioni contabili. Se quella lista cresce, è la scala a essere sbagliata.

**`calc()` non è una scappatoia.** Il primo controllo cercava «nessun `px`
scritto», che è un *surrogato* di «sta sulla scala»: `calc(var(--t-space-8) *
2.5)` fa 80px e lo superava, `calc(var(--t-space-8) + var(--t-space-2) +
var(--t-space-1))` fa 38px e pure. La guardia rifiuta perciò anche la
moltiplicazione o divisione per un numero nudo (tranne `* -1`, che nega un valore
di scala senza inventarne uno) e le espressioni con più di un token. Un token
solo accanto a termini non-token — `calc(100% - var(--t-space-4))` — resta
lecito: lì il token è una correzione, non un addendo di un numero da ricostruire.

**Quello che i test non coprono, e va detto:** che il risultato sia *bello*, e
che le tre gerarchie di sidebar stiano in 230px. Si vede guardando. La verifica
visiva manuale resta, ed è la prima cosa da fare quando il lotto rientra —
nessuno ha ancora guardato questa applicazione con i propri occhi.

## Fuori scopo

- Il tema chiaro come obiettivo di parità: si mantiene coerente, non si progetta.
- Ombre ed elevazione: il modello è recessivo.
- La resa visiva del trascritto chat: appartiene alla 5c.
- Qualunque funzionalità nuova rispetto alla versione Swift.
- I temi dell'utente e la personalizzazione dei colori.

## Domande aperte

- **L'ordinamento «chi aspetta prima»** nella sidebar: proposto, non deciso.
- **Il valore di `--t-rail-question` e `--t-rail-edit`**: in Swift sono alias di
  `gitModified` e `gitStaged`. Restano alias o diventano tinte proprie?
- Se un secondo screenshot di Superconductor mostrasse i suoi terminali, il
  limite dichiarato sul modello di rilievo andrebbe rivisto.
