#!/usr/bin/env python3
"""A stand-in for an agent CLI typed by hand into a Sirio terminal.

Used by Scripts/Tests/test-hand-agents-e2e.sh (python3 standard library only). The
run links it as `claude`, `codex`, `omp`, `pi` and `opencode`, and copies it to
`.../node_modules/@anthropic-ai/claude-code/cli.js` for `node cli.js` (node is
python3 there). Which agent it plays comes from the basename of argv[0], or from
a `claude-code`, `codex`, `opencode-ai` or `pi-coding-agent` segment of its path.

  agent --no-title    sets no window title (for a case that must not be told by one);
                      so does a file `<argv0>.no-title` beside the executable (a launch
                      that cannot pass the flag, such as `surface terminal open --agent`)
  agent --version     prints `<name> 9.9.9` and exits (a version probe, as
                      Scripts/Tests/stubs/session-panel/claude answers one)
  agent               with a terminal on stdin: starts as its agent does (below),
                      then reads typed lines in raw mode and answers each with
                      `ack <line>`. Without a terminal it exits at once.

Typed lines (the whole line, then Enter):
  work | idle | done | block | error | clear
                      an OSC 7501 report: state=working|idle|done|blocked|error,
                      or `clear` (the profile's reports only: pi and opencode
                      send none)
  hook <status>       `sirioctl notify --session "$SIRIO_PANE_ID" --status <status>`,
                      as an agent's hook runs it
  exit                leave, with code 0

Startup, per profile:
  claude    title `✳ Claude Code`; sends the OSC 7501 probe and reports (with
            app=claude-code) only once the terminal has answered it
  codex     reports unprompted with app=codex: idle, then working and idle 10 ms apart
  omp       title `π > <cwd>`, then reports idle with app=omp
  pi        sends the probe and never reports
  opencode  no OSC 7501; title `OpenCode`, and `OC | <title>` after its first `idle`

The terminal is raw before anything is written, so nothing typed is echoed, and
every line carries an explicit CR (raw mode turns output post-processing off).
"""
import atexit
import os
import select
import subprocess
import sys
import termios
import time
import tty

PROBE = b"\x1b]7501;?\x1b\\"
ANSWER_PREFIX = b"\x1b]7501;?"
STATES = {
    "work": "working",
    "idle": "idle",
    "done": "done",
    "block": "blocked",
    "error": "error",
    "clear": "clear",
}
APPS = {"claude": "claude-code", "codex": "codex", "omp": "omp"}


def profile():
    base = os.path.basename(sys.argv[0])
    for name in ("claude", "codex", "opencode", "pi", "omp"):
        if base == name:
            return name
    path = sys.argv[0].replace("\\", "/")
    for segment, name in (
        ("claude-code", "claude"),
        ("codex", "codex"),
        ("opencode-ai", "opencode"),
        ("pi-coding-agent", "pi"),
    ):
        if segment in path.split("/"):
            return name
    return "claude"


def say(line):
    os.write(1, line.encode() + b"\r\n")


def title(text):
    if "--no-title" in sys.argv[1:] or os.path.exists(sys.argv[0] + ".no-title"):
        return
    os.write(1, b"\x1b]2;" + text.encode() + b"\x07")


def report(name, state):
    app = APPS.get(name)
    body = "state=" + state + (":app=" + app if app else "")
    os.write(1, b"\x1b]7501;" + body.encode() + b"\x1b\\")


class Input:
    """Typed bytes with the terminal's answers to the probe taken out."""

    def __init__(self):
        self.pending = b""
        self.answered = False

    def feed(self, chunk):
        self.pending += chunk
        while True:
            at = self.pending.find(ANSWER_PREFIX)
            if at == -1:
                break
            ends = [
                self.pending.find(end, at)
                for end in (b"\x1b\\", b"\x07")
                if self.pending.find(end, at) != -1
            ]
            if not ends:
                break
            end = min(ends)
            width = 2 if self.pending[end : end + 2] == b"\x1b\\" else 1
            self.pending = self.pending[:at] + self.pending[end + width :]
            self.answered = True

    def line(self):
        for mark in (b"\r", b"\n"):
            at = self.pending.find(mark)
            if at != -1:
                text = self.pending[:at]
                self.pending = self.pending[at + 1 :]
                return text.decode("utf-8", "replace").strip()
        return None


def main():
    name = profile()
    if "--version" in sys.argv[1:] or "-V" in sys.argv[1:]:
        print(name + " 9.9.9")
        return
    if not os.isatty(0):
        return
    saved = termios.tcgetattr(0)
    atexit.register(lambda: termios.tcsetattr(0, termios.TCSANOW, saved))
    # TCSANOW: the default (TCSAFLUSH) would discard input already queued.
    tty.setraw(0, termios.TCSANOW)
    source = Input()
    oc_title = False

    if name == "claude":
        title("✳ Claude Code")
        os.write(1, PROBE)
    elif name == "pi":
        os.write(1, PROBE)
    elif name == "codex":
        report(name, "idle")
        report(name, "working")
        time.sleep(0.01)
        report(name, "idle")
    elif name == "omp":
        title("π > " + os.getcwd())
        report(name, "idle")
    elif name == "opencode":
        title("OpenCode")
    say(name + " ready")

    while True:
        ready, _, _ = select.select([0], [], [], 0.2)
        if ready:
            chunk = os.read(0, 4096)
            if not chunk:
                return
            source.feed(chunk)
        while True:
            line = source.line()
            if line is None:
                break
            if line == "":
                continue
            words = line.split()
            say("ack " + line)
            if words[0] in STATES:
                state = STATES[words[0]]
                if name == "claude" and not source.answered:
                    continue
                if name in APPS:
                    report(name, state)
                elif name == "opencode" and words[0] == "idle" and not oc_title:
                    oc_title = True
                    title("OC | OpenCode")
            elif words[0] == "hook" and len(words) == 2:
                subprocess.run(
                    [
                        "sirioctl",
                        "notify",
                        "--session",
                        os.environ.get("SIRIO_PANE_ID", ""),
                        "--status",
                        words[1],
                    ],
                    check=False,
                )
            elif words[0] == "exit":
                return


if __name__ == "__main__":
    main()
