# Piano 002: browser surface pilotabile dagli agenti

> **Istruzioni per l'esecutore**: seguire il piano fase per fase. Eseguire ogni
> verifica e confermare il risultato atteso prima di continuare. Se si verifica
> una delle condizioni "STOP", fermarsi e riferire senza improvvisare. Non
> sovrascrivere né annullare modifiche già presenti nel working tree.
>
> Le decisioni sono in [`002-browser-surface-spec.md`](./002-browser-surface-spec.md)
> e **non si rinegoziano in corso d'opera**: se una decisione risulta sbagliata,
> fermarsi, riferire, aggiornare la spec, poi riprendere.
>
> **Non eseguire `Scripts/ci.sh`**: il gate lo lancia solo l'orchestratore, una
> volta per lotto. Per fase, eseguire solo i test del package toccato.
>
> **Tutte le stringhe UI in inglese**, anche se il piano è in italiano.
>
> **Drift check iniziale**:
>
> ```bash
> git status --short
> git log --oneline -1
> git diff --stat d161896..HEAD -- App Packages AppTests
> ```
>
> Il piano è stato preparato su `d161896` (`main`, working tree pulito). Prima di
> ogni fase confrontare i simboli citati in "Stato corrente" con il codice
> attuale; se non corrispondono più, applicare la STOP condition della fase.

## Stato

- **Priorità**: P2
- **Effort**: L (8 fasi, S/M ciascuna)
- **Rischio**: MED
- **Dipende da**: nessun altro piano
- **Categoria**: feature
- **Pianificato su**: commit `d161896`, 2026-08-07

## Stato corrente (simboli su cui il piano si appoggia)

| Simbolo / file | Riga | Ruolo nel piano |
|---|---|---|
| `WorkspaceContentRef` | `Packages/TillerCore/Sources/TillerCore/Workspace/WorkspaceContentRef.swift:14` | Fase 2 aggiunge `.browser` |
| `WorkspaceIDs.swift` | `Packages/TillerCore/…/Workspace/WorkspaceIDs.swift` | Fase 2 aggiunge `BrowserContentID` |
| `AppDatabase.makeMigrator` ultima migrazione `v17` | `Packages/TillerPersistence/…/AppDatabase.swift:283` | Fase 2 aggiunge `v18` |
| `workspaceTab` con `contentKind`/`contentId` + unique | `AppDatabase.swift:245` (`v16`) | **Non cambia**: `"browser"` è un nuovo valore |
| `WorkspaceSnapshot.materialize()` con `.terminal(...)` hardcoded | `Packages/TillerCore/…/WorkspaceSnapshot.swift:60` | Fase 2 aggiunge il test di non-regressione |
| `SQLiteWorkspacePersistence` chiamanti di `materialize()` | `App/Workspace/SQLiteWorkspacePersistence.swift:140`, `:255` | Solo validazione strutturale: non toccare |
| `SplitContentMenuAction` (enum chiuso) | `App/Workspace/SplitContentMenu.swift:24` | Fase 3 aggiunge `newBrowser` |
| `SplitEligibility.check` (solo geometria) | `Packages/TillerWorkspace/…/SplitEligibility.swift:14` | Nessuna modifica: conferma che lo split è agnostico |
| `WorkspaceLayoutEngine.splitGroup` / `SplitContentPayload` | `Packages/TillerCore/…/WorkspaceLayoutEngine.swift:256` | Nessuna modifica |
| `TerminalOpenURLRouter.route` | `Packages/TillerTerminal/…/TerminalOpenURLRouter.swift:20` | Fase 5: punto di innesto già esistente |
| `AppModel.handleTerminalOpenURL` | `App/AppModel.swift:2391` | Fase 5: qui si biforca http(s) → surface interna |
| `ControlServer` + `TillerctlRequestBuilder` | `Packages/TillerControl/Sources/TillerControl/` | Fase 4/6: verbi `browser.*` e CLI |
| `App/AppModel+Control.swift` | — | Fase 4/6: dispatch |
| `project.yml` | — | Fase 1: nuovo package + target di test |

