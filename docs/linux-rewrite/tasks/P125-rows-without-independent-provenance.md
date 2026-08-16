# P125 — 26 rows carry no independent critic-pass provenance

Found while chasing a discrepancy in `Scripts/ledger-totals.py`'s own diagnostic (it reported 47;
the true figure is 26 — see `16070ef`, which fixed a `fullmatch`/`search` bug that rejected 21 rows a
numbered pass had in fact judged).

These 26 are not rows with *weak evidence*. Most carry detailed live drives. They are rows where the
`judged` column names **the orchestrator itself, or a source whose independence is not structurally
guaranteed** — so the project's standing rule ("a feature the critic hasn't successfully tried does
not exist", and the critic is never the agent that produced the evidence) is not demonstrably
satisfied for them.

## The 15 PASSED rows — the ones that matter

They are counted in the ledger's 315 PASSED, so if any is wrong the headline number is wrong.

| row | judged by | note |
| --- | --- | --- |
| `F-EDIT-07`, `F-EDIT-10`, `F-EDIT-11` | orchestrator drive | real live drives, incl. a clipboard read-back with `xclip -o -selection clipboard` against the running app |
| `F-PRJ-05`, `F-PRJ-08` | orchestrator drive | Clone / Create forms driven from the `+` menu |
| `F-TAB-06` | orchestrator drive | supersedes pass 8 — "the browser exists now" |
| `F-CTRL-BROWSER-01` | orchestrator headless-lane probe | re-exercised after `P90` (`988d9e9`) |
| `F-GIT-CLONE-01` | orchestrator audit, grep evidence | rests on a pass-12 test, not on a live drive |
| `F-SID-07`, `F-SID-08`, `F-SID-09`, `F-CHG-16` | `P108 critic; P104-report` | a critic reading **another agent's report**, not exercising it |
| `F-TERM-03`, `F-TERM-PTY-05`, `F-TERM-SCR-02` | `critic2` | live drives, but stamped outside the pass vocabulary |

`F-SID-07/08/09` and `F-CHG-16` are the weakest of these: the recorded evidence is a critic
summarising `P104-report`'s sections, which is a builder's own write-up. That is the exact shape the
independence rule exists to reject.

## The 9 UNREACHABLE rows

`F-CORE-ACT-17/18/22/23`, `F-GIT-DIFF-03`, `F-GIT-STATUS-02`, `F-TERM-02`, `F-TERM-PTY-07/08`.
All are honest **downgrades** the orchestrator made when it found "zero app callers — wiring owed"
sitting behind a PASSED. They are conservative by construction, so re-judging them cannot inflate the
ledger — but each one names a **real unwired feature**, and several are the same architectural-gap
class wave E worked (`AttentionSort`, `DirectoryStatusAggregator`, `TerminalPaneCache<T>`).

## The 2 `N/A — platform` rows

`F-USE-04` and `F-USE-05` say in their own evidence: *"premise stated, scope decision pending — not a
settled exemption."* macOS's menu bar is Apple-only, but the capability the rows describe (a global
roster; clicking a row reopens its worktree) is not. **This is an open product decision, not a
verdict** — it should be settled deliberately rather than left as an exemption that quietly counts
toward the terminal 41.

## What to do

1. Re-verify the 15 PASSED rows with independent critics that exercise them live. Highest value: they
   are inside the headline count.
2. Settle `F-USE-04`/`F-USE-05` as a scope decision.
3. Treat the 9 UNREACHABLE rows as a feature backlog, not a verification backlog.
