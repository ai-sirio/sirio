# Wave F slice F4-terminal — 3 rows, re-verification only

Every row below is currently **PASSED** and counted in the ledger's headline total, but its
`judged` stamp names the orchestrator itself or a source whose independence is not structurally
guaranteed. See `../tasks/P125-rows-without-independent-provenance.md`.

**You are re-judging these from scratch. Do not edit any code.**

## Rows

### `F-TERM-03` — ledger line 321, currently **PASSED**

- **Judged by:** critic2, reference/linux-progress/critic2-term03-{running,exit0,exit7,signal,final}.png
- **Evidence on record:** P82 (codex11) closed this. Live-verified all four VERIFY cases in one 4-tab fixture-DB drive: a long-running shell (no pill), `exit 0` (checkmark pill+badge "Success"), `exit 7` (warning pill+badge "Exit 7"), and a signalled process (pill+badge "Signal N") — matches the rendering code exactly (`tiller_terminal/src/lib.rs:1479-1495`: `TerminalExitStatus::label()` as an absolute bottom-left pill + tab badge)

### `F-TERM-PTY-05` — ledger line 530, currently **PASSED**

- **Judged by:** critic2, reference/linux-progress/critic2-pty05-{boot2,sent2,reply2,closed3}.png, critic2-pty05-lifecycle3-out.log
- **Evidence on record:** P82 (codex11) closed this. Live-connected a real coding agent (Codex CLI, not Claude Code — my own outer Claude Code session's env vars, e.g. `CLAUDECODE=1`, propagate to a nested `claude` child and make it silently exit, a testing-methodology confound distinct from a product defect; Codex has no analogous confound). Opened a fresh Codex tab, sent a prompt, got a live streamed reply ("PONG") within 3s with the Activity panel showing "1 running"; closed the tab, which raised a "Close dirty tab? Discard unsaved work in Codex?" confirmation (confirmed Close); `ps --ppid $APP_PID` showed the agent's PID present before close and **zero children after** — tab removed, Activity panel emptied. The 10 KiB/40-line content-matching cap is also covered by existing, currently-passing named unit tests (

### `F-TERM-SCR-02` — ledger line 528, currently **PASSED**

- **Judged by:** critic2, reference/linux-progress/critic2-scr02-debounce-tests.rs (exact test source), critic2-scr02-debounce-test.log
- **Evidence on record:** P82 (codex11) closed this: `OUTPUT_SETTLE_DEBOUNCE=200ms` (`lib.rs:552`), `TERMINAL_RESIZE_DEBOUNCE=120ms` (`lib.rs:511`), both real trailing-debounce mechanisms. No existing test counted callbacks across a burst, so wrote and ran two temporary executable tests matching the VERIFY clause (removed again afterward, not committed — see evidence file for the exact source): 5 resizes 15ms apart (≪120ms) reach the child PTY as **exactly 1** WINCH, final size = the LAST requested size, not an intermediate one; 400 lines across 2 bursts 50ms apart (<200ms) settle into **≤5** `OutputSettled` events, not ~400 — both prove trailing-debounce/coalescing, not per-byte/per-event, and passed 3/3 runs

