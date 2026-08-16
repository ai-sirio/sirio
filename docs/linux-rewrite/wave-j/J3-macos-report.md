# J3-macos — drive `cargo check --target aarch64-apple-darwin` to green

Slice: same bottom-up crate sweep as J2-windows, this time for macOS. **Outcome: partial** — 6 of 13
crates verified green; the other 7 hit environment walls of the *same two families* J2-windows already
found for Windows, just with macOS's own missing pieces (real SDK, real cross-toolchain) instead of
MSVC's. Two real, mechanical portability bugs were found and fixed by source inspection and one of them
is now compiler-confirmed; the other could not be (see below).

## Crates verified GREEN on `aarch64-apple-darwin` (6 of 13)

```
cargo check --target aarch64-apple-darwin -p tiller_project -p tiller_git -p tiller_agents \
  -p tiller_activity -p tiller_markdown -p tiller_usage
```

`tiller_project`, `tiller_git`, `tiller_agents`, `tiller_activity`, `tiller_markdown` needed **no
change** — this confirms the task's prediction that "much of the tree may check green with no change
at all." `tiller_activity`'s Layer-D `/proc` walk already has the `#[cfg(target_os = "linux")]` real
impl + `#[cfg(not(target_os = "linux"))]` `Err(Unsupported)` stub J2-windows put there; the stub branch
is what actually got compiled and typechecked here (macOS is a real, distinct `not(linux)` target, not
a hypothetical), and it's clean.

`tiller_usage` needed one fix — see below.

## Two environment walls, same shape as J2-windows, different platform

### Wall 1 — no macOS SDK/cross-toolchain, blocks `rusqlite`'s bundled SQLite

`tiller_persistence` depends on `rusqlite = { features = ["bundled"] }`, which compiles `sqlite3.c` via
`cc-rs` at build-script time. With no environment override, `cc-rs` invokes the host's plain `cc`
(`gcc`), which doesn't understand `-arch arm64` / `-mmacosx-version-min=11.0` at all and fails
immediately. Setting `CC_aarch64_apple_darwin=clang` (clang is present at `/usr/bin/clang` and *can*
emit Mach-O: `clang -target aarch64-apple-darwin -c -x c -o /tmp/test.o -` on a header-free translation
unit produces a real `arm64 Mach-O` object) gets one step further, but `sqlite3.c` `#include`s
`<stdio.h>`, and clang falls back to `/usr/include/stdio.h` — this box's **glibc** header, which
`#include`s `bits/libc-header-start.h`, a file that doesn't exist because it's a glibc-internal header
with no macOS counterpart. There is no macOS SDK anywhere on this box (`find / -iname "MacOSX*.sdk"`
and `find / -iname "*osxcross*"`: both empty), so there is no way to give clang real Apple libc headers
to satisfy that include. Confirmed hard, not shallow.

Every crate that depends on `tiller_persistence` inherits this: **`tiller_persistence`, `tiller_acp`,
`tiller_control`, `tiller_ui`, `tiller`** (the binary).

### Wall 2 — no macOS SDK, blocks `gpui` itself via `psm`'s asm step and then via `media`'s bindgen

