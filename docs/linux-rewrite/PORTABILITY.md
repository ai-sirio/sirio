# Portability to macOS and Windows — assessment

Requirement added by the user 2026-08-16: *"l'app dovrà essere già compatibile per delle versioni per
Mac OS e per Windows."* The rewrite has been Linux-first until now, so this records what that costs.

The headline: **the foundations are portable and the debt is small and localised.** Every hard problem
— GPU/windowing, the PTY, the webview, the clipboard — is already solved by a cross-platform
dependency. What is Linux-only is our own thin glue, plus one manifest that was written carelessly.

Note the directory name `docs/linux-rewrite/` is now a misnomer. Left alone deliberately: renaming it
would rewrite paths in several hundred committed documents for no functional gain.

## The floor is fine

| layer | macOS | Windows | evidence |
|---|---|---|---|
| **GPUI** | yes | yes | pinned rev `c05e346` — `gpui_platform/Cargo.toml` has a `[target.'cfg(target_os = "windows")'.dependencies]` section, and `gpui`'s own default features include `windows-manifest`. A `gpui_macos` crate sits beside `gpui_linux`. |
| **PTY / terminal** | yes | yes | `alacritty_terminal` (Zed's fork, rev `4c12966`) ships `src/tty/unix.rs` **and** `src/tty/windows/` (ConPTY). This was the single largest risk and it is already carried by the dependency. |
| **Webview** | yes | yes | `wry` targets WKWebView on macOS and WebView2 on Windows. Only our *X11 child-window attach* is Linux-specific — see below. |
| **Clipboard** | yes | yes | goes through GPUI, not through us. (This is why `xclip` cannot read it on Linux — it is smithay-clipboard inside GPUI.) |
| **SQLite persistence** | yes | yes | `rusqlite` bundled. |

**We had a false alarm.** There is no `gpui_windows` directory next to `gpui_linux` and `gpui_macos`,
which reads as "no Windows backend". It is not: the Windows implementation lives inside
`gpui_platform` itself. Do not re-derive this conclusion from the directory listing.

## The debt, in priority order

**1 — `rust/crates/tiller/Cargo.toml` does not build off Linux.** The only item that breaks the build
outright. Two Linux-only crates sit in the unconditional `[dependencies]`:

- `gtk = "0.18.2"` — GTK3 bindings, pulled in so `browser.wait` can pump the same process-global GTK
  main loop `tiller_ui`'s `BrowserSurface` drives.
- `ksni = "0.3.6"` — StatusNotifierItem, the D-Bus tray built for `F-USE-04`/`F-USE-05`/`F-WIN-08`.

Both need moving under `[target.'cfg(target_os = "linux")'.dependencies]`. **The pattern is already
established in this repo** — `tiller_ui/Cargo.toml` correctly gates `gtk`/`wry`/`raw-window-handle`
under Linux and has a separate `cfg(target_os = "macos")` section for objc2; `tiller_theme` does the
same. The binary crate is the one that got it wrong.

**2 — `rust/Cargo.toml` pins Linux windowing features unconditionally.**

```
gpui          = { …, default-features = false, features = ["wayland", "x11"] }
gpui_platform = { …, default-features = false, features = ["font-kit", "wayland", "x11"] }
```

`default-features = false` was a deliberate, hard-won choice (with `wayland`/`x11` off,
`gpui::guess_compositor()` reads neither `$DISPLAY` nor `$WAYLAND_DISPLAY` and the app runs headless
with no window). On macOS/Windows those two features are meaningless and the correct defaults differ,
so the pin needs to become target-conditional rather than one global setting.

**3 — Linux-only glue in our own code.** Small and enumerable:

| what | where | macOS counterpart | Windows counterpart |
|---|---|---|---|
| StatusNotifierItem tray | `crates/tiller/src/tray.rs` | `NSStatusItem` — which is exactly what the Swift original's `MenuBarExtra` already is | `Shell_NotifyIcon` |
| GTK main-loop pump | `crates/tiller/src/main.rs:5832` | not needed (WKWebView drives itself) | not needed (WebView2) |
| Titlebar double-click preference | `crates/tiller_ui/src/titlebar.rs` — reads `gsettings` | `AppleActionOnDoubleClick`, which is the Swift reference's own source | no user-configurable setting exists; double-click is maximize |
| Process enumeration | `/proc/` in `tiller_terminal/src/lib.rs`, `tiller_activity/src/process.rs`, `tiller_ui/src/settings.rs` | libproc `proc_listchildpids`/`proc_name` — **already written in the Swift original** (`App/ForegroundProcessAgent.swift`) | Toolhelp32 `CreateToolhelp32Snapshot` |
| Browser child attach | `tiller_ui/src/browser.rs` — `wry` `build_as_child` over an X11 parent | `NSView` child | `HWND` child |

Note how often the macOS answer is *already written in the Swift original*. For the tray and process
enumeration this is a port, not a design problem.

**4 — The verification harness is Linux-only, and that is the real constraint.** `wayland-drive.sh`
(nested sway + virtual pointer) and `linux-drive.sh` (`DISPLAY=:1`) are the instruments behind every
one of the 350 `PASSED` rows in `INVENTORY-LEDGER.md`. Neither has a macOS or Windows counterpart, and
none can be written on this machine.

## What can and cannot be proven here

This box is `x86_64-unknown-linux-gnu`, with no cross-linker (`cargo-xwin`, `cargo-zigbuild`, `zig`,
`osxcross` all absent) and no macOS or Windows machine.

**Achievable, and now set up:** `rust-std` for `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` is
installed, so `cargo check --target <triple>` runs. `check` does not link, so it needs no cross-linker.
That is a real gate — it catches unconditional Linux dependencies, missing `cfg` branches, and
platform APIs used outside their `cfg` — and it is what item 1 above should be measured against.

**Not achievable here:** running the app on either platform, and therefore the project's own standard
of evidence. The bar this rewrite has held to throughout is *a feature the critic hasn't successfully
tried does not exist* — 350 rows were graded by driving the real app and photographing it. **No
equivalent claim can be made for macOS or Windows from this machine.** A green `cargo check` means the
code compiles for the target; it says nothing about whether the app starts, renders, or behaves.

That distinction must not blur. If macOS/Windows rows are ever added to the ledger, they can reach at
most `half-proven` on cross-compilation evidence, and `PASSED` only from a critic on real hardware.
The seven rows in `P128-platform-exemptions-overstated.md` are the cautionary tale in one direction;
signing off untested platforms would be the same error in the other.

## What was actually done (wave J)

The plan this section used to lay out (target-gate the manifests, drive both `cargo check` targets
green, add them to the gate, seam the tray/process-enumeration) was executed in that order across
four slices — J1-gate, J2-windows, J3-macos, J4-ci — and this is the honest result, not the plan.
**The debt this document originally worried about (item 1: unconditional `gtk`/`ksni` deps, item 2:
the unconditional `wayland`/`x11` feature pin) is fully closed.** What is left is smaller, well
understood, and split cleanly into two kinds: things blocked by *this box*, and things nobody has
written yet.

### Per-crate, per-target: what `cargo check` actually proves today

13 workspace crates, checked bottom-up against both `x86_64-pc-windows-msvc` and
`aarch64-apple-darwin`. **6 need no platform work at all and are compiler-verified clean on both:**

| crate | Windows | macOS |
|---|---|---|
| `tiller_project`, `tiller_git`, `tiller_agents`, `tiller_activity`, `tiller_markdown`, `tiller_usage` | GREEN | GREEN |

**The other 7 cannot be compiler-verified from this box, on either target, for a reason that has
nothing to do with our source:**

| crate | blocked by |
|---|---|
| `tiller_persistence`, `tiller_acp`, `tiller_control`, `tiller_ui`, `tiller` | Wall 1 — `rusqlite`'s bundled `sqlite3.c` needs a real MSVC/macOS SDK to compile against |
| `tiller_theme`, `tiller_terminal`, `tiller_ui`, `tiller` | Wall 2 — `gpui`'s `stacker`/`psm` dependency needs a real cross-assembler/SDK for its per-target stack-probe stub |

(`tiller_ui` and `tiller` hit both walls, being downstream of everything.) Both walls were confirmed
*hard*, not shallow, by hand in J2/J3 — pointing `CC_<target>` at `clang` gets one step further in
each case (clang has real `-target` cross-compilation support) but then dead-ends on a missing header
(`windows.h`, or a glibc/musl `stdio.h` standing in for Apple's) that plain `clang` cannot conjure
without an actual SDK. `cargo-xwin`, `cargo-zigbuild`, `osxcross`, and `zig` are all absent from this
machine and installing one is real infrastructure work, not a `cfg` seam — explicitly out of scope
for this wave (see "Scope discipline" in the wave brief). **On real hardware — a native Windows box
with MSVC, or a native Mac with Xcode command line tools — neither wall exists**, since a native `cc`
already understands `-arch`/`-mmacosx-version-min` and already has `windows.h`/Apple's libc headers.
So this is a statement about what this Linux box can prove, not a statement about whether the 7
crates are actually portable; they may well compile cleanly the first time someone runs the check on
real hardware. Nobody has done that yet.

### The `cfg` seams that exist, and what each one's non-Linux branch actually does

J1 gated the manifests (`tiller/Cargo.toml`'s `gtk`/`ksni`, and the workspace's `gpui`/`gpui_platform`
`wayland`/`x11` pin, moved to `[target.'cfg(target_os = "linux")'.dependencies]`, matching the shape
`tiller_ui`/`tiller_theme` already used for `gtk`/`wry`/`raw-window-handle`). J2/J3 then swept the
source for every site that used to assume Linux and gave each an honest non-Linux branch:

| what | where | Linux | non-Linux branch today |
|---|---|---|---|
| GTK main-loop pump | `crates/tiller/src/main.rs` `browser.wait` | pumps `gtk::events_pending`/`main_iteration_do` | no-op; still polls `surface.state()` on the same deadline (WKWebView/WebView2 drive their own loop, so there is nothing to pump) |
| StatusNotifierItem tray | `crates/tiller/src/tray.rs` | real `ksni`-backed tray | `TrayHandle` is a unit struct, `nudge()` no-ops, `spawn()` returns `None` after an `eprintln!` — the same shape the Linux branch already uses for "no SNI host answered", so `main.rs` needed zero new branches |
| Process enumeration (Layer D / login-shell descendants) | `tiller_activity/src/process.rs`, `tiller_terminal/src/lib.rs`, `tiller_ui/src/settings.rs` | real `/proc` walk | `Err(Unsupported)`; on macOS, the portable `getpgid`/`killpg` half still runs and a one-shot `eprintln!` says descendant discovery is missing — **see the correction below, this row was wrong when first written** |
| File-system watch | `tiller_markdown/src/file_events.rs` | real `inotify` | `Err(Unsupported)` |
| Usage-fetch PTY | `tiller_usage/src/claude.rs` | real `openpty`/`fork` | `Unavailable(Error)` |
| Control socket transport | `tiller_control/src/{server,client}.rs` | real Unix-domain socket | `ServerError::Unsupported` / `ClientError::Unsupported` |
| `TIOCSCTTY` ioctl | `tiller_usage/src/claude.rs`, `tiller_control/src/panel.rs` | — | not a seam, a genuine cross-libc bug fix: `libc::TIOCSCTTY` is typed differently on glibc vs BSD/macOS libc; both call sites now cast `as _` so the same source compiles correctly on all three |

None of these claim to *work* on macOS/Windows — each one compiles, and is honest about doing
nothing (a log line, an `Err`, a `None`) rather than silently pretending. That is the bar the wave
set.

### Correction: two of those seams did not meet the bar, and this file said they did

The table above originally asserted every seam had been checked. It had not, and the process-
enumeration row was wrong in the most dangerous way available.

`descendant_pids` in `tiller_terminal/src/lib.rs` and `tiller_ui/src/settings.rs` was gated
`cfg(unix)` while its body reads `/proc`. **macOS is unix**, so macOS took the Linux
implementation: every `read_dir("/proc/<pid>/task")` fails, the loop falls through, and the
function returns an empty vector — indistinguishable from "this process has no descendants". It
type-checks perfectly, no test touches it, and the failure at runtime is silent.

For `tiller_ui/src/settings.rs` that is not a mild degradation. An empty descendant list reduces
`terminate_login_process_group` to `kill <launcher>`, which is *precisely* the failure `F-SET-14`'s
evidence recorded — the login command surviving. The bug would have returned on macOS wearing the
comment that explains why it was fixed.

Now `cfg(target_os = "linux")`, with a macOS/BSD branch that does the portable part
(`getpgid`/`killpg` on the shell's own group) and emits a one-shot `eprintln!` naming the missing
piece (libproc `proc_listchildpids`, already implemented in the Swift original).

The general lesson, worth more than the fix: **`cfg(unix)` on a body that assumes Linux is the most
dangerous shape in this codebase.** It compiles on macOS, passes every Linux test, and fails
silently at runtime. No compiler and no test suite can catch it — only someone reading the branch
and asking what it actually does on the other platform. Prefer `cfg(target_os = "linux")` and widen
deliberately.

### Correction: the CI gate could not do its job

`run_cross_target_stage` classified any failure containing the SDK wall string as `BLOCKED`. With
cargo's default fail-fast, whether an injected regression or the pre-existing wall surfaced first
was a scheduling race — a critic measured a real `gtk` regression being absorbed as `BLOCKED` **1
time in 7**.

Fixed by running `cargo check ... --keep-going` (so the wall can no longer pre-empt anything) and
classifying `BLOCKED` only when *every* error is attributable to an allowlisted environmental
failure. Running the corrected gate immediately surfaced **30 real errors in `tiller_control` that
the old classifier had been hiding** — the hand-rolled unix PTY in `panel.rs`, since seamed.

Two follow-on bugs in that fix are worth recording, because both are easy to repeat:

- The allowlist matched `` `psm` `` but cargo prints `` `psm v0.1.32` ``. Nothing ever matched, so
  `residual` was never empty and the stage could not reach `BLOCKED` at all. Always-red is safer
  than always-green and just as useless.
- The gate ran `cargo test --workspace`, which went red **twice in three runs** on a tree whose
  per-crate tests were all green, via two timing races that only appear when every test binary runs
  concurrently. Every agent in this repo is already instructed to test per crate for that exact
  reason; the gate was the last place still doing otherwise. Now per-crate.

Both controls now hold, verified against real logs: a clean tree classifies `BLOCKED` on both
targets, reintroducing `gtk` as an unconditional dependency is caught as `FAILED` via seven
unallowlisted GTK build-script failures, and the pre-seam log with real source errors is caught as
`FAILED`. `Scripts/ci-linux.sh` reaches `CI OK`.

### What is not seamed at all — the real, open gap

Two things were deliberately left untouched rather than guessed at, and both are still exactly what
they were before this wave:

- **`tiller_control/src/panel.rs`** — the ~20-method `PaneRegistry` (create/split/write/close/
  shutdown/…), built entirely on hand-rolled `openpty`/`fork`/`setsid`. A Windows counterpart is a
  ConPTY-backed second backend reusing `alacritty_terminal`'s own `tty/windows/`, which is a real
  second implementation, not a `cfg` branch on the existing one. Flagged explicitly in J2 rather than
  half-built.
- **`tiller_ui/src/browser.rs`** — the X11/XCB child-window attach (`XlibParent`, the whole
  `raw_window_handle::RawWindowHandle::Xcb`/`Xlib` bridge wry needs for WebKitGTK). Unlike everything
  in the table above, this file carries **no `cfg` at all** — it is Linux-only code with no
  acknowledgment either way, and it has never actually been reached by a compile check on this box,
  because `tiller_ui` is one of the 7 crates Wall 2 stops first. Its macOS/Windows shape (`NSView`/
  `HWND` child windows) is a different wry API entirely, same category of work as `panel.rs`'s
  ConPTY backend — a second implementation, not a seam. Whoever picks this up should not assume it
  is "probably fine because everything else was" — it is the one item in this document that has had
  zero attention.

### The gate (`Scripts/ci-linux.sh`)

J4 added `cargo check --target x86_64-pc-windows-msvc --workspace` and
`cargo check --target aarch64-apple-darwin --workspace` as two more staged, `PASS:`/`FAILED:`-style
gates, placed last (a cold `target/<triple>/` is tens of minutes and ~20 GB the first time). Missing
`rust-std` for a triple SKIPs with the exact `rustup target add` fix; a failure that bottoms out in
the `cc` crate's own compiler invocation (i.e. Wall 1/Wall 2 above, from *any* dependency, not just
`psm`/`libsqlite3-sys` by name) is reported as `BLOCKED` — loud, logged, distinct from `PASS` — rather
than failing a gate that cannot be made to pass from this box. Anything else, including a `cfg`
regression in the 7 blocked crates that would only surface once Wall 1/2 are lifted, or a new
unconditional Linux-only dependency in one of the 6 clean crates, still fails the gate hard. See the
comment block above those two stages in the script for the full reasoning, including why this does
not let the two gates (`Scripts/ci.sh` for Swift/macOS, `Scripts/ci-linux.sh` here) disagree about
what "green" means.

## What is next

1. **Real hardware** for the 7 blocked crates — a native Windows box with MSVC, or a Mac with Xcode
   command line tools, running the exact same two `cargo check --workspace` commands the gate now
   runs. That is the next fact this project is missing, and no amount of further seam-writing from
   this Linux box produces it.
2. **The two unseamed items** — `tiller_control/src/panel.rs`'s ConPTY backend and
   `tiller_ui/src/browser.rs`'s `NSView`/`HWND` child-window attach — are real second
   implementations, not `cfg` branches, and each needs the platform they target to write and test
   against.
3. **The tray and process-enumeration seams** are stubs by design (see table above); the real
   `NSStatusItem`/`Shell_NotifyIcon` and `libproc`/`Toolhelp32` implementations are additive work
   behind seams that already exist, not surgery.
4. Whatever lands from 1–3 stays at most `half-proven` in `INVENTORY-LEDGER.md` until a critic drives
   the real app on real hardware and photographs it — a green `cargo check` on this box was never
   evidence of that, only evidence that the code compiles for the target.
