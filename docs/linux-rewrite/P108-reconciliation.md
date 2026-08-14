# P108 reconciliation — census and drive reports

## Ledger totals — before
```text
INVENTORY-LEDGER.md: 389 F- rows

verdict counts, computed from the body:
  PASSED                              190
  half-proven                         25
  FAILED — absent                     82
  FAILED — defective                  32
  UNREACHABLE                         12
  N/A — platform                      12
  NOT EXERCISED                       35
  NOT EXERCISED — blocked on display  0
  builder-claimed, unverified         1
  TOTAL                               389

never independently judged by a critic pass: 116
  orchestrator drive, 2026-08-14    20
  orchestrator audit, 2026-08-14    15
  pass 19 (P92 live drive)          14
  orchestrator audit, grep evidence, 2026-08-14  9
  orchestrator subject-rule correction, 2026-08-14  8
  orchestrator socket probe, 2026-08-14  7
  fable drive, 2026-08-14, pass 18  5
  P96 headless lane, 2026-08-14     5
  orchestrator audit, existing frame applied, 2026-08-14  4
  builder-claimed, unverified       2
  orchestrator, live D-Bus capture + code trace, 2026-08-14  2
  pass 19 (P92 re-check)            2
  pass 14; re-checked orchestrator 2026-08-14  2
  critic2, SEAMS.md Open table + grep evidence  2
  P101 critic, 2026-08-14; `b2-notes-menu.png`; 900 s official-lock failure  1
  P101 critic, 2026-08-14; `/tmp/p101h.sock` two-read Changes exercise; official-lock failure  1
  pass 17 + orchestrator drive, 2026-08-14  1
  orchestrator, grep + code trace, 2026-08-14  1
  P101 critic, 2026-08-14; `/tmp/p101h.sock`; official-lock failure  1
  critic2, grep evidence + SEAMS.md Open table, 2026-08-14  1
  critic2, reference/linux-progress/critic2-term03-{running,exit0,exit7,signal,final}.png  1
  critic2, reference/linux-progress/critic2-term08-routeB-final.png, critic2-term08-routeC.png, ps evidence in session transcript  1
  P50 builder claim                 1
  orchestrator, code trace + live D-Bus, 2026-08-14  1
  orchestrator, code trace, 2026-08-14  1
  P101 critic, 2026-08-14; prepared fixture; official-lock failure  1
  P101 critic, 2026-08-14; static connection inspection only; official-lock failure  1
  orchestrator headless-lane probe, 2026-08-14 05:05  1
  orchestrator code-proof, 2026-08-14, awaiting live confirmation  1
  critic2, reference/linux-progress/critic2-scr02-debounce-tests.rs (exact test source), critic2-scr02-debounce-test.log  1
  critic2, reference/linux-progress/critic2-pty05-{boot2,sent2,reply2,closed3}.png, critic2-pty05-lifecycle3-out.log  1
  P101 critic, 2026-08-14; static connection inspection; official-lock failure  1
  P101 critic, 2026-08-14; static subscriber inspection; official-lock failure  1

Totals block matches the body.
```

