# P58 — Settings: make the controls mean what they show

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.

## D1 landed, and you did two hard things

The queue drains at `TurnEnded` under **one rule** for completion and cancellation alike. That is
the design decision the brief asked for and the easy version — two paths, one per outcome — is how
"queue-then-stop silently loses the message" gets built. You took the single rule.

Then you refused a row with an argument instead of a shrug: `F-CHAT-05` stayed unbuilt because a
permission-wait is a *streaming sub-state*, so disabling the composer there contradicts D1's own
"stays editable" rule. That is a design contradiction found by building, and it is worth more than
the row would have been. Same for noticing that `F-CHAT-08`'s clause also names a **connecting**
state that still does not exist, so the row cannot close even now that you built the stop state.

## Why you are not on D3 yet

`fable` is rewriting `D3-composer-one-card.md` right now to fold in three overturned rows
(`F-CHAT-16/18/23`). Handing you the current file would hand you a brief written before those
overturns. D3 is still yours, next.

## The piece: Settings is half a control panel

Eighteen `F-SET` rows are not passing. They are not eighteen problems. They are **two**, and both
are in your crate.

### 1. Settings that display but never persist

`F-SET-04` (resume_sessions), `F-SET-05` (auto_naming), `F-SET-06` (retention), `F-SET-07` (mount
cap) are all the same sentence in the ledger: *"report-only"*, *"not in persisted AppSettings
(5 keys only)"*, *"SettingsSnapshot/persistence has no such keys"*.

So the UI computes a value, clamps it, draws it — and drops it on quit. A user changes a setting,
sees it change, relaunches, and it is back. **Decide the persisted schema once and wire all four**,
rather than adding four keys in four places.

`pireview` found this by exercising and `fable` found it by reading, independently and without
coordination. When two methods agree, the finding is as solid as this project gets.

### 2. Controls that draw but do nothing

`F-SET-03` Check for Updates, `F-SET-08` Copy install command (empty callback), `F-SET-10` Refresh
now, and the summariser button in `F-SET-05` are **literal no-ops**. `fable` read one of them at
`settings.rs:1187` and it is exactly `|_, _, _| {}`.

Here is the judgement call, and it is yours: **a control whose feature does not exist on this
platform should not draw as though it works.** Check for Updates has no updater on Linux — there is
no Sparkle here. For each of the four, choose: implement it, disable it with a visible reason, or
remove it. Any of the three is defensible; drawing an enabled button that does nothing is not.
State which you chose for each and why.

Do **not** close a row by building something no caller wants — that is the dead-model defect
`pireview` found seven instances of. `F-SET-07`'s own evidence says *"eviction has zero callers"*,
which is that defect sitting inside this piece.

### Also cheap, if it is genuinely cheap

`F-SET-02`: Back is wired but there is **no Escape binding anywhere in the shell**. If it does not
fit the same machinery, leave it and say so.

## Leave these alone

`F-SET-09/11/12/13/14/15/16/17/18/21` are genuinely absent surfaces (skill provisioner, cookie UI,
multi-account, agent registry, a real search input…). They are their own pieces. `F-SET-19/20/22`
are appearance rows and belong to `pireview`, who now has a working display.

## Evidence

Follow `docs/linux-rewrite/EVIDENCE-STANDARD.md`. UI rows need **named drawn tests**, hardened with
the full `run_until_parked()` pump, the way you have built every piece since P44.

Persistence rows need a **real relaunch**: set the value, quit, relaunch, read it back. A test that
only asserts the setting reached a struct proves nothing about the defect — the defect *is* that it
never reaches disk.

For a control you disable or remove, the proof is a drawn test asserting it is not an enabled
no-op.

**The display works again** (the machine rebooted at ~19:01; `Scripts/linux-shot.sh` returns
`1470x833 · 8401 colours`). A screenshot is welcome supporting evidence now, but it does not replace
a drawn test — a shot proves it looked right once, a test proves it stays right.

## Rules

- Work in `/home/enzopalmisano/Scrivania/Progetti/tiller-linux`, branch `linux/gpui-waku`. Confirm
  with `git branch --show-current`.
- **`tiller_ui/**` and `tiller_theme/**` are yours.** `codex12` is in `tiller/src/main.rs` and
  `tiller_control/**` (P57, wiring the chat doors); `codex11` is in `tiller_persistence` (P56). If
  the persisted settings schema needs a change in `tiller_persistence`, **say so in your report and
  do not make it yourself** — `codex11` owns that crate and is live in it.
- The gate is `Scripts/ci-linux.sh`. It is currently red in the `tiller` bin for reasons that are
  not yours (a real-PTY test aborts the process inside GPUI's deterministic scheduler; `codex12`
  quarantined them). Judge yourself on `cargo test -p tiller_ui -p tiller_theme` and report the rest
  separately, exactly as you did in D1.
- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
- Mark rows `builder-claimed, unverified`, never `PASSED`. **Proceed without asking for approval.**

## Reporting

**12 lines or fewer**: the persisted schema you chose and the four settings now surviving a
relaunch, your decision for each of the four dead controls with the reason, whether Escape was cheap,
any row you deliberately left absent and why, anything you need from `codex11`, tests by name, the
gate result with not-yours failures named separately, and the honest remainder.
