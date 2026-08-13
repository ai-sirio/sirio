# Critic pass 11 — the 64 half-proven rows are now the vaguest thing in the project

**You are pireview, pane `w1:p6`. You are the critic.** Your context was just reset, deliberately.
You built none of this.

## Pass 10 was the largest single movement of the project

102 rows converted — 99 `NOT EXERCISED` plus three of your own pass-9 `UNREACHABLE` verdicts
**overturned**, which is the harder half. `NOT EXERCISED` fell from 127 to 28.

And you found the thing nobody was looking at:

> Layers B, C and D of agent activity detection are complete and fully tested — yet the running app
> wires only Layer A. `handle_title_change`, `detect_content_status` and `refresh_process_signal`
> have zero callers.

**Verified independently and it is worse than you stated.** Those three have no non-test callers
anywhere, and `tiller_terminal` has no title plumbing at all — `alacritty_terminal` raises
`Event::Title`, nothing forwards it, and `lib.rs:171` carries a comment saying title changes would be
distinguished *later*. So Layer B has no source, not merely no consumer. That is the sixth instance of
this codebase's characteristic failure — **a crate does the right thing and the surface does
something simpler and wrong** — and it has been routed as its own piece.

## The piece: triage the half-proven bucket

The ledger now reads: **146 PASSED · 102 FAILED—absent · 64 half-proven · 32 NOT EXERCISED ·
7 defective · 7 display-blocked · 23 N/A.**

`NOT EXERCISED` is nearly gone, and **half-proven is now the second-largest bucket and by far the
least actionable.** Nobody can act on it. A builder cannot tell whether a half-proven row needs code
or needs a test, and that is the whole distinction that decides who owns it.

**Convert half-proven rows into one of three real states:**

1. **PASSED** — the missing half exists and you exercised it.
2. **FAILED — absent** — the missing half is not built. This is the valuable outcome: it moves the
   row onto a builder's queue instead of leaving it in a bucket nobody reads.
3. **NOT EXERCISED — blocked on display** — but only where the claim is genuinely about *pixels*.
   Read the row's own reason before you accept it: several say "pixels owed" for things that are
   really about a value being present, and `VisualTestContext` reaches those.

The rows carry their reason in the verdict column — *"split_disabled_reason unit-tested; never
rendered, no split menu"*, *"close-X click unverified"*, *"$SHELL preference/terminfo unverified"*.
**The reason tells you which half is missing**, so each row already names its own test.

Go after them in groups that share machinery, largest first, exactly as pass 10 did.

## Where builders are right now, so your verdicts do not go stale

- `pi` is in `tiller_ui/**` on the chat **composer** (`F-CHAT-09/10/11/12/14/17`). Avoid those six.
- `codex11` and `codex12` are consuming `FAILED — absent` rows in the shell, control and domain
  crates.

**Prefer half-proven rows in the quiet crates**: `tiller_activity`, `tiller_persistence`,
`tiller_project`, `tiller_usage`, `tiller_markdown`, `tiller_acp`, `tiller_agents`, `tiller_terminal`.
Where you must judge a moving file, **record the snapshot time in the row** so a later reader knows
what your verdict was true of.

## Where and how

```bash
cp -a /home/enzopalmisano/Scrivania/Progetti/tiller-linux /tmp/critic-pass11
source ~/.cargo/env && cd /tmp/critic-pass11/rust && cargo build -p tiller -p tiller_control
export TILLER_SOCKET=/tmp/critic11.sock
env -u DISPLAY -u WAYLAND_DISPLAY ./target/debug/tiller &
```

`TestAppContext` + `VisualTestContext`, `.debug_selector(id)`, `simulate_keystrokes`, real mouse
events — examples in `tiller_ui/src/changes.rs`. **Harden any drawn test you rely on** with the full
`run_until_parked()`; you were the one who found the unhardened ones pass on timing luck.

**Never call an X tool** — one wedged the pane for 1h26m and produced nothing.

## Reporting

Reply in **12 lines or fewer**: how many half-proven rows you converted and into what, the updated
totals, how many rows moved onto a builder's queue as newly-absent, and **the single biggest gap that
is not the display and not the activity-layer wiring** — both of those are now claimed.
