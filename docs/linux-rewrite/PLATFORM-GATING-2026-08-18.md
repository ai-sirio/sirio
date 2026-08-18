# Platform-gating audit — 2026-08-18

Scope: does anything touching the OS sit outside a `cfg`-gated module, per
`docs/linux-rewrite/PORTABILITY.md` and the goal's "macOS, Windows, Linux" requirement. Gates were
run, not paraphrased; commands and real output are recorded below.

## Gate results

**`Scripts/check-module-boundaries.sh`** (Swift `Packages/` boundaries, invoked by `Scripts/ci.sh`):
exit 0, `module boundaries OK`. No violation.

**`Scripts/ci-linux.sh`**: first run went red —
`FAILED: cargo test -p tiller_terminal` at
`tests::shutdown_terminates_a_job_control_child_that_detached_into_its_own_process_group`
(`left: 157552 right: 157552`, "setsid must actually have put the child in a new process group").
At the same moment a second agent's `cargo build --manifest-path rust/Cargo.toml --workspace` was
running concurrently in this same shared tree (`ps aux` showed it live at the timestamp of the
failure). Re-ran the exact single test four times back-to-back with no other cargo activity present
— all four `ok`; re-ran `cargo test -p tiller_terminal` (full crate, default concurrency) four more
times — all four `ok`. Then re-ran the **entire** `Scripts/ci-linux.sh` end to end with the tree
quiet:

```
PASS: cargo fmt --check
PASS: cargo clippy (owned crates)
PASS: cargo build
PASS: cargo test -p tiller / tiller_acp / tiller_persistence / tiller_activity / tiller_agents /
      tiller_control / tiller_git / tiller_project / tiller_terminal / tiller_theme / tiller_ui /
      tiller_markdown / tiller_usage
SKIP: real ACP acceptance — set TILLER_ACP_REAL=1 to exercise it
PASS: test-crash-supervise.py
PASS: test-crash-freeze-supervise.py
PASS: test-visual-sweep.sh
BLOCKED: cargo check --target x86_64-pc-windows-msvc --workspace — known SDK/cross-toolchain wall
BLOCKED: cargo check --target aarch64-apple-darwin --workspace — known SDK/cross-toolchain wall
PASS: headless smoke test
CI OK
```

Verdict: the gate is **green**; the first run's failure was contention from a concurrent
cargo build elsewhere in this shared worktree, not a code regression — reproduced clean 8/8 times
once the tree was quiet. Not reporting it as a finding row; recording the reproduction here per
"a paraphrased check is a weaker check."

## Cross-target `cargo check`

