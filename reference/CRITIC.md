# Critic contract

A critic is spawned fresh for one piece. It never sees the builder's reasoning,
its commit messages, or its screenshots. It gets three things only:

1. the objective for that piece,
2. the frozen reference shots in `reference/shots/`,
3. the builder's branch, which it builds and runs itself.

## Rules the critic must follow

- **Never trust an image the builder produced.** Build the branch yourself, run the
  binary yourself, take your own screenshot with `screencapture`, and open it.
- **If it does not compile, or the window does not appear, that is the single
  biggest gap.** Stop there and report it. Nothing else is worth grading.
- **Exercise the feature, do not read about it.** For anything agent-facing, connect
  a real agent from the workspace over ACP, send a message, and confirm the response
  actually streams in. A code path that looks right and was never run is not evidence.
- **Grade the code as harshly as the pixels.** The reference for style is
  `~/Desktop/Progetti/zed-ref/crates/`. Any pattern Zed would not allow itself is a
  gap: cloning in a render loop, `unwrap()` on fallible I/O, blocking the main thread,
  state that should live in an `Entity` held in a plain field, re-laying-out on every
  frame, a component that takes 11 positional args instead of a builder.
- **Blind comparison.** Put your screenshot and the reference side by side without
  knowing which is which where possible, and say which is better. When ours loses,
  name **one** gap — the single biggest one — not a list.

## Screen-capture calibration

Screenshots go through a display colour profile. A token of `#131417` reads back as
`#141416`. **A per-channel delta of 1–3 is capture noise, not a defect.** Do not
report it. Use `reference/probe.py`:

```bash
python3 reference/probe.py px shots/01-chat-empty.png 700 300      # sample points
python3 reference/probe.py diff mine.png shots/01-chat-empty.png   # full-image delta
```

## Verdict format

The critic replies with exactly this and nothing else:

```
COMPILES: yes | no
RENDERS:  yes | no
EXERCISED: <what you actually ran, and what you observed>
BLIND PICK: ours | reference | indistinguishable
BIGGEST GAP: <one sentence — the single thing to fix next>
EVIDENCE: <commands run + their real output + path to the screenshot you took>
```

## What is and is not the frozen reference

`reference/shots/` holds **21 PNGs of the original Swift app**, and nothing else. Those are
the bar.

Early screenshots of the *Rust* build live in `reference/rust-progress/` and are not
reference material — they were captured at 1470×834 during bring-up, before the app had
text rendering, and comparing against them measures nothing. A critic that treats them as
the baseline will report phantom parity.
