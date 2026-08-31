# `rust/vendor/` — local override of the published `bezel-gpui-linux` crate

This directory holds a `[patch.crates-io]` override for one package, `bezel-gpui-linux` (release
`0.3.8`, see `rust/Cargo.toml`), wired in via that same file's `[patch.crates-io]` section.
Everything else in the Bezel GPUI family (`gpui`, `gpui_platform`, and the rest) still comes
straight from the published crates.io release, unpatched.

## Why this exists: F-CORE-FILE-03A

Upstream `gpui_linux`'s Wayland `wl_data_device` handling
(`src/linux/wayland/client.rs`) reads the dragged `text/uri-list` off a pipe in
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
  that file for regression coverage. Every other file in this crate is byte-for-byte the released
  `bezel-gpui-linux` 0.3.8 source.
- **`gpui_platform`** is **not** patched, and `vendor/gpui_platform/` no longer exists. It was
  patched under the old zed-git-rev dependency only because that `gpui_platform`'s own
  `gpui_linux = { workspace = true }` dependency was a *path* dependency inside the zed checkout —
  a different Cargo source than the git URL the old `[patch]` targeted, so patching `gpui_linux`
  alone would never have been seen by it. The published `bezel-gpui-platform` crate takes
  `gpui_linux` as an ordinary crates.io registry dependency instead (`package = "bezel-gpui-linux"`,
  version-pinned, no path involved — verified against its published manifest), so a single
  `[patch.crates-io] bezel-gpui-linux = { path = "vendor/gpui_linux" }` entry is enough:
  `[patch.crates-io]` substitutes the vendored tree for *every* consumer resolving that package
  name from crates.io, `bezel-gpui-platform` included. The reason the second override existed is
  gone, so it was removed rather than carried forward as dead weight.

`vendor/gpui_linux/Cargo.toml` carries a comment explaining every field that had to change from
the published manifest: `[package] name` stays `bezel-gpui-linux` so it matches what
`[patch.crates-io]` targets; `[lib] name` stays `gpui_linux` so it keeps resolving as the same
import path every consumer already uses; dependencies the released manifest expresses as
`.workspace = true` (resolving against zed's own root workspace, which does not exist here) are
written out as the literal registry dependency they resolve to instead. Everything else —
features, lints, `cargo-shear` metadata, dependency versions — mirrors the released manifest
field-for-field.

## Layout notes

- `vendor/gpui_linux` carries its own `[workspace]` table so it resolves as an independent
  workspace root (Cargo refuses to treat a directory as both a `[patch]` target *and* an implicit
  member of the enclosing workspace). `rust/Cargo.toml`'s `[workspace] exclude` keeps the outer
  workspace from trying to claim it too.
- It has its own committed `Cargo.lock`, so `cargo test --manifest-path
  rust/vendor/gpui_linux/Cargo.toml` is reproducible standalone, independent of the main `rust/`
  build.
- `target/` under it is covered by `rust/.gitignore`'s bare `target/` rule (gitignore patterns
  without a leading `/` match at any depth) — never commit it; it runs to several GB.
- `LICENSE-APACHE` is copied alongside the crate, matching upstream's own per-crate licensing.

## Keeping the standalone lock honest

`vendor/gpui_linux/Cargo.lock` only proves something if it describes the same dependency graph
`rust/Cargo.lock` actually ships. For every package the two locks share, their versions must
agree — otherwise `Scripts/ci-linux.sh`'s vendored-crate stage is testing a graph that exists
nowhere else. `bezel-gpui`, `bezel-gpui-wgpu`, and `bezel-gpui-linux` are the exception: those
three are pinned exactly to `0.3.8` on purpose, everything else the family pulls in (its
`bezel-zed-*` build-dep chain) resolves to whatever later patch release satisfies the caret range,
`0.3.8` as of this writing — that's ordinary semver composition of a published family, not drift.

To re-sync after `rust/Cargo.lock` moves: `cargo update --manifest-path
vendor/gpui_linux/Cargo.toml -p <package> --precise <version>` per drifted package, matching
whatever `rust/Cargo.lock` resolved it to. Never hand-edit either lockfile.

**Trap:** don't "fix" this by pinning those `bezel-zed-*` dependencies to an exact `=0.3.8` in
`vendor/gpui_linux/Cargo.toml`. Cargo unifies each package to one version graph-wide, and the
build-dep chain above is free to resolve those packages to a later compatible `0.3.x` release; an
exact `=0.3.8` requirement inside the `[patch.crates-io]` candidate then makes that candidate
infeasible as soon as it does, and cargo falls back to the unpatched registry crate — *silently*,
with only a warning, not a build failure. The symptom is `patch ... was not used in the crate
graph` and a registry-sourced `bezel-gpui-linux` in
`rust/Cargo.lock` instead of the path-sourced one. After touching those dependency lines, always
run `cargo tree --target x86_64-unknown-linux-gnu -i bezel-gpui-linux` and confirm it resolves to
the path source with no such warning.

## Maintenance cost — read before bumping the pinned `bezel-gpui-linux` version

This is a real, standing fork of one crate, not a one-line patch. Bumping
`rust/Cargo.toml`'s pinned `bezel-gpui-linux` version requires re-diffing `gpui_linux`'s
`client.rs` against the new release and re-applying the `PendingDrop` mechanism by hand (or
dropping this override entirely, if upstream has fixed the race some other way by then — worth
checking first). Nothing else in the vendored crate should ever need hand-merging, since nothing
else was changed from upstream.
