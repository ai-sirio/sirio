# `rust/vendor/` — local overrides and Ely chat components

Sirio builds on the published `gpui-pre` 0.3.12 family (`gpui-pre`, `gpui-pre-platform` and the
backends under it). This directory holds what that family does not give us as it is: three
platform backends carrying Sirio's patches, the HTTP client gpui-pre no longer installs, and the
two Bezel crates the shell still draws with, repointed at the new engine.

`ely/` holds upstream Ely's eight crates (`ely-core`, `ely-chat`, `ely-agent`, `ely-editor`,
`ely-git`, `ely-documents`, `ely-collab`, `ely-charts`) under `crates/`, vendored at rev `8d7723ec`
and licensed MIT; they build as workspace path dependencies on the same gpui-pre packages and
the patches described below. Its `LOCAL-CHANGES.md` records provenance, the file set taken, every
path that differs from upstream with the checklist row it carries and its proof, and the visible
changes the sync accepted. `ely/check-vendor.sh` re-extracts each crate at the rev and compares the
set of differing paths (text files, binary files, files on one side only) with the list under
"Differing paths" in `LOCAL-CHANGES.md`: an unlisted difference fails, and so does a listed path that
no longer differs (`ELY_UPSTREAM=<clone> rust/vendor/ely/check-vendor.sh`). It checks paths, not
hunks: that each hunk is the one its section describes is a review by hand, and it does not look at
`ely-palette` or the facade.

`ely-gpui-component/` is not a copy of anything: it is a thin facade (`Cargo.toml` and `src/lib.rs`)
that keeps Sirio's `ely_gpui_component::*` paths over the vendored crates, with `init_chat`,
`init_editor` and the `Assets` that serve the `ely/icons/` names.

`pulldown-cmark/` is upstream Ely's patched pulldown-cmark 0.13.6 (`vendor/pulldown-cmark` at `8d7723ec`), taken verbatim and wired through `[patch.crates-io]` (L7 in `ely/LOCAL-CHANGES.md`). It is what two `ely-documents` tests need; `ely/check-vendor.sh` fails on any difference from upstream.

`ely-palette/` holds Ely's palette and syntax colours, re-extracted from `ely-core`'s theme files
at that rev, into a small crate, so a host's theme crate can use Ely's palette without compiling
Ely's components; `ely-core` re-exports those types at their original paths.

Two things the sync brings that no Sirio file asks for, both from upstream's manifests taken as they
are. `ely-core` turns on gpui's `stacker` feature (`ely-core/Cargo.toml`), and Cargo unifies features,
so every crate that links gpui has it: taffy layout and the element tree run through `stacksafe`, and
`stacksafe`, `stacker` and `psm` are new in `Cargo.lock`, `psm` with a native assembler build step.
`windows-check.yml` builds no gpui crate, so the first compile of the app on Windows with them is the
nightly after the merge. And `ely-core` and `ely-documents` embed IBM Plex Sans and the KaTeX fonts
(SIL OFL 1.1); whether a release binary keeps them is not settled, and `THIRD_PARTY_NOTICES.md`
carries their licences either way, with Ely's own MIT licence.

## What is here