Both `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` report `BLOCKED`, exactly matching
PORTABILITY.md's documented Wall 1 (`libsqlite3-sys`'s bundled `sqlite3.c` needs a real MSVC/macOS
SDK) and Wall 2 (`gpui`'s `stacker`/`psm` needs a real cross-assembler). The Windows run stops at
`lib.exe` not found; the macOS run stops at `cc: error: unrecognized command-line option '-arch'`
inside `libsqlite3-sys`'s build script, and also at `media`'s missing generated `bindings.rs`
(already allowlisted by name in the gate). Since `tiller_ui` depends on `tiller_persistence`
(→ `rusqlite` → `libsqlite3-sys`), **`tiller_ui`'s own source is never reached by this check on
either cross target** — the wall trips one dependency layer before cargo gets to type-check
`tiller_ui` itself. This is exactly the situation PORTABILITY.md warns about ("has never actually
been reached by a compile check on this box") and is why the finding below survives undetected by
a currently-green gate.

## Findings

### PORT-1 — `tiller_ui::browser` module is unconditionally compiled but imports Linux-only crates

`crates/tiller_ui/src/lib.rs:6` declares `pub mod browser;` with no `cfg` gate at all — it is
compiled for every target `tiller_ui` builds for. `crates/tiller_ui/src/browser.rs` unconditionally
does:

```rust
use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle};
use wry::{NewWindowFeatures, NewWindowResponse, PageLoadEvent, Rect, WebView, WebViewBuilder, ...};
```

Both `wry` and `raw-window-handle` are declared **only** under
`crates/tiller_ui/Cargo.toml`'s `[target.'cfg(target_os = "linux")'.dependencies]` (confirmed by
reading the file — no unconditional or macOS/Windows entry exists for either). On a real macOS or
Windows build, neither crate is even in the dependency graph, so `browser.rs` fails with "can't find
crate `wry`" / "can't find crate `raw_window_handle`" — not a cross-compile SDK problem, an
unconditional-dependency problem, i.e. exactly Debt Item 1 from PORTABILITY.md, just relocated from
the manifest (already fixed) to the module tree (not fixed). The file's own header comment —
"intentionally not part of the production module graph yet" — is stale and actively misleading:
`pub mod browser;` in `lib.rs` puts it in the graph unconditionally. Nothing in the whole 2068-line
file carries a platform `cfg` (`grep -c cfg` finds exactly one hit, `#[cfg(test)]` on the test
module).

This cannot be caught by `Scripts/ci-linux.sh` today for the structural reason above (Wall 1 stops
`tiller_persistence`, a dependency of `tiller_ui`, before cargo ever reaches `tiller_ui`'s own
source on either cross target) — so the gate being green is not evidence this file is fine.

**Fix**: gate the module declaration itself —
`#[cfg(target_os = "linux")] pub mod browser;` in `crates/tiller_ui/src/lib.rs:6` (matching how
`tray.rs`/`titlebar.rs`'s Linux-only pieces are gated elsewhere in this codebase) — and correct the
stale header comment in `browser.rs:1-5` so the next reader doesn't repeat the same false premise.

### PORT-2 — a test reads `/proc/self/status` with no platform gate, unlike its siblings

`crates/tiller/src/main.rs:13588`, inside `drawn_save_failure_surfaces_file_notice`:

```rust
let path = PathBuf::from("/proc/self/status");
```

used as a "real read-only file that exists" fixture to provoke a save failure. Every other test in
this same file that touches `/proc` (e.g. `workspace_wires_real_process_signal_without_title_clobber`
at line ~12401) is gated `#[cfg(target_os = "linux")]`; this one is not. `cargo check --target
<triple> --workspace` (no `--tests`) does not catch it, so the cross-target gate stage is silent
here — but a native `cargo test --workspace` on macOS or Windows would either fail to open the path
(no such file) or, worse, silently pass/fail depending on what happens to exist at that path,
undermining the specific assertion the test exists to make.

**Fix**: add `#[cfg(target_os = "linux")]` to this test (or switch the fixture to a portable
always-present read-only path/mechanism), matching the pattern the file already uses at line 12401.

## Not violations (checked and confirmed already gated)

Spot-checked against the categories named in the assignment (libc, nix, wayland/x11, `/proc`, unix
sockets, PTY, `std::os::unix`, file-manager/DnD):

- `tiller_control/src/{server,client}.rs` (`UnixListener`/`UnixStream`, `libc::getsockopt`/
  `getpeereid`) — all under `#[cfg(unix)]`, with `#[cfg(not(unix))]` `Unsupported` twins.
- `tiller_control/src/panel.rs` — `spawn_process` has a real `#[cfg(not(unix))]` twin
  (`PaneError::Unsupported`) as of commit `da053445`; every other method funnels through it so no
  further split is needed. **Note**: `docs/linux-rewrite/PORTABILITY.md`'s "What is not seamed at
  all" section still lists `panel.rs` as fully unseamed ("hand-rolled openpty/fork/setsid... a real
  second implementation, not a cfg branch") — that description is now stale relative to the code;
  worth a doc fix, not a code finding.
- `tiller_activity/src/process.rs` (`/proc` walk) — `PROC_ROOT`/walker under
  `#[cfg(target_os = "linux")]`, with a documented non-Linux stub.
- `tiller_terminal/src/lib.rs:584`, `tiller_ui/src/settings.rs:671` (`/proc/<pid>/task`) —
  `#[cfg(target_os = "linux")]`, non-Linux branch does the portable `getpgid`/`killpg` half plus a
  one-shot `eprintln!`, per the wave-J correction already recorded in PORTABILITY.md.
  `tiller_terminal/src/lib.rs:2543` (`/proc/<pid>/stat`) is inside the same Linux-gated function.
  `tiller/src/main.rs:13588` is the one exception — see PORT-2.
  `crates/tiller/src/panes.rs` only mentions `/proc` in a doc comment, no actual access.
- `tiller_usage/src/claude.rs` (hand-rolled `openpty`/`fork`/`setsid`/`TIOCSCTTY`) — every raw-libc
  block is `#[cfg(unix)]`; the crate's `libc` dependency is unconditional in `Cargo.toml` but that's
  fine, `libc` itself compiles on every target.
- `tiller_project/src/git.rs`, `tiller_git/src/git.rs` (`libc::killpg` for process-group kill) —
  `#[cfg(unix)]` with `#[cfg(not(unix))]` twins.
- `tiller_acp/src/lib.rs:1486` (`libc::kill(-pid, ...)`) — `#[cfg(unix)]`.
- `std::os::unix::fs::symlink`/`PermissionsExt` sites in `tiller/src/main.rs`,
  `tiller_agents/src/lib.rs`, `tiller_usage/src/credentials.rs`, `tiller_project/src/{file,layout}.rs`
  — all `#[cfg(unix)]`, with portable fallbacks where the call site isn't test-only.
  `tiller_project/src/file.rs:276` and `tiller_project/src/layout.rs:412` are inside `#[cfg(test)]`
  fixture code building a symlink to test traversal-escape handling — Linux-only test but harmless
  (skipped, not miscompiled, on other targets — confirmed both are under `#[cfg(unix)]` test fns).
- `crates/tiller/src/tray.rs` (ksni/D-Bus tray) and the GTK main-loop pump in
  `crates/tiller/src/main.rs:6291-6293` — both `#[cfg(target_os = "linux")]` with real non-Linux
  branches, as PORTABILITY.md describes.
- `Command::new("gsettings"|"xdg-open")` (`tiller_ui/src/titlebar.rs`, `tiller_ui/src/editor.rs`,
  `tiller/src/main.rs:4184,6502`) — these are `std::process::Command` (portable API) invoking a
  Linux-only *binary name*; they do not fail to compile on macOS/Windows, they fail to `spawn()` at
  runtime and every call site already handles that `Err` (notice banner or `eprintln!`), matching
  the pattern PORTABILITY.md already accepts for `gsettings`. Not a compile-time violation; flagged
  here only because "Reveal in File Manager"/"open external link" have no macOS (`open`)/Windows
  (`explorer`) branch at all, so the *feature* silently no-ops on non-Linux even though the *code*
  is portable. Worth a follow-up ticket, not a `cfg` defect.
- GPUI's `ExternalPaths`-based drag-and-drop in `tiller_terminal/src/lib.rs` — only doc comments
  mention XDND; no raw X11 DnD API is called directly, GPUI's own portable type is used throughout.
- Workspace `Cargo.toml`'s `gpui`/`gpui_platform` `wayland`/`x11` feature pin, and
  `crates/tiller/Cargo.toml`'s `gtk`/`ksni` deps — both correctly under
  `[target.'cfg(target_os = "linux")'.dependencies]`, matching PORTABILITY.md's "fully closed" claim
  for Debt Items 1 and 2.

## Summary

Two findings, both real and both currently unreachable by the automated gates for structural reasons
explained above — `Scripts/ci-linux.sh` itself is green (`CI OK`, reproduced twice) and
`Scripts/check-module-boundaries.sh` is green. PORT-1 is the more serious of the two: it is a repeat
of the exact class of bug (unconditional Linux-only dependency) that PORTABILITY.md's Debt Item 1
already fixed once at the manifest level, now present one layer down in the module tree, hidden from
the cross-target gate by an unrelated SDK wall in a crate `tiller_ui` merely depends on.
