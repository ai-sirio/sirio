# wf-cancel — F-GIT-RUN-01: cancellation built and proven; a second, previously
# unnoticed gap (output limits / truncated-output) found and left for a fresh builder

Lane: `wf-cancel`. `CARGO_TARGET_DIR=/tmp/wf-cancel-target` was used for every build in this pass
(tiller_git is dependency-free apart from `libc`, so a private target dir is cheap for this one
crate and let this lane keep moving while the shared `rust/target` was locked by other agents for
most of the session — confirmed 12 concurrent `cargo build`/`cargo test` processes at one point,
`uptime` load average 64.7 on 12 cores). Binary was **not** pinned/live-driven this pass: this row
is machine-tier (GitRunner, no UI), so the evidence is executed tests, not a Wayland drive — see
EVIDENCE-STANDARD.md's tier table.

I am the builder. **This row is not `PASSED`.** See "What remains" at the bottom — a fresh critic
must drive that, not take my word for it.

---

## The row

> `F-GIT-RUN-01`: GitRunner runs `/usr/bin/git` asynchronously in a requested working directory,
> captures stdout/stderr, accepts configured exit codes, enforces output limits, and reports
> command failure, launch failure, CANCELLATION, timeout, or truncated-output conditions.
> PLATFORM: the absolute executable path is a reference assumption; Linux should resolve or
> configure git without assuming `/usr/bin/git`.
> VERIFY: Run successful, failing, missing, CANCELLED, and output-flooding git commands and inspect
> exit/error/truncation behavior.

Walking every clause against the tree as it stood before this pass:

| clause | state before this pass |
|---|---|
| runs asynchronously, requested cwd | green (existing tests) |
| captures stdout/stderr | green |
| accepts configured exit codes (`run_accepting`) | green |
| PLATFORM: no hardcoded `/usr/bin/git` | **already green** — `GIT_BINARY = "git"`, resolved via `PATH`. Confirmed by `grep -rn "/usr/bin/git" rust/crates/tiller_git rust/crates/tiller_ui rust/crates/tiller` → zero hits |
| reports command failure (`CommandFailed`) | green |
| reports launch failure (`Spawn`) | code existed, but **no test exercised it** anywhere in the crate |
| reports timeout (`TimedOut`) | green, `git_timeout_fires` |
| **reports CANCELLATION** | **absent — no code path anywhere in the workspace.** Confirmed again this pass: `grep -rln "cancel\|Cancel" rust/crates/tiller_git` → zero hits before this pass's edits |
| **enforces output limits** | **absent** — no byte cap, no size constant, no truncation logic anywhere in `tiller_git` |
| **reports truncated-output conditions** | **absent** — no `Truncated`-shaped `GitError` variant, no test |

The brief that dispatched this lane described the row as "everything except cancellation is green
(64/64 in tiller_git)" and asked me to build cancellation as the primary deliverable, with missing-
binary and output-flooding as optional stretch legs to *drive* ("only unit-tested"). Driving those
two legs live is what surfaced that the second one was never built at all, not merely undriven —
see "What remains".

## What was built

### `GitCancellationToken` + `GitRunner::run_streaming_cancellable`

`rust/crates/tiller_git/src/git.rs`:

- `GitCancellationToken` — a cloneable `Arc<AtomicBool>` handle. `cancel()` sets the flag from any
  thread, `is_cancelled()` reads it. Idempotent, no interior state beyond the one bool.
