# P116 — the fifty-one nobody has tried

**Three slices, three different owners.** Worktree
`/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
Take **only your named slice**.

## What this batch is

`NOT EXERCISED` is now the project's real backlog: **79 rows**, most of them arrived there today when
`P108` reconciled the census and removed the ledger's unearned "this feature is absent" claims. The
rows are not broken and not missing — **nobody has ever tried them.** Under this project's one rule,
that means they do not exist yet.

28 of the 79 are in `sonnet`'s `P109`. These are the other **51**.

## Why this can now run three ways at once

Until today every gesture queued behind one `DISPLAY=:1` lock. `P112` changed that: the nested
Wayland lane accepts **synthetic left-click, `move`, `type` and named keys**, through persistent
virtual devices created before Tiller connects — and it takes **no lock**, so any number of agents
drive in parallel. Read `WAYLAND-LANE.md` before you touch it.

**What that lane still cannot do**, as of writing: right-click, pointer drags, modifier chords, IME
and non-ASCII text, and webview content. Those stay on `DISPLAY=:1` behind the lock. If your row
needs one, drive everything else about it and record `owed: gesture — <the exact gesture>`.

Slice C needs no display at all.

---

## Slice A — the surfaces (15 rows) · Wayland lane

`F-CHAT-13` `F-CHAT-24` `F-CHAT-25` `F-CHAT-26` `F-CHAT-27` · `F-SET-09` `F-SET-11` `F-SET-24` ·
`F-PRJ-14` `F-PRJ-16` · `F-EDIT-05` `F-EDIT-12` · `F-SID-19` · `F-CHG-18` · `F-PER-08`

Chat content is drivable here since `P107` — `surface.chat.compose`/`.send` reach the **rendered**
transcript now. `tab.select` (1-based, string params) brings the Chat tab forward;
`surface.chat.open` does not focus.

**Not for `codex11`** (built `F-CHAT-29/30/32/35`) and **not for `pi`** (built the subagent cards in
`F-CHAT-28`). **Not for `fable`** on the `F-SET` rows if it built them in `P111` — check `P111-report.md`
first and hand back any row you built.

## Slice B — the browser (5 rows) · `DISPLAY=:1` only

`F-BRW-05` `F-BRW-06` `F-BRW-07` `F-BRW-08` `F-BRW-09`

The embedded webview needs an X11 window handle and gets a Wayland one, so its **chrome** renders on
the Wayland lane and its **page** never does. Every one of these needs the lock, which `sonnet` holds
for `P109` — so this slice starts when the lock frees. Do not "verify" a browser row against a blank
content area on the wrong lane.

## Slice C — the headless thirty-one · no display, no lock, no contention

`F-CORE-ACT-19/20/24/25/26` · `F-CORE-FILE-03/06/08` · `F-CORE-USG-05/06/07` · `F-CORE-DOM-03/07` ·
`F-CORE-WSP-04/08` · `F-CORE-SET-01` · `F-CORE-AUTH-01` · `F-AGENT-OMP-01/02` ·
`F-AGENT-SESSION-01/02` · `F-AGENT-API-01` · `F-AGENT-SAFE-02` · `F-CTRL-CLI-02` · `F-GIT-RUN-02` ·
`F-GIT-BRANCH-01` · `F-TERM-PTY-06` · `F-TERM-UI-02` · `F-USE-01/02/03`

These are domain and control-layer rows. Run your own instance —
`TILLER_SOCKET=/tmp/<your-name>.sock`, `TILLER_DB=/tmp/<your-name>.sqlite`, `env -u DISPLAY -u
WAYLAND_DISPLAY` — and drive them over the socket and the CLI. **Every agent can run its own
instance at once**, so this slice has no contention with anything.

`F-AGENT-OMP-01` is the launch-name row (`omp` vs the distribution's `oh-my-pi`) that
`F-AGENT-OMP-03`'s evidence points at. **Not for `fable`**, which built `F-AGENT-OMP-03` in `P111`.

---

## The standard, unchanged

- **A recipe says what to DO, never what to CONCLUDE.** If you are confirming an expectation rather
  than probing, you have stopped being an independent control.
- **A row you did not successfully exercise does not exist.** No inference from adjacent behaviour.
  Reading the code is how you find the control, never how you close the row.
- **Conjunctions split.** Drive each conjunct and report it separately; the second one is the one
  that gets skipped.
- **Report verbatim where wording matters** — dialog text, menu items in order, greyed entries and
  their reason text. A paraphrase of a label is not evidence of the label.
- If a row's control does not exist on screen, **that is a finding**, not a report bug.

## What to produce

`docs/linux-rewrite/P116-report.md`, **appending to your own slice's section** and committing every
10 rows. Per row: what you drove, what the screen or the socket actually did, and the capture
filename. **No verdict column** — `pireview` sets verdicts.

Three agents write this file. **Re-read it immediately before each write and append to your own
tail** — a check at task start does not authorise a write at task end, and this project has already
lost a section that way.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not edit `rust/` — a bug you find is a finding.
- Commit path-scoped, never `git add -A`. `grep '??'` before calling a row done. Expect
  `index.lock` contention with six panes committing: **retry, never delete the lock**.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
