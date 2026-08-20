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
`verify/streaming-truncation-3262725439`, rebased onto `c8192485` (picked
up the `kill_ours` harness fix — no Rust changed between `90329a22` and
`c8192485`, so the already-built binary didn't need a rebuild). Own build
dir. Fresh label per drive, never reused. Not pushed, ledger untouched.

**Answer: the flag is discarded, and it is discarded before it can go
anywhere — this isn't "nothing downstream reads it yet," it's structurally
impossible for anything to read it as the code is written.**

## Finding the path

Two production call sites reach `GitRunner::run_streaming*`. One
(`git.rs:787`) only exists inside a cancellation test, running a fake shell
script — not reachable from the app. The other is real:
`GitClone::clone` (`tiller_git/src/clone.rs:33-38`):

```rust
GitRunner::run_streaming(&arguments, parent, move |line| {
    if let Some(progress) = Self::parse_progress(&line) {
        on_progress(progress);
    }
})?;
Ok(())
```

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
outside tests.

**A second thing worth naming, because it changes what "past the pipe
buffer" should even mean here:** `read_stderr_lines` (`git.rs:396-439`)
decouples the live `on_line` callback from the byte cap entirely —
`events_tx.send(StreamEvent::StderrLine(line))` fires for every line
regardless of whether `captured` has already hit `limit`, because the
progress callback is what drives the UI's live percentage and was never
meant to depend on the retained buffer. Only the *separately* retained
`captured: Vec<u8>` (the thing nobody ever reads back) is capped. So even
if the flag survived to the UI, a capped run wouldn't show up as garbled or
truncated progress — the progress bar is immune to the cap by construction.
The `truncated` flag's only possible observers were always "the returned
`GitCommandResult`," and that value dies in `clone.rs` before it can be
observed at all.

## Fixture — and an honest miss against last time's bar

Same discipline as the buffered-path drive: size the output so it clears a
64 KiB pipe buffer, not merely the configured cap, so the drain itself is
under test. It didn't work the same way here, and that's worth recording
rather than glossing over.

`git clone --progress`'s stderr is **percentage-throttled, not
volume-scaled** — it reports 0-100% per phase regardless of how much data
or how many objects that percentage represents. Measured directly rather
than assumed, clone-ing progressively larger local fixtures with
`--no-local` (forcing the real object-transfer path instead of git's
hardlink shortcut):

| fixture | wall time | stderr bytes |
|---|---|---|
| 4,000 tiny files | ~2s | 17,850 |
| 24,000 tiny files | ~2s | 18,511 |
| 480 MB of random blobs (60 files) | 30.7s | 23,870 |
| 1.75 GB of random blobs (220 files) | 119s | 32,389 |

That's a plateau, not a scaling curve — 15x the wall-clock time bought
roughly 8 KB. Reaching 64 KiB this way would cost several more minutes of
clone time for a diminishing-returns extrapolation, which isn't a
reasonable use of the box for one data point. **I did not clear 64 KiB of
real stderr from this call site**, and I'm not going to imply otherwise.
What I used instead: the largest fixture above (1.75 GB source, ~32 KB of
real progress text) against `TILLER_GIT_OUTPUT_LIMIT_BYTES=1024` — about
32x the cap, comfortably enough to trigger `truncated` — plus the
structural reading above (the drain loop's `read()` call has no
cap-conditional early exit, matching the exact pattern the buffered path's
`read_capped` already proved live) as the substitute for the specific
"would this ever leave a pipe undrained" question at *this* call site.

## The drive

`TILLER_PROJECTS_DIR` redirected under `/var/tmp` (the real default is
`$HOME/Tiller/projects` — had to override it so a real clone never lands
in the user's actual home directory). `TILLER_GIT_TIMEOUT_MS=200000`, since
the default 10s deadline is well under this fixture's real clone time — the
first attempt at this drive hit exactly that
(`02-timeout-error-propagates-correctly.png`: "Clone failed: git clone
--progress --no-local … did not finish within 10s and was killed"), which
is a useful accidental control: `GitError::TimedOut` **did** reach the UI
correctly through this same call chain. That confirms error propagation
through `CloneForm::submit` works in general — the gap is specific to the
`Ok(GitCommandResult{truncated: true, ..})` success case, not a general
"nothing from clone ever reaches the UI."

With the timeout raised: opened the form (`01-clone-form-empty.png`), typed
the local fixture path into `clone-url-field`, pressed Return. At 105s the
form was still genuinely running (`03-clone-in-progress-live.png`,
"Cloning… 0%" — this repo spends most of its time in local compression
before the transfer phase the progress bar tracks). It finished sometime
before the 150s mark.

**Result** (`04-clone-succeeded-no-truncation-indicator.png`): the project
`tt-streamcap-3262725439-clonesrc` appears in the sidebar with its `master`
worktree, marked Primary. The clone form is gone — `CloneFormEvent::Cloned`
fires on success and closes it. **No error, no warning, no truncation
indicator anywhere.** Verified the clone itself is genuinely intact despite
the capped capture (git writes the real repository content to disk
independent of what the reader retains for progress purposes, so this was
expected, but checked rather than assumed): `git status --short` clean,
`git log --oneline` shows all 4 source commits, `git fsck --full` reports
nothing.

So: the flag was (with very high confidence — same command, same fixture,
same measured ~32KB baseline, 32x the configured cap) `true` for this run,
and a person looking at the app has zero way to know that. That's the
finding — not "a flag nothing reads yet," but a flag that was thrown away
one line after being produced, with no return-type slot for a future reader
to ever add one without changing `GitClone::clone`'s signature.

## One methodology note, not a code finding

Two probe attempts at this drive were launched with my Bash tool's cwd set
to the shared `tiller-linux` checkout (`/home/enzopalmisano/…/tiller-linux`
— the repo this whole engagement runs from). Tiller auto-registered that
directory as a project and mounted real Chat/Terminal tabs rooted there on
first launch, because `wayland-drive.sh` doesn't `cd` before exec-ing the
binary — it inherits the caller's cwd. Checked immediately, before touching
anything else: `git status --short` in that repo was clean and nothing
under it had a recent mtime, so nothing was written. Fixed by always
launching from `/var/tmp` for the rest of this drive; recorded here in case
the same shape of surprise costs someone else a "wait, why does my sidebar
have a real project in it" moment. Not the same class of thing as the
label-collision note from the previous drive — that was stale state
carried by a reused label; this is cwd inheritance on a clean one.
