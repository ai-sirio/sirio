# P131 — Root-causing the capture-staleness artifact in `wayland-drive.sh`

## The report that triggered this

A wave-G critic judging `F-CHAT-20` wrote:

> mid-stream captures taken seconds apart repeatedly returned frames with identical stale progress
> percentages, consistent with a capture-pipeline staleness artifact in this harness rather than
> evidence for or against the app's behavior.

If `shot()` can silently hand back a stale frame, every null result in this project is weakened, not
just this one row — so this needed a real experiment, not a read of the code.

## Why a live agent stream is the wrong instrument

`F-CHAT-20`'s "progress percentage" is the composer's context-usage readout — `"{percent}% of
context used"`, driven by `ContextUsage`/`TokenUsageBreakdown` ACP wire events
(`rust/crates/tiller_ui/src/chat.rs:1560-1577`, rendered at `chat.rs:5758` and `chat.rs:6483`; the
16px ring beside it is `context-ring` / `context-ring-progress`, `chat.rs:5649-5667`). Those events
arrive whenever the live agent decides to emit them — an unknown, uncontrolled cadence. A capture
that shows the same percentage twice in a row is consistent with *both* "the harness returned a
stale frame" and "the agent genuinely didn't send a usage update in that window," and a live agent
stream cannot distinguish the two: there is no ground truth for what the percentage *should* read at
capture time.

The fix is to stop depending on the agent and drive something whose value at time **T** is known in
advance. A terminal pane printing an incrementing counter on a fixed clock is exactly that
instrument — PTY output the app unambiguously has to render (there is no "did the app decide this
was worth a repaint" ambiguity the way there might be for a custom widget's own `cx.notify()` calls),
and its expected value at any capture time is `floor((now - t0) / interval)`, computable from
wall-clock timestamps bracketing each capture.

## Experiment 1 — does `shot()` keep up with a fast, predictable change?

Loaded the real app (`project.add` on this repo), clicked into a Terminal pane, and typed (through
`wtype`, the harness's normal keyboard path) a loop that echoes `TICK <n> <unix-epoch-with-ms>` every
300 ms. Bracketed three `shot()` calls (the actual, unmodified `shot()` — full nudge-to-`W2xH2`-and
-back dance) with `date +%s.%N` immediately before and after each one, spaced 2 s apart:

| capture | bracket (s since epoch) | last TICK line visible | TICK timestamp | lag behind bracket end |
|---|---|---|---|---|
| tick-a | 81.999 → 83.587 | `TICK 10` | 82.959 | 0.63 s |
| tick-b | 85.592 → 87.214 | `TICK 22` | 86.607 | 0.61 s |
| tick-c | 89.217 → 90.815 | `TICK 34` | 90.259 | 0.56 s |

All four frames in the run (baseline + 3 ticks) had **different MD5s**; the counter advanced by
exactly the number of ticks real time predicts (12 ticks / 2 s at a 0.3 s cadence for tick-a→b, same
for b→c). The lag is small, constant, and fully explained by `shot()`'s own internal timing (grim
fires partway through its `sleep 0.4` + `sleep 1` budget, not at the very end) — not growing, not
stuck. Screenshots: `reference/linux-progress/p131-capture-staleness/p131-tick-{a,b,c}-t{2,6,10}s.png`.

**Verdict: no staleness in `shot()` at 2 s spacing against a 0.3 s-cadence source.**

## Experiment 2 — is the forced resize-configure actually load-bearing?

`WAYLAND-LANE.md`'s trap 2 claims repaint is lazy and "after the first frame the app sits still, and
grim keeps returning that frame byte for byte" absent a forced configure. That claim, if still true,
would mean `shot()`'s nudge is necessary — and would also mean any capture *without* it is a false
negative waiting to happen. Tested it directly: same ticking terminal, but called `grim` **raw**, with
**no resize at all**, twice, 3 s apart:

- `raw-1` (grim fired ~0 s after a `sleep 3`, immediately, no nudge): showed `TICK 08` at `149.985`,
  bracket timestamp `150.687` — 0.7 s lag, same order as Experiment 1.
- `raw-2` (3 s later, still no nudge): showed `TICK 18` at `153.016`, bracket `153.754` — 0.7 s lag.

Both frames advanced correctly and both had different MD5s from each other and from the baseline.
Screenshots: `p131-terminal-raw-no-nudge-{1,2}.png`.

Repeated this for the exact scenario trap 2 documents — a **one-shot, non-animating, socket-driven**
state change (`project.add`), not a continuously-redrawing terminal — with raw `grim` calls at 0 s,
0.3 s and 2 s after the socket call, no resize anywhere in the sequence:

| capture | MD5 | matches |
|---|---|---|
| `raw-before-add` | `66cfa9…` | baseline (expected — nothing changed yet) |
| `raw-after-add-immediate` (+0.3 s, no nudge) | `9abc86…` | **neither** before nor final — a genuine in-flight/partial frame |
| `raw-after-add-2s` (+2 s, no nudge) | `2981dc…` | **identical** to the fully-settled, nudge-`shot()`-captured frame |

By 2 s, an **un-nudged** `grim` call converges to exactly the same bytes as `shot()`'s nudged one.
Screenshots: `p131-raw-before-add-no-nudge.png`, `p131-raw-after-add-0.3s-no-nudge.png`,
`p131-raw-after-add-2s-no-nudge.png`.

**Verdict: on this build (sway 1.9, `WLR_RENDERER=pixman`, current `tiller` binary), a plain `grim`
call repaints and delivers fresh content on its own within ~2 s, with or without a forced
configure.** Trap 2's blanket claim ("grim keeps returning that frame byte for byte" with no
qualification) does not reproduce here — the 2026-08-14 observation it documents most likely reflects
too-short a wait at the time, not a permanently stuck repaint pipeline. `shot()`'s nudge is not
*wrong* to keep (it is a strictly stronger, deterministic guarantee that costs ~1.4 s and removes any
dependence on ambient settle time), but it is not the thing standing between this harness and
staleness — plain elapsed time already is not stale here either.

