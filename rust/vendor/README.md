# `rust/vendor/` — local overrides of three published crates

This directory holds `[patch.crates-io]` overrides for three packages: `bezel-gpui-linux` and
`bezel-gpui-windows` (both release `0.3.8`) and `libghostty-vt-sys` (release `0.2.1`), all wired
in via `rust/Cargo.toml`'s `[patch.crates-io]` section. Everything else in the Bezel GPUI family
(`gpui`, `gpui_platform`, and the rest) and `libghostty-vt` itself still come straight from the
published crates.io releases, unpatched.

Each override changes one thing about its crate and nothing else; the three are unrelated fixes
that happen to need the same mechanism. `gpui_linux` is described first and at length because it
established the arrangement; `gpui_windows` and `libghostty-vt-sys` follow it, each with its own
short section at the end.

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

## `gpui_windows` — a bitblt swap chain so translucency can show (ADR 0003)

`vendor/gpui_windows/` is the released `bezel-gpui-windows` 0.3.8 source with exactly one change:
`create_swap_chain` in `src/directx_renderer.rs` — the swap chain GPUI builds when
`GPUI_DISABLE_DIRECT_COMPOSITION` is set, which Sirio sets on Windows so the Browser surface's
WebView2 child HWND has a redirection surface to compose into (ADR 0002) — is a bit-block-transfer
swap chain (`DXGI_SWAP_EFFECT_DISCARD`, `DXGI_SCALING_STRETCH`, `DXGI_ALPHA_MODE_UNSPECIFIED`)
instead of upstream's flip-model one. The DWM composes a flip-model HWND swap chain as opaque, so
upstream's fallback path could never show the acrylic accent `set_background_appearance(Blurred)`
installs; a bitblt present copies the back buffer, alpha included, into the redirection surface,
which the DWM does composite per-pixel under an accent policy. Measured both ways on real hardware
on 2026-09-05 — see `docs/adr/0003-windows-regains-translucency-through-a-bitblt-swap-chain.md`.
The comment above the changed descriptor in the vendored file says the same in place. Every other
file is byte-for-byte the released source.

The manifest follows `gpui_linux`'s conventions (`[workspace]` table, `publish = false`, package
name kept as `bezel-gpui-windows`, `[lib] name` kept as `gpui_windows`); the released manifest
already spells every dependency out as a literal registry dependency, so nothing else had to
change. `bezel-gpui-platform` takes `gpui_windows` as a plain crates.io dependency
(`package = "bezel-gpui-windows"`), so the single `[patch.crates-io]` entry reaches every consumer,
exactly as for `gpui_linux`.

Unlike `gpui_linux`, this override carries **no standalone `Cargo.lock` and no CI test stage**: the
change is one swap-chain descriptor whose effect exists only on a live DWM, and the crate's own
tests do not cover swap-chain creation. The check that matters is visual (an isolated instance over
a loud backdrop, with a Browser tab open), and `cargo tree -i bezel-gpui-windows` confirming the
path source is used with no `patch ... was not used` warning.

Bumping the pinned `bezel-gpui-windows` version means re-applying that one descriptor change by
hand — or dropping the override, if upstream's no-DirectComposition path has become
translucency-capable by then (worth checking first, as for `gpui_linux`).

## `libghostty-vt-sys` — a portable CPU floor for the Zig-built VT parser

`libghostty-vt-sys` shells out to `zig build` to compile Ghostty's VT parser and links the result
statically into every Sirio binary. Its `build.rs` passes `-Dtarget` to Zig **only when
cross-compiling**; for a native build it passes nothing, and a Zig target with no explicit arch is
resolved by *detecting the host CPU* rather than falling back to the architecture's baseline. The
asymmetry is exactly backwards for anyone shipping binaries: the cross build is portable and the
native build is not.

Every job in `build-release.yml` builds natively — including macOS, whose `--target
aarch64-apple-darwin` names the runner's own host triple and is therefore not a cross-build. So
the parser inside each published artifact was compiled for whichever CPU that runner happened to
have.

That is not a hypothetical. The Windows installer published as **v0.9.6** cannot start on a
Ryzen 5 5600X:

