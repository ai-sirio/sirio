# P106 — exercise the fifty-six rows the ledger got wrong

Worktree `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
**Three owners, one brief:** `codex12`, `fable`, `codex11`. Slices below.

## What P105 found

`RECENSUS-2026-08-14.md` re-censused 70 of the 86 rows the ledger calls `FAILED — absent`:

| | rows | what the ledger claimed |
|---|---|---|
| **BUILT** | 20 | wrong — the symbol exists with a production call site |
| **PARTIAL** | 36 | wrong in a subtler way — one conjunct exists, the other does not |
| **ABSENT** | 14 | right |

**Fifty-six of seventy were miscalled.** The bucket that was supposed to say "there is work to
build here" was 80% rows that are already built and have simply never been driven.

`fable` also found the mechanism, and it is worth knowing because it will keep happening:
three of the flipped rows carry code comments that **name their own ledger row id** —
`render_tool_diff` says `F-CHAT-31`, `segment_text` says `F-SET-11`, `render_agents` says
`F-SET-16`. Builders have been landing row-targeted fixes and nobody has been closing the loop
back to the ledger. The searching was never the broken part.

## What you are doing

**Driving these rows, not building them.** Every row in your slice has code and a file:line. The
census stopped exactly where a census must: a row found BUILT becomes `NOT EXERCISED`, never
`PASSED`. You are the step that converts it.

Take the census entry as a map, not as a conclusion. If the symbol it cites does not do what the
clause says when you drive it, **that is the finding** — report it, do not repair it.

### BUILT rows — drive the whole clause

Reach the feature the way the clause describes, make it act, and record what the screen or the
socket actually returned. A screenshot that merely contains the control is not proof the control
works; that is the mistake that produced this project's false `PASSED`s in the first place.

### PARTIAL rows — two separate jobs, reported separately

1. **Drive the conjunct the census found built.** Same standard as above.
2. **Confirm the missing conjunct is really missing**, and say what is owed. The census named
   which half was absent and the needle it used; re-run that needle. If you find it after all,
   say so — the census is one pass, and this is exactly the kind of bucket that rots.

Never merge the two into one verdict. Half a clause proven is `half-proven`.

## The lanes

**`Scripts/wayland-drive.sh` is new and it is the lane you should reach for first.** It boots a
nested headless compositor, launches the app against a private socket and DB, drives it over the
control socket, and captures real pixels — with **no drive lock**, so all three of you can run it
at the same time. Read `WAYLAND-LANE.md` first; the short version:

```bash
Scripts/wayland-drive.sh /tmp/<you>-shots '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open
  ctl tab.select index=1
  shot chat
'
```

`ctl <method> [k=v …]` sends a `ControlRequest`; `shot <name>` forces a repaint and captures.
Set `TILLER_WL_LABEL=<you>` so your paths never collide with the other two.

Four things that will otherwise cost you an hour each:

- **`open` does not focus.** `surface.chat.open` and `browser.open` return `ok` with full state
  and leave the visible tab where it was. `tab.select` brings a surface forward. It is
  **1-based** — `index=0` is refused — and every param is a **string**.
- **A capture that did not change proves nothing** until you know the frame is fresh. The app
  repaints lazily; `shot` forces it, but if you capture by hand, read trap 2.
- **The Wayland lane cannot render webview content.** `F-BRW-08`'s chrome is judgeable here; its
  page is not. That row's content half needs `DISPLAY=:1`.
- **There is no synthetic input on this lane at all.** Nothing you send lands, and every tool
  that sends it reports success. Anything gated on a click, a drag or typed text is **owed**, not
  failed.

**The X drive lock is held by `sonnet` for `P104` and will be for a while.** Do not queue behind
it. When a row needs a gesture, drive everything about it that you can reach, then record
`owed: gesture — <the exact gesture>` and move on. Those become one batch for whoever takes the
lock next, and a clean list of them is worth as much as a verdict.

## Slices

Each of you is exercising rows **someone else** censused, so no one grades their own pass.

### `codex12` — 22 rows (projects, chat, settings) — censused by `fable`

```
BUILT    F-PRJ-04 F-PRJ-07 F-PRJ-10 F-CHAT-21 F-CHAT-22 F-SET-11 F-SET-24
PARTIAL  F-PRJ-03 F-PRJ-11 F-PRJ-12 F-PRJ-17 F-PRJ-18 F-CHAT-02 F-CHAT-16
         F-CHAT-18 F-CHAT-23 F-CHAT-31 F-CHAT-34 F-SET-15 F-SET-16 F-SET-18 F-SET-21
```

`surface.settings.open` and `.select section=<id>` render Settings directly; sections are
`ai-providers`, `agents`, `general`, `permissions`, `appearance`. `F-CHAT-23`'s Dismiss conjunct
is `pi`'s build (`P102`) — confirm it is absent, do not build it.

### `fable` — 17 rows (window, sidebar, tabs, changes) — censused by `codex12`

```
BUILT    F-WIN-01 F-SID-16 F-SID-17 F-TAB-01 F-TAB-18 F-TAB-23 F-TAB-28 F-CHG-20
PARTIAL  F-WIN-07 F-WIN-10 F-SID-06 F-SID-11 F-SID-15 F-TAB-11 F-TAB-24 F-CHG-01 F-PER-07
```

This slice is the most gesture-bound of the three — tab menus and sidebar reordering are
right-click and drag. Expect a high `owed: gesture` count and do not treat that as failure;
`project.add`, `tab.select`, `tab.cycle`, `surface.changes.open` and the `panel.*` group still
reach a good deal of it. `sonnet` is driving a **different** set of `F-TAB`/`F-SID`/`F-CHG` rows
in `P104` — the two lists are disjoint, so do not read its report as covering yours.

### `codex11` — 17 rows (core, usage, control, agents) — censused by `codex12`

```
BUILT    F-BRW-08 F-USE-03 F-CORE-USG-06 F-CORE-USG-07 F-CTRL-CLI-02
PARTIAL  F-USE-01 F-USE-02 F-CORE-ACT-24 F-CORE-ACT-25 F-CORE-ACT-26 F-CORE-DOM-03
         F-CORE-WSP-04 F-CORE-WSP-08 F-CORE-FILE-08 F-CORE-SET-01 F-AGENT-API-01
         F-TERM-SPLIT-01
```

Mostly non-UI, which is your strongest lane after `P100`: `HEADLESS-LANE.md`'s socket-only route
and the `tillerctl` CLI reach most of it without any display. `F-BRW-08` is the exception — its
content half needs the X lane, so record it as owed.

## What to produce

`docs/linux-rewrite/P106-report.md`, one section per owner, **committed every 10 rows.** Three
sessions were interrupted mid-work yesterday; an uncommitted report is a lost report.

Per row: the row id, what you drove (the exact `ctl` calls or gesture), what happened
(verbatim where wording matters), the capture filename if you took one, and for PARTIAL rows the
two halves reported separately. **No verdict column** — you report observations; a critic sets
verdicts and it will not be you.

## The rules

- **Do not edit `INVENTORY-LEDGER.md`.** Do not edit `RECENSUS-2026-08-14.md` either — if the
  census is wrong about a row, say so in your report.
- **Do not fix what you find.** A defect is a finding. Fixing it makes you the builder of the row
  you were sent to judge, and the judgement is then worthless.
- Commit path-scoped, never `git add -A` — the three of you are writing into the same report file
  and other agents are editing `rust/`. `grep '??'` before calling a slice done: an explicit-path
  commit silently drops a file nobody was thinking about.
- Do not idle on an approval gate — `ENVIRONMENT.md` §"Working with the orchestrator".
