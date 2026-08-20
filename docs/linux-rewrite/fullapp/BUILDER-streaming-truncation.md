# F-GIT-RUN-01, streaming half — does the `truncated` flag reach anyone?

Requested by team-lead: the buffered path (`GitError::OutputTruncated`) was
already driven and PASSED. The streaming path reports truncation the
opposite way on purpose — `GitCommandResult::truncated` is set and the
capture is still **returned**, not raised as an error. The question was
whether that flag survives to somewhere a person or the control socket can
observe it, or whether it is set and dropped on the floor between the
runner and the UI — the same shape of gap as F-CORE-FILE-01, where a green
unit test covered a comparator the app never called.

Own worktree `/var/tmp/tt-streamcap-3262725439`, branch
`verify/streaming-truncation-3262725439`, rebased onto `2bc8875b` (current
origin tip at drive time — picked up the neutral-cwd harness fix that grew
out of this drive's own methodology note; no Rust changed between
`c8192485` and `2bc8875b`, so the already-built binary didn't need a
rebuild). Own build dir. Fresh label per drive, never reused. Not pushed,
ledger untouched.

**Answer: the flag is discarded, and it is discarded before it can go
anywhere — this isn't "nothing downstream reads it yet," it's structurally
impossible for anything to read it as the code is written, and it is the
*only* production caller in the workspace, not one of several.**

## Finding the path

This isn't "one call site I happened to check" — it's every call site there
is:

```
$ grep -rn run_streaming rust/crates/ --include=*.rs | grep -v '/git.rs:' | grep -v '/tests/'
rust/crates/tiller_git/src/lib.rs:61:pub use git::{GitCancellationToken, GitCommandResult, GitRunner, run_streaming};
rust/crates/tiller_git/src/clone.rs:33:        GitRunner::run_streaming(&arguments, parent, move |line| {
```

Two lines, and one of them is the `pub use` re-export that makes the
function visible outside the crate — not a call. `GitClone::clone`
(`tiller_git/src/clone.rs:33-38`) is the *only* place `run_streaming` (or
`run_streaming_with_binary` / `run_streaming_cancellable`) is invoked
outside `git.rs`'s own tests anywhere in this workspace:

```rust
GitRunner::run_streaming(&arguments, parent, move |line| {
    if let Some(progress) = Self::parse_progress(&line) {
        on_progress(progress);
    }
})?;
Ok(())
```

(The remaining match inside `git.rs` itself, at line 787, is a
`run_streaming_cancellable` call from `git_cancellation_kills_the_child_and_reports_cancelled` — a test running a fake shell script, not reachable from the app.)

`GitCommandResult` — the value carrying `.truncated` — is bound to nothing.
`GitClone::clone`'s own signature is `Result<(), GitError>`; there is no
slot in the return type for the flag to leave in even if this function
wanted to keep it. One layer up, `tiller_ui/src/project_forms.rs:179-184`
(`CloneForm::submit`) does `.map(|_| worker_destination)` — discarding the
already-discarded `()` again, for good measure. There is no `ctl` surface
for clone at all (grepped the control dispatch in `tiller/src/main.rs` —
nothing).

This *is* reachable from the app, though, contradicting the "maybe no path
uses it" alternative team-lead named: Sidebar's "+" → "Clone Repository…"
(`start_clone_project`, `sidebar.rs:1565`) opens a real `CloneForm`
(`clone-url-field` / `clone-submit` debug selectors confirmed in source),
which is the one and only place `GitClone::clone` is ever called from
outside tests. So this is a real gap on reachable UI, not dead code nobody
can get to — the same shape as F-TAB-13's move-tab machinery, except here
the path to it is live.

## The drain question is settled by construction — the fixture doesn't need to answer it

The buffered path's drain guarantee (`read_capped`, proved live last round
with a fixture past the 64 KiB pipe buffer) and the streaming path's drain
guarantee are not the same code, and I initially treated not being able to
force a 64 KiB stderr capture through *this* call site as a shortfall
against that earlier bar. That framing was wrong. For this specific
question — does the streaming reader ever leave git blocked on a full pipe
— the source is the better instrument than a fixture could ever be: a
fixture proves the drain happened for *one* clone of *one* size; the loop's
structure settles it for *every* clone, at any size, forever, without
needing to be re-run.

