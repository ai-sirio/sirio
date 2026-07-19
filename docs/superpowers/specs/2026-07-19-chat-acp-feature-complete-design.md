# Chat ACP — feature complete: terminali, MCP, following, edit review, fix UI

Data: 2026-07-19 · Branch: `feature/chat-interface` · Stato: approvata

## Contesto

La chat ACP di Tiller (spec 2026-07-18) copre già @-mention, immagini, tool
call con permission inline, plan/TODO card e slash command dall'agente.
L'adapter Claude è stato migrato a `@agentclientprotocol/claude-agent-acp@0.59.0`
(successore di `@zed-industries/claude-code-acp`), che espone il set completo
di feature ACP. Questa spec chiude il gap: terminali in chat, client MCP
servers, following, edit review, più due fix UI (font e "3 puntini").

Gap analysis (verificata sul dist del pacchetto 0.59.0):

| Feature | Stato attuale |
|---|---|
| Context @-mentions, immagini, tool call + permessi, plan, slash commands | già presenti |
| Following | parziale: locations cliccabili, nessun auto-follow |
| Edit review | parziale: diff nei card, nessun flusso review/undo |
| Terminali interattivi/background | assente (`terminal: false`) |
| Client MCP servers | assente (`mcpServers: []` fisso) |

## 1. Fix "3 puntini" (bug)

**Root cause.** In `TranscriptReducer` i rami `agentThoughtChunk`,
`userMessageChunk` e la materializzazione di `toolCallUpdate` azzerano
`openAgentMessageIndex` senza marcare il messaggio `isComplete: true`. Un
messaggio agente seguito da un thought — o ogni messaggio durante il replay di
`session/load` — resta incompleto per sempre: `TranscriptView` mostra
`RunningDots` perenne e lo stato sporco viene persistito da `saveTranscript`.

**Fix.**
- Estrarre `closeAgentMessage()` che marca `isComplete: true`; chiamarla in
  ogni punto che azzera `openAgentMessageIndex` (oggi solo `closeOpenStreams`
  lo fa correttamente).
- Al load del transcript persistito (`ChatSessionStore.loadTranscript` o
  `ChatController`), normalizzare gli `agentMessage` a `isComplete: true`: un
  transcript ripristinato è di turni finiti, e questo sana i dati già sporchi
  nel DB senza migrazione.

**Test.** Reducer: `message → thought` chiude il messaggio; sequenza replay
(`userMessageChunk` interleaved) non lascia messaggi aperti; load normalizza.

## 2. Font chat (sistema 13pt)

Le risposte agente usano il tema MarkdownUI `.gitHub` (~16px, stile pagina
web); bolle utente e thought usano font di sistema di taglie diverse.

- Tema MarkdownUI custom `tiller` al posto di `.gitHub`: body = font sistema
  13pt; code inline e blocchi = monospaced 12pt; heading compatti
  (15/14/13 semibold).
- Bolla utente e thought espanso allineati a 13pt.
- Nessuna impostazione configurabile.

## 3. Terminali in chat (card nel transcript) — rivista 2026-07-19

Verifica sul dist di claude-agent-acp 0.59.0: l'agente **non chiama mai** i
metodi ACP standard `terminal/*` sul client — la Claude Agent SDK esegue
Bash internamente e streamma l'output al client via un'estensione `_meta`
in tre step:

1. `tool_call` (Bash) → `_meta.terminal_info {terminal_id}` + content
   `{type: "terminal", terminalId}`
2. `tool_call_update` → `_meta.terminal_output {terminal_id, data}`
   (chunk di output in streaming, anche per task in background)
3. `tool_call_update` → `_meta.terminal_exit {terminal_id, exit_code,
   signal}`

Il client vi aderisce dichiarando `clientCapabilities._meta:
{"terminal_output": true}` nell'`initialize`.

Design (più piccolo e più sicuro dell'originale — Tiller non esegue alcun
processo, riceve solo testo):

- `ClientCapabilities` guadagna il campo `_meta` con `terminal_output: true`.
- Decodifica dei tre `_meta` nelle notifiche `session/update`
  (`tool_call`/`tool_call_update`); `ToolCallItem` guadagna stato terminale:
  `terminalId`, output accumulato, exit status. Il reducer applica i chunk.