`gpui` depends on `stacksafe` → `stacker` → `psm`, which assembles a small stack-probe routine
(`src/arch/aarch_aapcs64.s` for this target). With the plain host `cc`, this fails the same way as Wall
1 (`cc: error: unrecognized command-line option '-mmacosx-version-min=11.0'`) before it even gets to
`sqlite3.c`, so in an unmodified environment **Wall 2 is actually hit before Wall 1** for any
`gpui`-touching crate. Overriding `CC_aarch64_apple_darwin=clang` gets past the assembly step this
time (clang's integrated assembler understands the `.s` file with no header dependency) — further than
the equivalent Windows override got in J2-windows, where `psm`'s Windows path needs `windows.h` and
stops there. macOS's `psm` doesn't need headers, so the build gets substantially further: `gpui` itself
starts compiling, and dozens of its dependencies (`cocoa`, `cocoa-foundation`, `core-text`,
`core-video`, `accesskit`, `gpui_macros`, …) check clean. It stops at `media` — a Zed crate that
`bindgen`s against real macOS frameworks (CoreVideo/CoreMedia) — with `couldn't read
.../media-.../out/bindings.rs`, because `bindgen` needs the actual Apple SDK framework headers to
generate those bindings, and (see Wall 1) there is no SDK on this box in any form. This is not
fixable with a CC override; it needs the real SDK, which cannot be legally or practically fabricated
here.

Every crate that depends on `gpui` inherits this: **`tiller_theme`, `tiller_terminal`, `tiller_ui`,
`tiller`**.

Union of both walls: **`tiller_persistence`, `tiller_theme`, `tiller_terminal`, `tiller_acp`,
`tiller_control`, `tiller_ui`, `tiller`** — the exact same 7-crate set J2-windows could not verify,
for the platform-appropriate version of the same two root causes (bundled SQLite's C build; gpui's
native stack-probe + framework bindgen). This is worth recording precisely because it means the two
walls are systemic to this box's cross-compilation setup (no SDKs, no cross-linkers for either
foreign target), not something specific to Windows or macOS individually.

## Real portability bugs found and fixed (not seams — genuine cross-platform bugs)

The task's `cfg(unix)` prediction mostly held: a `grep -rn 'cfg(target_os = "linux")'` sweep of the
whole tree found nothing that needed widening to `cfg(unix)` for macOS — everywhere Linux-only code
exists (the D-Bus tray in `tray.rs`, the GTK main-loop pump in `main.rs`, the XDG-portal color-scheme
follower in `tiller_theme`, the `/proc` walks in `tiller_activity`/`tiller_terminal`/`tiller_ui`), the
`cfg(not(target_os = "linux"))` twin already exists (from J1-gate/J2-windows) and is already an honest,
correctly-typed stub or a genuinely-different real path (e.g. `tiller_control/src/server.rs`'s
`peer_is_owner` non-Linux branch already uses `libc::getpeereid`, the real BSD/macOS syscall, with a
doc comment that literally says "Tiller targets macOS, where the check above applies" — written before
this slice, already correct). `titlebar.rs`'s `gsettings` shell-out was reconfirmed to degrade for free
on macOS exactly as J2-windows found for Windows (`Command::new("gsettings")` just fails to spawn;
`AppleActionOnDoubleClick` stays an open, documented gap, same as it was for Windows — no code to
write, no speculative tray/titlebar built).

What the `cfg(unix)` sweep does **not** catch is a bug *inside* a `cfg(unix)` block that is only wrong
on one unix flavor. Found one, mechanically identical in two places:

### `libc::TIOCSCTTY`'s type differs between Linux glibc and macOS/BSD libc

`libc::ioctl(fd, request, ...)`'s `request` parameter is typed `c_ulong`. On Linux glibc,
`libc::TIOCSCTTY` is itself a `c_ulong`, so `libc::ioctl(fd, libc::TIOCSCTTY, 0)` typechecks. On
macOS/BSD libc, `libc::TIOCSCTTY` is a narrower integer type, so the identical call is `E0308:
mismatched types` under `--target aarch64-apple-darwin`. This is invisible to both the native Linux
build (only ever sees the glibc type) **and** to J2-windows's Windows check (Windows takes the
`cfg(not(unix))` branch entirely and never evaluates this call at all) — it can only be caught by an
actual non-Linux-unix compile, which is exactly what this slice is for.