## Fase 1 — package `TillerBrowser`, navigazione minima

**Obiettivo**: un motore WKWebView headless-testabile che apre un URL e sa dire
`url`, `title`, `text`, `html`. Nessuna UI, nessun socket.

**File nuovi**: `Packages/TillerBrowser/Package.swift`,
`Sources/TillerBrowser/{BrowserEngine,BrowserSurface,BrowserCommand,BrowserError,UserAgentPolicy}.swift`,
`Tests/TillerBrowserTests/`.

**Passi**
1. Package leaf: dipendenze **solo** Foundation + WebKit. Non importare
   `TillerCore` (il ref di contenuto lo conosce solo Core, non il motore).
2. `BrowserSurface`: attore `@MainActor` che possiede una `WKWebView` con
   `WKWebViewConfiguration.websiteDataStore` iniettabile dall'esterno (Fase 7
   passerà quello per-worktree; in test si passa `.nonPersistent()`).
3. UA fissa Safari desktop via `customUserAgent`, dietro una `UserAgentPolicy`
   con un unico punto di override (D19). Nessuna logica per-dominio.
4. `BrowserCommand` come enum dei verbi + `BrowserError` con i codici della
   spec. In questa fase implementare solo `open`, `navigate(back|forward|reload)`,
   `get(url|text|html)`.
5. `get text`/`get html` via `evaluateJavaScript` su `document.body`; su
   fallimento restituire `.jsError(hint:)`, mai stringa vuota (D16).
6. Registrare il package e il target di test in `project.yml`, poi
   `xcodegen generate`.

**Verifica**
```bash
cd Packages/TillerBrowser && swift test
```
Test attesi: apre un `file://` di fixture e legge `url`/`title`/`text`; `reload`
non cambia l'URL; `back` senza history restituisce un errore, non un crash;
`open` con URL malformato → `invalid_url`.

**STOP** se `xcodegen generate` produce un progetto che non compila, o se
WebKit non è linkabile da un package SPM senza aggiungere entitlements
(l'app non è sandboxed: non dovrebbe servire nulla — se serve, fermarsi e
riferire, **non** aggiungere entitlements di iniziativa).

## Fase 2 — `.browser` nel modello e in persistenza

**Obiettivo**: una surface browser esiste nel modello, si serializza, si
restaura. Nessuna UI ancora.

**Passi**
1. `BrowserContentID` in `WorkspaceIDs.swift`, stessa forma di
   `TerminalContentID`.
2. `WorkspaceContentRef.browser(BrowserContentID)`;
   `WorkspaceContentKind.browser` (raw `"browser"`);
   `contentIdentifierString` = UUID string. Compilare e **seguire ogni errore
   di switch esaustivo**: ogni sito che il compilatore segnala è un lettore da
   valutare, non da silenziare con `default`.
3. Migrazione `v18` in `AppDatabase.swift`: solo `CREATE TABLE browserContent`
   (`id`, `worktreeId` con cascade, `url`, `title`, `createdAt`).
   **Non** toccare `workspaceTab`.
4. Lettura/scrittura in `SQLiteWorkspacePersistence` accanto a
   `terminalContent`, incluso il purge per worktree.
5. Test di non-regressione su `WorkspaceSnapshot.materialize()`: un layout con
   un tab `.browser` deve **validare** (`.success`) nonostante il
   `.terminal(...)` hardcoded a `WorkspaceSnapshot.swift:60`, e il
   `duplicateContentOwnership` di `splitGroup` non deve scattare.

**Verifica**
```bash
cd Packages/TillerCore && swift test
cd Packages/TillerPersistence && swift test
```
Più un test di round-trip: crea worktree con un tab browser, `flush`, `restore`,
URL e titolo tornano identici.

**STOP** se un lettore esistente richiede `default:` per compilare — significa
uno switch non esaustivo che nasconderà la nuova surface (è il pattern
"entry point legacy-only", quarta ricorrenza nel repo): riferire l'elenco dei
siti prima di procedere.

