# P22 — It was never a crash. Catch the freeze and find where the render thread is stuck.

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P20 landed, and the crash hypothesis you were given was wrong

P20 is done: split / focus / close / tab-cycle / `JumpToTab1…9` actions, Linux chords (Ctrl+Alt
arrows, Ctrl+Alt+Shift for splits, Ctrl+Alt+W close, Ctrl-Tab and Ctrl-Shift-Tab, Ctrl-1…9), pure
transitions with typed split-disabled reasons, 40 tests including 12 pane tests, `main.rs` untouched
and a precise handoff written for the agent who owns it. Held the boundary exactly as asked.

Now the important part. **The defect you spent two pieces hunting is not a process death.**

The critic, running the app live and watching the window, measured this:

> Window pixmap frozen — **0 pixels changed over 2 seconds**. **The process is alive. The control
> socket still answers.** Recurring every 4–13 minutes.

Your six supervised trials found no death because **nothing was dying**. Your supervisor decomposes
wait status correctly and proved it does — it was measuring the right quantity for the wrong
hypothesis. That is not a wasted piece: `Scripts/crash-supervise.py` and its trial classification
stay, and the reframing is exactly what the evidence was for.

The lesson is worth stating because it is subtler than the one already in the log: it is not enough
that the instrument works. **It has to measure the quantity in question.** A wait-status supervisor
is the perfect tool for a process death and a blind one for a render loop that stops inside a
healthy process.

## The new question

**Where is the app stuck when the pixels stop?**

Everything below is available on this machine and checked before this brief was written.

### The oracle that defines the freeze

Two signals disagreeing is the definition:

- **Pixels**: hash the window's pixmap every ~500 ms (`import -window <id>`, then a checksum). Two
  or more seconds of identical hashes with no input is a freeze candidate.
- **Liveness**: `tillerctl` answering `system.ping`, and `workspace.current` returning real state.

Frozen pixels **plus** a responsive socket is the signature. Frozen pixels plus a dead socket is a
different defect and must be reported as such.

### The measurement that will crack it

`eu-stack` is installed (`/usr/bin/eu-stack`); `gdb` is not. **`/proc/sys/kernel/yama/ptrace_scope`
is `1`, so only a parent process may inspect the app.** Your supervisor already spawns it, which
makes it the parent — so run `eu-stack -p <pid>` **from the supervisor**, not from a separate shell,
or it will fail with a permissions error that looks like a tooling problem and is not.

At the moment of the freeze, capture and keep:

- `eu-stack -p <pid>` — every thread's user-space stack. **This is the answer if you can get it.**
  Take one immediately, then another 5 seconds later: a thread stuck at the *same* frame in both is
  blocked; one that moves is spinning.
- `/proc/<pid>/task/*/comm` and `/proc/<pid>/task/*/wchan` — cheap, always readable, and enough to
  tell a futex wait from an ioctl wait.
- `/proc/<pid>/task/*/status` — thread state (`R`/`S`/`D`), plus `voluntary_ctxt_switches` sampled
  twice, which distinguishes a thread doing nothing from one spinning hard.
- `/proc/<pid>/status` and `/proc/<pid>/stat` for overall CPU: a freeze at 0% CPU points at a
  blocked wait; a freeze at 100% points at a spin or a lock convoy.

Three shapes to distinguish, because they lead to completely different fixes: **blocked on the GPU**
(a thread in a DRM/Vulkan ioctl or a fence wait), **blocked on a lock** (futex, and then which two
threads), or **the event loop simply stopped being driven** (the main thread parked in `poll`/
`epoll_wait` with nothing waking it).

### Does it recover?

This is cheap and highly diagnostic. When frozen, try — in this order, recording what each does:

1. Damage the window (`xdotool windowsize`, `windowmove`, or unmap/map). **These are ordinary X
   requests, not XTEST**, so they do not disturb another agent's pointer and are safe on the shared
   display.
2. Ask the app to do something through the socket that must repaint — create a panel, select a
   worktree — and see whether the pixels move.
3. Wait: does it come back by itself, and after how long?

A freeze that recovers on damage points at the swapchain or a missed frame callback. One that
recovers on socket-driven work points at the frame loop only running on demand. One that recovers
from nothing at all points at a hang. Each answer eliminates the other two.

## Display etiquette — this matters today

**`:2` is the only display on this machine that still presents.** Everything created after ~09:55
comes back blank, including under software Vulkan and `MESA_VK_WSI_DEBUG=sw` — all tested, do not
spend budget there. The likely cause is a GPU device reset (`/dev/dri` has `card1` and no `card0`)
leaving the compositor on a stale render node; the fix is a compositor restart, which is the
operator's call.

So `:2` is shared with the critic, which is running the live inventory pass that defines "done", and
with a builder taking screenshots. Therefore:

- **Do not inject pointer or keyboard events** (no `xdotool key`, `type`, `click`, `mousemove`).
  XTEST is global to the display and would corrupt the critic's run and yours. Window resize and
  move are fine, as above.
- Do not raise or focus your window.
- Prefer `import -window <id>` over root crops — it reads the window's own pixmap and is immune to
  occlusion.
- Close your app instance when you finish.

You do not need input injection for this piece: the freeze happens on its own, including with nobody
touching the app.

## What "done" looks like

Best: **the stack of the stuck thread**, with the shape named — GPU wait, lock, or unfed event loop
— and the recovery behaviour. Good: a freeze detector that reliably catches the event and preserves
that evidence, so the next occurrence reports itself even if this run does not catch one.

Report **valid observation minutes** — time with the app actually rendering on `:2` and being
watched — separately from wall-clock. And remember: **failing to observe it is not evidence of
absence**; it was written off once already on a single failed reproduction, and five more instances
followed.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, and your `Scripts/crash-*` files.
  `tiller/src/main.rs` and `tiller_control/**` are codex12's — **it is actively editing main.rs
  right now**; `tiller_ui/**` and `tiller_theme/**` are pi's. If the fix belongs in a file you do
  not own, **report it with the evidence, do not patch it**.
- Diagnosis may read anything.

## Reporting

Reply in **12 lines or fewer**: valid observation minutes, whether you caught a freeze, the thread
state and stacks if you did, which of the three shapes it is, what recovered it, and the honest
remainder.
