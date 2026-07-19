# Chat UI — manual smoke checklist

Prerequisites: `claude` CLI authenticated; `opencode` installed; node/npx on PATH.

- [ ] Sidebar "+" → "Chat · Claude Code" opens a chat tab; header shows icon + "connessione…" then "pronto" (first run: npx download may take ~30s)
- [ ] Prompt "create file hello.txt with content hi" → tool call card appears → permission card (amber) with adapter options → Allow → file exists on disk only after Allow
- [ ] Reject on a second edit → file untouched, card shows rejection
- [ ] Sidebar badge: worktree shows running while prompting, needs-input badge + notification on permission request
- [ ] Plan card renders and updates for a multi-step request
- [ ] @-mention: type "@app" → popup lists files → select → chip; agent receives the file (asks it to summarize)
- [ ] Slash menu lists agent commands (if any); mode selector switches (plan/default) and survives current_mode_update
- [ ] Stop button cancels a long turn (stopReason cancelled, composer usable again)
- [ ] Queue: send while working → message queued → dispatched at turn end
- [ ] Image: attach from clipboard/file → agent describes it
- [ ] Quit app with chat open → relaunch → tab restored, transcript visible, "session/load" resume works (Claude Code); OpenCode: same flow
- [ ] Kill the adapter process manually → banner "Agente disconnesso" → "Riavvia agente" works
- [ ] No node installed scenario (rename npx): banner shows error, no crash
- [ ] Close chat tab with active turn → process killed (verify with ps)
- [ ] Remove worktree with chat history → chatSession rows gone (sqlite3 check)
- [ ] OpenCode chat end-to-end: open, prompt, tool call, permission