## Fase 3 — pane, chrome UI, creazione da menu e shortcut

**Obiettivo**: l'umano apre, vede e naviga un browser; lo splitta e lo trascina.

**Passi**
1. `App/Browser/BrowserPaneView.swift`: `NSViewRepresentable` sulla surface di
   `TillerBrowser` + barra back/forward/reload/URL/stop (D13). Stringhe in
   inglese.
2. Cablare il pane nel renderer di contenuto accanto a terminal/chat/document.
3. **`SplitContentMenuAction.newBrowser`** in `App/Workspace/SplitContentMenu.swift`
   + voce di menu + wiring dell'azione. Senza questo la surface è
   irraggiungibile dalla UI (`moveExistingTab` funzionerebbe, `newBrowser` no).
4. Titolo tab = titolo pagina live + favicon; **escludere `kind == .browser`
   dall'auto-rename** (D12) nel punto in cui l'auto-rename legge le surface
   (`activityOwner`, non i lettori legacy).
5. Shortcut: `⌘⇧L` nuovo browser nel pane attivo, `⌘L` focus address bar.
6. **Obbligo ereditato dalla Fase 2** (debito noto, non opzionale):
   `WorkspaceCoordinator.browserRecords(in:worktreeID:)` oggi deriva i record
   **solo** da `browserContents[worktreeID]`, popolato esclusivamente da
   `restore`. Un tab browser **creato durante la sessione** non ha voce lì e il
   suo record non viene mai persistito — la persistenza copre solo i browser già
   nel DB, cioè non il caso che conta. La surface viva deve riportare URL e
   titolo correnti al coordinator (il meccanismo esiste già:
   `adapters: [WorkspaceContentKind: WorkspaceContentAdapter]`), e
   `browserRecords` deve leggere dallo stato **vivo**, usando quello restorato
   solo come fallback. Verifica: creare un browser, navigare, riavviare,
   ritrovare l'URL — con un test a livello coordinator, non a livello
   persistence.

**Verifica**
- `cd Packages/TillerWorkspace && swift test` (regressioni layout).
- Manuale: apri browser, naviga, titolo e favicon corretti; **splittalo**
  (menu → new browser in split); trascina il tab in un altro pane; nessuna
  finestra di rename automatico compare.
- Screenshot di ognuno dei tre passaggi manuali.

**STOP** se il menu di split mostra la voce ma l'azione non crea nulla: è il
sintomo del modello mai chiamato già visto nel drag&drop (Fase 5 di quel
lavoro) — riferire prima di aggiungere codice altrove.

## Fase 4 — verbi socket di lettura + resolver ref brevi

**Obiettivo**: un agente apre una pagina e la legge dal socket.

**Passi**
1. `browser.open`, `browser.navigate`, `browser.get`, `browser.screenshot` nel
   protocollo `TillerControl`; dispatch in `App/AppModel+Control.swift`.
2. Targeting implicito: se `workspace`/`window` mancano, la surface nasce nel
   worktree del pane chiamante (come cmux con `CMUX_WORKSPACE_ID`).
3. Resolver ref brevi `surface:N ↔ UUID` **solo per il namespace `browser`**
   (D14): accetta entrambi in input, emette UUID salvo `--id-format`.
4. Comandi `tillerctl browser <surface> <verb>` in `TillerctlRequestBuilder`,
   nomi identici a cmux.

**Verifica**
```bash
cd Packages/TillerControl && swift test
```
Più uno smoke via socket: `tillerctl browser open http://127.0.0.1:PORT` (server
di fixture locale) → ritorna un `surface:N`; `get url` lo conferma.

**STOP** se il resolver richiede di cambiare la forma di richieste esistenti:
gli hook già installati sui worktree usano il protocollo attuale. Confinare il
resolver al namespace nuovo o fermarsi.

## Fase 5 — routing dei link da terminale e chat

**Obiettivo**: `cmd+click` su un URL apre il browser interno.

