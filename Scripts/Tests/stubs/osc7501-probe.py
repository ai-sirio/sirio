#!/usr/bin/env python3
"""A program that asks the terminal the OSC 7501 probe and reports what came back.

Used by Scripts/Tests/test-host-e2e.sh (python3 standard library only).

  osc7501-probe.py              write the probe, read for 2 s
  osc7501-probe.py --no-probe   write nothing, read for 2 s
  osc7501-probe.py --then-da1   write the probe and, in the same write, a DA1
                                query (ESC [ c): a program uses DA1 as a sentinel
                                after a query, to decide the query went unanswered
  osc7501-probe.py --repeat     write the probe, 0.1 s later again, 1.1 s later a
                                third time, and read for 2 s after the last: the
                                terminal answers a session's probe at most once a
                                second, so the second is dropped and the third is
                                answered (`ANSWERS 2`; no limiter reads 3, an
                                answer that is never repeated reads 1)

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
    received = b""
    started = time.monotonic()
    # (seconds after the start, bytes to write); the read runs 2 s past the last.
    if "--no-probe" in args:
        writes = []
    elif "--repeat" in args:
        writes = [(0.0, PROBE), (0.1, PROBE), (1.2, PROBE)]
    else:
        writes = [(0.0, PROBE + (DA1 if "--then-da1" in args else b""))]
    deadline = started + (writes[-1][0] if writes else 0.0) + 2.0
    while True:
        now = time.monotonic()
        while writes and started + writes[0][0] <= now:
            os.write(1, writes.pop(0)[1])
        left = deadline - now
        if left <= 0:
            break
        if writes:
            left = min(left, started + writes[0][0] - now)
        ready, _, _ = select.select([fd], [], [], max(left, 0.0))
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
