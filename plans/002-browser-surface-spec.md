# Spec 002: browser surface in Tiller (parità cmux)

> Documento di **decisioni e contratto**. L'esecuzione è in
> [`002-browser-surface-plan.md`](./002-browser-surface-plan.md). Questo file non
> contiene passi: contiene ciò che è stato deciso, perché, e cosa deve valere a
> feature chiusa. Se durante l'implementazione una decisione va cambiata, si
> aggiorna questo file **prima** di scrivere il codice divergente.

## Stato

- **Priorità**: P2 (feature nuova, nessun bug aperto dipende da essa)
- **Effort complessivo**: L, suddiviso in 8 fasi S/M indipendenti
- **Rischio**: MED — tocca `WorkspaceContentRef` (usato da tutto il motore universale), il protocollo del control socket, e aggiunge processi `WebContent` a un'app già pesante in IOSurface
- **Dipende da**: nessun altro piano
- **Categoria**: feature
- **Pianificato su**: commit `d161896`, branch `main`, working tree pulito, 2026-08-07

## Perché

Gli agenti che girano in Tiller modificano applicazioni web e non hanno modo di
verificare il risultato: l'unica strada oggi è chiedere all'umano di guardare.
cmux ha risolto lo stesso problema con una browser surface pilotabile dal
control socket, e gli agenti che usiamo (Claude Code, Codex, OpenCode) sono
letteralmente gli stessi. Copiare il contratto significa che la loro skill
`cmux-browser` funziona in Tiller quasi senza modifiche.

Secondo motivo, più banale ma quotidiano: `cmd+click` su un `http://localhost:5173`
in un terminale oggi apre Safari e ti sposta fuori dall'app.

## Precedente: cosa fa cmux (verificato, non dedotto)

Repo `manaflow-ai/cmux`, Swift + libghostty, stessa famiglia di Tiller.

| Fatto | Evidenza |
|---|---|
| Motore = WKWebView, nessun CDP | `Sources/Panels/WKWebView+BrowserViewport.swift`, `WKWebView+BrowserUserAgentPolicy.swift`, `CmuxWebViewSupport.swift` |
| Il browser è una `surface` (tab dentro un pane), stesso slot del terminale | `docs/agent-browser-port-spec.md`, sezione "Concepts": `window > workspace > pane > surface` |
| Le API agente sono un port di `vercel-labs/agent-browser` @ `03a8cb9` | `docs/agent-browser-port-spec.md` |
| Loop canonico: `open → get url → wait → snapshot --interactive → act su ref → re-snapshot` | `skills/cmux-browser/SKILL.md` |
| Targeting implicito dal terminale chiamante (`CMUX_WORKSPACE_ID`), override `--workspace`/`--window` | `skills/cmux-browser/SKILL.md` |
| Ref brevi `surface:7`, UUID accettati in input, `--id-format uuids\|both` in output | idem |
| `not_supported` su WKWebView: offline emulation, trace/screencast recording, network route/mock, raw input injection | idem, sezione "Limits" |
| Pagine complesse rifiutano il JS di `snapshot --interactive` e `eval` → `js_error`, recovery documentato come procedura dell'agente | idem, sezione "Troubleshooting `js_error`" |
| `⌘⇧L` apre browser in split, `⌘L` focus address bar | README cmux |

`cmux-browser/` nel loro repo è **un'altra cosa**: un fork Chromium (derivato
Helium, uBlock Origin, GPL-3) come browser standalone futuro. Non è il pane
in-app e non ci riguarda.

## Stato di Tiller rilevante (verificato su `d161896`)

