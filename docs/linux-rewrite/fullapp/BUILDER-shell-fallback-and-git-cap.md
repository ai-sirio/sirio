# F-TERM-PTY-04 and F-GIT-RUN-01 — live drives

Requested by team-lead: two rows where the code had already landed and only
the live drive was owed. Own worktree `/var/tmp/tt-termgit-1248215469`
(branch `verify/pty04-gitrun01-1248215469`), off `origin/linux/gpui-waku` at
`6f4de662`, own build dir (`CARGO_TARGET_DIR=/var/tmp/tt-termgit-1248215469-target`).
Not pushed, ledger untouched.

Both results are **positive** — nothing softened, nothing to report as a
negative this time.

## F-TERM-PTY-04 — the shell fallback

Question asked, verbatim: does a real System pane get a working shell when
`$SHELL` is unset? Plus a judgement call: is preferring `/bin/bash` over the
passwd-file shell the right Linux answer?

**Setup.** `/bin/zsh` confirmed absent on this box (`ls /bin/zsh` →
"File o directory non esistente") — the same unfakeable discriminator the
code comment relies on. Launched Tiller with `env -u SHELL` wrapped around
the whole `wayland-drive.sh` invocation, so the app process itself never saw
`$SHELL` at all (confirmed indirectly: see below).

**Drive.** `ctrl-t` (`WindowCommand::NewTerminalTab` — the same
`open_action(NewTabAction::NewTerminal, …)` path the `+` menu's "New
Terminal" item drives, not a side door) opened a System pane. Clicked into
it, typed two commands, pressed Return after each:

```
ps -p $$ -o comm=,args=
```

Output, read directly off the rendered pane
(`01-terminal-fallback-shell-and-ps-proof.png`):

```
bash            /bin/bash -il
```

That is the shell's own process, from inside itself, naming its own comm and
full argv — not an inference from a title or a banner. It matches
`default_system_shell()`'s Linux arm exactly: program `/bin/bash`, args
`["-il"]`. The same frame's fastfetch banner (started automatically by this
box's shell rc) independently reports `Shell: bash 5.2.21` via its own
`/proc` inspection, a second, differently-sourced confirmation. Had `$SHELL`
leaked through from somewhere, either line would have named something else
— it did not, and the exact fallback tuple appearing verbatim is itself
evidence that `$SHELL` was truly absent for this process, not just
unset in my own shell.

The pane was genuinely interactive throughout: it accepted the typed
command, executed it, printed correct output, and returned to a fresh
prompt — not a static placeholder that merely looks like a shell.

**Result: holds.** A real, working, interactive login shell appears when
`$SHELL` is unset on Linux, exactly as `default_system_shell()` intends.

**The judgement call, since team-lead asked for an opinion and not
agreement.** I don't think the current fallback is wrong, but I don't think
it's the most idiomatic Linux answer either, and I'd rank the gap as a
legitimate follow-up rather than a defect:

- Most real Linux terminal emulators (xterm, gnome-terminal, alacritty,
  kitty) fall back to the user's `/etc/passwd` shell entry when `$SHELL` is
  unset, not to a hardcoded binary. That's the more "correct" Linux
  convention, and it respects a user who deliberately runs zsh/fish but
  launched Tiller from a context that didn't set `$SHELL` (a bare `exec`
  from a compositor/session manager with no full PAM/login chain is a
  realistic way to reach exactly that state for a GUI app, not an exotic
  edge case).
- But a passwd read isn't free of its own failure modes: a locked account
  (`/usr/sbin/nologin`), a stale entry pointing at an uninstalled shell, or
  no NSS/passwd database at all in a minimal container (this crate's own
  test comment already worries about environments this bare). Any passwd-
  based fallback still needs the same bash-then-sh safety net underneath it
  for when the passwd entry itself is unusable.
- The code comment calls this "the last OS-specific assumption in the PTY
  spawn path" — a deliberately minimal function. The current bash-then-sh
  chain fully fixes the crash this row exists for (never fails to spawn on
  Linux) with no NSS dependency and no nologin-filtering logic to get
  wrong. That's a reasonable, low-risk floor.

My actual recommendation: add the passwd lookup as an *earlier* step —
`$SHELL`, then the passwd entry (if present, non-empty, and not a
nologin-style shell), then bash, then sh — rather than leave it out or
replace what's there. That's strictly better than today's chain and matches
real terminal-emulator convention, but it's more surface (NSS availability,
nologin filtering, error handling) for a function the codebase deliberately
keeps minimal, so I'd call it a worthwhile follow-up, not something this
row's fix is deficient for shipping without.

