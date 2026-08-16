# K — seam `tiller_control::panel` for non-unix targets

Slice: J2 (`docs/linux-rewrite/wave-j/J2-windows-report.md`) deliberately left `panel.rs` unseamed —
its own module doc comment said so, and named the reason: ~20 `PaneRegistry` methods sit on top of a
hand-rolled unix PTY, and deciding what each should *do* on Windows without a working PTY underneath
was ruled out of scope for that slice ("seam it, do not build it"). A fixed CI classifier (the gate
now runs `cargo check --keep-going` and only reports BLOCKED when *every* remaining error is
attributable to the known toolchain walls, instead of stopping at the first failure) surfaced this gap
concretely: 30 real `error[...]` sites, all in `tiller_control`, all traced to `panel.rs`'s
unconditional `libc`/`std::os::fd` use. This slice closes that gap. **Outcome: green.**

## What changed

`crates/tiller_control/src/panel.rs` — no other file needed edits (`client.rs`/`server.rs` were
already seamed in J2/J3; the task brief's line numbers for those two turned out to be stale — see
"Correction" below).

**The portable surface stays portable, unconditionally.** `PaneInfo`, `PaneExitStatus`,
`PaneStateSnapshot`, `PaneError`, `terminal_key_bytes`, `base64_encode`, and the entire
`PaneRegistry` public API (`new`, `create`, `set_external`, `set_external_state`, `split`, `list`,
`list_for`, `write`, `key`, `read`, `state`, `scrollback`, `wait`, `close`, `focus`, `shutdown`,
`shutdown_for`) exist with **identical signatures** on every target — the brief's "Linux public API
must not change" bar. None of `PaneRegistry`'s own method bodies needed a `cfg` split.

**Only the PTY backend is unix-only**, and that turned out to be a much smaller cut than the 20
methods it sits under suggested:

- Added `PaneError::Unsupported(String)`, matched in `Display` like every other variant.
- `spawn_process` — the single function every `PaneRegistry` method that can create a pane
  (`create`, and `split` which calls `create`) funnels through — got a `#[cfg(not(unix))]` twin that
  returns `Err(PaneError::Unsupported(...))` naming the counterpart (ConPTY via
  `alacritty_terminal`'s `tty/windows/`) instead of the real `#[cfg(unix)]` `openpty`/`fork` version.
- Because `spawn_process` is the *only* place a `PaneEntry`/`PaneProcess` is ever constructed, no
  non-unix build can ever put an entry into the `panes: HashMap<String, Arc<PaneEntry>>` map. Every
  other method that reads that map (`get`, `write`, `key`, `read`, `state`, `wait`, `close`, `list`,
  `list_for`, `focus`) therefore needed **no cfg split at all** — on non-unix they just always find the
  map empty and correctly return `PaneError::UnknownPane`/`ClosedPane` for any id a caller could have
  obtained, which is the honest behavior, not a silent no-op dressed as one.
- `terminate`, `reap`, `wait_for_process_group`, `process_done`, `child_exec`, `read_process_output`
  are unix-only (`libc::killpg`/`waitpid`/`WIFEXITED`/etc., `std::os::fd::FromRawFd`) and got
  `#[cfg(unix)]`. `terminate` additionally got a `#[cfg(not(unix))] fn terminate(_: &PaneProcess) {}`
  stub — not because it does anything, but because `close`/`shutdown`/`shutdown_for` call it
  unconditionally and are themselves platform-generic; the stub is provably unreachable (no live
  `PaneProcess` can exist without `spawn_process` succeeding), documented as such in a comment rather
  than left to look like a real implementation.
- `PaneProcess`'s `pid`/`process_group` fields (typed `libc::pid_t`) are `#[cfg(unix)]`-only; nothing
  outside the now-`#[cfg(unix)]` functions touches them.
- The renderer-owned-pane paths (`set_external`, `set_external_state`, and the `external` half of
  `list`/`list_for`/`read`/`state`) never touched libc in the first place and are completely
  unaffected — those already worked cross-platform because the GPUI workspace, not this crate's PTY,
  owns that pane's actual process.
- Imports (`CString`, `FromRawFd`/`RawFd`, `Read`, the `io` module besides `Write`) and the two
  scrollback/grace-period constants were moved behind `#[cfg(unix)]` alongside the code that uses
  them, to keep `cargo check --target x86_64-pc-windows-msvc` free of `unused_imports`/`dead_code`
  warnings, not because they were compile errors.
- Rewrote the module doc comment to describe the seam that now exists (previously it explained why
  the crate was *not* seamed).

## A second, unrelated bug found and fixed while verifying `aarch64-apple-darwin`

`spawn_process`'s `libc::openpty` call passed `std::ptr::null()` (a `*const _`) for `termp` and
`&window` (coerces to `*const winsize`) for `winp`. That is the Linux/glibc signature
(`termp: *const termios, winp: *const winsize`), but macOS/BSD libc types both `*mut` — the same
const/mut split the file already had a comment about for `TIOCSCTTY`, just not yet applied here. This
was a pre-existing bug (confirmed via `git stash`: present before this slice's edits too, at the
call's previous line 643), not something this slice introduced, but it sat directly in the file this
slice was touching and blocked bar #2 (`aarch64-apple-darwin`), so it was fixed in the same pass:
`window` is now `mut` and passed as `&mut window`, `null_mut()` replaces `null()` for `termp`. Both
`*mut T` and `&mut T` coerce to `*const T` at a call site, so the one call now compiles unchanged on
both the glibc (`*const`) and BSD (`*mut`) signatures — no `cfg` needed for this one.

## Correction to the task brief

The brief named `crates/tiller_control/src/client.rs:12` and `crates/tiller_control/src/server.rs:38`
as additional error sites ("a `UnixStream` in a type position that an earlier pass missed"). At the
start of this slice both were already `#[cfg(unix)]`-gated correctly (J2/J3 work, commits `51609ac4`
and `2f70ac38`) — those line numbers currently point at unrelated `use crate::protocol::{...}` lines
that produce only `unused_imports` *warnings* on the Windows target (`decode_response`/`encode_line`
in `client.rs`, `decode_request`/`encode_line` in `server.rs`), not errors. No source change was
needed or made in either file. Flagging this rather than silently doing nothing there, per this
project's own "check scope claims against the reference" convention.

## Verification

### 1. `x86_64-pc-windows-msvc`, `tiller_control` alone

```
cargo check --target x86_64-pc-windows-msvc -p tiller_control --keep-going
# Finished `dev` profile [unoptimized + debuginfo] target(s) — 0 errors, 3 pre-existing warnings
# (unused_imports x2 in client.rs/server.rs, dead_code on ControlServer::handler — none touched
# by this slice, none new)
```

Before this slice's `panel.rs` edits: 30 `error[...]` sites, all in `panel.rs` (confirmed by
re-running the same command against `git stash`).

### 2. `x86_64-pc-windows-msvc`, whole workspace

```
cargo check --target x86_64-pc-windows-msvc --workspace --keep-going > /tmp/win_full.log 2>&1
grep -E '^error' /tmp/win_full.log \
  | grep -vE 'error occurred in cc-rs:' \
  | grep -vE 'could not compile `(psm|stacker|libsqlite3-sys|rusqlite)`'
```

This does **not** print nothing — it prints four lines:

```
error: failed to run custom build command for `stacker v0.1.25`
error: failed to run custom build command for `psm v0.1.32`
error: failed to run custom build command for `libsqlite3-sys v0.28.0`
error: failed to run custom build command for `gpui v0.2.2 (...)`
```

All four are build-script failures (`error: failed to run custom build command for ...`), a different
message shape than the `could not compile \`X\`` pattern the given filter targets — on this box, a
build-script crash never produces a downstream "could not compile" line for the crate whose script
failed, only for crates further downstream that depend on its output (none appeared here). The filter
as literally written cannot match these regardless of source correctness; the substance underneath is
what matters:

- `stacker`/`psm` are exactly Wall 2 from J2's report (no Windows assembler/headers).
- `libsqlite3-sys` is exactly Wall 1 from J2's report (no MSVC toolchain, `rusqlite`'s bundled
  `sqlite3.c`).
