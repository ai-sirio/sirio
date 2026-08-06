# Activity panel — sostituzione della sezione Agents

## Context

La sezione in fondo al pannello destro si chiama "Agents" e mostra solo le tab in cui è stato *riconosciuto* un agente: `AgentTreeBuilder.build` scarta ogni tab priva sia di `agentStatus` sia di `paneAgents` (`Packages/TillerCore/Sources/TillerCore/AgentTree.swift`). Un terminale con una shell nuda è quindi invisibile, e la sezione risponde a "quali agenti stanno girando qui?" invece che a "cosa ho aperto?".

Inoltre è limitata al worktree selezionato, non offre modo di chiudere nulla, e occupa un 25% fisso dell'altezza del pannello (`RightPanelView.swift:29`) che non si può recuperare.

Obiettivo: **Activity** — lista piatta di ogni tab terminale e chat su tutti i worktree aperti, ogni riga chiudibile, sezione collassabile.

Spec approvata: `docs/superpowers/specs/2026-08-06-activity-panel-design.md` (commit 2ad68ab).
Piano dettagliato passo-passo, con tutto il codice: `docs/superpowers/plans/2026-08-06-activity-panel.md`.

## Decisioni prese

| Domanda | Decisione |
|---|---|
| Cosa conta come processo | Ogni tab terminale e chat, con o senza agente |
| Ambito | Tutti i worktree in `openWorktreeIds`, non solo il selezionato |
| Struttura | Lista piatta, ordinata per worktree, etichetta `progetto/branch` per riga |
| Righe figlie (subagent) | Rimosse |
| Chiusura | Immediata se idle/done; conferma se running/needs-input/error |
| Pulsante | Chevron nell'header della sezione, stato persistito |

Scartato l'undo toast: chiudere una tab terminale termina il PTY, quindi l'undo vero è impossibile — resterebbe un "riapri". Con zero infrastruttura toast generica (`UpdateToastView` è cablato su Sparkle) il costo era sproporzionato rispetto a una conferma che previene lo stesso errore.

Scartato il pulsante in titlebar: il pannello destro ha già il suo toggle (⌃⌘I), e un secondo bottone di chrome per una sotto-sezione di un pannello nascondibile dovrebbe forzarne l'apertura per fare qualcosa di visibile.

## Approccio

Builder puro in TillerCore, adattatore sottile nel layer App, vista che rende. La logica subagent (~50 righe: `subagentNodes`, `firstMatch`, `collectMatches`) sparisce invece di marcire.

`AgentTreeBuilder` ha **un solo consumatore** (`AgentsPanelModel.swift:32`) più i suoi test, quindi si riscrive invece di affiancarlo.

**Attenzione:** `ProcessNode` vive in `AgentTree.swift` ma appartiene al rilevamento Layer-D (`ForegroundProcessAgent.swift:35`, `ProcessScanCoordinator.swift:75`, `AppModel.swift:1995`, più due AppTests). Va estratto in `ProcessNode.swift` **prima** di cancellare il file, o il build si rompe. `ChatSubagentInput` invece muore con il suo unico consumatore.

## File

**Nuovi**
- `Packages/TillerCore/Sources/TillerCore/ActivityStatus.swift` — `.running/.needsInput/.done/.error/.idle`, `from(_ status: AgentStatus?)` con `nil → .idle`, `requiresCloseConfirmation`. Serve perché `AgentStatus` non ha un caso per "non sta girando niente".
- `Packages/TillerCore/Sources/TillerCore/ActivityList.swift` — `ActivityRow` + `ActivityListBuilder.build(worktrees:agentStatus:paneAgents:)`, piatto, documenti esclusi, terminale senza pane vivo saltato.
- `Packages/TillerCore/Sources/TillerCore/ProcessNode.swift` — estrazione, contenuto invariato.
- `App/RightPanel/ActivityPanelModel.swift` — itera `openWorktreeIds`, risolve con `AppModel.worktree(byId:)` (`:82`) e `workspaceCoordinator.layouts`, pane vivi via `liveControlPaneId(contentID:in:)` (`WorkspaceCoordinator.swift:154`).
- `App/RightPanel/ActivitySectionView.swift` — header cliccabile con chevron e conteggio, righe con icona/titolo/etichetta/stato/✕.
- `AppTests/ActivityPanelTests.swift`

