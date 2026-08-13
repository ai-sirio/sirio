# P30 — Make the terminal's state observable, so the display-blocked entries stop being blocked

**You are codex11, pane `w1:p2`.** Your context was just reset, so this brief is everything you
need.

## P29 landed, and the gap you recorded is the most useful part of it

`Scripts/visual-sweep.sh` covers chat, terminal, split layout, files tree and a fixture git status,
with PID-matched capture, blank-frame rejection, cleanup, and a waku side-by-side. The headless
sweep passes and the capture stage fails clearly with exit 1 when there is no display — which is
exactly right: a directory of black rectangles that someone later mistakes for evidence is worse
than no directory.

And you recorded that **Changes and Settings cannot be reached over the socket.** That is a real gap
in the automation surface, it has been routed to the agent who owns the socket, and writing it down
instead of clicking your way around it is what made it routable.

Also: the `CI fingerprint failed due to concurrent external Rust edits` was three agents editing the
tree while you ran. Not your defect.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
./Scripts/ci-linux.sh
cd rust && export TILLER_SOCKET=/tmp/codex11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

**No display presents.** `:2` answers again — a client of mine had wedged it and killing that client
freed the server — but every frame it produces is one colour. `Scripts/linux-shot.sh` and
`linux-drive.sh` now wrap every X call in `timeout 10`, because a wedged server accepts a connection
and then never answers: that cost the critic 1h26m sitting inside a single `import -window` call.
**A hang is the worst failure a tool can have** — unlike an error it produces no evidence and no
deadline. If you add an X call anywhere, give it a deadline.

codex12 is in `tiller/src/main.rs` and `tiller_control/**`; pi is in `tiller_ui/**` and
`tiller_theme/**`.

## The piece

In P24 you exercised the `## Terminals and agents` section and found **11 entries: PASSED 3,
NOT EXERCISED — display blocked 8.**

Eight of eleven blocked is the largest single block of unreachable inventory in the project, and it
does not have to stay that way. The trick that worked for the pane command layer applies here:
**a keyboard layer nobody could type into was proven anyway, because the action and a socket method
called the same function.** The state was made observable, so the behaviour became testable without
pixels.

Do the same for the terminal. Go through your 8 blocked entries and separate them into:

- **Entries whose substance is state** — the exit status of a finished command, the scrollback
  contents, the working directory, the running-agent identity, whether an indicator condition
  holds. These are facts the program computes, and a test can check the fact. Expose them from
  `tiller_terminal` as a proper API, with tests.
- **Entries whose substance is genuinely pixels** — where the glyph sits, what colour it is, that a
  menu visibly opens. These stay `NOT EXERCISED — blocked on display`, and they should, because
  proving the state does **not** prove the rendering.

Be strict about that line and say which entries fall on each side. It is the difference between
honestly widening what can be verified and quietly redefining "works" to mean "the data exists".
An entry that says a user can *see* the exit status is only half-proven by a test that the exit
status is known — record it as half-proven, in those words.

Then write down, precisely, the socket methods that would let the critic query those facts from
outside — names, parameters, what each returns. `tiller_control/**` is codex12's, so this is a
handoff, not an edit. Write it the way codex11 wrote the `on_action` spec that codex12 implemented
without needing to ask a follow-up question: **state the behaviour and how someone would check it
from outside**, not just the signature.

## Secondary, if the budget allows

`ci-linux.sh` reports **61 pre-existing clippy warnings**. Clear the ones in the crates you own
(`tiller_terminal/**`, `tiller/src/panes.rs`) and report the remaining count per crate so the others
can be routed. Do not touch other agents' crates to do it — a tidy diff that lands in the middle of
someone's refactor is not tidy.

## Rules and ownership

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  Every line of Tiller is written from scratch; transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.** Count what the inventory actually contains
  rather than trusting a number in a brief — you were right about 11 versus 14, and that habit is
  worth more than the correction.
- You own `tiller_terminal/**`, `tiller/src/panes.rs`, and the `Scripts/` you created.

## Reporting

Reply in **12 lines or fewer**: which of the 8 blocked entries are state and which are genuinely
pixels, what you exposed and its test count, the socket spec for codex12, the clippy counts, and the
honest remainder.
