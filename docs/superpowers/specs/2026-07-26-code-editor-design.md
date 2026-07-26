# Editor di codice con CodeEditSourceEditor — Design

Data: 2026-07-26
Stato: approvato dall'utente (approccio A), in attesa di review spec

## Obiettivo

Introdurre in Tiller un editor di codice vero (view + edit) basato su
CodeEditSourceEditor (l'editor SwiftUI costruito su CodeEditTextView +
CodeEditLanguages, org CodeEditApp, MIT) e unificarne il motore di
syntax highlighting tree-sitter su tre superfici:

1. **Diff** (right panel `GitDiffView` **e** card tool-call in chat):
   tutto il codice del diff formattato e parsato correttamente come codice.
2. **Tab editor**: click su un file → nuova tab per visualizzazione e modifica.
3. **Chat**: sostituire Highlightr con highlighting tree-sitter headless,
   se lo spike (M0) dimostra che è fattibile in modo pulito.

## Decisioni utente (confermate)

- Scope diff: **entrambe** le superfici (right panel + card chat).
- File apribili: **tutti i file di testo**; `.md` continua ad aprire
  l'editor Markdown esistente (toolbar/preview).
- Entry points: righe Git status, vista diff, card modifica in chat,
  terminale cmd+click.
- Git status: **click = diff (comportamento odierno invariato)**;
  l'apertura editor avviene tramite azione dedicata sulla riga.
- Chat: **spike headless**; se non fattibile, Highlightr resta solo per la chat.

## 1. Architettura & package

- Nuova dipendenza SPM in `project.yml`: **CodeEditSourceEditor**
  (porta con sé CodeEditTextView + CodeEditLanguages — quest'ultimo include
  `CodeLanguagesContainer.xcframework` con tutte le grammatiche tree-sitter).
  Solo il target `App/` la usa direttamente (per la view `SourceEditor`).
- Nuovo package leaf **`Packages/TillerCode`**
  (dipendenze: TillerCore, CodeEditLanguages, CodeEditTextView):
  - `CodeDocument` — documento file-backed (vedi §2)
  - `CodeLanguageResolver` — estensione/path → `CodeLanguage`, fallback plain
  - `TreeSitterHighlighter` — highlighter headless (vedi §5)
  - `DiffHighlighter` — slicing per riga per i diff (vedi §5)