**Modificati**
- `Packages/TillerCore/Sources/TillerCore/AppSettings.swift:108` — `activitySectionExpandedKey`, default `true`, accanto alle altre chiavi rightPanel.
- `App/RightPanel/RightPanelView.swift` — `@AppStorage` per l'espansione; `toolsRegion` con `maxHeight` condizionale (75% aperta, `.infinity` chiusa); via il wrapper `if let worktree` e il placeholder "No agents running"; tradotte le due stringhe italiane a `:79-80`.
- `Packages/TillerCore/Tests/TillerCoreTests/AgentActivityModelTests.swift:88` — commento che cita il guard rimosso; il test resta valido.

**Cancellati**
- `App/RightPanel/AgentsSectionView.swift`, `App/RightPanel/AgentsPanelModel.swift`, `Packages/TillerCore/Sources/TillerCore/AgentTree.swift`, `Packages/TillerCore/Tests/TillerCoreTests/AgentTreeBuilderTests.swift`

## Riuso

- `AgentIcon(agentId:)` (`App/AgentIcon.swift:8`) per le righe con agente; SF Symbol `terminal` / `bubble.left.and.text.bubble.right` per quelle senza.
- `RunningDots(color:dotSize:)` (`App/RunningDots.swift:8`) per lo stato running.
- `AppModel.focusTab(tabId:in:)` (`:2012`) e `AppModel.closeTab(_:in:)` (`:1356`) — nessuna logica di chiusura nuova: il coordinator fa già il teardown e termina il PTY.
- Pattern alert `pendingDiscard` già in `RightPanelView.swift:43-58`.
- `UniversalChatFixture` (`AppTests/Workspace/UniversalChatFixture.swift`) per i test App.

## Ordine di esecuzione

Sei task, ognuna con il proprio ciclo test-rosso → implementa → verde → commit. Il codice completo di ogni step è nel piano dettagliato.

1. `ActivityStatus` + test (3 test)
2. `ActivityListBuilder` + test (7 test) — convive col vecchio builder
3. Chiave `AppSettings` + test
4. `ActivityPanelModel` — convive con `AgentsPanelModel`
5. **Switchover atomico**: estrai `ProcessNode`, scrivi la vista, ricabla `RightPanelView`, cancella i quattro file morti. Non splittabile: spezzarla lascia il build rotto fra due commit.
6. AppTests + gate completo

## Verifica

```bash
cd Packages/TillerCore && swift test          # durante le task 1-3
Scripts/ci.sh                                  # gate finale, deve stampare "CI OK"
```

Se falliscono `PtyProcessTests` o `DividerCursorStripTests`: sono flaky noti sotto carico. Per scagionare il proprio diff serve girare `ci.sh` due volte **sullo stesso albero**, non confrontare con `HEAD`.

Dopo `CI OK`, build e run. Checklist manuale — i test automatici non vedono il layout, e il collasso è esattamente il genere di cosa che compila e viene storta:

- [ ] L'header dice "Activity"
- [ ] Un terminale con solo `zsh` compare in lista — è *il* cambiamento, se manca il resto non conta
- [ ] Le chat compaiono col titolo corrente (auto-rename)
- [ ] Un documento markdown/codice aperto **non** compare
- [ ] Con due worktree aperti entrambi contribuiscono, ciascuno con la sua etichetta `progetto/branch`
- [ ] Click su una riga porta il fuoco sulla sua tab, anche in un worktree non selezionato
- [ ] ✕ su riga idle chiude subito
- [ ] ✕ su riga con agente al lavoro chiede conferma; Cancel la lascia viva
- [ ] Collassata, Files/Changes prende tutta l'altezza, e lo stato sopravvive al riavvio
- [ ] Il conteggio nell'header corrisponde alle righe verdi
