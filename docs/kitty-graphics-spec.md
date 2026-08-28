# Rendering Kitty graphics in the terminal pane

Implementable spec produced by wayfinder map
[#263](https://github.com/tillerai/tiller/issues/263). Every requirement below
traces to a measurement or a closed decision ticket; this document assembles
them, it does not re-decide them.

**Scope.** Paint Kitty graphics protocol images inside a Tiller terminal pane.
Sixel and iTerm2 OSC 1337 are out of scope by the map's decision: both CLIs
that emit inline images prefer Kitty when it is offered.

**Not in scope**, each named so nobody has to re-derive that it was considered:
rectangle image regions, virtual placements and the Unicode placeholder
protocol, images across session save/restore, and what selecting over a
placement copies.

## Verified state

Everything in this section was measured on this tree, on Windows, not inferred
from reading.

| question | answer | how |
|---|---|---|
| Does `libghostty-vt` answer the Kitty support probe? | **Yes** — `_Gi=31;OK` back through `on_pty_write` | [#264](https://github.com/tillerai/tiller/issues/264), pinned as a test |
| Is `RenderImage` BGRA or RGBA? | **BGRA** | [#268](https://github.com/tillerai/tiller/issues/268), rendered and sampled |
| Does Tiller already link a PNG decoder? | **Yes** — `image` 0.25.10 with `png`, in the `tiller` binary | [#266](https://github.com/tillerai/tiller/issues/266) |
| Does pi probe for graphics at startup? | **No** — 45s, TUI up, zero graphics APCs | [#264](https://github.com/tillerai/tiller/issues/264) |

| Do codex or opencode probe? | **No** — zero graphics APCs from either | [#264](https://github.com/tillerai/tiller/issues/264) |
| Does omp probe? | **No** — 2×60s, TUI up, 2.0M and 2.4M bytes captured, zero graphics APCs; unchanged when handed `TERM_PROGRAM=ghostty` and `KITTY_WINDOW_ID` | [#264](https://github.com/tillerai/tiller/issues/264), pty capture with a `cmd.exe` control |
| What are omp's startup probes, then? | Kitty **keyboard** protocol, OSC 11 background colour, OSC 99 notifications, DECRQM private modes. No graphics query anywhere in `#attachInput` | [#264](https://github.com/tillerai/tiller/issues/264), read from the shipped `@oh-my-pi/pi-tui` source |
| How does pi decide, then? | **Environment variables only.** `detectCapabilities` reads `TERM_PROGRAM`, `TERM`, `KITTY_WINDOW_ID`, `GHOSTTY_RESOURCES_DIR`, `WEZTERM_PANE`. No terminal query anywhere in it. | [#264](https://github.com/tillerai/tiller/issues/264), read from the shipped bundle |

Three consequences that change the map's own framing:

- **The probe is already answered today.** A guest that asks gets `OK` from a
  pane that draws nothing. It is the *rendering* that is missing, not the reply.
- **`ESC [ ? u` in a capture is the Kitty _keyboard_ protocol**, not graphics.
  The two share a vendor name and nothing else. Pi and opencode both emit the
  keyboard query at startup and no graphics APC at all.
- **The gate is the environment, not the protocol.** Tiller gives a pane
  `TERM=xterm-256color` and `COLORTERM=truecolor` and nothing else
  (`tiller_terminal/src/lib.rs:1130-1131`). Run that through pi's tree and every
  image branch misses, so pi resolves to `images: null` and will not emit a
  graphics byte **whatever the pane replies to an APC query**.

  The map's binding decision reads "the probe alone is actively harmful: Pi and
  omp degrade gracefully to a placeholder *because* nobody answers." Measured:
  pi degrades because `TERM_PROGRAM` is unset. The harmful case cannot occur for
  pi — and neither can the useful one.

## Requirements

### 1. Arrival

- **R1.1** No reply is composed by Tiller. `libghostty-vt` answers the support
  probe itself, through the same `on_pty_write` channel the pane already
  flushes each poll iteration for DSR and DA1.
- **R1.2** A test pins that behaviour
  (`kitty_graphics_query_answer_comes_from_the_crate_or_not_at_all`), because a
  crate upgrade that stopped replying would hand the pane a reply it does not
  build — and since both emitting CLIs stay silent until answered, the symptom
  would be images that never arrive rather than an error.
- **R1.3** `set_apc_max_bytes_kitty` and `kitty_image_storage_limit` are set
  explicitly rather than left at their defaults. Neither bounds the
  _decompressed_ size; see R2.4.
- **R1.4** **The pane declares image support through the environment**, not
  only by answering a query. Pi decides from `TERM_PROGRAM` / `TERM` /
  `KITTY_WINDOW_ID` / `GHOSTTY_RESOURCES_DIR` / `WEZTERM_PANE` before it sends
  anything, and Tiller's current two variables miss every image branch — so
  today pi cannot emit a graphics byte no matter what R1.1 replies.

  Which claim to make is a real decision, not a lever to pull blind: asserting
  a terminal identity means honouring it, and [#86](https://github.com/tillerai/tiller/issues/86)
  already settled that Tiller's identity is `ai.tiller.Tiller`. Claiming
  `TERM_PROGRAM=ghostty` buys pi's `images: "kitty"` branch and buys with it
  every other behaviour a guest keys off "this is ghostty".

  **The first milestone is this one line, not the renderer** — but it needs a
  discriminating test, and the obvious one is not. Running pi with
  `TERM_PROGRAM=ghostty` and watching for graphics APCs was measured and gives
  `APC count: 0`, exactly as it does without the variable: pi emits nothing
  while idle either way, because the environment decides what it *may* send,
  not what it *has* to send. A test that returns the same answer whether or not
  the change worked is not a test.

  The milestone therefore needs **content**: an agent turn that actually
  produces an image, run once with the variable and once without. That costs a
  real model turn, which is why it is not done here.

  **For omp there is a narrower lever than an identity claim.** Its vendored
  `pi-tui` reads `PI_FORCE_IMAGE_PROTOCOL` (`kitty` / `iterm2` / `sixel`, plus
  an `off` kill switch) and treats it as pinning the choice — its own comment
  says a runtime capability probe must not override it. That turns images on
  and nothing else on, so it does not buy the rest of "this is ghostty" the way
  `TERM_PROGRAM` does, and it leaves #86's `ai.tiller.Tiller` identity intact.

  It is **omp-only**, despite the `PI_` prefix: the variable is absent from
  pi's own shipped tree, which is a separate copy of the same lineage. So the
  decision does not collapse into one setting — omp can be enabled honestly
  today, while pi still costs an identity claim or nothing.

### 2. Decoding

- **R2.1** Decoding uses the workspace's existing `image` 0.25.10 with the
  `png` feature, already linked by the `tiller` binary. `libghostty-vt`'s
  non-default `png` feature is **not** enabled: it would add a second PNG
  implementation for no capability the tree lacks.
- **R2.2** `tiller_terminal` gains an `image` dependency. It has none today.
- **R2.3** **Decoding does not happen on the terminal's owner thread.**
  `Image<'t>` borrows the terminal and the terminal is `!Send`, so the
  convenient placement puts an arbitrary-sized decode on the thread that drains
  the PTY. The compressed bytes, format and dimensions are copied out as plain
  data — the same shape [#259](https://github.com/tillerai/tiller/issues/259)
  used to keep `Selection` off the paint path — and decoded elsewhere.
- **R2.4** A decompression ceiling is applied where the decode lands. A
  ZlibDeflate payload that is small on the wire and enormous inflated is the
  zip-bomb shape, and neither existing knob bounds it.
- **R2.5** Kitty's `f=24`/`f=32` are RGB/RGBA. Every image swaps R and B before
  `RenderImage::new`, because `RenderImage` is BGRA.
- **R2.6** A format the pane refuses is **visible in the pane**. A silent drop
  is indistinguishable from the bug this work exists to fix, and is now worse
  than it looks: per R1.1 the guest has already been told `OK`.

### 3. Painting

- **R3.1** The prepaint state gains **three** placement lists, not four.
  Kitty's `Layer` is `All` / `BelowBg` / `BelowText` / `AboveText`, and `All`
  means _no filtering_ — the default single-pass mode, not a fourth bucket.
  `set_layer` does the bucketing, three times.
- **R3.2** They drain into the order `paint` already has:

  | layer | drains |
  |---|---|
  | `BelowBg` | before `backgrounds` |
  | `BelowText` | between `backgrounds` and `lines` |
  | `AboveText` | after `lines` |

  Three insertion points into an existing order. No interleave-by-z pass.

- **R3.3** A placement scrolled partly off the top is clipped by **passing the
  untruncated geometry**: full `image_bounds` with its negative origin, plus a
  `bounds` clipped to the pane. `Window::paint_image` computes
  `visible_bounds = bounds.intersect(&image_bounds)` and derives the atlas
  sub-rectangle itself. No `with_content_mask`, no hand-computed `source_rect`.

  Worth stating in review terms: the tempting version — clamp `viewport_pos`
  to zero and shrink the size — renders a _squashed_ image rather than a
  clipped one, and looks close enough to right to ship.

- **R3.4** Background quads under a placement still paint. Kitty images carry
  alpha, so a suppressed cell run shows the window behind wherever the image is
  transparent. Suppressing them for opaque images is an optimisation available
  later, not a correctness requirement.
- **R3.5** Placements are read through `placement_render_info`, one call each,
  not by reading fields piecemeal. `prepaint` is the hot path the Raspberry Pi
  5 budget constrains.

### 4. Caching and release

- **R4.1** Tiller's cache key is `(image_id, generation)`. `image_id` alone is
  wrong because guests reuse ids — which is what `Image::generation()` exists
  to disambiguate — and the failure mode is a replaced image still showing old
  pixels, which reads as a Tiller caching bug rather than a key that was too
  narrow.
- **R4.2** gpui's atlas key is `(RenderImage::id, frame_index)` and
  `RenderImage::new` assigns a fresh process-unique id every call. That key is
  an **output**, not a choice: constructing a `RenderImage` _is_ the cache
  miss. R4.1's map exists to avoid constructing a second one for the same
  pixels.
- **R4.3** The cache lives on the terminal entity, beside the other per-pane
  state — not in `TerminalPaintState`, which is rebuilt every prepaint.
- **R4.4** `Graphics::generation()` gates the re-scan. A steady pane costs one
  integer comparison per frame, not a placement walk.
- **R4.5** `Window::drop_image` is called on **four** events. Missing any one
  leaks GPU memory for the process lifetime:

  | event | why |
  |---|---|
  | image replaced | a new `generation` supersedes the old `RenderImage` |
  | placement deleted | Kitty has delete commands |
  | scrollback trimmed | an image scrolled out can never be shown again |
  | pane closed | a `Drop` on the per-pane state must guarantee it |

- **R4.6** The atlas ceiling and `kitty_image_storage_limit` are decided
  together and stated as one number. A terminal limit generous enough to retain
  200 images, with a cache that uploads all of them, is an atlas policy nobody
  chose.
- **R4.7** Eviction is least-recently-**painted**, not least-recently-added: an
  image the user is looking at must outlive a burst scrolling past. Nothing on
  screen is dropped silently (R2.6).

## Open, and deliberately left so

- **Where the cursor sits relative to an `AboveText` placement.** Kitty does
  not say, and the pane paints the cursor last today. Both answers are
  defensible; neither is derivable from the protocol.
- **Whether pi probes lazily**, at the moment it first has an image. Measured
  absent at startup; the lazy case is untested and is the likelier design.
*(omp's probe bytes were the third item here. They are now measured, above.)*
