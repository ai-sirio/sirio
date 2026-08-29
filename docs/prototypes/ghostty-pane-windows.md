# libghostty-vt on Windows — what the `prototype/ghostty-pane` spike found

Issue #33, part of the alacritty → libghostty-vt map (#27).
Prototype: `rust/crates/sirio_terminal/examples/prototype_ghostty_pane.rs` (throwaway).

## The question

Does `libghostty-vt` (VT parsing and state) plus `portable-pty` (the PTY) put a
working terminal pane on screen **on Windows**, inside this repo's GPUI setup —
and does `libghostty-vt` being `!Send`/`!Sync` force the pane out of the shape a
GPUI entity wants?

## The answer

Yes, with one condition that is invisible on Linux and fatal on Windows: **the
embedder must route the terminal's replies back to the pty.**

`libghostty-vt` is a VT *state* library, not a terminal. It never writes to the
pty itself; it hands the embedder the bytes it owes the host through the
`on_pty_write` effect callback. Installing that callback is optional-looking.
It is not.

## How it presented

The prototype opened its window and rendered nothing. A headless test made the
failure legible:

```
4 byte(s) from the pty; 24 row(s), 0 non-blank: []
```

Four bytes, then silence for fifteen seconds. A probe that drove `portable-pty`
alone — no libghostty-vt anywhere — reproduced it exactly, for every process
tried:

```
["cmd.exe", "/c", "echo", "..."]        4 bytes; child still running
["powershell.exe", "-NoProfile", ...]   4 bytes; child still running
["powershell.exe", "-NoProfile"]        4 bytes; child still running
hex: 1b 5b 36 6e        text: "\u{1b}[6n"
```

`ESC [ 6 n` is DSR-CPR: a cursor position request. **ConPTY emits it first and
blocks until the terminal answers.** Nothing else is ever written — not even by
`cmd.exe /c echo`, which otherwise writes and exits immediately.

Because the prototype never installed `on_pty_write`, the reply was generated
and dropped, ConPTY never unblocked, and no shell output existed to render.

## Why the current terminal does not have this problem

`sirio_terminal` runs on `alacritty_terminal`, which answers DSR internally and
writes the response to the pty itself. The obligation is discharged inside the
library, so no embedder ever had to know about it. Moving to `libghostty-vt`
moves that obligation outward, to us.

This is the same shape as the rest of the Windows port: a guarantee that lived
in the environment rather than in the code, which disappears without leaving
anything behind in a diff. On unix, forgetting the reply is a latent defect
nobody trips over. On Windows it stops the terminal before its first byte.

## On `!Send` / `!Sync` — the original worry

Not a problem, and the prototype shows why. The split that works:

- The `Terminal` and the render objects stay on one thread, owned directly by
  the GPUI entity. No `Arc`, no `Mutex`.
- The reader thread only ever sends `Vec<u8>`, which is plainly `Send`.
- Replies travel back the same way: an owned `Sender<Vec<u8>>` moved into the
  `on_pty_write` closure, drained on the owning thread after parsing.

`RenderState::begin_update(&terminal)?.end()?` is also a useful seam: only that
first call needs the terminal, and everything after reads render-state memory.
A real implementation can hold a lock for that call alone.

## The OSC title is tracked internally — no handler needed

Worth settling, because agent activity detection Layer B reads the OSC terminal
title and a regression there would be silent.

`Terminal::title()` reads `Data::TITLE` from libghostty's own state:

```rust
pub fn title(&self) -> Result<&str> {
    let str = self.get::<ffi::String>(Data::TITLE)?;
```

So the title is tracked by the library, and `on_title_changed` is a
*notification* hook rather than the mechanism that records it. A test feeding
`ESC ] 2 ; <title> BEL` through `vt_write` — the same entry point pty bytes take
— finds the title readable afterwards with no handler wired
(`osc_title_reaches_the_terminal_state`).

This contradicts an initial reading that the embedder must track the title
itself. It does not: Layer B keeps working through a `libghostty-vt` swap,
provided the pty bytes reach `vt_write` at all — which is what the DSR reply
above is for.

## Windows argument quoting — measured, and it breaks