## Ledger totals — after
```text
INVENTORY-LEDGER.md: 389 F- rows

verdict counts, computed from the body:
  PASSED                              190
  half-proven                         34
  FAILED — absent                     14
  FAILED — defective                  47
  UNREACHABLE                         12
  N/A — platform                      12
  NOT EXERCISED                       79
  NOT EXERCISED — blocked on display  0
  builder-claimed, unverified         1
  TOTAL                               389

never independently judged by a critic pass: 184
  P108 critic, 2026-08-14; RECENSUS Slice B  26
  P108 critic, 2026-08-14; P104-report  21
  orchestrator drive, 2026-08-14    19
  pass 19 (P92 live drive)          14
  P108 critic, 2026-08-14; RECENSUS Slice A  12
  orchestrator audit, 2026-08-14    11
  orchestrator audit, grep evidence, 2026-08-14  9
  P108 critic, 2026-08-14; RECENSUS + P106-report §Orchestrator correction  8
  orchestrator subject-rule correction, 2026-08-14  8
  orchestrator socket probe, 2026-08-14  6
  P96 headless lane, 2026-08-14     5
  fable drive, 2026-08-14, pass 18  4
  orchestrator audit, existing frame applied, 2026-08-14  4
  orchestrator, live D-Bus capture + code trace, 2026-08-14  2
  pass 19 (P92 re-check)            2
  pass 14; re-checked orchestrator 2026-08-14  2
  critic2, SEAMS.md Open table + grep evidence  2
  P108 critic, 2026-08-14; P106-report §F-WIN-07  1
  P108 critic, 2026-08-14; P106-report §F-SID-06  1
  P108 critic, 2026-08-14; P106-report §F-SID-11  1
  P108 critic, 2026-08-14; P106-report §F-TAB-01  1
  P108 critic, 2026-08-14; P106-report §F-TAB-23  1
  P108 critic, 2026-08-14; P106-report §F-CHG-01  1
  P108 critic, 2026-08-14; P106-report §F-CHG-20  1
  P101 critic, 2026-08-14; `b2-notes-menu.png`; 900 s official-lock failure  1
  P101 critic, 2026-08-14; `/tmp/p101h.sock` two-read Changes exercise; official-lock failure  1
  pass 17 + orchestrator drive, 2026-08-14  1
  orchestrator, grep + code trace, 2026-08-14  1
  P101 critic, 2026-08-14; `/tmp/p101h.sock`; official-lock failure  1
  P108 critic, 2026-08-14; P106-report §F-SET-15  1
  P108 critic, 2026-08-14; P106-report §F-SET-18  1
  critic2, grep evidence + SEAMS.md Open table, 2026-08-14  1
  critic2, reference/linux-progress/critic2-term03-{running,exit0,exit7,signal,final}.png  1
  critic2, reference/linux-progress/critic2-term08-routeB-final.png, critic2-term08-routeC.png, ps evidence in session transcript  1
  P50 builder claim                 1
  orchestrator, code trace + live D-Bus, 2026-08-14  1
  orchestrator, code trace, 2026-08-14  1
  P101 critic, 2026-08-14; prepared fixture; official-lock failure  1
  P101 critic, 2026-08-14; static connection inspection only; official-lock failure  1
  orchestrator headless-lane probe, 2026-08-14 05:05  1
  orchestrator code-proof, 2026-08-14, awaiting live confirmation  1
  critic2, reference/linux-progress/critic2-scr02-debounce-tests.rs (exact test source), critic2-scr02-debounce-test.log  1
  critic2, reference/linux-progress/critic2-pty05-{boot2,sent2,reply2,closed3}.png, critic2-pty05-lifecycle3-out.log  1
  P101 critic, 2026-08-14; static connection inspection; official-lock failure  1
  P108 critic, 2026-08-14; P106-report §F-TERM-SPLIT-01  1
  P101 critic, 2026-08-14; static subscriber inspection; official-lock failure  1

Totals block matches the body.
```

## Changed rows

