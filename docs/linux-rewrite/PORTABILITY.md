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

## Recommended order

1. Finish the Linux contract (wave I is closing the last 6 rows). Stopping mid-wave buys nothing.
2. **Portability wave**, in this order:
   a. Target-gate `gtk`/`ksni` in the binary crate and make the gpui/gpui_platform pin conditional.
   b. `cargo check` both targets; fix what it reports until both are green.
   c. Add both `cargo check` targets to the repo gate so the debt cannot silently return.
   d. Introduce a platform seam for the tray and for process enumeration, with the Linux impl behind
      it unchanged, so the macOS/Windows impls are additive rather than surgical.
3. Only then, per-platform implementations — and they stay unverified until someone runs them on real
   hardware.

The cheapest insurance is step 2c. Every item in this document except the tray was introduced
accidentally, by an agent solving a Linux problem with the nearest Linux tool; a compile gate on three
targets makes that impossible to do silently.
