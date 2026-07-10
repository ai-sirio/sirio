# Markdown Editor — Design

**Data:** 2026-07-10
**Stato:** approvato in brainstorming, in attesa di review scritta

## Obiettivo

Editor/lettore markdown integrato in Tiller, apribile come tab dentro un worktree
(stesso modello delle tab terminale, ispirato a Orca). Preview renderizzata
(MarkdownUI) + modalità codice (testo puro), toggle tra le due. Solo apertura di
file esistenti; niente creazione di nuovi file in v1.

## Decisioni prese

| Tema | Decisione |
|------|-----------|
| Ingressi apertura | Cmd+click su URL file nel terminale + drag & drop + menu "Apri file..." (`⌘O`) |
| Editing | Due modalità separate: codice (testo markdown puro) e preview (MarkdownUI read-only). Niente WYSIWYG. |
| Salvataggio | Manuale con `⌘S`, indicatore dirty sul tab |
| Nuovi file | No — solo file esistenti |
| Modifiche esterne | Watcher sul file: auto-reload se buffer pulito, banner "Ricarica / Mantieni" se dirty |
| Toolbar | Essenziale, solo in modalità codice: bold, italic, H1-H3, lista, link |
| Modalità default | Preview (caso d'uso primario: leggere ciò che l'agente ha scritto) |
| Architettura tab | Contenuto tab polimorfico (enum), stessa lista tab dei terminali |

## 1. Modello dati e persistenza

`TerminalTab` (TillerCore) diventa `WorkspaceTab` con contenuto polimorfico:

```swift
public enum TabContent: Equatable, Sendable {
    case terminal(SplitTree)
    case markdown(fileURL: URL)
}

public struct WorkspaceTab: Identifiable, Equatable, Sendable {
    public let id: UUID
    public var title: String        // markdown: nome file, es. "README.md"
    public var content: TabContent
}
```

- Rinomina `TerminalTab` → `WorkspaceTab` in tutto il codebase (refactor meccanico,
  guidato dagli errori di compilazione sul `switch` esaustivo).
- Helper `var tree: SplitTree?` per i punti che oggi accedono direttamente all'albero.
- I punti che assumono terminale (attention sort, `leafIds`, agent badge, scrollback
  persistence) trattano le tab markdown come "zero pane": `leafIds` → `[]`,
  nessuna attenzione, nessun agente.
- **Persistenza:** migration GRDB additiva su `terminalTab`:
  `kind TEXT NOT NULL DEFAULT 'terminal'`, `filePath TEXT NULL`.
  Tab markdown: `kind='markdown'`, `filePath` assoluto, `treeJSON` vuoto.
  Righe esistenti restano valide senza migrazione dati.
- Restore sessione: se il file non esiste più, la tab viene scartata in silenzio.
- Titolo tab = nome file + pallino dirty (stato runtime, non persistito: al riavvio
  il contenuto non salvato è perso, coerente con il salvataggio manuale).

## 2. Componenti editor