| Fatto | File |
|---|---|
| `WorkspaceContentRef` ha 3 casi: `.terminal`, `.chat`, `.document(_, editor:)` | `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceContentRef.swift:14` |
| `cmd+click` sui link terminale già instradato ad AppModel | `Packages/TillerTerminal/…/TerminalOpenURLRouter.swift:20` → `App/AppModel.swift:2391` (`handleTerminalOpenURL` → `openFileReference`, non-file → apertura di sistema) |
| Zero WebKit nel repo | `grep -rl WKWebView App Packages` → vuoto |
| Il motore di split **non guarda il kind**: `SplitContentPayload` = `.newTab(WorkspaceTab)` \| `.existingTab(id)` | `Packages/TillerCore/…/WorkspaceLayoutEngine.swift:256` |
| `SplitEligibility.check` filtra solo geometria + `soleTabOfItsOwnGroup` | `Packages/TillerWorkspace/…/SplitEligibility.swift:14` |
| `SplitContentMenuAction` è un **enum chiuso** senza voce browser | `App/Workspace/SplitContentMenu.swift:24` |
| `WorkspaceSnapshot.materialize()` ricostruisce **ogni** tab come `.terminal(TerminalContentID(tabID.rawValue))`; usato solo come **validatore strutturale** | `Packages/TillerCore/…/WorkspaceSnapshot.swift:60`, chiamanti `App/Workspace/SQLiteWorkspacePersistence.swift:140` e `:255` |
| `workspaceTab` ha già `contentKind TEXT` + `contentId TEXT` + `uniqueKey(["worktreeId","contentKind","contentId"])` | `Packages/TillerPersistence/…/AppDatabase.swift:245` (migrazione `v16`) |
| Ultima migrazione registrata: `v17` | `AppDatabase.swift:283` |
| `⌘L` e `⌘⇧L` sono **libere**. Occupate: `⌘T`, `⌘W`, `⌘O`, `⌘⇧O`, `⌘S`, `⌘,`, `⌃⌘I`, `⌃⇥`, `⌘⌥`+frecce | `grep keyboardShortcut( App` |
| Tiller **non è sandboxed**: unico entitlements è `App/InjectDebug.entitlements` (Debug, `disable-library-validation`) | `project.yml:72` |
| Nessun supporto a ref brevi nel control socket: oggi solo UUID | `grep -rn "surface:\|shortRef\|idFormat" Packages/TillerControl/Sources` → vuoto |

## Decisioni

| # | Decisione | Scelta | Nota |
|---|---|---|---|
| D1 | Motore | **WKWebView** | Come cmux. CEF/Chromium romperebbe signing, notarization e dimensione del bundle |
| D2 | Modello | Nuovo caso `.browser(BrowserContentID)` in `WorkspaceContentRef`, surface di prima classe, persistita | `contentKind = "browser"` è una stringa nuova in una colonna che esiste già |
| D3 | Trigger terminale | `cmd+click` su `http(s)` → browser interno **sempre**; **un tab riusato** per worktree; `cmd+shift+click` → browser di sistema | Regole per-host (solo localhost) scartate: tradiscono al primo link a documentazione |
| D4 | Auto-detect dev server nello scrollback | **No** in V1 | Quando servirà, si aggancia a `ScreenManifest` (Layer C), non a un nuovo scanner |
| D5 | Link nelle chat | Stessa regola di D3, nessuna eccezione | Due politiche per la stessa azione = nessuno le ricorda |
| D6 | Verbi V1 | `open`, `back`/`forward`/`reload`, `get url\|text\|html`, `screenshot`, `snapshot --interactive`, `click`, `fill`, `type`, `press`, `wait`, `eval`; poi `console`/`errors` | Sotto questa soglia l'agente non può *verificare* nulla |
| D7 | CLI | `tillerctl browser <surface> <verb>`, targeting implicito dal pane chiamante | Copia cmux verbatim per riusare la loro skill |
| D8 | Sessione | `WKWebsiteDataStore` **persistente per worktree** | Compartimenta il raggio d'azione di un agente compromesso |
| D9 | Codice | Nuovo package leaf **`TillerBrowser`**, unico a importare WebKit | Il port è logica, non UI: in `App/` finirebbe non testato |
| D10 | Budget surface | Tetto **sui soli nascosti**: max 3 vive, la 4ª nascosta si smonta (ricarica per URL al ritorno). **I visibili sono sempre esenti** | Uno split mostra più browser insieme: un tetto globale smonterebbe un pane sotto le mani |
| D11 | Shortcut | `⌘⇧L` apri browser, `⌘L` address bar | Verificate libere |
| D12 | Titolo tab | Titolo pagina live + favicon; **auto-rename escluso** per `kind == .browser` | Il titolo pagina è già il nome giusto |
| D13 | Chrome UI | back/forward/reload/URL/stop + **badge "agent driving"** | Senza badge, click umano e click agente sono indistinguibili |
| D14 | Ref brevi | Resolver `surface:N ↔ UUID` **solo nel namespace `browser`** | Estenderlo a tutto il protocollo romperebbe gli hook installati |
| D15 | Gating | Tutto libero su `localhost`/`127.0.0.1`/`::1`/`*.local`; altrove **conferma una volta per origine** per ogni verbo che legge o esegue il contenuto della pagina, `screenshot` incluso | Vedi "Modello di sicurezza" (emendata 2026-08-07) |
| D16 | `js_error` | Errore strutturato con `code` + `hint`; **nessun fallback automatico** | Un fallback silenzioso fa credere all'agente di avere un DOM interattivo |
| D17 | Download/upload | `not_supported` in V1 | Fuori dal caso d'uso; ogni download è scrittura su disco guidata da agente |
| D18 | Persistenza | Solo **URL + titolo** | `interactionState` è un blob opaco versionato da Apple |
| D19 | User agent | UA **Safari desktop fissa**, un punto di override predisposto | La policy per-dominio di cmux è nata *dopo* domini specifici: astrazione speculativa oggi |
| D20 | Navigazione | `target=_blank` → **stessa surface**; schemi non-http → sistema **solo da gesto umano**, ignorati con log se originati da `eval`/`click` agente | Altrimenti `eval` diventa un lanciatore di app arbitrarie via URL scheme |
| D21 | Chiusura | Unit test sul package + **e2e su server locale** (zero rete esterna) + test tabellare `not_supported` | Il gate è già rosso su 4 suite: l'e2e non deve diventare la quinta |
| D22 | Motore | Il browser è **universal-engine-only**: `SidebarTabProjection` scarta i tab browser invece di aggiungere un caso a `LegacyTabContent`; col gate legacy forzato le surface browser non si vedono e "New Browser" è indisponibile | Deciso 2026-08-07 durante l'esecuzione della Fase 2. Il motore universale è default-ON (`WorkspaceEngineGate`, cfr. `missingUserDefaultsValueEnablesTheUniversalEngine`) e il legacy è un opt-out: aggiungere un caso a un modello in ritiro significherebbe mantenere due rappresentazioni del browser per sempre per servire un escape hatch |

