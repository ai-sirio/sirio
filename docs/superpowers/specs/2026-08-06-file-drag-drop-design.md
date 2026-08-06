# Drag & drop di file e immagini — design

Data: 2026-08-06

## Obiettivo

Trascinare file e immagini dentro Tiller: dal Finder e dalla sidebar Files, verso
la chat e verso il terminale. Nessun export verso il Finder o altre app.

Matrice coperta:

| | → Chat | → Terminale |
|---|---|---|
| **Da Finder** | sì | sì |
| **Da Files sidebar** | sì | sì |

## Stato di partenza

- La chat ha già il modello giusto: `ComposerChip` (`.skill` / `.file(path)` /
  `.image(ImageAttachment)`), un chip = un `U+FFFC` nella `NSTextStorage`;
  `ComposerDraft.parse` produce `(text, mentionPaths, images)` per
  `ChatController.send`. Un drop deve solo produrre chip: sotto la UI non cambia
  niente.
- Nessun drop di file esiste oggi. `onDrop` compare solo in `RowReorder.swift` e
  `TabBarView.swift` (riordino tab).
- Il terminale può già ricevere testo su stdin via
  `PaneRegistry.write(paneId:data:)`.
- La sidebar Files ha già l'URL reale di ogni nodo
  (`panelModel.rootURL` + `node.url(relativeTo:)`).

Verifiche fatte in fase di design:

- `libghostty-spm` non registra alcun tipo trascinato (nessun
  `registerForDraggedTypes`, nessun `dragging*` in `Sources/`): un `.onDrop`
  SwiftUI attorno al pane riceve i drop senza conflitti di responder.
- `ChatComposerView` ha un solo call site (`App/Chat/ChatPaneView.swift:96`).
- `PtyTerminalPane` ha due call site, entrambi in `TillerTerminal`
  (`TerminalSurfaceHost.swift:116`, `SplitViewRenderer.swift:178`): il drop
  implementato dentro il pane copre pane singoli e split senza duplicazioni.

## Decisioni

1. **Path fuori dal worktree**: il chip file porta un path assoluto così com'è.
   Nessuna copia di file, nessun rifiuto. `mentionPaths` accetta quindi sia
   relativi (comportamento attuale delle `@`-mention) sia assoluti.
2. **Cosa è un'immagine**: `png`, `jpg`, `jpeg`, `gif`, `webp`, con tetto di
   10 MB sul file grezzo. Oltre il tetto → rifiuto con messaggio, nessun
   fallback silenzioso. Ogni altro tipo → chip file.
3. **Terminale**: inserisce i path assoluti shell-quotati al cursore, separati da
   spazio, senza newline; non esegue mai. Il pane prende il focus al drop. Le
   immagini nel terminale non sono un caso speciale: gli agenti CLI leggono il
   path dal prompt.
4. **Area di drop in chat**: tutto il pane chat, transcript inclusa, con overlay
   `Drop files to attach`.
5. **Cartelle**: in chat diventano un chip file col path della cartella; è
   l'agente a decidere cosa farne. Nel terminale sono un path come gli altri
   (serve per `cd `).
6. **Nessun tetto** sul numero di file droppati insieme; ogni immagine passa
   comunque il proprio check da 10 MB.
7. **Messaggi di rifiuto**: toast.

## Architettura

La logica che decide *cosa è* un file droppato è pura aritmetica su path e byte:
vive in `TillerCore`, non nelle view. Una sola funzione risponde per chat,
terminale e sidebar, quindi non può esistere una destinazione che salta la
regola.

### Nuovo: `Packages/TillerCore/Sources/TillerCore/FileDrop.swift`

```swift
public enum DroppedItem: Equatable, Sendable {
    case image(url: URL, mimeType: String)
    case file(path: String)
    case rejected(url: URL, reason: DropRejection)
}

public enum DropRejection: Equatable, Sendable {
    case imageTooLarge(byteCount: Int)
}

public enum FileDrop {
    public static let maxImageBytes = 10 * 1_024 * 1_024

    public static func classify(_ inputs: [(url: URL, byteCount: Int)],
                                worktreePath: String) -> [DroppedItem]

    public static func terminalInsertion(_ urls: [URL]) -> String
}
```

