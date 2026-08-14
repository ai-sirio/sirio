# Critic-2, first piece — verify P82 (the terminal tier)

**You are now the second critic, not a builder.** Read
`CRITIC-2-second-critic-handover.md` first — it defines the role, the four rules, and the list of
things you may not judge because you built them.

Worktree: `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`.

## The piece

Verify the claims of **P82 (`tiller_terminal`)**, built by `codex11`. You did not build it, so you
may judge it. **Do not read `codex11`'s reasoning or its pane output before judging** — start fresh,
from the ledger row and the running app.

Every line below is **builder-claimed**. None of it is a verdict yet. Only you can make it one.

| row | what `codex11` claims |
|---|---|
| `F-TERM-08` | process-group leak closed on shutdown — `SIGTERM` with `SIGKILL` fallback; `TerminalView::Drop` covers entity close |
| `F-TERM-03` | stale row — status and exit surface already existed |
| `F-TERM-PTY-06` | dead control closed — file drop routes through `tiller_project::terminal_file_drop`, inserts without a trailing newline |
| `F-TERM-PTY-07` | implemented in `tiller_terminal/src/lifecycle.rs` |
| `F-TERM-PTY-08` | implemented in `tiller_terminal/src/lifecycle.rs` |
| `F-TERM-SCR-02` | debounce is exactly 200 ms output / 120 ms resize |
| `F-TERM-02` | New Terminal / New… prompt emitting events |
| `F-TERM-PTY-05` | stale row — real runtime already present; matching now capped at 10 KiB / 40 rows |
| `F-TERM-UI-02` | terminal-local link router, Super/platform ruling recorded at `SEAMS.md:49` |

## The trap in this particular pass

`F-TERM-PTY-07`, `F-TERM-PTY-08` and `F-TERM-02` are **Half A**. Their mount into the shell
(`main.rs`, `panes.rs`) belongs to `codex12` and is still open — see the `SEAMS.md` Open table.

**A complete Half A is not PASSED.** If you cannot reach the behaviour from the UI as a person
would, the verdict is `half-proven` or `FAILED — absent`. Never `PASSED`. The whole reason this
project tracks seams is that finished-and-unreachable kept being recorded as finished.

## `F-TERM-08` needs no display

The leak is the one row here you can settle without the screen, and it is the row that was doing
active harm rather than merely missing:

1. Launch the app, open a terminal pane, and note the shell's PID and process group.
2. Quit the app (and separately: close the pane, and kill the app).
3. `ps -eo pid,pgid,ppid,stat,comm` and look for survivors in that process group.

An orphan that outlives the app is the defect. Check all three exit routes — a fix that covers quit
but not close is half a fix, and the row is about both.

## Sharing the display

The X display is a **single shared resource** and `fable` is driving it too. Two drivers corrupt each
other's captures silently: `xdotool` moves the one global pointer, so clicks land in whichever window
is under it while each driver still photographs its own window correctly. The result looks like a
real product bug and is not.

- Always drive through `Scripts/linux-drive.sh`, never raw `xdotool`.
- Set `TILLER_DRIVE_LABEL=critic2` and `TILLER_DRIVE_LOCK_WAIT=900`.
- The lock is a self-healing `mkdir` lock; it breaks a dead holder or one older than 30 min and
  prints `NOTE` when it does. If you see that note, believe it.
- **Batch long, not short.** Queue many actions into one drive rather than taking many small drives —
  every drive pays a full app launch and a turn of lock contention.
- If a capture shows an action that you did not perform, assume contention, not a bug. Re-take it.

Paint lag is real: the first capture after an action often shows the pre-action frame. Take the shot,
then take it again.

**Never name an in-script `shot` the same as the driver's primary output file** — the driver
overwrites that file with its own final capture when the run ends, silently replacing your
mid-sequence frame with the last one. That cost a run to diagnose.

## Recording the result

Write verdicts into `docs/linux-rewrite/INVENTORY-LEDGER.md` in the existing row format:

```
| `F-XXX-NN` | VERDICT | evidence | source |
```

Vocabulary: `PASSED` · `half-proven` · `FAILED — absent` · `FAILED — defective` · `UNREACHABLE` ·
`N/A — platform` · `NOT EXERCISED`. The denominator stays **389** — do not add or remove rows.

Evidence must name what you did and what you saw, and cite the capture file. "Builder says so" is not
evidence. A test passing is not evidence that a person can reach the feature.

Commit **by explicit path**, and run `git status --short | grep '??'` before you finish — new files,
especially your captures, are exactly what an explicit-path commit drops. Untracked evidence makes a
verdict unreplayable.
