# `rust/vendor/` — local overrides and Ely chat components

`ely-gpui-component/` is the pinned Ely chat/agent component source, licensed
MIT OR Apache-2.0 and built as a workspace path dependency. Its
`LOCAL-CHANGES.md` records provenance, the supported module closure and the
compatibility changes. It uses the same published Bezel GPUI packages and
the patches described below.

`ely-palette/` extracts Ely's upstream `src/theme/palette.rs` and
`src/theme/syntax.rs` at `e17e31a6890c09ebcfa8b61133d7bc7c625edf69` into a
small crate, so a host's theme crate can use Ely's palette without compiling
Ely's components; `ely-gpui-component` re-exports those types at their original
paths.

This directory holds `[patch.crates-io]` overrides for four packages: `bezel-gpui-linux` and
`bezel-gpui-windows` (both release `0.3.8`) and `bezel-ui` and `bezel-markdown` (both release
`0.1.4`), all wired in via `rust/Cargo.toml`'s `[patch.crates-io]` section. Everything else in the
Bezel GPUI family (`gpui`, `gpui_platform`, and the rest) still comes straight from the published
crates.io releases, unpatched.

Each override changes one thing about its crate and nothing else (`bezel_markdown` changes two, both
in `src/render.rs`); the four are unrelated fixes that happen to need the same mechanism.
`gpui_linux` is described first and at length because it established the arrangement; `gpui_windows`, `bezel_ui` and `bezel_markdown`
follow it, each with its own short section at the end.

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
  that file for regression coverage. Apart from that fix and the child environment described under
  `gpui_linux — links open in the user's environment` below, every file in this crate is
  byte-for-byte the released `bezel-gpui-linux` 0.3.8 source.
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

## `gpui_linux` — links open in the user's environment

`set_child_environment(env)` is a process-wide setter, its pairs held in a `OnceLock` in
`src/linux.rs` and re-exported from `src/gpui_linux.rs`. gpui starts `xdg-open` for
`open_with_system` and the URL opener for every `cx.open_url`, and both inherited the app's
environment: under an AppImage that is the mount's `LD_LIBRARY_PATH`, `XDG_DATA_DIRS` and GTK
variables, so the user's browser started with them. Once the setter has been called, each of those
commands is started with its environment cleared and the pairs given instead
(`with_child_environment`, applied in `open_with_system`, `open_uri_internal` and the file-manager
fallback of `reveal_path_internal` in `src/linux/platform.rs`; the last replaces
`open::that_detached` with the same `open::commands` loop, first command that starts wins, nothing
waits). `sirio`'s `main` calls it on Linux, right after `sirio_env::install`, with the user's
environment. The crate takes no dependency on Sirio's environment crate: the caller passes the pairs
in.

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

## `bezel_ui` — a text field that can leave its fill to its host

`vendor/bezel_ui/` is the released `bezel-ui` 0.1.4 source with exactly one change: `TextField`
gains `with_transparent_fill()`, which skips the field's `.bg(theme.input_bg)` in `src/input.rs`.
The chat composer is the one caller. Its shell (Ely's `PromptInput::custom`) paints the theme's
`surface` and the field inside it clips its own border away; without this the field painted the
app-wide input fill (`ely.sunken`, by `sirio_theme`'s contract) over almost the whole card, so the
card could only ever be `sunken` — a grey slab that matched nothing else in the theme. A builder
flag rather than a colour, because the theme can change under a live field and only a host that
reads it every frame stays right. Every other file is byte-for-byte the released source, plus
the MIT `LICENSE` from the repository's `v0.1.4` tag, which the published crate does not ship.

The manifest follows `gpui_windows`'s conventions (`[workspace]` table, `publish = false`, package
name kept as `bezel-ui`, `[lib] name` kept as `ui`); no standalone `Cargo.lock`. Check with
`cargo tree -i bezel-ui` (the path source, no `patch ... was not used` warning). Bumping the pinned
bezel version means re-applying the one builder by hand, or dropping the override if upstream has
gained a way to style a field's fill by then.

## `bezel_markdown` — a selection that covers each wrapped row from its first glyph, and a bar under wide blocks

`vendor/bezel_markdown/` is the released `bezel-markdown` 0.1.4 source with two changes. The first:
`range_rects` in `src/render.rs`, the function that turns a byte range into one rectangle per
visual row for the selection, the inline-code wash and the chips. Upstream 0.1.4 walked a range
row by row through `TextLayout::position_for_index`, finding each row's last index by bisection
and starting the next row one index later. gpui places a soft-wrap boundary — the first glyph of
the row below — at the *end of the row above*, so every wrapped row's rectangle began at its second
glyph: a selected chat response or Markdown Preview showed its rows' first letters unwashed, while
Copy read them. The function is upstream 0.2.16's, which walks each line's `wrap_boundaries`
directly, with one difference: the origin comes from `position_for_index(0)` rather than
`TextLayout::bounds`, which panics in gpui 0.3.8 on a layout with no bounds. 0.2.16 itself is out
of reach as a bump — it builds on gpui 0.3.19 and moves the whole bezel family off `=0.1.4`.
Every other file is byte-for-byte the released source, plus the MIT `LICENSE` from the repository's
`v0.1.4` tag, which the published crate does not ship.

Same manifest conventions as `bezel_ui`, no standalone `Cargo.lock`; `cargo tree -i bezel-markdown`
shows the path source, used by `sirio_ui` and by `bezel-editor`. The check is
`Scripts/Tests/test-markdown-selection-probe.sh --xvfb` (`MARKDOWN SELECTION PROBE OK`): it draws
`sirio_ui`'s `markdown_selection_probe` example, a selected document whose rows soft-wrap, break
inside one paragraph and wrap an inline code span, and fails when any row's wash leaves the text's
left edge bare — as it does with this override commented out.

The second is the horizontal scroll of a wide fenced block and a wide table. Upstream 0.1.4 draws
both as `overflow_x_scroll` containers with no scroll handle and no bar, so wide content is reachable
only with Shift+wheel and nothing says it is there; and the fence's horizontal padding sat on the
scroll container itself, whose trailing padding is not part of what it scrolls to, so the last
characters of a long line stayed under the right edge. Each block now keeps a `ScrollHandle`
(`Sideways`, a `RenderOnce`, so the state is keyed under the element ids above it and two documents on
one page do not share block 0's offset), draws a private horizontal bar as an overlay beside the
scroller (a child would scroll away with the content; `bezel-ui` 0.1.4's bar is vertical only, and
`bezel_markdown` takes no dependency on it or on Ely), and the fence's padding moved onto an inner
column. The thumb stops a left mouse-down from propagating, because a host such as the chat's
`MarkdownBody` starts a transcript selection from any left press on a response; the drag still
starts. Upstream 0.2.x carries the same through `ui::scroll::Viewport`. Bumping bezel past 0.1.4
drops both changes, since 0.2.x carries the function and the viewport. The second change's check is
`Scripts/Tests/test-markdown-scroll-probe.sh --xvfb` (`MARKDOWN SCROLL PROBE OK`), next to the
selection probe's: it draws `sirio_ui`'s `markdown_scroll_probe` example and drags the real thumb under
a fence and a table, in dark and light.