## Experiment 3 — can consecutive `shot()` calls, back-to-back, ever repeat?

This is the closest reproduction of what the critic actually did: multiple captures in the same
drive, close together, of continuously changing content. Five `shot()` calls issued with **zero**
extra sleep between them (each call's own ~1.4-1.6 s internal budget is the only spacing) against the
same 0.3 s-cadence ticker:

```
back1  12002 colours
back2  12311 colours
back3  12384 colours
back4  12420 colours  →  TICK 22 visible
back5  12420 colours  →  TICK 28 visible   (same colour count as back4, DIFFERENT md5 and content)
```

All 5 frames (plus baseline) had distinct MD5s. `back4`/`back5` sharing a colour count but differing
in content is itself useful: **colour count alone is not a sufficient freshness check** — two
different frames of scrolling terminal text can happen to use the same palette. MD5 (or, for a
targeted region, pixel content) is the check that matters.
Screenshots: `p131-backtoback-shot1.png`, `p131-backtoback-shot5.png`.

**Verdict: no staleness in five back-to-back `shot()` calls.**

## Conclusion — where the fault is, and where it isn't

None of the four candidates named in the task hold up as the explanation for the `F-CHAT-20` report:

- **`grim` reading a cached buffer** — ruled out. Two raw `grim` calls 3 s apart on a live terminal
  returned different bytes tracking real content each time.
- **The compositor not committing a new buffer** — ruled out. The un-nudged `project.add` test shows
  the compositor delivering a fresh composited frame within ~2 s with no configure event in the
  sequence at all.
- **The app not repainting (GPUI's lazy paint skipping a frame)** — ruled out *for the content types
  tested* (terminal PTY output, one-shot control-socket state mutation). Both repaint on their own.
  This experiment cannot rule it out for the *specific* `context-ring`/percentage widget, since that
  requires a live ACP `ContextUsage` event to exercise — which is exactly the untrusted instrument
  this task replaced. If a future pass wants to close that gap, the finding here is what to check:
  confirm `AcpEvent::ContextUsage`'s handler reaches the same `cx.notify()` call the rest of
  `handle_acp_event` relies on (`chat.rs:1560` is inside the same match whose fallthrough hits
  `cx.notify()` at `chat.rs:1736`) rather than assuming the harness is at fault.
- **The settle delay being too short** — ruled out at the spacings tested. `shot()`'s built-in ~1.4 s
  nudge budget consistently lagged real content by ≤0.7 s against a 0.3 s-cadence source, the same
  margin a plain 2 s wait gave with no nudge at all.

**The capture pipeline is not the source of the staleness the critic observed.** The far more likely
explanation for "identical percentages, captures seconds apart" is that the agent's own `UsageUpdate`
cadence during that particular turn was coarser than the seconds-apart sampling — i.e. the value
genuinely had not changed yet — which is evidence about ACP event timing (or possibly integer
rounding of the displayed percent), not a harness defect. `F-CHAT-20`'s manual-scroll-ownership half
should stay owed on its own terms (the scroll primitive/verification gap already on record), not be
discounted for a harness staleness concern this task did not reproduce.

## What changed in `Scripts/`

**Nothing in `shot()`'s mechanics.** No defect was found to fix, and inventing a change against a
pipeline that measurably isn't stale would be more likely to introduce a new bug (see the
alternating-resolution history this same function already survived) than to fix one. The three
invariants were re-verified against the unmodified script and all hold:

1. **Every frame is captured at exactly 1715x972.** Confirmed across every run in this task
   (`identify -format '%wx%h'` on every capture, including the final formal check below).
2. **Consecutive captures of a genuinely changing UI have different MD5s.** Confirmed repeatedly —
   Experiment 1 (3 spaced `shot()`s), Experiment 3 (5 back-to-back `shot()`s), plus the raw-`grim`
   variants — zero collisions where content actually changed.
3. **A click at coordinates read off a frame still lands where the frame showed it.** Final formal
   check: `shot()` → read `378,51` off the resulting frame (the Terminal-tab-selected state) →
   `click 378 51` → `shot()` again. The second frame shows the **Chat** tab underlined/selected, the
   composer with its context-ring readout, and `Terminal`'s tab marked with the unread dot — the
   click landed exactly where the prior frame showed the Chat tab to be.
   `p131-invariant-terminal.png` → `p131-invariant-chat-click-landed.png`.

`docs/linux-rewrite/WAYLAND-LANE.md`'s staleness note is updated with a pointer to this task and the
corrected framing of trap 2 (see next section of that doc).