Logica in **TillerCore** (testabile nel package), view in **App/MarkdownEditor/**.

### MarkdownDocument (TillerCore)

`@Observable @MainActor`, uno per tab aperta, tenuto in un registry keyed su tabId
(stesso pattern di `PaneRegistry`): sopravvive al cambio tab, rimosso alla chiusura.

```swift
@Observable @MainActor
final class MarkdownDocument {
    let fileURL: URL
    var text: String                  // buffer di editing
    private var savedText: String
    var isDirty: Bool { text != savedText }
    var externalChangeConflict: Bool  // banner "modificato su disco"

    func save() throws                // write atomico, aggiorna savedText
}
```

- Watcher: `DispatchSource.makeFileSystemObjectSource` sul fd, eventi `.write`,
  `.delete`, `.rename`. Write atomici (agenti, git checkout) sostituiscono il file:
  su `.delete`/`.rename` il fd va riaperto sul nuovo inode.
- Evento esterno: buffer pulito → reload automatico; buffer dirty →
  `externalChangeConflict = true` (banner).
- Guard anti-eco: dopo `save()` il watcher scatta sul nostro stesso write —
  se contenuto disco == `savedText`, l'evento viene ignorato.

### MarkdownEditorTabView (App/)

- Header: path del file + toggle preview/codice (icone `<>` / matita, stile Orca).
- Preview (default): `Markdown(document.text)` di MarkdownUI in `ScrollView`,
  tema adattato ad `AppTheme`.
- Codice: `TextEditor(text:selection:)` monospace — macOS 15 espone `TextSelection`,
  nessun `NSViewRepresentable` necessario.
- Banner conflitto: "File modificato su disco — Ricarica / Mantieni".

### MarkdownToolbar (App/)

Visibile solo in modalità codice: bold, italic, H1-H3, lista, link.
Ogni bottone avvolge/inserisce sintassi markdown sulla selezione corrente via
binding `TextSelection`. Le trasformazioni di testo sono funzioni pure in
TillerCore (`wrapSelection`, `insertHeading`, ...).

### Integrazione

- Dipendenza SPM: `MarkdownUI` (gonzalezreal/swift-markdown-ui) nel target app
  via `project.yml`.
- Renderer tab: `switch tab.content` → terminale come oggi, markdown →
  `MarkdownEditorTabView`.
- `⌘S` nei commands di `TillerApp` → salva il documento della tab attiva
  (attivo solo se la tab è markdown).
- `⌘W` chiude la tab come oggi; se dirty, alert "Salva / Non salvare / Annulla".

## 3. Punti di apertura

Funnel unico: `AppModel.openMarkdownTab(fileURL:in: worktree)`.
Dedup: tab già aperta per lo stesso file nel worktree → viene attivata, niente
duplicati. Estensioni riconosciute: `.md`, `.markdown` (case-insensitive).

1. **Cmd+click nel terminale.** Si implementa `TerminalSurfaceOpenURLDelegate`
   (già esposto da libghostty-spm, oggi inutilizzato) sul pane.
   In `terminalDidRequestOpenURL(url:kind:)`:
   - `file://` con estensione markdown → editor tab nel worktree del pane;
   - path relativi risolti contro il path del worktree;
   - tutto il resto → `NSWorkspace.shared.open`.

   **Limite noto:** ghostty riconosce come link solo URL (http, file://) — testo
   nudo tipo `README.md` nell'output non è cliccabile. È un limite di ghostty core
   (regex hardcoded), non aggirabile dal wrapper. Coperto da drag & drop e menu.

2. **Drag & drop.** `onDrop` di `.fileURL` sull'area contenuto: file markdown →
   tab editor nel worktree selezionato. Nessun worktree selezionato → drop ignorato.

3. **Menu "Apri file..."** (`⌘O`): `NSOpenPanel` filtrato su tipi markdown, apre
   nel worktree selezionato. Voce disabilitata senza worktree selezionato.

## 4. Gestione errori

- Caricamento fallito (permessi, file sparito) → alert, tab non creata.
- `⌘S` fallito → alert con errore, buffer intatto, riprova possibile.
- File cancellato da fuori mentre aperto → banner "File eliminato su disco",
  buffer resta, `⌘S` lo ricrea.
- Restore con file mancante → tab scartata in silenzio.
- File grandi: soft limit ~2MB — oltre, apertura diretta in modalità codice senza
  preview automatica (MarkdownUI rallenta su documenti enormi), avviso nell'header.

## 5. Testing

- Unit test TillerCore su `MarkdownDocument` (temp dir): dirty tracking, save
  atomico, guard anti-eco, reload esterno vs conflitto, delete/rename del file.
- Unit test encode/decode `WorkspaceTab` + record persistenza con `kind`/`filePath`.
- Unit test funzioni toolbar: selezione vuota / piena / multiriga.
- Verifica manuale: i 3 canali di apertura, toggle preview/codice, restore sessione.

## Fuori scope (v1)

- Creazione nuovi file markdown.
- WYSIWYG / editing nella vista renderizzata.
- Syntax highlighting nella modalità codice.
- Link detection di path nudi nel terminale (limite ghostty upstream).
- File non-markdown (nessun editor generico: Tiller non è un IDE).
