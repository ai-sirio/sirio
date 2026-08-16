# J2-windows — cfg-seam Linux-only code for the Windows target

Slice: drive `cargo check --target x86_64-pc-windows-msvc` to green, crate by crate, bottom-up, per
J1-gate's dependency-gating work. **Outcome: partial, and honestly so** — 6 of 13 crates verified
green on the real target; the other 7 are blocked from verification on this box by two independent,
pre-existing environment gaps that no amount of source-level cfg work can cross. Source-level fixes
were still made in every crate the task named, whether or not the crate's own compile could be
verified here.

## The two environment walls (found this slice, not previously documented)

J1-gate's report already flagged one of these for `tiller_persistence` specifically. This slice found
a **second, independent** wall that blocks a much larger set of crates, and confirmed neither is
fixable from this box.

### Wall 1 — no MSVC toolchain (`lib.exe`), blocks `rusqlite`'s bundled SQLite

`tiller_persistence` depends on `rusqlite = { features = ["bundled"] }`, which compiles SQLite's own
`sqlite3.c` via `cc-rs` at build-script time — for *any* `cargo check`, not just `cargo build`, since
Cargo must produce the native lib before it can typecheck code that links against it. Confirmed by
hand that this cannot be worked around with what is on this box: pointing `CC_x86_64_pc_windows_msvc`
at `clang`/`clang-cl` gets further (clang has a real msvc-compatible driver mode) but immediately
needs the actual Windows CRT/SDK headers (`stdlib.h`, `windows.h`, …) to compile against, and there is
no MSVC SDK, no mingw-w64, and no `windows.h` anywhere on this machine (`find / -iname windows.h`:
nothing; `dpkg -l | grep mingw`: nothing). This is the same conclusion J1-gate already reached and
documented; this slice re-confirmed it and pushed on it a little further (llvm-lib as a `lib.exe`
stand-in, clang-cl as a compiler stand-in) to make sure it was a hard wall, not a shallow one — it is.

Every crate that depends on `tiller_persistence` inherits this: **`tiller_persistence`, `tiller_acp`,
`tiller_control`, `tiller_ui`, `tiller`** (the binary).

### Wall 2 — no Windows assembler/headers, blocks `gpui` itself via `stacker`/`psm`

Not previously documented. `gpui` depends on `stacksafe` → `stacker` → `psm`, which needs to either
assemble a small stack-probe routine or (on `windows-gnu`) call into a helper compiled from
`src/arch/windows.c`, which `#include`s `windows.h`. On this host, `psm`'s build script picks the GNU
assembly variant (`x86_64_windows_gnu.s`) because no real `cl.exe`/`ml64.exe` is present, and the
plain host `cc`/`as` cannot assemble it — it uses COFF-only directives (`.def`/`.scl`/`.endef`) that
GNU `as` in ELF mode does not understand. Retrying with `clang` gets past the assembly step (clang's
integrated assembler does understand those directives) but then `stacker`'s own `windows.c` needs
`windows.h`, which — see Wall 1 — does not exist on this box in any form (no MSVC SDK, no mingw-w64).
So this is really the same underlying gap (no Windows headers/SDK at all) surfacing through a second,
unrelated dependency.

Every crate that depends on `gpui` inherits this: **`tiller_theme`, `tiller_terminal`, `tiller_ui`,
`tiller`**.

Union of both walls: **`tiller_persistence`, `tiller_theme`, `tiller_terminal`, `tiller_acp`,
`tiller_control`, `tiller_ui`, `tiller`** — 7 of 13 crates cannot be driven to a verified green
`cargo check --target x86_64-pc-windows-msvc` on this box, regardless of source correctness. This
matches PORTABILITY.md's own caveat ("no cross-linker … and no macOS or Windows machine") but is more
specific than what it previously said: it now names *which* dependencies hit the wall and why, so the
next slice (with real Windows hardware, or `cargo-xwin`/a real SDK) knows exactly what unblocks.

## Crates verified GREEN on `x86_64-pc-windows-msvc` (6 of 13)

```
cargo check --target x86_64-pc-windows-msvc -p tiller_project -p tiller_git \
  -p tiller_agents -p tiller_activity -p tiller_markdown -p tiller_usage
```