Estensione → MIME è una tabella (`png`, `jpg`, `jpeg`, `gif`, `webp`) e non
`UTType`: la decisione 2 nomina quattro tipi, e una tabella è verificabile senza
chiedere niente al sistema.

Il confronto sull'estensione è fatto in minuscolo (`.PNG` conta come `.png`).

Ordine delle regole in `classify`:

1. estensione nella tabella **e** `byteCount <= maxImageBytes` → `.image`
2. estensione nella tabella **ma** oltre il tetto → `.rejected(.imageTooLarge)`
3. tutto il resto → `.file(path:)`

Una cartella non ha un ramo dedicato: cade in `.file` perché non ha estensione
immagine. Un ramo in meno è un ramo che non può divergere dalla decisione presa.

`classify` riceve `byteCount` come parametro invece di leggerlo da disco: la
funzione resta pura e i test sulla soglia non devono creare file da 10 MB.
L'I/O (`resourceValues(forKeys: [.fileSizeKey])`) sta nella view, dove è
comunque inevitabile.

`chipPath`: se `url.path` sta sotto `worktreePath` → path relativo; altrimenti
assoluto.

`terminalInsertion`: `urls.map { shellQuote($0.path) }.joined(separator: " ")`,
sempre assoluti, nessun `\n`.

### Modifiche a codice esistente

- `Packages/TillerAgents/Sources/TillerAgents/ShellQuote.swift`: `shellQuote` si
  sposta in `TillerCore` come `public`. `TillerCore` non può importare
  `TillerAgents` (la dipendenza va nel verso opposto), e duplicare la quotatura
  POSIX in due punti è peggio dello spostamento. `TillerAgents` già dipende da
  `TillerCore`: i suoi call site non cambiano.
- `Packages/TillerACP/Sources/TillerACP/ChatPromptBuilder.swift:22`: guardia sul
  path assoluto, altrimenti un file droppato dal Finder produrrebbe
  `file:///worktree/Users/…/foo.png` — un resourceLink morto, in silenzio.

  ```swift
  let url = path.hasPrefix("/") ? URL(fileURLWithPath: path)
                                : URL(fileURLWithPath: worktreePath).appendingPathComponent(path)
  ```

### Chat — `ChatPaneView` + `ChatComposerView`

- `@State private var document = ComposerDocument()` sale da `ChatComposerView` a
  `ChatPaneView`; il composer lo riceve come parametro. Un solo call site da
  aggiornare.
- `.onDrop(of: [.fileURL], isTargeted: $isDropTargeted)` sul `VStack` del pane.
  Handler: risolvi gli URL → leggi `fileSizeKey` → `FileDrop.classify` → per ogni
  item `document.insert(chip, replacing: NSRange(location: document.storage.length, length: 0))`,
  la stessa chiamata già usata da `attachImage`. Le `.image` vengono lette e
  codificate base64 qui.
- Overlay quando `isDropTargeted`: bordo tratteggiato accent + `Drop files to attach`.
- I `.rejected` producono un toast.
- Il drop segue la stessa regola della digitazione: attivo quando il composer è
  interattivo (`canInteract`), ignorato altrimenti — non si accumulano chip in
  un composer bloccato in attesa di un permesso.

L'applicazione degli item al documento è una funzione a sé, non una closure
dentro `.onDrop`: altrimenti non è raggiungibile dai test.

### Terminale — `PtyTerminalPane`

- `.onDrop` sul pane → `FileDrop.terminalInsertion(urls)` →
  `PaneRegistry.write(paneId:data:)`. Nessuna newline: l'utente decide quando
  premere invio.