**Passi**
1. In `AppModel.handleTerminalOpenURL` (`App/AppModel.swift:2391`): biforcare
   **prima** di `openFileReference` — schema `http`/`https` → surface browser
   del worktree (riuso di quella esistente, D3); tutto il resto invariato.
2. `cmd+shift+click` → `NSWorkspace.open` (browser di sistema).
3. Stesso routing per i link nelle chat (D5), riusando la **stessa** funzione:
   una sola politica, un solo punto di codice.
4. `target=_blank` → stessa surface (D20). Schemi non-http: aperti dal sistema
   solo se la navigazione nasce da un gesto umano; se nasce da `eval`/`act`
   agente, ignorati e loggati.

**Verifica**
- Manuale: `python3 -m http.server` in un worktree, `cmd+click` sull'URL
  stampato → si apre dentro; secondo click su un URL diverso → **stesso tab**,
  non un secondo; `cmd+shift+click` → Safari; `cmd+click` su un `.md` continua
  ad aprire l'editor (non-regressione).
- Test: un `AppTests` che asserisce la biforcazione http vs file su
  `handleTerminalOpenURL` senza toccare WebKit.

**STOP** se il routing dei file markdown/codice cambia comportamento.

## Fase 6 — loop agente completo

**Obiettivo**: `snapshot → ref → act → wait → re-snapshot` funzionante.

**Passi**
1. `SnapshotBuilder` in `TillerBrowser`: `WKUserScript` che marca gli elementi
   interattivi e restituisce `{generation, nodes:[{ref, role, name, value?, box}]}`.
   Ref `e1`, `e2`, … stabili **dentro** una generation.
2. `generation` incrementata a ogni navigazione committata e a ogni snapshot;
   `browser.act` con ref di generation vecchia → `stale_ref`, **senza** match
   euristico.
3. `browser.act` per `click`, `fill`, `type`, `press`, `scroll`, con
   `snapshotAfter` opzionale.
4. `browser.wait` con `selector` | `text` | `urlContains` | `loadState` |
   `function` + `timeoutMs` obbligatorio.
5. `browser.eval`; `browser.console` via `WKUserScript` che patcha `console.*`
   e accoda in un buffer limitato.
6. Tabella `not_supported` (D17 + non-obiettivi): ogni verbo elencato risponde
   `not_supported`, mai un successo silenzioso.
7. **Obbligo ereditato dalla Fase 5** (debito noto, non opzionale):
   `AppModel.handleAgentOpenURL` esiste ed è il punto in cui le navigazioni
   originate da un agente vengono classificate `.agentAction` — oggi però è
   chiamato **solo da un test**, perché il suo chiamante di produzione nasce
   qui. Ogni navigazione che un agente può innescare (`eval` che scrive
   `location.href`, `browser.act` su un anchor, redirect indotto) deve passare
   da quella policy: se la Fase 6 instrada altrove, il divieto D20 sugli URL
   scheme resta scritto e aggirato, e `eval` torna a essere un lanciatore di app
   arbitrarie. Verifica: un `eval` che imposta `location.href = "mailto:…"` o
   uno schema custom **non** deve raggiungere l'opener di sistema — asserito con
   l'opener iniettato, non a occhio.
   Nota residua da valutare qui: lato browser il non-http umano è permesso solo
   per `WKNavigationType.linkActivated`; va verificato che un click sintetico
   prodotto da JS non venga classificato `.linkActivated`, altrimenti la
   distinzione umano/agente cade proprio nel caso che deve fermare.

**Verifica**
```bash
cd Packages/TillerBrowser && swift test
```
E2E via socket contro un server **locale** di fixture (nessuna rete esterna,
D21): pagina con form → `snapshot` → `fill e1` → `click e2` → `wait --text` →
`get text` mostra il risultato. Più: un `snapshot` dopo navigazione invalida i
ref vecchi (`stale_ref`); una pagina con CSP restrittiva produce `js_error` con
`hint` non vuoto.

**STOP** se il JS di snapshot funziona solo su pagine di fixture e fallisce su
un `localhost` reale: riferire l'errore esatto, non allargare il JS a tentativi.

