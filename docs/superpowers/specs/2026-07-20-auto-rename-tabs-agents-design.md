# Auto-rename tab e agenti — design

Data: 2026-07-20

## Obiettivo

Rinominare automaticamente il titolo di una `WorkspaceTab` (e di conseguenza,
via `AgentTreeBuilder`, il nodo root corrispondente nell'Agents panel) a
partire dal contenuto della conversazione, così che sessioni concorrenti dello
stesso agente non appaiano tutte come "Claude Code" / "Codex" nella tab bar.

Strategia di riferimento: [manaflow-ai/cmux — workspace auto-naming](https://github.com/manaflow-ai/cmux/blob/main/docs/workspace-auto-naming.md).
Cmux rinomina "workspace" (righe sidebar) e tab al termine di un turno agente,
riassumendo la conversazione con la CLI dell'agente stesso in modalità
non interattiva, con precedenza assoluta ai nomi impostati manualmente.

## Perché un solo meccanismo copre "tab" e "agenti"

`AgentTreeBuilder.build` (invocato da `AgentsPanelModel.nodes`,
`App/RightPanel/AgentsPanelModel.swift:19-27`) costruisce il nodo root
dell'Agents panel usando direttamente `tab.title`. Rinominare
`WorkspaceTab.title` propaga automaticamente sia alla tab bar sia al nodo
agente nel pannello destro — non serve un secondo campo "nome agente".

I nodi subagente (task interni, split) hanno già titoli propri derivati dal
tool-call (`ChatSubagentInput.title`) e non sono in scope: cambiano
naturalmente col task in corso.

## Trigger: riuso di `AgentTransition`

Tiller unifica già 4 layer di rilevamento stato (hook nativi, titolo OSC,
contenuto schermo, processo foreground) in un unico `AgentStatus` per pane,
esposto tramite `AgentActivityModel.notify` → `AgentTransition`
(`App/AppModel.swift:585-588`). Il turn-end di un agente è quindi già un
segnale esistente, indipendente dall'adapter:

- Fire quando `transition.old == .running` e
  `transition.new ∈ {.done, .needsInput}`.
- Skip su `.error` — il transcript intorno a un crash raramente produce un
  titolo utile.
- Throttle in-memory per pane (non persistito): min-intervallo (~30s) *e*
  crescita minima del transcript (~200 caratteri) dall'ultimo pass, stesso
  doppio gate di cmux. Uno stato effimero — un riavvio dell'app nel mezzo
  causa al più una chiamata di riassunto in più, non un bug.

Nessun nuovo hook da scrivere: il punto di innesco è un secondo call site
dentro `notifyTransition`, non una nuova sorgente di eventi.

## Sorgente transcript per superficie

### Chat tab (ACP) — tutti e 5 gli agenti

`ChatController` / `TillerACP.TranscriptReducer` tengono già la
conversazione strutturata in memoria per la tab attiva. Nessuna lettura da
disco: si estraggono le ultime N battute e si formattano come testo piano.

### Terminal tab — solo i 3 adapter con hook nativi

Claude Code, Codex e Oh-My-Pi (`hasNativeHooks == true`,
`Packages/TillerAgents/Sources/TillerAgents/*Adapter.swift`) hanno già un
riferimento di sessione persistito per pane
(`AgentSessionRef.sessionRef`, salvato via il control method `notify` /
`session.ref`, `App/AppModel.swift:589-591,599`), catturato per il resume.
Lo stesso riferimento individua il file di transcript su disco:

- **Claude Code**: `~/.claude/projects/<worktree-path-escaped>/<sessionRef>.jsonl`.
- **Codex**: rollout JSONL — il pattern `rollout-<data>-<uuid>.jsonl` e
  l'estrazione dell'id sono già implementati in
  `Packages/TillerControl/Sources/TillerControl/AgentSessionExtractor.swift`;
  serve solo risolvere la directory (glob per id, la data-prefix non è
  salvata separatamente).
- **Oh-My-Pi**: convenzione di transcript su disco non ancora verificata —
  **item aperto**, da chiarire in fase di planning prima di implementare
  questo ramo specifico. Non voglio assumere un path senza verifica.

### Terminal tab — OpenCode e Pi (`hasNativeHooks == false`)

Nessun hook nativo e nessun file di transcript noto e indirizzabile via
`AgentSessionRef`. **Fuori scope per questa iterazione**: il titolo resta
manuale finché non esiste una sorgente affidabile (gap dichiarato, non un
fallback euristico fragile come lo scraping dello scrollback).

## Provenance — i rename manuali vincono sempre

Nuovo campo `titleIsAutoNamed: Bool` su `WorkspaceTab`:

- Tab nuove: default `true` (titolo placeholder, candidato all'auto-naming).
- Rename manuale (`model.renameTab`, già esistente — doppio click sulla tab
  o menu contestuale "Rinomina") → flip permanente a `false`. Da quel
  momento l'auto-naming per quella tab si ferma.
- Migration DB (v11, dopo v10): le tab esistenti al momento dell'upgrade
  vengono "grandfathered" a `false` — mai rinominare automaticamente una tab
  che l'utente sta già guardando.
- Nessuna azione "cancella nome custom → riattiva auto-naming" in v1 (cmux
  ce l'ha; qui è YAGNI finché non richiesta — aggiungibile in seguito senza
  rotture, è solo un altro modo di flippare lo stesso booleano).

## Componenti nuovi

- **`TranscriptSource`** (protocollo, TillerCore o pacchetto nuovo minimale):
  `func recentTurns(paneId:, sessionRef:) -> String?`. Due implementazioni:
  `ChatTranscriptSource` e `FileTranscriptSource`.
- **`AgentAdapter` + 1 metodo**: `func summarizeCommand(prompt: String) -> String?`,
  nil quando l'adapter non ha (ancora) un'invocazione non interattiva sicura
  — stesso concetto di `summarizerSupported` in cmux. Mapping iniziale:
  Claude `claude -p`, Codex `codex exec --output-last-message`, OpenCode
  `opencode run --pure`, Pi `pi --print --no-tools`, Oh-My-Pi
  `omp --print --no-tools`.
- **`AutoNamer`**: orchestratore. Riceve paneId/tab/adapter, risolve la
  `TranscriptSource` giusta, costruisce un prompt di riassunto breve (2-5
  parole, lingua della conversazione), lancia `summarizeCommand` come
  `Process` **detached** (mai sul thread UI, mai bloccante per l'agente),
  parsa l'output e applica il titolo tramite un setter interno che **non**
  passa da `renameTab` (quindi non tocca `titleIsAutoNamed`).

## Gestione errori

Degrado sempre silenzioso, mai un errore visibile per una feature di
contorno:

- Binario dell'adapter assente sul PATH → skip (probe una tantum, cache del
  risultato).
- Timeout (~8-10s) sul processo di riassunto → kill + skip.
- Output vuoto o non parsabile → skip, titolo invariato.
- Tab/pane chiusi prima che il riassunto torni → scarta il risultato (guard
  su presenza in `model.tabs`).

## Settings

Toggle off-by-default in `Settings > Automation`
(`autoNaming.enabled`, stesso pattern di `controlSocket.enabled`). Nessun
picker "usa un altro agente come summarizer" in v1 — si usa sempre l'agente
della sessione stessa (evita di richiedere l'installazione di CLI che
l'utente potrebbe non avere, e riduce la configurabilità non richiesta).

## Test

- Unit: logica di throttle (intervallo + crescita transcript) — funzione
  pura, `swift-testing`.
- Unit: transizioni di provenance (`renameTab` flippa il flag; il path
  auto non lo tocca mai; la migration grandfathera le righe esistenti).
- Unit: risoluzione path Claude/Codex con fixture `AgentSessionRef` +
  fixture JSONL.
- Unit: parsing/troncamento del titolo restituito dal summarizer.
- Nessun nuovo test UI: il titolo attraversa lo stesso `Text(tab.title)` già
  coperto dai test esistenti della tab bar.

## Fuori scope (dichiarato, non nascosto)

- Terminal tab per OpenCode e Pi (nessuna sorgente transcript nota).
- Convenzione transcript Oh-My-Pi da verificare prima di implementare quel
  ramo.
- Comando "clear custom name" per riattivare l'auto-naming su una tab già
  rinominata manualmente.
- Picker per scegliere un summarizer diverso dall'agente della sessione.
