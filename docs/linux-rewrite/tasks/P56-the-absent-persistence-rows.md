# P56 — The eight absent persistence rows, in your own crate

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P52 and P53 both landed, in the right tree

Migration **v6** with an additive `chat_turn` table, `tiller_acp::ChatSession` non-drawing with
streaming, permissions, stop state, queueing and restore, and six socket builders under
`surface.chat.*`. Two proofs by name — the relaunch one and
`chat_door_streams_stops_and_restores_transcript_over_a_real_socket`. Verified present at
`crates/tiller_acp/src/chat.rs` and `crates/tiller_control/tests/control_integration.rs` on
`linux/gpui-waku`.

One detail worth keeping: `CURRENT_SCHEMA_VERSION = MIGRATIONS.len()`. Deriving the version from the
list instead of hardcoding it means the next person cannot add a migration and forget to bump. That is
the kind of choice that removes a future bug rather than fixing a present one.

**J1's spine is now complete at crate and socket level.** What remains is `main.rs` wiring — `codex12`
has both your contracts and will take them as one batch — and the composer, which is `pi`'s.

## The piece

Eight `F-PERSIST` rows are `FAILED — absent` and every one of them is in **your** crate. Take them.

- `F-PERSIST-DB-02` — absent: legacy tabs, agent accounts, browser content
- `F-PERSIST-DB-03` — worktree schema has no comment or timestamp columns
- `F-PERSIST-DB-05` — "no legacy terminal tab records; **no chat session/item persistence anywhere**"
- `F-PERSIST-DB-06` — no agent-account table
- `F-PERSIST-DB-07` — corrupt state is dropped, **never quarantined** (whole-file validation only)
- `F-PERSIST-DB-08` — `is_primary` has no exclusivity index; no exact-path worktree lookup in the store
- `F-PERSIST-DB-09` — no per-record corrupt-tab skip
- `F-PERSIST-DB-11` — the named chat/session and browser-content migrations are absent

**Read `F-PERSIST-DB-05` again: your own P52 closed half of it thirty minutes ago.** It says chat
persistence exists nowhere; `chat_turn` now exists. So begin by separating the rows that are genuinely
absent from the ones that are **stale in your favour**, and say which is which. Do not silently
re-mark: state the row, the evidence, and let the critic apply it.

Three of these are about the same idea and are worth treating as one design decision rather than three
patches: **DB-07 (quarantine instead of drop), DB-09 (per-record skip instead of whole-file reject),
and DB-11 (named migrations)** all concern what happens when stored data is not what the code expects.
Whole-file validation means one corrupt row costs a user their entire layout. Decide the policy, state
it, then implement it once.

`is_primary` without an exclusivity index (DB-08) is a data-integrity hole, not a feature gap: nothing
stops two primaries existing, and every reader then picks arbitrarily.

Where you judge a row should stay absent — an agent-account table nothing needs, say — **argue it and
leave it absent**. A row closed by building something no caller wants is exactly the dead-model defect
`pireview` just found seven instances of. Building for the ledger rather than for the program is the
failure mode here; say "not needed, because …" and move on.

## A stale-ledger fact you should know

Six `F-GIT` rows (`RUN-02`, `BRANCH-01`, `CLONE-01`, `REMOTE-01`, `STATUS-02`, `DIFF-03`) are still
marked `FAILED — absent` from **pass 5**, but P41 built all six: `crates/tiller_git/src/` now holds
`branches.rs`, `clone.rs`, `remote.rs`, `directory_status.rs`, `side_by_side.rs` and streaming in
`git.rs`. They are stale in the builder's favour. That is routed to `pireview`; you do not need to
touch them. It is here so you expect staleness in your own rows too, and check rather than assume.

## Evidence

Named tests, and for anything touching stored data a **real relaunch** — the shape you have used since
P28. Recovery behaviour specifically needs a database that is actually corrupt, not a mocked error:
write a bad record, reopen, and show the good records survived.

Mark rows `builder-claimed, unverified`, never `PASSED`.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm with
  `git branch --show-current` before you start.
- The gate is **`Scripts/ci-linux.sh`**, never `Scripts/ci.sh`.
- **Do not edit** `tiller_ui/**` (`pi`), `tiller/src/main.rs` (`codex12`), or `tiller_control/src/`
  beyond what your rows require — `codex12` is fixing five live control defects there right now.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Proceed without asking for approval.** The corruption policy and every keep-it-absent judgement are
  yours to make and state.

## Reporting

**12 lines or fewer**: which rows were genuinely absent versus stale in your favour, the corruption
policy you chose and why, any row you deliberately left absent with the reason, your proofs by test
name, the `ci-linux.sh` result, and the honest remainder.