```
Exception code: 0xc000001d          (STATUS_ILLEGAL_INSTRUCTION)
Fault offset:   0x38a615            (.text, i.e. the statically linked parser)

62 f2 7d 28 7a c2    vpbroadcastb ymm0, ecx   <- EVEX prefix, needs AVX512BW+VL
c4 a1 7e 7f 04 08    vmovdqu [rax+r9], ymm0   <- VEX, plain AVX2
```

The hosted Windows runner is an Intel Xeon with AVX-512; Zen 3 has AVX2 and no AVX-512. AVX2 and
AVX-512 sitting in the same routine is the signature of `-mcpu=native` codegen. Nothing downstream
can recover from this class of bug: a binary that cannot start never reaches the updater that
would replace it, so every affected user has to reinstall by hand.

### What's patched, and how little

Five lines in `build.rs`, backported verbatim from upstream
[uzaaft/libghostty-rs#73](https://github.com/uzaaft/libghostty-rs/pull/73) (merged 2026-08-14,
closing their #66): `-Dcpu` is now always passed, defaulting to `baseline` and overridable through
`LIBGHOSTTY_VT_SYS_CPU`. Every other file is byte-for-byte the published 0.2.1 source, apart from
the `[workspace]` table this arrangement needs (and the removed `Cargo.toml.orig`/`Cargo.lock`
publishing artifacts). `libghostty-vt`, the safe wrapper, is **not** patched: the vendored `-sys`
keeps 0.2.1's ghostty pin and generated bindings, so the two halves still match.

`baseline` rather than a higher floor because the *Rust* half of the binary already compiles at
the x86-64 baseline — no `target-cpu` is set anywhere in this repo. Raising the floor (`x86_64_v2`,
`x86_64_v3`) is a product decision about which machines Sirio supports, and it means raising both
halves together, deliberately. `build-release.yml` states the value at workflow level next to the
channel and the signing keys, so it is reviewable rather than inherited;
`Scripts/Tests/test-release-workflow.sh` pins it there.

### Why this is a backport and not a git dependency

Upstream's fix is merged but **unreleased**: 0.2.1 (2026-07-18) predates it and is still the newest
version on crates.io. The obvious move — `[patch.crates-io]` at upstream's merge revision — does
not work here, because every revision carrying the fix also carries a ghostty pin bump to
`22d1317`, and that tree refuses to build:

```
src/build/zig.zig:13:9: error: Your Zig version v0.15.2 does not meet the
                               required build version of v0.16.0
```

This repo pins Zig at exactly 0.15.2 in `Scripts/ci.sh`, `Scripts/ci-linux.sh`, all three
`setup-zig` steps and CLAUDE.md. The CPU fix (2026-08-14) landed *after* the ghostty bump
(2026-08-02), so there is no upstream revision that has one without the other.

[uzaaft/libghostty-rs#97](https://github.com/uzaaft/libghostty-rs/issues/97) asks for a 0.2.2.
Taking it will mean moving the whole repo to Zig 0.16 in the same change — which is the real work
this override defers, and the reason to drop the override and bump the version rather than keep
re-applying a hunk.

### Verifying the override is in effect

`cargo tree -i libghostty-vt-sys` must show the path source with no `patch ... was not used`
warning. That the flag is not a no-op is harder to see than it looks: on a machine without AVX-512
(any Zen 3, and every Intel consumer part since Alder Lake) a `native` build and a `baseline` build
both start, so "the app runs" proves nothing. The check that does discriminate is an A/B of the
produced archive —

```
cargo build -p sirio_terminal                                  # baseline
sha256sum target/debug/build/libghostty-vt-sys-*/out/ghostty-install/lib/ghostty-vt-static.lib
LIBGHOSTTY_VT_SYS_CPU=native cargo build -p sirio_terminal     # native
sha256sum target/debug/build/libghostty-vt-sys-*/out/ghostty-install/lib/ghostty-vt-static.lib
```

— which must differ. If the two hashes match, `-Dcpu` is not reaching Zig and the override is
doing nothing.
