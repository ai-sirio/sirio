# Rifinitura visiva — design

Chiude la sequenza del sistema di design: token → 5c/7 → sidebar a tre livelli →
**questa**. Decisioni prese sul mockup completo del 2026-08-03 (due giri,
approvato il secondo). Tutti i valori sono i token di `assets/tokens.css`; i hex
citati sono solo per leggibilità.

## Decisioni approvate

### 1. Chat come tab del worktree (strutturale)

La colonna fissa al 38% introdotta dalla 5c **si smonta**. `ChatPane` diventa
contenuto di un tab (`✳ Chat`), fratello dei terminali sotto lo stesso
worktree — com'era in Tiller Swift e come il vincolo V1 disegna l'albero.
Lo spazio orizzontale torna tutto al contenuto attivo; la griglia di
`App.svelte` perde la colonna `minmax(0, 38%)`.

Da verificare in fase di piano: il motore di layout (`workspaceTab`,
`contentKind`) prevede già il genere chat — se sì è un collegamento, non una
feature nuova.

### 2. Tab bar: il tab attivo è un riquadro chiaro

Niente sottolineatura colorata. Il tab attivo è una **pillola**
`--t-surface-pill` (#33343D) con raggio `--t-radius-3`, testo
`--t-text-selected`; gli inattivi sono solo testo `--t-text-meta` senza
sfondo. Ogni tab porta la sua identità: monogramma agente (lo stesso
`AgentIcon` della sidebar) o icona di genere (terminale, chat). Il monogramma
dentro il tab attivo inverte lo sfondo (chrome su pillola) per staccare.

### 3. Chrome senza cuciture in alto

**Via l'hairline sotto la barra del titolo e via il bordo destro della
sidebar.** Barra del titolo, sidebar e tab bar sono un'unica superficie
`--t-surface-chrome`; il contenuto `--t-surface-content` ci affonda dentro e
l'inversione del rilievo fa da divisore da sola. L'hairline sopra la status
bar resta (è l'unico bordo orizzontale superstite del chrome).

### 4. Status bar a tutta larghezza

La status bar attraversa l'intera finestra, **sotto anche la sidebar**:
a sinistra progetto · branch · conteggio modifiche, a destra la quota provider
(`QuotaBar`). Oggi vive dentro la colonna centrale: esce al livello della
finestra.

### 5. Chat: prosa prima, card come eccezione (rivisto sul riferimento
JetBrains/Junie, approvato 2026-08-03)

**La prosa dell'agente scorre libera sul fondo contenuto — nessuna card
attorno al testo.** Corpo `--t-text-subtitle`, affermazioni chiave in
`--t-text-title` con peso 500. Questo sostituisce la card-per-messaggio della
prima stesura.

- **Codice inline ambra**: `--t-git-modified`, monospace, un punto in meno del
  corpo — il tratto più riconoscibile del riferimento.
- **Tool call = riga compatta**, non card: sfondo `--t-surface-field`, bordo
  `--t-row-ring`, raggio `--t-radius-3`; dentro: icona, nome tool monospace
  `--t-text-title`, check `--t-git-staged` a esito, genere a destra in
  `--t-text-meta`, chevron per espandere. Sopra, la riga label
  «Running <server> MCP Server tool:» in meta con il nome server come chip
  monospace su chrome.
- **Passi collassati**: «✓ Processed» / «Worked for N seconds ›» come
  separatori muti in `--t-text-meta`, collassati di default.
- **Card SOLO per contenuto strutturato**: Insight/Task (`--t-surface-card`,
  rail `--t-rail-task`, angoli sinistri a zero, titolo in maiuscoletto),
  permission, plan, diff. La prosa non entra mai in card.
- **Errori = banner**: sfondo `--t-diff-del-bg`, bordo e testo
  `--t-git-conflict`, larghezza piena.
- **Utente = pillola compatta a destra**: `--t-surface-pill`,
  `--t-text-subtitle`, raggio `--t-radius-3` — non una bolla larga.
- **Composer col control bar dentro**: campo `--t-surface-field` con bordo
  `--t-row-ring`; sotto il testo, separata da un hairline `--t-row-selected`,
  la riga controlli: «+» allegati e selettore agente·modello (monogramma
  `AgentIcon`) a sinistra; modalità permessi e **Send** a destra. Send è
  l'unico elemento a colore pieno della superficie: `--t-rail-task` con testo
  bianco.

### 6. Finestra: grande all'avvio, e con memoria

Oggi la finestra nasce fissa a 1100×720 (`src/main/index.ts:126`). Deciso
(utente, 2026-08-03): **al primo avvio la finestra parte massimizzata**
(`maximize()`, non fullscreen macOS — niente Spaces separati); dagli avvii
successivi si **ripristinano posizione e dimensioni dell'ultima sessione**
(bounds salvati alla chiusura, nello stesso posto delle altre preferenze).
Se il display salvato non esiste più, si torna alla massimizzata.

### 7. Sidebar (già in costruzione nel lotto tre-livelli)

La selezione come card staccata `--t-surface-card` coi margini interni, il
contatore ambra sull'intestazione collassata, monogrammi e `Zzz`: decisi nella
spec della sidebar, il mockup li conferma visivamente. Qui non si rifà niente:
se il lotto sidebar consegna righe diverse dal mockup, la rifinitura le adegua.

### 8. I loghi degli harness (deciso: in scopo, 2026-08-03)

`AgentIcon.svelte` (nato coi monogrammi nel lotto sidebar) prende i marchi
veri, portati 1:1 da `App/AgentIcon.swift` del repo Swift (sola lettura):

- **claude**: `App/Assets.xcassets/agent-claude.imageset/claude.svg`, tinta
  `#D97757` (arancio Claude, colore di marca);
- **codex**: `App/Assets.xcassets/agent-codex.imageset/openai.svg`, tinta
  `currentColor` (segue il testo);
- **opencode**: due rettangoli even-odd, viewBox 240×300 (cornice piena +
  interno scuro 60,120,120,120) — coordinate in `OpenCodeFrameShape`/`Inner`;
- **pi**: path even-odd viewBox 800×800 — coordinate esatte in `PiShape`;
- **omp**: path viewBox 64×64 con gradiente lineare `#ED4ABF → #9B4DFF →
  #5AD8E6` — coordinate in `OmpShape`;
- **sconosciuti**: monogramma su cerchio colorato (arancio/verde/blu/viola/
  teal per id noto, grigio altrimenti) — già la semantica Swift.

I colori di marca sono **eccezioni dichiarate e delimitate al componente**: non
sono token UI e non devono diventarlo; se lo scale-guard li intercetta, si
estende la sua lista di eccezioni al solo `AgentIcon.svelte`, con il motivo
scritto accanto. Gli id con suffisso `-acp` si normalizzano come in Swift.

## Fuori scopo

- Animazioni/transizioni oltre l'esistente (pulse del needs-input).
- Tema chiaro: **stessa grammatica, token già pronti** nel blocco
  `[data-theme='light']` — la rifinitura non tocca i valori, e la direzione
  del rilievo nel chiaro resta invertita come da spec del design system.

## Verifica

- **Cattura di schermata per ogni task**, confrontata col mockup: è il criterio
  primario di questa fase — i test automatici non vedono «stona».
- E2e per la parte strutturale: il tab Chat apre la chat nel contenuto
  (`[data-message-kind]` visibile dentro l'area tab, non in una colonna);
  la colonna 38% non esiste più; la status bar è figlia della finestra, non
  della colonna centrale.
- Scale-guard: nessun esadecimale nuovo, nessun px fuori scala.
- I criteri e2e esistenti della 5c (composer, transcript, card) devono restare
  verdi: la chat cambia posto, non comportamento.
