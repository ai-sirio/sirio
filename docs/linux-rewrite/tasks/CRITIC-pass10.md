# Critic pass 10 — start converting the 127 that nobody has ever exercised

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately.
You built none of this.

## Pass 9 produced the number this project could not state

`INVENTORY-LEDGER.md`, 388 rows, one per entry:

| verdict | count |
|---|---|
| PASSED | **88** |
| half-proven | 46 |
| FAILED — absent | **90** |
| FAILED — defective | 4 |
| UNREACHABLE | 9 |
| N/A — platform | 17 |
| **NOT EXERCISED** | **127** |
| NOT EXERCISED — blocked on display | **7** |

And your corrections where the ledger beat the summary: `F-PER-06` down to **FAILED — defective**
(the process leak that its original narrow proof missed), `F-CHAT-24/25` to **FAILED — absent** (the
named cards do not exist), `F-SET-03` to **FAILED — defective** (a dead no-op button), `F-AUTO-02..08`
up to pass-3-confirmed, and the vague "≈196 never touched" resolved into a real **127 + 7**.

**One number reframes the whole day: the display blocks seven entries.** Seven of 388. That blockage
dominated hours of attention — a compositor diagnosis, four workarounds, three agents reporting it —
and it turns out to gate 1.8% of the contract, because the interaction tier was recovered through
GPUI's own test harness. The thing that felt like the wall was not the wall.

**The wall is the 127 nobody has ever looked at**, and the 90 features that were never built.

## The piece

Convert `NOT EXERCISED` into real verdicts, in bulk, starting with the largest blocks. This is the
only work that moves the project toward done, because **only your verdicts count** — a
builder-claimed entry is not done however good its evidence was.

Work from the ledger itself: take the `NOT EXERCISED` rows, group them by prefix, and go after the
biggest groups first. Most will be in `02-inventory-packages.md`, which is domain logic and
therefore headless by construction.

For each, the three questions in order — the discipline that caught the mega-row:

1. **Does the feature exist at all?** If not: `FAILED — absent`. No harness changes that, and
   distinguishing absent from defective is what keeps the build list honest.
2. Can it be exercised here?
3. Does it do what the `VERIFY` clause says — not what it plausibly does?

**Update the ledger row as you go**, rather than writing prose and reconciling later. The ledger is
now the primary artefact; `CRITIC-baseline.md` is where the reasoning goes, and it stays append-only.

Two cautions from your own findings:

- **Avoid crates being edited right now**: `tiller_ui/**` (chat surface), `tiller/src/main.rs` and
  `tiller_control/**` (window and sidebar commands), and the workspace-wide format/clippy sweep. A
  moving target wastes your verdict.
- **`builder-claimed, unverified` is not a verdict**, it is an absence of one. Where you have time,
  converting those is worth as much as a fresh entry — a builder proving its own work is exactly the
  case that produced the mega-row and the too-narrow `F-PER-06` proof.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass10
source ~/.cargo/env && cd /tmp/critic-pass10/rust && cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic10.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

Interaction is testable with no display: `TestAppContext`, `VisualTestContext`,
`.debug_selector(id)`, `simulate_keystrokes`, real mouse events — examples in
`tiller_ui/src/changes.rs`. **Harden any drawn test you rely on** with the full `run_until_parked()`;
you were the one who found the unhardened ones pass on timing luck.

**Never call an X tool.** Record the snapshot time. Check whether a file has moved before filing
against it.

## Reporting

Reply in **12 lines or fewer**: how many rows you converted and into what, the updated totals, the
largest remaining `NOT EXERCISED` group, and **the single biggest gap that is not the display** —
which, at seven entries, has stopped being the interesting answer.
