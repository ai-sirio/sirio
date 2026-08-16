# J1-gate — target-gate Linux-only dependencies and glue

Slice: target-gate the three items PORTABILITY.md names so non-Linux `cargo check` can begin.
Explicitly out of scope: chasing every remaining compile error for macOS/Windows (that is J2/J3).

## 1. `rust/crates/tiller/Cargo.toml` — `gtk`/`ksni` moved off unconditional deps

Moved `gtk = "0.18.2"` and `ksni = { version = "0.3.6", features = ["blocking"] }`, with their
existing explanatory comments unchanged, from `[dependencies]` into a new
`[target.'cfg(target_os = "linux")'.dependencies]` section — the same shape `tiller_ui/Cargo.toml`
and `tiller_theme/Cargo.toml` already use. No other line in the file changed.

## 2. `rust/Cargo.toml` — the `gpui`/`gpui_platform` pin

Read the long comment above the pin first, per the task. Workspace dependencies cannot themselves
be target-conditional (there is no `[target.'cfg(...)'.workspace.dependencies]` table), so of the
two honest shapes offered, I picked **per-crate target sections**, not a feature flag on our side,
for two reasons:

- A "feature on our side" would still need a target-specific section *somewhere* to turn the
  feature on by default for Linux (Cargo has no way to make a feature default-on conditional on
  `cfg(target_os)` from the manifest alone), so it does not actually avoid touching per-crate
  manifests — it just adds a layer of indirection through an extra feature name for no benefit.
- Per-crate target sections keep the fix local and self-explaining at each call site, matching the
  pattern the repo already uses for `gtk`/`wry`/`raw-window-handle` in `tiller_ui` and `tiller_theme`.

Verified empirically before touching the real manifests (scratch crate, `/tmp/tmp.*/testdup`) that
Cargo unifies features requested for the *same* dependency across a package's unconditional
`[dependencies]` entry and a `[target.'cfg(...)'.dependencies]` entry for that target: a base entry
`serde = { version = "1", default-features = false }` plus a Linux-only
`serde = { version = "1", features = ["derive"] }` compiled `serde_derive` on Linux. This is
additive, not a second/conflicting declaration of the same crate — confirmed with `cargo check`
before relying on it in the real manifests.

Changes:

- `gpui` in the workspace manifest: `default-features = false`, **`wayland`/`x11` removed** (they
  are the Linux-only pair per PORTABILITY.md). Comment above it rewritten to explain the new shape
  and why (workspace deps can't be target-conditional; Cargo unifies features from per-crate target
  sections; net effect for `x86_64-unknown-linux-gnu` is byte-for-byte the same final feature set).
- `gpui_platform`: `default-features = false, features = ["font-kit"]` kept **unconditional** —
  `font-kit` is not one of the two features PORTABILITY.md calls Linux-only (only `wayland`/`x11`
  are), so it stays where it was.
- Every crate that references `gpui` and/or `gpui_platform` — `tiller_theme`, `tiller_terminal`,
  `tiller_ui`, `tiller` — got a `gpui = { workspace = true, features = ["wayland", "x11"] }` (and
  `gpui_platform` likewise where applicable) added to its `[target.'cfg(target_os = "linux")'.
  dependencies]` section, alongside the existing entries there (or a new such section, for
  `tiller_theme`/`tiller_terminal`, which didn't have a Linux dependency section for `tiller`
  crates before). `tiller_theme` had no Linux target section for `gpui`/`gpui_platform` before
  (only `ashpd`); one line added there. `tiller_terminal` already had a customized
  `gpui_platform = { workspace = true, features = ["font-kit"] }` in its unconditional deps
  (unchanged) plus the new Linux-only `wayland`/`x11` addition.

Net result for the Linux build: identical resolved feature set to before this change (verified —
see "proof" below). For macOS/Windows: `gpui`/`gpui_platform` now resolve with
`default-features = false` and no Linux-only features, which is honestly incomplete (no
`windows-manifest` on Windows, no macOS-specific default restored) but that per-platform tuning is
explicitly J2/J3's job, not this slice's.

## 3. Linux-only glue in our own code