- Nuovo case `terminal(terminalId:)` in `ToolCallContent` (decodifica del
  content block; il rendering legge lo stato dal `ToolCallItem`).
- `ToolCallCardView` embedda **`TerminalOutputView`**: output monospace
  (ultime ~50 righe, auto-scroll), chip stato (running / exit code). Nessun
  bottone kill: il processo vive nell'agente, l'interruzione è il cancel del
  turno già esistente.
- Persistenza: gratis — output ed exit vivono nel `ToolCallItem`
  serializzato; nessuno snapshot separato.
- I metodi server-side `terminal/*` (per agenti che li chiamassero davvero)
  restano fuori scope finché un agente supportato non li usa.

## 4. Client MCP servers (`.mcp.json`)

- `ChatController.start` legge `<worktree>/.mcp.json` (convenzione Claude
  Code): `{"mcpServers": {nome: {command, args, env}}}` più varianti
  `type: "http" | "sse"` con `url`/`headers`.
- Nuovo parser puro **`McpConfig`** in TillerACP: file → `[McpServerSpec]`
  nel wire format ACP verificato — stdio: `{name, command, args, env}`;
  http/sse: `{type, name, url, headers: [{name, value}]}`.
- La lista passa in `session/new` e `session/load` (oggi `mcpServers: []`).
- File assente → `[]` (comportamento attuale). JSON invalido → banner warning
  non bloccante in chat, sessione parte senza MCP.

## 5. Following (right panel, toggle off)

- `ChatController` traccia l'ultima `ToolCallLocation` dagli update in
  streaming.
- Toggle "Segui l'agente" nell'header del pane chat: icona, default off, non
  persistito.
- Quando attivo, a ogni cambio location il right panel apre il file in
  preview riusando il plumbing esistente (apertura file/diff del RightPanel),
  con throttle ~500ms per evitare sfarfallio.
- Location con `line`: scroll-to-line se la preview lo supporta, altrimenti
  apertura semplice del file (da verificare in fase piano).

## 6. Edit review (card fine turno)

- Il reducer accumula i path dei tool call `kind == .edit` completati nel
  turno; `turnEnded` emette un nuovo item `.editSummary(paths)` (nuovo case
  `TranscriptItem` con kind label nuovo in `chatItem.kind`; nessuna
  migrazione DB, il payload è JSON).
- Card "N file modificati": lista file; click apre il diff nel right panel;
  bottone "Ripristina" per file = revert via TillerGit
  (`git checkout -- <path>`) con conferma. Nascosto nei progetti non-git.
- Il permission gate resta la difesa preventiva; la card è visibilità + undo
  post-hoc. Nessun buffering delle scritture (romperebbe il contratto
  `fs/write_text_file`: l'agente rilegge i file e deve trovare le sue
  modifiche).

## Spezzatura in piani

Tre piani indipendenti, ciascuno TDD (swift-testing) con gate `Scripts/ci.sh`
→ `CI OK`:

1. **Piano A — quick wins**: fix 3 puntini + font + MCP config.
2. **Piano B — terminali**: capability `_meta`, decodifica
   terminal_info/output/exit nel reducer, `ToolCallContent.terminal`,
   card UI.
3. **Piano C — following + edit review**: tracking locations, toggle header,
   apertura right panel, `.editSummary`, revert git.

## Decisioni chiuse

- Terminali come card nel transcript, non pane TillerTerminal (scope v1;
  l'ACP non richiede PTY interattivo).
- Terminali via estensione `_meta terminal_output` (l'unica che
  claude-agent-acp usa davvero), niente esecuzione processi lato Tiller.
- MCP solo da `.mcp.json` di progetto, nessuna UI settings (YAGNI).
- Following nel right panel con toggle default off.
- Edit review post-hoc con revert git, non staged edits.
- Font di sistema 13pt, nessuna opzione.

## Fuori scope

- UI settings per MCP servers, override globali.
- Rendering ANSI/interattività (stdin) nei terminali card.
- Review con accept/reject hunk-level.
- Persistenza del toggle following.
