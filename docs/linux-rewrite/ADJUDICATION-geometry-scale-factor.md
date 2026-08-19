# Adjudication — the frames are 2.00x, and one obsolete constant explains the whole disagreement

Two agents measured waku's published frames independently and returned different content-column
widths: **~597 logical px** (range 560-640) and **a lower bound of ~676**. They were twins — the
same task forked into two concurrent instances by an orchestration mistake — and they were
deliberately kept apart and told not to reconcile, because an accidental independent replication
of the load-bearing number was worth more than a tidy agreement reached by comparing notes.

That decision paid. The disagreement localises to a single constant.

## The two derivations

Both anchored the scale factor on the same on-screen element, the macOS traffic-light row, for the
same good reason: it is the one element in the frame whose logical geometry is fixed by the
operating system rather than by waku.

| | anchor | result |
|---|---|---|
| twin A (`geometry-typo-wf-e18ba166`) | traffic lights, **12 pt diameter / 20 pt pitch** | 2.30x, tolerance ±10% |
| twin B (`…-solo1238094`) | traffic lights, tried **both** era conventions | 2.00x |

## The measurement, taken independently a third time

Measured here from `reference/waku/app-screenshot-dark.png` (2266×1752), cropping the title-bar
row and scanning for saturated pixels:

- three runs, centres at x = 63.5, 109.5, 155.5 → **pitch 46.0 px**, identical across both gaps
- **diameter 28 px**
- colours at the three centres: `srgb(255,92,95)`, `srgb(255,204,0)`, `srgb(51,199,88)` — macOS's
  own red/yellow/green, so this is genuine system chrome and not custom-drawn decoration

Against the two conventions:

| convention | diameter | pitch | verdict |
|---|---|---|---|
| pre-Big-Sur, 12 pt / 20 pt | 28/12 = **2.333** | 46/20 = **2.300** | two anchors that do not agree with each other |
| Big Sur onward, 14 pt / 23 pt | 28/14 = **2.000** | 46/23 = **2.000** | both exact, and equal |

The internal proportion settles it without needing either absolute value: the measured
diameter-to-pitch ratio is **0.609**. The pre-Big-Sur spec is 12/20 = 0.600; the Big-Sur-onward
spec is 14/23 = **0.6087**. The frame matches the modern spec exactly and the old one only
approximately.

**The frames are 2.00x.** Twin A's 2.30 comes from an obsolete convention, not from a bad
measurement — its pixel work was correct and reproduced here to the pixel.

## What this does to the content column

Both twins measured the same thing in *frame* pixels and only the divisor differed:

- twin A: 597 logical × 2.30 ≈ **1373 frame px**
- twin B: 676 logical × 2.00 = **1352 frame px**

Those agree to within 1.5%. There was never a disagreement about the image — only about the ruler.
At 2.00x the column is **~676-686 logical px**.

Note what that does *not* establish. Twin B measured the ink extent of wrapped paragraph text,
which is a **lower bound**: word wrap breaks a line before the column edge, so the true column is
at least this wide and may be wider. **~676 is therefore consistent with the shipped 720 without
confirming it**, and the shipped value is not refuted by this measurement. It is also not
vindicated by it, and the provenance defect that opened this gap stands until the value is either
re-derived or declared ours by choice with a stated rule.

## The part worth learning from, which is twin A's

Twin A found a second, independent cross-check — the sidebar card measuring 102 frame px against
`CARD_TWO_LINE_HEIGHT = 51`, giving **exactly 2.000 in both frames** — and *rejected* it as
circular, on the grounds that `CARD_TWO_LINE_HEIGHT` is itself waku-sourced. Rejecting a circular
confirmation is good discipline and the instinct was right in general.

It was the wrong call here, and the reason is worth stating precisely: a cross-check that
**disagrees** with your anchor is not merely a check to discard, it is evidence against the anchor.
Twin A had 2.000 from one method and 2.30 from another, discarded the 2.000, and kept the 2.30
without revisiting the assumption underneath it. The discarded number was the correct one, and its
exactness — 102/51 = 2.000, reproduced in both frames — was the signal. A real device pixel ratio
is a clean number; 2.30 is not one, and that alone should have sent the anchor back for
re-examination.

The circularity concern also dissolves on inspection. Deriving *the scale factor* from a known
logical value and a measured rendered value is not circular; it would only be circular to then
conclude that the known logical value is correct. And in fact the card check now corroborates
rather than assumes: at 2.00x, `CARD_TWO_LINE_HEIGHT = 51` is consistent with the frames.

## Standing conclusions

1. **Scale factor: 2.00x**, established from the Big-Sur-onward traffic-light spec, corroborated
   independently by the card-height check (102/51) and by twin B's body-text line-pitch check.
2. **Content column: lower bound ~676-686 logical px.** Consistent with 720; not a confirmation of
   it. The open question of what the column *should* be (720 vs ~550 vs ~480) is a taste decision
   and remains the user's.
3. **The user pill's cap is unmeasurable** — no user turn in either frame is long enough to engage
   it. Both twins reached this independently, which is the strongest single result of the exercise.
4. Every geometry value must record its origin as measured / derived-from-measured / ours-by-choice
   / unmeasurable. A value whose only justification is "waku's source says so" is the defect this
   gap was opened for.
5. Whatever happens to the values, the false provenance comments must be corrected. Both twins did
   this first, correctly, and independently.