- TillerCore resta senza dipendenze pesanti. Il nuovo package è testabile
  con `swift test` fuori da Xcode, rispettando i confini di dipendenza
  (niente sotto `App/` dipende all'indietro).
- Dopo aggiunta file/target: `xcodegen generate` (mai editare
  `Tiller.xcodeproj` a mano).

## 2. Modello: tab & documento

- Nuovo caso `TabContent.code(fileURL: URL)` in `WorkspaceTab`
  (TillerCore): `leafIds` / `activityPaneIds` vuoti (come `.markdown`),
  accessor `codeFileURL`. `nextShellTitle` invariato.
- **`CodeDocument`** in TillerCode (`@Observable final class`): stesso
  contratto di `MarkdownDocument` —
  `text: String`, `isDirty`, `externalChangeConflict`, `fileDeleted`,
  `reloadFromDisk()`, `keepLocalBuffer()`, `save()`, `stopWatching()` —
  riusando `FileSystemEventMonitor` (FSEvent) di TillerCore.
  Duplicato controllato del pattern, niente astrazione forzata (YAGNI).
- **Persistenza**: estendere il record `ProjectStore` (oggi gestisce già
  `.markdown`) anche per `.code`, con restore di sessione identico.
  Tab ripristinata con file sparito → banner "file eliminato" (come markdown).
- `CodeLanguageResolver`: usa la detection di CodeEditLanguages da
  estensione/URL; sconosciuto → plain text.

## 3. View: `CodeEditorTabView` (App/CodeEditor/)

- Chrome identico al markdown editor: header con path (troncato al centro)
  - badge lingua; banner conflitto ("Ricarica" / "Mantieni"); banner file
  eliminato ("⌘S lo ricrea").
- Corpo: `SourceEditor` SwiftUI con `$document.text`,
  `SourceEditorConfiguration` con `EditorTheme` derivato da `AppTheme`
  (dark/light, warm graphite), font monospaced, editing sempre attivo.
- Guard dimensione: oltre ~2 MB la syntax highlight si disattiva,
  l'editing resta attivo.
- Dispatch in `ContentView.swift` (nuovo `case .code`).
- Dirty dot in `TabBarView` / `SidebarView`: stessa logica markdown
  (`codeDocuments[tab.id]?.isDirty`).
- `⌘S` generalizzato: salva il documento della tab attiva (markdown *o*
  code). Alert dirty-close (`resolveDirtyClose`) riusato/esteso.

## 4. Entry points — funnel unico `openFileTab(fileURL:in:)`

`AppModel.openMarkdownTab` viene generalizzato in `openFileTab`:
`.md`/`.markdown` → tab `.markdown`; altri file UTF-8 → tab `.code`;
binari/non-testo → `NSWorkspace.shared.open` (come oggi).
Dedup per `fileURL` standardizzato (un file già aperto attiva la tab esistente).

| Superficie | Comportamento |
| --- | --- |
| **Git status rows** | Click = diff nel right panel (**invariato**). Nuova azione dedicata sulla riga (icona "Apri in editor" + voce context menu) → `openFileTab`. |
| **GitDiffView** | Nuovo pulsante "Apri in editor" nella toolbar (accanto a refresh) per il file selezionato. |
| **Card chat** (EditSummaryCardView / ToolCallCardView) | Click = diff (**invariato**, coerente con Git status). Nuova azione dedicata sulla riga file (icona "Apri in editor") → `openFileTab`. |
| **Terminale cmd+click** | `MarkdownFileLink.resolve` generalizzato in `FileLink.resolve` (stesso parsing `path[:linea]`), poi funnel `openFileTab`. URL non-file → sistema. |

## 5. Diff highlighting (entrambe le superfici)

- **`TreeSitterHighlighter`** (spike M0, in TillerCode):
  `highlight(code: String, language: CodeLanguage, theme: CodeHighlightTheme) -> NSAttributedString?`
  completamente headless (nessuna view on-screen).
  `CodeHighlightTheme` è un value type di TillerCode (mappa capture→colore),
  costruito a partire da `AppTheme`/`EditorTheme` nel layer App: così
  TillerCode non dipende da CodeEditSourceEditor. La firma esatta è un
  deliverable di M0.
  Candidati implementativi: stack TextKit off-screen + `TreeSitterClient`
  di CodeEditTextView, oppure query tree-sitter dirette via
  CodeLanguagesContainer + mappa capture→colore.
  Criterio di successo dello spike: attributi colore corretti su fixture
  multilingua (swift/py/js/json), nessuna dipendenza da view.
- **`DiffHighlighter`**: highlight full-doc una sola volta per versione
  (old e new), poi slice `NSAttributedString` per riga:
  righe addition/context dal new doc, deletion dall'old doc.
  Righe oltre EOF → plain. Cache per (path, hash contenuto), esecuzione
  off-main, cap ~500 KB oltre → plain (pattern `maxHighlightableLength`).
- `GitDiffView` (`UnifiedDiffPane.lineRow`): `Text(AttributedString)`
  sliced; sfondi/gutter/numeri diff invariati, colori syntax solo nel testo.
- `ToolCallCardView.diffView`: stesso slicing su oldText/newText,
  lingua dall'estensione del path del tool call.

## 6. Chat: sostituzione Highlightr (condizionata a M0)

- `CodeHighlighter` diventa **facade con API invariata**
  (`highlight(code:language:isDark:) -> NSAttributedString?`): internamente
  usa `TreeSitterHighlighter` con `CodeHighlightTheme` derivato da `AppTheme`.
  Alias fence markdown → `CodeLanguage` (js→javascript, ts→typescript,
  py→python, sh→bash, …). Cache LRU esistente riusata.
- Se M0 riesce: **Highlightr rimosso** da `project.yml`.
- Se M0 fallisce: Highlightr resta solo per la chat; il diff usa comunque
  tree-sitter (può girare off-main senza vincoli di streaming).
  Decisione documentata alla fine di M0.

## 7. Error handling

- File illeggibile/codifica non UTF-8 → `lastError` esistente + fallback
  apertura sistema.
- Highlight failure (lingua mancante, errore parser) → plain text
  silenzioso, mai errori a video (pattern attuale di `CodeHighlighter`).
- Conflitto su disco / file eliminato → banner esistenti
  (ricarica/mantieni, ⌘S ricrea).
- File oltre soglia → highlight off, editing on.

## 8. Testing (swift-testing, mai XCTest)

- **TillerCode**: `CodeDocument` (load/save/dirty/conflitto su file temp),
  `CodeLanguageResolver` (estensioni note, fallback),
  `TreeSitterHighlighter` (fixture swift/py/js/json → attributi attesi;
  sconosciuto → nil), `DiffHighlighter` (slicing add/context dal new doc,
  del dall'old doc; diff oltre EOF; cap dimensione).
- **TillerCore**: `WorkspaceTab.code` (leafIds/activityPaneIds) e
  round-trip persistenza `ProjectStore` per `.code`.
- Gate finale obbligatorio: `Scripts/ci.sh` → "CI OK".

## 9. Milestones

- **M0** — Spike `TreeSitterHighlighter` headless (timeboxed).
  Output: demo su fixture + decisione chat (Highlightr sì/no).
- **M1** — Editor tab + entry points (indipendente da M0; usa
  `SourceEditor`, funziona anche se lo spike fallisce).
- **M2** — Diff highlighting su right panel + card chat (dipende da M0).
- **M3** — Facade chat + rimozione Highlightr (solo se M0 ok).

## Fuori scope

- Minimap, code completion, find panel avanzato di SourceEditor
  (disponibili ma non attivati in questa iterazione).
- Side-by-side diff rendering (esiste `GitDiffSideBySide` ma la view
  resta unified).
- Editor embedded per code block nella chat (scartato: rischio
  performance su transcript lunghi).
- Modifica dell'input editor della chat (`ChatTextEditor`).