## Contratto control socket

Namespace `browser.*`, dispatch in `App/AppModel+Control.swift`, uno per
famiglia di verbi. Tutte le richieste accettano `surface` come UUID **o** come
ref breve `surface:N` (D14).

| Request | Payload | Risposta |
|---|---|---|
| `browser.open` | `{url, workspace?, window?}` | `{surface, url, title}` |
| `browser.navigate` | `{surface, action: back\|forward\|reload}` | `{url, title}` |
| `browser.get` | `{surface, what: url\|text\|html, selector?}` | `{value}` |
| `browser.snapshot` | `{surface, interactive: Bool}` | `{generation, nodes: [{ref, role, name, value?, box}]}` |
| `browser.act` | `{surface, verb: click\|fill\|type\|press\|scroll, ref?, selector?, value?, snapshotAfter?}` | `{ok, snapshot?}` |
| `browser.wait` | `{surface, selector?, text?, urlContains?, loadState?, function?, timeoutMs}` | `{ok, elapsedMs}` |
| `browser.eval` | `{surface, script}` | `{value}` |
| `browser.screenshot` | `{surface, path?}` | `{path}` |
| `browser.console` | `{surface, since?}` | `{entries: [{level, text, at}]}` |

### Ciclo di vita dei ref

`browser.snapshot` restituisce una `generation` intera, incrementata a ogni
navigazione committata e a ogni nuovo snapshot. `browser.act` con un `ref`
prodotto da una `generation` precedente fallisce con `stale_ref` — **non**
tenta un match euristico. È l'unico modo onesto di rendere osservabile il
problema che la doc cmux descrive come la prima causa di errori degli agenti.

### Codici di errore

`surface_not_found`, `stale_ref`, `js_error` (+ `hint`), `timeout`,
`not_supported`, `origin_denied`, `invalid_url`, `invalid_argument`.

`invalid_argument` (aggiunto 2026-08-07 durante la Fase 4) è obbligatorio per
un valore di parametro malformato o non supportato — `action`, `what`, e ogni
enumerazione futura — e il messaggio deve nominare il parametro e i valori
accettati. **Non** si collassa con `surface_not_found`: l'API è pilotata da
agenti, e un agente agisce sul codice che riceve. Detto `surface_not_found`,
va a cercare una surface che esiste invece di correggere l'argomento. Regola
generale: un `guard` multi-binding il cui unico `else` unifica cause distinte
è un difetto, non una scorciatoia di stile.

`not_supported` è la risposta obbligatoria — non un errore generico — per:
offline emulation, trace/screencast/record, network route/mock/intercept, raw
input injection, `download`, `upload`, `pdf`, `viewport` (fuori V1).

