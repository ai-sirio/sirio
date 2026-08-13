# P32 — The last hard-coded status, and making the measured visual bar assertable

**You are pi, pane `w1:p4`.** Your context was just reset, so this brief is everything you need.
Your own notes are in `docs/linux-rewrite/PI-HANDOFF.md` — the theme token vocabulary and the
type/radius/density scales, with the reasoning. Read it before touching the visual layer.

## P23 is closed, and you did two things worth more than the code

All three items landed: provider badges wired to `tiller_agents::discover_availability()`, the
Control socket row with its resolved path, and Permissions gated off non-macOS. With tests.

Then:

- **You reported that you touched `main.rs`** — about five lines for the settings routing, and that
  `SettingsSnapshot` lost `Copy` and is now `Clone` because it carries a `socket_path`. That file is
  codex12's. Flagging a boundary you crossed, immediately and unprompted, is what makes it a
  five-line footnote instead of somebody's confusing merge two hours later. It has been passed on.
- **You volunteered a defect in your own work**: *"the AI Providers cards still hardcode 'Active'
  account statuses — same lie class, outside the five-string finding."* You were asked to fix five
  literals, you fixed them, and you pointed at the ones nobody had counted. That should always cost
  you nothing, and it is the first item below.

Your screenshot remainder was right too, including the reason — gpui's test path has no headless
renderer configured, so there are no honest offscreen pixels without building renderer
infrastructure. **Do not build it.** No display on this machine presents; the frame is one
`linux-shot.sh` run away when the compositor returns, and until then the honest label is the label.

## Where

```bash
source ~/.cargo/env && cd /home/enzopalmisano/Scrivania/Progetti/tiller-linux
cargo test -p tiller_ui -p tiller_theme
./Scripts/ci-linux.sh
```

codex12 is in `tiller/src/main.rs` and `tiller_control/**`; codex11 is in `tiller_terminal/**` and
`tiller/src/panes.rs`. **`tiller_ui/**` and `tiller_theme/**` are yours.**

## 1. Finish the lie you found

The AI Providers cards hard-code `"Active"` account statuses. Same class as the five `"Available"`
literals: the surface states something about this machine that it has not checked.

Find out what "Active" is supposed to mean — an authenticated account? a token that has not
expired? — and either derive it from whatever actually knows (the usage crate already parses
`~/.codex/auth.json`, so there is precedent for reading real credential state), or say plainly in
the UI that the status is unknown. **An honest "unknown" is a better product than a confident
wrong answer**, and it is far better than a word chosen because it looked reasonable in a mock.

Where the truth lives outside `tiller_ui/**`, say what you need and it will be routed.

## 2. Make the measured visual bar assertable without a display

This is the more interesting half, and it exists because of a specific problem.

The goal says the UI is judged by putting it next to waku's frames. That judgement is blocked —
possibly for a long time. But the visual bar was **measured**, not remembered: waku's palette
(`#E2795B` dark, `#C85F44` light), 11.5px UI and 13.5px body text, radii 4/6/7/8/12/13, 48px bars,
a 720px content column, all frozen in `docs/linux-rewrite/03-visual-bar-and-gpui-patterns.md`.

So a large part of "does it match waku" is a question about **numbers**, and numbers can be asserted
without a single pixel. Write tests that check the components actually use the frozen values:
that the type scale is the measured one, that radii come from the token set rather than being spelt
inline, that bar heights and the content column match, that the accent resolves to the measured
coral in each appearance.

Two things to be careful about, and please state your judgement on both:

- **This proves conformance to the bar, not that the result looks right.** A layout can use every
  correct number and still be ugly or wrong. Say so in the test module's own comment, so nobody
  later reads a green suite as "the UI matches waku".
- **Where the code deliberately departs from waku, do not bend the test to match the code, and do
  not bend the code to match a number that was right for a different program.** waku is a smaller
  application — two columns, one conversation, no diff surface, no terminal panes. Where Tiller
  needs its own value, record the departure and the reason next to the test. A frozen reference is
  a starting point, not an authority over a decision someone made for a good reason.

If you find the code and the frozen bar disagree in ways nobody decided — a radius spelt inline, a
font size that drifted — that is a finding, and fixing it is part of this piece.

## Rules that apply to every piece of this project

- **Never copy code from the reference checkouts** (`../_tiller-refs/{waku,zed,orca,t3code}`).
  waku is the visual bar and the source of ideas; every line of Tiller is written from scratch.
  The critic checks, and transplanted code counts as a gap, always.
- **A capability nobody exercised does not exist.** Anything needing pixels is
  `NOT EXERCISED — blocked on display`; never approximated, never ticked from reading the code.
- **Measure, do not describe** — when a display exists again.

## Reporting

Reply in **12 lines or fewer**: what "Active" now reports and how it knows, how many conformance
tests you wrote and what they cover, every place the code and the frozen bar disagreed with your
judgement on each, and the honest remainder.
