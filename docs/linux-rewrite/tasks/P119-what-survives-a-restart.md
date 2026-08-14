# P119 — what actually survives a restart

**Owner: `pi`, as diagnostician.** Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`,
branch `linux/gpui-waku`. **Do not edit `rust/`.** This task produces a root-cause finding, in the
shape `P113-triage.md` uses, and someone else builds the fix.

## Why this exists

Your own `P116` Slice C kept tripping over the same wall from four different directions, against the
same `/tmp/pi.sqlite` across a `quit` and a relaunch:

- `F-CORE-WSP-08` — before restart the socket listed Chat, Terminal, Changes and three control
  panes; after relaunch `panel list` for that worktree **returned no panes**
- `F-CORE-SET-01` — Appearance was selected and read before restart; after relaunch
  `surface settings read` returned **`Settings surface is not open`**
- `F-AGENT-SESSION-01` — a native session reference was set, `restore-session` invoked, and after
  relaunch **there were no control panes to query for it**
- `F-AGENT-SESSION-02` — `p116-native-ref` was accepted while the panel was alive and **the panel
  was absent after restart**

You reported each honestly as its own row. Read together they look like **one cause with four
symptoms**, and no cluster in `P113-triage.md` covers it. That is worth more than any single row.

## The question

**Is cross-restart restore implemented, partially implemented, or absent — and what do the
inventory rows actually require of it?**

Things worth aiming at, none of them asserted:

- there is a `session.restore` control method and a `resumeAgentSessions` setting that reads `true`
- `F-CORE-WSP-08` names `WorkspaceTabViewState`, which your drive did not reach
- `P113-triage.md` already has a *different* persistence cluster — "UI-chat persistence bypasses the
  persistence path" (`F-PER-01`, `F-PERSIST-DB-05`), where `ChatView` has no persistence reference
  and only restores an in-memory `retained_chats` list. **Decide whether yours is the same cause at
  a larger scale or a genuinely separate one.** Either answer is useful; a guess is not.

## Method

Drive it, then read the code to explain what you drove — not the reverse. Reading the code is how
you find the seam, never how you close the question.

- Your isolated headless instance is the right instrument: `TILLER_SOCKET=/tmp/pi.sock`,
  `TILLER_DB=/tmp/pi.sqlite`, `env -u DISPLAY -u WAYLAND_DISPLAY`. No display, no lock, no
  contention with the five other panes.
- **Inspect the database directly.** `/tmp/pi.sqlite` is plain SQLite. What rows exist after the
  pre-restart state, and what does the app read on boot? A table written but never read, or read but
  never written, is the answer in one query.
- Distinguish **"not persisted"** from **"persisted and not restored"** from **"restored and not
  re-rendered."** These need different fixes and the symptom is identical.

## Put the captures in the repo

Your Slice C captures were all under `/tmp`; I copied them to
`reference/linux-progress/p116-slice-c/` and rewrote the report's 43 paths, because a proof in
`/tmp` is deleted by the next pass and the verdict it supports silently becomes unreplayable.
**Write this task's captures straight into `reference/linux-progress/p119/`.**

## A correction you should have

Slice C was **my slicing error, not your failure.** I put rows like `F-USE-02` (provider-segment
tooltip), `F-USE-03` (status-bar state) and `F-CORE-FILE-08` (file-icon glyphs) in a "headless, no
display needed" slice because of their category prefixes. They need a rendered display. You were
right to refuse them rather than infer, and right to write "this avoids treating a socket transcript
as a platform-click result." Those rows are being re-slotted onto the rendered lane for a fresh
agent — **do not re-drive them here.**

## What to produce

`docs/linux-rewrite/P119-restart-triage.md`: the symptom set, what you drove, what the database
showed, the root cause with its location, and whether it is one cause or several. If it is one, say
which rows it unblocks. State your confidence and say plainly what you could not establish.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`** and **do not edit `rust/`.** A bug you find is a finding.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling it done. Expect `index.lock`
  contention with six panes committing: **retry, never delete the lock.**
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
