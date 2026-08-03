# Rifinitura visiva — piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) NON si esegue: lo esegue Claude a fine lotto.

**Obiettivo:** portare tutta la UI alla grammatica approvata il 2026-08-03 —
spec: `docs/superpowers/specs/2026-08-03-electron-rifinitura-visiva-design.md`.

**Prerequisito:** il lotto sidebar tre-livelli è mergiato (`AgentIcon.svelte`,
`TreeRow` con `agentId`, contatore needs-input esistono). Se manca qualcosa di
citato qui, fermati e dichiaralo: non ricostruirlo alla cieca.

**Architettura:** nessun motore nuovo. Layout di `App.svelte`, stile dei
componenti esistenti, un componente riscritto dentro (`AgentIcon`), la chat
spostata di posto (colonna → tab). I contenuti e i comportamenti della 5c non
cambiano: i suoi criteri e2e devono restare verdi.

## Vincoli globali

- Stringhe UI in inglese; commenti e nomi interni in italiano; Conventional
  Commits inglese minuscolo imperativo; niente `.tokensave/**`.
- **Colori e misure solo dai token** (`scale-guard.test.ts`). Unica eccezione
  nuova ammessa: i colori di marca dentro `AgentIcon.svelte` (spec §8) — se lo
  scale-guard li intercetta, estendi la sua lista di eccezioni al solo file,
  col motivo scritto accanto.
- «Rosso prima» sul singolo criterio (`-g`); **il primo run e2e completo del
  file nuovo è la baseline delle mutazioni — non fare un run completo di
  verifica finale separato**; typecheck/lint a fine lotto, una volta se verdi,
  altrimenti correggi e riesegui finché verdi.
- La **cattura di schermata è parte di ogni task visivo** (file `.spec.ts`
  temporaneo con `window.screenshot()`, poi cancellato): confronto col mockup,
  percorsi delle immagini nel report.
- Il repo Swift `~/Desktop/Progetti/tiller` è **sola lettura** (si copiano
  fuori solo i due SVG del Task 6).

## Punti di realtà (verificati 2026-08-03, prima del lotto sidebar)

- Finestra: `src/main/index.ts:126` — `width: 1100, height: 720`, fissi.
- Griglia: `App.svelte` ~riga 656 — `grid-template-columns: minmax(0, 1fr)
  minmax(0, 38%) auto auto`; `ChatPane` montato ~riga 603.
- Tab strip: `TopBar.svelte` + `TabSegment.svelte` (+ `topbar-segments.ts`,
  `tab-segment-layout.ts`).
- Status bar: `StatusBar.svelte` (+ `QuotaBar.svelte`), oggi dentro la colonna
  centrale.
- Chat: `Transcript.svelte`, `cards/*.svelte`, `Composer.svelte`,
  `ComposerControlBar.svelte`, `markdown-render.ts` (sanitizzazione 6b-3 —
  NON toccarla).
- I criteri chat girano con l'agente `fake` (`TILLER_FAKE_AGENT=1`).
- Icone Swift: percorsi e coordinate esatte in spec §8.

---

## Task 1: finestra massimizzata con memoria dei bounds

**File:** modifica `src/main/index.ts`; crea `src/main/window/bounds.ts` +
`bounds.test.ts`; modifica `e2e/rifinitura-visiva.spec.ts` (crea il file).

**Interfacce:** `export function boundsIniziali(salvati: SavedBounds | null,
displays: Rect[]): { maximize: boolean; bounds: SavedBounds | null }` — pura:
`salvati === null` → maximize; salvati dentro un display esistente → quei
bounds; salvati fuori da ogni display → maximize. `SavedBounds =
{ x, y, width, height }`. Persistenza alla chiusura nello stesso posto delle
altre preferenze dell'app (trova dove vivono — se non esiste un posto, JSON
`window-bounds.json` in `app.getPath('userData')`).

- [ ] Test unit rossi su `boundsIniziali` (3 casi sopra) → implementa → verdi.
- [ ] Criterio 1 e2e, rosso prima:

```
criterio 1: al primo avvio la finestra è massimizzata; a un secondo avvio con
bounds salvati li ripristina
```

(`app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMaximized())`;
per il secondo avvio riusa il pattern di riavvio dei test 5c.)

- [ ] `git commit -m "feat: launch maximized and restore window bounds"`

## Task 2: la chat diventa un tab del worktree

**File:** modifica `App.svelte` (via la colonna 38% e il mount fisso di
`ChatPane`); il motore tab (`TopBar.svelte`/`topbar-segments.ts` e il layout
`workspaceTab`) guadagna il genere chat se non ce l'ha già — **verifica prima
se `contentKind` prevede già `chat`** (la migrazione universale lo aveva):
se sì è solo un collegamento, dichiaralo.

- [ ] Criterio 2 e2e, rosso prima:

```
criterio 2: la chat si apre come tab (il transcript compare nell'area contenuto
del tab attivo), la colonna fissa non esiste, e i criteri chat della 5c restano
verdi
```

L'asserzione strutturale: `[data-message-kind]` è discendente dell'area tab,
e la griglia non ha più la colonna chat (misura le colonne, non i pixel).

- [ ] Dopo il verde: `npx playwright test e2e/fase-5c-chat.spec.ts` — tutti i
  criteri della 5c devono restare verdi (stessa chat, altro posto).
- [ ] `git commit -m "feat: open the chat as a worktree tab"`

## Task 3: chrome senza cuciture e status bar a tutta larghezza

