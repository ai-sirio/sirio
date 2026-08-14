# `sweep-D*` captures — orphaned frames, no verdict rests on them

The four `sweep-D1-settings`, `sweep-D2-chat-core`, `sweep-D3-term-brw` and
`sweep-D4-workspace` directories hold 213 frames captured on 2026-08-14 against HEAD
`4073297` by the first exercise-sweep's drive slices. **All four of those agents were killed
before they wrote a report**, so no frame here is named by any evidence file and no ledger
row cites one.

They are committed rather than deleted for one reason: they were captured from the same
commit the relaunched drive slices will test, so a later agent may be able to reuse one
instead of re-driving. Treat that as an offer, not a source.

**Do not cite a frame from these directories as evidence.** A capture whose drive nobody
recorded cannot be replayed, and a verdict resting on it would be exactly the unreplayable
proof this project's evidence standard exists to forbid. If a frame here looks useful,
re-drive the gesture that produced it and capture your own.

## Why the agents died

Every one of the 26 drive attempts ended at exactly 180.0 s of silence — the workflow
watchdog. The cause was not the drive lane and not context size: only 5 of the 26 stalls
followed a drive script at all, 17 followed instant commands like `grep -n … | head -20`,
and the two largest transcripts in the run (5.4 MB and 5.0 MB) belong to adjudicators that
finished successfully. What remains is model-side silence during deliberation. See
`docs/linux-rewrite/sweep/PLAN.md` §"What the first launch cost, and what it bought".
