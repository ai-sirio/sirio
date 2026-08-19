# Builder fix — the restored Chat tab rendered nothing

Merged 2026-08-19. Commit `e6b3816c` `fix(F-CHAT): give centre-surface a definite height instead
of flex_1`, branch `fix/chat-empty-971757`, worktree `/var/tmp/tt-chat-971757`.

## What it was

The ACP-backed Chat tab rendered **completely empty** — no composer, no placeholder, no error
message — in any session where `restore_tabs` had run, which is any session that opened a worktree
with persisted tabs: a full app restart, or a `select_worktree` worktree switch.

**What triggers it is the session having restored tabs, not how the individual tab was made.** An
earlier version of this document said fresh creation from the `+` menu "was never affected". That
turned out to be wrong and is disproved below by a deliberate run: a chat created from the `+` menu
*inside an already-restored session* is blank too. The distinction only looked real because a
brand-new worktree has no persisted tabs, so reaching the `+` menu from a clean session avoids the
precondition rather than the bug.

That is why this looked intermittent for so long, and why the first read of it was "probably host
contention": whether you saw it depended on how you had arrived, not on anything visible in the UI.

## Root cause

`centre-surface`'s `.flex_1()` height, grown by Taffy against its `centre-column` ancestor, resolved
to roughly **1656px against a real window height near 970px** — but only on frames following a
`restore_tabs`-triggered replacement of the tab array, and it stayed wrong indefinitely rather than
self-correcting on a later frame.

A chat pane's transcript is itself a `flex_1` child of `chat-root`, so it grew to fill that wrong
height and pushed the composer below the bottom edge of the actually-visible, `overflow`-hidden
area. The composer was in the element tree the whole time — never painted, never hit-testable.
The screenshots below show exactly that: the composer lives at the bottom of the surface, which is
precisely the part an oversized parent pushes out of view.

## The fix

Compute `centre-surface`'s height directly from `window.viewport_size()` minus the three fixed bars
around it, instead of trusting flex-grow for that one node. `viewport_size` is a plain field the
platform backend writes synchronously on resize — not a value Taffy computes or caches, which is
what makes it trustworthy on exactly the frames where the flex chain was not.

Everything below `centre-surface` is untouched: the pane tree, splits, and other pane kinds already
resolve their percentage/`flex-1` chain correctly against a definite ancestor height. Only how
`centre-surface` itself obtains that height changes.

## Evidence

Screenshots preserved in `builder-chat-empty-shots/` — copied out of `/tmp` when this was merged,
because that is where the drive wrote them and the next pass would have deleted them.

**The A/B/A control, which is what rules out contention** (run on a quiet box, load average in the
single digits, ~8-9):

| frame | state |
|---|---|
| `01-aba-baseline.png` | baseline, fix present |
| `02-aba-restart-unfixed.png`, `03-aba-restart-unfixed-settled.png` | **fix reverted** — Chat tab open and active, sidebar correct, centre area entirely blank |
| `04-aba-restart-fixed.png`, `05-aba-restart-fixed-settled.png` | **fix restored** — same window, same tab, same worktree; composer renders ("Message…", `idle`, Opus Plan Mode, 0%) with the worktree-path footer beneath it |

The two states differ in nothing but the fix. That is the whole argument, and it is why "the box was
busy" is not an available explanation.

Supporting frames: `06-first-repro-restart1-settled.png` and
`07-first-repro-restart11-settled.png` are from the original reproduction run — eleven restarts,
establishing that the blank surface was reliable rather than occasional, before any fix existed.
`08-worktree-switch-path.png` covers the second entry point (`select_worktree`).
`09-fresh-newchat-renders-with-the-fix.png` shows `+` → New Chat rendering correctly.

**That last frame is named carefully, because an earlier name for it claimed more than it shows.**
It was captured on the `verifyfix` run — i.e. *with* the fix applied — so it demonstrates that the
`+` path works afterwards. It is **not** evidence that the `+` path was unaffected beforehand.

## Resolved by measurement — read this before the section below it

**The discriminating run was performed, and it came back blank.** See
`BUILDER-chat-empty-followup.md` and `builder-chat-empty-followup-shots/`. With the fix
reverse-applied and a worktree carrying a persisted tab, on a quiet box (load 4-25, against the
44-48 of the original report): the restored tab is blank, and a **second** chat created immediately
afterwards through the literal `+` → New Chat → Claude Code path — in that same, already-restored
session — is blank as well (`02-unfixed-plus-created-second-tab-ALSO-blank.png`: two Claude Code
tabs in the strip, two rows in the sidebar, centre area entirely empty).

