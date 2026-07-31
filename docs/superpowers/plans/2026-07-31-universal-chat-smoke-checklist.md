# Universal chat port — manual smoke checklist

Build: `Scripts/build-dev.sh` (Debug, launched from `DerivedData`). The
universal engine is on by default; nothing to toggle.

Covers what CI cannot: every item below either needs a real agent process, a
real relaunch, or a pointer gesture.

## Chat basics

- [ ] **MC-1** Sidebar `+` → new chat with any agent. Tab appears, agent
      connects, a first prompt round-trips.
- [ ] **MC-2** After the first turn the tab title stops being "Chat" and
      becomes a summary. (Auto-naming must be enabled in Settings.)
- [ ] **MC-3** Rename that tab by hand, send another turn: the manual name
      survives. Auto-rename must not overwrite the user's word.

## Restore — the riskiest area

- [ ] **MC-4** Quit (⌘Q, not just closing the window) and relaunch. The chat
      tab comes back with its transcript.
- [ ] **MC-5** Immediately after that relaunch, before clicking anything:
      **no agent process should be running.** Check with
      `ps aux | grep -E "claude|codex|opencode"`. Agents may only start when
      a chat tab is actually viewed.
- [ ] **MC-6** Now click the restored chat tab: the agent starts, and the
      transcript is still there.
- [ ] **MC-7** In a terminal pane, start an agent CLI, let it register a
      session, quit and relaunch: the pane resumes that conversation instead
      of opening a bare shell. This is the path that was broken before the
      `TerminalContentID` re-keying — worth doing twice.

## History menu

- [ ] **MC-8** Chat history menu → open an older conversation. Transcript
      renders; no new turn is fired on its own.
- [ ] **MC-9** Open the *same* conversation again: it focuses the existing
      tab rather than opening a duplicate.
- [ ] **MC-10** Delete a conversation from the menu while its tab is open:
      the tab closes and the row disappears.
- [ ] **MC-11** Open a chat, close it without sending anything: it leaves no
      row behind in the history menu.

## Layout

- [ ] **MC-12** Split a chat pane right. Both halves render; neither loses
      its transcript.
- [ ] **MC-13** Drag a chat tab into another pane group: transcript, scroll
      position and composer draft all survive the move.
- [ ] **MC-14** ⌘W on a chat tab closes it. (This path did nothing before
      this port.)
- [ ] **MC-15** Open a markdown file, edit without saving, ⌘W: a
      "unsaved changes" prompt appears. Silently losing the buffer here was a
      real bug.

## Panels and failure

- [ ] **MC-16** Agents panel lists the chat and any subagent tasks.
- [ ] **MC-17** Point an agent at a bad install (or disconnect it) so it
      fails to launch: the pane shows "Agent not available", not a blank
      rectangle.