- `GitRunner::run_streaming_cancellable(binary, args, cwd, cancellation: Option<&GitCancellationToken>, on_line)`
  is now the real implementation. The existing `run_streaming` / `run_streaming_with_binary` are
  unchanged in signature and behavior — they delegate with `cancellation: None`, so every current
  caller (`GitClone::clone`, used by `project_forms.rs`'s `CloneForm`) is untouched.
- The runner's poll loop already wakes every `POLL_INTERVAL` (5 ms) to check its wall-clock
  deadline. Cancellation is checked on that same tick, before the deadline check: if the token is
  cancelled, the runner kills the whole process group exactly the way a timeout does
  (`kill_tree` → `killpg(SIGKILL)`, then `reap_within_grace`) and returns the new
  `GitError::Cancelled { command }` instead of the command's output.
- `GitError::Cancelled` added to `error.rs`, distinct from `TimedOut` (the runner's own deadline)
  and `CommandFailed` (git ran to completion and failed on its own), with a `Display` impl.
- `GitCancellationToken` re-exported from `lib.rs`.

**Why this scope, not the synchronous `run`/`run_accepting` family too:** `GitRunner` — the type the
row names — is today only the streaming struct (`run_streaming`, `run_streaming_with_binary`). The
free functions `run`/`run_accepting`/`run_with_timeout` used by status/diff/actions/branches/
worktree are `pub(crate)`, not part of `GitRunner`'s public surface, and back near-instant local
metadata reads where cancellation has no real caller-facing use case today (nothing in this
codebase runs one of those for long enough to want to cancel it — `git clone`, run through
`run_streaming`, is the one operation in this crate a user would ever wait long enough on to want to
stop). Extending cancellation to the synchronous family too is a small mechanical follow-up if a
caller ever needs it; I did not invent that caller.

**UI wiring — checked, not built.** `CloneForm` (`rust/crates/tiller_ui/src/project_forms.rs`) is
the one real caller of the streaming path and the natural home for a "Cancel" button: it already
spawns the clone on a background thread and polls a channel every 20 ms, so wiring a
`GitCancellationToken` through would be a small, mechanical change. I did not build it — the row is
about `GitRunner`, not about `CloneForm`, and the brief was explicit not to invent UI the row does
not ask for. Naming it here so it isn't lost: a fresh brief for `CloneForm` cancellation would be a
short, well-scoped follow-up, not a rebuild.

### Regression test: cancellation is real, not merely reported

`rust/crates/tiller_git/src/git.rs`,
`git::tests::git_cancellation_kills_the_child_and_reports_cancelled`:

1. Writes a fake git script (`/bin/sh`) that echoes its own `$$` to a pid file, then loops emitting
   `Receiving objects: N% (1234/5678)` to stderr every 100 ms for up to 5 s.
2. Runs it through `GitRunner::run_streaming_cancellable` on a worker thread with a fresh token.
3. Waits for **two independent proofs the process is genuinely running** before cancelling: the pid
   file exists on disk (the process actually started), and at least one progress line has arrived
   over the `on_line` callback (it is mid-flight, not merely spawned). This directly follows this
   session's own instructions: a cancel that raced a process which never truly started would prove
   nothing.
4. Calls `token.cancel()`, joins the worker thread.
5. Asserts **both halves of the clause**: the runner's `Result` is `Err(GitError::Cancelled { .. })`,
   *and* the captured pid is actually gone from the process table afterward
   (`libc::kill(pid, 0)` fails, polled for up to 2 s) — the reported outcome alone is not proof; the
   process has to actually be dead.

**Red, with the cancellation check disabled** (a `if false &&` guard placed in front of the check,
reverted immediately after capturing this transcript — never committed):

```
thread 'git::tests::git_cancellation_kills_the_child_and_reports_cancelled' panicked at
crates/tiller_git/src/git.rs:710:9:
expected Cancelled, got Ok(GitCommandResult { stdout: [], stderr: "Receiving objects: 0% (1234/5678)
\nReceiving objects: 2% (1234/5678)\n...Receiving objects: 98% (1234/5678)\n", exit_code: 0 })
test git::tests::git_cancellation_kills_the_child_and_reports_cancelled ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 25 filtered out; finished in 5.07s
```

This is the exact failure mode the row describes: with no cancellation path, `token.cancel()` does
nothing observable and the fake "clone" simply runs to completion (`exit_code: 0`) 5 seconds later —
proving the pre-fix runner genuinely has no way to stop a running operation, not just that the test
was mis-written.

**Green, with the fix restored:**

```
test git::tests::git_cancellation_kills_the_child_and_reports_cancelled ... ok
```

### Regression test: missing git binary reports launch failure

`git::tests::missing_git_binary_reports_spawn_failure` — points `run_streaming_with_binary` at a
path with nothing there and asserts `GitError::Spawn`. No existing test in this crate exercised this
leg at all (every prior test ran either the real system `git` or a fake script that itself existed
on disk).

### Full `tiller_git` test suite after the change

```
CARGO_TARGET_DIR=/tmp/wf-cancel-target cargo test --manifest-path rust/Cargo.toml -p tiller_git
```

```
running 27 tests   (lib)          -> ok. 27 passed; 0 failed
running 13 tests   (git_integration) -> ok. 13 passed; 0 failed
running 9 tests    (p41_git_behaviors) -> ok. 9 passed; 0 failed
running 6 tests    (p99_git_rows)   -> ok. 6 passed; 0 failed
running 11 tests   (worktree_integration) -> ok. 11 passed; 0 failed
```

66/66 (up from the pre-pass 64/64: +2 new tests, 0 regressions).

**Not run this pass:** a full `cargo build --workspace` / `cargo check -p tiller_ui -p tiller` to
confirm the app crates still build against the new `tiller_git` surface. Attempted twice
(`timeout 60 cargo check -p tiller_ui -p tiller`) and both times hit the shared `rust/target` lock
under heavy contention from other lanes and returned nothing before the timeout. The change is
purely additive — no existing public signature changed, `GitError` gained one new enum variant and
nothing outside `tiller_git` matches on `GitError` exhaustively (`grep -rn "match .*GitError" rust/crates/tiller_ui/src rust/crates/tiller/src` → zero hits) — but a fresh critic should still run
that build for real rather than trust this reasoning. Named explicitly below.

## What was investigated but NOT built: output limits / truncated-output

The brief's framing was that this leg was "only unit-tested" and needed live driving. That is not
what I found. Before touching anything, I grepped the whole workspace for every spelling I could
think of — `output.limit`, `OutputLimit`, `MAX_OUTPUT`, `max_bytes`, `truncat`/`Truncat`, `TooLarge`,
`byte_limit`, `oversize` — across `rust/**/*.rs` excluding `target/`. The only truncation-shaped
hits anywhere are unrelated: `remote.rs:62`'s `trimmed.truncate(...)` trims a trailing CRLF from a
URL string, and `diff.rs:451`'s 500 KB `line_count` cutoff is a file-on-disk line-counting heuristic
for the diff panel's "too large to diff" state — neither is `GitRunner` capping *process output* or
reporting a truncation *condition* to a caller. There is no `GitError` variant shaped like
"truncated" and no byte-budget constant anywhere near `read_to_end` in `git.rs`.

I proved this live rather than trusting the grep, per this session's own evidence standard (a
negative grep needs a positive control, and reading code cannot prove absence — only an executed
probe can). Built a 10,488,890-byte tracked file in a scratch repo and ran it straight through the
production `GitRunner::run_streaming` entry point (the same one `GitClone` uses):

```rust
let result = GitRunner::run_streaming(&["show", "HEAD:big.txt"], repo, |_line| {});
```

```
TEMP-PROBE: stdout bytes = 10488890, exit_code = 0
```

The full ~10 MB comes back whole, `exit_code: 0`, no error, no truncation marker of any kind — the
runner has no concept of an output limit to hit. (This probe was a temporary `#[test]` used only to
capture this transcript; it was removed before committing — `git diff --stat` on `git.rs` was empty
after removing it, confirming no probe code shipped.)

**Why I did not build this too, given time remained:** enforcing an output limit correctly touches
both `run_with_timeout`'s and `run_streaming_cancellable`'s pipe-reading loops (`read_to_end`,
`read_stderr_lines`), needs a new `GitError` variant and a policy for *what* gets returned when a
command is killed mid-flight for exceeding its budget (partial output? none? which stream?), and
every one of this crate's callers — `diff::diff_entry`, `status::status`, `actions`, `worktree` —
would need to agree on a sane default limit that does not truncate a legitimately large real-world
diff or status output. That is a second, independently-scoped feature, not a corollary of
cancellation, and rushing it under this pass's disk-pressure/time-pressure constraints risked
shipping something under-tested. Cancellation was the row's explicit, named, "Build it" ask; output
limits were explicitly framed as optional and turned out to need a real design decision, not fifteen
minutes of wiring.

## What remains — for a fresh critic and, separately, a fresh builder

**A fresh critic must independently verify** (I am the builder; I do not get to mark this myself):

1. Re-run `CARGO_TARGET_DIR=<private> cargo test -p tiller_git --lib git::tests::git_cancellation_kills_the_child_and_reports_cancelled -- --nocapture` and confirm it passes, and ideally reproduce the red
   transcript by re-adding the same `if false &&` guard, to confirm this report is not describing a
   test that was quietly changed since.
2. Run `cargo build --workspace` (or at least `-p tiller_ui -p tiller`) for real, on a quiet box —
   this pass could not get the shared lock to confirm it. My reasoning that the change is additive
   and safe is not a substitute for that build actually running green.
3. Decide whether `CloneForm` should get a visible Cancel control now that the runner-level API
   exists — I deliberately left this unbuilt as out of this row's scope.

**A fresh builder still owes the row two things this pass did not close:**

1. **Output limits.** `GitRunner` enforces none, and no `GitError` variant reports a truncated-output
   condition. Live-proven absent (10.4 MB returned whole above), not merely undriven. This is a
   `FAILED — absent` sub-clause independent of the cancellation work in this report.
2. **The `cargo build --workspace` confirmation** named in point 2 above, which this pass could not
   obtain due to shared-target contention.

Given both, my own verdict on `F-GIT-RUN-01` for this pass is **half-proven**: cancellation — the
row's explicitly named, previously-confirmed-absent clause — is now built, tested red-then-green,
and proven with a hard discriminator (a PID that answers `kill(pid, 0)` before cancellation and does
not after). Launch failure now has a test. Output limits / truncated-output remain genuinely absent,
proven live, and the workspace-wide build was not re-confirmed. I am not marking this row `PASSED`.
