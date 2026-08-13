# FABLE-06 — The cluster tier: dead subsystems, not dead functions

**You are fable, pane `w1:pD`.** Your context was just reset, so this brief is everything you need.

## FABLE-05 was better than the brief it answered, in three specific ways

**You reported a negative result honestly.** The brief hoped the census would reclassify a dozen
`FAILED — absent` rows as built-and-unwired. You found far fewer, said so, and said why: pass 11/12's
"zero app callers" idiom had already harvested that direction row by row, and the big absent blocks
(`F-PRJ` ×18, `F-BRW` ×9) have no dead function beneath them. **"Absent mostly means absent."** An
inflated number would have redirected three builders onto wiring work that does not exist.

**You corrected the brief's central claim rather than confirming it.** I wrote that the layered
activity model is substantially built-and-unwired. The truth is sharper and more useful: the
**detection core is wired and live** — `notify`, `agent_spawned`, `apply_exit_result`, `process_gone`,
and status does reach the sidebar dot. What is dead is *every product feature built on top of it*:
identity badges, urgency ordering, close confirmation, restore identity, close cleanup, launch-restore
ordering, eviction, notifications. The engine runs and nothing downstream consumes it beyond the dot.
That is a different repair, and a different size of repair, than what I described.

**And you found my guard was doing harm.** I added the sibling heuristic to stop false FAILEDs, said
explicitly I was keeping it conservative because a false "it's fine" flag would hide a genuinely dead
feature — and it hid two anyway: `~should` masked `F-CORE-DOM-07` and `~needs` masked
`F-CORE-USG-05`. Your defined-in-home constraint took the flags from 21 to 7, "exactly the true
variants". The guard was aimed at the right danger and still fired wrong; that is worth more than the
rows it recovered.

`TILLER_SOCKET_ENABLE` being inert in production — documented, tested, mirroring macOS, and never
consulted by the boot path — is the strongest single fact in the document. No ledger row owns it.

## The piece: the gap you identified in the instrument

Your own report names what counting cannot see:

> `sorted`/`rows`/`status`/`on_refresh` (parole comuni, campo omonimo) restano invisibili in entrambe
> le direzioni — quella è lettura.

And the census has a structural blind spot you diagnosed earlier: **a cluster of functions that call
each other, which nothing outside calls, is invisible to `DEAD`** — every member has `in > 0`, so it
lands in `local` among 211 others. `resolve_file_link` was exactly this shape and only your reading
caught it.

So the sequel is not more functions. It is **modules**.

### 1. Module reachability

Ask of every file in every crate: **is anything in it referenced from outside its own module?** A
module whose entire public surface is consumed only by itself and its tests is a dead subsystem, and
it hides a whole feature rather than one function. This is mechanical — extend
`Scripts/dead-models.py`, or write a sibling script, whichever is honest — and it collapses the 211
`local` entries into a much smaller number of *units*.

Apply the same discipline the function census earned: a positive control before any zero is believed,
and the same warning at the top that a zero on a **module** is a question, not a verdict — `mod`
declarations, re-exports and trait impls will produce false deaths exactly as they did for functions.

### 2. The name-masked tier, by reading

The residue you named: functions whose identifiers collide with struct fields, local variables or
common words, invisible to counting in both directions. That is reading work, and it is the one tier
where reading is the *only* instrument — so it is yours.

Bound it. Do not read the whole workspace. Take the names the census had to skip (`--min-len` drops
everything under 5 characters, and the common-word cases you listed) and resolve them one at a time,
with the mirror test you added to the standard: *would my probe have found this if it existed?*

### 3. Say what the shape of the remaining work actually is

You established that absent mostly means absent. That has a consequence nobody has stated: **the
remaining 123 `FAILED — absent` rows are largely real construction, not wiring.** If your module pass
confirms that from the other direction — few dead subsystems, so few hidden half-built features —
then say so plainly, because it sets the size of everything left and I am sequencing builders against
it.

## What is queued behind you, so you do not duplicate it

Your closing suggestion — hand `DEAD-MODELS.md` to `pireview` for the 5+2 adjudication — is the right
call and it is queued for `pireview`'s next dispatch. **Only the critic changes a verdict**, so do not
apply those yourself; you held that line correctly twice and it still holds.

`TILLER_SOCKET_ENABLE` is queued for `codex12`, who owns `main.rs`.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.
- **Write `docs/linux-rewrite/DEAD-MODULES.md`** (or extend `DEAD-MODELS.md` with a clearly separated
  section — your call, say which). **Do not edit `INVENTORY-LEDGER.md`.**
- `Scripts/dead-models.py` is yours to extend. Keep its header comment current: it is the only part
  of this project where the reasons behind a guard survive the guard.
- **Change no crate code.** Builders are live: `pi` in `tiller_ui` (settings/sidebar/chat/status_bar),
  `codex11` in `tiller_markdown` + `tiller_ui/editor.rs`/`file_view.rs`, `codex12` in `main.rs` +
  `tiller_control` + `tab_bar.rs`, `sonnet` in `tiller_theme`.
- **Re-run the sweep before you rely on any line of it.** Your own warning: these rows have a
  half-life of hours, and four agents are editing right now.
- **Never copy code from the reference checkouts.** **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: how many modules the reachability pass finds unreachable and which are real
after triage, what the name-masked reading tier turned up, whether the "absent means absent"
conclusion holds from the module direction, any row or seam no ledger entry owns, what you changed in
the script and its measured effect, and the honest remainder.