- **`crates/tiller_usage/src/claude.rs:578`** (`Pty::spawn`'s `pre_exec` closure) — **compiler-confirmed**:
  `cargo check --target aarch64-apple-darwin -p tiller_usage` failed with this exact `E0308` before the
  fix, and passes clean after it. Fixed with `libc::TIOCSCTTY as _`, which widens correctly on both
  platforms since the underlying `ioctl(2)` ABI is unaffected by the constant's Rust-side type.
- **`crates/tiller_control/src/panel.rs:719`** (`child_exec`, the control-socket's own hand-rolled PTY
  spawn) — same call, same bug, found **by inspection**, not by the compiler: this crate cannot reach
  `rustc` for this target at all yet (Wall 1, `tiller_persistence` blocks it upstream). Fixed
  identically, with a comment explaining it's an inspection-only fix pending the wall being crossed.
  Re-ran `cargo check` (native) and `cargo test -p tiller_control --lib` after the edit to confirm no
  regression on the platform that *can* be verified.

No other `libc::ioctl`/`TIOCSCTTY`/similarly-typed raw syscall call sites exist elsewhere in the tree
(`grep -rn "TIOCSCTTY"` shows only these two production sites plus doc comments).

## What was and wasn't verified

- `cargo check --target aarch64-apple-darwin` for the 6 green crates: actually run, actually clean.
- `cargo check --workspace` (native `x86_64-unknown-linux-gnu`): clean after both fixes, same two
  pre-existing warnings as before this slice (`tiller_ui::browser::BrowserSurface::pump_task` and
  `tiller::main::sidebar_projects`, both dead-code, both predate this slice).
- `cargo test -p tiller_usage -p tiller_control --lib` (native): 10 + 41 passed, 0 failed — confirms
  the `as _` cast changed no native behavior.
- `cargo check --target x86_64-pc-windows-msvc -p tiller_usage` (native regression check on J2's
  target): still clean after the `tiller_usage` fix, as expected — Windows takes the `cfg(not(unix))`
  branch and never touches the changed line.
- The `panel.rs` fix in `tiller_control` is **not** compiler-confirmed for any target — it cannot be,
  from this box, until Wall 1 is crossed by someone with a real macOS toolchain. It is included because
  it is the mechanically identical bug in the mechanically identical call, not a guess.
- The other 7 crates: **not verified green** for `aarch64-apple-darwin`, honestly. `cargo check
  --workspace` passing and the `cfg(unix)`/`cfg(not(target_os = "linux"))` sweep both being clean is
  evidence the *known* seams are correct, but it is not the same claim as a green target-specific
  `cargo check`, and this report does not make that claim.

## Not done, deliberately

- No macOS SDK was fabricated, borrowed, or partially reconstructed to push further into Wall 1/Wall 2
  — that would produce a `cargo check` result build on headers this project has no license to ship and
  that would not exist on a real macOS build box anyway, i.e. a false green.
- No macOS tray (`NSStatusItem`), no libproc-based process enumeration, no `AppleActionOnDoubleClick`
  reader were implemented, even though all three now have a correct, honest, already-in-place
  `cfg(not(target_os = "linux"))` no-op/stub. The task was explicit that this wave seams, it does not
  implement — these three items already have their macOS counterparts named in code comments
  (`tray.rs`, `process.rs`, `titlebar.rs`) pointing at `App/AgentRosterView.swift`/`TillerApp.swift`,
  `App/ForegroundProcessAgent.swift`, and `AppleActionOnDoubleClick` respectively, from J1/J2.
- Item 2c from PORTABILITY.md (adding these `cargo check --target` invocations to a repo gate script)
  was not built — it wasn't asked for in this slice's brief, and doing it before all three targets have
  at least one real successful crossing risks locking in a script nobody can run to completion on this
  box.

## Seams

- `rust/crates/tiller_usage/src/claude.rs`: `Pty::spawn`'s `pre_exec` — `libc::ioctl(slave,
  libc::TIOCSCTTY, 0)` → `libc::ioctl(slave, libc::TIOCSCTTY as _, 0)`. Compiler-confirmed fix for a
  real Linux-glibc-vs-macOS/BSD-libc type mismatch in `libc::TIOCSCTTY`, not a platform-behavior seam.
- `rust/crates/tiller_control/src/panel.rs`: `child_exec` — identical `libc::ioctl(slave,
  libc::TIOCSCTTY, 0)` → `as _` fix, found by inspection (same call, same crate family, cannot reach
  `rustc` for this target yet because `tiller_persistence` blocks it upstream at Wall 1).
