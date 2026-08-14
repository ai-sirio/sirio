# The exercise sweep — and the move off Herdr panes

**Decided by the user, 2026-08-14:** once the running Herdr panes finish their current
dispatches, close them and spawn every subsequent worker through the orchestrator's own
dynamic `Workflow` tool instead. The earlier standing rule — *"the designated panes are the
complete roster; never spawn subagents or workflows"* — is **revoked**. Do not reinstate it
from an older transcript.

## Why the shape of the fleet changed

Three properties of the Workflow lane decided the design, and each is a fact about this repo
rather than a preference:

1. **`Scripts/wayland-drive.sh` takes no lock and does not build.** It refuses to run without
   `rust/target/debug/tiller` (`exit 2`) and derives every `/tmp` path from `TILLER_WL_LABEL`.
   So N agents can drive N app instances concurrently — provided the binary is built *once*,
   before the fan-out, and no agent recompiles. `Scripts/linux-drive.sh` (`:1`) is the
   opposite: a single global mutex. Exactly one slice is authorised to take it.
2. **Workflow agents can see PNGs.** The adjudicators open the frames with `Read` and look at
   them, rather than accepting the driver's prose description of a frame. That closes the gap
   that made an earlier round of visual verification second-hand.
3. **One cargo target dir is shared by the whole tree.** Giving each agent its own git
   worktree would mean N cold builds; that once filled the disk. So the fleet shares the tree
   and is forbidden from editing `rust/` — this sweep produces evidence, never fixes.

## The denominator this sweep addresses

389 inventory rows. 190 `PASSED`, 12 `N/A — platform`, 12 `UNREACHABLE`, and **175 needing
work**. Those 175 split cleanly by what they need:

| need | verdicts | count |
|---|---|---|
| **a route** — the work exists, only the proof is missing | `NOT EXERCISED` + `half-proven` | **116** |
| construction — the behaviour is missing or wrong | `FAILED — absent` + `FAILED — defective` + `builder-claimed` | 59 |

This sweep takes the **116**. They are the cheapest conversions on the board and they need no
source edits, which is what makes a 7-way parallel fan-out safe. The 59 construction rows are
a later, differently-shaped workflow: they collide on source files and cannot be parallelised
the same way.

## The slices

| slice | families | rows | lane |
|---|---|---|---|
| `S1-core` | F-CORE | 22 | wayland |
| `S2-chat` | F-CHAT | 16 | wayland |
| `S3-settings` | F-SET, F-USE | 16 | wayland |
| `S4-sidebar` | F-PRJ, F-SID | 19 | wayland |
| `S5-tabs` | F-TAB, F-WIN, F-EDIT, F-PER | 13 | wayland |
| `S6-changes` | F-CHG, F-GIT, F-PERSIST, F-CTRL | 12 | wayland |
| `S7-term-brw` | F-TERM, F-AGENT, F-BRW | 18 | wayland + sole holder of `:1` |

Families are kept whole because rows in a family share a surface: one app launch and one drive
script can exercise a dozen of them.

## The two phases, and why they are two

Each slice is driven by one agent and adjudicated by **a different** agent. That is the
project's own rule — the critic is never the agent that built the piece — extended one step:
the critic is not the agent that *drove* it either, because a driver who scores its own drive
has every incentive to read a blurry frame generously. The adjudicator opens the same PNGs and
decides independently.

The two phases run as a `pipeline`, not a barrier: `S2-chat` can be under adjudication while
`S1-core` is still driving. There is no cross-slice dependency that would justify making the
fast slices wait for the slow one.

## Rules carried into every agent prompt

- **A green test is never a `PASSED`.** 134 tests passed over a chat transcript that drew
  nothing at all (`P117-report.md`); that defect is the standing proof of why.
- **Discriminating evidence.** When a row's fixed value and its default are the same value, a
  capture of that value proves nothing. Drive to a state the system would never reach on its
  own and capture a marker only the drive could have produced.
- **Positive controls bound false negatives only.** A control that fires proves the instrument
  is alive, not that its vocabulary is complete.
- **Captures land in the repo**, under `reference/linux-progress/sweep-<slice>/`. A proof that
  lives in `/tmp` is deleted by the next pass and the verdict resting on it becomes
  unreplayable.
- **Nobody but the orchestrator writes `INVENTORY-LEDGER.md`.** Seven agents adjudicating into
  one file is a check-then-write race; they emit structured verdicts and one writer applies
  them.

## Capacity, measured rather than assumed

Seven nested compositors sounded like the risk, so it was measured before being accepted.
One instance under `TILLER_WL_KEEP=1`, with processes identified by their **environment**
(`grep TILLER_WL_LABEL` in `/proc/<pid>/environ`) rather than by name:

| process | RSS |
|---|---|
| `target/debug/tiller` | 257 MB |
| `sway` | 51 MB |
| **one instance** | **307 MB** |

Seven instances ≈ 2.1 GB against 15 GB free on a 12-core, 32 GB machine. The Workflow
concurrency cap here is `min(16, CPUs − 2) = 10`, so all seven slices start at once and
memory is not the binding constraint. The probe was killed by the same env-var match that
found it — never by process name, per `WAYLAND-LANE.md` trap 4, because a name match would
have taken every sibling agent's app down with it.

## Pre-flight, in order

1. Wait for all four worker panes to leave `working`. Do not interrupt a live dispatch.
2. Close the panes.
3. `cargo build -p tiller` **and** `cargo test --workspace --no-run` — once, so no agent in the
   fleet ever compiles. The commit under test is recorded in each evidence file.
4. Launch the workflow.
