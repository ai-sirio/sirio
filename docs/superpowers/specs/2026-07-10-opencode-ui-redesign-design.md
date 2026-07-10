# Redesign UI stile opencode — Design

**Data:** 2026-07-10
**Stato:** approvato

## Obiettivo

Avvicinare l'aspetto di Tiller allo screenshot di riferimento (opencode: UI
dark, monospace, chrome minimale). Non è un redesign da zero — la sidebar di
Tiller (`AppTheme.swift`, `SidebarView.swift`) è già flat/dark e vicina al
riferimento. Il gap sta in: typography (sans vs mono), assenza di una top
bar sopra il workspace, e footer che mostra solo usage provider senza
contesto worktree/path.

Fuori scope (deciso durante il brainstorm): breadcrumb testuale in top bar,
wordmark/empty-state "ask anything" (Tiller non ha chat, è terminal-based),
badge di stato live per i permessi, agent picker in top bar (nessun
equivalente pulito — gli agent in Tiller sono pannelli terminale lanciati
dal `+` della tab bar, non un selettore modello per una chat unica).

## 1. Typography — `AppFont`

Nuovo file `App/AppFont.swift`, stesso pattern di `AppTheme.swift`:

```swift
enum AppFont {
    static func system(size: CGFloat, weight: Font.Weight = .regular) -> Font {
        .system(size: size, weight: weight, design: .monospaced)
    }
}
```

SF Mono (font di sistema, nessuna dipendenza/bundling). Perché non un
modifier `.environment(\.font:)` globale: SwiftUI ignora l'environment font
su qualunque view che specifica già un `.font(.system(size:))` esplicito —
e quasi tutte le view di Tiller lo fanno (`SidebarView`, `TabBarView`,
`UsageBarView`, ecc.). Serve quindi un find-replace meccanico dei call site
esistenti (`.font(.system(size:` → `.font(AppFont.system(size:`), ma
centralizzato in un solo punto — coerente con come il progetto già gestisce
i colori via `AppTheme` invece di sparpagliare `Color(red:green:blue:)`
ovunque.

Attenzione ai call site che già specificano `design: .monospaced` (es.
`WorktreeRow.relativeAge` in `SidebarView.swift:304`) — non applicare
`AppFont` lì, sarebbe ridondante ma non-conflittuale (stesso design).

## 2. Top bar

Nuovo `App/TopBarView.swift`, inserito in `ContentView.workspaceView` sopra
`NavigationSplitView`, sotto la titlebar trasparente (`WindowChromeConfigurator`).
Stessa superficie continua di `AppTheme.background` — nessun bordo/divider
verso la titlebar, stesso trattamento "trasparente" già applicato oggi a
titlebar+sidebar+detail.

Contenuto: **solo icone a destra**, nessuna breadcrumb testuale (la sidebar
mostra già progetto/branch).

- **Split terminale** (`square.split.1x2`) — chiama
  `model.splitCurrent(.horizontal)`, già esistente (`AppModel.swift:561`) e
  già usata altrove: risolve da sola worktree/tab/pane attivi e non fa
  nulla se manca uno dei tre (no-op sicuro, oggi raggiungibile solo da
  tasto destro via `TerminalContextMenuProvider`). Nessuna nuova logica di
  resolution: il bottone in top bar è solo un secondo entry point sulla
  funzione esistente.
- **Permessi** (`lock.shield`, stesso symbol di
  `SettingsCategory.permissions`) — `model.settingsCategory = .permissions;
  model.openSettings()`. Solo scorciatoia di navigazione, nessun badge di
  stato live (deciso esplicitamente: niente dipendenza da `PermissionsModel`
  fuori dalla pagina Settings).

Nessuna modifica a `SidebarView`: gear Settings, readme/help e "Nuovo
Worktree…" restano dove sono oggi. Non vengono duplicati né spostati.

## 3. Footer — estensione di `UsageBarView`

Non una nuova view: `UsageBarView.swift` guadagna, a sinistra dei segmenti
provider esistenti, branch + path del worktree selezionato
(`model.selectedWorktree`). Stessa condizione di visibilità già esistente
(`showUsageBar`/nessun worktree → bar nascosta, nessuna nuova logica).
Nessun nuovo model: dato già disponibile su `AppModel`.

## 4. Empty state e palette colori

`ContentUnavailableView` ("No worktree selected") invariato — nessuna
wordmark, nessuna CTA nuova. Eredita il font mono passivamente tramite gli
stessi call site circostanti dopo il find-replace di `AppFont`, nessun
lavoro dedicato.

Palette (`AppTheme` / `AppSurfaceColor`, dark navy `#1A1C23`) invariata: già
vicina al riferimento (nero/dark navy), nessuna richiesta di modifica.

## Testing

Pattern esistente del repo: logica pura testabile dove possibile (es. in
`Packages/TillerCore` se la resolution del pane da splittare richiede
logica extraibile), test Swift Testing per:

- `AppFont.system` produce un `Font` con `design: .monospaced` (verifica
  indiretta via rendering o via un test di regressione sui call site
  principali, coerente con come il repo testa oggi la view logic non
  banale).
- Azione permessi in top bar: click → `settingsCategory == .permissions`
  e `route == .settings`.
- Azione split: nessun test nuovo, `splitCurrent` è già coperta/esistente —
  il bottone in top bar la richiama senza logica propria.
- Estensione `UsageBarView`: branch/path mostrati solo con worktree
  selezionato, invariati quando `selectedWorktree == nil`.

App target non ha test target proprio (pattern già noto nel repo) — la
logica non banale va estratta in `TillerCore` dove serve un test diretto.