| Directory | Package | Relation to upstream | Wired by |
|---|---|---|---|
| `gpui_linux/` | `gpui-pre-linux` 0.3.12 | two Sirio patches | `[patch.crates-io]` |
| `gpui_windows/` | `gpui-pre-windows` 0.3.12 | three Sirio changes | `[patch.crates-io]` |
| `gpui_macos/` | `gpui-pre-macos` 0.3.12 | one Sirio change (#810) | `[patch.crates-io]` |
| `bezel_theme/` | `bezel-theme` 0.1.4 | pristine `src/`, manifest repointed | `[patch.crates-io]` |
| `bezel_markdown/` | `bezel-markdown` 0.1.4 | four changes in `src/render.rs` (+ `lib.rs` re-export), manifest repointed | `[patch.crates-io]` |
| `reqwest_client/` | `sirio-reqwest-client` | zed's `reqwest_client`, republished by bezel | workspace member |
| `ely/crates/*` | eight upstream Ely crates at `8d7723ec` | verbatim plus the changes in `ely/LOCAL-CHANGES.md` | workspace members |
| `ely-palette/`, `ely-gpui-component/` | as above | Ely palette split out; Sirio's facade | workspace members |

macOS builds on the upstream `gpui-pre-apple` 0.3.12, unpatched, and on `gpui-pre-macos` 0.3.12
with the one change described under `gpui_macos` below. One item is open: against bezel's
`bezel-gpui-apple` 0.3.8 the Metal destination-alpha blend changed (`One` became
`OneMinusSourceAlpha` on two of the three pipelines), the same change `gpui_windows/` undoes on
Windows. A blend factor applies to every draw, and Sirio requests `Blurred` on macOS
(`rust/crates/sirio/src/shell_chrome.rs`), so framebuffer alpha reaches the screen: the blurred
window's look may differ. Checking it by hand on a Mac is owed before release; it has not been
verified. If a Mac shows a regression, the follow-up is to vendor `gpui-pre-apple` the way
`gpui_windows/` is vendored and restore the blend. Two
other differences do not touch Sirio: there is no backdrop-blur pass, and the shadow shader lost
the `inset == 2` mode. A search of `rust/crates`, `vendor/ely` and
`vendor/ely-palette` finds no use of either.

`gpui_platform` is not patched. `gpui-pre-platform` takes `gpui-pre-linux`, `gpui-pre-windows` and
`gpui-pre-macos` as ordinary registry dependencies, so one `[patch.crates-io]` entry per backend substitutes the
vendored tree for every consumer that resolves that package name from crates.io.

## How a vendored package is laid out

Every `[patch.crates-io]` package here follows the same recipe, and re-vendoring one means
repeating it:

- **Compare against the published crate, not against history.** Sirio's changes are whatever
  differs from the released `.crate`. A patched package is never fetched by a build, so download
  and unpack it into an empty directory, then diff the unpacked subdirectory against the vendored
  one:
  `curl -sL https://static.crates.io/crates/<name>/<name>-<version>.crate | tar xz -C <empty dir>`,
  then `diff -r <empty dir>/<name>-<version> rust/vendor/<dir>`. Read the manifest diff too: it
  holds real Sirio changes (`gpui_windows`'s `sirio_perf` path dependency, the `gpui-pre =0.3.12`
  repointing of `bezel_theme` and `bezel_markdown`, which is `bezel_theme`'s only change). The
  vendored `Cargo.lock` is Sirio's own standalone lock and differs from any published one;
  `Cargo.toml.orig` and `.cargo_vcs_info.json` (and a `Cargo.lock`, if the crate ships one) in the
  unpacked crate are packaging noise. The branch history that vendored the gpui crates did start
  each with a pristine commit (`06bc49ac6` for `gpui-pre-linux`, `70cb0ad06` for
  `gpui-pre-windows`, under `gpui_pre_linux` / `gpui_pre_windows`, the old directory names), but a
  squash merge to `main` erases such commits, so do not rely on one. `bezel_markdown` never had a
  pristine commit: its first commit already contained a fix.
- **A normalized manifest.** `[package] name` stays the published package name so it matches
  what `[patch.crates-io]` targets, and `[lib] name` stays the import name consumers already use;
  `publish = false`. Whatever the released manifest expresses as `.workspace = true` (zed's root
  workspace does not exist here) is written out as the literal registry dependency it resolves to.
  Dependencies on the engine are `=0.3.12` pins.
- **Its own `[workspace]` table**, because Cargo refuses a directory that is both a `[patch]`
  target and an implicit member of the enclosing workspace, plus an entry in `rust/Cargo.toml`'s
  `[workspace] exclude` (today `vendor/gpui_linux`, `vendor/gpui_windows`, `vendor/gpui_macos` and
  `vendor/bezel_theme`; `bezel_markdown` carries the table but is not listed there).
  `reqwest_client/` is the exception: it is a workspace member and carries no `[workspace]` table.
- **Its own committed `Cargo.lock`** where the crate has one (`gpui_linux`, `gpui_windows`,
  `bezel_theme`; `gpui_macos` keeps the published lock, see its section), so a standalone `cargo test --manifest-path rust/vendor/<dir>/Cargo.toml` is
  reproducible. Never hand-edit a lockfile.
- **The `[patch.crates-io]` entry by package name** in `rust/Cargo.toml`, and a comment there
  saying why.
- `target/` under any of them is covered by `rust/.gitignore`'s bare `target/` rule; never commit
  it, it runs to several GB.
- Licences: `gpui_linux`, `gpui_windows`, `gpui_macos` and `reqwest_client` carry `LICENSE-APACHE`;
  `bezel_markdown` carries the MIT `LICENSE` from the repository's `v0.1.4` tag, which the
  published crate does not ship; `bezel_theme` carries none, because the published crate has none.

After touching a dependency line in a vendored manifest, run
`cargo tree --target x86_64-unknown-linux-gnu -i gpui-pre-linux` (or `-i gpui-pre-windows` for the
Windows crate, with the Windows target; `-i gpui-pre-macos` with `--target aarch64-apple-darwin`)
and confirm it resolves to the path source with no
`patch ... was not used in the crate graph` warning. A candidate whose requirement the graph cannot
satisfy is dropped *silently*, with only that warning, and the build then uses the unpatched
registry crate.

## `gpui_linux` — two patches

`vendor/gpui_linux/` is `gpui-pre-linux` 0.3.12 plus two changes. Every other file is the
released source.

### A drop that beats a slow provider (F-CORE-FILE-03A)

Upstream's Wayland `wl_data_device` handling (`src/linux/wayland/client.rs`) reads the dragged
`text/uri-list` off a pipe in a background task kicked off by `Enter`, and only stores
`DragState::window` once that task's continuation completes. `Drop` bailed out whenever the window
was still unset: no `finish()`/`destroy()`, no event, no log. A provider slower than roughly 150 ms
reliably triggers it, since `Enter` to `Drop` can arrive faster than the read resolves, and a real
XDND drop was then discarded with no trace on either side of the protocol. Live reproduction:
`Scripts/wayland-drive.sh`'s `xdnd <x1> <y1> <x2> <y2> <file...> --delay-ms 400` against a Terminal
pane.

`DragState` now remembers the drop (`pending_drop`, with `uri_read_unresolved` and a
`uri_read_generation` that ties it to one `Enter`'s read):

- A `Drop` with the read unresolved is remembered instead of dropped.
- When the read resolves with files, the continuation delivers the transfer itself: an `Entered`,
  then a `Submit` at the last position `Motion` tracked, not `Enter`'s own, possibly stale, entry
  position.
- A `Leave` while a drop is pending is ignored, because the compositor ends a drag with a `Leave`
  after the `Drop` and the files are still being read.
- Only a newer `Enter` (a new read supersedes the old one and its pending drop) or a read that
  fails or lists no files cancels the pending drop.

The `tests` module at the bottom of `client.rs` covers it, and `Scripts/ci-linux.sh` runs the
crate's own tests (`cargo test --manifest-path vendor/gpui_linux/Cargo.toml --locked --features
test-support`; the gpui-pre copy compiles its tests only with that feature).

### Links open in the user's environment

`set_child_environment(env)` is a process-wide setter, its pairs held in a `OnceLock` in
`src/linux.rs` and re-exported from `src/gpui_linux.rs`. gpui starts `xdg-open` for
`open_with_system` and the URL opener for every `cx.open_url`, and both inherited the app's
environment: under an AppImage that is the mount's `LD_LIBRARY_PATH`, `XDG_DATA_DIRS` and GTK
variables, so the user's browser started with them. Once the setter has been called, each of those
commands is started through `with_child_environment`: environment cleared, the given pairs set,
plus `XDG_ACTIVATION_TOKEN` when gpui has one. Unset, the commands run as upstream starts them.
`src/linux/platform.rs` applies it at the `xdg-open` site and in `run_open_commands` (the
`open::commands` loop: each command is spawned and awaited, and the loop returns at the first
one that exits successfully) instead of upstream's
`environment.apply_to` when the setter has been called. The setter is first-call-wins: a later
call is ignored. `sirio`'s `main` calls it on Linux, right after `sirio_env::install`, with the
user's environment; the crate takes no dependency on Sirio's environment crate, the caller passes
the pairs in.

### Keeping the standalone lock honest

`vendor/gpui_linux/Cargo.lock` only proves something if it describes the same dependency graph
`rust/Cargo.lock` ships. For every package the two locks share, their versions must agree,
otherwise `Scripts/ci-linux.sh`'s vendored-crate stage tests a graph that exists nowhere else.
To re-sync after `rust/Cargo.lock` moves: `cargo update --manifest-path
vendor/gpui_linux/Cargo.toml -p <package> --precise <version>` per drifted package.

## `gpui_windows` — a bitblt swap chain, additive alpha, and perf spans

`vendor/gpui_windows/` is `gpui-pre-windows` 0.3.12 plus three changes:

1. **A bitblt swap chain so translucency can show (ADR 0003).** `create_swap_chain` in
   `src/directx_renderer.rs` — the swap chain GPUI builds when `GPUI_DISABLE_DIRECT_COMPOSITION`
   is set, which Sirio sets on Windows so the Browser surface's WebView2 child HWND has a
   redirection surface to compose into (ADR 0003, Context) — is a bit-block-transfer swap chain
   (`DXGI_SWAP_EFFECT_DISCARD`, `DXGI_SCALING_STRETCH`, `DXGI_ALPHA_MODE_UNSPECIFIED`) instead of
   the flip-model one. The DWM composes a flip-model HWND swap chain as opaque, so upstream's
   fallback path could never show the acrylic accent `set_background_appearance(Blurred)`
   installs; a bitblt present copies the back buffer, alpha included, into the redirection
   surface, which the DWM does composite per-pixel under an accent policy. See
   `docs/adr/0003-windows-regains-translucency-through-a-bitblt-swap-chain.md`; the comment
   above the changed descriptor says the same in place.
2. **`DestBlendAlpha` stays `ONE`.** gpui-pre 0.3.12 changed `DestBlendAlpha` to
   `INV_SRC_ALPHA` in `create_blend_state` and `create_blend_state_for_path_sprite`, the ordinary
   "over" operator for alpha. That undoes the additive alpha accumulation Windows translucency was
   measured with (a panel over the frame veil saturates to alpha 1, so panels are opaque there and
   the blur lives in the title strip and the gaps: ADR 0003 and `shell_chrome::MaterialStrength`).
   Both functions set it back to `D3D11_BLEND_ONE`, with a `Sirio-local change` comment. The
   subpixel text blend state is untouched.
3. **`sirio_perf` spans.** Four files carry them: `src/events.rs` (the window-event handlers
   `resize_input`, `mouse_input`, `char_input`, `draw_window`, and the `force_render` event),
   `src/directx_renderer.rs` (`DirectXRenderer.present`, `.draw`, `.render`), `src/platform.rs`
   (the `Windows.vsync` event) and `src/direct_write.rs` (`DirectWrite.layout_line` and
   `DirectWrite.rasterize_glyph`, with the Sirio test
   `text_trace_covers_real_layout_and_rasterization_without_content`, which checks the trace
   carries a span for each and none of the text). So the Windows perf trace has the engine's
   frame in it. The crate depends on `sirio_perf` by path
   (`../../crates/sirio_perf`). Names are content-free `&'static str`s, per
   `docs/agents/architecture/perf-trace.md`.

The manifest follows the recipe above and has its own `Cargo.lock`. There is no CI test stage for
this crate: the swap chain's effect exists only on a live DWM, and the crate's tests do not cover
swap-chain creation. The check that matters is visual (an isolated instance over a loud backdrop,
with a Browser tab open), plus `cargo tree -i gpui-pre-windows` showing the path source.

## `gpui_macos` — every window paced by CVDisplayLink (#810)

`vendor/gpui_macos/` is `gpui-pre-macos` 0.3.12 with `src/` identical to the copy Ely vendors at
ELY-Open-Source/Ely-GPUI-Components `0b983008`. That copy differs from the release in two ways:

- **The frame source.** In `MacWindow::open` (`src/window.rs`), `metal_frames` is always `None`.
  Every window therefore takes the `CVDisplayLink` frame source (`FrameSource::DisplayLink`), as
  gpui-pre 0.3.10 and bezel-gpui 0.3.8 did. The release picks a per-layer `CAMetalDisplayLink` on
  macOS 14 and later. The doc comments on `FrameSource` (`src/window.rs`) and at the top of
  `src/display_link.rs` still describe the release's choice; they are left as Ely left them.
- **Frame logging.** `log::info!` and `log::warn!` lines name when a link attaches, detaches, is
  refused, or ignores wakes, and when a window is occluded. Sirio installs no `log` logger, so these
  lines print nothing in Sirio today.

Why: #810. On the `macos-15` runner, a window in the background sometimes drew its next frame about
10 s late on gpui-pre 0.3.12. In two runs before #806 merged, host-sessions case 21 took 15 s
against 7 s on bezel-gpui 0.3.8. The final #806 run (44450730490) took 5 s, so the delay is
intermittent. The suspected cause is a `CAMetalDisplayLink` left paused: `MetalFrames::end_frame`
pauses the link after an update that neither drew nor asked for another frame. Then the link
answers only `MetalFrames::wake` or a new link that `start_display_link` builds on an occlusion,
screen or key change. This has not been confirmed on a Mac.

Ely's evidence is related but not the same symptom. On their hosted macOS CI the link stopped
answering wakes 16.5 minutes after it attached and sent again 5 to 8 s later. Their pixel job
passed on gpui-pre 0.3.10 (`CVDisplayLink`) and failed on every run after 0.3.11 moved to
`CAMetalDisplayLink`.

The cost: on macOS 14 and later, a visible window now gets a display-link tick at the screen's
rate for as long as it stays visible, idle or not, where the Metal link paused itself. Each tick
runs the window's request-frame callback, which draws only when something asked for a frame. This
is what bezel-gpui 0.3.8 did on every macOS version.

The manifest follows the recipe above: `publish = false` and an empty `[workspace]` table, with no
dependency changed. Its `Cargo.lock` is the lock the published crate ships, unchanged and not
synced with `rust/Cargo.lock`. The crate builds only on macOS, so no stage runs it standalone and
the lock proves nothing; it is kept only so the directory matches the release. To check that the
patch is in use, run `cargo tree -i gpui-pre-macos --target aarch64-apple-darwin` and look for the
path source. Proof that the fix works has to come from a Mac: put Sirio in the background, open a
tab with `sirioctl`, and time its first render (#810, To do). The `macos-check.yml` host-sessions
timings can show a regression, but a green run proves nothing, because the delay was intermittent.

## `bezel_theme` — repointed, source pristine

`vendor/bezel_theme/` is `bezel-theme` 0.1.4 with its `src/` byte-for-byte the release. The
published crate depends on `bezel-gpui`, an engine this workspace no longer builds, so only the
manifest changed: the `gpui` dependency is `gpui-pre =0.3.12`, and the usual normalization
(`[workspace]`, `publish = false`, own `Cargo.lock`, `exclude`d from the root workspace) applies.
`sirio_theme`, `sirio_ui`, `sirio_syntax` and `sirio_terminal` take it by its published name, and
`bezel-markdown` and `bezel-syntax` take it transitively (`bezel-syntax` is not vendored but gets
the patched copy all the same); the `[patch.crates-io]` entry substitutes it for every one.

## `reqwest_client` — the HTTP client gpui-pre no longer installs

`gpui-pre-platform`'s `application()` starts an `Application` with an HTTP client that refuses
every request, where the engine Sirio ran on before installed `ReqwestClient::new()`.
`vendor/reqwest_client/` (package `sirio-reqwest-client`, version `0.3.8+zed.82aeef`, lib name
`reqwest_client`) is zed's `reqwest_client` as the bezel project republished it, with its
`http_client` and `gpui_util` dependencies moved to `gpui-pre-http-client` / `gpui-pre-util`
`=0.3.12`. It is a workspace member (`rust/Cargo.toml`'s `reqwest_client` workspace dependency
names it by path), not a patch. Its TLS half stays on the bezel package: `http_client_tls` is
still `bezel-zed-http-client-tls` 0.3.8 (`vendor/reqwest_client/Cargo.toml`), the one
`bezel-zed-*` crate left in `rust/Cargo.lock`. `sirio_ui::app_platform::application`
(`rust/crates/sirio_ui/src/app_platform.rs`) wraps `gpui_platform::application()`
and installs `ReqwestClient::new()` with no user agent, proxy override or extra header; every app,
example and probe starts through it, so remote images load the same in each. A program that built
its `Application` straight from `gpui_platform` would fail those fetches silently.

## `bezel_markdown` — a selection that covers each wrapped row from its first glyph, a bar under wide blocks, and text that follows the interface size

The manifest change is the same as `bezel_theme`'s: `gpui` is `gpui-pre =0.3.12`, and nothing else
in the manifest moved. The engine swap touched no source; the source changes below are older and
unrelated to the engine.

Note that `src/lib.rs` (the crate docs, around line 22) still says `gpui_platform::application`
installs the HTTP client an image at an `http` URL needs. That is stale: gpui-pre's platform
installs none. Only that comment is left as released, on purpose (the source changes below are
the other edits);
`sirio_ui::app_platform::application` is what installs it.

`vendor/bezel_markdown/` is the released `bezel-markdown` 0.1.4 source with four changes. The first:
`range_rects` in `src/render.rs`, the function that turns a byte range into one rectangle per
visual row for the selection, the inline-code wash and the chips. Upstream 0.1.4 walked a range
row by row through `TextLayout::position_for_index`, finding each row's last index by bisection
and starting the next row one index later. gpui places a soft-wrap boundary — the first glyph of
the row below — at the *end of the row above*, so every wrapped row's rectangle began at its second
glyph: a selected chat response or Markdown Preview showed its rows' first letters unwashed, while
Copy read them. The function is upstream 0.2.16's, which walks each line's `wrap_boundaries`
directly, with one difference: the origin comes from `position_for_index(0)` rather than
`TextLayout::bounds`, which panics on a layout with no bounds (it still does on gpui-pre 0.3.12: `elements/text.rs` unwraps `bounds`). 0.2.16 itself is out
of reach as a bump — it builds on gpui 0.3.19 and moves the whole bezel family off `=0.1.4`.
Every other file except `src/lib.rs` (which re-exports `TextOffset`, below) is byte-for-byte the released source, plus the MIT `LICENSE` from the repository's
`v0.1.4` tag, which the published crate does not ship.

Same manifest conventions as `gpui_windows` (`[workspace]` table, `publish = false`, package name kept
as `bezel-markdown`), no standalone `Cargo.lock`; `cargo tree -i bezel-markdown` shows the path source,
used by `sirio_ui` alone. The check is
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
scroller (a child would scroll away with the content; upstream `bezel-ui` 0.1.4's bar is vertical only, and
`bezel_markdown` takes no dependency on it or on Ely), and the fence's padding moved onto an inner
column. The thumb stops a left mouse-down from propagating, because a host such as the chat's
`MarkdownBody` starts a transcript selection from any left press on a response; the drag still
starts. Upstream 0.2.x carries the same through `ui::scroll::Viewport`. Bumping bezel past 0.1.4
drops both changes, since 0.2.x carries the function and the viewport. The second change's check is
`Scripts/Tests/test-markdown-scroll-probe.sh --xvfb` (`MARKDOWN SCROLL PROBE OK`), next to the
selection probe's: it draws `sirio_ui`'s `markdown_scroll_probe` example and drags the real thumb under
a fence and a table, in dark and light.

The third is the text size. Upstream 0.1.4 draws every size and line height at a fixed pixel value
(body 14/22, code 12.5/18, headings 19 to 14, the captions smaller at 11.5 and the cards at 12), so Sirio's
Interface font size changed everything around a chat reply or a Markdown Preview and left the text
itself alone. `render.rs` now has a `TextOffset(f32)` gpui `Global` (default 0), re-exported from the
crate root, which `render_with_selection` (the one `render` and `markdown` go through) reads once per call into a thread-local; the
private `sized(points)` helper turns each of those constants into `points + offset` (never below 6 px)
at every text size and text line height, and the list marker column widens in proportion so numbers,
bullets and checkboxes keep lining up, list indents widen with it so a nested level still clears its
marker, the code block's Copy button grows with the text and stays centred on the header band, and a bookmark card's fixed height grows by the offset once
for each of its four lines so its footer is not clipped. Spacing, paddings, radii and the
glyphs inside fixed boxes (the checkbox tick, the favicon initial) stay as they were. Sirio's theme bridge,
`sirio_ui::ely::sync_theme_if_changed`, sets it to `base_size - 14.5`, so the offset is 0 at the default
size and the markdown draws pixel for pixel as before. The check is the unit test
`rendered_markdown_follows_the_interface_font_size` in `sirio_ui`'s `markdown_preview.rs`. Bumping
bezel past 0.1.4 means re-applying this change by hand, unless upstream has its own way to set the
size by then (worth checking first).

The fourth is the link click. Upstream 0.1.4's `InteractiveText` click on a link in a paragraph
calls `cx.open_url(url)` for every URL, so a `file:` link (Sirio draws the file paths a reply names
as `file://` links) went to the system opener, which hands the file to whatever owns its type, and
bezel arms that click on any mouse button. `render.rs`'s `is_file_url` now makes that click do
nothing for a URL whose scheme is `file` in any letter case, read the way a URL parser reads it
(ASCII tab, LF and CR dropped, leading controls and spaces trimmed); every other URL opens as
before. What a `file:` link does is the host's to decide: `sirio_ui`'s chat opens it through
`ChatEvent::OpenFileAt`. The guard covers bezel's own click handler only; a host that calls
`cx.open_url` itself is outside it (the known ones are `change_request_tab.rs`'s `open_links` and
`file_view.rs`, which falls back to it for a link it did not resolve). The bookmark chip and card
clicks are not touched (they open a web URL).
The check is the unit test `only_a_file_link_never_reaches_the_system_opener` in `sirio_ui`'s
`selectable_markdown.rs`. Bumping bezel past 0.1.4 means re-applying this change by hand.
