# P53 — J1's chat leg has no door at all

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P52 landed, and the reporting was right

`chat_transcript_survives_process_relaunch_with_tool_and_permission_outcome` is a proof a critic can
replay by name — that is the standard now, and you met it without being chased. Old-schema migration
passing matters as much as the feature: adding a table did not turn an existing database into an
unopenable one.

Two factual notes, neither a rebuke:

- The gate you ran stopped at `xcodegen: command not found`. That is `Scripts/ci.sh`, the **macOS**
  gate. This platform's gate is **`Scripts/ci-linux.sh`**. Use it and you will get a real answer
  instead of a missing-tool answer.
- The disk hit 100% at ~16:10 (eleven critic trees holding 339G of `rust/target`). 279G reclaimed,
  now 23%. If a build artefact looks nonsensical, it may be a partial write from that window.

## The piece: the door J1 stops at

The active journey is J1 (`05-the-design-of-the-program.md`, `DESIGN-LEDGER.md`). `codex12` built its
**project** doors, then stopped and said what it needed: *"a non-drawing Chat adapter/readback plus
transcript persistence."*

**You built the persistence half in P52. This is the other half.**

The current fact, verified in this checkout just now: `grep -rn '"chat\.' crates/` returns **nothing**.
Twenty-three socket methods are dispatched in `crates/tiller/src/main.rs`, twelve protocol variants
live in `crates/tiller_control/src/protocol.rs`, and **not one of them is a chat method**. So a
headless critic cannot start a chat, send a turn, watch it stream, stop it, or read the transcript
back — which means `D-J1` cannot be exercised past its project leg by anybody, ever, including after
`pi` finishes drawing the composer.

Build the **non-drawing chat session and its readback**: a chat that can be created, driven, streamed
from, stopped, and read, with no window and no element tree involved. Yours to place —
`tiller_acp` is the natural home and it is your crate.

It must cover the J1 steps that actually exist as behaviour:

- start a chat in a worktree, and read its state back
- send a turn; observe streaming output arrive incrementally rather than as one final blob
- **stop a turn in flight**, and read back that it stopped rather than completed — `F-CHAT-08` was
  audited false this hour: the composer's `↑` control only goes enabled/disabled and **never becomes
  stop**, so the stop *behaviour* has never been exercised at any layer. Do not assume it works.
- a tool call and a permission outcome appearing in the transcript — you already persist both
- read the transcript back, including after a relaunch (P52's proof, now reachable from outside)

## What you own, and what you must not touch

- **Yours**: `tiller_acp`, `tiller_persistence`, and the `chat.*` request/response variants in
  `crates/tiller_control/src/protocol.rs`. Add integration coverage in
  `crates/tiller_control/tests/control_integration.rs`.
- **Not yours**: `crates/tiller/src/main.rs` (`codex12`) and `crates/tiller_ui/**` (`pi`, mid-piece).

**Do not let this become another built-and-unwired layer.** Write the surface contract as you did for
P50 and P52 — `docs/linux-rewrite/tasks/P53-chat-door-contract.md`. `codex12`'s next piece after D2 is
now **one batched wiring piece discharging all three of your contracts** (P50 activity, P52
persistence, P53 chat) in `main.rs`. Three contracts, one owner, one piece — so name in your contract
exactly what `main.rs` must call, in the order it must call it.

## Evidence

An adapter with unit tests is not the piece. The proof is **an executed transcript**: a real process,
driven over the socket with `TILLER_SOCKET` set to your own endpoint, that starts a chat, streams a
turn, stops one, and reads the transcript back — the shape you used for the relaunch proof.

Name every proof by test function or transcript path so the critic replays rather than re-derives.
Mark closed rows `builder-claimed, unverified`, never `PASSED`.

Why that rule just got sharper: `fable` audited the 146 `PASSED` rows this hour and found **10 false**,
all by the same mechanism — every noun in a VERIFY clause existed somewhere in the file, so three true
facts about three different things were assembled into one verdict about a control that never existed.
The measured false rate was **21% among rows judged by reading code, and ~0% among rows judged by
executing a transcript**. A verdict cannot outrun its transcript. That is the entire reason this brief
asks for an executed one.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Run **`Scripts/ci-linux.sh`**. If it is red from other agents' in-flight work, report it honestly as
  blocked-by-others; do not fix foreign files. Your own crates' drift is yours.
- **Proceed without asking for design approval.** Where the session type lives, what streaming looks
  like on the wire, and how stop is represented are yours to decide and state.

## Reporting

**12 lines or fewer**: where the session lives and why, how streaming and stop are represented on the
wire, the executed transcript **by name/path**, what went into the contract file, the `ci-linux.sh`
result, and the honest remainder.