`read_stderr_lines` (`git.rs:396-439`) reads in a plain loop with no
cap-conditional exit:

```rust
let count = match std::io::Read::read(&mut pipe, &mut buffer) {
    Ok(0) | Err(_) => break,
    Ok(count) => count,
};
let room = limit.saturating_sub(captured.len());
if count > room {
    captured.extend_from_slice(&buffer[..room]);
    truncated = true;
} else {
    captured.extend_from_slice(&buffer[..count]);
}
```

`room` only ever changes how much of *this read* gets copied into
`captured` — it never changes whether the next `read()` call happens. The
loop keeps calling `read()` until the pipe returns EOF or an error,
regardless of how full `captured` already is. That is what "drained by
construction" means: there is no code path where the reader stops pulling
bytes off the pipe because the cap was hit. It also decouples the *live*
per-line callback from the cap entirely — the `for &byte in &buffer[..count]`
scan that drives `events_tx.send(StreamEvent::StderrLine(line))` runs over
the full read every iteration, so the progress bar the UI shows is fed from
the uncapped stream even when `captured` (the buffer nobody ever reads back)
has already stopped growing.

One genuine gap, not a fixture-sized one: `git.rs:424`,
`if events_tx.send(StreamEvent::StderrLine(line)).is_err() { return; }`,
exits the loop without draining whatever is still in the pipe. That only
fires when the receiving end of the channel has already gone away — the UI
task dropped or the app is shutting down — not as a function of output
size or cap. It's worth naming because it's the one place "always drains"
isn't quite true, but it isn't a truncation-cap bug; it's a shutdown-race
one, and a different question from the one this drive was asked to answer.

## The fixture measurements — kept because they're the useful part, not the drain proof

