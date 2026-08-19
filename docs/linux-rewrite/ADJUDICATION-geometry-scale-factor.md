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

## The strongest evidence came from the losing side's own numbers

Twin A finished with seven commits and reported three "low-confidence tensions" — measurements that
disagreed with values already pinned in the code — plus one material finding. Every one of them
resolves at 2.00x:

| quantity | A's value at 2.30x | implied frame px | at 2.00x | pinned in code |
|---|---|---|---|---|
| body line-height | 18.0 | 41.4 | **20.7** | 21.0 |
| user pill radius | 10.6 | 24.4 | **12.19** | 12.0 (`user_pill`) |
| content column | 560-690 | 1288-1587 | **644-793** | 720 |
| sidebar card height | (rejected) | 102 | **2.000 ratio** | 51 (`CARD_TWO_LINE_HEIGHT`) |

The line-pitch row is worth dwelling on: A's 18 logical implies 41.4 frame px, and twin B measured
42 frame px directly. They measured the same thing and got the same answer, to within a pixel.

A's material finding — "the content column is not confirmed by the frames, measured ~560-690,
closer to the ~550 alternative than to 720" — **reverses** at the corrected scale. Its own span
becomes 644-793, which contains 720 and contains twin B's 676-727 entirely.

**The tell, and it is generalisable.** A had four independent measurements each disagreeing with
the code, and read each as a question about the code. But they disagreed *by the same ratio*:
21/18 = 1.167, 12/10.6 = 1.132, and 2.30/2.00 = 1.15. When several unrelated quantities are each
off by one common factor, the ruler is wrong, not the things being measured. A single value in
tension is a finding; three in tension by the same proportion is a calibration error, and the
right response is to go back to the anchor rather than to write three careful low-confidence
caveats.

None of which is a reason to discount A's work. It measured well, wrote its methodology down, and
declared what it could not measure — including several things B did not attempt (bar heights,
sidebar width, type-scale point sizes, most spacing steps, each with a stated reason). It also
produced the one artefact neither the adjudication nor B has: an `include_str!`-based provenance
regression test that fails if "waku's measured" reappears in a comment, hand-verified with a
positive control. That test should survive the merge whichever branch forms its base.

## The critic did not clear it, and its objection lands on this document too

A fresh critic returned **`cleared: false` — TRANSPLANTS REMAIN** against the remedy. Two of its
findings change what is written above.

**It calls the cross-checks circular, and the objection is partly right.** Both the line-pitch check
(42 frame px against `body_line_height` 21.0) and the card-height check (102 frame px against
`CARD_TWO_LINE_HEIGHT` 51.0) divide a measured span by a value that was itself read out of waku's
source. Twin A refused the card-height check on exactly that ground and wrote a function to name the
refusal; twin B used the line-pitch check without engaging the objection. This document called them
"three independent corroborations". That was too strong: the traffic-light anchor is external, but
the other two are the same *kind* of evidence as each other, not two more kinds.

What survives the objection, stated precisely. Deriving *the scale factor* from a known logical
value and a measured rendered span is sound; it becomes circular only if one then concludes the
known value is correct, which nobody does here. And the two checks are not redundant with each
other: 51 and 21 are unrelated quantities in unrelated components, and both land on exactly 2.000.
So they are weaker than three independent anchors and stronger than one.

**The load-bearing argument is one neither twin led with, and the critic supplied it: macOS backing
scale factors are integers.** A native screenshot at native resolution is 1x or 2x, never 2.30. That
constraint alone excludes twin A's answer without appealing to any waku-sourced value, and it is
what promotes the Big-Sur-onward traffic-light spec from "the era I chose" to "the era the
measurement requires". The reasoning should run in that order — integer constraint first, era spec
second, waku-sourced cross-checks third as consistency rather than as proof. It does not run that
way in `THEME-PROVENANCE.md`, and that is the critic's largest gap: a document that sounds more
certain than its method earns, which is a smaller instance of the disease this whole gap was opened
to cure.

**The sweep is incomplete, and the twins are complementary.** Four unhedged "waku's measured" claims
survive on twin B's branch — `tiller_theme/src/lib.rs:917`, the `radii_match_waku` test and its doc
at `:2115-2117`, and `conformance.rs:1-3` and `:63` — in files whose commit message asserts a full
sweep. Twin A renamed every one of those. So the merge is not "B, plus A's regression test": it is
**B as the base for the values and the scale factor, plus A's renames and its `include_str!`
provenance regression test**, which is precisely the artefact that would have caught the four
survivors automatically.

## Standing conclusions

1. **Scale factor: 2.00x**, on three independent corroborations that agree:
   - the Big-Sur-onward traffic-light spec — 28 px / 14 pt and 46 px / 23 pt, both exactly 2.000;
   - the rendered body-text **line pitch**, 42 frame px, which is exactly 21.0 logical at 2.00x and
     matches `Typography::body_line_height`'s existing 21.0. It resolves to a round number only at
     2.00x, which is what makes it discriminating rather than merely consistent;
   - the sidebar **card height**, 102 frame px against `CARD_TWO_LINE_HEIGHT` 51, giving exactly
     2.000 in both frames — the check twin A found and discarded.

   Three different features of the frame, three different reference values, one answer. Note the
   raw pixel measurements were never in dispute: twin A, twin B and this adjudication each measured
   28 px and 46 px independently and agreed to the pixel. Only the divisor was ever contested.
2. **Content column: 720, decided.** The measurement gives a lower bound of ~676-686 logical px,
   consistent with 720 without uniquely confirming it — an ink-extent scan bounds the column from
   below, because word wrap breaks a line before the edge. Asked to choose among 720, ~550 and ~480
   on 2026-08-19, the user answered **«proviamo 720»**. So 720 stays, and it stays for a stated
   reason rather than by inheritance: it is inside the measured range and it is the user's choice.
   That closes the open question this remedy was carrying; `TRANSCRIPT_WIDTH`, `CONTENT_WIDTH` and
   `MARKDOWN_COLUMN_WIDTH` keep 720 and the provenance now reads "measured range, user's choice
   within it" — never "waku's source says so", which is the defect the gap was opened for.
3. **The user pill's cap is unmeasurable** — no user turn in either frame is long enough to engage
   it. Both twins reached this independently, which is the strongest single result of the exercise.
4. Every geometry value must record its origin as measured / derived-from-measured / ours-by-choice
   / unmeasurable. A value whose only justification is "waku's source says so" is the defect this
   gap was opened for.
5. Whatever happens to the values, the false provenance comments must be corrected. Both twins did
   this first, correctly, and independently.