**File:** `App.svelte` (griglia: status bar fuori dalla colonna centrale,
figlia della finestra), `TopBar.svelte`, `SidebarTree.svelte`/stili sidebar,
`StatusBar.svelte`.

Cosa cambia: via l'hairline sotto la barra del titolo e il bordo destro della
sidebar (chrome continuo `--t-surface-chrome`); resta l'hairline sopra la
status bar; la status bar attraversa l'intera finestra (sinistra:
progetto · branch · modifiche; destra: `QuotaBar`).

- [ ] Criterio 3 e2e, rosso prima:

```
criterio 3: la status bar è larga quanto la finestra (boundingBox.width ==
viewport width), non quanto la colonna centrale
```

- [ ] Schermata di confronto col mockup (chrome cucito, prima/dopo).
- [ ] `git commit -m "feat: seamless chrome and full-width status bar"`

## Task 4: tab attivo come riquadro chiaro, con identità

**File:** `TabSegment.svelte` (o dove vive lo stile del tab), `AgentIcon`
dentro il tab.

Attivo: sfondo `--t-surface-pill`, raggio `--t-radius-3`, testo
`--t-text-selected`, **nessuna sottolineatura**; inattivi solo testo
`--t-text-meta`. Ogni tab porta `AgentIcon` (agente) o icona di genere
(terminale/chat); il monogramma nel tab attivo inverte lo sfondo (chrome su
pillola).

- [ ] Criterio 4 e2e, rosso prima:

```
criterio 4: il tab attivo ha lo sfondo pillola e il tab inattivo no
(si asserisce sulla computed background-color via i token, non su un
esadecimale)
```

- [ ] Schermata. - [ ] `git commit -m "feat: active tab as a light box with agent identity"`

## Task 5: chat prosa-prima

**File:** `Transcript.svelte`, `cards/*.svelte`, `Composer.svelte`,
`ComposerControlBar.svelte`.

Dalla spec §5, in sintesi vincolante: prosa senza card (corpo
`--t-text-subtitle`, enfasi `--t-text-title` peso 500); codice inline
`--t-git-modified` monospace; tool call = riga compatta su `--t-surface-field`
con icona, nome monospace, check `--t-git-staged`, genere a destra, chevron;
label «Running … tool:» sopra in meta; passi collassati «✓ …» in meta; card
solo per Insight/Task/permission/plan/diff (rail, angoli sinistri a zero);
errori = banner `--t-diff-del-bg`/`--t-git-conflict`; utente = pillola
compatta a destra su `--t-surface-pill`; composer con control bar interna
(hairline `--t-row-selected`, «+» e agente·modello a sinistra, permessi e
Send `--t-rail-task` pieno a destra).

- [ ] Criterio 5 e2e, rosso prima:

```
criterio 5: la prosa dell'agente NON ha uno sfondo card (computed background
trasparente), una tool call ha la sua riga compatta, e un messaggio utente è
allineato a destra
```

- [ ] Dopo il verde: i criteri 5c di nuovo tutti verdi (4/5 inclusi: il
  markdown e la sanitizzazione non si toccano).
- [ ] Schermata di confronto col mockup della chat.
- [ ] `git commit -m "feat: prose-first chat rendering"`

## Task 6: i loghi degli harness

**File:** `AgentIcon.svelte` (riscrittura interna, stessa interfaccia);
copia `claude.svg` e `openai.svg` da
`~/Desktop/Progetti/tiller/App/Assets.xcassets/agent-{claude,codex}.imageset/`
in `src/renderer/src/assets/agents/`.

Dalla spec §8: claude tinta `#D97757`; codex `currentColor`; opencode due rect
even-odd 240×300 (cornice + interno 60,120,120,120); pi path even-odd 800×800
(coordinate esatte in `App/AgentIcon.swift` del repo Swift, righe 124-149 —
leggile, non stimarle); omp path 64×64 con gradiente `#ED4ABF→#9B4DFF→#5AD8E6`
(righe 153-172); sconosciuti = monogramma su cerchio (arancio/verde/blu/
viola/teal/grigio). Id `-acp` normalizzati. Colori di marca = eccezione
scale-guard limitata al file, motivata.

- [ ] Test unit rosso: `AgentIcon` per i 5 id noti rende un SVG con
  `data-agent="<id>"` e per un id ignoto rende il monogramma → implementa →
  verde. Schermata della sidebar coi loghi.
- [ ] `git commit -m "feat: real harness marks in the agent icon"`

## Task 7: chiusura

- [ ] Baseline: `npx electron-vite build && npx playwright test e2e/rifinitura-visiva.spec.ts` (è la baseline delle mutazioni, non un run in più).
- [ ] Mutazioni: (a) ripristina la colonna 38% → solo criterio 2 rosso;
  (b) status bar di nuovo nella colonna → solo criterio 3 rosso;
  (c) unit: `boundsIniziali` ignora i display → i suoi test rossi.
- [ ] `npx playwright test e2e/fase-5c-chat.spec.ts e2e/sidebar-tre-livelli.spec.ts` — verdi (la rifinitura non rompe ciò su cui si appoggia).
- [ ] `npm run typecheck` e `npm run lint` (0 errori; warning non oltre il baseline trovato).
- [ ] Schermate finali: finestra intera scura + tema chiaro
  (`[data-theme='light']`), confronto col mockup.

## Report finale

Commit; output verbatim; rossi-prima; esito mutazioni; schermate (percorsi);
ogni punto del piano trovato sbagliato, detto invece che aggirato; cosa non
verificato.

**Fuori scopo:** animazioni nuove; riordino per stato; valori dei token
(non si toccano); il pannello destro Files/Changes (resta com'è).