`tiller_project`, `tiller_git`, `tiller_agents` needed **no changes** — their `libc::killpg`
(process-group kill) call sites were already behind `#[cfg(unix)]` from earlier work. `tiller_activity`,
`tiller_markdown`, `tiller_usage` needed the fixes below.

## Fixes made, by family (from the task brief)

### 1. `libc` outside `cfg(unix)`

- **`tiller_markdown/src/file_events.rs`** — `FileSystemEventMonitor` was inotify-only
  (`libc::inotify_init1`/`inotify_add_watch`/the `IN_*` constants), unconditional. inotify is
  Linux-only, not even generically Unix (macOS has no inotify). Split into a
  `#[cfg(target_os = "linux")]` real implementation and a `#[cfg(not(target_os = "linux"))]` stub
  whose `new()` returns `Err(Unsupported)` — already handled gracefully by the one caller
  (`tiller_ui/src/file_view.rs` does `FileSystemEventMonitor::new(&path).ok()`). Comment names the
  counterparts: FSEvents/kqueue (macOS), `ReadDirectoryChangesW` (Windows).
- **`tiller_usage/src/claude.rs`** — `ClaudeUsageFetcher::fetch_with_env` and its private `Pty`
  struct used `posix_openpt`/`ptsname`/`grantpt`/`unlockpt`/`setsid`/`TIOCSCTTY`/`Stdio::from_raw_fd`/
  `CommandExt::pre_exec` unconditionally to drive a hidden `claude` PTY for the `/usage` panel. Split
  into `#[cfg(unix)]` (real) and `#[cfg(not(unix))]` (`UsageFetchOutcome::Unavailable(Error)`) twins.
  Comment points at ConPTY (`CreatePseudoConsole`) as the counterpart, specifically noting
  `alacritty_terminal`'s already-vendored `tty/windows/` as the implementation to reuse rather than
  hand-rolling a second ConPTY client.
- **`tiller_acp/src/lib.rs`**, **`tiller_control`** (`server.rs`/`client.rs`/`panel.rs`) — see below;
  `tiller_acp`'s one production `libc::kill` call site was already `#[cfg(unix)]`-gated from earlier
  work (nothing to fix). `tiller_control` needed real work — see family 3.

### 2. Process enumeration through `/proc/`

