#!/usr/bin/env python3
"""A program that asks the terminal the OSC 7501 probe and reports what came back.

Used by Scripts/Tests/test-host-e2e.sh (python3 standard library only).

  osc7501-probe.py              write the probe, read for 2 s
  osc7501-probe.py --no-probe   write nothing, read for 2 s
  osc7501-probe.py --then-da1   write the probe and, in the same write, a DA1
                                query (ESC [ c): a program uses DA1 as a sentinel
                                after a query, to decide the query went unanswered

Prints one `ANSWER OK` per OSC 7501 answer starting with `?` that arrived, or
`ANSWER NONE` when none did, then `ANSWERS <n>`. With --then-da1 it also prints
`ORDER OK` when the 7501 answer came before the DA1 reply and `ORDER WRONG`
when it came after it or either is missing. Then `READY`, and it sleeps.

The terminal is in raw mode before anything is written, so the answer is not
echoed, and the output carries explicit CRs because raw mode turns OPOST off.
"""
import os
import select
import sys
import termios
import time
import tty

PROBE = b"\x1b]7501;?\x1b\\"
DA1 = b"\x1b[c"


def say(line):
    os.write(1, line.encode() + b"\r\n")


def main():
    args = sys.argv[1:]
    fd = 0
    # TCSANOW: the default (TCSAFLUSH) would discard input already queued.
    tty.setraw(fd, termios.TCSANOW)
    if "--no-probe" not in args:
        os.write(1, PROBE + (DA1 if "--then-da1" in args else b""))
    received = b""
    deadline = time.monotonic() + 2.0
    while True:
        left = deadline - time.monotonic()
        if left <= 0:
            break
        ready, _, _ = select.select([fd], [], [], left)
        if ready:
            chunk = os.read(fd, 4096)
            if not chunk:
                break
            received += chunk
    marker = b"\x1b]7501;?"
    answers = received.count(marker)
    for _ in range(answers):
        say("ANSWER OK")
    if answers == 0:
        say("ANSWER NONE")
    say("ANSWERS %d" % answers)
    if "--then-da1" in args:
        at = received.find(marker)
        # A DA1 reply is `ESC [ ? ... c`.
        da1 = received.find(b"\x1b[?")
        say("ORDER OK" if at != -1 and da1 != -1 and at < da1 else "ORDER WRONG")
    say("READY")
    while True:
        time.sleep(60)


if __name__ == "__main__":
    main()