## Fase 7 — store per worktree, gating per origine, badge

**Obiettivo**: sessioni compartimentate e capacità sensibili gated.

**Passi**
1. `WKWebsiteDataStore` persistente **per worktree** (D8), directory dedicata
   sotto Application Support.
2. `OriginPolicy` in `TillerBrowser` (puro, zero WebKit): classifica
   `localhost`/`127.0.0.1`/`::1`/`*.local` come locali → `eval`/`cookies`/`storage`
   liberi; ogni altra origine richiede una concessione `(worktree, origine)`.
3. Concessione: prompt utente alla prima invocazione, memorizzata, revocabile
   dalle Settings. Diniego o assenza → `origin_denied`, nessun fallback.
4. Badge "agent driving" nel chrome del pane mentre un comando socket è in
   volo (D13).

**Verifica**
```bash
cd Packages/TillerBrowser && swift test   # OriginPolicy: tabella origini → decisione
```
Manuale: `eval` su `localhost` passa senza prompt; `eval` su un dominio esterno
chiede conferma una volta e poi ricorda; revoca dalle Settings ripristina il
prompt; il badge appare durante un comando agente e scompare dopo.

**STOP** se il prompt di conferma può essere aggirato da un percorso di codice
(es. `browser.act` che finisce in `evaluateJavaScript` senza passare da
`OriginPolicy`): è un difetto di sicurezza, fermarsi e riferire.

## Fase 8 — budget risorse e chiusura

**Obiettivo**: la feature non peggiora la memoria e il gate non peggiora.

**Passi**
1. Tetto **sui soli nascosti** (D10): max 3 surface nascoste vive; la 4ª si
   smonta e ricarica per URL al ritorno. **Le visibili non si smontano mai.**
2. Sospensione delle nascoste rimaste vive (stop rendering).
3. Skill/doc per gli agenti: un file che elenca i verbi, il ciclo dei ref e la
   tabella `not_supported`, modellato su `skills/cmux-browser/SKILL.md`.

**Verifica**
- Misura memoria: 5 tab browser di cui 2 visibili → surface vive ≤ 5, nessuna
  visibile smontata; confronto IOSurface prima/dopo con la metodologia già
  documentata (gate su bundle id `dev.tiller.Tiller`, `log show --signpost`,
  A/B interlacciato).
- Gate finale (**solo orchestratore**): `Scripts/ci.sh` deve stampare `CI OK`
  oppure fallire **esattamente** sulle 4 suite della baseline rossa del
  2026-08-06, non su altre. Per scagionare il diff: due run sullo stesso albero,
  non HEAD-vs-mio.

**STOP** se compare una quinta suite rossa.

## Checklist manuale finale

1. `⌘⇧L` apre un browser nel pane attivo; `⌘L` mette il fuoco sull'URL.
2. Menu di split → "New Browser" crea un browser **in split**.
3. Trascinare un tab browser in un altro pane funziona; il processo resta vivo.
4. `cmd+click` su `http://localhost:PORT` in un terminale apre dentro; un
   secondo URL riusa lo stesso tab; `cmd+shift+click` apre Safari.
5. `cmd+click` su un link in chat si comporta come al punto 4.
6. `cmd+click` su un `.md` continua ad aprire l'editor.
7. Titolo tab = titolo pagina, con favicon; nessun rename automatico.
8. Riavvio dell'app: il tab browser torna sull'URL giusto.
9. Split con 4 browser visibili: nessuno si smonta.
10. `tillerctl browser open` da un terminale dentro Tiller crea la surface nel
    worktree di quel terminale.
11. Loop completo agente su un dev server locale: snapshot, fill, click, wait,
    get text.
12. `eval` su dominio esterno chiede conferma una volta; su localhost no.
13. Badge "agent driving" visibile durante i comandi agente.
14. Un verbo non supportato (es. `download`) risponde `not_supported`.
15. Nessuna stringa UI in italiano nelle viste nuove.
