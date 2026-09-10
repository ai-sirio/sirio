#!/usr/bin/env python3
"""Deterministic PTY child for the sirio_terminal real-PTY tests.

These tests drive a real PTY with a real child process. On macOS and Linux
that child is (and stays) `/bin/sh` running the exact script the test wants:
macOS is the reference release platform, and the `real_pty_*` tests earn
their name by exercising a genuine POSIX shell behind a genuine PTY. Windows
has no `/bin/sh`, no `sleep`, no `cat` and no `printf`, so the same tests run
this program under `python3` instead. It is not a shell: every mode below
does one narrowly defined thing, chosen so the observable bytes on the PTY
match what the unix script produces.

The unix scripts only ever ask for two things, and so does this file:

  * stay alive until Sirio tears the pane down (`exec sleep 60`, `exec cat`),
    so the pane has a live child to draw, resize, drop files onto and kill;
  * emit exact bytes (`printf '\\033]0;title\\007'`, a nonce, `$$`, an argv
    probe, `$SIRIO_PANE_ID`), so an assertion can look for them in the grid.

Modes (argv[1]); every mode ends by staying alive or by exiting 0.

  sleep <seconds>
      Stay alive for <seconds>, then exit 0. `inf` sleeps until killed, and
      is the stand-in for `exec sleep 60`.

  print <text> [--sleep S] [--delay D] [--expand]
      Wait D seconds (default 0), write <text> to the PTY, then stay alive
      for S seconds (default `inf`). <text> is unescaped first (see
      `unescape` below: \\n \\r \\t \\a \\e \\0NN \\\\), so a caller can send
      an OSC title or any other control sequence verbatim. With --expand,
      `${NAME}` in <text> is replaced by that environment variable's value
      ("" when unset) — the portable form of `printf '%s' "$SIRIO_PANE_ID"`,
      and it must be the child that expands it, since the point of the test
      is that the child inherited the variable.

  lines <prefix> <count>
      Write <count> lines `<prefix>%03d`, numbered from zero, then exit 0.
      Fills the scrollback past the viewport the way the unix `while` loop
      does.

  cat [--prologue TEXT]
      Optionally write TEXT (unescaped) once, then echo every byte that
      arrives on the PTY straight back, forever. This is `exec cat` plus the
      terminal driver's own echo: on unix a `cat` on a canonical tty shows
      typed bytes because the line discipline echoes them, and a ConPTY only
      echoes during a cooked read, so here the echo is explicit. Windows
      input is switched to raw VT mode first, so a single keystroke — or a
      paste with no trailing newline — comes back immediately instead of
      waiting for a line to be completed.

  prompt
      Write a `$ ` prompt, then stay alive forever, echoing whatever is
      typed at it and answering each completed line with a fresh prompt.
      The stand-in for the interactive `/bin/sh -i` the drawn tests spawn:
      they need a child that settles quietly on a prompt, and that produces
      output again the moment something is typed. No command is ever run —
      none of those tests reads the result of one; they only need output to
      exist so a retained grid or mutation stamp has to move.

  echo <word>...
      Write the words joined by spaces, then a newline, and exit 0. The
      stand-in for `/bin/echo ARGV_PROBE`: a program that is not a shell,
      printing its own argv.

  pid-file <path> [--sleep S]
      Write this process's pid to <path> (no trailing newline), then stay
      alive like `sleep`. The stand-in for `printf '%s' "$$" > file`.

  size
      Report the PTY's current size as `<rows> <columns>` every time input
      arrives, then keep waiting. The stand-in for typing `stty size` at an
      interactive shell.
"""

import os
import sys
import time

WINDOWS = os.name == "nt"

FOREVER = 10 * 60.0


def unescape(text):
    """Decode the backslash escapes a `printf` format would decode.

    `codecs.decode(..., "unicode_escape")` is not usable here: it round-trips
    through latin-1 and would mangle the non-ASCII characters these tests
    deliberately send (the `✳` in an agent's OSC title).
    """
    simple = {
        "n": "\n",
        "r": "\r",
        "t": "\t",
        "a": "\a",
        "b": "\b",
        "f": "\f",
        "v": "\v",
        "e": "\033",
        "\\": "\\",
        "'": "'",
        '"': '"',
    }
    out = []
    index = 0
    while index < len(text):
        char = text[index]
        if char != "\\" or index + 1 >= len(text):
            out.append(char)
            index += 1
            continue
        following = text[index + 1]
        if following in simple:
            out.append(simple[following])
            index += 2
            continue
        if following in "01234567":
            digits = ""
            index += 1
            while index < len(text) and len(digits) < 3 and text[index] in "01234567":
                digits += text[index]
                index += 1
            out.append(chr(int(digits, 8)))
            continue
        out.append(char)
        index += 1
    return "".join(out)