The plateau below is real data and worth having on its own terms, even
though it no longer needs to answer the drain question: `git clone
--progress`'s stderr is **percentage-throttled, not volume-scaled** — it
reports 0-100% per phase regardless of how much data or how many objects
that percentage represents. Measured directly rather than assumed, cloning
progressively larger local fixtures with `--no-local` (forcing the real
object-transfer path instead of git's hardlink shortcut):

| fixture | wall time | stderr bytes |
|---|---|---|
| 4,000 tiny files | ~2s | 17,850 |
| 24,000 tiny files | ~2s | 18,511 |
| 480 MB of random blobs (60 files) | 30.7s | 23,870 |
| 1.75 GB of random blobs (220 files) | 119s | 32,389 |

That's a plateau, not a scaling curve — 15x the wall-clock time bought
roughly 8 KB. Reaching 64 KiB this way would cost several more minutes of
clone time for a diminishing-returns extrapolation. This is why a 64 KiB
pipe cannot be filled through this call site at any repo size anyone would
reasonably wait for — a fact about `git clone --progress`'s own output
volume, not a limitation of the harness or the drive.

## The drive

Since the drain question is settled by the source and the truncation flag
has no consumer regardless of its value, the fixture no longer needed to
be large — what was still worth proving live is that the clone path is
genuinely reachable end-to-end and renders real progress, not a synthetic
route. Used the smallest fixture that still shows visible in-progress
state rather than jumping straight from 0% to done: a fresh single-commit
repo with 14 × 8 MB random blobs (112 MB on disk), standalone `time git
clone --no-local` measured at 10.7s wall before touching the app.

`TILLER_PROJECTS_DIR` redirected under `/var/tmp` (the real default is
`$HOME/Tiller/projects`). First attempt at this smaller drive still left
`TILLER_GIT_TIMEOUT_MS` unset by mistake (a fresh shell per command losing
an earlier export, not a code issue) and hit the same default-10s deadline
as last round's first attempt — same accidental control as before,
`GitError::TimedOut` reaching the UI correctly as "Clone failed: … did not
finish within 10s and was killed." That partial clone left a stale
destination directory behind (caught and removed before the next attempt —
see the methodology note below). Re-ran with `TILLER_GIT_TIMEOUT_MS=60000`,
comfortably above the measured 10.7s.

Opened the form (`01-clone-form-empty.png`), typed the local fixture path
into `clone-url-field`, clicked "Clone repository". Captured a frame while
the clone was genuinely in flight (`03-clone-in-progress-live.png` —
"Cloning… 0%", live). It finished cleanly a few seconds later.

**Result** (`04-clone-succeeded-no-truncation-indicator.png`): the project
`tt-streamcap-3262725439-smallclonesrc` appears in the sidebar with its
`master` worktree, marked Primary. The clone form is gone —
`CloneFormEvent::Cloned` fires on success and closes it. **No error, no
warning, no truncation indicator anywhere** — there is no surface in this
UI that could show one even if the flag had survived to reach it. Verified
the clone itself is genuinely intact: `git log --oneline` shows the one
source commit, `git fsck --full` reports nothing, `du -sh` shows the full
112 MB (225 MB with git's own object-store overhead) present on disk.

## One methodology note, not a code finding

`wtype`'s default text-typing mode (`type "<text>"` in this harness, no
`-k`) sent zero characters to this specific text field across repeated
attempts — the URL field stayed on its placeholder every time, even though
the same mechanism has worked elsewhere in past drives. A single named key
(`key a`) landed reliably; a bare-string `type` of the same one or two
characters did not, and firing many `key <name>` calls back-to-back with no
delay between them dropped all but the first one or two. What worked
consistently: one `key <xkb-keysym-name>` per character (`/` → `slash`, `-`
→ `minus`, digits and letters literal) with a `sleep 0.2`-0.25s between
each. That's roughly 10-12s of pure typing overhead for a 46-character
path, paid once. Worth a line in `WAYLAND-LANE.md`'s trap table for the
next drive that needs to type an arbitrary string into a GPUI text field —
`key`-per-character-with-settle is the reliable path, `type` is not,
against this app.

Separately: the app's default project directory (`$HOME/Tiller/projects`)
turned out to already be an active shared scratch area from earlier rounds
of this same verification engagement (other builders' test project
directories dated two days prior were already there), not empty, untouched
personal storage — but it is still the *real* user directory, not something
this drive should write into, and an early attempt did land a stale
partial-clone directory there by mistake (a fresh Bash tool call losing a
previous call's `TILLER_PROJECTS_DIR` export, not a code issue). Caught by
checking the destination immediately after the failure that revealed it,
removed before continuing, and every later attempt in this drive correctly
set `TILLER_PROJECTS_DIR` under `/var/tmp` first. Also confirmed the app
now launches from a neutral cwd on its own (`2bc8875b`, merged since the
previous drive) rather than needing a manual `cd /var/tmp` before invoking
the harness, which was still done here as belt-and-suspenders.

## Verdict

**F-GIT-RUN-01 stays PASSED.** The row is about the buffered path, and that
was driven and confirmed last round — nothing here changes it.

The streaming finding is a separate fact, worth recording on its own
terms rather than folded into a row it isn't about: the streaming runner's
`GitCommandResult.truncated` is written, unit-tested
(`run_streaming_reports_a_truncated_capture`), reachable by drain-by-
construction on every path except a receiver-hangup shutdown race, and has
*zero* consumers in the shipping app — not "not yet wired up," but
structurally discarded by `GitClone::clone`'s own return type before it
can reach `CloneForm::submit`, which discards it again regardless. Same
shape as F-TAB-13's move-tab machinery — real, tested code the app cannot
act on — except this one sits behind UI a person actually reaches (Sidebar
"+" → "Clone Repository…"), so it's a live gap, not a dead corner. Whether
to wire it up — return it through `GitClone::clone`, surface it in
`CloneForm`, or decide a truncated progress capture never mattered to a
user in the first place — is a scope call for the user. Reporting it, not
fixing it.