## Modello dati

- `BrowserContentID` (UUID) in `WorkspaceIDs.swift`.
- `WorkspaceContentRef.browser(BrowserContentID)`; `contentIdentifierString` =
  UUID string; `WorkspaceContentKind.browser` con raw value `"browser"`.
- Migrazione **`v18`**: una sola `CREATE TABLE browserContent` gemella di
  `terminalContent` — `id TEXT PK`, `worktreeId TEXT NOT NULL REFERENCES worktree ON DELETE CASCADE`,
  `url TEXT NOT NULL`, `title TEXT`, `createdAt DATETIME NOT NULL`.
- `workspaceTab` **non cambia**: `contentKind = "browser"` sfrutta la colonna
  esistente e il vincolo unique già in `v16`.
- Nessuna scrittura di `interactionState` (D18).

## Modello di sicurezza

Con store persistente per worktree (D8), una sessione autenticata reale può
vivere nel browser. `eval` libero su quell'origine significa che qualunque testo
che l'agente legge in una pagina — inclusa una pagina ostile — può indurlo a
leggere `document.cookie` o `localStorage` e a fare `fetch` verso l'esterno.
Prompt injection dentro una pagina ispezionata diventa un percorso verso i
token dell'utente.

Regola (D15, **emendata il 2026-08-07 — decisione A**), implementata in
`OriginPolicy` (puro, testabile senza WebKit) e applicata da
`BrowserSurface.authorizePageAccess`:

1. **Origini locali** — `localhost`, `127.0.0.1`, `::1`, `*.local`, qualsiasi
   porta: tutti i verbi permessi senza conferma.
2. **Ogni altra origine** — la linea passa fra *leggere o eseguire il contenuto
   della pagina* e *guidare la finestra*. Richiedono conferma: `eval`, `act`,
   `get text`, `get html`, `snapshot`, `console`, `wait` (valuta predicati) e
   **`screenshot`**. Non la richiedono: `open`, `navigate`, `get url`, e la
   lettura di titolo/favicon con cui Tiller disegna il proprio chrome.
   La concessione è ricordata per `(worktree, origine)` e revocabile
   dalle Settings.
3. **Conferma negata o assente** → `origin_denied`. Mai un fallback silenzioso.
4. Schemi non-http originati da un'azione agente sono **ignorati e loggati**,
   non aperti (D20).

La formulazione precedente escludeva `get`, `snapshot` e `screenshot` dal gate
perché «non estraggono credenziali». È sbagliata: su una pagina autenticata il
testo e l'immagine trasportano gli stessi dati derivati dalla sessione a cui il
cookie dà accesso, e `screenshot` ne trasporta di più. La distinzione utile non
è *credenziale sì/no*, è *contenuto della pagina sì/no*.

**Eccezione, deliberata e in `PageAccessPurpose.chromeMetadata`:** titolo e
favicon vengono letti in JavaScript dopo ogni navigazione, inclusa quella di un
umano che digita nella barra indirizzi. Gatarli chiedeva il permesso per un
agente inesistente su ogni sito visitato, e insegnava a concedere per riflesso
esattamente il permesso che serve a `eval`. Un gate che scatta troppo spesso
vale meno di nessun gate, perché produce consenso automatico.

Non è nell'ambito di questa spec: sandboxing dell'app, isolamento di rete per
worktree, filtri sui contenuti.

## Invarianti a feature chiusa

1. Un tab browser si splitta, si trascina fra pane e sopravvive al restore,
   esattamente come un tab chat o document — **e la voce per crearlo è
   raggiungibile dal menu di split**, non solo dal codice.
2. Nessun percorso mostra una surface browser senza il badge quando un comando
   agente è in volo.
3. Nessun verbo non implementato risponde con successo: o funziona, o
   `not_supported`.
4. `Scripts/ci.sh` non peggiora rispetto alla baseline rossa del 2026-08-06
   (4 suite pre-esistenti).
5. Con 5 tab browser aperti di cui 2 visibili, le surface vive sono al massimo
   2 visibili + 3 nascoste; nessuna surface visibile viene mai smontata.

## Non-obiettivi espliciti

Import di cookie/sessioni da Chrome/Firefox/Arc; network interception; proxy;
registrazione video; `viewport` emulation; policy UA per-dominio; devtools
inspector; estensioni; bookmark; history persistente cross-sessione.