def expand(text):
    """Replace `${NAME}` with that environment variable's value."""
    out = []
    index = 0
    while index < len(text):
        if text.startswith("${", index):
            end = text.find("}", index)
            if end != -1:
                out.append(os.environ.get(text[index + 2 : end], ""))
                index = end + 1
                continue
        out.append(text[index])
        index += 1
    return "".join(out)


def enable_windows_vt():
    """Make this console behave like the PTY the unix side already is.

    Two console modes matter. On stdout, ENABLE_VIRTUAL_TERMINAL_PROCESSING
    is what makes conhost parse the escape sequences written below instead
    of drawing them as literal text — an OSC title has to reach Sirio as a
    title, not as the eight printable characters that spell one. On stdin,
    ENABLE_VIRTUAL_TERMINAL_INPUT with line input and echo cleared is raw
    mode: reads return as soon as a byte arrives, and nothing is echoed
    behind this program's back, so `cat` mode's echo is the only one.
    """
    if not WINDOWS:
        return
    import ctypes

    kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
    std_input, std_output = -10, -11
    enable_processed_input = 0x0001
    enable_line_input = 0x0002
    enable_echo_input = 0x0004
    enable_virtual_terminal_input = 0x0200
    enable_virtual_terminal_processing = 0x0004

    handle = kernel32.GetStdHandle(std_output)
    mode = ctypes.c_uint32()
    if kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
        kernel32.SetConsoleMode(
            handle, mode.value | enable_virtual_terminal_processing
        )

    handle = kernel32.GetStdHandle(std_input)
    mode = ctypes.c_uint32()
    if kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
        cleared = mode.value & ~(
            enable_processed_input | enable_line_input | enable_echo_input
        )
        kernel32.SetConsoleMode(handle, cleared | enable_virtual_terminal_input)


def write(text):
    sys.stdout.buffer.write(text.encode("utf-8"))
    sys.stdout.buffer.flush()


def read_some():
    """Block until at least one character arrives on the PTY; return it.

    Returns "" once the input side is gone, which ends the caller's loop.
    """
    if WINDOWS:
        import msvcrt

        # Raw console reads: `getwch` takes one wide character straight from
        # the console input buffer without waiting for a line and without
        # echoing it, which is the behaviour a unix raw tty read has.
        try:
            return msvcrt.getwch()
        except OSError:
            return ""
    data = os.read(0, 4096)
    return data.decode("utf-8", "replace")


def stay_alive(seconds):
    if seconds == float("inf"):
        seconds = FOREVER
    deadline = time.monotonic() + seconds
    while True:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return
        time.sleep(min(remaining, 0.05))


def parse_seconds(value):
    return float("inf") if value == "inf" else float(value)


def option(args, name, default=None):
    """Read `--name value` out of `args`, leaving positional arguments alone."""
    if name not in args:
        return default
    index = args.index(name)
    return args[index + 1]


def main():
    argv = sys.argv[1:]
    if not argv:
        sys.stderr.write(__doc__)
        return 2
    mode, args = argv[0], argv[1:]
    positional = []
    index = 0
    while index < len(args):
        if args[index].startswith("--"):
            index += 2 if args[index] != "--expand" else 1
            continue
        positional.append(args[index])
        index += 1

    enable_windows_vt()

    if mode == "sleep":
        stay_alive(parse_seconds(positional[0]))
        return 0

    if mode in ("print", "env"):
        text = unescape(positional[0])
        if "--expand" in args:
            text = expand(text)
        delay = float(option(args, "--delay", "0"))
        if delay:
            time.sleep(delay)
        write(text)
        stay_alive(parse_seconds(option(args, "--sleep", "inf")))
        return 0

    if mode == "lines":
        prefix, count = positional[0], int(positional[1])
        write("".join(f"{prefix}{number:03d}\n" for number in range(count)))
        return 0

    if mode == "cat":
        prologue = option(args, "--prologue")
        if prologue:
            write(unescape(prologue))
        while True:
            chunk = read_some()
            if chunk == "":
                return 0
            write(chunk)

    if mode == "prompt":
        write("$ ")
        at_line_start = False
        while True:
            chunk = read_some()
            if chunk == "":
                return 0
            echoed = []
            for char in chunk:
                if char in "\r\n":
                    # One prompt per line, so a CRLF pair does not draw two.
                    if not at_line_start:
                        echoed.append("\r\n$ ")
                        at_line_start = True
                else:
                    echoed.append(char)
                    at_line_start = False
            write("".join(echoed))

    if mode == "echo":
        write(" ".join(positional) + "\n")
        return 0

    if mode == "pid-file":
        with open(positional[0], "w", encoding="utf-8") as handle:
            handle.write(str(os.getpid()))
        stay_alive(parse_seconds(option(args, "--sleep", "inf")))
        return 0

    if mode == "size":
        while True:
            chunk = read_some()
            if chunk == "":
                return 0
            size = os.get_terminal_size(sys.stdout.fileno())
            write(f"{size.lines} {size.columns}\n")

    sys.stderr.write(f"unknown mode: {mode}\n")
    return 2


if __name__ == "__main__":
    sys.exit(main())