| Row | Old | New | Source |
|---|---|---|---|
| `F-AGENT-API-01` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-BRW-08` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CHAT-02` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-16` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-18` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-21` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-22` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-23` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-31` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHAT-34` | FAILED — absent | NOT EXERCISED | RECENSUS + P106-report §Orchestrator correction |
| `F-CHG-01` | FAILED — absent | half-proven | P106-report §F-CHG-01 |
| `F-CHG-20` | FAILED — absent | half-proven | P106-report §F-CHG-20 |
| `F-CORE-ACT-24` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-ACT-25` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-ACT-26` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-DOM-03` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-FILE-08` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-SET-01` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-USG-06` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-USG-07` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-WSP-04` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CORE-WSP-08` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CTRL-CLI-02` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-PER-07` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-PRJ-03` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-04` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-07` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-10` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-11` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-12` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-17` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-PRJ-18` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-SET-11` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-SET-15` | FAILED — absent | half-proven | P106-report §F-SET-15 |
| `F-SET-16` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-SET-18` | FAILED — absent | half-proven | P106-report §F-SET-18 |
| `F-SET-21` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-SET-24` | FAILED — absent | NOT EXERCISED | RECENSUS Slice A |
| `F-SID-06` | FAILED — absent | half-proven | P106-report §F-SID-06 |
| `F-SID-11` | FAILED — absent | half-proven | P106-report §F-SID-11 |
| `F-SID-15` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-SID-16` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-SID-17` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-TAB-01` | FAILED — absent | half-proven | P106-report §F-TAB-01 |
| `F-TAB-11` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-TAB-18` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-TAB-23` | FAILED — absent | FAILED — defective | P106-report §F-TAB-23 |
| `F-TAB-24` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-TAB-28` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-TERM-SPLIT-01` | FAILED — absent | FAILED — defective | P106-report §F-TERM-SPLIT-01 |
| `F-USE-01` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-USE-02` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-USE-03` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-WIN-01` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-WIN-07` | FAILED — absent | half-proven | P106-report §F-WIN-07 |
| `F-WIN-10` | FAILED — absent | NOT EXERCISED | RECENSUS Slice B |
| `F-CHG-03` | half-proven | FAILED — defective | P104-report |
| `F-CHG-05` | PASSED | FAILED — defective | P104-report |
| `F-CHG-11` | FAILED — absent | half-proven | P104-report |
| `F-CHG-13` | FAILED — absent | FAILED — defective | P104-report |
| `F-CHG-16` | FAILED — absent | PASSED | P104-report |
| `F-CHG-18` | FAILED — absent | NOT EXERCISED | P104-report |
| `F-CHG-22` | FAILED — absent | half-proven | P104-report |
| `F-EDIT-02` | PASSED | FAILED — defective | P104-report |
| `F-EDIT-08` | PASSED | FAILED — defective | P104-report |
| `F-SID-07` | FAILED — absent | PASSED | P104-report |
| `F-SID-08` | NOT EXERCISED | PASSED | P104-report |
| `F-SID-09` | FAILED — absent | PASSED | P104-report |
| `F-SID-19` | FAILED — absent | NOT EXERCISED | P104-report |
| `F-TAB-02` | PASSED | FAILED — defective | P104-report |
| `F-TAB-12` | NOT EXERCISED | FAILED — defective | P104-report |
| `F-TAB-13` | FAILED — absent | FAILED — defective | P104-report |
| `F-TAB-14` | FAILED — absent | FAILED — defective | P104-report |
| `F-TAB-15` | NOT EXERCISED | FAILED — defective | P104-report |
| `F-TAB-16` | FAILED — absent | FAILED — defective | P104-report |
| `F-TAB-17` | NOT EXERCISED | FAILED — defective | P104-report |
| `F-TAB-21` | FAILED — absent | FAILED — defective | P104-report |

## Disputed

- None. Ambiguous report observations were conservatively recorded as `NOT EXERCISED`, not promoted.

## Owed a gesture on `DISPLAY=:1`

- `F-CHAT-02`
- `F-CHAT-16`
- `F-CHAT-18`
- `F-CHAT-21`
- `F-CHAT-22`
- `F-CHAT-23`
- `F-CHAT-31`
- `F-CHAT-34`
- `F-CHG-18`
- `F-PER-07`
- `F-PRJ-03`
- `F-PRJ-04`
- `F-PRJ-07`
- `F-PRJ-10`
- `F-PRJ-11`
- `F-PRJ-12`
- `F-PRJ-17`
- `F-PRJ-18`
- `F-SET-11`
- `F-SET-16`
- `F-SET-21`
- `F-SET-24`
- `F-SID-15`
- `F-SID-16`
- `F-SID-17`
- `F-SID-19`
- `F-TAB-11`
- `F-TAB-18`
- `F-TAB-24`
- `F-TAB-28`
- `F-WIN-01`
- `F-WIN-10`

## Genuinely absent — builder needed

- `F-AGENT-OMP-03` — RECENSUS ABSENT needle
- `F-AGENT-OPENCODE-03` — RECENSUS ABSENT needle
- `F-CHAT-28` — RECENSUS ABSENT needle
- `F-CHAT-29` — RECENSUS ABSENT needle
- `F-CHAT-30` — RECENSUS ABSENT needle
- `F-CHAT-32` — RECENSUS ABSENT needle
- `F-CHAT-35` — RECENSUS ABSENT needle
- `F-CHG-02` — RECENSUS ABSENT needle
- `F-SET-12` — RECENSUS ABSENT needle
- `F-SET-13` — RECENSUS ABSENT needle
- `F-SET-17` — RECENSUS ABSENT needle
- `F-SID-18` — RECENSUS ABSENT needle
- `F-TAB-25` — RECENSUS ABSENT needle
- `F-TERM-11` — RECENSUS ABSENT needle