`sirio_terminal::command_shell_invocation` (`src/lib.rs:432`) builds a Windows
shell invocation by pre-wrapping the command in quotes itself:

```rust
(program, vec!["/C".to_string(), format!("\"{command}\"")])
```

`portable_pty::CommandBuilder::cmdline()` then runs `append_quoted` over every
argument. That takes its fast path only when the argument contains no space,
tab, newline, vertical tab **or quote**. Sirio's argument has both spaces and
quotes, so it takes the slow path and the embedded quotes come back escaped.

Measured, not deduced (`windows_shell_invocation_survives_command_builder`):

| argv | result |
|---|---|
| `["cmd.exe", "/C", "\"echo SENTINEL\""]` — Sirio's form | **does not execute** |
| `["cmd.exe", "/C", "echo SENTINEL"]` — plain | executes |

The failing capture carries the explanation in its own bytes: `\"echo …`.
`CommandBuilder` escaped the pre-wrapped quotes, so `cmd.exe` saw a quoted
program name rather than a command, and never ran `echo`.

alacritty's Windows `tty` builds the command line raw and does not re-quote,
which is why the current code works. The swap removes that property, so
`command_shell_invocation` has to change alongside it — pass the command
unquoted and let `CommandBuilder` do the quoting, rather than quoting twice.

One trap worth recording, because it produces a convincing false pass: on the
failing form `cmd.exe` **echoes the mangled command back** inside its
"not recognized" error, sentinel included. A plain `contains(SENTINEL)` check
reports success. The test therefore requires the sentinel as its own output
line (`SENTINEL
`), and separately asserts the literal `\"echo` bytes as
locale-independent evidence of the mangling.

## What this does not answer

The prototype renders flex rows of divs, not the `ShapedLine` path `lib.rs`
builds through a custom `Element`. It proves cells and their styles arrive, not
that they can be drawn the production way at production speed. Selection,
scrollback UI, mouse, IME, link routing and the split tree are all untouched.

## Build note

Two environment requirements, neither discoverable from the source tree:

**MSVC ABI.** `libghostty-vt-sys` guards its static-library name on
`windows && msvc`, so the GNU ABI asks for the DLL import library instead of the
static archive and rustc reports `could not find native static library
'ghostty-vt'`. This machine now carries a directory override —
`stable-x86_64-pc-windows-msvc` for the repo root — which settles it (#36,
closed). Under MSVC the build produces and links `ghostty-vt-static.lib` on its
own; no artifact has to be placed by hand.

**Zig 0.15.2.** The `-sys` crate shells out to `zig build`. Version 0.16.0 is
incompatible and sits in the same parent directory, so picking the newest fails
in a more confusing way than the missing binary does. Neither is on `PATH` by
default:

```bash
export PATH="/d/toolchains/zig/zig-x86_64-windows-0.15.2:$PATH"
```

Without it every build touching the crate dies with
`failed to execute zig build: program not found`.

## Final state of the spike

`cargo test -p sirio_terminal --example prototype_ghostty_pane -- --test-threads=1`

```
running 6 tests
test tests::a_keystroke_round_trips_into_cells ... ok
test tests::osc_title_reaches_the_terminal_state ... ok
test tests::pty_alone_delivers_bytes ... ignored (diagnostic probe)
test tests::resize_reflows_without_error ... ok
test tests::shell_output_reaches_the_cells ... ok
test tests::windows_shell_invocation_survives_command_builder ... ok

test result: ok. 5 passed; 0 failed; 1 ignored
```

`a_keystroke_round_trips_into_cells` is the load-bearing one: a keystroke goes
down the pty, PowerShell executes it, and the result comes back as rendered
cells. The sentinel is required twice — echo plus output — so the pty talking to
itself does not count as a pass.

`pty_alone_delivers_bytes` stays `#[ignore]`d and always panics by design: it is
a diagnostic report, not an assertion. Run it with `--ignored` to reproduce the
ConPTY measurement above.

One open decision is deliberately left unmade: `fn timed_out` still holds an
`unimplemented!()`. It decides whether a pty wait that runs out of patience
fails the build or is treated as "this machine could not answer". With the tests
passing it is never reached; it matters the day one of them starves again.