- **`tiller_activity/src/process.rs`** (Layer D) — `inspect_process_names` walked
  `/proc/<pid>/task/<pid>/children` + `/proc/<pid>/comm` via plain `std::fs`, so it already *compiled*
  everywhere (no libc, no unix-only types) but would only ever fail with an accidental filesystem
  `NotFound` on non-Linux — indistinguishable at the call site from "no agent running". Split into
  `#[cfg(target_os = "linux")]` real + `#[cfg(not(target_os = "linux"))]` stub that returns an explicit
  `Err(Unsupported, "…libproc; …Toolhelp32")`. Confirmed the one caller
  (`tiller/src/main.rs`'s `refresh_process_signal` match) already logs `Err` and continues rather than
  panicking, and that `Err(NotFound)` vs. any other `Err` were already distinguished correctly in
  `tiller_activity::model::refresh_process_signal`.
- **`tiller_terminal/src/lib.rs`** — `terminate_process_group`/`descendant_pids`/
  `descendant_process_groups`/`terminate_descendant_process_groups` (PTY-shutdown process-group walk,
  used to reach job-control-detached descendants) used `libc::killpg`/`getpgid` and
  `/proc/<pid>/task/<tid>/children` unconditionally. Gated all four `#[cfg(unix)]`; the entry point
  gets an honest `#[cfg(not(unix))]` no-op, documented as a **real** no-op (the PTY's own direct child
  is still torn down via the existing `Msg::Shutdown` send right after this call), not a partial
  Toolhelp32 port — the idiomatic Windows fix is a Job Object
  (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`) owned by `alacritty_terminal`'s ConPTY backend at spawn time,
  which is a different *kind* of fix (no enumerate-then-kill race at all), not a Win32 substitution
  for `killpg`/`getpgid`. All remaining libc/`/proc` sites in this file are inside `#[cfg(test)] mod
  tests` (confirmed: everything from line 1958 on), which plain `cargo check` never compiles, so they
  needed no changes for this gate.
- **`tiller_ui/src/settings.rs`** — `terminate_login_process_group`/`descendant_pids` (kills a
  canceled account-login's terminal and its descendants) walked `/proc` and shelled out to the unix
  `kill` binary unconditionally. Same shape: `#[cfg(unix)]` real + `#[cfg(not(unix))]` no-op, same
  Toolhelp32/Job-Object comment. The caller (`cancel_account_login`) already fires this from a
  detached background task with no return value to observe, so the only Windows-visible effect of the
  gap is that a canceled login's terminal process is not force-killed yet.

### 3. Unix-only path/permission assumptions

- **`tiller_usage/src/credentials.rs`** — already had `#[cfg(unix)]` branches for `0o700`/`0o600`
  with a silent non-unix fallthrough (create the file/dir at whatever the platform default is).
  Per the task's "check what its non-unix path actually does": it does nothing, silently. Added
  comments at both sites naming the real Windows counterpart (a per-user DACL via
  `SetNamedSecurityInfoW`, not a `mode()` bit) so the gap is documented rather than silently
  inherited. No behavior change — the silent fallthrough was already the only reasonable choice
  without inventing a DACL implementation, which this wave was told not to do.
- **`tiller_control` — unix domain sockets.** This was the largest single piece of work.
  `server.rs`'s `ControlServer` struct itself needed **no change** — none of its fields
  (`socket_path`, `handler`, `shutdown`, `started`, `accept_thread`) touch a unix-specific type, and
  neither do `new()`, `socket_path()`, or `stop()`. Only the methods that actually touch
  `UnixListener`/`UnixStream`/`PermissionsExt` (`start`, `prepare_parent_directory`, `prepare_path`,
  `bind_private`, `post_bind_sanity_check`, plus the free functions `accept_loop`, `peer_is_owner`,
  `serve_connection`, `write_line`) are `#[cfg(unix)]`-only. Added `ServerError::Unsupported` and a
  `#[cfg(not(unix))] ControlServer::start` that reports it honestly, with a comment explaining why a
  named pipe is a *different-shaped* transport (one handle per connection, no accept loop, no
  `set_nonblocking`) rather than a type substitution — this is the "real design question" the task
  called out, correctly left unanswered rather than guessed at.
  - **Found and fixed an existing latent bug while doing this**: `peer_is_owner`'s "any other
    platform" fallback arm was `#[cfg(not(any(linux, macos, ios, freebsd, dragonfly, openbsd,
    netbsd)))]` — which matches Windows too, but its body takes `&UnixStream`, a type that does not
    exist on Windows at all. Before this slice, that fallback would have been a hard compile error
    the moment anything tried to build this file for Windows (once past the two walls above). It now
    requires `cfg(all(unix, not(any(…))))` explicitly.
  - `client.rs`'s `round_trip` got the identical treatment (`#[cfg(unix)]` real +
    `#[cfg(not(unix))] ClientError::Unsupported` stub).
  - **`panel.rs` (the PTY-per-pane registry backing `panel.*` control methods) was deliberately left
    unseamed.** It hand-rolls a PTY via `openpty`/`fork`/`setsid`/`TIOCSCTTY`/`execvp` underneath
    **~20** `PaneRegistry` methods (`create`, `split`, `write`, `read`, `wait`, `close`, `shutdown`,
    `shutdown_for`, …), not a handful of free functions like every other fix in this slice. Giving
    each of those a `cfg(not(unix))` twin means deciding what each should *do* on Windows with no
    working transport underneath it (refuse every pane creation? report every method as
    `PaneError`? partially degrade?) — exactly the kind of speculative design the task said not to
    do ("seam it, do not build it" was said specifically about the socket, and this is the same
    principle applied to the PTY registry it carries). Added a prominent module-doc comment instead,
    naming ConPTY via `alacritty_terminal`'s already-vendored `tty/windows/` as the real counterpart
    to reuse. **This remains a known, unaddressed gap** — flagging it here rather than claiming it
    is fixed.

