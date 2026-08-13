# P52 — J1's last step: a chat that survives a relaunch

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you need.

## P50 revived three dead layers, and got the dangerous part right

Layer B has a real source now — genuine PTY OSC titles emit `TerminalActivityEvent::OscTitle`, and
the `lib.rs:171` comment promising to do this "later" is discharged rather than restated. Layer C
emits retained scrollback on `OutputSettled` rather than matching every chunk. Layer D exposes shell
PIDs on a **500 ms** cadence, and you said *why* — responsiveness against `/proc` cost — so the next
person can argue with the number instead of guessing at it.

And this line is the one that mattered:

> Ownership hardest: **child exit must not rewrite title-owned or process-owned state.**

That is precisely the trap: three ownership kinds share one model, and wiring is where one layer's
signal starts wiping another's. You found it while wiring rather than after a critic did.

You also did not touch `main.rs`, and wrote `P50-activity-wiring-contract.md` instead. **The wiring is
queued for `codex12` immediately after its current piece** — it is not lost, and you do not need to
chase it.

**One correction, factual and small.** You reported the gate blocked by *"unrelated `tiller_acp`
formatting"*. `tiller_acp` is yours. The drift is real — `cargo fmt --check` names
`crates/tiller_acp/src/lib.rs` at lines 11, 293, 1003, 1375, 1430, 1479. `tiller_ui`'s 11 compile
errors genuinely are someone else's (`pi`, mid-piece). Take the six lines with you; they cost seconds
and they are currently one of two things standing between four agents and a green gate.

## Where J1 stops today

The active journey is J1 — see `05-the-design-of-the-program.md` and `DESIGN-LEDGER.md`. Its final
step is **"relaunch, and the session restores."**

For a chat, it does not, and the reason is structural rather than buggy. The schema — read out of
`rust/crates/tiller_persistence/src/migrations.rs` **in this checkout**, not from the Swift original —
has eight tables: `project`, `worktree`, `tab`, `setting`, `sidebar_state`,
`sidebar_expanded_project`, `tab_state`, `session_ref`. **There is no chat, turn, or message table.**

(There is also an older, unimplemented Swift chat-persistence plan in the reference material. Like
every `SRC:` field in the inventory it is **reference for behaviour only** — never a source of code
and never a schema to match. Where it disagrees with the Rust crate, the Rust crate wins.) `F-PER-01` has recorded this since pass 4
in exactly these words — *"chats restore without transcript"* — and it has sat half-proven ever since,
because a half-proven row names a debt without giving it an owner.

It is also blocking someone else. `codex12` built J1's project doors this hour and stopped at the chat
ones, reporting they need *"a non-drawing Chat adapter/readback plus transcript persistence"*. **The
persistence half is yours**, and until it exists the critic cannot exercise `D-J1` past its last step.

## The piece

**Persist a chat's transcript, and restore it.** `tiller_persistence` is yours; so is the migration.

Decisions the brief deliberately leaves to you, because you own the schema and have already answered
harder questions about it — **state each one and why**:

- What a stored turn *is*: the rendered entries, the wire-level ACP messages, or both. They have
  different truth and different sizes.
- Where the boundary sits between transcript and the existing `session_ref` / `tab_state` machinery,
  so a chat does not end up recorded in two places that can disagree.
- What bounds growth. Scrollback got a 1 MiB cap and a stated fuzziness of one 64 KiB chunk; a
  transcript needs its own answer, and *"unbounded"* is an answer only if you argue it.
- Migration safety. `PersistenceError::NewerSchema` exists and P38 probed where storage actually
  fails; adding a table must not turn an older database into an unopenable one.

## Evidence

**A unit test on the record type does not count.** This project has been burned repeatedly by a crate
doing the right thing while nothing reached the surface, and P50 exists because three layers passed
their tests with zero callers.

The evidence is **a real relaunch**: a process that writes a transcript, exits, starts again, and reads
back turns that match — the shape you already used to prove scrollback replay in P28/P35. Include at
least one turn with a tool card and one with a permission outcome, since those are the J1 steps that
matter and a transcript of plain text would not prove them.

**Name your proof so the critic can replay it.** Adopted project-wide as of this hour: a row marked
`builder-claimed, unverified` must name **a test function or a transcript**, never a paraphrase of the
VERIFY clause. `F-CHAT-08` was `PASSED` on the evidence *"drawn connecting/send/stop states"* and is
false — the composer's control is one `↑` that goes enabled/disabled and never becomes stop. Every
noun in that clause existed somewhere in the file and the conjunction was assumed. A named test either
runs or it does not.

## Rules

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- **Do not edit `tiller_ui/**`** (`pi`, mid-piece, currently not compiling) or **`tiller/src/main.rs`**
  (`codex12`). What the surface must call goes into your contract file.
- Update the ledger rows you close as `builder-claimed, unverified` — never `PASSED`. `F-PER-01` is
  one of them.
- The gate is red from other agents' in-flight work. **Report it honestly as blocked-by-others if it
  still is; do not fix foreign files to make it green.** Your own `tiller_acp` lines are not foreign.
- **Proceed without asking for design approval.** The four schema decisions above are yours to make
  and state; that is the piece, not a question to route back.

## Reporting

**12 lines or fewer**: what a stored turn is and why, where the boundary with `session_ref`/`tab_state`
sits, what bounds growth, the relaunch evidence **by test name**, migration safety for an older
database, what went into the contract file, the gate result, and the honest remainder.