- `crates/tiller/src/main.rs` (`browser.wait`, was line 5832, now inside the same block after the
  above edits): the `while gtk::events_pending() { gtk::main_iteration_do(false); }` pump is now
  `#[cfg(target_os = "linux")]`-gated in place. Non-Linux branch: nothing runs there — the loop
  still polls `surface.state()` and honors the deadline exactly as before, it just never calls into
  `gtk`. Comment added explaining why this is honest: WKWebView/WebView2 drive their own event
  loops (per PORTABILITY.md's "Browser child attach" row), so there is no equivalent pump to call,
  not a case of "haven't gotten to it yet".
- `crates/tiller/src/tray.rs`: split every ksni-touching item (`AgentRosterTray`, its
  `ksni::Tray` impl, the `ksni`-backed `TrayHandle(ksni::blocking::Handle<..>)`, and the Linux
  `spawn()`) behind `#[cfg(target_os = "linux")]`. Added a `#[cfg(not(target_os = "linux"))]`
  counterpart for each: `TrayHandle` becomes a unit struct whose `nudge()` is a no-op, and `spawn()`
  unconditionally returns `None` after an `eprintln!("[tray] not implemented on this platform,
  roster disabled")` — the same "no SNI host answered" shape the Linux branch already uses when
  `tray.spawn()` errors, so callers (`main.rs`) don't need new branches: they already treat `spawn()
  -> None` and a no-op `nudge()` as "tray absent, app runs without the roster surface". Public API
  (`TrayRosterEntry`, `TrayRequest`, `SharedRoster`, `TrayRequestQueue`, `TrayHandle::nudge`,
  `spawn()`) is unchanged across platforms; `main.rs` required zero edits for the tray split.
  `TrayRosterEntry`/`TrayRequest`/the two type aliases don't touch `ksni` at all and stayed
  unconditional.

This is a stub, not a macOS/Windows tray implementation — no `NSStatusItem`/`Shell_NotifyIcon` code
was written, per the scope-discipline instruction not to build platform features on speculation.

## Proof the Linux side did not regress

```
cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux/rust
cargo build -p tiller          # Finished `dev` profile ... (only pre-existing dead_code warnings)
cargo test -p tiller           # test result: ok. 162 passed; 0 failed; 0 ignored
cargo check --workspace        # Finished, clean (same 2 pre-existing dead_code warnings)
```

Also ran `cargo test -p tiller` on `git stash` (i.e. before any of this slice's edits) to establish
the baseline: **162 passed**, not 161 — the task brief's "161" predates the J0-chat05 slice (already
landed on this branch before J1 started), which added a test. My changes are confirmed to hold that
count exactly: 162 before my edits, 162 after.

## Windows dependency-resolution gate

```
cargo check --target x86_64-pc-windows-msvc -p tiller_persistence
```

Gets past dependency resolution and into actually compiling a dependency
(`libsqlite3-sys v0.28.0`'s build script, which shells out to `cc`/`lib.exe` to build bundled
SQLite). It fails there with:

```
error occurred in cc-rs: failed to find tool "lib.exe": No such file or directory (os error 2)
```

This is the expected, documented limitation from PORTABILITY.md/the task background — no MSVC
toolchain or cross-linker exists on this box (`cargo-xwin`, `zig`, `osxcross` all absent) — not a
dependency-resolution failure. Resolution itself (the Cargo.lock SAT-solve across the whole
workspace, including the now target-conditional `gpui`/`gpui_platform`/`gtk`/`ksni` entries)
succeeded; the run got well past that stage before hitting a native-toolchain wall this task's bar
explicitly does not require crossing.

Note: `tiller_persistence` does not depend on `gpui`/`gtk`/`ksni` at all, so this specific gate
command was not actually stress-testing items 1–2 above (Cargo resolves the whole workspace's
`Cargo.lock` regardless of which `-p` package is being checked, but only *builds* units reachable
from that package, and `gpui`'s optional Linux-backend crates such as `wayland-client` are outside
`tiller_persistence`'s reachable unit graph for any target). Item 1/2's real test is `cargo check
--target x86_64-pc-windows-msvc -p tiller` (or `-p tiller_ui`/`-p tiller_terminal`), which is J2/J3's
job per the task's own "do not chase every remaining compile error in this slice" instruction — not
run to completion here, since it is expected to hit further Linux-only code inside those crates
(X11 child-window attach in `tiller_ui/src/browser.rs`, `/proc` process enumeration, etc.) that
PORTABILITY.md item 3's table already names as future work, not this slice's.

## Commits

- `chore(J1-gate): move gtk/ksni to Linux-only deps in tiller/Cargo.toml`
- `chore(J1-gate): make the gpui/gpui_platform wayland/x11 pin Linux-only`
- `fix(J1-gate): cfg-gate the GTK main-loop pump and the ksni tray to Linux`
- `docs(J1-gate): record the target-gating slice`
