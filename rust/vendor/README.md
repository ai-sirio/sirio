# `rust/vendor/` — local overrides of the pinned `gpui` git dependency

This directory holds `[patch]` overrides for two packages inside the pinned
`zed-industries/zed` git dependency (`rev c05e34637b4f7f100a688bf6ac71cb70877fc8ad`, see
`rust/Cargo.toml`), wired in via that same file's `[patch."https://github.com/zed-industries/zed"]`
section. Everything else in the `gpui`/`gpui_*` family (including `gpui` itself) still comes
straight from the pinned git rev, unpatched.

## Why this exists: F-CORE-FILE-03A

Upstream `gpui_linux`'s Wayland `wl_data_device` handling
(`crates/gpui_linux/src/linux/wayland/client.rs`) reads the dragged `text/uri-list` off a pipe in
a **background task** kicked off by `Enter`, and only stores `DragState::window` once that task's
foreground continuation completes. `Drop`'s handler bailed out immediately whenever
`DragState::window` was still unset — no `data_offer.finish()`/`destroy()`, no error logged, no
event delivered to the app — which a provider slower than roughly 150ms reliably triggers, since
`Enter`→`Drop` can arrive from the compositor faster than the pipe read resolves. The result: a
real XDND drop is silently and totally discarded, with no trace on either side of the protocol.

Full root-cause writeup: `docs/linux-rewrite/tasks/P133-gpui-xdnd-slow-provider-race.md`. Live
reproduction: `Scripts/wayland-drive.sh`'s `xdnd <x1> <y1> <x2> <y2> <file...> --delay-ms 400`
action against a Terminal pane (see `docs/linux-rewrite/WAYLAND-LANE.md`'s XDND section).

## What's patched, and how little

- **`gpui_linux`** (`vendor/gpui_linux/`) — the actual fix. `DragState` gained a `PendingDrop`
  flag: `Drop` sets it instead of bailing out when `window` is unset; the `Enter` continuation
  checks it once the paths are resolved and, if set, finishes the transfer itself (an `Entered`
  then a `Submit`, at the last position `Motion` tracked — not `Enter`'s own, possibly stale,
  entry-point coordinate, which was a real bug caught live while building this fix). See the
  `pending_drop`/`PendingDrop`/`pending_drop_submit_position` doc comments in
  `src/linux/wayland/client.rs` for the exact mechanism, and the `tests` module at the bottom of
  that file for regression coverage. Every other file in this crate is byte-for-byte the upstream
  source at the pinned rev.
- **`gpui_platform`** (`vendor/gpui_platform/`) — **no code changed at all.** It exists only
  because `gpui_platform`'s own `gpui_linux = { workspace = true }` dependency is a *path*
  dependency inside the zed checkout (a different Cargo source than the git URL this `[patch]`
  targets), so patching `gpui_linux` alone would never be seen by `gpui_platform` — it would keep
  pulling the unpatched copy via its internal path. This override's only change from upstream is
  that one dependency line, now `{ path = "../gpui_linux" }`; everything else, including every
  other target's dependencies (macOS/Windows/wasm), is preserved unchanged.

Each crate's own `Cargo.toml` carries a comment explaining every field that had to change from
upstream (`.workspace = true` inheritance does not resolve outside the zed workspace, so those
became literal values — the same ones zed's own root `Cargo.toml` gives them at this rev).

## Layout notes

- Each crate carries its own `[workspace]` table so it resolves as an independent workspace root
  (Cargo refuses to treat a directory as both a `[patch]` target *and* an implicit member of the
  enclosing workspace). `rust/Cargo.toml`'s `[workspace] exclude` keeps the outer workspace from
  trying to claim them too.
- Each has its own committed `Cargo.lock`, so `cargo test --manifest-path
  rust/vendor/gpui_linux/Cargo.toml` (or `gpui_platform`) is reproducible standalone, independent
  of the main `rust/` build.
- `target/` under either directory is covered by `rust/.gitignore`'s bare `target/` rule (gitignore
  patterns without a leading `/` match at any depth) — never commit it; it runs to several GB.
- `LICENSE-APACHE` is copied alongside each crate, matching upstream's own per-crate licensing.

## Maintenance cost — read before bumping the pinned `gpui` rev

This is a real, standing fork of two crates, not a one-line patch. Bumping
`rust/Cargo.toml`'s pinned rev requires re-diffing `gpui_linux`'s `client.rs` against the new
upstream revision and re-applying the `PendingDrop` mechanism by hand (or dropping this override
entirely, if upstream has fixed the race some other way by then — worth checking first). Nothing
else in either vendored crate should ever need hand-merging, since nothing else was changed from
upstream.