So the corruption is **not scoped to the tabs `restore_tabs` itself builds**. `centre-surface` is
one shared wrapper for the whole centre column; once anything sets the bad height, every pane
rendered into it afterwards inherits it. That is broader than this document originally said, and it
means the visual-bar critic's fresh-creation blank needs no separate explanation — it is this same
bug. **There is no second, unfixed problem blanking that surface under load.**

The fix covers the wider scope, and that was checked rather than assumed: it recomputes the height
from `window.viewport_size()` on every render, so nothing sticky survives to be inherited. Against
the same poisoned database with the fix restored, both previously-blank restored tabs render and a
third `+`-created tab renders too.

The section below is kept as the reasoning that predicted this, and as the record of an inference
that did not hold.

## The tension this replaced, and the inference that failed

The commit message says fresh creation via the `+` menu "was never affected". The visual-bar critic
that first reported this defect says the opposite: it opened the chat via `+` → `New Chat` →
`Claude Code` and got a blank surface (`VISUAL-BAR.md`, row `01-chat-empty`).

Both can be true, and the root cause is what reconciles them: what gets poisoned is
**`centre-surface`'s own height**, on frames after a `restore_tabs`. Once it is oversized, *any*
pane living inside it is pushed out of view — including a chat created a moment later from the `+`
menu. So the distinguishing factor is not how the tab was made; it is whether the app had performed
a `restore_tabs` in that session, which it does at startup for any worktree with persisted tabs.

Under that reading the `+` menu is not a safe path, it is merely a path that looks safe when you
reach it from a clean session.

**The builder did re-drive the visual-bar critic's exact sequence**, coordinates included
(`+` at 1289,48 → New Chat at 1360,349 → Claude Code submenu at 1360,383), on a brand-new worktree
on a quiet box (load 6-9, against the 44-48 regime of the original report). The composer rendered
correctly and immediately, with its focus ring. Its conclusion: the fresh-creation blank the
visual-bar critic saw "actually was contention-related".

**That run cannot support that conclusion, and the reason is worth writing down.** It changes more
than one variable against the visual-bar session at once:

- The binary it used had the fix. The stash-revert that produced the A/B/A came afterwards, by the
  builder's own ordering.
- A brand-new worktree has no persisted tabs, so `restore_tabs` never ran — the precondition for
  the poisoning is absent regardless of the fix.

Either of those alone explains a correctly-rendered composer, so the run does not isolate
contention as the cause of anything. Two explanations for the visual-bar blank are still standing:
contention, or the ambient `centre-surface` height already being wrong in a session that had
restored tabs before the chat was created.

**The discriminating run, if anyone wants to close this:** open a worktree that *has* persisted
tabs, so `restore_tabs` runs, then create a chat from the `+` menu, on a quiet box, **with the fix
reverted**. Blank means the `+` path was affected by this same bug and nothing else needs
explaining. Rendered means something else blanked that surface under load, and that something is
still in the tree.

## Regression test

Added alongside the existing P117 test, and the difference between the two is the point: the new one
constructs the tab through the **real `restore_tabs` constructor** instead of a direct `OpenTab`
literal. The P117 test built its tab the way `add_chat_tab` does, which is the path that always
worked — so it passed throughout, while the bug was live.

## What was still owed, and is now measured

Nobody had measured whether any *other* surface reached through `restore_tabs` inherited the same
oversized `centre-surface` height. That has been done — see `BUILDER-browser-changes-restart.md`:
a Browser tab and a Changes tab, both created through the real `+` menu and both put through a
genuine restart so `restore_tabs` rebuilds them, render identically before and after. The Changes
tab keeps its full diff list, section controls and Unified/Split toggle; the Browser pane's chrome
is correctly sized and positioned.

The red "Direct XCB build failed / unsupported handle: `Wayland(...)`" banner in the Browser frames
is **identical before and after** the restart and is the known nested-Wayland harness limitation —
no native window-handle path for embedding a browser surface in this sandboxed compositor. It is
not a symptom of this bug, and is noted here so a later reader does not file it as a regression.