### 4. `gsettings` in `tiller_ui/src/titlebar.rs`

**No seam needed.** `DoubleClickAction::from_system` already shells out via
`Command::new("gsettings")` and degrades to `ToggleMaximize` on any spawn failure — which already
covers every non-Linux platform for free, with no `cfg` at all, since `Command::new` on a missing
binary just returns `Err` everywhere. Better still, that fallback happens to be *exactly correct* on
both other platforms, for different reasons: Windows has no user-configurable double-click action at
all (PORTABILITY.md: "double-click is always maximize"), which **is** `ToggleMaximize`; macOS has a
real, distinct preference (`AppleActionOnDoubleClick`, readable via `defaults read -g
AppleActionOnDoubleClick` — the same preference the Swift original reads) that a future macOS
implementation should read instead of silently accepting the GNOME-shaped default. Documented both in
a comment rather than changing behavior that was already correct.

## Verification

Windows target, the six unblocked crates:

```
cargo check --target x86_64-pc-windows-msvc -p tiller_project -p tiller_git \
  -p tiller_agents -p tiller_activity -p tiller_markdown -p tiller_usage
# Finished `dev` profile [unoptimized + debuginfo] target(s) — clean, 0 errors, 0 warnings
```

Linux, unaffected (same two pre-existing `dead_code` warnings from before this slice, unrelated to
it — `tiller_ui`'s `pump_task` field, `tiller`'s `sidebar_projects` fn):

```
cargo check --workspace                       # clean
cargo test --workspace --lib                  # all green on a clean re-run (see flake note below)
cargo test -p tiller_control --test control_integration   # 46 passed
```

Per-crate counts touched by this slice, all 0 failed: `tiller_markdown` 12, `tiller_usage` 41,
`tiller_activity` 35, `tiller_terminal` 44, `tiller_ui` 322 (44+17 for the two touched modules
individually), `tiller_control` 10 (`--lib`) + 46 (`control_integration`).

**One flake observed and run down, not a regression**: a single `cargo test --workspace --lib` run
showed `tiller_terminal::tests::shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
FAILED. Reproduced 3/3 in isolation (`cargo test -p tiller_terminal --lib`: pass every time) and on a
second full `cargo test --workspace --lib` run (pass). This is a `Duration::from_secs(5)`
deadline-polling test for real process-group teardown, timing-sensitive under the CPU contention of
running the whole workspace's fork/exec-heavy test suites in parallel — not caused by this slice's
changes (the test body itself was not touched; only `#[cfg(unix)]` was added to already-unix-only
production code it exercises).

## What is still not verifiable from this box

- **7 of 13 crates** (`tiller_persistence`, `tiller_theme`, `tiller_terminal`, `tiller_acp`,
  `tiller_control`, `tiller_ui`, `tiller`) cannot reach a verified green `cargo check --target
  x86_64-pc-windows-msvc` here, for the two environment reasons above — not because their own source
  is known-broken. Source-level fixes were made in every one of these crates that the task named
  (`tiller_terminal`, `tiller_acp`, `tiller_control`, `tiller_ui`), by careful inspection, matching
  the shape and rigor of the six verified fixes, but they are **unconfirmed by the actual gate**.
- **`tiller_control/src/panel.rs`** is the one item in the task's named list that was *not* even
  attempted at the same depth as the others — documented above and left as an explicit open item for
  whoever next has a real Windows toolchain or wants to scope a ConPTY-backed second `panel.rs`
  backend.
- Nothing in this slice attempted `aarch64-apple-darwin` — out of scope per the task (Windows only).

## Commits

- `fix(J2-windows): seam inotify and unix PTY/perms behind cfg(unix)`
- `fix(J2-windows): seam tiller_activity's /proc walk behind cfg(target_os = "linux")`
- `fix(J2-windows): seam tiller_terminal's process-group teardown behind cfg(unix)`
- `fix(J2-windows): seam tiller_ui's login-terminal /proc kill; document titlebar gsettings`
- `fix(J2-windows): seam tiller_control's socket transport; document panel.rs's larger gap`
- `docs(J2-windows): record the cfg-seam slice and the two environment walls` (this file)