- Focus: `TerminalSurfaceProxy` tiene già `weak var view: TerminalView?` (una
  `NSView`) accanto a `copySelectionToPasteboard` / `pasteFromPasteboard` /
  `clearScreen`. Si aggiunge un `focus()` nello stesso stile:
  `view.window?.makeFirstResponder(view)`.
- Overlay: bordo accent sopra la superficie Ghostty (dentro Ghostty non si
  disegna).

### Files sidebar — `FileExplorerView.fileRow`

`.draggable(node.url(relativeTo: root))` sulla riga. `URL` è già `Transferable` e
si espone come `.fileURL`: chat e terminale ricevono lo stesso payload del
Finder, quindi esiste un solo formato in ingresso ovunque.

Trascinamento singolo: la sidebar oggi ha `selectedPath: String?` e nessuna
multi-selezione.

### Toast

Non esiste un sistema di toast generico: `App/Updates/UpdateToastView.swift` è
cablato su un `switch` degli stati di `UpdaterModel`. Serve un `TransientMessage`
(`@Observable`, messaggio + scadenza) su `AppModel` e una view che riusa lo
stesso stile (`.regularMaterial`, `AppTheme.hairline`, angolo in basso a destra).

Stringhe in inglese, es. `Image is too large (max 10 MB)`.

## Rischi

**Principale — `NSTextView` si mangia i drop.** `ChatTextEditor` è un
`NSTextView` editabile: di default accetta i file droppati e li inserisce da sé,
come allegato o come testo. Senza disattivarlo, un drop mirato al composer —
cioè dove l'utente mira di più — bypassa `FileDrop` e corrompe la storage dei
chip. Va gestito per primo: azzerare i tipi trascinati registrati dal text view,
oppure intercettare `performDragOperation` e inoltrare all'handler.

`.onDrop` di SwiftUI non "vince" su una `NSView` che ha registrato i tipi: la
`NSView` è più in basso nella catena e risponde per prima.

**Secondario — transcript.** `AgentMarkdownTextView` e `StreamingAgentTextView`
sono `NSTextView` non editabili, che in teoria non registrano tipi trascinati.
Da verificare, non da assumere.

**Secondario — gesture della sidebar.** La riga ha già `.onTapGesture`
(selezione) e `.onTapGesture(count: 2)` sul nome (apri file). Aggiungere
`.draggable` è la classe di bug già incontrata nel progetto: un riconoscitore di
drag che mangia i tap, col sintomo "hover sì, click no". Verifica manuale
obbligatoria.

## Verifica

### Test automatici

`TillerCore` — `FileDropTests`:

- `png` / `jpg` / `jpeg` / `gif` / `webp` sotto soglia → `.image` col MIME giusto
- `png` oltre 10 MB → `.rejected(.imageTooLarge)`
- `.txt` e cartella → `.file`
- file dentro il worktree → path relativo; fuori → path assoluto
- `terminalInsertion`: quota spazi e apostrofi, unisce con spazio singolo,
  nessun `\n`

`TillerACP` — `ChatPromptBuilderTests`:

- mention assoluto → `resourceLink` con quell'URI esatto
- mention relativo → invariato (regressione)

`AppTests`:

- applicare `[png piccolo, txt, png enorme]` al `ComposerDocument` → 2 chip
  (`.image`, `.file`) + 1 messaggio

### Checklist manuale (screenshot a ogni punto)

1. Finder → chat: screenshot PNG → chip immagine → invio → l'agente la vede
2. Finder → terminale: file con spazi nel nome → path quotato corretto
3. Sidebar → chat, sidebar → terminale
4. Regressione gesture sidebar: click singolo seleziona ancora, doppio click apre
   ancora
5. Immagine > 10 MB → toast, nessun chip
6. Cartella → chip file
7. Drop su un terminale splittato: finisce nel pane sotto il cursore

## Fuori scope

- Export drag verso Finder o altre app
- Multi-selezione nella sidebar Files
- Drop *dentro* la sidebar Files (importare file nel worktree)
- Intercettazione di ⌘V (buco noto, documentato a `ChatComposerView.swift:252`)
- Conversione HEIC/TIFF
