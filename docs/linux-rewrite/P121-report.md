# P121 — independent verification of the P117 fix

**Verifier did not build P117.** Every claim below is from a fresh drive of the current binary
(`23af26c`, `rust/target/debug/tiller` built 18:26) run by the verifier, not from the builder's
frames. Captures are in `reference/linux-progress/p121/`. `INVENTORY-LEDGER.md` is untouched —
this report closes no inventory row, per instruction.

## Verdict summary

| # | Item | Verdict |
|---|---|---|
| 1 | Marker reproduction (own marker, not the builder's) | **CONFIRMED** |
| 2 | Changes surface draws its full list unclipped | **CONFIRMED** |
| 3 | Scrolling on an overflowing transcript | **CONFIRMED** |
| 4 | Split behaviour after the height fix | **CONFIRMED — no regression** (splits are strictly better than pre-fix, which fully collapsed); two unrelated findings noted below |

## 1. Reproduction, with a marker only this drive could have put on screen

Re-ran the report's drive verbatim except for the marker text, so the frame can't be confused with
the builder's:

```
TILLER_WL_LABEL=p121verify Scripts/wayland-drive.sh reference/linux-progress/p121 '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open
  ctl tab.select index=1
  sleep 2
  ctl surface.chat.send surfaceId=default-chat text=P121VERIFY_MARKER_ENZO
  sleep 6
  ctl surface.chat.read surfaceId=default-chat
  shot 01-chat-after-send
'
```

`02-01-chat-after-send.png` / crop `02a-chat-after-send-crop.png` show `P121VERIFY_MARKER_ENZO` as
a user bubble with a real assistant reply beneath it and the composer pinned at the bottom — not
the pre-fix layout (composer at the top, ~670px of blank below it) described in the P117 report.
`ctl surface.chat.read` corroborates: the transcript JSON returned the same string. The marker is
mine, sent in this drive, so this discriminates against "some region is no longer blank" the way
`CRITIC-visual-baseline.md` requires.

## 2. Changes surface — full list, not clipped

```
TILLER_WL_LABEL=p121changes2 Scripts/wayland-drive.sh reference/linux-progress/p121 '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.changes.open
  ctl tab.select index=3
  sleep 3
  ctl surface.changes.open
  shot 04-changes-open
'
```

`02-04-changes-open.png` / crop `04a-changes-full-crop.png` show `Local changes (20)` — `Changed
(8)` and `Untracked (12)`, all 20 rows drawn, with genuine blank space below the last row (not a
clip boundary). Measured per `WAYLAND-LANE.md`'s stddev method as a sanity check, not as the
primary evidence (I can see the frame directly): the row region reads `stddev=36.5` (clearly
non-uniform — text and glyphs), the region below the last row reads `stddev=1.2` (uniform
background, as expected for empty space, not a cut list).

## 3. Scrolling on an overflowing transcript — not exercised by P117, exercised here

The report flagged this as untested: the test window's transcript is 720×892, and nothing in P117
drove enough content to overflow it. Built up 8 real turns (`send_and_wait`, polling
`surface.chat.read` for `"status":"completed"` between each send so the composer's single-slot
queue — confirmed to coalesce/drop intermediate sends while `status:"streaming"` — didn't swallow
messages) with unique per-turn markers `P121SCROLL2_A` … `P121SCROLL2_H_LAST`:

```
TILLER_WL_LABEL=p121scroll2 Scripts/wayland-drive.sh reference/linux-progress/p121 '
send_and_wait() {
  ctl surface.chat.send surfaceId=default-chat text="$1" >/dev/null
  for i in $(seq 1 40); do
    st=$(ctl surface.chat.read surfaceId=default-chat)
    grep -q "\"status\":\"completed\"" <<<"$st" && break
    sleep 0.5
  done
}
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open
  ctl tab.select index=1
  sleep 2
  send_and_wait P121SCROLL2_A_the_quick_brown_fox_jumps_over_the_lazy_dog
  ... (B through G identical shape)
  send_and_wait P121SCROLL2_H_LAST_the_quick_brown_fox_jumps_over_the_lazy_dog
  shot 06-scroll-overflow-full
  ctl surface.chat.read surfaceId=default-chat
'
```

`02-06-scroll-overflow-full.png` / crop `06a-scroll-overflow-full-crop.png`: the transcript
auto-scrolled to the bottom. `_H_LAST` and its reply are fully visible; the *top* of the visible
region is a sentence fragment ("anything until you send an actual request.") that is the tail end
of an earlier reply — proof the view is genuinely scrolled past the earlier turns, not merely
showing everything unclipped in a tall box. `surface.chat.read` confirms all 8 user/assistant pairs
are in the transcript even though only the last ~1.5 are on screen. Scrolling works; the fix did
not disturb it.

## 4. Split behaviour — the flagged risk

The report's own risk: `MIN_SPLIT_PANE_SIZE` (160px) was flooring a *collapsed-to-zero* subtree
before the fix, so a real split could have looked fine by coincidence while actually being masked
the same way chat and Changes were. Re-drove it directly.

**One split (`pane.split direction=right` then `direction=down` on the new pane), normal case:**

```
TILLER_WL_LABEL=p121split Scripts/wayland-drive.sh reference/linux-progress/p121 '
  ctl project.add path=/home/enzopalmisano/Scrivania/Progetti/tiller-linux
  ctl surface.chat.open
  ctl tab.select index=1
  sleep 2
  shot 07-before-split
  ctl pane.split direction=right
  sleep 1
  shot 08-split-right
  ctl pane.split direction=down
  sleep 1
  shot 09-split-down
  ctl surface.chat.send surfaceId=default-chat text=P121SPLIT_MARKER_ENZO
  sleep 6
  shot 10-split-with-marker
'
```

`04-09-split-down.png`: Chat pane (left, ~50% width) and two terminal panes stacked on the right,
each roughly **450px tall** — half of the ~926px content height, nowhere near the 160px floor.
This is the direct answer to the flagged risk: a normal split gets real, proportional height from
the now-fixed `group-surfaces-wrapper`, not a floor value that happened to look plausible.

**CONFIRMED — no regression.**

### A second thing, found while driving this — not a P117 regression

`05-10-split-with-marker.png` / zoom `10c-split-marker-zoom2x.png`: after the split, sending
`P121SPLIT_MARKER_ENZO` into the now-narrower Chat pane draws a user bubble that is **clipped to
`P121S`** — the text does not wrap to the pane's reduced width, it is cut at the pane's right edge.
This is real (confirmed by direct inspection, not inferred), but it is a **width**-wrap issue in
the chat bubble, not the **height** collapse P117 fixed, and P117's diff never touches bubble
rendering. Not filing this as a P117 regression; flagging it here so it isn't lost. It does not
block this report's verdict, and per instruction I am not touching `INVENTORY-LEDGER.md` for it.

### A third thing, found while stress-testing splits — checked against the pre-fix commit

Four consecutive `pane.split direction=down` calls on the Chat pane produce a lopsided stack (each
split roughly halves the *newly created* pane, not the whole group), and the last pane(s) get
pushed low enough that the bottom of the stack is cut off by the window edge rather than the
group reflowing or becoming scrollable (`02-11-quad-split-down.png`).

This looked like it could be the same class of bug as P117 — enough splits drives a leaf toward
`MIN_SPLIT_PANE_SIZE`, and the question was whether the *new* real-height wrapper handles that
better or worse than before. So rather than guess, I built the pre-fix parent commit
(`0eac14d`, the commit immediately before `23af26c`) in an isolated `git worktree` at
`/tmp/tiller-p121-prefix`, with its own `CARGO_TARGET_DIR` so the comparison build never touched
the shared target other agents in this worktree are using, and re-ran the same drive against it —
first an **unsplit** baseline to confirm the old binary actually reproduces the known defect on
this worktree, then the same quad-split.

`14-prefix-unsplit-baseline.png`: composer pinned near the **top** of the pane with the worktree
path directly beneath it and nothing else — the exact pre-fix signature described in the P117
report (composer at top, not bottom, no transcript room). This confirms the pre-fix binary and
drive genuinely reproduce the defect on this worktree, so the comparison is apples-to-apples.

`15-prefix-quad2-split-down.png`: after two `pane.split direction=down` calls, the pane is
**completely blank** — no composer at all, no divider, nothing. That's strictly worse than the
unsplit pre-fix case, which at least drew a squished composer. So the pre-fix code didn't produce
a "160px-floored but usable" split — it produced total collapse the moment a split and a
zero-height wrapper combined.

Compare that to the **fixed** binary's `04-09-split-down.png` from the same drive shape: two
clearly visible, correctly proportioned ~450px panes with normal content. The fix did not
introduce a regression in splits — it fixed a second, more severe collapse than the one it was
built for, one nobody had exercised: chat content interacting with `pane.split`.

**CONFIRMED: no regression, and split-plus-chat is strictly better than before.**

The one caveat is the "third thing" above (four *consecutive* nested splits eventually clip past
the viewport edge on the fixed binary) — worth a look separately, but it is a partial-viewport
overflow in an extreme, unrealistic nesting case, not the was-it-there-before collapse the risk
was about. I did not bisect that specific edge case against the pre-fix binary because the pre-fix
binary can't render a split at all to compare against.

Cleaned up afterward: killed the temporary instance by matching its own `TILLER_SOCKET`/`SWAYSOCK`
env vars (never by process name, per `WAYLAND-LANE.md` trap 4), then `git worktree remove
/tmp/tiller-p121-prefix --force` and removed its separate `CARGO_TARGET_DIR`. `git worktree list`
now shows only the two real worktrees again.

## What I did not touch

`INVENTORY-LEDGER.md` — unedited, per instruction; this report closes no inventory row. I did not
run `cargo test` in the shared worktree target — the report itself says the test is not the proof,
and building here risked contending with `codex12`'s live `P120` work in the same tree. All
captures are under `reference/linux-progress/p121/`, nothing under `/tmp` was kept as evidence.

## Two findings for someone else's queue, not this report's verdict

1. **Chat bubble text does not wrap in a narrowed pane** (`10c-split-marker-zoom2x.png`) — clips at
   the pane edge instead of wrapping. Width, not height; P117's diff never touches bubble
   rendering. Likely pre-existing, not reproduced against the pre-fix binary since the pre-fix
   binary can't render a split chat pane to compare against.
2. **Four consecutive nested `pane.split direction=down` calls overflow the viewport**
   (`02-11-quad-split-down.png`) — the deepest pane(s) get clipped by the window bottom rather than
   the stack reflowing or scrolling. Real, but an extreme nesting case, and not the collapse this
   report was checking for.

## Overall verdict

**The P117 fix is correct and holds up under independent re-drive.** All four items the report
asked the verifier to establish are confirmed with fresh evidence, not the builder's frames: a
verifier-chosen marker draws in the transcript, the Changes list draws all 20 rows unclipped,
scrolling on an overflowing transcript works and auto-follows the bottom, and — the one thing this
change could plausibly have disturbed — splits are not just undamaged, they went from **totally
broken** (confirmed against the actual pre-fix binary) to correctly proportioned. Two unrelated,
pre-existing-looking issues were found along the way and are recorded above for someone else's
queue; neither is a P117 regression and neither blocks this verdict.