## F-GIT-RUN-01 — the output cap and the drain

Question asked: does the cap hold on a real diff that exceeds a deliberately
lowered limit, and — the part team-lead flagged as the actual point — does
the reader keep draining instead of leaving git blocked on a full pipe.

**Fixture.** Fresh repo, one commit, then:
- `huge.txt` — every line replaced with 60,000 unique filler lines, unstaged.
  `git diff -- huge.txt` measured at **3,540,144 bytes** — comfortably past
  both the test cap below and a single 64 KiB pipe buffer, which is the
  number that matters: an old "stop reading at the cap" implementation would
  leave git blocked mid-`write()` once the kernel pipe buffer fills, long
  before hitting the cap itself.
- `normal.txt` — one line added, unstaged — a normal small diff as an
  in-frame contrast.

Launched with `TILLER_GIT_OUTPUT_LIMIT_BYTES=4096` — deliberately far below
both the real diff size and the pipe buffer.

**Timing.** Opened the Changes tab and polled `ctl surface.changes.read`
(the real control-socket path `control_read_changes` → `ChangesTab::report`,
not a paraphrase) with wall-clock timestamps bracketing it:

| event | time |
|---|---|
| before `surface.changes.open` | `1787183812.860583089` |
| `ready:"true"`, both files' stats correct | `1787183813.494384098` |

**0.634 seconds** from open to a fully loaded, correct report. `git status`
in `load_snapshot` runs a sequential `for entry in &entries { diff_entry(…) }`
loop, so if the huge.txt read had actually blocked on a full pipe it would
have stalled the *entire* tab — including normal.txt's own counts — for the
full `DEFAULT_GIT_TIMEOUT` (10 seconds) before surfacing `TimedOut` instead
of `OutputTruncated`. It did not: both files' additions/deletions
(`huge.txt`: −1/+60000, `normal.txt`: −0/+1 — both numerically correct)
were present and correct in well under a second
(`02-changes-tab-loaded-fast-both-files-correct-stats.png`).

**The message.** Expanding huge.txt's row
(`03-huge-diff-truncated-message.png`):

```
diff unavailable: git diff --no-color --no-ext-diff --unified=24 HEAD -- huge.txt produced more than 4096 bytes on one stream, so its output was truncated and cannot be parsed
```

That's `GitError::OutputTruncated`'s `Display` impl, verbatim, reaching the
UI exactly as `changes.rs`'s `load_snapshot` comment says it should: a
per-file failure recorded in `diff_errors`, not a panic, not a blank
expansion, not a silently-short diff read as a real one.

**The contrast.** Expanding normal.txt's row in the same frame
(`04-huge-truncated-and-normal-diff-ok-side-by-side.png`) shows a normal,
correct unified diff (`@@-1,2 +1,3 @@`, the added line highlighted) sitting
directly under huge.txt's truncation message — the cap doesn't degrade an
ordinary small diff sitting right next to a pathological one.

**Result: holds**, on both halves team-lead called out as the actual point
— the cap fires and is reported the way the buffered path's design commits
to (`Err`, not a silently-partial `Ok`), and the drain means hitting that
cap costs milliseconds, not the ten-second timeout a blocked pipe would
cost.

**What I did not drive.** I used the diff path only, not a clone —
team-lead's ask accepted either ("a real diff or clone"), and diff is the
one wired into everyday traffic (the Changes tab, on every refresh). The
streaming runner's opposite contract (`GitCommandResult.truncated: bool`,
`Ok` rather than `Err`) is real in the source and covered by its own unit
tests, but I did not re-drive a streaming call (e.g. a clone) live myself —
saying so rather than implying I covered both paths equally.

I also did not reach `tiller_project`'s duplicate, uncapped git runner —
team-lead flagged it as an adjacent finding, not mine to chase, and nothing
in either drive exercised that crate.

## One thing worth recording: a label collision, not a bug

An earlier pass at this same drive, under labels `tpty1248`/`tpty1248b`,
showed the Projects sidebar populated with dozens of unrelated project
names — matching real worktrees from elsewhere in this engagement, not
anything I added. Checked before treating it as a finding: my own file
writes stayed confined to that label's own `/tmp/tpty1248*.sqlite`
(confirmed by listing it directly — no shared or home-directory Tiller
state was touched), and I never clicked any of those rows. Re-running under
a fresh label (`tpty1248d`) produced a clean sidebar with only my own
project, which is the frame actually cited above. Most likely a label I
picked collided with another agent's concurrent session reusing the same
short string — not investigated further since it didn't touch anything
shared and isn't part of either row.