- `gpui` is a **new** manifestation of the same root wall, not previously named in J2/J3: its build
  script needs `llvm-rc`/`lib.exe` to compile `resources/windows/gpui.manifest.xml` into a `.rc`/`.lib`
  and fails with `error occurred in cc-rs: failed to find tool "lib.exe"`. Confirmed pre-existing and
  unrelated to this slice two ways: (a) `git stash` + rerun reproduces the identical failure with
  this slice's `panel.rs` change absent; (b) `cargo check --target x86_64-pc-windows-msvc -p gpui`
  in isolation (a crate this slice never touches, pulled from a pinned Zed git rev) fails the same
  way on its own, independent of the workspace graph.

`grep -n tiller_control /tmp/win_full.log` shows exactly one relevant line — `Checking tiller_control
v0.1.0 (...)` — with no accompanying `error` line anywhere in the log. **Zero errors attributable to
`tiller_control` or any other crate this slice touched.** Recommend updating the check command in the
next task brief that reuses it to also exclude `^error: failed to run custom build command for` for
these four crate names, since that is the message shape `--keep-going` actually produces for a
build-script wall.

### 3. `aarch64-apple-darwin`, `tiller_control` alone and whole workspace

```
cargo check --target aarch64-apple-darwin -p tiller_control --keep-going
# Finished `dev` profile [unoptimized + debuginfo] target(s) — clean, 0 errors, 0 warnings

cargo check --target aarch64-apple-darwin --workspace --keep-going
# 4 error lines total: libsqlite3-sys (Wall 1), psm (Wall 2), and `media`'s build script failing to
# find its own generated bindings.rs as a direct consequence of libsqlite3-sys's build script never
# running to completion first — none in tiller_control or any crate this slice touched.
```

### 4. Linux baseline, unchanged

```
cargo build -p tiller                    # Finished, clean (pre-existing dead_code warnings only)
cargo test -p tiller_control             # 46 passed; 0 failed
cargo test -p tiller                     # 162 passed; 0 failed
cargo test -p tiller_ui                  # 322 passed; 0 failed
cargo test -p tiller_terminal            # 44 passed; 0 failed
```

All four counts match the task's stated baseline exactly. Each crate run alone (never
`--workspace`), per this project's standing note about
`shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`'s flake under
workspace-wide concurrency — not hit this run.

## What is still not done, on purpose

**No Windows PTY backend was written.** `create`/`split` on non-unix always return
`PaneError::Unsupported`; the module doc comment at the top of `panel.rs` names the honest
counterpart (ConPTY via `alacritty_terminal`'s already-vendored `tty/windows/`, which already owns a
child process's lifetime end-to-end) for whoever picks this up next, and explains why a hand-rolled
`CreateNamedPipeW`/`CreatePseudoConsole` client written from scratch in this file would be the wrong
shape to build speculatively.

## Commits

- `fix(K-seam): seam tiller_control::panel for non-unix targets; fix a pre-existing macOS openpty signature bug`
- `docs(K-seam): record the panel.rs seam and the gpui build-script wall`
