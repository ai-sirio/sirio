# The Ghostty render boundary: how `tiborvass/zed` and `Xuanwo/gpui-ghostty` own a `!Send` terminal

This document answers one question — **given that every `libghostty-vt` type is `!Send` and `!Sync`, how does a working GPUI application own the terminal, and what crosses the thread boundary instead?** It exists because Tiller today shares an `Arc<FairMutex<Term>>` between the alacritty event-loop thread and the GPUI paint thread, and that arrangement cannot survive the swap unchanged (issue [#27](https://github.com/tillerai/tiller/issues/27)). Two projects have already done the work; this reads their code rather than their descriptions of it.

Facts gathered on **2026-08-24**. Both projects are moving targets and both pin pre-1.0 dependencies, so every revision is named below.

## Source discipline used here

Only primary sources: the source of `tiborvass/zed@libghostty` at its head commit, the source of `Xuanwo/gpui-ghostty` at its head commit, the published `libghostty-vt` 0.1.1 crate source from crates.io, `herdrdev/herdr`'s own files fetched from GitHub, and the Zed discussion thread quoted verbatim from the GitHub API. No blog posts, no summaries. Where a claim is about behaviour I did not execute, it is written as an observation about the code, not about the running program. Where I could not establish something, Part 6 says so instead of reasoning it out.

Commit-pinned reading copies were made by cloning each repository; file paths and line numbers below refer to those revisions:

| Project | Revision read | Date |
|---|---|---|
| `tiborvass/zed` branch `libghostty` | `62911d02cca8e9a960519ec4c17b715b3eff52a4` | 2026-05-19 |
| `Xuanwo/gpui-ghostty` branch `main` | `e3025981c6211dd7db2a825dc364ffb5d342f45e` | 2026-01-08 |
| `libghostty-vt` (crates.io) | `0.1.1`, published 2026-03-28 | — |
| `herdrdev/herdr` branch `master` | fetched via GitHub contents API | 2026-08-24 |

---

# Part 0 — The claim being tested

The obstacle named by a Zed maintainer, quoted in full from the thread the ticket cites:

> "There's nothing on the roadmap albeit it would be nice to be able to use different emulators indeed.
> IIRC, it's not that trivial to plug Ghostty into Zed due to the way rendering is happening in both programs, not allowing to unite both worlds — maybe I'm wrong and somebody can come up with the PR."
> — SomeoneToIgnore, 2024-09-20, <https://github.com/zed-industries/zed/discussions/18129#discussioncomment-10706723>

Note what the sentence is actually about: reusing **Ghostty's renderer** inside Zed. `libghostty-vt` did not exist in a consumable form when that was written; it is VT state only and ships no renderer, which removes the specific obstacle described. The two projects below both keep their own GPUI renderer and take only the VT core — which is the same shape Tiller's map already chose (issue #27, "Tiller keeps its own GPUI layer").

The `!Send` constraint is separate from that quote and is stated by the binding crate itself:

> "All `libghostty-vt` objects are **not** thread-safe, and have been marked `!Send + !Sync` accordingly. The expectation is for them to be managed by a single thread, that may communicate with other threads via channels."
> — `libghostty-vt` 0.1.1, `src/lib.rs:19-23`

The marking is structural rather than declarative: every opaque handle is wrapped in `pub(crate) struct Object<'alloc, T> { ptr: NonNull<T>, _phan: PhantomData<&'alloc GhosttyAllocator> }` (`src/alloc.rs:40-46`), and `NonNull<T>` is neither `Send` nor `Sync`. There is no `impl !Send`; there is simply a raw pointer, so the only way to move one across a thread boundary is `unsafe impl Send`.

---

# Part 1 — `tiborvass/zed`: does it exist, on what, how stale

## 1.1 The branch

`tiborvass/zed` exists and is a fork of `zed-industries/zed` (`gh api repos/tiborvass/zed` → `"fork": true`, `"parent.full_name": "zed-industries/zed"`). It has two branches:

| Branch | Head |
|---|---|
| `main` | `d42edc15f9868f38bea566faabe92d0bb16a3501` |
| `libghostty` | `62911d02cca8e9a960519ec4c17b715b3eff52a4` |

The work is **six commits**, all authored by Tibor Vass and signed off, sitting on upstream Zed commit `3bd9d13b63fc5a5ffa39326597bc4fd91adc82d1` ("settings: Fix inverted VS Code import for `files.simpleDialog.enable` (#55678)", 2026-05-15):

```
62911d02cc  Bundle libghostty in macOS builds                 2026-05-19
a2bc8a2ee8  Restore Ghostty terminal speed optimizations      2026-05-19
58e595c0ef  Build terminal layout from render cells           2026-05-19
1665eaa2f8  Optimize terminal background rendering            2026-05-19
db1d956837  Batch Ghostty PTY ingestion                       2026-05-19
1775fe0836  Add Ghostty terminal backend bridge               2026-05-19
```

**Staleness.** As of 2026-08-24, `zed-industries/zed@main` is **1,859 commits ahead** of the base commit (`gh api repos/zed-industries/zed/compare/3bd9d13b63...main` → `ahead_by: 1859`, `behind_by: 0`). The branch has not been touched since 2026-05-19. Zed's terminal crate is not frozen upstream, so a rebase is not free.

**No pull request exists.** Querying every PR ever opened against `zed-industries/zed` for `user.login == "tiborvass"` returns nothing, and a title search for "ghostty" in that repository returns only an unrelated 2024 crash report (#9493). The branch has never been proposed upstream.

## 1.2 Does it build?

**I did not build it, and there is no recorded evidence that the author's CI did either.** Three separate facts:

1. `gh api repos/tiborvass/zed/actions/runs?branch=libghostty` returns `total_count: 0`. No workflow has ever run on this branch. The combined commit status for the head is `pending` with `total_count: 0`.
2. The Ghostty backend is compiled only on macOS and Linux — `crates/terminal/Cargo.toml` gains a `[target.'cfg(any(target_os = "macos", target_os = "linux"))'.dependencies]` section for `libghostty-vt` and `portable-pty`. This machine is Windows, where the new code path is entirely `#[cfg]`-ed out; a Windows build would prove nothing about it.
3. Building it requires a Zig toolchain plus a full Zed build. `docs/src/development/macos.md` gains: "Install [Zig](https://ziglang.org/download/) and ensure `zig` is available in your `PATH`. This is required by Zed's Ghostty terminal backend, which builds `libghostty-vt` from source."

Two dependency facts that bear on whether it *would* still build: the pinned `libghostty-vt = "0.1.1"` is still present and unyanked on crates.io (versions published: 0.1.0, 0.1.1, 0.2.0, 0.2.1 — the current maximum is 0.2.1, published 2026-07-18), and the vendored sys crate pins Ghostty at `bebca84668947bfc92b9a30ed58712e1c34eee1d` (2026-03-24, "vt: handle pixel sizes and size reports in `ghostty_terminal_resize` (#11818)"), whose `build.zig.zon` declares `.version = "1.3.2-dev"` and `.minimum_zig_version = "0.15.2"`. So the toolchain floor is Zig 0.15.2, not the 0.16.x the 0.2.x line requires.

## 1.3 The shape of the diff

23 files, +5,835 / −464 against `crates/terminal` and its surroundings:

```
 Cargo.lock                                     |  211 +-
 Cargo.toml                                     |    2 +
 crates/gpui/src/color.rs                       |   34 +   <- vertical_split_background
 crates/gpui/src/scene.rs                       |   34 +   <- Scene::insert_quads (batched quads)
 crates/gpui/src/style.rs                       |    1 +
 crates/gpui/src/window.rs                      |   26 +   <- Window::paint_quads
 crates/gpui_macos/src/shaders.metal            |    7 +-
 crates/gpui_wgpu/src/shaders.wgsl              |   15 +-
 crates/gpui_windows/src/shaders.hlsl           |   11 +-
 crates/repl/src/outputs/plain.rs               |   48 +-
 crates/terminal/Cargo.toml                     |    4 +
 crates/terminal/src/ghostty.rs                 |  853 ++++++++      <- the whole bridge
 crates/terminal/src/pty_info.rs                |   18 +-
 crates/terminal/src/terminal.rs                |  772 +++++++-
 crates/terminal_view/src/terminal_element.rs   | 1225 +++++++++---
 docs/src/development/macos.md                  |   10 +
 script/bundle-mac                              |   34 +
 vendor/libghostty-vt-sys/...                   | 2994 +++            <- vendored FFI crate
```

`vendor/libghostty-vt-sys` is a **path-patched copy of the upstream sys crate**, not a new one: its `Cargo.toml` still carries `repository = "https://github.com/uzaaft/libghostty-rs"` and `version = "0.1.1"`, and the root `Cargo.toml` adds `libghostty-vt-sys = { path = "vendor/libghostty-vt-sys" }` under `[patch]`. So the branch takes the safe wrapper from crates.io unmodified and vendors only the build-and-link layer.

The vendored `build.rs` shells out to `zig build -Demit-lib-vt` (adding `-Doptimize=ReleaseFast` for release profiles), clones Ghostty at the pinned commit into `OUT_DIR` unless `GHOSTTY_SOURCE_DIR` overrides it, and then links **dynamically**:

```rust
let lib_name = if target.contains("darwin") {
    "libghostty-vt.0.1.0.dylib"
} else {
    "libghostty-vt.so.0.1.0"
};
...
println!("cargo:rustc-link-lib=dylib=ghostty-vt");
```
— `vendor/libghostty-vt-sys/build.rs:69-87`

Dynamic linking is why the sixth commit exists at all: `script/bundle-mac` gains a `copy_libghostty_vt` function that locates the freshest `ghostty-install/lib/libghostty-vt.dylib` under `target/`, copies it into `Contents/Frameworks`, runs `install_name_tool -id "@rpath/libghostty-vt.dylib"`, adds `@executable_path/../Frameworks` to the binary's rpath, and codesigns the dylib separately with `--options runtime`.

**Windows is excluded by construction.** The target-gated dependency block keeps the crate off Windows, and the vendored build script's target mapper panics on anything else:

```rust
fn zig_target(target: &str) -> String {
    let value = match target {
        "x86_64-unknown-linux-gnu" => "x86_64-linux-gnu",
        "x86_64-unknown-linux-musl" => "x86_64-linux-musl",
        "aarch64-unknown-linux-gnu" => "aarch64-linux-gnu",
        "aarch64-unknown-linux-musl" => "aarch64-linux-musl",
        "aarch64-apple-darwin"      => "aarch64-macos-none",
        "x86_64-apple-darwin"       => "x86_64-macos-none",
        other => panic!("unsupported Rust target for vendored build: {other}"),
    };
    value.to_owned()
}
```
— `vendor/libghostty-vt-sys/build.rs:141-152`

On Windows the branch keeps alacritty's `tty::new` + `EventLoop` path untouched (`crates/terminal/src/terminal.rs:684-717`). **For a project whose map puts Windows first, this branch supplies no Windows answer at all.**

---

# Part 2 — Ownership under `!Send` (`tiborvass/zed`)

This is the central question, so it gets the most space.

## 2.1 The answer in one sentence

It does **not** avoid `!Send`. It asserts `Send` with an `unsafe impl` on a wrapper struct, keeps the terminal behind an `Arc<Mutex<…>>` exactly as alacritty was, and moves the *renderer-side* objects — which stay `!Send` — permanently onto the GPUI thread, where they are allocated once and reused forever.

```rust
pub(crate) type GhosttyTerminalHandle = Arc<Mutex<GhosttyTerminalState>>;

pub(crate) struct GhosttyTerminalState {
    // libghostty-vt stores callback userdata as a raw pointer into its Terminal.
    terminal: Box<libghostty_vt::Terminal<'static, 'static>>,
    effects: Arc<Mutex<Vec<GhosttyEffect>>>,
}

// libghostty's render-state docs describe terminal IO and rendering as safe to
// coordinate across threads when all terminal access is serialized by a lock.
// GhosttyTerminalState only installs callbacks that capture Send state.
unsafe impl Send for GhosttyTerminalState {}
```
— `crates/terminal/src/ghostty.rs:163-174`

The safety comment cites the render-state documentation. That documentation says, verbatim:

> "The key design principle of this API is that it only needs read/write access to the terminal instance during the update call. This allows the render state to minimally impact terminal IO performance and also allows the renderer to be safely multi-threaded (as long as a lock is held during the update call to ensure exclusive access to the terminal instance)."
> — `libghostty-vt` 0.1.1, `src/render.rs:15-25`

So the upstream contract is: `!Send` because nothing enforces the lock, but *serialized* access across threads is the sanctioned pattern. The `Box` is load-bearing and the comment says why — libghostty stores callback userdata as a raw pointer into the `Terminal`, so the address must not move after `on_pty_write`/`on_bell`/`on_title_changed` are installed (`ghostty.rs:216-246`).

## 2.2 Which thread owns what

Four threads per terminal. Three are spawned in `spawn_pty` (`ghostty.rs:522-661`), the fourth is GPUI's.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ GPUI main thread — owns Entity<Terminal>                                    │
│                                                                             │
│  Terminal {                                                                 │
│    term: Arc<FairMutex<Term<ZedListener>>>,   ← alacritty SHADOW, still here │
│    ghostty: Option<GhosttyTerminal> {                                       │
│        terminal:     Arc<Mutex<GhosttyTerminalState>>  ─────┐  (Send)       │
│        render_state: RenderState<'static>       !Send  ─┐   │               │
│        rows:         RowIterator<'static>       !Send   ├── never leaves     │
│        cells:        CellIterator<'static>      !Send  ─┘   │  this thread   │
│    },                                                       │               │
│    pending_alacritty_shadow_output: Arc<Mutex<Vec<u8>>>     │               │
│  }                                                          │               │
└──────────┬────────────────────────────────┬─────────────────┼───────────────┘
           │ PtyCommand                     │ TerminalEvent   │ lock() for
           │ (Input/Resize/Shutdown)        │ (effects)       │ write + render
           │ std::sync::mpsc                │ futures unbounded              │
           v                                ^                 v
┌──────────────────────┐   Vec<u8>  ┌───────┴───────────────────────────────┐
│ "Ghostty PTY writer" │            │ "Ghostty PTY parser"                  │
│  writer.write_all    │            │  coalesce ≤16 KiB, lock, vt_write,    │
│  master.resize       │            │  take_effects, send effects to UI     │
└──────────────────────┘            └───────^───────────────────────────────┘
                                            │ sync_channel(256), Vec<u8>
                                    ┌───────┴───────────────────────────────┐
                                    │ "Ghostty PTY reader"                  │
                                    │  reader.read into recycled 8 KiB bufs │
                                    └───────────────────────────────────────┘
```

The split is the whole trick: **the `Terminal` handle is `Send` and shared; the render-side objects are `!Send` and unshared.** `GhosttyTerminal` — the struct that owns `RenderState`, `RowIterator` and `CellIterator` — is never given an `unsafe impl Send`, so the compiler pins it to whichever thread constructed it, which is the GPUI thread that owns the `Entity<Terminal>`.

## 2.3 What crosses channels

Nothing VT-shaped. Four channel types, all carrying plain owned data:

| Direction | Channel | Payload |
|---|---|---|
| reader → parser | `sync_channel::<Vec<u8>>(256)` | raw PTY bytes, in recycled buffers |
| parser → reader | `sync_channel::<Vec<u8>>(64)` | drained buffers, returned for reuse |
| parser → UI | `futures::channel::mpsc::UnboundedSender<TerminalEvent>` | `GhosttyPtyOutput { effects: Vec<GhosttyEffect> }` and `GhosttyChildExit(Option<ExitStatus>)` |
| UI → writer | `std::sync::mpsc::Sender<PtyCommand>` | `Input(Cow<'static,[u8]>)`, `Resize(TerminalBounds)`, `Shutdown` |

The effect type is deliberately small and owned:

```rust
pub(crate) enum GhosttyEffect {
    PtyWrite(Vec<u8>),
    Bell,
    TitleChanged(String),
}
```
— `ghostty.rs:150-155`

These are produced by the three callbacks installed on the terminal, which push into an `Arc<Mutex<Vec<GhosttyEffect>>>` (`ghostty.rs:222-246`); the parser thread drains that vec while it already holds the terminal lock and ships the result to the UI (`ghostty.rs:704-712`). The design note in the commit message is explicit that this replaced a worse arrangement: "Change Ghostty PTY output events so the UI receives parsed terminal effects instead of raw byte buffers that it must acknowledge synchronously."

Buffer recycling is real: the reader pulls an 8 KiB buffer off the recycle channel or allocates one (`next_pty_read_buffer`, `ghostty.rs:678-684`), and both the parser and reader return buffers to it, dropping any whose capacity exceeded 16 KiB (`recycle_pty_buffer`, `ghostty.rs:686-695`).

## 2.4 What the render snapshot is, who allocates it, and how often

**The render snapshot is not an allocation.** `RenderState`, `RowIterator` and `CellIterator` are each created **once**, in `GhosttyTerminal::new`, and live for the terminal's lifetime:

```rust
Ok(Self {
    terminal: Arc::new(Mutex::new(GhosttyTerminalState { terminal, effects })),
    render_state: RenderState::new()?,
    rows: RowIterator::new()?,
    cells: CellIterator::new()?,
})
```
— `ghostty.rs:248-253`

Per frame, `RenderState::update` refreshes that one buffer in place and hands back a *borrow* of it. In the wrapper crate, `pub struct Snapshot<'alloc, 's>(&'s mut RenderState<'alloc>)` (`libghostty-vt` 0.1.1 `src/render.rs:195-196`) and `update` is a single FFI call, `ghostty_render_state_update` (`src/render.rs:281-287`). The copying happens inside Ghostty's Zig, into memory the render state already owns.

The critical section is exactly that call, plus two cheap queries:

```rust
let (scrollbar, mode, snapshot) = {
    let terminal = self.terminal.lock();
    (
        terminal.terminal.scrollbar()?,
        Self::term_mode(&terminal.terminal)?,
        self.render_state.update(&terminal.terminal)?,
    )
};
```
— `ghostty.rs:367-374` (the same block appears at `:281-288`)

The guard drops at the end of the block; the `snapshot` outlives it because it borrows `self.render_state`, not the terminal. Everything after that — row iteration, cell iteration, colour conversion, layout — runs **with the terminal unlocked**, so the PTY parser thread can keep ingesting while the frame is being built. The third commit's message names this as a deliberate step: "Shorten Ghostty terminal locking by taking the render snapshot and metadata under the terminal lock, then building layout from that snapshot outside the lock."

Downstream of the snapshot there is no grid copy either. `Terminal::visit_render_cells` streams cells straight into the layout builder:

```rust
pub trait TerminalRenderCell {
    fn point(&self) -> AlacPoint;
    fn character(&self) -> char;
    fn foreground(&self) -> alacritty_terminal::vte::ansi::Color;
    fn background(&self) -> alacritty_terminal::vte::ansi::Color;
    fn flags(&self) -> Flags;
    fn zero_width_chars(&self) -> Option<&[char]>;
    fn has_hyperlink(&self) -> bool;
}

pub trait TerminalRenderCellVisitor {
    fn visit(&mut self, cell: &dyn TerminalRenderCell);
}
```
— `crates/terminal/src/terminal.rs:917-929`

Both `IndexedCell` (alacritty) and `GhosttyRenderCell` implement the trait (`terminal.rs:961-987`, `ghostty.rs:115-148`), so the layout code is shared. `LayoutGridBuilder` in the view crate *is* the visitor (`crates/terminal_view/src/terminal_element.rs:688`), and `layout_streamed_grid` (`terminal_element.rs:851-885`) drives `terminal.visit_render_cells(rows_to_skip, visible_row_count, &mut builder)` — only the visible rows, chosen by the caller. The commit message states the intent: "The terminal view can ask Ghostty for only the visible rows needed for layout instead of first copying the full screen into Zed-owned Alacritty cells."

The per-cell conversion is a stack value with an inline array and an escape hatch:

```rust
struct GhosttyRenderCell {
    point: AlacPoint,
    character: char,
    foreground: Color,
    background: Color,
    flags: Flags,
    zero_width_chars: [char; MAX_INLINE_ZERO_WIDTH_CHARS],   // 8
    zero_width_chars_len: usize,
    zero_width_chars_overflow: Vec<char>,
}
```
— `ghostty.rs:46-55`, with `MAX_INLINE_ZERO_WIDTH_CHARS = 8` at `:27`

The `Vec` is only touched for grapheme clusters longer than nine codepoints (`ghostty.rs:82-94`). **So on the Ghostty path the per-frame heap traffic is the layout output — `Vec<LayoutRect>`, `Vec<SplitRect>`, `Vec<GraphicRect>`, `Vec<BatchedTextRun>` — and nothing else.** Compare Tiller today: `fn snapshot(&self) -> (Vec<Vec<Cell>>, (usize, usize))` at `rust/crates/tiller_terminal/src/lib.rs:696`, a full `Cell`-by-`Cell` grid clone per paint (established in issue #27).

## 2.5 The part that is easy to miss: alacritty is still running

The branch does **not** remove `alacritty_terminal`. It runs both emulators on the same byte stream. `Terminal` keeps `term: Arc<FairMutex<Term<ZedListener>>>` *and* `ghostty: Option<GhosttyTerminal>` *and* `pending_alacritty_shadow_output: Arc<ParkingMutex<Vec<u8>>>` (`terminal.rs:1056-1064`).

The parser thread appends every chunk to the shadow buffer before writing it to Ghostty:

```rust
fn write_pty_output_to_ghostty(...) -> bool {
    crate::append_alacritty_shadow_output(&mut pending_alacritty_shadow_output.lock(), output);
    let effects = {
        let mut terminal = terminal.lock();
        terminal.write(output);
        terminal.take_effects()
    };
    events_tx.unbounded_send(TerminalEvent::GhosttyPtyOutput { effects }).is_ok()
}
```
— `ghostty.rs:697-712`

`append_alacritty_shadow_output` filters the stream as it copies: it drops SGR sequences and cursor-visibility toggles, which are pure styling/animation traffic the shadow will never be asked about.

```rust
fn should_skip_alacritty_shadow_csi(final_byte: u8, parameters: &[u8]) -> bool {
    final_byte == b'm' || matches!((final_byte, parameters), (b'h' | b'l', b"?25"))
}
```
— `terminal.rs:3044-3046`

The shadow is parsed lazily, on the UI thread, only when something needs alacritty-backed state:

```rust
fn needs_alacritty_shadow_flush(&self) -> bool {
    !self.events.is_empty()
        || self.selection_phase == SelectionPhase::Selecting
        || self.last_content.selection.is_some()
        || self.last_content.selection_text.is_some()
        || self.selection_head.is_some()
        || !self.matches.is_empty()
        || self.last_hyperlink_search_position.is_some()
        || self.mouse_down_hyperlink.is_some()
        || self.pending_alacritty_shadow_output.lock().len()
            >= MAX_DEFERRED_ALACRITTY_SHADOW_BYTES     // 2 MiB, terminal.rs:1143
}
```
— `terminal.rs:2077-2088`

and the flush constructs a fresh `alacritty_terminal::vte::ansi::Processor` and advances the shadow `Term` (`terminal.rs:1325-1343`).

Ghostty therefore supplies **cells, cursor, modes and scroll position**; alacritty still supplies **selection, search matches, hyperlink detection, vi mode, and `get_content`**. `make_content_metadata` (`terminal.rs:2163-2189`) is the ghostty-path replacement for `make_content` and returns `cells: Vec::new()` with `cursor_char: ' '` — it reads everything *except* the grid out of the shadow.

This is worth naming plainly, because it is the opposite of what "replaced alacritty with Ghostty" suggests: **on this branch the two emulators coexist, and the migration is only half done.** For Tiller, whose destination is `alacritty_terminal` gone from `Cargo.toml`, this branch is a demonstration of the ownership model, not of the endpoint.

---

# Part 3 — What replaced alacritty's `EventLoop`

Alacritty's `EventLoop::new(term, listener, pty, drain_on_exit, hold)` + `event_loop.spawn()` is a single thread doing poll-driven read, parse-into-`Term`, and write, holding the `FairMutex<Term>` itself. On the Ghostty path it is replaced by **three hand-written threads** with an explicit batching stage between read and parse.

**Reader** (`ghostty.rs:599-620`) — blocking `reader.read(&mut buffer)` on a `portable_pty` cloned reader, into a recycled 8 KiB buffer; truncate to the byte count; `output_tx.send(buffer)`. Breaks on `Ok(0)`, ignores `ErrorKind::Interrupted`, logs and breaks on anything else.

**Parser** (`ghostty.rs:543-596`) — takes a batch off the queue, then greedily `try_recv`s more chunks and concatenates until it would exceed `MAX_PTY_PARSE_BATCH_BYTES = 16 * 1024`, stashing the chunk that would have overflowed for the next round. Then one lock / one `vt_write` / one `take_effects` for the whole batch. After the loop ends it receives the `Child` handle over its own channel, `child.wait()`s, and emits `GhosttyChildExit(status)` — so child reaping is on the parser thread, deliberately after the byte stream has drained.

**Writer** (`ghostty.rs:633-657`) — a plain `command_rx.recv()` loop over `PtyCommand`, doing `writer.write_all`, `master.resize(pty_size(bounds))`, or breaking on `Shutdown`.

Above the threads there is a **second** coalescing stage on the UI side. `Terminal::process_events` accumulates consecutive `GhosttyPtyOutput` events into one `Vec<GhosttyEffect>` and flushes it when a non-Ghostty event interrupts or the batch ends (`terminal.rs:1160-1186`), so a burst of PTY traffic produces one `Event::Wakeup` rather than one per read.

Two constants govern backpressure: `MAX_QUEUED_PTY_OUTPUT_BUFFERS = 256` (the reader→parser `sync_channel` bound) and `MAX_RECYCLED_PTY_BUFFERS = 64` (`ghostty.rs:28-31`). When the parser falls behind, the bounded channel blocks the reader, which stops draining the PTY, which applies flow control to the child — the same backpressure alacritty's loop gets for free.

---

# Part 4 — PTY

## 4.1 What each project uses

| Project | PTY |
|---|---|
| `tiborvass/zed` (macOS/Linux) | `portable-pty` (workspace dependency, added in `crates/terminal/Cargo.toml`) |
| `tiborvass/zed` (Windows) | unchanged: `alacritty_terminal::tty` + `EventLoop` |
| `Xuanwo/gpui-ghostty` | `portable-pty = "0.9"` — **only in the examples**, not in the terminal crate |
| `herdrdev/herdr` | `portable-pty` pinned to `=0.9.0`, patched to a vendored copy (established in issue #27) |

`libghostty-vt` supplies no PTY, so this is forced.

## 4.2 How the PTY thread hands bytes to the non-`Send` terminal

**`tiborvass/zed`** — the PTY reader thread never touches the terminal. It hands `Vec<u8>` to a parser thread over a bounded channel; that parser thread holds a `Send`-asserted `Arc<Mutex<GhosttyTerminalState>>` and does the `vt_write` itself. So VT parsing runs off the UI thread, on a thread that is not the reader, and the terminal is reached only through the mutex.

`spawn_pty`'s ordering matters and is not accidental. It opens the pty and clones reader/writer, spawns the parser and reader threads first, *then* spawns the child, then sends the `Child` over `child_tx` to the parser thread, and only then drops `pair.slave` (`ghostty.rs:529-631`). `PtyProcessInfo` is built from the master fd plus the child pid via a new constructor `ProcessIdGetter::new_from_fd(handle, fallback_pid)` added to `pty_info.rs` — the pre-existing `tcgetpgrp`-based foreground-process logic is retained, just fed from a `portable-pty` fd instead of an alacritty `Pty`.

**`Xuanwo/gpui-ghostty`** — the opposite. The PTY reader thread sends `Vec<u8>` over `std::sync::mpsc` to a **GPUI task**, and the VT feed happens on the UI thread. See Part 5.

---

# Part 5 — `Xuanwo/gpui-ghostty`

## 5.1 What it is, and where it already diverges

Head `e3025981c6211dd7db2a825dc364ffb5d342f45e`, 2026-01-08, 81 commits, Apache-2.0. It is **not stale by accident but by completion**: `ROADMAP.md` ends with "## Future Work — None." It is also, by its own `AGENTS.md`, an agent-driven project ("Do not ask the user any questions. Keep going until `ROADMAP.md` is fully completed"; discussion in Simplified Chinese, code in English).

The first and largest divergence from `tiborvass/zed`: **it does not use `libghostty-vt` at all.** It vendors Ghostty as a git submodule pinned to tag `v1.2.3` (submodule commit `6d2dd585a5d87fa745d48188dd096ca6e63014d0`) and writes its **own 870-line Zig shim** against Ghostty's *internal* modules —

```zig
const ghostty_input = @import("ghostty_src/input.zig");
const terminal = @import("ghostty_src/terminal/main.zig");

const TerminalHandle = struct {
    alloc: Allocator,
    terminal: terminal.Terminal,
    stream: terminal.Stream(*Handler),
    handler: Handler,
    ...
```
— `crates/ghostty_vt_sys/zig/lib.zig:1-15`

— exposing a hand-written 86-line C header (`crates/ghostty_vt_sys/include/ghostty_vt.h`) with its own 7-bit style flags bitset (`0x01 inverse, 0x02 bold, 0x04 italic, 0x08 underline, 0x10 faint, 0x20 invisible, 0x40 strikethrough`). Zig is pinned to **0.14.1** (`README.md`, and `.github/workflows/ci.yml` uses `goto-bus-stop/setup-zig@v2` with `version: 0.14.1`). GPUI is a git dependency on Zed pinned by `Cargo.lock`.

This makes it a *less* transferable reference than its name suggests: none of its FFI surface exists in `libghostty-vt` 0.2.x, and it is pinned to a Ghostty release two minor versions behind the one `tiborvass/zed` and herdr build against.

**CI covers Linux (VT crates only) and macOS (full). There is no Windows job.**

## 5.2 Ownership: single-threaded, no `unsafe impl Send` anywhere

```rust
pub struct Terminal {
    ptr: NonNull<c_void>,
}
```
— `crates/ghostty_vt/src/lib.rs:26-28`

That is the whole handle, and there is **no `unsafe impl Send` or `unsafe impl Sync` in the crate**. `TerminalSession` owns it by value (`crates/gpui_ghostty_terminal/src/session.rs:5-18`), `TerminalView` owns the session by value (`crates/gpui_ghostty_terminal/src/view/mod.rs:210-228`), and the view is a GPUI `Entity`. There is no mutex on the terminal anywhere in the codebase.

```
┌──────────────────────────────────────────────────────────────────────┐
│ GPUI main thread                                                      │
│                                                                       │
│  Entity<TerminalView> {                                               │
│      session: TerminalSession { terminal: ghostty_vt::Terminal }      │  !Send, owned by value
│      pending_output:      Vec<u8>          (≤ 256 KiB)                │
│      viewport_lines:      Vec<String>      ── the render snapshot     │
│      viewport_style_runs: Vec<Vec<StyleRun>>                          │
│      line_layouts:        Vec<Option<gpui::ShapedLine>>  ── shaping cache
│  }                                                                    │
│                                                                       │
│  window task: every 16 ms → drain stdout_rx → queue_output_bytes()    │
│  Render::render → feed pending_output into the VT → dirty rows → paint│
└───────▲──────────────────────────────────────────────┬────────────────┘
        │ mpsc::Receiver<Vec<u8>>                       │ TerminalInput callback
        │                                               │ mpsc::Sender<Vec<u8>>
┌───────┴───────────────┐                    ┌──────────▼────────────────┐
│ PTY reader thread     │                    │ PTY writer thread         │
│ read 8 KiB → send     │                    │ recv → write_all + flush  │
└───────────────────────┘                    └───────────────────────────┘
```

The 16 ms poll loop is literal, in both PTY examples:

```rust
window.spawn(cx, async move |cx| {
    loop {
        cx.background_executor().timer(Duration::from_millis(16)).await;
        let mut batch = Vec::new();
        while let Ok(chunk) = stdout_rx.try_recv() {
            batch.extend_from_slice(&chunk);
        }
        if batch.is_empty() { continue; }
        cx.update(|_, cx| {
            view_for_task.update(cx, |this, cx| { this.queue_output_bytes(&batch, cx); });
        }).ok();
    }
}).detach();
```
— `examples/pty_terminal/src/main.rs:152-175`

`queue_output_bytes` appends to `pending_output` and calls `cx.notify()`; it only feeds the VT eagerly when the buffer would exceed `MAX_PENDING_OUTPUT_BYTES = 256 * 1024`, chunking oversized writes (`view/mod.rs:682-713`). The actual feed happens at the top of `Render::render` (`view/mod.rs:2038-2053`). **VT parsing is on the UI thread, inside the render pass.**

## 5.3 What crosses channels

Only `Vec<u8>`, in both directions. `TerminalInput::new(move |bytes| { let _ = stdin_tx.send(bytes.to_vec()); })` is the only egress (`view/mod.rs:194-208`, `examples/pty_terminal/src/main.rs:96-98`). Nothing terminal-shaped ever leaves the UI thread.

## 5.4 What the render snapshot is, who allocates it, how often

A per-row cache on the view, updated **only for dirty rows**:

```rust
pub struct TerminalView {
    session: TerminalSession,
    viewport_lines: Vec<String>,
    viewport_line_offsets: Vec<usize>,
    viewport_total_len: usize,
    viewport_style_runs: Vec<Vec<StyleRun>>,
    line_layouts: Vec<Option<gpui::ShapedLine>>,
    line_layout_key: Option<(Pixels, Pixels)>,
    ...
}
```
— `view/mod.rs:210-228`

```rust
fn apply_dirty_viewport_rows(&mut self, dirty_rows: &[u16]) -> bool {
    ...
    for &row in dirty_rows {
        let line = match self.session.dump_viewport_row(row as u16) { ... };
        self.viewport_lines[row].clear();
        self.viewport_lines[row].push_str(line);
        self.viewport_style_runs[row] = self.session
            .dump_viewport_row_style_runs(row as u16).unwrap_or_default();
        if row < self.line_layouts.len() {
            self.line_layouts[row] = None;      // invalidate the cached ShapedLine
        }
    }
    ...
}
```
— `view/mod.rs:617-662`

The dirty set comes from the shim (`take_dirty_viewport_rows`, `crates/ghostty_vt/src/lib.rs:247`), and the row `String`s are reused (`clear()` + `push_str`, not reallocated). The style unit is a **run**, not a cell — `StyleRun { start_col, end_col, fg, bg, flags }` (`ghostty_vt/src/lib.rs:45-51`) — which the roadmap records as a deliberate startup-cost optimisation ("M5.14: Startup Perf (style-run dump, avoid per-cell styles)"). The single largest structural difference from Tiller: shaping results are cached per row and only invalidated for rows the terminal marked dirty.

## 5.5 `pty_terminal` and `split_pty_terminal` — directly relevant to Tiller

`examples/split_pty_terminal/src/main.rs` is the split-pane case, and it is instructive precisely because it is *boring*:

- `spawn_shell_pane(cx) -> Pane { view: Entity<TerminalView>, master: Arc<dyn MasterPty + Send>, stdout_rx: mpsc::Receiver<Vec<u8>> }` (struct at `:15-19`, built by `spawn_shell_pane` at `:21-92`). **Each pane is a fully independent stack**: its own pty pair, its own reader thread, its own writer thread, its own `TerminalSession`, its own `Entity<TerminalView>`. Nothing is shared between panes.
- The container is an ordinary GPUI element: `div().flex().flex_row().child(left).child(divider).child(right)` (`:123-135`).
- Resize is one `observe_window_bounds` handler that computes cell metrics from the shaped width of `"M"`, divides the pane width, then calls `master.resize(PtySize { rows, cols, .. })` and `view.resize_terminal(cols, rows, cx)` for each pane (`:166-201`).
- **One** shared 16 ms task drains both receivers and feeds each view in turn (`:203-233`).

So `n` panes cost `2n` OS threads plus one GPUI task; scaling is per-pane and the `!Send` terminal never becomes a coordination problem because it never leaves the thread that owns the view. That is exactly the shape Tiller's per-worktree pane tree would land on if it chose this model. What the example does *not* demonstrate is many panes at once, or what happens when one pane floods — the single 16 ms task drains left then right sequentially with no fairness budget.

## 5.6 Measured performance

This is the only *recorded measurement* found in either project. `docs/perf_instruments.md` documents a reproducible Instruments run:

```sh
xcrun xctrace record --template 'Time Profiler' --time-limit 10s --no-prompt \
  --output traces/pty_terminal_timeprof_yes.trace \
  --env GPUI_GHOSTTY_PTY_DEMO_COMMAND='yes 0123456789abcdef…' \
  --launch -- ./target/release/pty_terminal
```

and reports the steady-state top *leaf* samples:

> - `_lib.Handler.print` (Zig terminal print/cell update hot path)
> - `ghostty_vt_terminal_feed` / `ghostty_vt::Terminal::feed` (FFI + VT feed loop)
> - `gpui_ghostty_terminal::session::TerminalSession::feed_with_pty_responses`
> - `gpui_ghostty_terminal::session::OscQueryScanState::advance` (OSC query scanning overhead)
>
> "The main implication is that, under high-throughput output, the bottleneck is currently on the *ingest/feed* path (VT parsing + terminal state mutation) rather than on rendering/shaping."

Its own stated next step is the architecture `tiborvass/zed` already has:

> "3. Avoid feeding unlimited output on the UI thread: apply a per-frame budget and/or move feed onto a worker thread, then publish coalesced state/damage to the UI thread."

**Note what this is and is not.** It is a real trace with named hot symbols. It contains **no numbers** — no frame times, no throughput, no before/after. It does not compare against alacritty or against anything else. Treat it as a direction, not a benchmark.

## 5.7 Its own compensation for a thin C API

Because the pinned v1.2.3 shim exposes no effect callbacks, `TerminalSession` **re-scans the byte stream in Rust** for things the VT layer will not tell it: it keeps a rolling 2 KiB `parse_tail` and hand-parses `CSI ? … h/l` for modes 2004/1000/1002/1003/1006, and `OSC 0/2` (title) and `OSC 52` (clipboard) (`session.rs:92-239`); it runs two byte-at-a-time state machines, `DsrScanState` and `OscQueryScanState`, to answer `CSI 5n`/`CSI 6n` and `OSC 10/11` queries by splitting the feed at the query byte and injecting a reply (`session.rs:246-296`, `:378-482`). One of those scanners is already visible in its own profile as overhead.

`tiborvass/zed` has none of this: `libghostty-vt` 0.1.1's effect callbacks (`on_pty_write`, `on_bell`, `on_title_changed`) and `Terminal::mode()` give it modes, titles, bells and DA/DSR responses directly, and a unit test in the branch asserts the XTVERSION reply comes back through the effect channel:

```rust
terminal.write(b"\x1b[>0q");
let effects = terminal.take_effects();
assert!(effects.iter().any(|effect| matches!(effect,
    GhosttyEffect::PtyWrite(bytes) if bytes == b"\x1bP>|libghostty\x1b\\")));
```
— `crates/terminal/src/ghostty.rs:838-852`

**This is the strongest argument in this document for using the real `libghostty-vt` C API rather than a hand-rolled shim.**

---

# Part 6 — Trade-offs, and what could not be established

## 6.1 What `tiborvass/zed` gave up

Each of these is read off the code, not inferred:

1. **Windows.** Not supported on the Ghostty path at all (Part 1.3). Alacritty remains the Windows backend.
2. **A single emulator.** Two emulators run on the same stream (Part 2.5). Memory cost: a second grid and scrollback per terminal. CPU cost: a filtered re-parse, deferred but not eliminated, with a 2 MiB forced-flush ceiling.
3. **OSC 8 hyperlinks from Ghostty.** `impl TerminalRenderCell for GhosttyRenderCell { fn has_hyperlink(&self) -> bool { false } }` (`ghostty.rs:145-147`). Hyperlinks come from the alacritty shadow's regex searches, unchanged.
4. **Dirty tracking.** `libghostty-vt` exposes `Snapshot::dirty()` (`Clean`/`Partial`/`Full`) and per-row dirty flags; `grep -rn 'dirty\|Dirty'` across `ghostty.rs`, `terminal.rs` and `terminal_element.rs` returns only alacritty's unrelated `MouseCursorDirty`. Every paint re-visits every visible row. `gpui-ghostty` *does* use dirty rows; this branch leaves that optimisation on the table.
5. **Ghostty's key encoder.** `libghostty-vt` 0.1.1 ships `key::Encoder` (`src/key.rs`, 651 lines); nothing in `crates/terminal/src` references it. Keys still go through Zed's alacritty-era mappings, so the kitty keyboard protocol is not gained.
6. **Kitty graphics.** No reference anywhere in the diff.
7. **Theme-driven ANSI palette — probable.** `CellIteration::fg_color()` "Resolves palette indices through the palette… Returns `None` if the cell has no explicit foreground color" (`libghostty-vt` 0.1.1 `src/render.rs:639-647`), and `rgb_to_alacritty_color` maps `Some(rgb)` to `Color::Spec` and `None` to `Color::Named(Foreground|Background)` (`ghostty.rs:752-760`). The branch never calls `snapshot.colors()` and never writes a palette into the terminal. So `AnsiColor::Indexed` can no longer reach the layout builder on the Ghostty path, and the `indexed_color_cache`/`get_color_at_index` theme lookup added in the same commit is dead for Ghostty cells. **I did not run the branch to confirm the visual result**; what is established is the absence of any palette wiring in the code.
8. **Cost per frame under the lock.** `Self::term_mode` issues **eighteen** separate `ghostty_terminal_mode` FFI calls to rebuild alacritty's `TermMode` bitflags, and it runs inside the critical section alongside `scrollbar()` and `update()` (`ghostty.rs:367-374`, `:416-507`). A cheaper mode representation would shorten the lock hold.
9. **Dynamic linking and bundling.** The dylib must be found, copied, `install_name_tool`-ed and separately codesigned (Part 1.3). A static build would avoid all of it; the vendored build script does not offer one.

## 6.2 What `Xuanwo/gpui-ghostty` gave up

1. **VT parsing on the UI thread**, inside `Render::render`. Its own profile says that is where the time goes under load, and its own roadmap says the fix is to move it off.
2. **Up to 16 ms of added latency** on every byte, by construction of the poll loop.
3. **Fidelity.** 7 style flags in a `u8`; no underline styles or colours, no wide-char/spacer classification in the style path, no zero-width grapheme model in the cell dump — the render surface is `String` per row plus `StyleRun`s.
4. **The upstream C API.** A hand-rolled Zig shim over Ghostty *internals* pinned to v1.2.3, which will not track `libghostty-vt` releases.
5. **Windows.** No CI job; no Windows-specific code.
6. **Backpressure on the PTY.** Its reader thread uses an unbounded `mpsc::channel`; the bound is applied later, on the UI side, by chunking a 256 KiB `pending_output` buffer. A flooding child is not throttled at the pipe.

## 6.3 Performance numbers actually measured

**Only one, and it has no figures.** `gpui-ghostty`'s Instruments trace (Part 5.6) names hot symbols and gives an ordered list of next targets; it reports no timings.

`tiborvass/zed` records **no measurements at all**. Its four optimisation commits assert improvements in prose — "reduces layout work for terminal workloads dominated by colored background cells, such as ANSI animation and benchmark output", "reducing foreground-thread churn during high-throughput output" — with no numbers, no benchmark, and no CI run. The only instrumentation in the branch is a `log::debug!` in `LayoutGridBuilder::finish` that prints cells processed, batched runs, rect counts and `self.start_time.elapsed()` per layout (`crates/terminal_view/src/terminal_element.rs:590-601`) — a live diagnostic, not a recorded result.

**I could not find a primary source for the "blazingly fast" description** attributed to the author in issue #27. A GitHub-wide issue/PR search for that phrase together with `author:tiborvass` returns zero results, and neither the branch's commit messages, its README, nor any Zed issue or discussion contains it. If it exists it is somewhere I did not reach (a social post, a chat, a talk). **Treat the claim as unsourced.**

## 6.4 Other things I could not establish

- **Whether either branch builds today.** Not attempted: Windows host, both Ghostty paths `cfg`-gated off Windows, and both need a Zig toolchain. `tiborvass/zed` has zero CI runs ever; `gpui-ghostty` has CI but its last green run predates any dependency drift since 2026-01-08.
- **Whether `unsafe impl Send for GhosttyTerminalState` is actually sound.** The upstream doc sanctions cross-thread use "as long as a lock is held during the update call". The branch does hold the lock for `vt_write`, `resize`, `scroll_viewport`, `clear`, `take_effects` and `render_state.update`. What I did *not* verify is whether any libghostty-vt object holds thread-affine state below the C ABI (thread-local allocators, TLS in the Zig runtime, etc.). No project reads as having audited this; all three assert it.
- **How `libghostty-vt` 0.2.x differs from 0.1.1.** The branch pins `^0.1.1`; 0.2.0 and 0.2.1 are published. I did not diff the API, so I cannot say what a Tiller port targeting current would inherit or lose.
- **Whether `gpui-ghostty` is maintained.** Last commit 2026-01-08, `Future Work: None`. Whether that means finished or abandoned is not determinable from the repository.

---

# Part 7 — `herdrdev/herdr`, briefly, for contrast

herdr is a TUI, not a GPU renderer, and issue #27 already establishes its vendoring and build story; this is only the ownership shape.

It writes its **own** bindings (`src/ghostty/mod.rs`, 4,082 lines) rather than using `libghostty-vt`, and it asserts `Send` on **five** types, each with the same one-line justification:

```rust
// SAFETY: these opaque handles are only used behind external synchronization in pane runtime.
unsafe impl Send for Terminal {}      // :1881
unsafe impl Send for RenderState {}   // :2551
unsafe impl Send for KeyEncoder {}    // :2625
unsafe impl Send for RowIterator {}   // :2774
unsafe impl Send for RowCells {}      // :2916
```

The "external synchronization" is one lock covering the terminal **and** the render state together:

```rust
pub(crate) struct GhosttyPaneTerminal {
    pub core: Mutex<GhosttyPaneCore>,
    key_encoder: Mutex<crate::ghostty::KeyEncoder>,
    pending_pty_responses: Arc<Mutex<Vec<Bytes>>>,
}

pub(crate) struct GhosttyPaneCore {
    pub terminal: crate::ghostty::Terminal,
    #[cfg(windows)]
    recent_fallback: windows_recent_fallback::Cache,
    pub render_state: crate::ghostty::RenderState,
    pub kitty_keyboard: KittyKeyboardTracker,
    ...
}
```
— `src/pane/terminal.rs:165-190`

That is the meaningful contrast. `tiborvass/zed` keeps `RenderState` **outside** the lock, on the render thread, so the frame is built with the terminal unlocked. herdr puts `RenderState` **inside** the lock with the terminal, so the read is serialized with ingestion — acceptable when the "renderer" is emitting ANSI to a client at TUI cadence, more costly at 120 Hz paint. herdr also carries `#[cfg(windows)]` members in that same struct (`recent_fallback`, and `ghostty_tracked_grid_ref_free` in `Terminal::drop`), which is the only one of the three projects with Windows in the terminal core.

---

# Part 8 — The three models side by side

| | `tiborvass/zed` | `Xuanwo/gpui-ghostty` | `herdrdev/herdr` |
|---|---|---|---|
| VT binding | `libghostty-vt` 0.1.1 (crates.io) + vendored sys | own 870-line Zig shim over Ghostty internals | own 4,082-line bindings |
| Ghostty pin | commit `bebca846` (1.3.2-dev) | tag `v1.2.3` (submodule) | 1.3.2, source `c5a21edfc` (per #27) |
| Zig floor | 0.15.2 | 0.14.1 | 0.15.2 (per #27) |
| `!Send` handling | `unsafe impl Send` on a wrapper; `Arc<Mutex<…>>` | none — single-threaded ownership | `unsafe impl Send` on 5 types; one `Mutex` |
| Terminal lives on | shared, reached via mutex | the GPUI thread, by value | shared, reached via mutex |
| VT parsing runs on | dedicated parser thread | the GPUI thread, inside `render()` | pane runtime |
| `RenderState` lives | outside the lock, UI thread, allocated once | n/a (row `String` + `StyleRun` cache) | inside the lock with the terminal |
| Render snapshot | in-place `RenderState`, streamed via visitor, **no per-frame alloc** | dirty-row `String`/`StyleRun` cache + `ShapedLine` cache | in-place `RenderState` |
| Dirty tracking used | no | yes | yes |
| PTY | `portable-pty` | `portable-pty` (examples only) | `portable-pty` `=0.9.0`, patched |
| Threads per terminal | 3 (+GPUI) | 2 (+ one shared GPUI task) | not surveyed |
| Windows | no | no | yes |
| alacritty still present | **yes**, as a shadow | no | no |
| Recorded measurements | none | one Instruments trace, no figures | not surveyed |

## The shape a Tiller session could copy

Taking `tiborvass/zed`'s model and removing the shadow, one Tiller pane becomes:

```
GPUI thread (owns TerminalView / pane entity)
    RenderState        allocated once per pane, !Send, never moves          ┐
    RowIterator        allocated once per pane, !Send, never moves          ├ render side
    CellIterator       allocated once per pane, !Send, never moves          ┘
    Arc<Mutex<PaneVt>> ── where PaneVt { terminal: Box<Terminal>, effects } ┐
                          and `unsafe impl Send for PaneVt`                 │ shared
                                                                            ┘
  per paint:  lock → scrollbar + modes + render_state.update() → UNLOCK
              → iterate rows/cells → stream into layout → done
  per input:  lock → vt_write / resize / scroll → drain effects → unlock

reader thread   read() into recycled 8 KiB bufs → sync_channel(256)
parser thread   coalesce ≤16 KiB → lock → vt_write → take_effects → unlock
                → send Vec<Effect> to the pane entity
writer thread   recv PtyCommand → write_all / master.resize / shutdown
```

Three facts make this a smaller change than it looks for Tiller. The seam is already private and confined to `tiller_terminal/src/lib.rs` (issue #27). `snapshot() -> (Vec<Vec<Cell>>, (usize, usize))` at `lib.rs:696` — the full grid clone per paint — is replaced by an iterator-driven visitor with no grid allocation at all, which is a *reduction* in per-frame work, not a cost. And `Arc<FairMutex<Term>>` becomes `Arc<Mutex<PaneVt>>` with an `unsafe impl Send` — the same ownership topology, not a new one.

Two facts make it larger. There is no Windows precedent in either GPUI reference; only herdr has one, and herdr is a TUI. And the parser-thread arrangement is the piece with no test coverage in either reference — `tiborvass/zed`'s three unit tests in `ghostty.rs` all drive `GhosttyTerminal` synchronously and never touch `spawn_pty`.

---

## Source index

**Zed / `tiborvass/zed`**
- Fork and branch metadata — `gh api repos/tiborvass/zed`, `…/branches`, `…/actions/runs?branch=libghostty`
- Branch `libghostty` @ `62911d02cca8e9a960519ec4c17b715b3eff52a4` — <https://github.com/tiborvass/zed/tree/libghostty>
- Base commit `3bd9d13b63fc5a5ffa39326597bc4fd91adc82d1` — <https://github.com/zed-industries/zed/commit/3bd9d13b63fc5a5ffa39326597bc4fd91adc82d1>
- Divergence from upstream — `gh api repos/zed-industries/zed/compare/3bd9d13b63…main`
- "Plans to use Ghostty as a terminal emulator in Zed?" — <https://github.com/zed-industries/zed/discussions/18129>, maintainer comment <https://github.com/zed-industries/zed/discussions/18129#discussioncomment-10706723>
- Files read: `crates/terminal/src/ghostty.rs`, `crates/terminal/src/terminal.rs`, `crates/terminal/src/pty_info.rs`, `crates/terminal/Cargo.toml`, `crates/terminal_view/src/terminal_element.rs`, `crates/gpui/src/{color,scene,window}.rs`, `vendor/libghostty-vt-sys/{Cargo.toml,README.md,build.rs}`, `script/bundle-mac`, `docs/src/development/macos.md`, `Cargo.toml`, `Cargo.lock`

**`libghostty-vt`**
- crates.io versions — <https://crates.io/api/v1/crates/libghostty-vt> (0.1.0, 0.1.1, 0.2.0, 0.2.1; max 0.2.1, 2026-07-18)
- Source read: `libghostty-vt-0.1.1.crate` from <https://static.crates.io/crates/libghostty-vt/libghostty-vt-0.1.1.crate> — `src/lib.rs`, `src/render.rs`, `src/alloc.rs`, `src/terminal.rs`
- Upstream repository named in the vendored crate — <https://github.com/uzaaft/libghostty-rs>

**Ghostty**
- Pinned commit `bebca84668947bfc92b9a30ed58712e1c34eee1d`, 2026-03-24 — <https://github.com/ghostty-org/ghostty/commit/bebca84668947bfc92b9a30ed58712e1c34eee1d>
- `build.zig.zon` at that commit (`.version = "1.3.2-dev"`, `.minimum_zig_version = "0.15.2"`) — `gh api repos/ghostty-org/ghostty/contents/build.zig.zon?ref=bebca846…`

**`Xuanwo/gpui-ghostty`**
- Repository @ `e3025981c6211dd7db2a825dc364ffb5d342f45e` — <https://github.com/Xuanwo/gpui-ghostty>
- Files read: `README.md`, `ROADMAP.md`, `AGENTS.md`, `.gitmodules`, `.github/workflows/ci.yml`, `docs/perf_instruments.md`, `crates/ghostty_vt/src/lib.rs`, `crates/ghostty_vt_sys/{zig/lib.zig,include/ghostty_vt.h,build.rs}`, `crates/gpui_ghostty_terminal/{Cargo.toml,src/session.rs,src/view/mod.rs}`, `examples/pty_terminal/src/main.rs`, `examples/split_pty_terminal/src/main.rs`

**`herdrdev/herdr`**
- `src/ghostty/mod.rs`, `src/pane/terminal.rs` — fetched via `gh api repos/herdrdev/herdr/contents/…` on 2026-08-24 (branch `master`)

**Tiller**
- `rust/crates/tiller_terminal/src/lib.rs:696` — `fn snapshot(&self) -> (Vec<Vec<Cell>>, (usize, usize))` (confirmed via tokensave)
- Issue #27, the map — <https://github.com/tillerai/tiller/issues/27>
- Issue #29, this ticket — <https://github.com/tillerai/tiller/issues/29>
