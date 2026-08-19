# F-CHAT empty-composer bug — the discriminating run team-lead asked for

Follow-up to the fix landed in `e6b3816c` (merged as `0f75a093`). Team-lead
pointed out that my earlier claim — "the fresh-creation blank VISUAL-BAR saw
was contention-related" — didn't hold: that test changed two variables at
once against VISUAL-BAR's session (fixed binary, and a worktree that had
never run `restore_tabs` with real tab content, so the poisoning precondition
was absent regardless of load). This doc is the actual discriminating run:
**a worktree with persisted tabs (so `restore_tabs` really runs), then a
chat created from the `+` menu in that same session, on a quiet box, fix
reverted.**

## Setup

- Host load throughout this run: 4–25 (`uptime`, recorded before/after every
  drive below) — nowhere near VISUAL-BAR's 44–48 regime.
- Worktree `/var/tmp/tt-chat-971757-fixtureM`, wayland-drive label
  `cdbgM757`, same binary (`/var/tmp/tt-chat-971757-target/debug/tiller`)
  and same persisted DB (`/tmp/cdbgM757.sqlite`) across every step below.
- "Unfixed" = the fix commit's diff reverse-applied to the working tree only
  (`git apply -R`), rebuilt, tested, then `git checkout --` to restore —
  commit history untouched throughout.

## Step 1 — confirm the poisoning precondition (unfixed, restore_tabs runs)

The DB already had one persisted "Claude Code" tab from earlier verification
work. Relaunched the same label with the **unfixed** binary — this reruns
`restore_tabs` with real tab content and reproduces the original bug:
`01-unfixed-restored-tab-blank.png` — restored tab active, content area
totally empty.

## Step 2 — the actual discriminator: create a NEW chat via `+` in the same (unfixed, already-poisoned) session

Without restarting, drove the exact `+` → `New Chat` → `Claude Code` submenu
path (same coordinates VISUAL-BAR used: 1289,48 → 1360,349 → 1360,383) to
create a **second** chat tab via `add_chat_tab` — the fresh-creation
constructor, not `restore_tabs` — in the session that had already run
`restore_tabs` once.

**Result: also blank.** `02-unfixed-plus-created-second-tab-ALSO-blank.png`
— two "Claude Code" tabs in the strip, the newly-created one active, content
area empty.

This settles the question team-lead posed. The root cause isn't scoped to
tabs `restore_tabs` itself constructs — `centre-surface`'s oversized,
sticky-wrong height is a property of the shared ambient wrapper the whole
window's centre column renders into. Once anything triggers it, it poisons
every pane rendered into that wrapper for the rest of the window's lifetime,
regardless of how that specific tab was built. A chat created seconds later
from the `+` menu, in a session that had restored tabs earlier, is exactly
as broken as the restored tab was. Nothing else needs to be invoked to
explain VISUAL-BAR's fresh-creation blank — this is the same bug, just
broader in scope than the original fix's write-up characterized it.

## Step 3 — confirm the fix covers the broader scope too

Restored the fix (`git checkout --`), rebuilt, relaunched the same label
(DB now has two persisted, previously-blank tabs) with the **fixed**
binary:

- Both restored tabs render correctly: `03-fixed-restored-tabs-render.png`.
- Drove the same `+` → `New Chat` → `Claude Code` sequence again in that
  same (now-fixed) session to create a **third** tab. It also renders
  correctly: `04-fixed-plus-created-third-tab-also-renders.png` — composer,
  focus ring, idle pill, all present.

This is expected from the fix's own mechanism: `centre-surface`'s height is
now computed fresh from `window.viewport_size()` on every `columns()` render
call, not derived from Taffy's flex-grow result — there is no cached/sticky
state left for a `restore_tabs` call to poison in the first place, so the
fix's coverage isn't scoped to restore-built tabs any more than the bug was.

## Conclusion

Two explanations were on the table for VISUAL-BAR's fresh-creation blank:
contention, or the ambient `centre-surface` height already being wrong from
an earlier `restore_tabs` call in the same session. This run produced
**blank** on the discriminating drive (unfixed code, poisoned session,
`+`-created tab) — the second explanation. Nothing further needs to be
unfixed; the same fix in `e6b3816c` covers it, confirmed by step 3.
