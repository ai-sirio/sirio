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

Two consequences that change the map's own framing:

- **The probe is already answered today.** A guest that asks gets `OK` from a
  pane that draws nothing. The map's decision that "the probe answer and the
  rendering ship together" still holds, but it now costs nothing to honour —
  the answer is already going out, and it is the *rendering* that is missing.
- **`ESC [ ? u` in a capture is the Kitty _keyboard_ protocol**, not graphics.
  The two share a vendor name and nothing else. Pi emits the keyboard query at
  startup and no graphics APC at all.

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
- **omp's probe bytes.** Not installed on the machine that produced this spec.
