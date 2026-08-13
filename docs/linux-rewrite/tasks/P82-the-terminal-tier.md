# P82 — the terminal tier

**Owner: `codex11`.** Nine rows in your crate, two named as seams. Queue this after P81.

Read `../ENVIRONMENT.md`, `../OWNERSHIP.md` and `../SEAMS.md` first.

`tiller_terminal/**` is yours and it is not a headless crate — `TerminalElement` implements
`IntoElement` at `lib.rs:935` and there is a `render` at `lib.rs:1107`. Most of this piece is
buildable end-to-end without touching another pane's file. **Pane composition is not**: `panes.rs`
and `main.rs` belong to `codex12`, and two rows fall there.

## Start here: a row that is leaking processes

`F-TERM-08` — `FAILED — defective`, evidence: **"process-group leak on close/quit; no confirmation
prompt."**

This is the only row in the cluster that is actively doing harm rather than merely missing. Closing a
pane or quitting leaves process groups alive on the user's machine. **Fix it before anything else
here**, and fix the leak before the prompt — the prompt is a feature, the leak is a bug.

The teardown story connects to `F-TERM-PTY-07` below, so read both before designing either.

## Verified before this brief was written

Do not re-derive these; they were counted from the tree tonight.

| symbol | total refs | refs in app + UI | meaning |
|---|---|---|---|
| `TerminalExitStatus` | live at `lib.rs:98` | — | data exists, `ChildExited { status }`, `exit_status: Option<…>` |
| `terminal_file_drop` | 3 | **0** | dead control |
| `TerminalSurfaceHost` | **0** | 0 | genuinely absent |
| `TerminalPaneCache` | **0** | 0 | genuinely absent |
| `SEAM_WIDTH` | 4 | 4 — all in `main.rs` | **not your file** |

## The rows

### Dead controls — the fix is a surface or a caller, not a feature

- **`F-TERM-03`** — *see running, successful exit, nonzero exit, and signal status.* The ledger is
  explicit: exit-status data is **real** via `panel.state` (0/3/137), and `TerminalExitStatus` exists
  in your crate. **No surface renders it.** Do not rebuild the model; render it.
- **`F-TERM-PTY-06`** — `terminal_file_drop` exists with three references and **zero** in the app or
  UI. There is no drop-to-pane path reaching it. Again: a caller, not a feature.

That makes **sixteen** rows in this ledger with that shape. It is the project's most common defect
and the reason a row is phrased from the user's side: *"drop a file onto a pane"* cannot be satisfied
by a function that exists.

### Genuinely absent — real builds

- **`F-TERM-PTY-07`** — `TerminalSurfaceHost`: a stable host per terminal content ID, a **new
  generation on relaunch**, and defined teardown. Zero references today. The generation counter is
  the part that is easy to get subtly wrong and is what makes relaunch distinguishable from reuse.
- **`F-TERM-PTY-08`** — `TerminalPaneCache`: preserve pane controllers and PTYs when a pane **moves
  within a worktree**, and restore focus by pane/content ID. The abstraction is yours; whoever
  composes panes must call it, which is `codex12` — build it callable and name the seam.
- **`F-TERM-SCR-02`** — a **200 ms output-settle** debounce before forwarding to the activity model,
  and a **120 ms resize** debounce. Neither exists anywhere in the crate. These numbers are from the
  contract; do not round them.
- **`F-TERM-02`** — empty-pane prompt.
- **`F-TERM-PTY-05`** — the runtime loop: user input to the PTY, output appended to session and
  scrollback, content matching on the last 10 KiB. The pass-15 evidence is worth understanding
  before you build: `tillerctl panel write pane-1 "sleep 8"` produced frames at running (+1.5 s) and
  done (+10.5 s) that were **byte-identical**. Running and finished look the same, which points at
  the signal never reaching the surface — likely the same gap as `F-TERM-03`.

### One row that needs a ruling, not code

**`F-TERM-UI-02`** — *"Cmd-click terminal URLs are routed to the owning terminal view state rather
than globally"*, and the inventory itself flags `PLATFORM: the reference uses AppKit/Gh…`.

**GPUI's platform modifier is ⌘ on macOS and the Super key on Linux.** This project has already
produced a false `PASSED` by building a chord that no user could press. So decide the Linux gesture
first, write down what you decided, and only then build. Routing to the owning view state rather
than globally is the *substance* of the row and is platform-independent; the modifier is the part
that needs the ruling.

## The two seams — name them, do not reach

- **`F-TERM-SPLIT-01`.** Splits and layout persistence are already live (pass 4). The gap is that the
  divider is a **1-pixel seam against the reference's 6**, and `SEAM_WIDTH: f32 = 1.` lives at
  **`main.rs:179`**, used at `main.rs:3897` and `main.rs:5761`. That is `codex12`'s file. Register it
  in `SEAMS.md` as a one-constant change plus whatever else the row needs, and check the rest of the
  row's demands (recursive leaf/split hosts, cached leaf controllers, 50/50 initial fractions)
  against what pass 4 already proved before declaring anything absent.
- **`F-TERM-11`** — the no-worktree empty state is an app-level surface, not a terminal-crate one.
  Seam it.

## Done means

1. `F-TERM-08`'s leak fixed, with a test that would fail if processes leaked again.
2. `F-TERM-03` and `F-TERM-PTY-06` fixed as wiring, with the existing model untouched.
3. `F-TERM-UI-02`'s platform ruling written down before its code.
4. Both seams registered in `SEAMS.md` with the change named in one line each.
5. `Scripts/transplant-check.py` run, with **your files' status** stated — the count is 46
   pre-existing, so absence of your files is the target, not zero. A terminal element is close to
   Zed's own territory; write every line.

**The critic will run a command that exits non-zero and look at the pane.** If the frame looks the
same as a command that succeeded, the row fails no matter what the model holds.
