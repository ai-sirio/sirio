# Tiller — sidebar in vetro e titlebar nativa — Design

**Data:** 2026-07-10  
**Stato:** design approvato nel brainstorming; in attesa della revisione del file da parte dell’utente.

## Obiettivo

Riallineare il workspace di Tiller alla reference preferita: titlebar e main pane
continui, sidebar distinta ma discreta, gerarchia macOS nativa. La sidebar deve
essere un vero pannello di vetro scuro — traslucido e sfocato — non una seconda
superficie opaca con un colore diverso.

Questa revisione sostituisce le decisioni precedenti che imponevano SF Mono
nell’intera chrome UI e una `TopBarView` separata sopra il workspace. Conserva
il footer usage già esteso con branch e path del worktree selezionato.

## Decisioni approvate

- Gli shortcut **Split terminale** e **Permessi** restano, ma sono integrati
  nella titlebar/toolbar nativa; non esiste una barra orizzontale aggiuntiva
  sotto la titlebar.
- Tutta la chrome UI usa **SF Pro** tramite `Font.system` senza design
  monospaziato.
- La sidebar usa un **materiale nativo macOS** a effetto vetro leggero.
- Main pane e terminale restano dark navy opachi.
- Il divario visivo resta limitato a un divisore verticale a basso contrasto e
  al materiale della sidebar; non vengono introdotti bordi pesanti o pannelli
  chat-style.

## Architettura delle view

### `ContentView`

`ContentView.workspaceView` torna a comporre direttamente il
`NavigationSplitView`: `TopBarView` viene rimosso dalla gerarchia. Gli shortcut
sono forniti da un `ToolbarItemGroup` della titlebar, allineato a destra e
presenti solo nel workspace.

Le azioni conservano gli entry point esistenti:

- Split chiama `model.splitCurrent(.horizontal)`.
- Permessi assegna `model.settingsCategory = .permissions`, poi chiama
  `model.openSettings()`.

Non viene creata alcuna nuova logica in `AppModel`.

### `SidebarMaterialContainer`

Un solo contenitore SwiftUI/AppKit circoscrive la sidebar del workspace. Espone
un `NSVisualEffectView` con materiale `.sidebar`, blending `.withinWindow` e
stato attivo/inattivo gestito dal sistema. Il materiale è dark, traslucido e
sfocato; è applicato una volta all’intera colonna e ritagliato al suo confine.

Il bridge non conosce progetti, worktree, selezioni o azioni. Non alloca né
applica blur per righe individuali.

### `SidebarView`

`SidebarView` mantiene filtro, elenco progetti/worktree, context menu, footer
Settings/Help e toolbar “Add Project”. Per lasciare visibile il materiale del
contenitore, non dipinge più `AppTheme.background` come fill opaco alla radice.

Restano invariati:

- larghezza sidebar (`min 200`, `ideal 240`, `max 400`);
- filtro e suo bordo tenue;
- divider del footer laterale;
- hover, selezione, accessibilità e context menu;
- dati e animazioni esistenti.

Le palette di testo, hover e selezione sono verificate sul materiale scuro per
mantenere contrasto leggibile.

### Tipografia

`AppFont` viene eliminato perché esiste esclusivamente per forzare SF Mono e
sarebbe un wrapper senza semantica dopo questa revisione. Tutti i call site UI
migrati tornano a `Font.system` nativo, con gli stessi size e weight correnti.
Il renderer del terminale non viene toccato.

### Rimozioni mirate

- `App/TopBarView.swift` viene eliminato.
- `App/AppFont.swift` viene eliminato.
- Nessun alias, shim o percorso deprecato rimane nel progetto.

## Flusso e stati

Il materiale della sidebar non introduce stato applicativo o persistenza. Si
adatta automaticamente agli stati attivo/inattivo della finestra. Il workspace
continua a usare `AppModel` come unica fonte per worktree, tab, route e usage.

- Senza worktree aperti, l’empty state resta invariato.
- Con worktree selezionato, tab strip, terminale e footer branch/path restano
  invariati.
- In Settings, la route esistente resta separata: il materiale riguarda solo la
  colonna sidebar del workspace.
- Drag/drop Markdown, filtro e resize della split view conservano il
  comportamento attuale.

## Accessibilità e dettagli visivi

- Split e Permessi mantengono label accessibili e help text nella toolbar.
- Nessun significato viene veicolato esclusivamente dal colore.
- Il divisore tra sidebar e detail è sottile e a basso contrasto.
- Il materiale non deve rendere leggibile il contenuto del terminale attraverso
  la sidebar; deve comunicare profondità, non sovrapposizione.

## Verifica

Il target App non ha un test target dedicato. La verifica non inventa coverage
per cablaggi SwiftUI:

1. Eseguire `./Scripts/ci.sh`: build dell’App e test di tutti i package.
2. Lanciare la build e verificare manualmente:
   - titlebar continua, senza barra orizzontale separata;
   - shortcut Split e Permessi presenti nella toolbar nativa e azionabili;
   - sidebar in vetro scuro, traslucida, con blur contenuto alla colonna;
   - testo, filtro, hover e selezione leggibili;
   - main pane e terminale opachi;
   - footer usage, branch e path invariati con worktree selezionato;
   - empty state invariato senza worktree.
3. Ripetere il controllo con finestra ridimensionata e non attiva per escludere
   bleed del materiale o perdita di contrasto.

## Fuori scope

- Breadcrumb testuale o wordmark nella titlebar.
- Empty state chat-style.
- Badge live dei permessi.
- Agent picker nella titlebar.
- Nuove route, stato, persistenza, networking o dipendenze.
- Cambiamenti a progetti/worktree, alla semantica del terminale o al footer
  usage oltre al comportamento già presente.
